//! The search index and the observed times on both stores (handoff 003 T6; RFC 003 D-7, D-8; IQ-2): the same
//! invariants on memory and SQLite, FTS5 with the trigram tokenizer in the bundled SQLite, the substring
//! behaviour a Japanese title needs, and the migration from version 6.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    DerivedStore, DisplayName, MemoryStore, NewWiki, SearchDoc, SearchKind, Slug, SqliteStore,
    WikiId, WikiStore,
};

const TIP_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TIP_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Temp(PathBuf);

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> Temp {
    let p = std::env::temp_dir().join(format!("inklkunn-search-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    Temp(p)
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

fn doc(path: &str, kind: SearchKind, text: &str, folded: &str) -> SearchDoc {
    SearchDoc {
        path: path.to_owned(),
        kind,
        text: text.to_owned(),
        folded: folded.to_owned(),
    }
}

fn corpus() -> Vec<SearchDoc> {
    vec![
        doc("pages/Home.md", SearchKind::Title, "Home", "home"),
        doc(
            "pages/Guide.md",
            SearchKind::Title,
            "Getting Started",
            "getting started",
        ),
        doc("pages/Guide.md", SearchKind::Alias, "How-to", "how-to"),
        doc(
            "pages/Jp.md",
            SearchKind::Title,
            "日本語のウィキ入門",
            "日本語のウィキ入門",
        ),
        doc(
            "pages/Pct.md",
            SearchKind::Title,
            "100% done_ok",
            "100% done_ok",
        ),
        doc(
            "pages/Quote.md",
            SearchKind::Title,
            "Say \"hi\"",
            "say \"hi\"",
        ),
    ]
}

fn paths(hits: &[inklkunn_store::SearchHit]) -> Vec<&str> {
    hits.iter().map(|h| h.path.as_str()).collect()
}

fn invariants(store: &(impl DerivedStore + WikiStore)) {
    let w1 = wiki(store, "search-one");
    let w2 = wiki(store, "search-two");

    // No index is "not built", which is not "no hits".
    assert_eq!(store.search(&w1, TIP_A, "home", 10).unwrap(), None);
    store.put_search_index(&w1, TIP_A, &corpus(), 5).unwrap();
    assert_eq!(
        paths(&store.search(&w1, TIP_A, "home", 10).unwrap().unwrap()),
        ["pages/Home.md"]
    );
    // Another tip and another wiki have their own (none).
    assert_eq!(store.search(&w1, TIP_B, "home", 10).unwrap(), None);
    assert_eq!(store.search(&w2, TIP_A, "home", 10).unwrap(), None);

    // A substring of a text finds it, at any position, whatever the script.
    let hit = |q: &str| -> Vec<String> {
        store
            .search(&w1, TIP_A, q, 10)
            .unwrap()
            .unwrap()
            .iter()
            .map(|h| format!("{}:{}", h.path, h.kind.code()))
            .collect()
    };
    assert_eq!(hit("start"), ["pages/Guide.md:title"]);
    assert_eq!(hit("how-to"), ["pages/Guide.md:alias"]);
    assert_eq!(hit("ウィキ"), ["pages/Jp.md:title"]);
    assert_eq!(hit("本語の"), ["pages/Jp.md:title"]);
    // One and two characters are a scan, not a failure.
    assert_eq!(hit("ウィ"), ["pages/Jp.md:title"]);
    assert_eq!(hit("日"), ["pages/Jp.md:title"]);
    // The query's own wildcards and quotes mean themselves.
    assert_eq!(hit("100%"), ["pages/Pct.md:title"]);
    assert_eq!(hit("_ok"), ["pages/Pct.md:title"]);
    assert_eq!(hit("%"), ["pages/Pct.md:title"]);
    assert_eq!(hit("say \"hi\""), ["pages/Quote.md:title"]);
    assert_eq!(hit("\""), ["pages/Quote.md:title"]);
    assert_eq!(hit("NEAR(a b)"), Vec::<String>::new());
    assert_eq!(hit("zzz"), Vec::<String>::new());

    // The limit holds.
    assert_eq!(store.search(&w1, TIP_A, "o", 2).unwrap().unwrap().len(), 2);

    // A replacement is whole: the old texts are gone.
    store
        .put_search_index(
            &w1,
            TIP_A,
            &[doc("pages/New.md", SearchKind::Title, "New", "new")],
            6,
        )
        .unwrap();
    assert_eq!(hit("home"), Vec::<String>::new());
    assert_eq!(hit("new"), ["pages/New.md:title"]);

    // The cache keeps the tips it is told to keep, and `clear` drops all of a wiki's.
    store.put_search_index(&w1, TIP_B, &corpus(), 7).unwrap();
    store.put_search_index(&w2, TIP_A, &corpus(), 7).unwrap();
    assert_eq!(store.retain_tips(&w1, &[TIP_B.to_owned()]).unwrap(), 1);
    assert_eq!(store.search(&w1, TIP_A, "new", 10).unwrap(), None);
    assert!(store.search(&w1, TIP_B, "home", 10).unwrap().is_some());
    assert!(store.search(&w2, TIP_A, "home", 10).unwrap().is_some());
    assert_eq!(store.clear(&w1).unwrap(), 1);
    assert_eq!(store.search(&w1, TIP_B, "home", 10).unwrap(), None);
    assert!(store.search(&w2, TIP_A, "home", 10).unwrap().is_some());
}

fn observations(store: &(impl DerivedStore + WikiStore)) {
    let w = wiki(store, "seen-one");
    let other = wiki(store, "seen-two");
    let blocks = vec!["b1".to_owned(), "b2".to_owned()];
    assert_eq!(store.first_seen(&w, &blocks, 100).unwrap(), [100, 100]);
    // The first time is kept, and a new block gets the new time, in the order asked.
    let more = vec!["b3".to_owned(), "b1".to_owned()];
    assert_eq!(store.first_seen(&w, &more, 200).unwrap(), [200, 100]);
    // Another wiki's blocks are its own.
    assert_eq!(
        store.first_seen(&other, &["b1".to_owned()], 300).unwrap(),
        [300]
    );
    // Clearing the cache, and keeping only some tips, leave the observations alone: they cannot be rebuilt.
    store.clear(&w).unwrap();
    store.retain_tips(&w, &[]).unwrap();
    assert_eq!(store.first_seen(&w, &blocks, 999).unwrap(), [100, 100]);
    assert_eq!(store.first_seen(&w, &[], 1).unwrap(), Vec::<i64>::new());
}

#[test]
fn the_memory_store_keeps_the_search_invariants() {
    invariants(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_keeps_the_search_invariants() {
    let tmp = temp("sqlite");
    invariants(&SqliteStore::open(&tmp.0.join("store.sqlite")).expect("open"));
}

#[test]
fn both_stores_keep_the_observed_times() {
    observations(&MemoryStore::new());
    let tmp = temp("seen");
    observations(&SqliteStore::open(&tmp.0.join("store.sqlite")).expect("open"));
}

#[test]
fn observed_times_and_the_search_index_survive_a_reopen() {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let id = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "reopen");
        store.put_search_index(&w, TIP_A, &corpus(), 1).unwrap();
        store.first_seen(&w, &["b1".to_owned()], 42).unwrap();
        w
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.first_seen(&id, &["b1".to_owned()], 99).unwrap(), [42]);
    assert_eq!(
        paths(&store.search(&id, TIP_A, "ウィキ", 5).unwrap().unwrap()),
        ["pages/Jp.md"]
    );
}

/// IQ-2: the bundled SQLite has FTS5 and its trigram tokenizer (this is the check the decision rests on).
#[test]
fn the_bundled_sqlite_has_fts5_with_the_trigram_tokenizer() {
    let conn = rusqlite::Connection::open_in_memory().expect("open");
    conn.execute_batch(
        "CREATE VIRTUAL TABLE t USING fts5(x, tokenize = 'trigram');
         INSERT INTO t (x) VALUES ('日本語のウィキ入門'), ('Getting Started');",
    )
    .expect("FTS5 and the trigram tokenizer are compiled in");
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM t WHERE t MATCH '\"ウィキ\"'",
            [],
            |r| r.get(0),
        )
        .expect("match");
    assert_eq!(n, 1);
}

/// The schema as version 6 left it: every migration but the last, with a wiki in it.
#[test]
fn a_version_6_database_gains_the_search_index_and_the_observed_times_and_keeps_everything() {
    use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
    let tmp = temp("v6");
    let db = tmp.0.join("store.sqlite");
    let id = "019a0000-0000-7000-8000-0000000000bb";
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 6) {
            raw.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        raw.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '6')",
            [],
        )
        .expect("version");
        raw.execute(
            "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
             policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
             maintainer_key_id, created_at, seq) VALUES (?1, 'six', 'Six', 'heads/main', 'CC-BY-SA-4.0', \
             '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, 'ed25519-aaaa', 'ed25519-bbbb', \
             1700000000, 1)",
            [id],
        )
        .expect("a wiki");
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
    assert_eq!(version(), "6");
    {
        let store = SqliteStore::open(&db).expect("a version 6 database opens");
        let w = WikiId::parse(id).expect("id");
        assert_eq!(store.get(&w).expect("the wiki").slug.as_str(), "six");
        store.put_search_index(&w, TIP_A, &corpus(), 1).unwrap();
        assert!(store.search(&w, TIP_A, "guide", 5).unwrap().is_some());
        assert_eq!(store.first_seen(&w, &["b".to_owned()], 5).unwrap(), [5]);
    }
    assert_eq!(version(), SCHEMA_VERSION.to_string());
    assert_eq!(SCHEMA_VERSION, 7);
    // Opening again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    let w = WikiId::parse(id).expect("id");
    assert!(store.search(&w, TIP_A, "guide", 5).unwrap().is_some());
}
