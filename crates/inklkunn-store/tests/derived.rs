//! The derived cache on both stores (handoff 003 T3; RFC 003 D-2; DM-6, DM-7): the same invariants, what
//! only SQLite can show (reopen, a damaged row, the schema's own shape), and the migrations to version 5.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    DerivedHeading, DerivedLinks, DerivedMeta, DerivedPage, DerivedStore, DisplayName, IndexEntry,
    MemoryStore, NewWiki, PageKey, Slug, SqliteStore, StoreError, WikiId, WikiStore,
};

const TIP_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TIP_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TIP_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn key<'a>(wiki: &'a WikiId, tip: &'a str, path: &'a str) -> PageKey<'a> {
    PageKey {
        wiki,
        tip,
        path,
        lang: "en",
        presentation: "/w",
    }
}

fn page(html: &str) -> DerivedPage {
    DerivedPage {
        html: html.to_owned(),
        meta: DerivedMeta {
            title: "Home".into(),
            id: Some("019a0000-0000-7000-8000-000000000001".into()),
            kind: "page".into(),
            status: "stable".into(),
            lang: Some("en".into()),
            license: None,
            description: Some("A <b>description</b> is data".into()),
            tags: vec!["a".into(), "日本語".into()],
            aliases: vec!["Start".into()],
        },
        links: DerivedLinks {
            resolved: vec![("Glossary".into(), "pages/Glossary.md".into())],
            dangling: vec!["Nowhere".into()],
            ambiguous: vec![(
                "Ab".into(),
                vec!["pages/A/Ab.md".into(), "pages/B/Ab.md".into()],
            )],
            interwiki: vec!["wp:Wiki".into()],
            transcluded: vec!["pages/Included.md".into()],
        },
        toc: vec![DerivedHeading {
            level: 2,
            id: "intro".into(),
            text: "Intro".into(),
        }],
        computed_at: 1_700_000_000,
    }
}

