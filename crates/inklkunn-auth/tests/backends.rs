//! The authenticator over the SQLite store, and with the production password cost (the paths a deployment
//! takes; the other tests use the in-memory store and a cheap hash).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::*;
use inklkunn_auth::{AuthConfig, AuthError, Authenticator};
use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_store::{ExternalIdentity, Scope, SqliteStore};

#[test]
fn the_whole_story_works_over_sqlite() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let auth = Authenticator::with_hasher(store.clone(), config(), Counting::new()).unwrap();
    let alice = auth
        .register_local("alice", "Alice", ALICE_PASSWORD, NOW)
        .unwrap();
    assert!(matches!(
        auth.register_local("Alice", "Another", "long enough password", NOW),
        Err(inklkunn_auth::authenticator::AccountError::LoginTaken)
    ));

    let signed = auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    assert_eq!(
        signed.principal,
        Principal::session(&IdentityAuthority::for_tests(), alice.id.clone())
    );
    let secret = signed.session.secret().as_str();
    assert!(
        auth.principal_for_session(secret, NOW + 1)
            .unwrap()
            .is_some()
    );

    let token = auth
        .issue_token(&alice.id, Scope::Review, "ci", 3_600, NOW)
        .unwrap();
    assert_eq!(
        auth.principal_for_token(token.secret.as_str(), NOW + 1)
            .unwrap(),
        Principal::token(
            &IdentityAuthority::for_tests(),
            alice.id.clone(),
            Scope::Review
        )
    );
    assert!(auth.revoke_token(&alice.id, &token.id, NOW + 2).unwrap());
    assert_eq!(
        auth.principal_for_token(token.secret.as_str(), NOW + 3),
        Err(AuthError::BadCredentials)
    );

    let identity = ExternalIdentity::new("https://idp.example", "u1").unwrap();
    auth.link_external(&alice.id, &identity, NOW).unwrap();
    auth.reset_password(&alice.id, "alice", "a brand new password", NOW + 3)
        .unwrap();
    assert!(
        auth.principal_for_session(secret, NOW + 4)
            .unwrap()
            .is_none(),
        "sessions ended"
    );
    assert!(
        auth.sign_in("alice", "a brand new password", &ip("203.0.113.2"), NOW + 5)
            .is_ok()
    );
}

#[test]
fn the_production_cost_signs_in_and_refuses_for_real() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let auth =
        Authenticator::new(store, AuthConfig::default(), IdentityAuthority::for_tests()).unwrap();
    auth.register_local("alice", "Alice", ALICE_PASSWORD, NOW)
        .unwrap();
    assert!(
        auth.sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
            .is_ok()
    );
    assert!(matches!(
        auth.sign_in("alice", "not the password at all", &ip("203.0.113.2"), NOW),
        Err(AuthError::BadCredentials)
    ));
    assert!(matches!(
        auth.sign_in("nobody", "not the password at all", &ip("203.0.113.3"), NOW),
        Err(AuthError::BadCredentials)
    ));
}
