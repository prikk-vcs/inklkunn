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
