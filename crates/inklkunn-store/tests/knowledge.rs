//! The derived knowledge index (RFC 005 part 2 D-6; handoff K-T4; schema version 13) on both stores: the same
//! invariants, what only SQLite can show (reopen, a damaged table, the shape of the tables), and a version 12
//! database migrated.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
use inklkunn_store::{
    DerivedStore, KnowledgeAnchorRow, KnowledgeClaimRow, KnowledgeInvalidRow, KnowledgeLinkRow,
    KnowledgeRelationRow, KnowledgeRows, KnowledgeSourceRow, MemoryStore, SqliteStore, WikiStore,
};
use support::{temp, wiki};

const TIP_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TIP_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn rows(tag: &str) -> KnowledgeRows {
    KnowledgeRows {
        claims: vec![
            KnowledgeClaimRow {
                id: format!("c-{tag}-2"),
                kind: "observation".into(),
                lifecycle: "adopted".into(),
                standing: "contested".into(),
                standing_proposal: Some("p-1".into()),
                standing_judgement: "earlier".into(),
            },
            KnowledgeClaimRow {
                id: format!("c-{tag}-1"),
                kind: "unknown".into(),
                lifecycle: "unknown".into(),
                standing: "unknown".into(),
                standing_proposal: None,
                standing_judgement: "none".into(),
            },
        ],
        sources: vec![KnowledgeSourceRow {
            id: format!("s-{tag}-1"),
            kind: "primary".into(),
            availability: "unavailable".into(),
            last_reviewed: Some("2026-10-31".into()),
            superseded_by: None,
            title: Some("日本語の題名".into()),
            author: Some("The Harbour Authority".into()),
            date: Some("Spring 1950".into()),
            reference: Some("https://example.org/survey".into()),
        }],
        relations: vec![KnowledgeRelationRow {
            claim: format!("c-{tag}-2"),
            kind: "refines".into(),
            other: format!("c-{tag}-1"),
        }],
        links: vec![KnowledgeLinkRow {
            claim: format!("c-{tag}-2"),
            source: format!("s-{tag}-1"),
            relation: "supports".into(),
            bearing: "partial".into(),
            judgement: "current".into(),
        }],
        anchors: vec![
            KnowledgeAnchorRow {
                claim: format!("c-{tag}-2"),
                page_id: "日本語-id".into(),
                block: "b1".into(),
                page_path: Some("pages/日本語.md".into()),
                state: "present".into(),
                digest: Some("d".repeat(64)),
            },
            KnowledgeAnchorRow {
                claim: format!("c-{tag}-1"),
                page_id: "gone".into(),
                block: "b2".into(),
                page_path: None,
                state: "page-missing".into(),
                digest: None,
            },
        ],
        invalid: vec![KnowledgeInvalidRow {
            path: "knowledge/claims/x.yaml".into(),
            rule: "path".into(),
        }],
    }
}

fn invariants(store: &(impl DerivedStore + WikiStore)) {
    let w = wiki(store, "kn-one").id;
    let other = wiki(store, "kn-two").id;

    // Nothing stored: a miss, not an empty index.
    assert_eq!(store.knowledge_index(&w, TIP_A).unwrap(), None);

    // Stored whole, read back in key order, per wiki and per tip.
    let a = rows("a");
    store.put_knowledge_index(&w, TIP_A, &a, 5).unwrap();
    assert_eq!(
        store.knowledge_index(&w, TIP_A).unwrap(),
        Some(a.clone().sorted())
    );
    assert_eq!(store.knowledge_index(&w, TIP_B).unwrap(), None);
    assert_eq!(store.knowledge_index(&other, TIP_A).unwrap(), None);

    // An empty index is an index: a tip with no knowledge files is a fact.
    store
        .put_knowledge_index(&w, TIP_B, &KnowledgeRows::default(), 6)
        .unwrap();
    assert_eq!(
        store.knowledge_index(&w, TIP_B).unwrap(),
        Some(KnowledgeRows::default())
    );

    // Put again: replaced, not merged.
    let b = rows("b");
    store.put_knowledge_index(&w, TIP_A, &b, 7).unwrap();
    assert_eq!(store.knowledge_index(&w, TIP_A).unwrap(), Some(b.sorted()));

    // The past is dropped with the rest of it, and counted once per index.
    store
        .put_knowledge_index(&other, TIP_A, &rows("o"), 8)
        .unwrap();
    assert_eq!(store.retain_tips(&w, &[TIP_A.to_owned()]).unwrap(), 1);
    assert!(store.knowledge_index(&w, TIP_A).unwrap().is_some());
    assert_eq!(store.knowledge_index(&w, TIP_B).unwrap(), None);
    assert!(store.knowledge_index(&other, TIP_A).unwrap().is_some());

    // Cleared: only that wiki's.
    assert_eq!(store.clear(&w).unwrap(), 1);
    assert_eq!(store.knowledge_index(&w, TIP_A).unwrap(), None);
    assert!(store.knowledge_index(&other, TIP_A).unwrap().is_some());
}

