//! The record of a view's creation (RFC 005 part 2 D-9; handoff 005 K-T8, K3): written before the signer is asked,
//! ended once, listed oldest first, the name held by the first act that stands and freed by one that failed. The
//! suite is written against the trait, so the memory store and the SQLite store (schema 16) run the same one; what
//! only a database can refuse (a raw connection, the migration from 15) is in the tests below it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::{
    AccountId, AccountStore, ActStore, MemoryStore, SqliteStore, StoreError, ViewActEnd, ViewActId,
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

    // --- The record names its view in the form a view's name has.
    let long = "a".repeat(65);
    for bad in [
        "", "Draft", "-draft", ".draft", "dra ft", "dra/ft", "a..b", "draft.", "draft:x", "é",
        &long,
    ] {
        assert!(
            matches!(
                store.begin_view_act(&act(&w.id, &by, bad, ViewKind::Experimental, 1)),
                Err(StoreError::Backend(_))
            ),
            "{bad:?} is not a name"
        );
    }
    let longest = "a".repeat(64);
    for good in ["a", "0", "draft-2", "a.b_c-d", &longest] {
        store
            .begin_view_act(&act(&other.id, &by, good, ViewKind::Experimental, 1))
            .unwrap_or_else(|e| panic!("{good:?}: {e:?}"));
    }
    assert_eq!(
        store.view_acts(&other.id).unwrap().len(),
        5,
        "the refused ones left nothing"
    );

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
fn the_sqlite_store_keeps_the_same_record_of_a_views_creation() {
    the_suite(&SqliteStore::open_in_memory().unwrap());
}

/// A store that keeps none: the older methods of the trait answer, the view-act ones are the trait's defaults (what a
/// stub is, and what a service must never take a first step over).
struct KeepsNone(MemoryStore);

type R<T> = inklkunn_store::Result<T>;

impl ActStore for KeepsNone {
    fn recovery_flag(&self, w: &WikiId) -> R<Option<inklkunn_store::RecoveryFlag>> {
        self.0.recovery_flag(w)
    }
    fn set_recovery_flag(&self, f: &inklkunn_store::RecoveryFlag) -> R<bool> {
        self.0.set_recovery_flag(f)
    }
    fn clear_recovery_flag(&self, w: &WikiId) -> R<bool> {
        self.0.clear_recovery_flag(w)
    }
    fn record_overlap(&self, o: &inklkunn_store::OverlapRecord) -> R<()> {
        self.0.record_overlap(o)
    }
    fn overlaps_of(&self, p: &inklkunn_store::ProposalId) -> R<Vec<inklkunn_store::OverlapRecord>> {
        self.0.overlaps_of(p)
    }
    fn begin_system_act(&self, a: &inklkunn_store::SystemActRecord) -> R<()> {
        self.0.begin_system_act(a)
    }
    fn finish_system_act(&self, e: &inklkunn_store::SystemActEnd) -> R<()> {
        self.0.finish_system_act(e)
    }
    fn open_system_act(
        &self,
        i: &inklkunn_store::record::SystemActId,
    ) -> R<Option<inklkunn_store::SystemActRecord>> {
        self.0.open_system_act(i)
    }
    fn system_acts(&self, w: &WikiId) -> R<Vec<inklkunn_store::SystemActEntry>> {
        self.0.system_acts(w)
    }
    fn insert_wiki_with_creation_act(
        &self,
        r: inklkunn_store::WikiRecord,
        b: &inklkunn_store::SystemActRecord,
        e: &inklkunn_store::SystemActEnd,
    ) -> R<inklkunn_store::WikiRecord> {
        self.0.insert_wiki_with_creation_act(r, b, e)
    }
    fn begin_recovery_act(&self, a: &inklkunn_store::RecoveryActRecord) -> R<()> {
        self.0.begin_recovery_act(a)
    }
    fn finish_recovery_act(&self, e: &inklkunn_store::RecoveryActEnd) -> R<()> {
        self.0.finish_recovery_act(e)
    }
    fn open_recovery_act(
        &self,
        i: &inklkunn_store::record::RecoveryActId,
    ) -> R<Option<inklkunn_store::RecoveryActRecord>> {
        self.0.open_recovery_act(i)
    }
    fn recovery_acts(&self, w: &WikiId) -> R<Vec<inklkunn_store::RecoveryActEntry>> {
        self.0.recovery_acts(w)
    }
}

