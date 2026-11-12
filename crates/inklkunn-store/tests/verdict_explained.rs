//! The retired keys that explain a failed verdict are stored with it (handoff 004, Q5; store schema 15, still true at 16): on both
//! stores, a verdict replaces the explanation and a passing one clears it, an attempt that could not run leaves it
//! alone, the store refuses an explanation beside a verdict that did not fail or one that is not made of key ids,
//! and a version 14 database is brought forward keeping every wiki.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    Attempt, DisplayName, MemoryStore, NewWiki, NotRun, Slug, SqliteStore, StoreError,
    Verification, VerificationState, WikiId, WikiStore,
};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> Temp {
    let p = std::env::temp_dir().join(format!("inklkunn-explained-{}-{tag}", std::process::id()));
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

fn verdict(state: VerificationState, at: i64) -> Verification {
    Verification {
        state,
        at: Some(at),
    }
}

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|k| (*k).to_owned()).collect()
}

fn invariants(store: &dyn WikiStore) {
    let w = wiki(store, "explained");
    let other = wiki(store, "bystander");
    assert!(store.get(&w).unwrap().verification_explained_by.is_empty());

    // A failure a retirement recorded: kept with the verdict, read back whole.
    let failed = verdict(VerificationState::Failed, 20);
    let r = store
        .set_verification_explained(&w, failed, &keys(&["old-key", "inklkunn-w-maintainer_2"]))
        .unwrap();
    assert_eq!(r.verification, failed);
    assert_eq!(
        r.verification_explained_by,
        ["old-key", "inklkunn-w-maintainer_2"]
    );
    assert_eq!(store.get(&w).unwrap(), r);
    // Another wiki is untouched.
    assert!(
        store
            .get(&other)
            .unwrap()
            .verification_explained_by
            .is_empty()
    );

    // An attempt that could not run leaves the verdict and its explanation as they were (R1).
    let attempt = Attempt {
        at: 30,
        why: NotRun::Limit,
    };
    let r = store.record_attempt(&w, attempt).unwrap();
    assert_eq!(r.last_attempt, Some(attempt));
    assert_eq!(r.verification, failed);
    assert_eq!(
        r.verification_explained_by,
        ["old-key", "inklkunn-w-maintainer_2"]
    );

    // The next verdict replaces it: a failure nothing explains has none, whichever way it is written.
    let again = verdict(VerificationState::Failed, 40);
    let r = store.set_verification_explained(&w, again, &[]).unwrap();
    assert!(r.verification_explained_by.is_empty());
    assert_eq!(r.last_attempt, None);
    store
        .set_verification_explained(&w, again, &keys(&["old-key"]))
        .unwrap();
    let r = store.set_verification(&w, again).unwrap();
    assert!(
        r.verification_explained_by.is_empty(),
        "a verdict written the plain way clears it"
    );

    // A passing verdict clears it too.
    store
        .set_verification_explained(&w, again, &keys(&["old-key"]))
        .unwrap();
    let r = store
        .set_verification_explained(&w, verdict(VerificationState::Verified, 50), &[])
        .unwrap();
    assert_eq!(r.verification.state, VerificationState::Verified);
    assert!(r.verification_explained_by.is_empty());

    // Refused, and nothing is changed: an explanation beside a verdict that did not fail, and what is not key ids.
    store
        .set_verification_explained(&w, again, &keys(&["old-key"]))
        .unwrap();
    let before = store.get(&w).unwrap();
    for (what, v, names) in [
        (
            "beside a pass",
            verdict(VerificationState::Verified, 60),
            keys(&["old-key"]),
        ),
        (
            "beside no verdict",
            Verification::default(),
            keys(&["old-key"]),
        ),
        ("an empty id", again, keys(&[""])),
        ("a space", again, keys(&["not a key id"])),
        ("a slash", again, keys(&["a/b"])),
        ("too long", again, vec!["k".repeat(129)]),
        (
            "too many",
            again,
            (0..65).map(|i| format!("k{i}")).collect(),
        ),
    ] {
        assert_eq!(
            store.set_verification_explained(&w, v, &names),
            Err(StoreError::StateChanged),
            "{what}"
        );
        assert_eq!(store.get(&w).unwrap(), before, "{what}");
    }
    assert_eq!(
        store.set_verification_explained(&WikiId::new(), again, &keys(&["old-key"])),
        Err(StoreError::NotFound)
    );
}

