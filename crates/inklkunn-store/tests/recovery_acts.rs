//! The recovery acts (RFC 004 D-7; handoff 004 T5) and the question recovery starts from, on both stores: a plan is
//! written before the signer is asked and ends once, what is written once is refused a second time on both stores
//! and, for SQLite, through SQL, everything survives a reopen, and a version 11 database is migrated. And
//! `pending_acceptances`: an acceptance with no end is found whether or not its intent was written.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, AcceptanceStore,
    AccountId, AccountStore, ActStore, MemoryStore, ProposalStore, RecoveryActEnd, RecoveryActId,
    RecoveryActRecord, RecoveryEnding, Role, SqliteStore, StoreError, WikiId, WikiStore,
};
use support::{account, nth, open_draft, submit, temp, wiki};

fn act(w: &WikiId, plan: &str, at: i64) -> RecoveryActRecord {
    RecoveryActRecord {
        id: RecoveryActId::new(),
        wiki: w.clone(),
        plan: plan.to_owned(),
        created_at: at,
    }
}

fn end(a: &RecoveryActRecord, ending: RecoveryEnding, at: i64) -> RecoveryActEnd {
    RecoveryActEnd {
        act: a.id.clone(),
        ending,
        at,
    }
}

fn the_suite(store: &(impl ActStore + WikiStore)) {
    let w = wiki(store, "recovering");
    let other = wiki(store, "elsewhere");

    // --- A plan is kept whole, in order, per wiki, and is open until it ends.
    assert!(store.recovery_acts(&w.id).unwrap().is_empty());
    let first = act(
        &w.id,
        "{\"steps\":[\"seal\"]}\nmultibyte \u{65e5}\u{672c}\u{8a9e}",
        10,
    );
    let second = act(&w.id, "{}", 11);
    store.begin_recovery_act(&first).unwrap();
    store.begin_recovery_act(&second).unwrap();
    assert_eq!(
        store.begin_recovery_act(&first),
        Err(StoreError::AlreadyRecorded),
        "an id is used once"
    );
    assert_eq!(
        store.begin_recovery_act(&act(&WikiId::new(), "{}", 1)),
        Err(StoreError::NotFound),
        "a plan is for a wiki that exists"
    );
    assert_eq!(
        store.open_recovery_act(&first.id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        store.open_recovery_act(&RecoveryActId::new()).unwrap(),
        None
    );
    let all = store.recovery_acts(&w.id).unwrap();
    assert_eq!(
        all.iter().map(|e| &e.act).collect::<Vec<_>>(),
        vec![&first, &second]
    );
    assert!(all.iter().all(|e| e.end.is_none()), "both are open");
    assert!(store.recovery_acts(&other.id).unwrap().is_empty());

    // --- It ends once, and an act that has ended is not open (the signer's guard refuses it).
    let done = end(&first, RecoveryEnding::Recovered, 12);
    store.finish_recovery_act(&done).unwrap();
    assert_eq!(store.open_recovery_act(&first.id).unwrap(), None);
    assert_eq!(
        store.finish_recovery_act(&end(&first, RecoveryEnding::Stopped, 13)),
        Err(StoreError::AlreadyRecorded),
        "a recovered act is not turned into a stopped one"
    );
    assert_eq!(
        store.finish_recovery_act(&RecoveryActEnd {
            act: RecoveryActId::new(),
            ending: RecoveryEnding::Stopped,
            at: 1
        }),
        Err(StoreError::NotFound)
    );
    store
        .finish_recovery_act(&end(&second, RecoveryEnding::Stopped, 14))
        .unwrap();
    let all = store.recovery_acts(&w.id).unwrap();
    assert_eq!(nth(&all, 0).end, Some(done));
    assert_eq!(
        nth(&all, 1).end.as_ref().map(|e| e.ending),
        Some(RecoveryEnding::Stopped)
    );
    for ending in [RecoveryEnding::Recovered, RecoveryEnding::Stopped] {
        assert_eq!(RecoveryEnding::parse(ending.as_str()), Some(ending));
    }
    assert_eq!(RecoveryEnding::parse("fine"), None);

    // --- A plan is bounded.
    let big = "x".repeat(inklkunn_store::acts::MAX_RECOVERY_PLAN_BYTES + 1);
    assert!(matches!(
        store.begin_recovery_act(&act(&w.id, &big, 20)),
        Err(StoreError::Backend(_))
    ));
    let at_the_bound = "x".repeat(inklkunn_store::acts::MAX_RECOVERY_PLAN_BYTES);
    store
        .begin_recovery_act(&act(&w.id, &at_the_bound, 21))
        .unwrap();
}

#[test]
fn the_memory_store_holds_the_recovery_act_invariants() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_holds_the_recovery_act_invariants() {
    the_suite(&SqliteStore::open_in_memory().expect("open"));
}

fn acceptance(
    p: &inklkunn_store::ProposalRecord,
    acceptor: &AccountId,
    at: i64,
) -> AcceptanceRecord {
    AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: p.id.clone(),
        revision: 1,
        wiki: p.wiki.clone(),
        view: p.view.clone(),
        acceptor: acceptor.clone(),
        acceptor_role: Role::Maintainer,
        policy_version: "bootstrap-1".to_owned(),
        proposer: p.author.clone(),
        digest: "d".repeat(64),
        bytes: 1,
        created_at: at,
    }
}

