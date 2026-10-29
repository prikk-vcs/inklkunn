//! A verification that could not run is kept apart from the last verdict (handoff 003, review R1): on both
//! stores, an attempt never replaces a verdict, a verdict clears a later attempt, and a database that held
//! `could-not-check` in the verdict column is brought forward honestly.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    Attempt, DisplayName, MemoryStore, NewWiki, NotRun, Slug, SqliteStore, Verification,
    VerificationState, WikiId, WikiStore,
};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> Temp {
    let p = std::env::temp_dir().join(format!("inklkunn-attempts-{}-{tag}", std::process::id()));
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

fn invariants(store: &dyn WikiStore) {
    let w = wiki(store, "attempted");
    let other = wiki(store, "bystander");
    assert_eq!(store.get(&w).unwrap().last_attempt, None);

    // An attempt on a wiki with no verdict yet is recorded, and the verdict stays unknown.
    let first = Attempt {
        at: 10,
        why: NotRun::Busy,
    };
    let r = store.record_attempt(&w, first).unwrap();
    assert_eq!(r.last_attempt, Some(first));
    assert_eq!(r.verification, Verification::default());

    // A failed verdict, then an attempt that could not run: the verdict stands untouched.
    let failed = verdict(VerificationState::Failed, 20);
    let r = store.set_verification(&w, failed).unwrap();
    assert_eq!(r.last_attempt, None, "a verdict is the latest thing now");
    for (at, why) in [
        (30, NotRun::Busy),
        (40, NotRun::Limit),
        (50, NotRun::Unavailable),
    ] {
        let a = Attempt { at, why };
        let r = store.record_attempt(&w, a).unwrap();
        assert_eq!(
            r.verification, failed,
            "{why:?} must not replace the verdict"
        );
        assert_eq!(r.last_attempt, Some(a));
        assert_eq!(store.get(&w).unwrap(), r, "and it is what is stored");
    }

    // A later verdict clears the attempt.
    let verified = verdict(VerificationState::Verified, 60);
    let r = store.set_verification(&w, verified).unwrap();
    assert_eq!((r.verification, r.last_attempt), (verified, None));

    assert_eq!(
        store.get(&other).unwrap().last_attempt,
        None,
        "another wiki is untouched"
    );
    assert!(
        store
            .record_attempt(&WikiId::new(), first)
            .is_err_and(|e| e == inklkunn_store::StoreError::NotFound)
    );
}

#[test]
fn memory_store_keeps_attempts_apart_from_verdicts() {
    invariants(&MemoryStore::new());
}

#[test]
fn sqlite_store_keeps_attempts_apart_from_verdicts() {
    let tmp = temp("sqlite");
    invariants(&SqliteStore::open(&tmp.0.join("store.sqlite")).expect("open"));
}

#[test]
fn an_attempt_survives_a_reopen() {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let a = Attempt {
        at: 7,
        why: NotRun::Limit,
    };
    let w = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "kept");
        store
            .set_verification(&w, verdict(VerificationState::Failed, 5))
            .unwrap();
        store.record_attempt(&w, a).unwrap();
        w
    };
    let r = SqliteStore::open(&db).expect("reopen").get(&w).unwrap();
    assert_eq!(r.last_attempt, Some(a));
    assert_eq!(r.verification, verdict(VerificationState::Failed, 5));
}

#[test]
fn a_version_7_database_that_held_could_not_check_says_unknown_and_keeps_the_attempt() {
    use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
    let tmp = temp("v7");
    let db = tmp.0.join("store.sqlite");
    let (checked, uncheckable) = (
        "019a0000-0000-7000-8000-0000000000c1",
        "019a0000-0000-7000-8000-0000000000c2",
    );
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 7) {
            raw.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        raw.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '7')",
            [],
        )
        .expect("version");
        for (id, slug, state) in [
            (checked, "checked", "failed"),
            (uncheckable, "uncheckable", "could-not-check"),
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
    let store = SqliteStore::open(&db).expect("a version 7 database opens");
    let kept = store.get(&WikiId::parse(checked).unwrap()).unwrap();
    assert_eq!(
        kept.verification,
        verdict(VerificationState::Failed, 1_700_000_000)
    );
    assert_eq!(kept.last_attempt, None);
    let moved = store.get(&WikiId::parse(uncheckable).unwrap()).unwrap();
    assert_eq!(
        moved.verification,
        Verification::default(),
        "no verdict is claimed"
    );
    assert_eq!(
        moved.last_attempt,
        Some(Attempt {
            at: 1_700_000_000,
            why: NotRun::Unavailable
        })
    );
    assert_eq!(SCHEMA_VERSION, 8);
}
