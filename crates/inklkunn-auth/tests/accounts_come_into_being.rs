//! How accounts come into being (the sign-in handoff's U2; the owner's ruling DR-11): invitations that are
//! one-time and expiring, joining that is off unless the operator opens it, and the account page's own acts
//! (the sessions, the display name).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::atomic::Ordering;

use common::*;
use inklkunn_auth::invitation::{DEFAULT_LIFETIME_SECS, MAX_LIFETIME_SECS};
use inklkunn_auth::session::secret_from_cookie_header;
use inklkunn_auth::{
    AccountError, AuthConfig, AuthError, InviteError, JoinPolicy, SessionCheck, ThrottlePolicy,
};
use inklkunn_store::{
    AccountStore, DisplayName, InvitationStore, LoginName, NewWiki, Role, RoleStore, Slug,
    WikiStore,
};

const LONG: &str = "a password that is long enough";

fn wiki(w: &World) -> inklkunn_store::WikiRecord {
    w.store
        .create(NewWiki::new(
            Slug::new("garden").unwrap(),
            DisplayName::new("Garden").unwrap(),
        ))
        .unwrap()
}

fn open_to_anyone() -> AuthConfig {
    AuthConfig {
        join: JoinPolicy::Anyone,
        ..config()
    }
}

#[test]
fn an_invitation_is_a_secret_shown_once_and_kept_only_as_a_hash() {
    let w = world();
    let invitation = w.auth.invite(None, DEFAULT_LIFETIME_SECS, NOW).unwrap();
    let secret = invitation.secret.as_str().to_owned();
    assert!(secret.starts_with("inv_") && secret.len() == 4 + 43);
    assert_eq!(invitation.expires_at, NOW + 7 * 24 * 3600);
    assert!(
        !format!("{invitation:?}").contains(&secret),
        "the secret is not in a debug print"
    );
    // The store knows the hash, not the secret: the secret itself names nothing there.
    assert_eq!(w.store.invitation_by_hash(&secret).unwrap(), None);
    assert!(w.auth.invitation_usable(&secret, NOW + 1).unwrap());
}

#[test]
fn an_invitation_lives_between_a_second_and_thirty_days_and_names_a_wiki_that_exists() {
    let w = world();
    for bad in [0, -5, MAX_LIFETIME_SECS + 1] {
        assert_eq!(
            w.auth.invite(None, bad, NOW).err(),
            Some(InviteError::Lifetime)
        );
    }
    assert!(w.auth.invite(None, 1, NOW).is_ok());
    assert!(w.auth.invite(None, MAX_LIFETIME_SECS, NOW).is_ok());
    let elsewhere = (inklkunn_store::WikiId::new(), Role::Admin);
    assert_eq!(
        w.auth
            .invite(Some(elsewhere), DEFAULT_LIFETIME_SECS, NOW)
            .err(),
        Some(InviteError::NoSuchWiki)
    );
}

#[test]
fn the_person_who_opens_an_invitation_chooses_a_password_and_is_signed_in_with_the_role_it_named() {
    let w = world();
    let garden = wiki(&w);
    let invitation = w
        .auth
        .invite(
            Some((garden.id.clone(), Role::Admin)),
            DEFAULT_LIFETIME_SECS,
            NOW,
        )
        .unwrap();
    let signed_in = w
        .auth
        .redeem_with_password(
            invitation.secret.as_str(),
            "Carol",
            "Carol Q.",
            LONG,
            NOW + 60,
        )
        .unwrap();
    // The account exists with her display name and the password she chose, and holds the role.
    let account = w.store.account(&signed_in.account).unwrap().unwrap();
    assert_eq!(account.display_name.as_str(), "Carol Q.");
    assert_eq!(
        w.store.role(&account.id, &garden.id).unwrap(),
        Some(Role::Admin)
    );
    // She is signed in: the session is live and its token is the one writes need.
    let cookie = signed_in.session.set_cookie(NOW + 60);
    let secret = secret_from_cookie_header(cookie.split(';').next().unwrap()).unwrap();
    assert!(
        w.auth
            .principal_for_session(secret, NOW + 61)
            .unwrap()
            .is_some()
    );
    // And she can sign in again with the password, by the folded name.
    assert!(
        w.auth
            .sign_in("carol", LONG, &ip("203.0.113.9"), NOW + 120)
            .is_ok()
    );
}

