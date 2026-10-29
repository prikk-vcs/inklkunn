#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn a_view_is_heads_and_a_start_is_heads_or_tags() {
    for ok in [
        "heads/main",
        "heads/draft-2",
        "heads/team/alpha",
        "heads/v1.2_x",
    ] {
        assert!(is_view_ref(ok), "{ok}");
        assert!(is_start_ref(ok), "{ok}");
    }
    assert!(!is_view_ref("tags/v1") && is_start_ref("tags/v1"));
    for bad in [
        "",
        "main",
        "heads/",
        "heads//x",
        "heads/.x",
        "heads/-x",
        "heads/a b",
        "heads/a\nb",
        "heads/a/.b",
        "heads/a/",
        "remotes/heads/main",
        "HEADS/x",
        "heads/ünï",
        "--help",
    ] {
        assert!(!is_view_ref(bad), "{bad:?}");
        assert!(!is_start_ref(bad) || bad.starts_with("tags/"), "{bad:?}");
    }
    assert!(!is_view_ref(&format!("heads/{}", "a".repeat(129))));
}

#[test]
fn key_store_failures_are_told_in_the_signers_terms() {
    assert!(matches!(ks(KeyStoreError::NoKeys), SignerError::Refused(_)));
    assert!(matches!(ks(KeyStoreError::Exists), SignerError::Refused(_)));
    assert!(matches!(
        ks(KeyStoreError::Record("x")),
        SignerError::Damaged(_)
    ));
    assert!(matches!(
        ks(KeyStoreError::Locked),
        SignerError::Unavailable(_)
    ));
    assert!(matches!(
        ks(KeyStoreError::Random),
        SignerError::Unavailable(_)
    ));
    assert!(matches!(
        ks(KeyStoreError::SeedsNotRemoved),
        SignerError::SeedsNotRemoved
    ));
}

#[test]
fn a_leftover_seed_directory_is_added_to_a_failure_and_never_replaces_it() {
    for (e, payload) in [
        (AcceptError::Refused("r-payload".into()), "r-payload"),
        (AcceptError::WorktreeDirty("d-payload".into()), "d-payload"),
        (
            AcceptError::Queued {
                step: "seal",
                detail: "q-payload".into(),
                ceiling: None,
            },
            "q-payload",
        ),
        (
            AcceptError::SealedUnconfirmed {
                sealed: None,
                why: Unconfirmed::NotOurs,
                detail: "s-payload".into(),
            },
            "s-payload",
        ),
        (AcceptError::Unavailable("u-payload".into()), "u-payload"),
    ] {
        let text = note_leftover(e.clone()).to_string();
        assert!(text.contains("decrypted seeds"), "{text}");
        assert!(
            text.contains(payload),
            "the failure's own words stay: {text}"
        );
    }
    assert_eq!(
        note_leftover(AcceptError::NotPending),
        AcceptError::NotPending
    );
    assert_eq!(
        note_leftover(AcceptError::MessageMismatch),
        AcceptError::MessageMismatch
    );
}

struct NoLedger;

impl AcceptanceLedger for NoLedger {
    fn pending(
        &self,
        _: &crate::accept::AcceptanceToken,
    ) -> Result<Option<crate::accept::ExpectedMessage>, crate::accept::LedgerError> {
        Ok(None)
    }
    fn view_creation(
        &self,
        _: &crate::accept::ViewCreationAct,
    ) -> Result<Option<crate::accept::ViewCreationFacts>, crate::accept::LedgerError> {
        Ok(None)
    }
}

#[test]
fn opening_the_signer_on_a_mistyped_key_directory_is_refused_and_makes_nothing() {
    let tmp = std::env::temp_dir().join(format!("inklkunn-signer-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let secret = tmp.join("operator.secret");
    std::fs::write(&secret, "an operator secret that is long enough").unwrap();
    let keys = tmp.join("mistyped-keys");
    let config = || {
        SignerConfig::new(
            "prikk",
            Sandbox::Unconfined,
            RootLayout::new(tmp.join("wikis")),
            &keys,
            &secret,
            tmp.join("seeds"),
        )
    };
    let err = Signer::open(config(), Arc::new(NoLedger)).unwrap_err();
    assert!(matches!(err, SignerError::Unavailable(_)), "{err:?}");
    assert!(
        err.to_string().contains(&*keys.to_string_lossy()),
        "the path is not named: {err}"
    );
    assert!(!keys.exists(), "opening made a key directory");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn the_in_process_turn_is_given_up_only_after_the_file_lock_is_released() {
    // A thread waiting for its turn must find the file lock free the moment it has the turn: if the guard
    // gave the turn up first, the waiter would see its own process "busy" in the gap.
    let dir = std::env::temp_dir().join(format!("inklkunn-turn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("w.lock");
    let wiki = WikiId::new();
    let locks = WikiLocks::default();
    for round in 0..400 {
        locks.enter(&wiki);
        let Acquired::Held(file) = flock::try_exclusive(&path).unwrap() else {
            panic!("round {round}: the lock was busy for the first holder");
        };
        let guard = WriteGuard {
            locks: &locks,
            wiki: wiki.clone(),
            file: Some(file),
        };
        std::thread::scope(|s| {
            let waiter = s.spawn(|| {
                locks.enter(&wiki);
                let got = flock::try_exclusive(&path).unwrap();
                let held = matches!(got, Acquired::Held(_));
                drop(got);
                locks.leave(&wiki);
                held
            });
            // Let the waiter reach the condition variable, then release.
            std::thread::yield_now();
            drop(guard);
            assert!(
                waiter.join().unwrap(),
                "round {round}: the waiter was given the turn while the file lock was still held"
            );
        });
    }
    let _ = std::fs::remove_dir_all(&dir);
}
