//! The acceptance, intent and lease-holder records on both stores (handoff 004 T1; RFC 004 D-4, D-7;
//! CON-1, CON-3, CON-6): a retry finds the record and never starts a second, what is written once is refused
//! a second time on both stores and, for SQLite, through SQL, and everything survives a reopen.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::{
    AcceptanceBegin, AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult,
    AcceptanceStore, AccountId, AccountStore, IntentCompletion, IntentEnd, IntentId, IntentRecord,
    LeaseHolder, MemoryStore, ProposalRecord, ProposalStore, Role, SqliteStore, StoreError, WikiId,
    WikiStore,
};
use support::{account, nth, open_draft, submit, temp, wiki};

fn acceptance(
    p: &ProposalRecord,
    revision: u32,
    acceptor: &AccountId,
    bytes: u64,
    at: i64,
) -> AcceptanceRecord {
    AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: p.id.clone(),
        revision,
        wiki: p.wiki.clone(),
        view: p.view.clone(),
        acceptor: acceptor.clone(),
        acceptor_role: Role::Maintainer,
        policy_version: "default-1".to_owned(),
        proposer: p.author.clone(),
        digest: "d".repeat(64),
        bytes,
        created_at: at,
    }
}

fn sealed(a: &AcceptanceRecord, at: i64) -> AcceptanceResult {
    AcceptanceResult {
        acceptance: a.id.clone(),
        outcome: AcceptanceOutcome::Sealed {
            block_id: "b".repeat(64),
            patch_id: "c".repeat(64),
        },
        at,
    }
}

fn failed(a: &AcceptanceRecord, at: i64) -> AcceptanceResult {
    AcceptanceResult {
        acceptance: a.id.clone(),
        outcome: AcceptanceOutcome::Failed {
            word: "wiki-busy".to_owned(),
        },
        at,
    }
}

fn intent(a: &AcceptanceRecord, at: i64) -> IntentRecord {
    IntentRecord {
        id: IntentId::new(),
        acceptance: a.id.clone(),
        wiki: a.wiki.clone(),
        view: a.view.clone(),
        base_block: "e".repeat(64),
        message_digest: "f".repeat(64),
        message: "A summary\n\nKind: proposal\n".to_owned(),
        created_at: at,
    }
}