#[test]
fn an_invitation_works_once() {
    let w = world();
    let invitation = w.auth.invite(None, DEFAULT_LIFETIME_SECS, NOW).unwrap();
    let link = invitation.secret.as_str();
    w.auth
        .redeem_with_password(link, "first", "First", LONG, NOW + 1)
        .unwrap();
    assert!(!w.auth.invitation_usable(link, NOW + 2).unwrap());
    assert_eq!(
        w.auth
            .redeem_with_password(link, "second", "Second", LONG, NOW + 3)
            .err(),
        Some(AccountError::InvitationUnusable)
    );
    assert_eq!(
        w.store
            .account_by_login(&LoginName::parse("second").unwrap())
            .unwrap(),
        None,
        "the second person got no account"
    );
}

#[test]
fn an_invitation_ends_when_it_ends() {
    let w = world();
    let invitation = w.auth.invite(None, 100, NOW).unwrap();
    let link = invitation.secret.as_str();
    assert!(w.auth.invitation_usable(link, NOW + 99).unwrap());
    assert!(!w.auth.invitation_usable(link, NOW + 100).unwrap());
    assert_eq!(
        w.auth
            .redeem_with_password(link, "late", "Late", LONG, NOW + 100)
            .err(),
        Some(AccountError::InvitationUnusable)
    );
    assert_eq!(
        w.auth
            .redeem_with_password(link, "late", "Late", LONG, NOW + 5_000)
            .err(),
        Some(AccountError::InvitationUnusable)
    );
}

#[test]
fn a_link_that_is_no_invitation_costs_no_hashing_and_says_the_same_as_a_used_one() {
    let w = world();
    let hashed_at_start = w.hasher.hashes.load(Ordering::SeqCst);
    let invitation = w.auth.invite(None, DEFAULT_LIFETIME_SECS, NOW).unwrap();
    // Well-formed but unknown, malformed, empty, and the very secret of another kind of credential.
    let unknown = format!("inv_{}", "A".repeat(43));
    for bad in [
        unknown.as_str(),
        "inv_short",
        "",
        "ink_whatever",
        "../etc/passwd",
    ] {
        assert!(!w.auth.invitation_usable(bad, NOW).unwrap(), "{bad}");
        assert_eq!(
            w.auth
                .redeem_with_password(bad, "mallory", "Mallory", LONG, NOW)
                .err(),
            Some(AccountError::InvitationUnusable),
            "{bad}"
        );
    }
    assert_eq!(
        w.hasher.hashes.load(Ordering::SeqCst),
        hashed_at_start,
        "a stranger's bad link made the service hash nothing"
    );
    // The real one is untouched by all of that.
    assert!(
        w.auth
            .invitation_usable(invitation.secret.as_str(), NOW)
            .unwrap()
    );
}

#[test]
fn a_refused_choice_spends_nothing_so_the_same_link_is_tried_again() {
    let w = world();
    let invitation = w.auth.invite(None, DEFAULT_LIFETIME_SECS, NOW).unwrap();
    let link = invitation.secret.as_str();
    // A name that is taken (alice is in the world), a name that cannot be, a password too short, a blank name.
    assert_eq!(
        w.auth
            .redeem_with_password(link, "alice", "Imposter", LONG, NOW)
            .err(),
        Some(AccountError::LoginTaken)
    );
    assert!(matches!(
        w.auth
            .redeem_with_password(link, "a", "Short name", LONG, NOW)
            .err(),
        Some(AccountError::Login(_))
    ));
    assert!(matches!(
        w.auth
            .redeem_with_password(link, "dana", "Dana", "short", NOW)
            .err(),
        Some(AccountError::Password(_))
    ));
    assert!(matches!(
        w.auth
            .redeem_with_password(link, "dana", "   ", LONG, NOW)
            .err(),
        Some(AccountError::DisplayName(_))
    ));
    assert!(w.auth.invitation_usable(link, NOW).unwrap());
    assert!(
        w.auth
            .redeem_with_password(link, "dana", "Dana", LONG, NOW)
            .is_ok(),
        "and then it works"
    );
}