fn entry(path: &str, title: &str) -> IndexEntry {
    IndexEntry {
        path: path.to_owned(),
        title: title.to_owned(),
        aliases: Vec::new(),
        redirect: None,
        page_id: None,
        kind: "page".into(),
        problem: None,
    }
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

fn invariants(store: &(impl DerivedStore + WikiStore)) {
    let w1 = wiki(store, "derived-one");
    let w2 = wiki(store, "derived-two");

    // Nothing is cached until it is.
    assert_eq!(store.page(&key(&w1, TIP_A, "pages/Home.md")).unwrap(), None);
    assert_eq!(store.title_index(&w1, TIP_A).unwrap(), None);

    // A page comes back exactly as it went in, every field.
    let home = page("<p>Hello</p>");
    store
        .put_page(&key(&w1, TIP_A, "pages/Home.md"), &home)
        .unwrap();
    assert_eq!(
        store.page(&key(&w1, TIP_A, "pages/Home.md")).unwrap(),
        Some(home.clone())
    );

    // The key is all five parts: a different wiki, tip, path, language or presentation is a different page.
    let others = [
        key(&w2, TIP_A, "pages/Home.md"),
        key(&w1, TIP_B, "pages/Home.md"),
        key(&w1, TIP_A, "pages/Other.md"),
        PageKey {
            lang: "ja",
            ..key(&w1, TIP_A, "pages/Home.md")
        },
        PageKey {
            presentation: "/renamed",
            ..key(&w1, TIP_A, "pages/Home.md")
        },
    ];
    for k in &others {
        assert_eq!(store.page(k).unwrap(), None, "{k:?}");
    }

    // A new value replaces the old one under the same key and only there.
    let changed = page("<p>Changed</p>");
    store
        .put_page(&key(&w1, TIP_A, "pages/Home.md"), &changed)
        .unwrap();
    assert_eq!(
        store
            .page(&key(&w1, TIP_A, "pages/Home.md"))
            .unwrap()
            .map(|p| p.html),
        Some("<p>Changed</p>".to_owned())
    );

    // A title index is stored whole, read back by path, and replaced whole.
    let entries = vec![
        entry("pages/Zebra.md", "Zebra"),
        IndexEntry {
            aliases: vec!["Start".into(), "Front page".into()],
            page_id: Some("019a0000-0000-7000-8000-000000000001".into()),
            ..entry("pages/Home.md", "Home")
        },
        IndexEntry {
            redirect: Some("Home".into()),
            kind: "redirect".into(),
            ..entry("pages/Old.md", "Old")
        },
        IndexEntry {
            problem: Some("frontmatter".into()),
            ..entry("pages/Broken.md", "Broken")
        },
    ];
    store
        .put_title_index(&w1, TIP_A, &entries, 1_700_000_001)
        .unwrap();
    let back = store.title_index(&w1, TIP_A).unwrap().expect("stored");
    let mut expected = entries.clone();
    expected.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(back, expected, "by path, every field");
    assert_eq!(
        store.title_index(&w2, TIP_A).unwrap(),
        None,
        "another wiki has none"
    );
    assert_eq!(store.title_index(&w1, TIP_B).unwrap(), None);
    store
        .put_title_index(&w1, TIP_A, entries.get(..1).expect("four"), 1_700_000_002)
        .unwrap();
    assert_eq!(
        store.title_index(&w1, TIP_A).unwrap(),
        Some(vec![entry("pages/Zebra.md", "Zebra")]),
        "replaced whole, nothing of the old index left"
    );
    // An empty index is an index (a wiki with no pages is not a wiki whose index is unknown).
    store.put_title_index(&w1, TIP_B, &[], 3).unwrap();
    assert_eq!(store.title_index(&w1, TIP_B).unwrap(), Some(Vec::new()));

    // Retaining tips drops the others, for this wiki only, and counts what went (a page once, an index once).
    store
        .put_page(&key(&w1, TIP_B, "pages/Home.md"), &home)
        .unwrap();
    store
        .put_page(&key(&w1, TIP_C, "pages/Home.md"), &home)
        .unwrap();
    store.put_title_index(&w1, TIP_C, &entries, 4).unwrap();
    store
        .put_page(&key(&w2, TIP_A, "pages/Home.md"), &home)
        .unwrap();
    // w1 holds: pages at A, B, C; indexes at A, B, C.
    let removed = store.retain_tips(&w1, &[TIP_B.to_owned()]).unwrap();
    assert_eq!(removed, 4, "the page and the index at A and at C");
    assert!(
        store
            .page(&key(&w1, TIP_A, "pages/Home.md"))
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .page(&key(&w1, TIP_B, "pages/Home.md"))
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .page(&key(&w1, TIP_C, "pages/Home.md"))
            .unwrap()
            .is_none()
    );
    assert!(store.title_index(&w1, TIP_A).unwrap().is_none());
    assert!(store.title_index(&w1, TIP_B).unwrap().is_some());
    assert!(store.title_index(&w1, TIP_C).unwrap().is_none());
    assert!(
        store
            .page(&key(&w2, TIP_A, "pages/Home.md"))
            .unwrap()
            .is_some(),
        "the other wiki is untouched"
    );
    assert_eq!(store.retain_tips(&w1, &[TIP_B.to_owned()]).unwrap(), 0);

    // Clearing a wiki drops all of it and nothing of another.
    assert_eq!(store.clear(&w1).unwrap(), 2, "the page and the index at B");
    assert!(
        store
            .page(&key(&w1, TIP_B, "pages/Home.md"))
            .unwrap()
            .is_none()
    );
    assert!(store.title_index(&w1, TIP_B).unwrap().is_none());
    assert!(
        store
            .page(&key(&w2, TIP_A, "pages/Home.md"))
            .unwrap()
            .is_some()
    );
    assert_eq!(store.clear(&w1).unwrap(), 0);
    assert_eq!(store.clear(&w2).unwrap(), 1);
}

#[test]
fn both_stores_hold_the_derived_cache_invariants() {
    invariants(&MemoryStore::new());
    invariants(&SqliteStore::open_in_memory().expect("open"));
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-store-d-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

#[test]
fn the_derived_cache_persists_across_reopen() {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let (w, home) = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persisted");
        let home = page("<p>Kept</p>");
        store
            .put_page(&key(&w, TIP_A, "pages/Home.md"), &home)
            .unwrap();
        store
            .put_title_index(&w, TIP_A, &[entry("pages/Home.md", "Home")], 5)
            .unwrap();
        (w, home)
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(
        store.page(&key(&w, TIP_A, "pages/Home.md")).unwrap(),
        Some(home)
    );
    assert_eq!(
        store.title_index(&w, TIP_A).unwrap(),
        Some(vec![entry("pages/Home.md", "Home")])
    );
}

#[test]
fn a_damaged_derived_row_is_a_miss_never_an_error_and_never_a_guess() {
    let tmp = temp("damaged");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "damaged");
    store
        .put_page(&key(&w, TIP_A, "pages/Home.md"), &page("<p>x</p>"))
        .unwrap();
    store
        .put_title_index(
            &w,
            TIP_A,
            &[entry("pages/A.md", "A"), entry("pages/B.md", "B")],
            1,
        )
        .unwrap();
    let raw = rusqlite::Connection::open(&db).expect("raw open");

    // A page whose metadata is no longer JSON is derived again (a miss); putting it again repairs it.
    raw.execute("UPDATE derived_pages SET meta = 'not json'", [])
        .unwrap();
    assert_eq!(store.page(&key(&w, TIP_A, "pages/Home.md")).unwrap(), None);
    store
        .put_page(&key(&w, TIP_A, "pages/Home.md"), &page("<p>y</p>"))
        .unwrap();
    assert!(
        store
            .page(&key(&w, TIP_A, "pages/Home.md"))
            .unwrap()
            .is_some()
    );

    // An index with an entry whose aliases are no longer JSON is a miss as a whole.
    raw.execute(
        "UPDATE title_index_entries SET aliases = 'oops' WHERE path = 'pages/B.md'",
        [],
    )
    .unwrap();
    assert_eq!(store.title_index(&w, TIP_A).unwrap(), None);

    // An index that lost an entry (the table was edited) is not whole, so it is a miss too.
    store
        .put_title_index(
            &w,
            TIP_A,
            &[entry("pages/A.md", "A"), entry("pages/B.md", "B")],
            2,
        )
        .unwrap();
    assert!(store.title_index(&w, TIP_A).unwrap().is_some());
    raw.execute(
        "DELETE FROM title_index_entries WHERE path = 'pages/B.md'",
        [],
    )
    .unwrap();
    assert_eq!(store.title_index(&w, TIP_A).unwrap(), None);
}

#[test]
fn the_derived_tables_have_no_column_for_a_verification_result() {
    // INV-5 and RD-4: a page served from the cache cannot carry a badge, because the cache cannot hold one.
    let tmp = temp("columns");
    let db = tmp.0.join("store.sqlite");
    drop(SqliteStore::open(&db).expect("open"));
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    for table in ["derived_pages", "title_index_tips", "title_index_entries"] {
        let mut stmt = raw
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .expect("table info");
        let columns: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .expect("columns")
            .collect::<Result<_, _>>()
            .expect("names");
        assert!(!columns.is_empty(), "{table} exists");
        for c in &columns {
            let lower = c.to_lowercase();
            for word in ["verif", "assur", "badge", "verdict", "integrity"] {
                assert!(
                    !lower.contains(word),
                    "{table}.{c}: the derived cache holds no verification result"
                );
            }
        }
    }
}

/// The schema as a build one version earlier wrote it (version 4: every migration but the last), with a
/// wiki, a role grant, an audit event and an account in it.
fn make_version_4(db: &std::path::Path) -> String {
    use inklkunn_store::sqlite::MIGRATIONS;
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 4) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '4')",
        [],
    )
    .expect("version");
    let id = "019a0000-0000-7000-8000-0000000000aa";
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'four', 'Four', 'heads/main', 'CC-BY-SA-4.0', \
         '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, 'ed25519-aaaa', 'ed25519-bbbb', \
         1700000000, 1)",
        [id],
    )
    .expect("a wiki");
    raw.execute(
        "INSERT INTO audit_log (at, principal, action, resource, outcome, detail) \
         VALUES (1, 'anonymous', 'export', 'wiki:four', 'denied', '')",
        [],
    )
    .expect("an event");
    id.to_owned()
}

