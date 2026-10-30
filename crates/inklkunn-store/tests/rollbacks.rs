//! Rollback proposals and revision flags on both stores (handoff 004 T6; RFC 004 D-8, D-10; schema version
//! 10): a proposal is a rollback from its creation or never, the note is written once, and the flags of a
//! proposal's latest revision are asked for in one step.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::{
    AccountId, AccountStore, MemoryStore, ProposalChange, ProposalId, ProposalOp, ProposalState,
    ProposalStore, RevisionContent, RevisionFlags, RevisionRecord, SqliteStore, StoreError,
    WikiStore,
};
use support::{account, base, content, draft_record, open_draft, step, submit, temp, wiki};

const UNDONE: &str = "00000000000000000000000000000000000000000000000000000000c0ffee01";

fn open_rollback(
    store: &(impl ProposalStore + WikiStore + AccountStore),
    wiki: &inklkunn_store::WikiId,
    author: &AccountId,
    undoes: &str,
) -> Result<inklkunn_store::ProposalRecord, StoreError> {
    let record = draft_record(wiki, author, 1);
    let created = step(
        &record.id,
        ProposalOp::Create,
        None,
        ProposalState::Draft,
        author,
        None,
        1,
    );
    store
        .create_rollback_proposal(&record, &content(1, "the old text"), &created, undoes)
        .map(|()| record)
}

fn the_suite(store: &(impl ProposalStore + WikiStore + AccountStore)) {
    let w = wiki(store, "rollbacks");
    let (me, you) = (account(store), account(store));

    // An ordinary proposal undoes nothing.
    let plain = open_draft(store, &w.id, &me, 1);
    assert_eq!(store.rollback_of(&plain.id, Some(&me)).unwrap(), None);

    // A rollback is one from its creation, with its draft and its log entry in the same step.
    let r = open_rollback(store, &w.id, &me, UNDONE).expect("a rollback");
    assert_eq!(r.state, ProposalState::Draft);
    assert_eq!(
        store.rollback_of(&r.id, Some(&me)).unwrap().as_deref(),
        Some(UNDONE)
    );
    assert!(store.draft(&r.id, Some(&me)).unwrap().is_some());
    assert_eq!(store.transitions(&r.id, Some(&me)).unwrap().len(), 1);

    // Until it is submitted it is its author's alone, and so is what it undoes.
    assert_eq!(store.rollback_of(&r.id, Some(&you)).unwrap(), None);
    assert_eq!(store.rollback_of(&r.id, None).unwrap(), None);
    let held = submit(store, &r, 3);
    assert_eq!(held.state, ProposalState::HeldForReview);
    for viewer in [Some(&you), None] {
        assert_eq!(
            store.rollback_of(&r.id, viewer).unwrap().as_deref(),
            Some(UNDONE)
        );
    }

    // A proposal that is not there undoes nothing.
    assert_eq!(
        store.rollback_of(&ProposalId::new(), Some(&me)).unwrap(),
        None
    );

    // A block id that is not 64 lower-case hex digits is refused, and nothing is created.
    for bad in ["", "abc", &UNDONE.to_uppercase(), &format!("{UNDONE}0")] {
        let record = draft_record(&w.id, &me, 1);
        let created = step(
            &record.id,
            ProposalOp::Create,
            None,
            ProposalState::Draft,
            &me,
            None,
            1,
        );
        let refused = store.create_rollback_proposal(&record, &content(1, "x"), &created, bad);
        assert!(matches!(refused, Err(StoreError::Backend(_))), "{bad:?}");
        assert_eq!(store.proposal(&record.id, Some(&me)).unwrap(), None);
    }

    // A refused creation leaves no rollback note behind: unknown wiki, unknown author.
    let phantom = AccountId::new();
    let record = draft_record(&w.id, &phantom, 1);
    let created = step(
        &record.id,
        ProposalOp::Create,
        None,
        ProposalState::Draft,
        &me,
        None,
        1,
    );
    assert_eq!(
        store.create_rollback_proposal(&record, &content(1, "x"), &created, UNDONE),
        Err(StoreError::AccountNotFound)
    );
    assert_eq!(store.rollback_of(&record.id, Some(&me)).unwrap(), None);
    assert_eq!(
        open_rollback(store, &inklkunn_store::WikiId::new(), &me, UNDONE).err(),
        Some(StoreError::NotFound)
    );
}

#[test]
fn the_memory_store_keeps_rollback_proposals() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_keeps_rollback_proposals() {
    the_suite(&SqliteStore::open_in_memory().expect("open"));
}

fn flagged(minor: bool, automated: bool) -> RevisionContent {
    RevisionContent {
        base: base(1),
        minor,
        automated,
        ..content(1, "text")
    }
}

fn revise(
    store: &impl ProposalStore,
    p: &inklkunn_store::ProposalRecord,
    seq: u32,
    flags: RevisionContent,
    at: i64,
) {
    store
        .change_proposal(&ProposalChange {
            transition: step(
                &p.id,
                ProposalOp::Revise,
                Some(ProposalState::HeldForReview),
                ProposalState::HeldForReview,
                &p.author,
                None,
                at,
            ),
            text: None,
            revision: Some(RevisionRecord {
                proposal: p.id.clone(),
                seq,
                content: flags,
                created_at: at,
            }),
            superseded_by: None,
        })
        .expect("revise");
}

