//! How an account comes into being through the provider (the OpenID Connect handoff's U3; RFC 002 D-1; the
//! owner's ruling DR-11): a first sign-in where the join setting lets it, and an invitation redeemed with the
//! provider instead of a password. Never matched or linked by e-mail address: one person, one account, and an
//! account is reached by the provider's `(issuer, subject)` and by nothing else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::*;
use inklkunn_auth::invitation::DEFAULT_LIFETIME_SECS;
use inklkunn_auth::oidc::{
    FetchError, FetchRequest, HttpsFetcher, OidcConfig, OidcError, OidcProvider, STATE_COOKIE,
    TokenVerifier, VerifiedIdentity,
};
use inklkunn_auth::{
    AuthConfig, AuthError, JoinPolicy, OidcPurpose, OidcSignInError, OidcSignedIn,
};
use inklkunn_store::{
    AccountStore, DisplayName, ExternalIdentity, NewWiki, Role, RoleStore, Slug, WikiId, WikiStore,
};

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

/// A verifier that vouches for whoever the test says is at the provider now, with an e-mail address that
/// matches nothing here.
struct Vouches(Arc<Mutex<&'static str>>);

impl TokenVerifier for Vouches {
    fn verify(&self, _token: &str, _nonce: &str, _now: i64) -> Result<VerifiedIdentity, OidcError> {
        let subject = *self.0.lock().unwrap();
        Ok(VerifiedIdentity::for_test(
            ExternalIdentity::new(ISSUER, subject).unwrap(),
            Some("alice@example.org".into()),
        ))
    }
}

struct Rig {
    w: World,
    wiki: WikiId,
    at_the_provider: Arc<Mutex<&'static str>>,
}

fn rig(join: JoinPolicy) -> Rig {
    let at_the_provider = Arc::new(Mutex::new("nobody"));
    let idp = Arc::new(Idp(Mutex::new(HashMap::from([
        (
            "https://idp.example/.well-known/openid-configuration".to_owned(),
            format!(
                r#"{{"issuer":"{ISSUER}","authorization_endpoint":"https://idp.example/auth","token_endpoint":"https://idp.example/token","jwks_uri":"https://idp.example/jwks"}}"#
            )
            .into_bytes(),
        ),
        (
            "https://idp.example/token".to_owned(),
            br#"{"id_token":"t"}"#.to_vec(),
        ),
    ]))));
    let provider = OidcProvider::with_verifier(
        OidcConfig {
            issuer: ISSUER.into(),
            client_id: "c".into(),
            client_secret: None,
            redirect_uri: "https://wiki.example/cb".into(),
            scopes: Vec::new(),
        },
        idp,
        Arc::new(Vouches(at_the_provider.clone())),
    );
    let w = world_built(AuthConfig { join, ..config() }, |auth| {
        auth.with_oidc(provider)
    });
    let wiki = w
        .store
        .create(NewWiki::new(
            Slug::new("garden").unwrap(),
            DisplayName::new("Garden").unwrap(),
        ))
        .unwrap()
        .id;
    Rig {
        w,
        wiki,
        at_the_provider,
    }
}

impl Rig {
    fn person_at_the_provider(&self, subject: &'static str) {
        *self.at_the_provider.lock().unwrap() = subject;
    }

    /// A whole round trip as a browser makes it.
    fn round_trip(
        &self,
        purpose: OidcPurpose<'_>,
        client: &str,
        now: i64,
    ) -> Result<OidcSignedIn, OidcSignInError> {
        let begun = self.w.auth.begin_oidc(purpose, &ip(client), now)?;
        let cookie = format!("{STATE_COOKIE}={}", begun.state);
        self.w
            .auth
            .finish_oidc(&begun.state, Some(&cookie), "code", &ip(client), now + 1)
    }

    fn sign_in(&self, client: &str, now: i64) -> Result<OidcSignedIn, OidcSignInError> {
        self.round_trip(
            OidcPurpose::SignIn {
                next: Some("/back"),
                after_joining: Some("/-/account"),
            },
            client,
            now,
        )
    }

    fn accounts_with(&self, subject: &str) -> Option<inklkunn_store::Account> {
        self.w
            .store
            .account_by_identity(&ExternalIdentity::new(ISSUER, subject).unwrap())
            .unwrap()
    }
}

#[test]
fn where_joining_is_off_an_unlinked_identity_makes_no_account_and_is_told_so() {
    let r = rig(JoinPolicy::Off);
    r.person_at_the_provider("newcomer");
    let e = r.sign_in("203.0.113.1", NOW).unwrap_err();
    assert_eq!(e, OidcSignInError::NotLinked);
    assert!(r.accounts_with("newcomer").is_none());
}

#[test]
fn where_the_provider_may_make_accounts_a_first_sign_in_makes_one_with_no_password_and_no_role() {
    for policy in [JoinPolicy::Provider, JoinPolicy::Anyone] {
        let r = rig(policy);
        r.person_at_the_provider("newcomer");
        let first = r.sign_in("203.0.113.1", NOW).unwrap();
        assert!(first.account_created, "{policy:?}");
        // A new account lands where the joining asked, not where an existing one would.
        assert_eq!(first.landing.target(), "/-/account");
        let account = r.accounts_with("newcomer").expect("the account");
        assert_eq!(account.id, first.signed_in.account);
        assert!(account.local.is_none(), "no password");
        assert!(!account.disabled);
        assert!(
            account.display_name.as_str().starts_with("member-"),
            "a placeholder, not anything the provider said: {}",
            account.display_name
        );
        // No role in any wiki: nothing beyond what a wiki gives everyone.
        assert_eq!(r.w.store.role(&account.id, &r.wiki).unwrap(), None);
        // The next sign-in is the same account, and lands on `next`.
        let again = r.sign_in("203.0.113.1", NOW + 100).unwrap();
        assert!(!again.account_created);
        assert_eq!(again.signed_in.account, account.id);
        assert_eq!(again.landing.target(), "/back");
    }
}

#[test]
fn an_account_is_never_matched_or_linked_by_email() {
    let r = rig(JoinPolicy::Provider);
    // The provider reports alice's e-mail address for a stranger; the stranger gets a new account, and alice's
    // is untouched and not linked to anything.
    r.person_at_the_provider("a-stranger");
    let signed = r.sign_in("203.0.113.1", NOW).unwrap();
    assert!(signed.account_created);
    assert_ne!(signed.signed_in.account, r.w.alice.id);
    assert_eq!(
        r.accounts_with("a-stranger").unwrap().id,
        signed.signed_in.account
    );
    let alice = r.w.store.account(&r.w.alice.id).unwrap().unwrap();
    assert_eq!(alice.local, r.w.alice.local);
    // And the stranger is not alice: one person, one account.
    assert!(
        r.w.store
            .account_by_identity(&ExternalIdentity::new(ISSUER, "alice").unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_disabled_account_does_not_come_back_through_joining() {
    let r = rig(JoinPolicy::Provider);
    r.person_at_the_provider("leaver");
    let first = r.sign_in("203.0.113.1", NOW).unwrap();
    r.w.auth.disable(&first.signed_in.account, NOW + 5).unwrap();
    // Not a new account, and not a session: the identity is taken by a disabled one.
    assert_eq!(
        r.sign_in("203.0.113.1", NOW + 10).unwrap_err(),
        OidcSignInError::Auth(AuthError::BadCredentials)
    );
    assert_eq!(
        r.w.store
            .account_by_identity(&ExternalIdentity::new(ISSUER, "leaver").unwrap())
            .unwrap()
            .map(|a| a.id),
        Some(first.signed_in.account)
    );
}

#[test]
fn making_accounts_through_the_provider_is_counted_per_client_address() {
    let r = rig(JoinPolicy::Provider);
    let subjects: [&'static str; 12] = [
        "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11",
    ];
    let mut made = 0;
    let mut stopped = None;
    for (i, subject) in subjects.into_iter().enumerate() {
        r.person_at_the_provider(subject);
        match r.sign_in("198.51.100.7", NOW + i64::try_from(i).unwrap()) {
            Ok(done) => {
                assert!(done.account_created);
                made += 1;
            }
            Err(e) => {
                stopped = Some(e);
                break;
            }
        }
    }
    assert!(made >= 1, "some are made");
    let Some(OidcSignInError::Auth(AuthError::Locked { .. })) = stopped else {
        panic!("the address is stopped after {made}: {stopped:?}");
    };
    // Another address is not.
    r.person_at_the_provider("elsewhere");
    assert!(r.sign_in("198.51.100.8", NOW + 20).unwrap().account_created);
}

// ---- an invitation redeemed with the provider ----

fn invite(r: &Rig, role: Option<Role>) -> String {
    let grant = role.map(|role| (r.wiki.clone(), role));
    r.w.auth
        .invite(grant, DEFAULT_LIFETIME_SECS, NOW)
        .unwrap()
        .secret
        .as_str()
        .to_owned()
}

#[test]
fn an_invitation_redeemed_with_the_provider_makes_the_account_bound_to_it_with_the_role_it_named() {
    // Joining is off: an invitation is how this server's accounts come into being.
    let r = rig(JoinPolicy::Off);
    let secret = invite(&r, None);
    r.person_at_the_provider("invited");
    let done = r
        .round_trip(
            OidcPurpose::Invitation {
                secret: &secret,
                next: Some("/-/account"),
            },
            "203.0.113.1",
            NOW + 1,
        )
        .unwrap();
    assert!(done.account_created);
    assert_eq!(done.landing.target(), "/-/account");
    let account = r.accounts_with("invited").expect("the account");
    assert_eq!(account.id, done.signed_in.account);
    assert!(
        account.local.is_none(),
        "bound to the provider, no password"
    );
    // The invitation is spent: the link says what it says for any used link, and a second person cannot use it.
    assert!(!r.w.auth.invitation_usable(&secret, NOW + 2).unwrap());
    r.person_at_the_provider("second-person");
    assert_eq!(
        r.round_trip(
            OidcPurpose::Invitation {
                secret: &secret,
                next: None
            },
            "203.0.113.2",
            NOW + 3
        )
        .unwrap_err(),
        OidcSignInError::InvitationUnusable
    );
    assert!(r.accounts_with("second-person").is_none());
    // And the account signs in with the provider afterwards, as the same account.
    r.person_at_the_provider("invited");
    let again = r.sign_in("203.0.113.1", NOW + 10).unwrap();
    assert!(!again.account_created);
    assert_eq!(again.signed_in.account, account.id);
}

#[test]
fn an_invitation_that_names_a_role_grants_it_in_the_same_step() {
    let r = rig(JoinPolicy::Off);
    let secret = invite(&r, Some(Role::Reviewer));
    r.person_at_the_provider("reviewer");
    let done = r
        .round_trip(
            OidcPurpose::Invitation {
                secret: &secret,
                next: None,
            },
            "203.0.113.1",
            NOW + 1,
        )
        .unwrap();
    assert_eq!(
        r.w.store.role(&done.signed_in.account, &r.wiki).unwrap(),
        Some(Role::Reviewer)
    );
}

#[test]
fn an_identity_that_has_an_account_spends_no_invitation_and_is_told_to_sign_in() {
    let r = rig(JoinPolicy::Provider);
    r.person_at_the_provider("existing");
    let _ = r.sign_in("203.0.113.1", NOW).unwrap();
    let secret = invite(&r, Some(Role::Admin));
    let e = r
        .round_trip(
            OidcPurpose::Invitation {
                secret: &secret,
                next: None,
            },
            "203.0.113.1",
            NOW + 10,
        )
        .unwrap_err();
    assert_eq!(e, OidcSignInError::IdentityTaken);
    // The link is still good for someone else, and nobody got the role.
    assert!(r.w.auth.invitation_usable(&secret, NOW + 11).unwrap());
    let existing = r.accounts_with("existing").unwrap();
    assert_eq!(r.w.store.role(&existing.id, &r.wiki).unwrap(), None);
}

#[test]
fn an_invitation_that_is_not_usable_does_not_start_a_sign_in_and_costs_no_start() {
    let r = rig(JoinPolicy::Off);
    let client = ip("203.0.113.9");
    let never = format!("inv_{}", "A".repeat(43));
    for i in 0..40 {
        assert_eq!(
            r.w.auth
                .begin_oidc(
                    OidcPurpose::Invitation {
                        secret: &never,
                        next: None
                    },
                    &client,
                    NOW + i
                )
                .unwrap_err(),
            OidcSignInError::InvitationUnusable,
            "{i}"
        );
    }
    // None of those spent the address's starts (nothing was started).
    assert!(
        r.w.auth
            .begin_oidc(
                OidcPurpose::SignIn {
                    next: None,
                    after_joining: None
                },
                &client,
                NOW + 50
            )
            .is_ok()
    );
}

#[test]
fn an_invitation_used_while_the_person_was_at_the_provider_is_refused_at_the_callback() {
    let r = rig(JoinPolicy::Off);
    let secret = invite(&r, None);
    r.person_at_the_provider("slow");
    let begun =
        r.w.auth
            .begin_oidc(
                OidcPurpose::Invitation {
                    secret: &secret,
                    next: None,
                },
                &ip("203.0.113.1"),
                NOW,
            )
            .unwrap();
    // Somebody else uses the link first, with a password.
    r.w.auth
        .redeem_with_password(&secret, "quick", "Quick", "a password long enough", NOW + 1)
        .unwrap();
    let cookie = format!("{STATE_COOKIE}={}", begun.state);
    let e =
        r.w.auth
            .finish_oidc(
                &begun.state,
                Some(&cookie),
                "code",
                &ip("203.0.113.1"),
                NOW + 2,
            )
            .unwrap_err();
    assert_eq!(e, OidcSignInError::InvitationUnusable);
    assert!(r.accounts_with("slow").is_none());
}

#[test]
fn the_invitation_secret_is_in_no_cookie_no_landing_and_no_printed_value() {
    let r = rig(JoinPolicy::Off);
    let secret = invite(&r, None);
    r.person_at_the_provider("quiet");
    let purpose = OidcPurpose::Invitation {
        secret: &secret,
        next: Some("/-/account"),
    };
    assert!(!format!("{purpose:?}").contains(&secret));
    let begun =
        r.w.auth
            .begin_oidc(purpose, &ip("203.0.113.1"), NOW)
            .unwrap();
    assert!(!begun.state_cookie().contains(&secret));
    assert!(!begun.authorization_url.contains(&secret));
    let cookie = format!("{STATE_COOKIE}={}", begun.state);
    let done =
        r.w.auth
            .finish_oidc(
                &begun.state,
                Some(&cookie),
                "code",
                &ip("203.0.113.1"),
                NOW + 1,
            )
            .unwrap();
    for cookie in done.landing.set_cookies() {
        assert!(!cookie.contains(&secret), "{cookie}");
    }
    assert!(!format!("{:?}", done.landing).contains(&secret));
}
