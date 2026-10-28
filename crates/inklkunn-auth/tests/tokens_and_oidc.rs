//! API tokens through `authorize()`, and OpenID Connect sign-in through the authenticator (RFC 002 D-1).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::*;
use inklkunn_auth::oidc::{
    FetchError, FetchRequest, HttpsFetcher, InvalidReason, OidcConfig, OidcError, OidcProvider,
    STATE_COOKIE, TokenVerifier, VerifiedIdentity,
};
use inklkunn_auth::{AccountError, AuthError, OidcSignInError};
use inklkunn_core::Principal;
use inklkunn_core::authorize::{Action, Decision, Snapshot, decide};
use inklkunn_store::{ExternalIdentity, Role, Scope, Visibility};

#[test]
fn a_token_gets_a_principal_with_its_scope_and_authorize_never_lets_it_do_more() {
    let w = world();
    let alice = id(&w.alice);
    for scope in Scope::ALL {
        let issued = w.auth.issue_token(&alice, scope, "ci", 3_600, NOW).unwrap();
        let principal = w
            .auth
            .principal_for_token(issued.secret.as_str(), NOW + 1)
            .unwrap();
        assert_eq!(principal, Principal::token(alice.clone(), scope));
        // Every role the account could hold, every action: the token is never allowed what its scope or
        // the role forbids, and never allowed what the same account's session is not.
        let session = Principal::session(alice.clone());
        for role in Role::ALL.map(Some).into_iter().chain([None]) {
            for visibility in [
                Visibility::Public,
                Visibility::Internal,
                Visibility::Restricted,
            ] {
                for action in Action::ALL {
                    let snapshot = Snapshot {
                        visibility: Some(visibility),
                        role,
                    };
                    let by_token = decide(snapshot, &principal, action);
                    let by_session = decide(snapshot, &session, action);
                    if by_token == Decision::Allow {
                        assert_eq!(
                            by_session,
                            Decision::Allow,
                            "{scope:?} {role:?} {action:?}: more than the session"
                        );
                        assert!(
                            scope >= action.required_scope(),
                            "{scope:?} {role:?} {action:?}: a scope that does not reach the action"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_propose_token_held_by_a_maintainer_still_cannot_accept_but_the_session_can() {
    let w = world();
    let alice = id(&w.alice);
    let snapshot = Snapshot {
        visibility: Some(Visibility::Public),
        role: Some(Role::Maintainer),
    };
    let token = w
        .auth
        .issue_token(&alice, Scope::Propose, "ci", 3_600, NOW)
        .unwrap();
    let by_token = w
        .auth
        .principal_for_token(token.secret.as_str(), NOW)
        .unwrap();
    assert_eq!(
        decide(snapshot, &by_token, Action::Propose),
        Decision::Allow
    );
    assert_eq!(decide(snapshot, &by_token, Action::Accept), Decision::Deny);
    assert_eq!(decide(snapshot, &by_token, Action::Admin), Decision::Deny);
    let signed = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    assert_eq!(
        decide(snapshot, &signed.principal, Action::Accept),
        Decision::Allow
    );
}

#[test]
fn an_expired_revoked_or_made_up_token_is_the_same_refusal() {
    let w = world();
    let alice = id(&w.alice);
    let t = w
        .auth
        .issue_token(&alice, Scope::Read, "ci", 100, NOW)
        .unwrap();
    assert!(
        w.auth
            .principal_for_token(t.secret.as_str(), NOW + 99)
            .is_ok()
    );
    assert_eq!(
        w.auth.principal_for_token(t.secret.as_str(), NOW + 100),
        Err(AuthError::BadCredentials)
    );
    let u = w
        .auth
        .issue_token(&alice, Scope::Read, "ci", 100, NOW)
        .unwrap();
    assert!(w.auth.revoke_token(&alice, &u.id, NOW + 1).unwrap());
    assert_eq!(
        w.auth.principal_for_token(u.secret.as_str(), NOW + 2),
        Err(AuthError::BadCredentials)
    );
    assert_eq!(
        w.auth.principal_for_token("ink_made-up", NOW),
        Err(AuthError::BadCredentials)
    );
    // Bob cannot revoke Alice's token, and the token list shows labels and times only.
    let v = w
        .auth
        .issue_token(&alice, Scope::Read, "ci", 100, NOW)
        .unwrap();
    assert!(!w.auth.revoke_token(&id(&w.bob), &v.id, NOW + 1).unwrap());
    assert!(
        w.auth
            .principal_for_token(v.secret.as_str(), NOW + 2)
            .is_ok()
    );
    assert_eq!(w.auth.tokens(&alice).unwrap().len(), 3);
}

// ---- OpenID Connect through the authenticator ----

const ISSUER: &str = "https://idp.example";

struct Idp(Mutex<HashMap<String, Vec<u8>>>);

impl HttpsFetcher for Idp {
    fn fetch(&self, req: &FetchRequest) -> Result<Vec<u8>, FetchError> {
        self.0
            .lock()
            .unwrap()
            .get(&req.url)
            .cloned()
            .ok_or_else(|| FetchError::Transport(format!("no route for {}", req.url)))
    }
}

/// A verifier that vouches for one subject, whatever the token.
struct Vouches(&'static str);

impl TokenVerifier for Vouches {
    fn verify(&self, _token: &str, _nonce: &str, _now: i64) -> Result<VerifiedIdentity, OidcError> {
        Ok(VerifiedIdentity::for_test(
            ExternalIdentity::new(ISSUER, self.0).unwrap(),
            Some("someone@example.org".into()),
        ))
    }
}

/// A verifier that refuses everything.
struct Refuses;

impl TokenVerifier for Refuses {
    fn verify(&self, _: &str, _: &str, _: i64) -> Result<VerifiedIdentity, OidcError> {
        Err(OidcError::Invalid(InvalidReason::Signature))
    }
}

fn provider_with(verifier: Arc<dyn TokenVerifier>) -> OidcProvider {
    let idp = Arc::new(Idp(Mutex::new(HashMap::from([
        (
            "https://idp.example/.well-known/openid-configuration".to_owned(),
            format!(
                r#"{{"issuer":"{ISSUER}","authorization_endpoint":"https://idp.example/auth","token_endpoint":"https://idp.example/token","jwks_uri":"https://idp.example/jwks"}}"#
            )
            .into_bytes(),
        ),
        ("https://idp.example/token".to_owned(), br#"{"id_token":"t"}"#.to_vec()),
    ]))));
    OidcProvider::with_verifier(
        OidcConfig {
            issuer: ISSUER.into(),
            client_id: "c".into(),
            client_secret: None,
            redirect_uri: "https://wiki.example/cb".into(),
        },
        idp,
        verifier,
    )
}

fn oidc_world(subject: &'static str) -> World {
    world_with_oidc(provider_with(Arc::new(Vouches(subject))))
}

/// A complete sign-in as a browser does it: start, then come back with the state in the query and in the
/// cookie.
fn sign_in_through_the_provider(
    w: &World,
    client: &str,
    now: i64,
) -> Result<inklkunn_auth::SignedIn, OidcSignInError> {
    let begun = w.auth.begin_oidc(&ip(client), now)?;
    assert!(
        begun
            .authorization_url
            .starts_with("https://idp.example/auth?")
    );
    let cookie = begun.state_cookie();
    let header = cookie.split(';').next().unwrap().to_owned();
    assert!(header.starts_with(STATE_COOKIE), "{header}");
    w.auth
        .finish_oidc(&begun.state, Some(&header), "code", now + 1)
}

#[test]
fn a_linked_identity_signs_in_through_the_whole_flow_and_gets_the_accounts_session() {
    let w = oidc_world("subject-1");
    let sso = w.auth.register_external("Sso Person", NOW).unwrap();
    w.auth
        .link_external(
            &sso.id,
            &ExternalIdentity::new(ISSUER, "subject-1").unwrap(),
            NOW,
        )
        .unwrap();
    let signed = sign_in_through_the_provider(&w, "203.0.113.1", NOW).unwrap();
    assert_eq!(signed.principal, Principal::session(sso.id.clone()));
    let found = w
        .auth
        .principal_for_session(signed.session.secret().as_str(), NOW + 3)
        .unwrap();
    assert_eq!(found.unwrap().account, sso.id);
}

#[test]
fn an_identity_nobody_linked_signs_nobody_in_even_with_a_matching_email() {
    let w = oidc_world("unlinked-subject");
    // An account whose display name looks like the e-mail the provider reports changes nothing: only the
    // (issuer, subject) pair links.
    w.auth
        .register_external("someone@example.org", NOW)
        .unwrap();
    assert_eq!(
        sign_in_through_the_provider(&w, "203.0.113.1", NOW).unwrap_err(),
        OidcSignInError::Auth(AuthError::BadCredentials)
    );
}

#[test]
fn a_disabled_accounts_identity_signs_nobody_in() {
    let w = oidc_world("subject-2");
    let sso = w.auth.register_external("Sso Person", NOW).unwrap();
    w.auth
        .link_external(
            &sso.id,
            &ExternalIdentity::new(ISSUER, "subject-2").unwrap(),
            NOW,
        )
        .unwrap();
    w.auth.disable(&sso.id, NOW).unwrap();
    assert_eq!(
        sign_in_through_the_provider(&w, "203.0.113.1", NOW).unwrap_err(),
        OidcSignInError::Auth(AuthError::BadCredentials)
    );
}

#[test]
fn a_callback_this_browser_did_not_start_or_one_used_twice_ends_before_any_account_is_looked_at() {
    let w = oidc_world("subject-3");
    let sso = w.auth.register_external("Sso Person", NOW).unwrap();
    w.auth
        .link_external(
            &sso.id,
            &ExternalIdentity::new(ISSUER, "subject-3").unwrap(),
            NOW,
        )
        .unwrap();
    let begun = w.auth.begin_oidc(&ip("203.0.113.1"), NOW).unwrap();
    // No cookie, someone else's cookie: the login-CSRF check.
    assert_eq!(
        w.auth
            .finish_oidc(&begun.state, None, "code", NOW + 1)
            .unwrap_err(),
        OidcSignInError::Oidc(OidcError::State)
    );
    let other = format!("{STATE_COOKIE}=not-this-state");
    assert_eq!(
        w.auth
            .finish_oidc(&begun.state, Some(&other), "code", NOW + 1)
            .unwrap_err(),
        OidcSignInError::Oidc(OidcError::State)
    );
    // The real browser finishes once; the same callback again finds nothing.
    let header = format!("{STATE_COOKIE}={}", begun.state);
    assert!(
        w.auth
            .finish_oidc(&begun.state, Some(&header), "code", NOW + 2)
            .is_ok()
    );
    assert_eq!(
        w.auth
            .finish_oidc(&begun.state, Some(&header), "code", NOW + 3)
            .unwrap_err(),
        OidcSignInError::Oidc(OidcError::State)
    );
}

#[test]
fn a_token_the_verifier_refuses_ends_the_sign_in_with_the_reason_for_the_log() {
    let w = world_with_oidc(provider_with(Arc::new(Refuses)));
    let err = sign_in_through_the_provider(&w, "203.0.113.1", NOW).unwrap_err();
    assert_eq!(
        err,
        OidcSignInError::Oidc(OidcError::Invalid(InvalidReason::Signature))
    );
}

#[test]
fn starting_a_sign_in_spends_the_clients_budget_so_logins_in_flight_cannot_be_flooded() {
    let w = oidc_world("subject-4");
    let flooder = ip("198.51.100.9");
    for i in 0..30 {
        assert!(w.auth.begin_oidc(&flooder, NOW + i).is_ok(), "{i}");
    }
    assert!(matches!(
        w.auth.begin_oidc(&flooder, NOW + 40),
        Err(OidcSignInError::Auth(AuthError::Locked { .. }))
    ));
    // Another address is unaffected.
    assert!(w.auth.begin_oidc(&ip("198.51.100.10"), NOW + 41).is_ok());
}

#[test]
fn without_a_provider_there_is_no_openid_connect() {
    let w = world();
    assert!(matches!(
        w.auth.begin_oidc(&ip("203.0.113.1"), NOW),
        Err(OidcSignInError::Oidc(OidcError::NotConfigured))
    ));
    assert!(matches!(
        w.auth.finish_oidc("s", None, "c", NOW),
        Err(OidcSignInError::Oidc(OidcError::NotConfigured))
    ));
}

#[test]
fn one_identity_links_to_one_account_only() {
    let w = world();
    let identity = ExternalIdentity::new(ISSUER, "subject-5").unwrap();
    w.auth.link_external(&id(&w.alice), &identity, NOW).unwrap();
    assert_eq!(
        w.auth
            .link_external(&id(&w.bob), &identity, NOW)
            .unwrap_err(),
        AccountError::IdentityTaken
    );
    assert!(
        w.auth.link_external(&id(&w.alice), &identity, NOW).is_ok(),
        "again for the same account is fine"
    );
}

#[test]
fn nothing_about_a_sign_in_in_flight_prints_a_secret() {
    let w = oidc_world("subject-6");
    let begun = w.auth.begin_oidc(&ip("203.0.113.1"), NOW).unwrap();
    let shown = format!("{begun:?}");
    assert!(
        !shown.contains(&begun.state) && !shown.contains("auth?"),
        "{shown}"
    );
    let request = FetchRequest {
        url: "https://idp.example/token".into(),
        form: Some("grant_type=authorization_code&code=THE-CODE&code_verifier=THE-VERIFIER".into()),
        authorization: Some("Basic Y2xpZW50OnNlY3JldA==".into()),
    };
    let shown = format!("{request:?}");
    assert!(shown.contains("https://idp.example/token"));
    for secret in ["THE-CODE", "THE-VERIFIER", "Y2xpZW50OnNlY3JldA"] {
        assert!(!shown.contains(secret), "{shown}");
    }
    let verified = VerifiedIdentity::for_test(
        ExternalIdentity::new(ISSUER, "u").unwrap(),
        Some("private@example.org".into()),
    );
    assert!(!format!("{verified:?}").contains("private@example.org"));
}