#[test]
fn a_version_4_database_gains_the_derived_cache_and_the_checkpoint_fields_and_keeps_everything() {
    use inklkunn_store::sqlite::SCHEMA_VERSION;
    use inklkunn_store::{AuditLog, PrikkVersion, VerificationState};
    let tmp = temp("v4");
    let db = tmp.0.join("store.sqlite");
    let id = make_version_4(&db);
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
    assert_eq!(version(), "4");
    {
        let store = SqliteStore::open(&db).expect("a version 4 database opens");
        let w = WikiId::parse(&id).expect("id");
        // The wiki survived, with an empty checkpoint.
        let record = store.get(&w).expect("the wiki");
        assert_eq!(record.slug.as_str(), "four");
        assert_eq!(record.verification.state, VerificationState::Verified);
        assert_eq!(record.author_key_id.as_deref(), Some("ed25519-aaaa"));
        assert_eq!(record.checkpoint.refreshed_with, None);
        assert_eq!(record.checkpoint.refresh_requested_at, None);
        // The audit trail survived.
        assert_eq!(store.read(0, 10).expect("audit").len(), 1);
        // The new tables and columns work on the old wiki.
        store
            .put_page(&key(&w, TIP_A, "pages/Home.md"), &page("<p>new</p>"))
            .unwrap();
        store
            .put_title_index(&w, TIP_A, &[entry("pages/Home.md", "Home")], 9)
            .unwrap();
        assert!(store.title_index(&w, TIP_A).unwrap().is_some());
        let asked = store.request_checkpoint_refresh(&w, 77).expect("request");
        assert_eq!(asked.checkpoint.refresh_requested_at, Some(77));
        let prikk = PrikkVersion::parse("prikk 0.54.1").expect("version text");
        let refreshed = store.record_checkpoint_refresh(&w, &prikk).expect("record");
        assert_eq!(refreshed.checkpoint.refreshed_with, Some(prikk));
        assert_eq!(refreshed.checkpoint.refresh_requested_at, None);
    }
    assert_eq!(version(), SCHEMA_VERSION.to_string());
    assert_eq!(SCHEMA_VERSION, 5);
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    let w = WikiId::parse(&id).expect("id");
    assert_eq!(
        store
            .get(&w)
            .expect("the wiki")
            .checkpoint
            .refreshed_with
            .map(|v| v.to_string()),
        Some("prikk 0.54.1".to_owned())
    );
    assert!(store.title_index(&w, TIP_A).unwrap().is_some());
    assert_eq!(version(), "5");
}

#[test]
fn a_database_one_version_ahead_is_refused_at_open_and_left_as_it_was() {
    use inklkunn_store::sqlite::SCHEMA_VERSION;
    let tmp = temp("ahead");
    let db = tmp.0.join("store.sqlite");
    {
        let store = SqliteStore::open(&db).expect("open");
        wiki(&store, "ahead");
    }
    let doctor = |value: &str| {
        rusqlite::Connection::open(&db)
            .expect("raw open")
            .execute(
                "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
                [value],
            )
            .expect("doctor");
    };
    // What a build with a schema 6 would leave behind.
    doctor(&(SCHEMA_VERSION + 1).to_string());
    assert_eq!(
        SqliteStore::open(&db).err(),
        Some(StoreError::SchemaVersion {
            found: (SCHEMA_VERSION + 1).to_string(),
            known: SCHEMA_VERSION,
        })
    );
    // The refusal changed nothing: with the version back, the wiki is there.
    doctor(&SCHEMA_VERSION.to_string());
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.list().expect("list").len(), 1);
}
