//! API tokens through `authorize()`, and OpenID Connect sign-in through the authenticator (RFC 002 D-1).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::*;
use inklkunn_auth::AuthError;
use inklkunn_auth::oidc::{
    FetchError, FetchRequest, HttpsFetcher, OidcConfig, OidcError, OidcProvider, TokenVerifier,
    VerifiedIdentity,
};
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

/// A verifier that vouches for one subject, whatever the token, but only for the nonce it is given.
struct Vouches(&'static str);

impl TokenVerifier for Vouches {
    fn verify(&self, _token: &str, _nonce: &str, _now: i64) -> Result<VerifiedIdentity, OidcError> {
        Ok(VerifiedIdentity::for_test(
            ExternalIdentity::new(ISSUER, self.0).unwrap(),
            Some("someone@example.org".into()),
        ))
    }
}

fn provider(subject: &'static str) -> OidcProvider {
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
        Arc::new(Vouches(subject)),
    )
}

fn complete(p: &OidcProvider) -> VerifiedIdentity {
    let b = p.begin_login(NOW).unwrap();
    p.finish_login(&b.state, &b.state, "code", NOW + 1).unwrap()
}

#[test]
fn a_linked_identity_signs_in_and_gets_the_accounts_session() {
    let w = world();
    let sso = w.auth.register_external("Sso Person", NOW).unwrap();
    w.auth
        .link_external(
            &sso.id,
            &ExternalIdentity::new(ISSUER, "subject-1").unwrap(),
            NOW,
        )
        .unwrap();
    let verified = complete(&provider("subject-1"));
    assert_eq!(verified.email_hint(), Some("someone@example.org"));
    let signed = w.auth.sign_in_oidc(&verified, NOW + 2).unwrap();
    assert_eq!(signed.principal, Principal::session(sso.id.clone()));
    let found = w
        .auth
        .principal_for_session(signed.session.secret().as_str(), NOW + 3)
        .unwrap();
    assert_eq!(found.unwrap().account, sso.id);
}

#[test]
fn an_identity_nobody_linked_signs_nobody_in_even_with_a_matching_email() {
    let w = world();
    // An account whose display name looks like the e-mail the provider reports changes nothing: only the
    // (issuer, subject) pair links.
    w.auth
        .register_external("someone@example.org", NOW)
        .unwrap();
    let verified = complete(&provider("unlinked-subject"));
    assert_eq!(
        w.auth.sign_in_oidc(&verified, NOW + 2).unwrap_err(),
        AuthError::BadCredentials
    );
}

#[test]
fn a_disabled_accounts_identity_signs_nobody_in() {
    let w = world();
    let sso = w.auth.register_external("Sso Person", NOW).unwrap();
    w.auth
        .link_external(
            &sso.id,
            &ExternalIdentity::new(ISSUER, "subject-2").unwrap(),
            NOW,
        )
        .unwrap();
    w.auth.disable(&sso.id).unwrap();
    let verified = complete(&provider("subject-2"));
    assert_eq!(
        w.auth.sign_in_oidc(&verified, NOW + 2).unwrap_err(),
        AuthError::BadCredentials
    );
}

#[test]
fn one_identity_links_to_one_account_only() {
    use inklkunn_auth::authenticator::AccountError;
    let w = world();
    let identity = ExternalIdentity::new(ISSUER, "subject-3").unwrap();
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
