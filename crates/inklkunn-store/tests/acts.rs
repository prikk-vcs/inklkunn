//! The recovery flag, the overlaps kept on a proposal and the system acts, on both stores (handoff 004 T4;
//! RFC 004 D-4, D-6, D-14): the flag is set once and cleared, what is written once is refused a second time on
//! both stores and, for SQLite, through SQL, everything survives a reopen, and a version 10 database is migrated.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
use inklkunn_store::{
    AcceptanceOutcome, AccountStore, ActStore, MemoryStore, OverlapRecord, OverlapText,
    ProposalStore, RecoveryFlag, RecoveryReason, SqliteStore, StoreError, SystemActEnd,
    SystemActId, SystemActKind, SystemActRecord, WikiId, WikiStore,
};
use support::{account, nth, open_draft, submit, temp, wiki};

fn flag(w: &WikiId, reason: RecoveryReason, at: i64) -> RecoveryFlag {
    RecoveryFlag {
        wiki: w.clone(),
        reason,
        act: Some("an-act".to_owned()),
        since: at,
    }
}

fn overlap(p: &inklkunn_store::ProposalId, revision: u32, at: i64) -> OverlapRecord {
    OverlapRecord {
        proposal: p.clone(),
        revision,
        tip_block: "a".repeat(64),
        texts: vec![
            OverlapText {
                page: "pages/Home.md".to_owned(),
                place: "block 2".to_owned(),
                base: Some("Hello.".to_owned()),
                theirs: Some("Hello, you.".to_owned()),
                ours: Some("Hello, world \u{1F30D}.".to_owned()),
            },
            OverlapText {
                page: "pages/Gone.md".to_owned(),
                place: "page".to_owned(),
                base: Some("x".to_owned()),
                theirs: None,
                ours: Some("y".to_owned()),
            },
        ],
        created_at: at,
    }
}

fn act(w: &WikiId, kind: SystemActKind, at: i64) -> SystemActRecord {
    SystemActRecord {
        id: SystemActId::new(),
        wiki: w.clone(),
        view: "heads/main".to_owned(),
        kind,
        base_block: "e".repeat(64),
        message: "Import\n\nKind: import\n".to_owned(),
        message_digest: "f".repeat(64),
        digest: "d".repeat(64),
        bytes: 12,
        created_at: at,
    }
}

fn the_suite(store: &(impl ActStore + ProposalStore + WikiStore + AccountStore)) {
    let w = wiki(store, "acts");
    let other = wiki(store, "elsewhere");
    let author = account(store);

    // --- The flag: set once, the first reason stays, cleared by recovery, per wiki.
    assert_eq!(store.recovery_flag(&w.id).unwrap(), None);
    let first = flag(&w.id, RecoveryReason::Queued, 5);
    assert!(store.set_recovery_flag(&first).unwrap());
    assert!(
        !store
            .set_recovery_flag(&flag(&w.id, RecoveryReason::WorktreeDirty, 6))
            .unwrap(),
        "a second incident does not replace the first"
    );
    assert_eq!(store.recovery_flag(&w.id).unwrap(), Some(first));
    assert_eq!(store.recovery_flag(&other.id).unwrap(), None);
    assert_eq!(
        store.set_recovery_flag(&flag(&WikiId::new(), RecoveryReason::Queued, 1)),
        Err(StoreError::NotFound)
    );
    assert!(store.clear_recovery_flag(&w.id).unwrap());
    assert!(!store.clear_recovery_flag(&w.id).unwrap());
    assert_eq!(store.recovery_flag(&w.id).unwrap(), None);
    for reason in RecoveryReason::ALL {
        assert_eq!(RecoveryReason::parse(reason.as_str()), Some(reason));
        assert!(store.set_recovery_flag(&flag(&w.id, reason, 7)).unwrap());
        assert_eq!(store.recovery_flag(&w.id).unwrap().unwrap().reason, reason);
        assert!(store.clear_recovery_flag(&w.id).unwrap());
    }

    // --- Overlaps are kept on the proposal, whole, in order, and name what exists.
    let p = submit(store, &open_draft(store, &w.id, &author, 10), 11);
    assert!(store.overlaps_of(&p.id).unwrap().is_empty());
    let (o1, o2) = (overlap(&p.id, 1, 20), overlap(&p.id, 1, 30));
    store.record_overlap(&o1).unwrap();
    store.record_overlap(&o2).unwrap();
    assert_eq!(store.overlaps_of(&p.id).unwrap(), vec![o1, o2]);
    assert_eq!(
        store.record_overlap(&overlap(&p.id, 2, 40)),
        Err(StoreError::NotFound),
        "no revision 2"
    );
    assert_eq!(
        store.record_overlap(&overlap(&inklkunn_store::ProposalId::new(), 1, 40)),
        Err(StoreError::NotFound)
    );

    // --- A system act is written before the signer is asked and ends once.
    assert!(store.system_acts(&w.id).unwrap().is_empty());
    let a1 = act(&w.id, SystemActKind::Import, 50);
    let a2 = act(&w.id, SystemActKind::System, 51);
    store.begin_system_act(&a1).unwrap();
    store.begin_system_act(&a2).unwrap();
    assert_eq!(
        store.begin_system_act(&a1),
        Err(StoreError::AlreadyRecorded)
    );
    assert_eq!(
        store.begin_system_act(&act(&WikiId::new(), SystemActKind::Import, 1)),
        Err(StoreError::NotFound)
    );
    assert_eq!(store.open_system_act(&a1.id).unwrap(), Some(a1.clone()));
    assert_eq!(store.open_system_act(&SystemActId::new()).unwrap(), None);
    let open = store.system_acts(&w.id).unwrap();
    assert_eq!(
        open.iter().map(|e| &e.act).collect::<Vec<_>>(),
        vec![&a1, &a2]
    );
    assert!(open.iter().all(|e| e.end.is_none()), "both are open");
    assert!(store.system_acts(&other.id).unwrap().is_empty());
    let sealed = SystemActEnd {
        act: a1.id.clone(),
        outcome: AcceptanceOutcome::Sealed {
            block_id: "b".repeat(64),
            patch_id: "c".repeat(64),
        },
        at: 52,
    };
    store.finish_system_act(&sealed).unwrap();
    assert_eq!(
        store.open_system_act(&a1.id).unwrap(),
        None,
        "an act that has ended is not open"
    );
    assert_eq!(
        store.finish_system_act(&SystemActEnd {
            outcome: AcceptanceOutcome::Failed {
                word: "wiki-busy".to_owned()
            },
            ..sealed.clone()
        }),
        Err(StoreError::AlreadyRecorded),
        "a sealed act is not turned into a failed one"
    );
    assert_eq!(
        store.finish_system_act(&SystemActEnd {
            act: SystemActId::new(),
            outcome: AcceptanceOutcome::Failed {
                word: "x".to_owned()
            },
            at: 1
        }),
        Err(StoreError::NotFound)
    );
    store
        .finish_system_act(&SystemActEnd {
            act: a2.id.clone(),
            outcome: AcceptanceOutcome::Failed {
                word: "wiki-busy".to_owned(),
            },
            at: 53,
        })
        .unwrap();
    let all = store.system_acts(&w.id).unwrap();
    assert_eq!(nth(&all, 0).end, Some(sealed));
    assert!(matches!(
        nth(&all, 1).end.map(|e| e.outcome),
        Some(AcceptanceOutcome::Failed { word }) if word == "wiki-busy"
    ));
}