fn pending_suite(store: &(impl ProposalStore + AcceptanceStore + WikiStore + AccountStore)) {
    let w = wiki(store, "pending");
    let other = wiki(store, "pending-elsewhere");
    let (author, maintainer) = (account(store), account(store));
    let p = submit(store, &open_draft(store, &w.id, &author, 10), 11);
    assert!(store.pending_acceptances(&w.id).unwrap().is_empty());

    // An acceptance with no intent is pending: the process died between its record and its intent.
    let a = acceptance(&p, &maintainer, 20);
    store.begin_acceptance(&a).unwrap();
    assert_eq!(store.pending_acceptances(&w.id).unwrap(), vec![a.clone()]);
    assert!(store.open_intents(&w.id).unwrap().is_empty());
    assert!(
        store.pending_acceptances(&other.id).unwrap().is_empty(),
        "per wiki"
    );
    // Once it has ended, in either way, it is not.
    store
        .finish_acceptance(&AcceptanceResult {
            acceptance: a.id.clone(),
            outcome: AcceptanceOutcome::Failed {
                word: "interrupted".to_owned(),
            },
            at: 21,
        })
        .unwrap();
    assert!(store.pending_acceptances(&w.id).unwrap().is_empty());
    let b = acceptance(&p, &maintainer, 30);
    store.begin_acceptance(&b).unwrap();
    assert_eq!(store.pending_acceptances(&w.id).unwrap(), vec![b.clone()]);
    store
        .finish_acceptance(&AcceptanceResult {
            acceptance: b.id.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: "b".repeat(64),
                patch_id: "c".repeat(64),
            },
            at: 31,
        })
        .unwrap();
    assert!(store.pending_acceptances(&w.id).unwrap().is_empty());
}

#[test]
fn pending_acceptances_are_found_without_an_intent_on_the_memory_store() {
    pending_suite(&MemoryStore::new());
}

