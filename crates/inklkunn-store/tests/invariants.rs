//! The same invariants on both stores (handoff 001 T4), plus reopen persistence, the schema-version
//! refusal and the stored-slug rule for SQLite.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    DisplayName, MemoryStore, NewWiki, RootLayout, Slug, SqliteStore, StoreError, Verification,
    VerificationState, Visibility, WikiId, WikiRecord, WikiStore,
};

fn slug(s: &str) -> Slug {
    Slug::new(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn name(s: &str) -> DisplayName {
    DisplayName::new(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn invariants(store: &dyn WikiStore) {
    let a = store
        .create(NewWiki::new(slug("alpha"), name("Alpha")))
        .expect("create");
    assert_eq!(a.slug.as_str(), "alpha");
    assert_eq!(a.default_view, "heads/main");
    assert_eq!(a.verification.state, VerificationState::Unknown);

    // Slugs are unique, and an insert says exactly what was taken.
    assert_eq!(
        store
            .create(NewWiki::new(slug("alpha"), name("Again")))
            .err(),
        Some(StoreError::SlugTaken("alpha".into()))
    );
    let dup = WikiRecord::from_new(a.id.clone(), NewWiki::new(slug("other"), name("Dup")), 2)
        .expect("record");
    assert_eq!(
        store.insert(dup).err(),
        Some(StoreError::IdTaken(a.id.to_string()))
    );
    assert_eq!(
        store.list().expect("list").len(),
        1,
        "nothing half-inserted"
    );

    let mut b_new = NewWiki::new(slug("beta"), name("Beta"));
    b_new.visibility = Visibility::Restricted;
    b_new.languages = vec!["ja".into(), "en".into()];
    let b = store.create(b_new).expect("create b");
    assert_eq!(b.visibility, Visibility::Restricted);
    assert_eq!(b.languages, vec!["ja", "en"]);

    // A request with a bad free-text field is refused before anything is stored.
    let mut bad = NewWiki::new(slug("gamma"), name("Gamma"));
    bad.default_view = Some("main".into());
    assert!(matches!(
        store.create(bad),
        Err(StoreError::InvalidRequest(_))
    ));
    assert!(store.by_slug(&slug("gamma")).is_err());

    // Open by id and by slug agree.
    assert_eq!(store.get(&a.id).expect("get"), a);
    assert_eq!(store.by_slug(&slug("beta")).expect("by slug"), b);
    assert_eq!(store.get(&WikiId::new()).err(), Some(StoreError::NotFound));
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
        .rename(&a.id, Some(slug("alpha-two")), Some(name("Alpha II")))
        .expect("rename");
    assert_eq!(renamed.id, a.id);
    assert_eq!(renamed.slug.as_str(), "alpha-two");
    assert_eq!(renamed.display_name.as_str(), "Alpha II");
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
    assert_eq!(
        store.rename(&WikiId::new(), None, Some(name("x"))).err(),
        Some(StoreError::NotFound)
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

    // A complete record inserted as one lands with every field.
    let mut full = WikiRecord::from_new(WikiId::new(), NewWiki::new(slug("full"), name("Full")), 3)
        .expect("record");
    full.author_key_id = Some("ed25519-cccc".into());
    full.verification = Verification {
        state: VerificationState::Verified,
        at: Some(4),
    };
    let stored = store.insert(full.clone()).expect("insert");
    assert_eq!(stored, full);
    assert_eq!(store.get(&full.id).expect("get"), full);
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
            .create(NewWiki::new(slug("persist"), name("Persist")))
            .expect("create")
            .id
    };
    let store = SqliteStore::open(&db).expect("reopen");
    let r = store.get(&id).expect("still there");
    assert_eq!(r.slug.as_str(), "persist");
    assert_eq!(store.list().expect("list").len(), 1);
}

#[test]
fn sqlite_store_refuses_a_newer_or_unreadable_schema_before_touching_it() {
    let tmp = temp("schema");
    let db = tmp.0.join("store.sqlite");
    {
        let store = SqliteStore::open(&db).expect("open");
        store
            .create(NewWiki::new(slug("one"), name("One")))
            .expect("create");
    }
    let doctor = |value: &str| {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.execute(
            "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
            [value],
        )
        .expect("doctor");
    };
    doctor("99");
    assert_eq!(
        SqliteStore::open(&db).err(),
        Some(StoreError::SchemaVersion {
            found: "99".into(),
            known: inklkunn_store::sqlite::SCHEMA_VERSION,
        })
    );
    doctor("garbage");
    assert!(matches!(
        SqliteStore::open(&db).err(),
        Some(StoreError::SchemaVersion { .. })
    ));
    // The refusal changed nothing: restore the version and the wiki is still there.
    doctor("1");
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.list().expect("list").len(), 1);
}

#[test]
fn a_stored_slug_later_reserved_stays_readable() {
    let tmp = temp("reserved");
    let db = tmp.0.join("store.sqlite");
    {
        let store = SqliteStore::open(&db).expect("open");
        store
            .create(NewWiki::new(slug("keep"), name("Keep")))
            .expect("create");
    }
    {
        // A wiki that took a name before the name was reserved.
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.execute("UPDATE wikis SET slug = 'api' WHERE slug = 'keep'", [])
            .expect("doctor");
    }
    let store = SqliteStore::open(&db).expect("reopen");
    let all = store.list().expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all.first().map(|r| r.slug.as_str()), Some("api"));
    // But no new wiki can take it, and no rename can.
    assert!(Slug::new("api").is_err());
}

#[test]
fn the_root_layout_puts_a_wiki_under_its_shard_renames_move_nothing_and_discard_removes_both() {
    let tmp = temp("root");
    let layout = RootLayout::new(&tmp.0);
    let store = MemoryStore::new();
    let w = store
        .create(NewWiki::new(slug("laid-out"), name("Laid out")))
        .expect("create");
    let repo = layout.prepare(&w.id).expect("prepare");
    assert!(repo.is_dir());
    assert!(layout.scratch_dir(&w.id).is_dir());
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
    std::fs::write(repo.join("file"), b"x").expect("write");
    layout.discard_unfinished(&w.id).expect("discard");
    assert!(!repo.exists());
    assert!(!layout.scratch_dir(&w.id).exists());
    // Discarding what is already gone is not an error.
    layout.discard_unfinished(&w.id).expect("idempotent");
}