#[test]
fn a_store_that_keeps_none_says_so_refuses_to_begin_one_and_lists_none() {
    // Neither of this crate's stores is one any more; a store that implements only the older methods answers with
    // the trait's defaults, so a service over it never takes a first step it cannot record.
    let store = KeepsNone(MemoryStore::new());
    let w = wiki(&store.0, "views");
    let by = account(&store.0);
    assert!(!store.keeps_view_acts());
    let a = act(&w.id, &by, "draft", ViewKind::Experimental, 1);
    assert!(matches!(
        store.begin_view_act(&a),
        Err(StoreError::Backend(_))
    ));
    assert!(matches!(
        store.finish_view_act(&ended(&a, ViewActOutcome::Created, 2)),
        Err(StoreError::Backend(_))
    ));
    assert_eq!(store.open_view_act(&a.id).unwrap(), None);
    assert!(store.view_acts(&w.id).unwrap().is_empty());
}

#[test]
fn both_of_this_crates_stores_keep_the_record() {
    assert!(MemoryStore::new().keeps_view_acts());
    assert!(SqliteStore::open_in_memory().unwrap().keeps_view_acts());
}

// ------------------------------------------------------------------------------------ the database itself

/// A database file with a wiki, an account and one open act, and a raw connection to it with foreign keys on.
fn database(tag: &str) -> (support::TempDir, rusqlite::Connection, WikiId, AccountId) {
    let tmp = support::temp(tag);
    let path = tmp.0.join("store.sqlite");
    let (w, by) = {
        let store = SqliteStore::open(&path).unwrap();
        let w = wiki(&store, "views");
        let by = account(&store);
        store
            .begin_view_act(&act(&w.id, &by, "draft", ViewKind::Experimental, 10))
            .unwrap();
        (w.id, by)
    };
    let raw = rusqlite::Connection::open(&path).expect("raw open");
    raw.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    (tmp, raw, w, by)
}

fn raw_act(
    raw: &rusqlite::Connection,
    wiki: &WikiId,
    by: &AccountId,
    id: &str,
    name: &str,
) -> rusqlite::Result<usize> {
    raw.execute(
        "INSERT INTO view_acts (id, seq, wiki_id, name, forked_from, from_block, kind, created_by, created_at) \
         VALUES (?1, 99, ?2, ?3, 'main', ?4, 'experimental', ?5, 1)",
        rusqlite::params![id, wiki.as_str(), name, "e".repeat(64), by.as_str()],
    )
}