#[test]
fn the_memory_store_keeps_the_explanation_with_the_verdict() {
    invariants(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_keeps_the_explanation_with_the_verdict_across_a_reopen() {
    let tmp = temp("sqlite");
    let db = tmp.0.join("store.sqlite");
    invariants(&SqliteStore::open(&db).expect("open"));
    let store = SqliteStore::open(&db).expect("reopen");
    let w = store.by_slug(&Slug::new("explained").unwrap()).unwrap().id;
    store
        .set_verification_explained(
            &w,
            verdict(VerificationState::Failed, 70),
            &keys(&["old-key"]),
        )
        .unwrap();
    drop(store);
    let r = SqliteStore::open(&db).expect("reopen").get(&w).unwrap();
    assert_eq!(r.verification_explained_by, ["old-key"]);
}

#[test]
fn the_database_itself_refuses_an_explanation_beside_a_verdict_that_did_not_fail() {
    let tmp = temp("check");
    let db = tmp.0.join("store.sqlite");
    let w = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "checked");
        store
            .set_verification(&w, verdict(VerificationState::Verified, 5))
            .unwrap();
        w
    };
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    let refused = raw.execute(
        "UPDATE wikis SET verification_explained_by = '[\"k\"]' WHERE id = ?1",
        [w.as_str()],
    );
    assert!(refused.is_err(), "{refused:?}");
    raw.execute(
        "UPDATE wikis SET verification_state = 'failed', verification_explained_by = '[\"k\"]' WHERE id = ?1",
        [w.as_str()],
    )
    .expect("a failure may carry one");
    // ... and a failure cannot become a pass while it still carries one.
    let refused = raw.execute(
        "UPDATE wikis SET verification_state = 'verified' WHERE id = ?1",
        [w.as_str()],
    );
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_version_14_database_is_migrated_forward_keeping_every_wiki_and_only_once() {
    use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
    let tmp = temp("v14");
    let db = tmp.0.join("store.sqlite");
    let (failed, verified) = (
        "019a0000-0000-7000-8000-0000000000d1",
        "019a0000-0000-7000-8000-0000000000d2",
    );
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 14) {
            raw.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        raw.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '14')",
            [],
        )
        .expect("version");
        for (id, slug, state) in [
            (failed, "failed", "failed"),
            (verified, "verified", "verified"),
        ] {
            raw.execute(
                "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, \
                 visibility, policy_bootstrap, prikk_format_version, verification_state, verification_at, \
                 author_key_id, maintainer_key_id, created_at, seq) VALUES (?1, ?2, ?2, 'heads/main', \
                 'CC-BY-SA-4.0', '[\"en\"]', 'public', 'default', NULL, ?3, 1700000000, NULL, NULL, \
                 1700000000, 1 + (SELECT COUNT(*) FROM wikis))",
                rusqlite::params![id, slug, state],
            )
            .expect("a wiki");
        }
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
    assert_eq!(version(), "14");
    {
        let store = SqliteStore::open(&db).expect("a version 14 database opens");
        assert_eq!(version(), SCHEMA_VERSION.to_string());
        // Every wiki is there, its verdict kept, with nothing explained (the verdict was recorded without it).
        let failed = store.get(&WikiId::parse(failed).unwrap()).unwrap();
        assert_eq!(
            failed.verification,
            verdict(VerificationState::Failed, 1_700_000_000)
        );
        assert!(failed.verification_explained_by.is_empty());
        let verified = store.get(&WikiId::parse(verified).unwrap()).unwrap();
        assert_eq!(verified.verification.state, VerificationState::Verified);
        // The new column works on the old wiki.
        store
            .set_verification_explained(
                &failed.id,
                verdict(VerificationState::Failed, 1_700_000_100),
                &keys(&["old-key"]),
            )
            .unwrap();
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(version(), SCHEMA_VERSION.to_string());
    assert_eq!(
        store
            .get(&WikiId::parse(failed).unwrap())
            .unwrap()
            .verification_explained_by,
        ["old-key"]
    );
    // The migrated schema is the fresh schema: the same columns on the wikis table, in the same order.
    let columns = |path: &std::path::Path| -> Vec<(String, String)> {
        let conn = rusqlite::Connection::open(path).expect("raw open");
        let mut stmt = conn.prepare("PRAGMA table_info(wikis)").unwrap();
        stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    };
    let fresh = tmp.0.join("fresh.sqlite");
    drop(SqliteStore::open(&fresh).expect("fresh"));
    assert_eq!(columns(&db), columns(&fresh));
    assert!(
        columns(&db)
            .iter()
            .any(|(name, _)| name == "verification_explained_by")
    );
}
