//! Watches and inbox events on both stores (handoff 004 T1; RFC 004 D-9): the records only. Who is told what,
//! and when, is handoff 004 T6.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::{
    AccountId, AccountStore, InboxEvent, InboxEventId, InboxEventKind, InboxStore, MemoryStore,
    ProposalId, SqliteStore, StoreError, WatchRecord, WatchTarget, WikiId, WikiStore,
};
use support::{account, open_draft, submit, temp, wiki};

fn event(
    to: &AccountId,
    w: &WikiId,
    p: Option<&ProposalId>,
    kind: InboxEventKind,
    at: i64,
) -> InboxEvent {
    InboxEvent {
        id: InboxEventId::new(),
        account: to.clone(),
        wiki: w.clone(),
        proposal: p.cloned(),
        kind,
        actor: None,
        created_at: at,
        read_at: None,
    }
}

fn the_suite(store: &(impl InboxStore + inklkunn_store::ProposalStore + WikiStore + AccountStore)) {
    let w = wiki(store, "inbox");
    let other_wiki = wiki(store, "elsewhere");
    let (me, you) = (account(store), account(store));
    let p = submit(store, &open_draft(store, &w.id, &me, 1), 2);

    // --- Watching a page or a namespace; again is no change; the two kinds are different watches.
    let page = WatchRecord {
        account: me.clone(),
        wiki: w.id.clone(),
        target: WatchTarget::Page("pages/home.md".to_owned()),
        created_at: 10,
    };
    let namespace = WatchRecord {
        target: WatchTarget::Namespace("pages/home.md".to_owned()),
        created_at: 11,
        ..page.clone()
    };
    assert!(store.watch(&page).unwrap());
    assert!(
        !store
            .watch(&WatchRecord {
                created_at: 99,
                ..page.clone()
            })
            .unwrap()
    );
    assert!(store.watch(&namespace).unwrap());
    let yours = WatchRecord {
        account: you.clone(),
        created_at: 12,
        ..page.clone()
    };
    assert!(store.watch(&yours).unwrap());
    assert_eq!(
        store.watches_of(&me).unwrap(),
        vec![page.clone(), namespace.clone()]
    );
    assert_eq!(
        store.watches_in(&w.id).unwrap(),
        vec![page.clone(), namespace.clone(), yours.clone()]
    );
    assert!(store.watches_in(&other_wiki.id).unwrap().is_empty());
    assert!(store.unwatch(&me, &w.id, &page.target).unwrap());
    assert!(!store.unwatch(&me, &w.id, &page.target).unwrap());
    assert_eq!(store.watches_of(&me).unwrap(), vec![namespace.clone()]);
    assert_eq!(store.watches_of(&you).unwrap(), vec![yours]);
    let ghost_wiki = WatchRecord {
        wiki: WikiId::new(),
        ..page.clone()
    };
    assert_eq!(store.watch(&ghost_wiki), Err(StoreError::NotFound));
    let ghost = WatchRecord {
        account: AccountId::new(),
        ..page.clone()
    };
    assert_eq!(store.watch(&ghost), Err(StoreError::AccountNotFound));

    // --- Events go to an inbox, newest first, and carry ids and a kind and nothing else.
    let e1 = event(&me, &w.id, Some(&p.id), InboxEventKind::Reviewed, 20);
    let e2 = event(&me, &w.id, Some(&p.id), InboxEventKind::Accepted, 21);
    let e3 = InboxEvent {
        actor: Some(you.clone()),
        ..event(&me, &w.id, None, InboxEventKind::WatchedPageChanged, 22)
    };
    let theirs = event(&you, &w.id, None, InboxEventKind::Mentioned, 23);
    for e in [&e1, &e2, &e3, &theirs] {
        store.push_event(e).unwrap();
    }
    assert_eq!(
        store.push_event(&e1),
        Err(StoreError::IdTaken(e1.id.to_string()))
    );
    assert_eq!(
        store.inbox(&me, false, 10).unwrap(),
        vec![e3.clone(), e2.clone(), e1.clone()]
    );
    assert_eq!(
        store.inbox(&me, false, 2).unwrap(),
        vec![e3.clone(), e2.clone()]
    );
    assert_eq!(store.inbox(&you, false, 10).unwrap(), vec![theirs.clone()]);
    assert!(
        store
            .inbox(&AccountId::new(), false, 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.push_event(&event(
            &me,
            &WikiId::new(),
            None,
            InboxEventKind::Declined,
            1
        )),
        Err(StoreError::NotFound)
    );
    assert_eq!(
        store.push_event(&event(
            &AccountId::new(),
            &w.id,
            None,
            InboxEventKind::Declined,
            1
        )),
        Err(StoreError::AccountNotFound)
    );
    assert_eq!(
        store.push_event(&event(
            &me,
            &w.id,
            Some(&ProposalId::new()),
            InboxEventKind::Declined,
            1
        )),
        Err(StoreError::NotFound)
    );

    // --- Reading marks an event, once; someone else's inbox is not reachable by an id.
    assert!(
        !store.mark_read(&you, &e1.id, 30).unwrap(),
        "not your event"
    );
    assert_eq!(store.inbox(&me, true, 10).unwrap().len(), 3);
    assert!(store.mark_read(&me, &e1.id, 31).unwrap());
    assert!(store.mark_read(&me, &e1.id, 99).unwrap());
    let read = store.inbox(&me, false, 10).unwrap();
    assert_eq!(
        read.last().and_then(|e| e.read_at),
        Some(31),
        "the first reading is kept"
    );
    assert_eq!(store.inbox(&me, true, 10).unwrap(), vec![e3, e2]);
    assert!(!store.mark_read(&me, &InboxEventId::new(), 1).unwrap());
    assert_eq!(
        store.inbox(&you, true, 10).unwrap(),
        vec![theirs],
        "yours is untouched"
    );
}

#[test]
fn the_memory_store_holds_the_inbox_invariants() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_holds_the_inbox_invariants() {
    the_suite(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn watches_and_events_persist_across_reopen() {
    let tmp = temp("inbox-reopen");
    let db = tmp.0.join("store.sqlite");
    let (me, w) = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persist");
        let me = account(&store);
        store
            .watch(&WatchRecord {
                account: me.clone(),
                wiki: w.id.clone(),
                target: WatchTarget::Namespace("pages/".to_owned()),
                created_at: 1,
            })
            .unwrap();
        store
            .push_event(&event(&me, &w.id, None, InboxEventKind::Mentioned, 2))
            .unwrap();
        (me, w.id)
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.watches_in(&w).unwrap().len(), 1);
    assert_eq!(store.inbox(&me, true, 10).unwrap().len(), 1);
}

#[test]
fn an_event_kind_this_build_cannot_read_is_an_error_not_a_guess() {
    let tmp = temp("inbox-kind");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "kinds");
    let me = account(&store);
    store
        .push_event(&event(&me, &w.id, None, InboxEventKind::Mentioned, 1))
        .unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("UPDATE inbox_events SET kind = 'from-the-future'", [])
        .unwrap();
    assert!(matches!(
        store.inbox(&me, false, 10),
        Err(StoreError::Backend(_))
    ));
}
