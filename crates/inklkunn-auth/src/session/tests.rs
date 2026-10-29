#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_store::{Account, AccountStore, DisplayName, MemoryStore};

use super::*;

fn store_with_account() -> (Arc<MemoryStore>, AccountId) {
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
    (store, id)
}

#[test]
fn a_session_is_opaque_random_stored_only_as_a_hash_and_expires() {
    let (store, id) = store_with_account();
    let sessions = Sessions::new(store.clone(), 1_000);
    let s = sessions.start(&id, 100).unwrap();
    assert_eq!(s.secret().as_str().len(), 43);
    assert!(!s.secret().as_str().contains(id.as_str()));
    assert_eq!(s.expires_at(), 1_100);
    // What the store holds is a hash of the secret, never the secret.
    let record = store
        .session(&stored_form(s.secret().as_str()), 100)
        .unwrap()
        .unwrap();
    assert_ne!(record.id_hash, s.secret().as_str());
    assert!(record.id_hash.starts_with("sha256$"));

    let found = sessions
        .resolve(s.secret().as_str(), 1_099)
        .unwrap()
        .unwrap();
    assert_eq!(found.account, id);
    assert!(found.csrf.matches(s.csrf().as_str()));
    assert!(
        sessions
            .resolve(s.secret().as_str(), 1_100)
            .unwrap()
            .is_none(),
        "over at its end"
    );
}

#[test]
fn every_start_is_a_new_secret_and_only_a_well_formed_one_is_looked_up() {
    let (store, id) = store_with_account();
    let sessions = Sessions::new(store, 1_000);
    let a = sessions.start(&id, 100).unwrap();
    let b = sessions.start(&id, 100).unwrap();
    assert_ne!(a.secret(), b.secret());
    assert_ne!(a.csrf(), b.csrf());
    for bad in [
        "",
        "short",
        &"a".repeat(44),
        &format!("{}!", "a".repeat(42)),
        &"é".repeat(21),
    ] {
        assert!(sessions.resolve(bad, 100).unwrap().is_none(), "{bad:?}");
    }
}

#[test]
fn ending_a_session_and_ending_all_of_an_accounts() {
    let (store, id) = store_with_account();
    let sessions = Sessions::new(store, 1_000);
    let a = sessions.start(&id, 100).unwrap();
    let b = sessions.start(&id, 100).unwrap();
    let c = sessions.start(&id, 100).unwrap();
    assert!(sessions.end(a.secret().as_str()).unwrap());
    assert!(!sessions.end(a.secret().as_str()).unwrap());
    assert!(
        sessions
            .resolve(a.secret().as_str(), 101)
            .unwrap()
            .is_none()
    );
    assert_eq!(sessions.end_all(&id).unwrap(), 2);
    assert!(
        sessions
            .resolve(b.secret().as_str(), 101)
            .unwrap()
            .is_none()
    );
    assert!(
        sessions
            .resolve(c.secret().as_str(), 101)
            .unwrap()
            .is_none()
    );
}

#[test]
fn starting_a_session_clears_away_the_ones_that_have_ended() {
    let (store, id) = store_with_account();
    let sessions = Sessions::new(store.clone(), 10);
    let old = sessions.start(&id, 100).unwrap();
    sessions.start(&id, 500).unwrap();
    assert!(
        store
            .session(&stored_form(old.secret().as_str()), 0)
            .unwrap()
            .is_none(),
        "purged"
    );
}

#[test]
fn the_cookie_is_host_prefixed_secure_httponly_and_samesite_strict() {
    let (store, id) = store_with_account();
    let s = Sessions::new(store, 1_000).start(&id, 100).unwrap();
    let cookie = s.set_cookie(100);
    assert!(cookie.starts_with(&format!(
        "__Host-inklkunn-session={}; ",
        s.secret().as_str()
    )));
    for part in [
        "Path=/",
        "Secure",
        "HttpOnly",
        "SameSite=Strict",
        "Max-Age=1000",
    ] {
        assert!(cookie.split("; ").any(|p| p == part), "{part} in {cookie}");
    }
    assert!(!cookie.contains("Domain"), "a __Host- cookie has no Domain");
    assert!(clear_cookie().contains("Max-Age=0"));
    assert!(clear_cookie().starts_with("__Host-inklkunn-session=;"));
}

#[test]
fn a_csrf_mismatch_is_refused_and_nothing_empty_matches() {
    let t = CsrfToken::mint().unwrap();
    let other = CsrfToken::mint().unwrap();
    assert!(t.matches(t.as_str()));
    assert!(!t.matches(other.as_str()));
    assert!(!t.matches(""));
    assert!(!t.matches(&t.as_str()[..10]));
    assert!(!t.matches(&format!("{}x", t.as_str())));
}

#[test]
fn the_sign_in_form_uses_a_double_submit_cookie() {
    let t = CsrfToken::mint().unwrap();
    let cookie = format!("theme=dark; {CSRF_COOKIE}={}; other=1", t.as_str());
    assert!(form_token_matches(Some(&cookie), t.as_str()));
    assert!(!form_token_matches(Some(&cookie), "forged"));
    assert!(!form_token_matches(Some(&cookie), ""));
    assert!(!form_token_matches(None, t.as_str()));
    assert!(!form_token_matches(Some("theme=dark"), t.as_str()));
    let set = t.form_cookie();
    assert!(set.contains("Secure") && set.contains("HttpOnly") && set.contains("SameSite=Strict"));
}