#[test]
fn an_invitation_with_no_role_makes_an_account_with_none() {
    let w = world();
    let garden = wiki(&w);
    let invitation = w.auth.invite(None, DEFAULT_LIFETIME_SECS, NOW).unwrap();
    let signed_in = w
        .auth
        .redeem_with_password(invitation.secret.as_str(), "erin", "Erin", LONG, NOW)
        .unwrap();
    assert_eq!(w.store.role(&signed_in.account, &garden.id).unwrap(), None);
}

#[test]
fn joining_is_off_by_default_and_for_a_provider_alone_it_is_not_a_password_door() {
    // The default configuration.
    assert_eq!(AuthConfig::default().join, JoinPolicy::Off);
    for policy in [JoinPolicy::Off, JoinPolicy::Provider] {
        let w = world_with(AuthConfig {
            join: policy,
            ..config()
        });
        let hashed = w.hasher.hashes.load(Ordering::SeqCst);
        assert_eq!(w.auth.join_policy(), policy);
        assert_eq!(
            w.auth
                .join_with_password("newcomer", "Newcomer", LONG, &ip("203.0.113.5"), NOW)
                .err(),
            Some(AccountError::JoinClosed),
            "{policy:?}"
        );
        assert_eq!(w.hasher.hashes.load(Ordering::SeqCst), hashed);
        assert_eq!(
            w.store
                .account_by_login(&LoginName::parse("newcomer").unwrap())
                .unwrap(),
            None
        );
    }
}

#[test]
fn where_anyone_may_join_a_person_makes_an_account_with_no_role_and_is_signed_in() {
    let w = world_with(open_to_anyone());
    let garden = wiki(&w);
    let signed_in = w
        .auth
        .join_with_password("newcomer", "Newcomer", LONG, &ip("203.0.113.5"), NOW)
        .unwrap();
    let account = w.store.account(&signed_in.account).unwrap().unwrap();
    assert_eq!(
        account.created_at, NOW,
        "the new-account quotas count from here"
    );
    assert_eq!(w.store.role(&account.id, &garden.id).unwrap(), None);
    assert!(
        matches!(
            w.auth
                .join_with_password("newcomer", "Another", LONG, &ip("203.0.113.5"), NOW)
                .err(),
            Some(AccountError::LoginTaken)
        ),
        "a name belongs to one account"
    );
}

#[test]
fn joining_is_throttled_per_client_address() {
    let w = world_with(AuthConfig {
        join: JoinPolicy::Anyone,
        join_throttle: ThrottlePolicy {
            max_failures: 3,
            lock_secs: 600,
            forget_secs: 600,
            max_tracked: 100,
        },
        ..config()
    });
    let one = ip("203.0.113.5");
    for n in 0..3 {
        w.auth
            .join_with_password(&format!("joiner{n}"), "Joiner", LONG, &one, NOW)
            .unwrap();
    }
    let hashed = w.hasher.hashes.load(Ordering::SeqCst);
    let locked = w
        .auth
        .join_with_password("joiner9", "Joiner", LONG, &one, NOW + 1)
        .err();
    assert!(matches!(
        locked,
        Some(AccountError::Auth(AuthError::Locked { .. }))
    ));
    assert_eq!(
        w.hasher.hashes.load(Ordering::SeqCst),
        hashed,
        "no work was done"
    );
    // Another address is not held back, and the lock ends.
    assert!(
        w.auth
            .join_with_password("joiner8", "Joiner", LONG, &ip("203.0.113.6"), NOW + 1)
            .is_ok()
    );
    assert!(
        w.auth
            .join_with_password("joiner7", "Joiner", LONG, &one, NOW + 700)
            .is_ok()
    );
}

