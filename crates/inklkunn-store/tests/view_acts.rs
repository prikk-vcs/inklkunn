//! The record of a view's creation (RFC 005 part 2 D-9; handoff 005 K-T8): written before the signer is asked,
//! ended once, listed oldest first, the name held by the first act that stands and freed by one that failed. The
//! suite is written against the trait, so the SQLite store, when it keeps these records, runs the same one.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::{
    AccountId, AccountStore, ActStore, MemoryStore, StoreError, ViewActEnd, ViewActId,
    ViewActOutcome, ViewActRecord, ViewKind, WikiId, WikiStore,
};
use support::{account, wiki};

fn act(w: &WikiId, by: &AccountId, name: &str, kind: ViewKind, at: i64) -> ViewActRecord {
    ViewActRecord {
        id: ViewActId::new(),
        wiki: w.clone(),
        name: name.to_owned(),
        forked_from: "main".to_owned(),
        from_block: "e".repeat(64),
        kind,
        created_by: by.clone(),
        created_at: at,
    }
}

fn ended(a: &ViewActRecord, outcome: ViewActOutcome, at: i64) -> ViewActEnd {
    ViewActEnd {
        act: a.id.clone(),
        outcome,
        at,
    }
}

fn the_suite(store: &(impl ActStore + WikiStore + AccountStore)) {
    let w = wiki(store, "views");
    let other = wiki(store, "elsewhere");
    let by = account(store);

    // --- Nothing yet; the kinds are two words and read back.
    assert!(store.keeps_view_acts());
    assert!(store.view_acts(&w.id).unwrap().is_empty());
    assert_eq!(ViewKind::ALL.len(), 2);
    for kind in ViewKind::ALL {
        assert_eq!(ViewKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(ViewKind::parse("canonical"), None);
    assert_eq!(ViewKind::parse(""), None);

    // --- A creation is written before the signer is asked; it is open until it ends.
    let draft = act(&w.id, &by, "draft", ViewKind::Experimental, 10);
    let reading = act(&w.id, &by, "reading", ViewKind::Interpretation, 11);
    store.begin_view_act(&draft).unwrap();
    store.begin_view_act(&reading).unwrap();
    assert_eq!(
        store.begin_view_act(&draft),
        Err(StoreError::AlreadyRecorded),
        "an id is used once"
    );
    assert_eq!(
        store.begin_view_act(&act(&WikiId::new(), &by, "x", ViewKind::Experimental, 1)),
        Err(StoreError::NotFound),
        "no such wiki"
    );
    assert_eq!(store.open_view_act(&draft.id).unwrap(), Some(draft.clone()));
    assert_eq!(store.open_view_act(&ViewActId::new()).unwrap(), None);
    let listed = store.view_acts(&w.id).unwrap();
    assert_eq!(
        listed.iter().map(|e| &e.act).collect::<Vec<_>>(),
        vec![&draft, &reading],
        "oldest first"
    );
    assert!(listed.iter().all(|e| e.end.is_none() && e.stands()));
    assert!(store.view_acts(&other.id).unwrap().is_empty());

    // --- The name is held by the first act that stands, in the wiki only.
    assert_eq!(
        store.begin_view_act(&act(&w.id, &by, "draft", ViewKind::Interpretation, 12)),
        Err(StoreError::AlreadyRecorded),
        "an open act holds its name"
    );
    store
        .begin_view_act(&act(&other.id, &by, "draft", ViewKind::Experimental, 12))
        .expect("another wiki may use the name");

    // --- An act ends once: created, or failed with a word.
    let created = ended(&draft, ViewActOutcome::Created, 20);
    store.finish_view_act(&created).unwrap();
    assert_eq!(
        store.finish_view_act(&created),
        Err(StoreError::AlreadyRecorded),
        "a second end is refused"
    );
    assert_eq!(
        store.finish_view_act(&ended(
            &draft,
            ViewActOutcome::Failed {
                word: "refused".to_owned()
            },
            21
        )),
        Err(StoreError::AlreadyRecorded)
    );
    assert_eq!(
        store.finish_view_act(&ended(
            &act(&w.id, &by, "ghost", ViewKind::Experimental, 1),
            ViewActOutcome::Created,
            1
        )),
        Err(StoreError::NotFound),
        "an end names an act that is there"
    );
    assert_eq!(
        store.open_view_act(&draft.id).unwrap(),
        None,
        "an ended act is no longer open"
    );
    assert_eq!(
        store.begin_view_act(&act(&w.id, &by, "draft", ViewKind::Interpretation, 22)),
        Err(StoreError::AlreadyRecorded),
        "a created view keeps its name"
    );

    // --- One that failed took nothing in: the name is free, and the failed act no longer stands.
    let failed = ended(
        &reading,
        ViewActOutcome::Failed {
            word: "refused".to_owned(),
        },
        23,
    );
    store.finish_view_act(&failed).unwrap();
    let again = act(&w.id, &by, "reading", ViewKind::Experimental, 24);
    store.begin_view_act(&again).unwrap();
    let listed = store.view_acts(&w.id).unwrap();
    assert_eq!(listed.len(), 3, "the failed act stays on the record");
    let standing: Vec<&str> = listed
        .iter()
        .filter(|e| e.stands())
        .map(|e| e.act.name.as_str())
        .collect();
    assert_eq!(standing, vec!["draft", "reading"]);
    assert_eq!(
        listed
            .iter()
            .find(|e| e.act.id == reading.id)
            .and_then(|e| e.end.clone()),
        Some(failed)
    );
    assert_eq!(
        listed.iter().find(|e| e.act.id == draft.id).unwrap().end,
        Some(created)
    );
}

#[test]
fn the_memory_store_keeps_the_record_of_a_views_creation() {
    the_suite(&MemoryStore::new());
}

#[test]
fn a_store_that_keeps_none_says_so_refuses_to_begin_one_and_lists_none() {
    // The SQLite store keeps no record of a view's creation until the schema that would hold it is decided: the
    // defaults of the trait answer, so a service over it never takes a first step it cannot record.
    let store = inklkunn_store::SqliteStore::open_in_memory().unwrap();
    let w = wiki(&store, "views");
    let by = account(&store);
    assert!(!store.keeps_view_acts());
    let a = act(&w.id, &by, "draft", ViewKind::Experimental, 1);
    assert!(matches!(
        store.begin_view_act(&a),
        Err(StoreError::Backend(_))
    ));
    assert_eq!(store.open_view_act(&a.id).unwrap(), None);
    assert!(store.view_acts(&w.id).unwrap().is_empty());
}