#[test]
fn the_memory_store_holds_the_act_invariants() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_holds_the_act_invariants() {
    the_suite(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_flag_the_overlaps_and_the_acts_survive_a_reopen() {
    let tmp = temp("acts-reopen");
    let db = tmp.0.join("store.sqlite");
    let (w, p, a) = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persist");
        let author = account(&store);
        let p = submit(&store, &open_draft(&store, &w.id, &author, 1), 2);
        store
            .set_recovery_flag(&flag(&w.id, RecoveryReason::SealedUnconfirmed, 3))
            .unwrap();
        store.record_overlap(&overlap(&p.id, 1, 4)).unwrap();
        let a = act(&w.id, SystemActKind::Import, 5);
        store.begin_system_act(&a).unwrap();
        (w.id, p.id, a)
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(
        store.recovery_flag(&w).unwrap().map(|f| f.reason),
        Some(RecoveryReason::SealedUnconfirmed)
    );
    assert_eq!(store.overlaps_of(&p).unwrap(), vec![overlap(&p, 1, 4)]);
    assert_eq!(store.system_acts(&w).unwrap().len(), 1);
    assert_eq!(nth(&store.system_acts(&w).unwrap(), 0).act, a);
}

#[test]
fn what_is_written_once_is_refused_through_sql_too() {
    let tmp = temp("acts-sql");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "append-only");
    let author = account(&store);
    let p = submit(&store, &open_draft(&store, &w.id, &author, 1), 2);
    store.record_overlap(&overlap(&p.id, 1, 3)).unwrap();
    let a = act(&w.id, SystemActKind::Import, 4);
    store.begin_system_act(&a).unwrap();
    store
        .finish_system_act(&SystemActEnd {
            act: a.id.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: "b".repeat(64),
                patch_id: "c".repeat(64),
            },
            at: 5,
        })
        .unwrap();

    let conn = rusqlite::Connection::open(&db).expect("raw open");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let statements: Vec<String> = vec![
        "UPDATE proposal_overlaps SET tip_block = 'x'".into(),
        "DELETE FROM proposal_overlaps".into(),
        format!(
            "INSERT OR REPLACE INTO proposal_overlaps (seq, proposal_id, revision, tip_block, texts, created_at) VALUES (1, '{}', 1, 'x', '[]', 9)",
            p.id
        ),
        "UPDATE system_acts SET bytes = 0".into(),
        "DELETE FROM system_acts".into(),
        format!(
            "INSERT OR REPLACE INTO system_acts (id, seq, wiki_id, view, kind, base_block, message, message_digest, digest, bytes, created_at) VALUES ('{}', 99, '{}', 'v', 'import', 'b', 'm', 'x', 'x', 0, 9)",
            a.id, w.id
        ),
        "UPDATE system_act_ends SET outcome = 'sealed'".into(),
        "DELETE FROM system_act_ends".into(),
        format!(
            "INSERT OR REPLACE INTO system_act_ends (act_id, outcome, word, at) VALUES ('{}', 'failed', 'x', 9)",
            a.id
        ),
    ];
    for sql in &statements {
        let e = conn.execute(sql, []).expect_err(sql).to_string();
        assert!(e.contains("append-only"), "{sql}: {e}");
    }
    // The flag is the one thing that changes, and it can.
    store
        .set_recovery_flag(&flag(&w.id, RecoveryReason::Queued, 6))
        .unwrap();
    conn.execute("UPDATE wiki_flags SET since = 7", [])
        .expect("the flag is mutable");
    // The checks that say what a row may be.
    for sql in [
        format!(
            "INSERT INTO wiki_flags (wiki_id, reason, since) VALUES ('{}', 'because', 1)",
            WikiId::new()
        ),
        format!(
            "INSERT INTO system_acts (id, seq, wiki_id, view, kind, base_block, message, message_digest, digest, bytes, created_at) VALUES ('{}', 90, '{}', 'v', 'proposal', 'b', 'm', 'x', 'x', 0, 9)",
            SystemActId::new(),
            w.id
        ),
        format!(
            "INSERT INTO system_act_ends (act_id, outcome, at) VALUES ('{}', 'sealed', 1)",
            a.id
        ),
    ] {
        assert!(conn.execute(&sql, []).is_err(), "{sql}");
    }
}