#[test]
fn cookie_parsing_takes_exactly_one_value_and_refuses_a_duplicate() {
    assert_eq!(cookie_value("a=1; b=2", "b"), Some("2"));
    assert_eq!(cookie_value("a=1; b=2", "c"), None);
    assert_eq!(
        cookie_value("b=1; b=2", "b"),
        None,
        "two copies are ambiguous"
    );
    assert_eq!(
        cookie_value("b=1; garbage", "b"),
        Some("1"),
        "a pair with no `=` is skipped"
    );
    let secret = "a".repeat(43);
    let header = format!("{SESSION_COOKIE}={secret}");
    assert_eq!(secret_from_cookie_header(&header), Some(secret.as_str()));
    assert_eq!(
        secret_from_cookie_header(&format!("{SESSION_COOKIE}=short")),
        None
    );
}

#[test]
fn debug_output_never_shows_a_secret() {
    let (store, id) = store_with_account();
    let s = Sessions::new(store, 1_000).start(&id, 100).unwrap();
    let shown = format!("{s:?} {:?} {:?}", s.secret(), s.csrf());
    assert!(
        !shown.contains(s.secret().as_str()) && !shown.contains(s.csrf().as_str()),
        "{shown}"
    );
}

/// Which of these secrets still open a session at `now`.
fn live(sessions: &Sessions, secrets: &[String], now: i64) -> Vec<bool> {
    secrets
        .iter()
        .map(|s| sessions.resolve(s, now).unwrap().is_some())
        .collect()
}

#[test]
fn an_account_holds_at_most_a_hundred_live_sessions_and_the_oldest_end_not_the_sign_in() {
    assert_eq!(MAX_LIVE_PER_ACCOUNT, 100, "the tokens' hundred");
    let (store, id) = store_with_account();
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
    let sessions = Sessions::new(store, 1_000_000);
    let bobs = sessions.start(&other, 100).unwrap();
    // 150 sign-ins at the same instant: none is refused, the newest hundred live, the first fifty are gone.
    let secrets: Vec<String> = (0..150)
        .map(|_| {
            sessions
                .start(&id, 100)
                .unwrap()
                .secret()
                .as_str()
                .to_owned()
        })
        .collect();
    let alive = live(&sessions, &secrets, 101);
    assert_eq!(alive.iter().filter(|a| **a).count(), 100);
    assert!(
        alive.iter().take(50).all(|a| !a),
        "the oldest fifty ended, in order"
    );
    assert!(alive.iter().skip(50).all(|a| *a), "the newest hundred live");
    assert!(
        sessions
            .resolve(bobs.secret().as_str(), 101)
            .unwrap()
            .is_some(),
        "another account's session is not touched"
    );
}

#[test]
fn the_session_just_made_is_never_the_one_that_goes_even_when_the_clock_went_backwards() {
    let (store, id) = store_with_account();
    let sessions = Sessions::new(store, 1_000_000);
    for i in 0..MAX_LIVE_PER_ACCOUNT {
        sessions
            .start(&id, 10_000 + i64::try_from(i).unwrap())
            .unwrap();
    }
    // The next sign-in is stamped long before all the others (a clock set back): it is still the newest.
    let late = sessions.start(&id, 5).unwrap();
    assert!(
        sessions
            .resolve(late.secret().as_str(), 6)
            .unwrap()
            .is_some()
    );
}

#[test]
fn sessions_that_are_over_do_not_count_against_the_cap() {
    let (store, id) = store_with_account();
    let sessions = Sessions::new(store, 50);
    let first: Vec<String> = (0..MAX_LIVE_PER_ACCOUNT)
        .map(|_| {
            sessions
                .start(&id, 100)
                .unwrap()
                .secret()
                .as_str()
                .to_owned()
        })
        .collect();
    // They all end at 150; a sign-in at 200 finds a clean account and ends nobody who is live.
    let fresh: Vec<String> = (0..MAX_LIVE_PER_ACCOUNT)
        .map(|_| {
            sessions
                .start(&id, 200)
                .unwrap()
                .secret()
                .as_str()
                .to_owned()
        })
        .collect();
    assert!(live(&sessions, &fresh, 201).iter().all(|a| *a));
    assert!(live(&sessions, &first, 120).iter().all(|a| !a), "gone");
}

#[test]
fn a_burst_of_sign_ins_from_many_threads_is_never_refused_and_ends_at_the_cap() {
    let (store, id) = store_with_account();
    let sessions = Arc::new(Sessions::new(store, 1_000_000));
    let secrets = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..40 {
                    let s = sessions
                        .start(&id, 100)
                        .expect("a sign-in is never refused");
                    secrets.lock().unwrap().push(s.secret().as_str().to_owned());
                }
            });
        }
    });
    let secrets = secrets.into_inner().unwrap();
    assert_eq!(secrets.len(), 320);
    assert_eq!(
        live(&sessions, &secrets, 101)
            .iter()
            .filter(|a| **a)
            .count(),
        MAX_LIVE_PER_ACCOUNT,
        "exactly the cap is live after the burst"
    );
}
