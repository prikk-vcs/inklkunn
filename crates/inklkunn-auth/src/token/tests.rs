#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_store::{Account, AccountStore, DisplayName, MemoryStore};

use super::*;

fn setup() -> (Arc<MemoryStore>, Tokens, AccountId) {
    let store = Arc::new(MemoryStore::new());
    let id = AccountId::new();
    store
        .add_account(&Account {
            id: id.clone(),
            display_name: DisplayName::new("Alice").unwrap(),
            local: None,
            created_at: 1,
            disabled: false,
        })
        .unwrap();
    (store.clone(), Tokens::new(store), id)
}

#[test]
fn a_token_is_prefixed_random_shown_once_and_kept_only_as_a_hash() {
    let (store, tokens, id) = setup();
    let t = tokens
        .issue(&id, Scope::Propose, "ci", 3_600, 1_000)
        .unwrap();
    let secret = t.secret.as_str();
    assert!(secret.starts_with("ink_") && secret.len() == 47, "{secret}");
    assert_eq!(t.expires_at, 4_600);
    let stored = store.token_by_hash(&stored_form(secret)).unwrap().unwrap();
    assert_ne!(stored.hash, secret);
    assert_eq!((stored.scope, stored.account), (Scope::Propose, id.clone()));
    assert_ne!(
        tokens
            .issue(&id, Scope::Propose, "ci", 3_600, 1_000)
            .unwrap()
            .secret,
        t.secret,
        "two tokens differ"
    );
    assert!(!format!("{t:?}").contains(secret), "{t:?}");
}

#[test]
fn a_token_authenticates_with_the_scope_it_was_issued_with_and_no_other() {
    let (_, tokens, id) = setup();
    for scope in Scope::ALL {
        let t = tokens.issue(&id, scope, "x", 100, 0).unwrap();
        let v = tokens.verify(t.secret.as_str(), 50).unwrap();
        assert_eq!(
            (v.scope, v.account.clone(), v.id.clone()),
            (scope, id.clone(), t.id.clone())
        );
    }
}

#[test]
fn unknown_malformed_expired_and_revoked_tokens_are_one_answer() {
    let (_, tokens, id) = setup();
    let t = tokens.issue(&id, Scope::Read, "x", 100, 0).unwrap();
    let live = t.secret.as_str();
    assert!(tokens.verify(live, 99).is_ok());
    for (what, presented, now) in [
        ("expired at its end instant", live.to_owned(), 100),
        ("long expired", live.to_owned(), 10_000),
        ("unknown", format!("ink_{}", "a".repeat(43)), 50),
        ("wrong prefix", format!("plt_{}", &live[4..]), 50),
        ("truncated", live[..20].to_owned(), 50),
        ("extended", format!("{live}a"), 50),
        ("empty", String::new(), 50),
    ] {
        assert_eq!(
            tokens.verify(&presented, now),
            Err(AuthError::BadCredentials),
            "{what}"
        );
    }
    assert!(tokens.revoke(&id, &t.id, 60).unwrap());
    assert_eq!(
        tokens.verify(live, 61),
        Err(AuthError::BadCredentials),
        "revoked"
    );
}

#[test]
fn only_the_owner_can_revoke_and_only_once() {
    let (store, tokens, id) = setup();
    let other = AccountId::new();
    store
        .add_account(&Account {
            id: other.clone(),
            display_name: DisplayName::new("Bob").unwrap(),
            local: None,
            created_at: 1,
            disabled: false,
        })
        .unwrap();
    let t = tokens.issue(&id, Scope::Read, "x", 100, 0).unwrap();
    assert!(!tokens.revoke(&other, &t.id, 1).unwrap(), "not Bob's token");
    assert!(tokens.verify(t.secret.as_str(), 2).is_ok(), "still live");
    assert!(tokens.revoke(&id, &t.id, 3).unwrap());
    assert!(!tokens.revoke(&id, &t.id, 4).unwrap());
}

#[test]
fn a_lifetime_is_required_and_bounded() {
    let (_, tokens, id) = setup();
    for bad in [0, -5, MAX_LIFETIME_SECS + 1, i64::MAX] {
        assert_eq!(
            tokens.issue(&id, Scope::Read, "x", bad, 0).unwrap_err(),
            IssueError::Lifetime,
            "{bad}"
        );
    }
    assert!(
        tokens
            .issue(&id, Scope::Read, "x", MAX_LIFETIME_SECS, 0)
            .is_ok()
    );
    assert!(tokens.issue(&id, Scope::Read, "x", 1, 0).is_ok());
}

#[test]
fn a_listing_shows_labels_and_times_and_never_a_token_or_a_hash() {
    let (_, tokens, id) = setup();
    let t = tokens
        .issue(&id, Scope::Review, "deploy\nbot", 100, 5)
        .unwrap();
    tokens.revoke(&id, &t.id, 7).unwrap();
    let listed = tokens.list(&id).unwrap();
    assert_eq!(listed.len(), 1);
    let item = listed.first().unwrap();
    assert_eq!(item.label, "deploy?bot");
    assert_eq!(
        (
            item.scope,
            item.created_at,
            item.expires_at,
            item.revoked_at
        ),
        (Scope::Review, 5, Some(105), Some(7))
    );
    assert!(!format!("{listed:?}").contains("sha256"));
}

#[test]
fn issuing_for_an_account_that_is_not_there_is_a_backend_error() {
    let (_, tokens, _) = setup();
    assert!(matches!(
        tokens.issue(&AccountId::new(), Scope::Read, "x", 100, 0),
        Err(IssueError::Auth(AuthError::Backend(_)))
    ));
}