#[test]
fn the_database_refuses_what_the_store_would_not_write() {
    let (_tmp, raw, w, by) = database("raw");
    let fresh = ViewActId::new();
    let fresh = fresh.as_str();

    // A name held by an act that stands is held against any connection (the trigger), and a bad form is a CHECK.
    let held = raw_act(&raw, &w, &by, fresh, "draft")
        .unwrap_err()
        .to_string();
    assert!(held.contains("held by an act that stands"), "{held}");
    let long = "a".repeat(65);
    for bad in [
        "", "Draft", "-draft", ".draft", "dra ft", "a..b", "draft.", "é", &long,
    ] {
        let refused = raw_act(&raw, &w, &by, fresh, bad).unwrap_err().to_string();
        assert!(refused.contains("CHECK constraint"), "{bad:?}: {refused}");
    }
    // The wiki is a reference.
    assert!(raw_act(&raw, &WikiId::new(), &by, fresh, "x").is_err());
    // An id is used once, and an act is never changed or removed.
    let existing: String = raw
        .query_row("SELECT id FROM view_acts", [], |r| r.get(0))
        .unwrap();
    assert!(raw_act(&raw, &w, &by, &existing, "other").is_err());
    for sql in [
        "UPDATE view_acts SET name = 'changed'",
        "DELETE FROM view_acts",
    ] {
        let refused = raw.execute(sql, []).unwrap_err().to_string();
        assert!(refused.contains("append-only"), "{sql}: {refused}");
    }

    // An end names an act that is there; a created view has no word and a failed one has one.
    let end = |act: &str, outcome: &str, word: Option<&str>| {
        raw.execute(
            "INSERT INTO view_act_ends (act_id, outcome, word, at) VALUES (?1, ?2, ?3, 5)",
            rusqlite::params![act, outcome, word],
        )
    };
    assert!(end(fresh, "created", None).is_err(), "no such act");
    assert!(end(&existing, "created", Some("refused")).is_err());
    assert!(end(&existing, "failed", None).is_err());
    assert!(end(&existing, "sealed", None).is_err());
    end(&existing, "failed", Some("refused")).unwrap();
    assert!(
        end(&existing, "created", None).is_err(),
        "an end is written once"
    );
    for sql in [
        "UPDATE view_act_ends SET outcome = 'created', word = NULL",
        "DELETE FROM view_act_ends",
    ] {
        let refused = raw.execute(sql, []).unwrap_err().to_string();
        assert!(refused.contains("append-only"), "{sql}: {refused}");
    }

    // The act that failed frees its name, for the database as for the store.
    raw_act(&raw, &w, &by, fresh, "draft").unwrap();
    let held = raw_act(&raw, &w, &by, ViewActId::new().as_str(), "draft")
        .unwrap_err()
        .to_string();
    assert!(held.contains("held by an act that stands"), "{held}");
}

#[test]
fn a_version_15_database_is_migrated_to_16_keeping_every_wiki_and_only_once() {
    use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
    let tmp = support::temp("v15");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000f1";
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
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
            "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, \
             visibility, policy_bootstrap, prikk_format_version, verification_state, verification_at, \
             author_key_id, maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'old', 'heads/main', \
             'CC-BY-SA-4.0', '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, \
             1700000000, 1)",
            [wiki_id],
        )
        .expect("a wiki");
    }
    let version = || -> String {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version")
    };
    assert_eq!(version(), "15");
    let w = WikiId::parse(wiki_id).unwrap();
    {
        let store = SqliteStore::open(&db).expect("a version 15 database opens");
        assert_eq!(version(), SCHEMA_VERSION.to_string());
        assert!(
            SCHEMA_VERSION >= 16,
            "the view-act tables came with version 16"
        );
        // The wiki is there, with nothing recorded of any view; the new tables work on it.
        assert_eq!(store.get(&w).unwrap().slug.as_str(), "old");
        assert!(store.keeps_view_acts());
        assert!(store.view_acts(&w).unwrap().is_empty());
        let by = account(&store);
        store
            .begin_view_act(&act(&w, &by, "draft", ViewKind::Interpretation, 3))
            .unwrap();
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(version(), SCHEMA_VERSION.to_string());
    assert_eq!(store.view_acts(&w).unwrap().len(), 1);
    // The migrated schema is the fresh schema: the same tables, indexes and triggers, and the same SQL.
    let names = |path: &std::path::Path| -> Vec<(String, String)> {
        let conn = rusqlite::Connection::open(path).expect("raw open");
        let mut stmt = conn
            .prepare(
                "SELECT name, COALESCE(sql, '') FROM sqlite_master WHERE type IN ('table', 'trigger', 'index') \
                 AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'search_fts%' ORDER BY name",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    };
    let fresh = tmp.0.join("fresh.sqlite");
    drop(SqliteStore::open(&fresh).expect("fresh"));
    let migrated = names(&db);
    assert_eq!(migrated, names(&fresh));
    for table in ["view_acts", "view_act_ends"] {
        assert!(migrated.iter().any(|(n, _)| n == table), "{table}");
    }
    assert!(migrated.iter().any(|(n, _)| n == "view_acts_name"));
}