/// A database exactly as build 10 left it: an intent written without a message, a wiki and an account.
fn make_version_10(db: &std::path::Path, wiki_id: &str, account: &str) {
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 10) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '10')",
        [],
    )
    .expect("version");
    raw.execute(
        "INSERT INTO accounts (id, display_name, created_at) VALUES (?1, 'Somebody', 1)",
        [account],
    )
    .expect("an account");
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'Old', 'heads/main', 'CC-BY-SA-4.0', \
         '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, 1700000000, 1)",
        [wiki_id],
    )
    .expect("a wiki");
}

#[test]
fn a_version_10_database_is_migrated_to_11_keeping_its_records_and_only_once() {
    let tmp = temp("v9");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000e1";
    let account_id = "019a0000-0000-7000-8000-0000000000e2";
    make_version_10(&db, wiki_id, account_id);
    // An intent as build 10 wrote it: no message column.
    {
        let raw = rusqlite::Connection::open(&db).unwrap();
        raw.execute_batch(&format!(
            "INSERT INTO proposals (id, seq, wiki_id, view, state, author, summary, sources, created_at, updated_at) \
               VALUES ('019a0000-0000-7000-8000-0000000000e3', 1, '{wiki_id}', 'heads/main', 'held-for-review', '{account_id}', 's', '[]', 1, 1);
             INSERT INTO proposal_revisions (proposal_id, seq, content, created_at) VALUES ('019a0000-0000-7000-8000-0000000000e3', 1, '{{}}', 1);
             INSERT INTO acceptances (id, seq, proposal_id, revision, wiki_id, view, acceptor, acceptor_role, policy_version, proposer, digest, bytes, created_at) \
               VALUES ('019a0000-0000-7000-8000-0000000000e4', 1, '019a0000-0000-7000-8000-0000000000e3', 1, '{wiki_id}', 'heads/main', '{account_id}', 'maintainer', 'x', '{account_id}', 'd', 1, 1);
             INSERT INTO intents (id, seq, acceptance_id, wiki_id, view, base_block, message_digest, created_at) \
               VALUES ('019a0000-0000-7000-8000-0000000000e5', 1, '019a0000-0000-7000-8000-0000000000e4', '{wiki_id}', 'heads/main', 'b', 'm', 1);"
        ))
        .expect("version 9 records");
    }
    let version = || -> String {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version")
    };
    assert_eq!(version(), "10");
    {
        let store = SqliteStore::open(&db).expect("a version 10 database opens");
        assert_eq!(version(), SCHEMA_VERSION.to_string());
        assert_eq!(SCHEMA_VERSION, 11);
        let w = store.list().unwrap().remove(0);
        let intents = inklkunn_store::AcceptanceStore::intents(&store, &w.id).unwrap();
        assert_eq!(intents.len(), 1, "the old intent is still there");
        assert_eq!(nth(&intents, 0).intent.message, "", "and says no message");
        // The new tables work on the old wiki.
        assert!(
            store
                .set_recovery_flag(&flag(&w.id, RecoveryReason::Interrupted, 3))
                .unwrap()
        );
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    let w = store.list().unwrap().remove(0);
    assert!(store.recovery_flag(&w.id).unwrap().is_some());
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