#[test]
fn an_account_lists_its_own_sessions_and_ends_one_by_its_handle() {
    let w = world();
    let phone = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let laptop = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW + 10)
        .unwrap();
    let bobs = w
        .auth
        .sign_in("bob", "another long password", &ip("203.0.113.2"), NOW + 11)
        .unwrap();
    let laptop_secret = laptop.session.secret().as_str().to_owned();
    let phone_secret = phone.session.secret().as_str().to_owned();

    let listed = w
        .auth
        .sessions(&w.alice.id, Some(&laptop_secret), NOW + 20)
        .unwrap();
    assert_eq!(listed.len(), 2, "Bob's session is not Alice's to see");
    let [newest, oldest] = listed.as_slice() else {
        panic!("{listed:?}");
    };
    assert!(
        newest.current && !oldest.current,
        "the newest first, the asking one marked"
    );
    assert!(listed.iter().all(|s| s.handle.len() == 16));
    assert!(
        !listed
            .iter()
            .any(|s| laptop_secret.contains(&s.handle) || phone_secret.contains(&s.handle)),
        "a handle is not a piece of the cookie"
    );
    let other = w.auth.sessions(&w.alice.id, None, NOW + 20).unwrap();
    assert!(other.iter().all(|s| !s.current));

    // A handle of another account's session ends nothing.
    let bobs_handle = w
        .auth
        .sessions(&w.bob.id, None, NOW + 20)
        .unwrap()
        .first()
        .unwrap()
        .handle
        .clone();
    assert!(
        !w.auth
            .end_session_of(&w.alice.id, &bobs_handle, NOW + 21)
            .unwrap()
    );
    assert!(
        w.auth
            .principal_for_session(bobs.session.secret().as_str(), NOW + 22)
            .unwrap()
            .is_some()
    );
    // Nor do handles that are not handles.
    for junk in [
        "",
        "zzzzzzzzzzzzzzzz",
        "0123",
        "0123456789abcdef0",
        "../../../../../x",
    ] {
        assert!(
            !w.auth.end_session_of(&w.alice.id, junk, NOW + 21).unwrap(),
            "{junk}"
        );
    }
    // Her own, by handle: that session stops authenticating and the other goes on.
    let phone_handle = oldest.handle.clone();
    assert!(
        w.auth
            .end_session_of(&w.alice.id, &phone_handle, NOW + 22)
            .unwrap()
    );
    assert!(
        w.auth
            .principal_for_session(&phone_secret, NOW + 23)
            .unwrap()
            .is_none()
    );
    assert!(
        w.auth
            .principal_for_session(&laptop_secret, NOW + 23)
            .unwrap()
            .is_some()
    );
    assert!(
        !w.auth
            .end_session_of(&w.alice.id, &phone_handle, NOW + 24)
            .unwrap(),
        "once"
    );
    assert_eq!(
        w.auth.sessions(&w.alice.id, None, NOW + 25).unwrap().len(),
        1
    );
}

#[test]
fn an_expired_session_is_not_listed() {
    let w = world();
    let s = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    assert_eq!(
        w.auth.sessions(&w.alice.id, None, NOW + 1).unwrap().len(),
        1
    );
    assert!(
        w.auth
            .sessions(&w.alice.id, None, s.session.expires_at())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_display_name_changes_and_is_checked() {
    let w = world();
    w.auth.set_display_name(&w.alice.id, "Alice A.").unwrap();
    assert_eq!(
        w.store
            .account(&w.alice.id)
            .unwrap()
            .unwrap()
            .display_name
            .as_str(),
        "Alice A."
    );
    assert!(matches!(
        w.auth.set_display_name(&w.alice.id, "  ").err(),
        Some(AccountError::DisplayName(_))
    ));
    assert!(matches!(
        w.auth
            .set_display_name(&inklkunn_store::AccountId::new(), "Ghost")
            .err(),
        Some(AccountError::NotFound)
    ));
}

#[test]
fn a_session_made_by_an_invitation_checks_its_csrf_token_like_any_other() {
    let w = world();
    let invitation = w.auth.invite(None, DEFAULT_LIFETIME_SECS, NOW).unwrap();
    let signed_in = w
        .auth
        .redeem_with_password(invitation.secret.as_str(), "frank", "Frank", LONG, NOW)
        .unwrap();
    let secret = signed_in.session.secret().as_str();
    assert!(matches!(
        w.auth
            .principal_for_write(secret, "wrong", NOW + 1)
            .unwrap(),
        SessionCheck::CsrfMismatch
    ));
    assert!(matches!(
        w.auth
            .principal_for_write(secret, signed_in.session.csrf().as_str(), NOW + 1)
            .unwrap(),
        SessionCheck::Valid(_)
    ));
}