#[test]
fn both_stores_hold_the_knowledge_index_invariants() {
    invariants(&MemoryStore::new());
    invariants(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_knowledge_index_persists_across_reopen() {
    let tmp = temp("kn-reopen");
    let db = tmp.0.join("store.sqlite");
    let w = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "kept").id;
        store.put_knowledge_index(&w, TIP_A, &rows("k"), 1).unwrap();
        w
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(
        store.knowledge_index(&w, TIP_A).unwrap(),
        Some(rows("k").sorted())
    );
}

#[test]
fn an_altered_knowledge_table_makes_the_index_a_miss_never_a_guess() {
    let tmp = temp("kn-damaged");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "damaged").id;
    store.put_knowledge_index(&w, TIP_A, &rows("d"), 1).unwrap();
    assert!(store.knowledge_index(&w, TIP_A).unwrap().is_some());
    let raw = rusqlite::Connection::open(&db).expect("raw");
    raw.execute("DELETE FROM knowledge_links", [])
        .expect("alter");
    assert_eq!(
        store.knowledge_index(&w, TIP_A).unwrap(),
        None,
        "the count no longer agrees: the index is not whole"
    );
    // Put again, and it is whole.
    store.put_knowledge_index(&w, TIP_A, &rows("d"), 2).unwrap();
    assert!(store.knowledge_index(&w, TIP_A).unwrap().is_some());
}

#[test]
fn the_knowledge_tables_have_no_numeric_column_but_the_count_and_no_score_or_verdict() {
    let store = SqliteStore::open_in_memory().expect("open");
    drop(store);
    let tmp = temp("kn-shape");
    let db = tmp.0.join("store.sqlite");
    drop(SqliteStore::open(&db).expect("open"));
    let raw = rusqlite::Connection::open(&db).expect("raw");
    let tables = [
        "knowledge_claims",
        "knowledge_sources",
        "knowledge_relations",
        "knowledge_links",
        "knowledge_anchors",
        "knowledge_invalid",
        "knowledge_tips",
    ];
    for table in tables {
        let mut stmt = raw
            .prepare(&format!(
                "SELECT name, type FROM pragma_table_info('{table}')"
            ))
            .unwrap();
        let columns: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!columns.is_empty(), "{table}");
        for (name, ty) in columns {
            let numeric = !ty.eq_ignore_ascii_case("TEXT");
            let allowed =
                table == "knowledge_tips" && matches!(name.as_str(), "row_count" | "built_at");
            assert!(!numeric || allowed, "{table}.{name} is {ty}");
            for forbidden in [
                "score",
                "rank",
                "truth",
                "weight",
                "verdict",
                "verified",
                "confidence",
            ] {
                assert!(!name.contains(forbidden), "{table}.{name}");
            }
        }
    }
}

fn make_version_12(db: &std::path::Path, wiki_id: &str) {
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 12) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '12')",
        [],
    )
    .expect("version");
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'Old', 'heads/main', 'CC-BY-SA-4.0', \
         '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, 1700000000, 1)",
        [wiki_id],
    )
    .expect("a wiki");
    raw.execute(
        "INSERT INTO title_index_entries (wiki_id, tip, path, title, aliases, redirect, page_id, kind, problem) \
         VALUES (?1, ?2, 'pages/Home.md', 'Home', '[]', NULL, NULL, 'page', NULL)",
        [wiki_id, TIP_A],
    )
    .expect("a title index row");
    raw.execute(
        "INSERT INTO title_index_tips (wiki_id, tip, entries, built_at) VALUES (?1, ?2, 1, 5)",
        [wiki_id, TIP_A],
    )
    .expect("a title index tip");
}

