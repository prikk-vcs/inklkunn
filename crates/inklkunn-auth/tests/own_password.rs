//! A person's own password: asked again for what outlives a session (corrections G6), and changed from the
//! account page without the operator (G10).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::atomic::Ordering;

use common::*;
use inklkunn_auth::{AccountError, AuthError, ConfirmError, FRESH_SESSION_SECS, SessionCheck};
use inklkunn_store::Scope;

const NEW: &str = "a different password altogether";

#[test]
fn a_sensitive_act_asks_for_the_password_again_and_counts_the_guess() {
    let w = world();
    let s = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let secret = s.session.secret().as_str().to_owned();
    let c = ip("203.0.113.2");
    assert_eq!(
        w.auth
            .confirm_for_sensitive_act(&id(&w.alice), &secret, ALICE_PASSWORD, &c, NOW + 1),
        Ok(())
    );
    for wrong in ["", "wrong", "another long password"] {
        assert_eq!(
            w.auth
                .confirm_for_sensitive_act(&id(&w.alice), &secret, wrong, &c, NOW + 2),
            Err(ConfirmError::Auth(AuthError::BadCredentials)),
            "{wrong:?}"
        );
    }
    // The account's name is locked after five failures, as at the sign-in page: the guess is not free here.
    for i in 0..5 {
        let _ = w.auth.confirm_for_sensitive_act(
            &id(&w.alice),
            &secret,
            "nope nope nope",
            &ip(&format!("203.0.113.{}", 30 + i)),
            NOW + 3,
        );
    }
    assert!(matches!(
        w.auth
            .confirm_for_sensitive_act(&id(&w.alice), &secret, ALICE_PASSWORD, &c, NOW + 4),
        Err(ConfirmError::Auth(AuthError::Locked { .. }))
    ));
    assert!(matches!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.77"), NOW + 5),
        Err(AuthError::Locked { .. })
    ));
}

#[test]
fn an_account_with_no_password_confirms_with_a_session_of_the_last_ten_minutes() {
    let w = world();
    let sso = w.auth.register_external("Sso Person", NOW).unwrap();
    assert!(!w.auth.has_password(&sso.id).unwrap());
    assert!(w.auth.has_password(&id(&w.alice)).unwrap());
    // A session started now (what a provider sign-in makes).
    let started = w.auth.start_session_for_test(&sso.id, NOW);
    let c = ip("203.0.113.3");
    assert_eq!(
        w.auth
            .confirm_for_sensitive_act(&sso.id, &started, "", &c, NOW + FRESH_SESSION_SECS),
        Ok(())
    );
    assert_eq!(
        w.auth
            .confirm_for_sensitive_act(&sso.id, &started, "", &c, NOW + FRESH_SESSION_SECS + 1),
        Err(ConfirmError::SignInAgain)
    );
    // A secret that names no session of the account is not fresh either.
    assert_eq!(
        w.auth
            .confirm_for_sensitive_act(&sso.id, "ink_unknown", "", &c, NOW),
        Err(ConfirmError::SignInAgain)
    );
}

