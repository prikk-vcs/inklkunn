//! What the service knows about prikk's checkpoint record (handoff 003 T3, hold note IS-9; ledger PK-24):
//! the prikk version of the last refresh and a reader's request for one, on both stores.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    Checkpoint, DisplayName, MemoryStore, NewWiki, PrikkVersion, Slug, SqliteStore, StoreError,
    WikiId, WikiStore,
};

fn version(s: &str) -> PrikkVersion {
    PrikkVersion::parse(s).unwrap_or_else(|| panic!("{s}"))
}

fn wiki(store: &dyn WikiStore, slug: &str) -> WikiId {
    store
        .create(NewWiki::new(
            Slug::new(slug).expect("slug"),
            DisplayName::new(slug).expect("name"),
        ))
        .expect("create")
        .id
}

fn invariants(store: &dyn WikiStore) {
    let w = wiki(store, "checkpointed");
    let other = wiki(store, "bystander");

    // A new wiki has never been refreshed and nobody has asked.
    let fresh = store.get(&w).unwrap();
    assert_eq!(fresh.checkpoint, Checkpoint::default());
    let running = version("prikk 0.54.1");
    assert!(
        fresh.checkpoint.needs_refresh(&running),
        "never refreshed under this prikk"
    );

    // The first request is kept; asking again changes nothing (the earliest unanswered request stands).
    let asked = store.request_checkpoint_refresh(&w, 100).unwrap();
    assert_eq!(asked.checkpoint.refresh_requested_at, Some(100));
    let again = store.request_checkpoint_refresh(&w, 200).unwrap();
    assert_eq!(again.checkpoint.refresh_requested_at, Some(100));
    assert_eq!(store.get(&w).unwrap(), again);
    assert_eq!(
        store.get(&other).unwrap().checkpoint,
        Checkpoint::default(),
        "another wiki is untouched"
    );

    // Recording a refresh names the prikk and clears the request.
    let done = store.record_checkpoint_refresh(&w, &running).unwrap();
    assert_eq!(done.checkpoint.refreshed_with, Some(running.clone()));
    assert_eq!(done.checkpoint.refresh_requested_at, None);
    assert!(
        !done.checkpoint.needs_refresh(&running),
        "refreshed under this prikk and nobody asking"
    );
    // A different prikk (an upgrade) needs one again; so does a new request under the same prikk.
    assert!(done.checkpoint.needs_refresh(&version("prikk 0.55.0")));
    let again = store.request_checkpoint_refresh(&w, 300).unwrap();
    assert_eq!(again.checkpoint.refresh_requested_at, Some(300));
    assert!(again.checkpoint.needs_refresh(&running));
    // The version recorded before the request stays recorded until the next refresh replaces it.
    assert_eq!(again.checkpoint.refreshed_with, Some(running));
    let newer = version("prikk 0.55.0");
    let done = store.record_checkpoint_refresh(&w, &newer).unwrap();
    assert_eq!(done.checkpoint.refreshed_with, Some(newer));
    assert_eq!(done.checkpoint.refresh_requested_at, None);

    // An unknown wiki is not found, and nothing is made.
    let ghost = WikiId::new();
    assert_eq!(
        store.request_checkpoint_refresh(&ghost, 1).err(),
        Some(StoreError::NotFound)
    );
    assert_eq!(
        store
            .record_checkpoint_refresh(&ghost, &version("prikk 0.54.1"))
            .err(),
        Some(StoreError::NotFound)
    );
}

#[test]
fn both_stores_hold_the_checkpoint_invariants() {
    invariants(&MemoryStore::new());
    invariants(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn a_version_text_is_short_printable_and_trimmed() {
    assert_eq!(version("prikk 0.54.1").as_str(), "prikk 0.54.1");
    assert_eq!(version("  prikk 0.54.1\n").as_str(), "prikk 0.54.1".trim());
    for bad in [
        "",
        "   ",
        "prikk\u{0}0.54",
        "prikk\n0.54",
        "prikk ü",
        &"x".repeat(65),
    ] {
        assert!(PrikkVersion::parse(bad).is_none(), "{bad:?}");
    }
    assert!(PrikkVersion::parse(&"x".repeat(64)).is_some());
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_checkpoint_persists_across_reopen_and_a_wiki_inserted_with_one_keeps_it() {
    let p = std::env::temp_dir().join(format!("inklkunn-store-cp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    let tmp = TempDir(p);
    let db = tmp.0.join("store.sqlite");
    let (w, inserted) = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persists");
        store.request_checkpoint_refresh(&w, 5).unwrap();
        // A complete record inserted as one lands with its checkpoint.
        let mut full = inklkunn_store::WikiRecord::from_new(
            WikiId::new(),
            NewWiki::new(
                Slug::new("complete").expect("slug"),
                DisplayName::new("Complete").expect("name"),
            ),
            3,
        )
        .expect("record");
        full.checkpoint = Checkpoint {
            refreshed_with: Some(version("prikk 0.54.1")),
            refresh_requested_at: Some(9),
        };
        assert_eq!(store.insert(full.clone()).expect("insert"), full);
        (w, full)
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(
        store.get(&w).unwrap().checkpoint.refresh_requested_at,
        Some(5)
    );
    assert_eq!(store.get(&inserted.id).unwrap(), inserted);
}