#[test]
fn a_version_12_database_is_migrated_to_13_keeping_everything_and_only_once() {
    let tmp = temp("kn-v12");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000f2";
    make_version_12(&db, wiki_id);
    let version = || -> String {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version")
    };
    assert_eq!(version(), "12");
    {
        let store = SqliteStore::open(&db).expect("a version 12 database opens");
        assert_eq!(version(), SCHEMA_VERSION.to_string());
        let w = store.list().unwrap().remove(0);
        // What was there is still there.
        assert_eq!(
            store.title_index(&w.id, TIP_A).unwrap().map(|e| e.len()),
            Some(1)
        );
        // There is no knowledge index yet (a miss, built on demand), and the new tables work on the old wiki.
        assert_eq!(store.knowledge_index(&w.id, TIP_A).unwrap(), None);
        store
            .put_knowledge_index(&w.id, TIP_A, &rows("m"), 9)
            .unwrap();
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    let w = store.list().unwrap().remove(0);
    assert_eq!(
        store.knowledge_index(&w.id, TIP_A).unwrap(),
        Some(rows("m").sorted())
    );
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
    assert!(names(&db).contains(&"knowledge_links".to_owned()));
}

/// A database as version 15 left it, with a knowledge index of one tip stored whole, in the shape that version
/// gave a source (no name).
fn make_version_15_with_an_index(db: &std::path::Path, wiki_id: &str) {
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 15) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '15')",
        [],
    )
    .expect("version");
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'Old', 'heads/main', 'CC-BY-SA-4.0', \
         '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, 1700000000, 1)",
        [wiki_id],
    )
    .expect("a wiki");
    raw.execute(
        "INSERT INTO title_index_entries (wiki_id, tip, path, title, aliases, redirect, page_id, kind, problem) \
         VALUES (?1, ?2, 'pages/Home.md', 'Home', '[]', NULL, NULL, 'page', NULL)",
        [wiki_id, TIP_A],
    )
    .expect("a title index row");
    raw.execute(
        "INSERT INTO title_index_tips (wiki_id, tip, entries, built_at) VALUES (?1, ?2, 1, 5)",
        [wiki_id, TIP_A],
    )
    .expect("a title index tip");
    raw.execute(
        "INSERT INTO knowledge_sources (wiki_id, tip, id, kind, availability, last_reviewed, superseded_by) \
         VALUES (?1, ?2, 's-old', 'primary', 'available', NULL, NULL)",
        [wiki_id, TIP_A],
    )
    .expect("a source of the old shape");
    raw.execute(
        "INSERT INTO knowledge_tips (wiki_id, tip, row_count, built_at) VALUES (?1, ?2, 1, 5)",
        [wiki_id, TIP_A],
    )
    .expect("a knowledge index tip");
}

#[test]
fn a_version_15_database_is_migrated_with_its_knowledge_index_dropped_so_that_it_is_rebuilt() {
    let tmp = temp("kn-v15");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000f5";
    make_version_15_with_an_index(&db, wiki_id);
    // As the old version left it, the index reads as whole.
    {
        let raw = rusqlite::Connection::open(&db).expect("raw");
        let tips: i64 = raw
            .query_row("SELECT COUNT(*) FROM knowledge_tips", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tips, 1);
    }
    let store = SqliteStore::open(&db).expect("a version 15 database opens");
    let w = store.list().unwrap().remove(0);
    // The index of the old shape is not read: a miss, which the read service answers by building it again.
    assert_eq!(store.knowledge_index(&w.id, TIP_A).unwrap(), None);
    // Nothing of it is left in any table, and what is not derived is.
    let raw = rusqlite::Connection::open(&db).expect("raw");
    for table in [
        "knowledge_tips",
        "knowledge_claims",
        "knowledge_sources",
        "knowledge_relations",
        "knowledge_links",
        "knowledge_anchors",
        "knowledge_invalid",
    ] {
        let n: i64 = raw
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    assert_eq!(
        store.title_index(&w.id, TIP_A).unwrap().map(|e| e.len()),
        Some(1),
        "the title index is not touched"
    );
    // The index of the new shape is stored and read back, names and all.
    store
        .put_knowledge_index(&w.id, TIP_A, &rows("n"), 9)
        .unwrap();
    let back = store.knowledge_index(&w.id, TIP_A).unwrap().expect("whole");
    let named = back.sources.first().expect("a source");
    assert_eq!(named.title.as_deref(), Some("日本語の題名"));
    assert_eq!(named.author.as_deref(), Some("The Harbour Authority"));
    assert_eq!(named.date.as_deref(), Some("Spring 1950"));
    assert_eq!(
        named.reference.as_deref(),
        Some("https://example.org/survey")
    );
}
