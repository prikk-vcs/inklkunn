//! The same invariants on both stores (handoff 001 T4), plus reopen persistence for SQLite.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    MemoryStore, NewWiki, RootLayout, Slug, SqliteStore, StoreError, Verification,
    VerificationState, Visibility, WikiStore,
};

fn slug(s: &str) -> Slug {
    Slug::new(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn invariants(store: &dyn WikiStore) {
    let a = store
        .create(NewWiki::new(slug("alpha"), "Alpha"))
        .expect("create");
    assert_eq!(a.slug.as_str(), "alpha");
    assert_eq!(a.default_view, "heads/main");
    assert_eq!(a.verification.state, VerificationState::Unknown);

    // Slugs are unique.
    assert_eq!(
        store.create(NewWiki::new(slug("alpha"), "Again")).err(),
        Some(StoreError::SlugTaken("alpha".into()))
    );

    let mut b_new = NewWiki::new(slug("beta"), "Beta");
    b_new.visibility = Visibility::Restricted;
    b_new.languages = vec!["ja".into(), "en".into()];
    let b = store.create(b_new).expect("create b");
    assert_eq!(b.visibility, Visibility::Restricted);
    assert_eq!(b.languages, vec!["ja", "en"]);

    // Open by id and by slug agree.
    assert_eq!(store.get(&a.id).expect("get"), a);
    assert_eq!(store.by_slug(&slug("beta")).expect("by slug"), b);
    assert_eq!(
        store.get(&inklkunn_store::WikiId::new()).err(),
        Some(StoreError::NotFound)
    );
    assert_eq!(
        store.by_slug(&slug("nope")).err(),
        Some(StoreError::NotFound)
    );

    // List is in creation order.
    let ids: Vec<_> = store
        .list()
        .expect("list")
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, vec![a.id.clone(), b.id.clone()]);

    // Rename changes the slug and the name, never the id; a taken slug is refused.
    let renamed = store
        .rename(&a.id, Some(slug("alpha-two")), Some("Alpha II".into()))
        .expect("rename");
    assert_eq!(renamed.id, a.id);
    assert_eq!(renamed.slug.as_str(), "alpha-two");
    assert_eq!(renamed.display_name, "Alpha II");
    assert_eq!(
        store.by_slug(&slug("alpha")).err(),
        Some(StoreError::NotFound)
    );
    assert_eq!(
        store.rename(&a.id, Some(slug("beta")), None).err(),
        Some(StoreError::SlugTaken("beta".into()))
    );
    assert_eq!(
        store
            .rename(&a.id, Some(slug("alpha-two")), None)
            .expect("same slug is fine")
            .slug
            .as_str(),
        "alpha-two"
    );

    // Verification and keys.
    let v = store
        .set_verification(
            &b.id,
            Verification {
                state: VerificationState::Verified,
                at: Some(1_700_000_000),
            },
        )
        .expect("verification");
    assert_eq!(v.verification.at, Some(1_700_000_000));
    let k = store
        .set_keys(
            &b.id,
            Some("ed25519-aaaa".into()),
            Some("ed25519-bbbb".into()),
            Some("prikk-format-3".into()),
        )
        .expect("keys");
    assert_eq!(k.author_key_id.as_deref(), Some("ed25519-aaaa"));
    assert_eq!(
        store
            .get(&b.id)
            .expect("get b")
            .prikk_format_version
            .as_deref(),
        Some("prikk-format-3")
    );
}

#[test]
fn memory_store_holds_the_invariants() {
    invariants(&MemoryStore::new());
}

#[test]
fn sqlite_store_holds_the_invariants() {
    invariants(&SqliteStore::open_in_memory().expect("open"));
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-store-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

#[test]
fn sqlite_store_persists_across_reopen() {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let id = {
        let store = SqliteStore::open(&db).expect("open");
        store
            .create(NewWiki::new(slug("persist"), "Persist"))
            .expect("create")
            .id
    };
    let store = SqliteStore::open(&db).expect("reopen");
    let r = store.get(&id).expect("still there");
    assert_eq!(r.slug.as_str(), "persist");
    assert_eq!(store.list().expect("list").len(), 1);
}

#[test]
fn the_root_layout_puts_a_wiki_under_its_shard_and_renames_move_nothing() {
    let tmp = temp("root");
    let layout = RootLayout::new(&tmp.0);
    let store = MemoryStore::new();
    let w = store
        .create(NewWiki::new(slug("laid-out"), "Laid out"))
        .expect("create");
    let repo = layout.prepare(&w.id).expect("prepare");
    assert!(repo.is_dir());
    assert!(repo.starts_with(tmp.0.join(w.id.shard())));
    assert!(!repo.to_string_lossy().contains("laid-out"));
    let renamed = store
        .rename(&w.id, Some(slug("other")), None)
        .expect("rename");
    assert_eq!(
        layout.repo_dir(&renamed.id),
        repo,
        "the directory is the id's, not the slug's"
    );
}