#[test]
fn pending_acceptances_are_found_without_an_intent_on_the_sqlite_store() {
    pending_suite(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn recovery_acts_survive_a_reopen_and_are_refused_changes_through_sql_too() {
    let tmp = temp("recovery-acts");
    let db = tmp.0.join("store.sqlite");
    let (w, a) = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persist");
        let a = act(&w.id, "plan \u{1F30D}", 5);
        store.begin_recovery_act(&a).unwrap();
        store
            .finish_recovery_act(&end(&a, RecoveryEnding::Recovered, 6))
            .unwrap();
        let b = act(&w.id, "still open", 7);
        store.begin_recovery_act(&b).unwrap();
        (w.id, (a, b))
    };
    let store = SqliteStore::open(&db).expect("reopen");
    let all = store.recovery_acts(&w).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(nth(&all, 0).act, a.0);
    assert_eq!(
        nth(&all, 0).end.as_ref().map(|e| e.ending),
        Some(RecoveryEnding::Recovered)
    );
    assert_eq!(nth(&all, 1).act, a.1);
    assert!(nth(&all, 1).end.is_none());

    let conn = rusqlite::Connection::open(&db).expect("raw open");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let statements: Vec<String> = vec![
        "UPDATE recovery_acts SET plan = 'x'".into(),
        "DELETE FROM recovery_acts".into(),
        format!(
            "INSERT OR REPLACE INTO recovery_acts (id, seq, wiki_id, plan, created_at) VALUES ('{}', 99, '{}', 'x', 9)",
            a.0.id, w
        ),
        "UPDATE recovery_act_ends SET ending = 'stopped'".into(),
        "DELETE FROM recovery_act_ends".into(),
        format!(
            "INSERT OR REPLACE INTO recovery_act_ends (act_id, ending, at) VALUES ('{}', 'stopped', 9)",
            a.0.id
        ),
    ];
    for sql in &statements {
        let e = conn.execute(sql, []).expect_err(sql).to_string();
        assert!(e.contains("append-only"), "{sql}: {e}");
    }
    // The checks that say what a row may be.
    for sql in [
        format!(
            "INSERT INTO recovery_act_ends (act_id, ending, at) VALUES ('{}', 'fine', 1)",
            a.1.id
        ),
        format!(
            "INSERT INTO recovery_acts (id, seq, wiki_id, plan, created_at) VALUES ('{}', 90, '{}', '{}', 9)",
            RecoveryActId::new(),
            w,
            "x".repeat(inklkunn_store::acts::MAX_RECOVERY_PLAN_BYTES + 1)
        ),
        format!(
            "INSERT INTO recovery_acts (id, seq, wiki_id, plan, created_at) VALUES ('{}', 91, '{}', 'x', 9)",
            RecoveryActId::new(),
            WikiId::new()
        ),
    ] {
        assert!(conn.execute(&sql, []).is_err(), "{}", &sql[..60]);
    }
}

/// A database exactly as build 11 left it: a wiki, and a flag.
fn make_version_11(db: &std::path::Path, wiki_id: &str) {
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 11) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '11')",
        [],
    )
    .expect("version");
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'Old', 'heads/main', 'CC-BY-SA-4.0', \
         '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, 1700000000, 1)",
        [wiki_id],
    )
    .expect("a wiki");
    raw.execute(
        "INSERT INTO wiki_flags (wiki_id, reason, act, since) VALUES (?1, 'queued', NULL, 5)",
        [wiki_id],
    )
    .expect("a flag");
}

#[test]
fn a_version_11_database_is_migrated_to_12_keeping_its_records_and_only_once() {
    let tmp = temp("v11");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000f1";
    make_version_11(&db, wiki_id);
    let version = || -> String {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version")
    };
    assert_eq!(version(), "11");
    {
        let store = SqliteStore::open(&db).expect("a version 11 database opens");
        assert_eq!(version(), SCHEMA_VERSION.to_string());
        assert_eq!(SCHEMA_VERSION, 12);
        let w = store.list().unwrap().remove(0);
        assert!(
            store.recovery_flag(&w.id).unwrap().is_some(),
            "the old flag is still there"
        );
        // The new tables work on the old wiki.
        store
            .begin_recovery_act(&act(&w.id, "a plan", 9))
            .expect("a recovery act on the migrated wiki");
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    let w = store.list().unwrap().remove(0);
    assert_eq!(store.recovery_acts(&w.id).unwrap().len(), 1);
    // The migrated schema is the fresh schema: the same tables, indexes and triggers.
    let names = |path: &std::path::Path| -> Vec<String> {
        let conn = rusqlite::Connection::open(path).expect("raw open");
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'trigger', 'index') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'search_fts%' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    };
    let fresh = tmp.0.join("fresh.sqlite");
    drop(SqliteStore::open(&fresh).expect("fresh"));
    assert_eq!(names(&db), names(&fresh));
}