#[test]
fn changing_ones_own_password_ends_every_session_and_every_token_and_starts_a_new_session() {
    let w = world();
    let c = ip("203.0.113.4");
    let here = w.auth.sign_in("alice", ALICE_PASSWORD, &c, NOW).unwrap();
    let there = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &c, NOW + 1)
        .unwrap();
    let token = w
        .auth
        .issue_token(&id(&w.alice), Scope::Read, "t", 3600, NOW)
        .unwrap();
    let here_secret = here.session.secret().as_str().to_owned();
    let new = w
        .auth
        .change_own_password(&id(&w.alice), ALICE_PASSWORD, NEW, &c, NOW + 2)
        .unwrap();
    // The session that made the change is over too (a leaked copy of its cookie dies with the password), as
    // is the other; the one the change returns is alive, with a token of its own; the API token is revoked.
    let live = |secret: &str| {
        w.auth
            .principal_for_session(secret, NOW + 3)
            .unwrap()
            .is_some()
    };
    assert!(!live(&here_secret));
    assert!(!live(there.session.secret().as_str()));
    assert!(live(new.secret().as_str()));
    assert_ne!(new.secret().as_str(), here_secret);
    assert_ne!(new.csrf().as_str(), here.session.csrf().as_str());
    assert_eq!(
        w.auth
            .principal_for_token(token.secret.as_str(), NOW + 3)
            .err(),
        Some(AuthError::BadCredentials)
    );
    // The old password is over, the new one works.
    assert!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &c, NOW + 4)
            .is_err()
    );
    assert!(w.auth.sign_in("alice", NEW, &c, NOW + 5).is_ok());
    // The new session writes with its own CSRF token, and the ended one's token opens nothing.
    assert!(matches!(
        w.auth
            .principal_for_write(new.secret().as_str(), new.csrf().as_str(), NOW + 6)
            .unwrap(),
        SessionCheck::Valid(_)
    ));
    assert!(!matches!(
        w.auth
            .principal_for_write(&here_secret, here.session.csrf().as_str(), NOW + 6)
            .unwrap(),
        SessionCheck::Valid(_)
    ));
    // The cookie it sets is the session's: `__Host-`, `Secure`, `HttpOnly`, `SameSite=Strict`.
    let cookie = new.set_cookie(NOW + 2);
    assert!(
        cookie.starts_with("__Host-inklkunn-session=")
            && cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Strict"),
        "{cookie}"
    );
}

#[test]
fn a_wrong_current_password_changes_nothing_and_is_judged_before_the_new_one() {
    let w = world();
    let c = ip("203.0.113.5");
    w.auth.sign_in("alice", ALICE_PASSWORD, &c, NOW).unwrap();
    let there = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &c, NOW + 1)
        .unwrap();
    // Wrong current password, and a new one that would be refused: the first is what is said.
    assert_eq!(
        w.auth
            .change_own_password(&id(&w.alice), "wrong", "short", &c, NOW + 2)
            .err(),
        Some(AccountError::Auth(AuthError::BadCredentials))
    );
    // Right current password, a new one that is refused: the sessions are left alone.
    assert!(matches!(
        w.auth
            .change_own_password(&id(&w.alice), ALICE_PASSWORD, "short", &c, NOW + 3)
            .err(),
        Some(AccountError::Password(_))
    ));
    assert!(
        w.auth
            .principal_for_session(there.session.secret().as_str(), NOW + 4)
            .unwrap()
            .is_some()
    );
    assert!(w.auth.sign_in("alice", ALICE_PASSWORD, &c, NOW + 5).is_ok());
}

#[test]
fn the_current_password_is_one_verification_and_counts_against_the_throttle_like_a_sign_in() {
    let w = world();
    let c = ip("203.0.113.6");
    w.auth.sign_in("alice", ALICE_PASSWORD, &c, NOW).unwrap();
    let before = w.hasher.verifies.load(Ordering::SeqCst);
    let _ = w
        .auth
        .change_own_password(&id(&w.alice), "wrong", NEW, &c, NOW + 1);
    assert_eq!(
        w.hasher.verifies.load(Ordering::SeqCst),
        before + 1,
        "one verification, and the new password was not hashed"
    );
    for i in 0..5 {
        let _ = w.auth.change_own_password(
            &id(&w.alice),
            "wrong wrong wrong",
            NEW,
            &ip(&format!("203.0.113.{}", 40 + i)),
            NOW + 2,
        );
    }
    assert!(matches!(
        w.auth
            .change_own_password(&id(&w.alice), ALICE_PASSWORD, NEW, &c, NOW + 3)
            .err(),
        Some(AccountError::Auth(AuthError::Locked { .. }))
    ));
}

#[test]
fn an_account_with_no_password_has_none_to_change() {
    let w = world();
    let sso = w.auth.register_external("Sso Person", NOW).unwrap();
    assert_eq!(
        w.auth
            .change_own_password(&sso.id, "", NEW, &ip("203.0.113.7"), NOW)
            .err(),
        Some(AccountError::Auth(AuthError::BadCredentials))
    );
}