fn flags_suite(store: &(impl ProposalStore + WikiStore + AccountStore)) {
    let w = wiki(store, "flags");
    let me = account(store);
    // Revision 1 of `submit` is unflagged; the latest revision is the one that counts.
    let a = submit(store, &open_draft(store, &w.id, &me, 1), 2);
    let b = submit(store, &open_draft(store, &w.id, &me, 3), 4);
    revise(store, &b, 2, flagged(true, false), 5);
    let c = submit(store, &open_draft(store, &w.id, &me, 6), 7);
    revise(store, &c, 2, flagged(true, true), 8);
    revise(store, &c, 3, flagged(false, true), 9);
    let draft = open_draft(store, &w.id, &me, 10);

    let asked = [
        a.id.clone(),
        b.id.clone(),
        c.id.clone(),
        draft.id.clone(),
        ProposalId::new(),
    ];
    let mut got = store.revision_flags(&asked).unwrap();
    got.sort_by_key(|(id, _)| asked.iter().position(|x| x == id));
    assert_eq!(
        got,
        vec![
            (a.id, RevisionFlags::default()),
            (
                b.id,
                RevisionFlags {
                    minor: true,
                    automated: false
                }
            ),
            (
                c.id,
                RevisionFlags {
                    minor: false,
                    automated: true
                }
            ),
        ],
        "a draft and a proposal that is not there have no flags; the latest revision's count"
    );
    assert!(store.revision_flags(&[]).unwrap().is_empty());
}

#[test]
fn the_memory_store_reads_the_flags_of_the_latest_revision() {
    flags_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_reads_the_flags_of_the_latest_revision() {
    flags_suite(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_rollback_note_is_written_once_even_to_a_connection_that_bypasses_the_store() {
    let tmp = temp("rollback-once");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "once");
    let me = account(&store);
    let r = open_rollback(&store, &w.id, &me, UNDONE).expect("a rollback");
    let other = "00000000000000000000000000000000000000000000000000000000c0ffee02";

    let conn = rusqlite::Connection::open(&db).expect("raw open");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    for sql in [
        format!("UPDATE proposal_rollbacks SET undoes = '{other}'"),
        "DELETE FROM proposal_rollbacks".to_owned(),
        format!(
            "INSERT OR REPLACE INTO proposal_rollbacks (proposal_id, undoes) VALUES ('{}', '{other}')",
            r.id
        ),
        format!(
            "INSERT INTO proposal_rollbacks (proposal_id, undoes) VALUES ('{}', '{other}')",
            r.id
        ),
        // A note on a proposal that does not exist, and one that names no block.
        format!(
            "INSERT INTO proposal_rollbacks (proposal_id, undoes) VALUES ('{}', '{other}')",
            ProposalId::new()
        ),
        format!(
            "INSERT INTO proposal_rollbacks (proposal_id, undoes) VALUES ('{}', 'not-a-block')",
            open_draft(&store, &w.id, &me, 2).id
        ),
    ] {
        assert!(conn.execute(&sql, []).is_err(), "{sql}");
    }
    assert_eq!(
        store.rollback_of(&r.id, Some(&me)).unwrap().as_deref(),
        Some(UNDONE)
    );
}

#[test]
fn a_version_9_database_is_migrated_to_10_keeping_its_proposals_and_only_once() {
    use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
    let tmp = temp("v9");
    let db = tmp.0.join("store.sqlite");
    let (wiki_id, account_id) = (
        "019a0000-0000-7000-8000-0000000000e1",
        "019a0000-0000-7000-8000-0000000000e2",
    );
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 9) {
            raw.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        raw.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '9')",
            [],
        )
        .expect("version");
        raw.execute(
            "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, \
             visibility, policy_bootstrap, prikk_format_version, verification_state, verification_at, \
             author_key_id, maintainer_key_id, created_at, seq) VALUES (?1, 'nine', 'nine', 'heads/main', \
             'CC-BY-SA-4.0', '[\"en\"]', 'public', 'default', NULL, 'unknown', NULL, NULL, NULL, \
             1700000000, 1)",
            rusqlite::params![wiki_id],
        )
        .expect("a wiki");
        raw.execute(
            "INSERT INTO accounts (id, display_name, created_at) \
             VALUES (?1, 'Nine', 1700000000)",
            rusqlite::params![account_id],
        )
        .expect("an account");
    }
    let version = || -> String {
        rusqlite::Connection::open(&db)
            .expect("raw open")
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("version")
    };
    assert_eq!(version(), "9");
    let (w, a) = (
        inklkunn_store::WikiId::parse(wiki_id).unwrap(),
        inklkunn_store::AccountId::parse(account_id).unwrap(),
    );
    let old = {
        let store = SqliteStore::open(&db).expect("a version 9 database opens");
        assert_eq!(version(), SCHEMA_VERSION.to_string());
        assert_eq!(SCHEMA_VERSION, 10);
        // A proposal of version 9 is an ordinary one and is kept as it was; the new table works beside it.
        let old = submit(&store, &open_draft(&store, &w, &a, 5), 6);
        assert_eq!(store.rollback_of(&old.id, None).unwrap(), None);
        let r = open_rollback(&store, &w, &a, UNDONE).expect("a rollback");
        assert_eq!(
            store.rollback_of(&r.id, Some(&a)).unwrap().as_deref(),
            Some(UNDONE)
        );
        old
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(version(), "10");
    assert_eq!(
        store.proposal(&old.id, None).unwrap().map(|p| p.state),
        Some(ProposalState::HeldForReview)
    );
}