fn the_suite(store: &(impl ProposalStore + AcceptanceStore + WikiStore + AccountStore)) {
    let w = wiki(store, "accept");
    let other_wiki = wiki(store, "elsewhere");
    let (author, maintainer) = (account(store), account(store));
    let p = submit(store, &open_draft(store, &w.id, &author, 10), 11);

    // --- A new acceptance starts; a retry of the same revision finds it and writes nothing (CON-6).
    let a1 = acceptance(&p, 1, &maintainer, 100, 20);
    assert_eq!(
        store.begin_acceptance(&a1).unwrap(),
        AcceptanceBegin::Started
    );
    let retry = acceptance(&p, 1, &maintainer, 100, 21);
    match store.begin_acceptance(&retry).unwrap() {
        AcceptanceBegin::Existing(found) => {
            assert_eq!(found.acceptance, a1);
            assert_eq!(found.result, None, "it is pending seal");
        }
        other => panic!("the retry started a second acceptance: {other:?}"),
    }
    let listed = store.acceptances_of(&p.id, 1).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(nth(&listed, 0).acceptance, a1);

    // --- Its end is written once; after a seal a retry still finds the record.
    let done = sealed(&a1, 22);
    store.finish_acceptance(&done).unwrap();
    assert_eq!(
        store.finish_acceptance(&failed(&a1, 23)),
        Err(StoreError::AlreadyRecorded),
        "a second end is refused, and a sealed acceptance is not turned into a failed one"
    );
    assert_eq!(
        store.finish_acceptance(&done),
        Err(StoreError::AlreadyRecorded)
    );
    let found = store.acceptances_of(&p.id, 1).unwrap();
    assert_eq!(nth(&found, 0).result, Some(done.clone()));
    match store
        .begin_acceptance(&acceptance(&p, 1, &maintainer, 100, 24))
        .unwrap()
    {
        AcceptanceBegin::Existing(found) => assert_eq!(found.result, Some(done)),
        other => panic!("a sealed revision was accepted again: {other:?}"),
    }
    assert_eq!(store.acceptances_of(&p.id, 1).unwrap().len(), 1);

    // --- A failed acceptance does not stand in the way of the next try.
    let q = submit(store, &open_draft(store, &w.id, &author, 30), 31);
    let first = acceptance(&q, 1, &maintainer, 50, 32);
    assert_eq!(
        store.begin_acceptance(&first).unwrap(),
        AcceptanceBegin::Started
    );
    store.finish_acceptance(&failed(&first, 33)).unwrap();
    let second = acceptance(&q, 1, &maintainer, 50, 34);
    assert_eq!(
        store.begin_acceptance(&second).unwrap(),
        AcceptanceBegin::Started
    );
    let both = store.acceptances_of(&q.id, 1).unwrap();
    assert_eq!(both.len(), 2);
    assert!(matches!(
        nth(&both, 0).result.as_ref().map(|r| &r.outcome),
        Some(AcceptanceOutcome::Failed { word }) if word == "wiki-busy"
    ));
    assert_eq!(nth(&both, 1).result, None);

    // --- It names what exists.
    let other_p = acceptance(&p, 2, &maintainer, 1, 40);
    assert_eq!(
        store.begin_acceptance(&other_p),
        Err(StoreError::NotFound),
        "no revision 2"
    );
    let mut ghost = acceptance(&p, 1, &maintainer, 1, 40);
    ghost.proposal = inklkunn_store::ProposalId::new();
    assert_eq!(store.begin_acceptance(&ghost), Err(StoreError::NotFound));
    let mut nowhere = acceptance(&p, 1, &maintainer, 1, 40);
    nowhere.wiki = WikiId::new();
    assert_eq!(store.begin_acceptance(&nowhere), Err(StoreError::NotFound));
    let stranger = acceptance(&p, 1, &AccountId::new(), 1, 40);
    assert_eq!(
        store.begin_acceptance(&stranger),
        Err(StoreError::AccountNotFound)
    );
    assert_eq!(
        store.finish_acceptance(&sealed(&acceptance(&p, 1, &maintainer, 1, 40), 41)),
        Err(StoreError::NotFound)
    );
    let mut reused = acceptance(&p, 1, &maintainer, 1, 40);
    reused.id = a1.id.clone();
    assert_eq!(
        store.begin_acceptance(&reused),
        Err(StoreError::AlreadyRecorded)
    );

    // --- The accepted-bytes quota reads pending and sealed acceptances of the proposer, not failed ones.
    // a1 (100, sealed at 20), first (50, failed), second (50, pending at 34).
    assert_eq!(
        store.accepted_bytes_since(&w.id, &author, 0).unwrap(),
        vec![(20, 100), (34, 50)]
    );
    assert_eq!(
        store.accepted_bytes_since(&w.id, &author, 21).unwrap(),
        vec![(34, 50)]
    );
    assert_eq!(
        store
            .accepted_bytes_since(&w.id, &author, 20)
            .unwrap()
            .len(),
        2,
        "the boundary counts"
    );
    assert!(
        store
            .accepted_bytes_since(&w.id, &maintainer, 0)
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .accepted_bytes_since(&other_wiki.id, &author, 0)
            .unwrap()
            .is_empty()
    );

    // --- An intent is written once per acceptance, before the signer is asked; it ends once.
    assert!(store.open_intents(&w.id).unwrap().is_empty());
    let i1 = intent(&a1, 25);
    store.record_intent(&i1).unwrap();
    assert_eq!(
        store.record_intent(&intent(&a1, 26)),
        Err(StoreError::AlreadyRecorded)
    );
    let mut same_id = intent(&second, 26);
    same_id.id = i1.id.clone();
    assert_eq!(
        store.record_intent(&same_id),
        Err(StoreError::AlreadyRecorded)
    );
    let mut lost = intent(&a1, 26);
    lost.acceptance = AcceptanceId::new();
    assert_eq!(store.record_intent(&lost), Err(StoreError::NotFound));
    let i2 = intent(&second, 35);
    store.record_intent(&i2).unwrap();
    let open = store.open_intents(&w.id).unwrap();
    assert_eq!(
        open.iter().map(|e| &e.intent).collect::<Vec<_>>(),
        vec![&i1, &i2]
    );
    assert!(store.open_intents(&other_wiki.id).unwrap().is_empty());
    let end = IntentCompletion {
        intent: i1.id.clone(),
        end: IntentEnd::Completed,
        at: 27,
    };
    store.complete_intent(&end).unwrap();
    assert_eq!(
        store.complete_intent(&IntentCompletion {
            end: IntentEnd::Discarded,
            ..end.clone()
        }),
        Err(StoreError::AlreadyRecorded)
    );
    assert_eq!(
        store.complete_intent(&IntentCompletion {
            intent: IntentId::new(),
            end: IntentEnd::Abandoned,
            at: 1
        }),
        Err(StoreError::NotFound)
    );
    let still_open = store.open_intents(&w.id).unwrap();
    assert_eq!(
        still_open.iter().map(|e| &e.intent).collect::<Vec<_>>(),
        vec![&i2]
    );
    let all = store.intents(&w.id).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(nth(&all, 0).completion, Some(end));
    assert_eq!(nth(&all, 1).completion, None);

    // --- The sealed acceptances, with their intents: what the reconciliation compares the history with.
    // a1 sealed and has its intent; `second` is pending; `first` failed and has none.
    let sealed_list = store.sealed_acceptances(&w.id).unwrap();
    assert_eq!(sealed_list.len(), 1);
    let only = nth(&sealed_list, 0);
    assert_eq!(only.acceptance, a1);
    assert_eq!(
        only.intent, i1,
        "the message the intent holds comes back whole"
    );
    assert_eq!(
        (only.block_id.as_str(), only.patch_id.as_str()),
        (&*"b".repeat(64), &*"c".repeat(64))
    );
    assert!(store.sealed_acceptances(&other_wiki.id).unwrap().is_empty());

    // --- The lease holder is a witness that changes hands.
    let holder = |who: &str, at: i64| LeaseHolder {
        wiki: w.id.clone(),
        holder: who.to_owned(),
        purpose: "acceptance".to_owned(),
        acceptance: Some(second.id.clone()),
        since: at,
    };
    assert_eq!(store.lease_holder(&w.id).unwrap(), None);
    assert_eq!(
        store
            .record_lease_holder(&holder("instance-a", 50))
            .unwrap(),
        None
    );
    assert_eq!(
        store.lease_holder(&w.id).unwrap(),
        Some(holder("instance-a", 50))
    );
    assert_eq!(
        store
            .record_lease_holder(&holder("instance-b", 60))
            .unwrap(),
        Some(holder("instance-a", 50)),
        "the replaced record is handed back"
    );
    assert!(
        !store.release_lease_holder(&w.id, "instance-a").unwrap(),
        "a stale release clears nothing"
    );
    assert_eq!(
        store.lease_holder(&w.id).unwrap(),
        Some(holder("instance-b", 60))
    );
    assert!(
        !store
            .release_lease_holder(&other_wiki.id, "instance-b")
            .unwrap()
    );
    assert!(store.release_lease_holder(&w.id, "instance-b").unwrap());
    assert_eq!(store.lease_holder(&w.id).unwrap(), None);
    let mut no_wiki = holder("x", 1);
    no_wiki.wiki = WikiId::new();
    assert_eq!(
        store.record_lease_holder(&no_wiki),
        Err(StoreError::NotFound)
    );
    let mut no_acceptance = holder("x", 1);
    no_acceptance.acceptance = Some(AcceptanceId::new());
    assert_eq!(
        store.record_lease_holder(&no_acceptance),
        Err(StoreError::NotFound)
    );
    let mut bare = holder("x", 1);
    bare.acceptance = None;
    assert_eq!(store.record_lease_holder(&bare).unwrap(), None);
}

#[test]
fn the_memory_store_holds_the_acceptance_invariants() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_holds_the_acceptance_invariants() {
    the_suite(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_write_once_acceptance_tables_cannot_be_edited_or_erased_through_sql() {
    let tmp = temp("accept-sql");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "sql");
    let (author, maintainer) = (account(&store), account(&store));
    let p = submit(&store, &open_draft(&store, &w.id, &author, 1), 2);
    let a = acceptance(&p, 1, &maintainer, 10, 3);
    store.begin_acceptance(&a).unwrap();
    store.finish_acceptance(&failed(&a, 4)).unwrap();
    let i = intent(&a, 5);
    store.record_intent(&i).unwrap();
    store
        .complete_intent(&IntentCompletion {
            intent: i.id.clone(),
            end: IntentEnd::Abandoned,
            at: 6,
        })
        .unwrap();

    let conn = rusqlite::Connection::open(&db).expect("raw open");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let statements: Vec<String> = vec![
        "UPDATE acceptances SET bytes = 0".into(),
        "DELETE FROM acceptances".into(),
        format!("INSERT OR REPLACE INTO acceptances (id, seq, proposal_id, revision, wiki_id, view, acceptor, acceptor_role, policy_version, proposer, digest, bytes, created_at) VALUES ('{}', 99, '{}', 1, '{}', 'heads/main', '{maintainer}', 'admin', 'x', '{author}', 'x', 0, 9)", a.id, p.id, w.id),
        "UPDATE acceptance_results SET outcome = 'sealed', block_id = 'b', patch_id = 'p', word = NULL".into(),
        "DELETE FROM acceptance_results".into(),
        format!("INSERT OR REPLACE INTO acceptance_results (acceptance_id, outcome, block_id, patch_id, at) VALUES ('{}', 'sealed', 'b', 'p', 9)", a.id),
        "UPDATE intents SET message_digest = 'forged'".into(),
        "DELETE FROM intents".into(),
        format!("INSERT OR REPLACE INTO intents (id, seq, acceptance_id, wiki_id, view, base_block, message_digest, created_at) VALUES ('{}', 99, '{}', '{}', 'v', 'b', 'forged', 9)", i.id, a.id, w.id),
        // The other unique key: a different id, the same acceptance.
        format!("INSERT OR REPLACE INTO intents (id, seq, acceptance_id, wiki_id, view, base_block, message_digest, created_at) VALUES ('{}', 99, '{}', '{}', 'v', 'b', 'forged', 9)", IntentId::new(), a.id, w.id),
        "UPDATE intent_completions SET how = 'completed'".into(),
        "DELETE FROM intent_completions".into(),
        format!("INSERT OR REPLACE INTO intent_completions (intent_id, how, at) VALUES ('{}', 'completed', 9)", i.id),
    ];
    for sql in &statements {
        let e = conn.execute(sql, []).expect_err(sql).to_string();
        assert!(e.contains("append-only"), "{sql}: {e}");
    }
    // Nothing moved: the acceptance still failed, the intent still ended as abandoned.
    let kept = store.acceptances_of(&p.id, 1).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(nth(&kept, 0).result, Some(failed(&a, 4)));
    assert_eq!(nth(&kept, 0).acceptance.bytes, 10);
    let intents = store.intents(&w.id).unwrap();
    assert_eq!(intents.len(), 1);
    assert_eq!(nth(&intents, 0).intent, i);
    assert_eq!(
        nth(&intents, 0).completion.as_ref().map(|c| c.end),
        Some(IntentEnd::Abandoned)
    );
    // The lease holder, on the other hand, is meant to change.
    let holder = LeaseHolder {
        wiki: w.id.clone(),
        holder: "me".into(),
        purpose: "export".into(),
        acceptance: None,
        since: 1,
    };
    store.record_lease_holder(&holder).unwrap();
    conn.execute("UPDATE lease_holders SET holder = 'someone-else'", [])
        .expect("mutable");
    conn.execute("DELETE FROM lease_holders", [])
        .expect("mutable");
}

#[test]
fn the_acceptance_records_persist_across_reopen() {
    let tmp = temp("accept-reopen");
    let db = tmp.0.join("store.sqlite");
    let (w, p, a, i) = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persist");
        let (author, maintainer) = (account(&store), account(&store));
        let p = submit(&store, &open_draft(&store, &w.id, &author, 1), 2);
        let a = acceptance(&p, 1, &maintainer, 77, 3);
        store.begin_acceptance(&a).unwrap();
        let i = intent(&a, 4);
        store.record_intent(&i).unwrap();
        store
            .record_lease_holder(&LeaseHolder {
                wiki: w.id.clone(),
                holder: "instance-a".into(),
                purpose: "acceptance".into(),
                acceptance: Some(a.id.clone()),
                since: 5,
            })
            .unwrap();
        (w, p, a, i)
    };
    // The process that wrote them is gone: what a crashed acceptance leaves is found again, pending, with
    // its intent open and its lease holder named (what recovery starts from).
    let store = SqliteStore::open(&db).expect("reopen");
    let found = store.acceptances_of(&p.id, 1).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(nth(&found, 0).acceptance, a);
    assert_eq!(nth(&found, 0).result, None);
    let open = store.open_intents(&w.id).unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(nth(&open, 0).intent, i);
    assert_eq!(
        store.lease_holder(&w.id).unwrap().map(|l| l.holder),
        Some("instance-a".to_owned())
    );
    // A retry after the restart still finds the pending record instead of starting another.
    assert!(matches!(
        store
            .begin_acceptance(&acceptance(&p, 1, &a.acceptor, 77, 9))
            .unwrap(),
        AcceptanceBegin::Existing(_)
    ));
}
