//! Password sign-in (RFC 002 D-1): constant work, throttles, the gate, sessions and CSRF.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use inklkunn_auth::session::{cookie_value, secret_from_cookie_header};
use inklkunn_auth::{AuthConfig, AuthError, Authenticator, PasswordHasher};
use inklkunn_core::Principal;
use inklkunn_store::{MemoryStore, Scope};

#[test]
fn the_dummy_hash_is_made_once_at_start() {
    let w = world();
    // One hash for the dummy, one each for the two registered accounts.
    assert_eq!(w.hasher.hashes.load(Ordering::SeqCst), 3);
}

#[test]
fn every_sign_in_that_reaches_the_hasher_does_exactly_one_verification() {
    let w = world();
    let c = ip("203.0.113.1");
    let mut disabled = w
        .auth
        .register_local("carol", "Carol", "carol has a long one", NOW)
        .unwrap();
    w.auth.disable(&disabled.id).unwrap();
    disabled.disabled = true;

    let cases: Vec<(&str, &str, &str, bool)> = vec![
        ("the right password", "alice", ALICE_PASSWORD, true),
        ("a wrong password", "alice", "not the password", false),
        (
            "the right password in capitals of the name",
            "ALICE",
            ALICE_PASSWORD,
            true,
        ),
        ("an unknown name", "nobody", "whatever whatever", false),
        (
            "a name no account could have",
            "no body!",
            "whatever whatever",
            false,
        ),
        ("an empty name", "", "whatever whatever", false),
        ("an empty password", "alice", "", false),
        (
            "the right password of a disabled account",
            "carol",
            "carol has a long one",
            false,
        ),
    ];
    for (what, login, password, ok) in cases {
        let before = w.hasher.verifies();
        let result = w.auth.sign_in(login, password, &c, NOW);
        assert_eq!(result.is_ok(), ok, "{what}");
        if !ok {
            assert!(
                matches!(result, Err(AuthError::BadCredentials)),
                "{what}: {result:?}"
            );
        }
        assert_eq!(
            w.hasher.verifies() - before,
            1,
            "{what}: one verification, whatever the credential"
        );
    }
}

#[test]
fn an_oversized_password_is_refused_without_any_hashing_and_counts_as_a_failure() {
    let w = world();
    let before = w.hasher.verifies();
    let huge = "a".repeat(2_000);
    assert!(matches!(
        w.auth.sign_in("alice", &huge, &ip("203.0.113.1"), NOW),
        Err(AuthError::BadCredentials)
    ));
    assert_eq!(w.hasher.verifies(), before);
}

#[test]
fn unknown_account_and_wrong_password_cost_the_same_within_noise() {
    // The real Argon2id at a cost large enough to dominate a map lookup, on the best of several runs each.
    let store = Arc::new(MemoryStore::new());
    let hasher = Arc::new(inklkunn_auth::Argon2idHasher::with_cost(4096, 2, 1));
    let auth = Authenticator::with_hasher(store, config(), hasher).unwrap();
    auth.register_local("alice", "Alice", ALICE_PASSWORD, NOW)
        .unwrap();
    let best_of = |login: &str, password: &str| {
        (0..7)
            .map(|i| {
                // A distinct address each time so no throttle gets in the way of the measurement.
                let c = ip(&format!("198.51.100.{}", i + 1));
                let started = Instant::now();
                let _ = auth.sign_in(login, password, &c, NOW + i64::from(i));
                started.elapsed()
            })
            .min()
            .unwrap()
    };
    let unknown = best_of("nobody-at-all", "some password here");
    let wrong = best_of("alice", "some password here");
    let (lo, hi) = if unknown < wrong {
        (unknown, wrong)
    } else {
        (wrong, unknown)
    };
    assert!(
        hi.as_secs_f64() <= lo.as_secs_f64() * 2.0 + 0.005,
        "unknown {unknown:?} against wrong {wrong:?}"
    );
}

#[test]
fn a_name_locks_after_five_failures_and_a_correct_password_is_refused_while_locked() {
    let w = world();
    // A different address each time, so it is the name that locks and not the address.
    for i in 0..5 {
        let r = w.auth.sign_in(
            "alice",
            "wrong wrong wrong",
            &ip(&format!("203.0.113.{}", i + 1)),
            NOW + i,
        );
        assert!(matches!(r, Err(AuthError::BadCredentials)), "{r:?}");
    }
    let before = w.hasher.verifies();
    let locked = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.99"), NOW + 10);
    assert_eq!(
        locked.unwrap_err(),
        AuthError::Locked {
            until: NOW + 4 + 15 * 60
        }
    );
    assert_eq!(
        w.hasher.verifies(),
        before,
        "a locked name costs no hashing"
    );
    // Case and surrounding space do not escape the lock.
    assert!(matches!(
        w.auth
            .sign_in("  ALICE ", ALICE_PASSWORD, &ip("203.0.113.98"), NOW + 11),
        Err(AuthError::Locked { .. })
    ));
    // Other names are untouched, and after the lock the right password works again.
    assert!(
        w.auth
            .sign_in(
                "bob",
                "another long password",
                &ip("203.0.113.97"),
                NOW + 12
            )
            .is_ok()
    );
    assert!(
        w.auth
            .sign_in(
                "alice",
                ALICE_PASSWORD,
                &ip("203.0.113.96"),
                NOW + 4 + 15 * 60
            )
            .is_ok()
    );
}

#[test]
fn a_name_that_does_not_exist_locks_exactly_like_one_that_does() {
    let w = world();
    let outcomes = |login: &str| -> Vec<String> {
        (0..7)
            .map(|i| {
                let r = w.auth.sign_in(
                    login,
                    "wrong wrong wrong",
                    &ip(&format!("192.0.2.{}", i + 1)),
                    NOW + i,
                );
                format!("{r:?}")
            })
            .collect()
    };
    let (real, ghost) = (outcomes("bob"), outcomes("ghost"));
    // Same answers in the same places, down to the instant the lock ends.
    assert_eq!(real, ghost);
    assert!(
        real.get(5).is_some_and(|r| r.contains("Locked")),
        "{real:?}"
    );
}

#[test]
fn one_address_that_fails_thirty_times_is_locked_whatever_names_it_tries() {
    let w = world();
    let attacker = ip("203.0.113.50");
    for i in 0..30 {
        let r = w
            .auth
            .sign_in(&format!("user{i}"), "wrong wrong wrong", &attacker, NOW + i);
        assert!(matches!(r, Err(AuthError::BadCredentials)), "{i}: {r:?}");
    }
    // A correct password for a real account, from the locked address, is refused.
    let r = w.auth.sign_in("alice", ALICE_PASSWORD, &attacker, NOW + 31);
    assert!(matches!(r, Err(AuthError::Locked { .. })), "{r:?}");
    // The same account from somewhere else is fine: the lock is the address's.
    assert!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.51"), NOW + 32)
            .is_ok()
    );
}

#[test]
fn a_success_does_not_wipe_an_addresss_failures() {
    let w = world();
    let attacker = ip("203.0.113.60");
    for i in 0..29 {
        let _ = w
            .auth
            .sign_in(&format!("user{i}"), "wrong wrong wrong", &attacker, NOW + i);
    }
    // The attacker signs in to an account they own to try to reset the count.
    assert!(
        w.auth
            .sign_in("bob", "another long password", &attacker, NOW + 40)
            .is_ok()
    );
    let _ = w
        .auth
        .sign_in("user-last", "wrong wrong wrong", &attacker, NOW + 41);
    assert!(
        matches!(
            w.auth
                .sign_in("bob", "another long password", &attacker, NOW + 42),
            Err(AuthError::Locked { .. })
        ),
        "the 30th failure still locks"
    );
}

#[test]
fn a_busy_gate_says_busy_and_decides_nothing_about_the_credential() {
    // One permit, and a hasher that holds it until told to let go.
    struct Holding {
        inner: inklkunn_auth::Argon2idHasher,
        entered: Mutex<mpsc::Sender<()>>,
        release: Mutex<mpsc::Receiver<()>>,
        hold: std::sync::atomic::AtomicBool,
    }
    impl PasswordHasher for Holding {
        fn hash(&self, p: &str) -> Result<String, AuthError> {
            self.inner.hash(p)
        }
        fn verify(&self, p: &str, s: &str) -> bool {
            if self.hold.load(Ordering::SeqCst) {
                self.entered.lock().unwrap().send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            }
            self.inner.verify(p, s)
        }
    }
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let hasher = Arc::new(Holding {
        inner: inklkunn_auth::Argon2idHasher::with_cost(64, 1, 1),
        entered: Mutex::new(entered_tx),
        release: Mutex::new(release_rx),
        hold: std::sync::atomic::AtomicBool::new(false),
    });
    let store = Arc::new(MemoryStore::new());
    let auth = Arc::new(
        Authenticator::with_hasher(
            store,
            AuthConfig {
                hash_permits: 1,
                gate_wait: Duration::from_millis(100),
                ..AuthConfig::default()
            },
            hasher.clone(),
        )
        .unwrap(),
    );
    auth.register_local("alice", "Alice", ALICE_PASSWORD, NOW)
        .unwrap();
    hasher.hold.store(true, Ordering::SeqCst);
    let first = {
        let auth = Arc::clone(&auth);
        std::thread::spawn(move || auth.sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW))
    };
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // The only permit is held: a second sign-in waits its 100 ms and is told the service is busy.
    let started = Instant::now();
    let second = auth.sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.2"), NOW);
    assert_eq!(second.unwrap_err(), AuthError::Busy);
    assert!(started.elapsed() >= Duration::from_millis(90));
    release_tx.send(()).unwrap();
    assert!(
        first.join().unwrap().is_ok(),
        "the one holding the permit finished normally"
    );
    // Busy was not a failure: five more Busy answers would not have locked the name. The name has no
    // failures, so a right password signs in now that the gate is free.
    hasher.hold.store(false, Ordering::SeqCst);
    assert!(
        auth.sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.3"), NOW)
            .is_ok()
    );
}

#[test]
fn a_sign_in_makes_a_session_that_a_request_can_present_and_sign_out_ends() {
    let w = world();
    let signed = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    assert_eq!(signed.principal, Principal::session(id(&w.alice)));
    let cookie = signed.session.set_cookie(NOW);
    let header = format!("theme=dark; {}", cookie.split(';').next().unwrap());
    let secret = secret_from_cookie_header(&header).expect("the cookie holds a well-formed secret");

    let found = w
        .auth
        .principal_for_session(secret, NOW + 60)
        .unwrap()
        .expect("a live session");
    assert_eq!(found.principal, Principal::session(id(&w.alice)));
    assert!(found.csrf.matches(signed.session.csrf().as_str()));
    assert_eq!(found.expires_at, NOW + 7 * 24 * 3600);

    w.auth.sign_out(secret).unwrap();
    assert!(
        w.auth
            .principal_for_session(secret, NOW + 61)
            .unwrap()
            .is_none()
    );
    assert!(cookie_value(&header, "theme").is_some());
}

#[test]
fn every_sign_in_is_a_new_session_and_a_session_ends_when_its_time_does() {
    let w = world();
    let a = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let b = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    assert_ne!(a.session.secret(), b.session.secret());
    let end = a.session.expires_at();
    assert!(
        w.auth
            .principal_for_session(a.session.secret().as_str(), end - 1)
            .unwrap()
            .is_some()
    );
    assert!(
        w.auth
            .principal_for_session(a.session.secret().as_str(), end)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_new_password_ends_every_session_of_the_account_and_the_old_one_stops_working() {
    let w = world();
    let s = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    w.auth
        .set_password(&w.alice.id, "alice", "a brand new password")
        .unwrap();
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 1)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.2"), NOW + 2),
        Err(AuthError::BadCredentials)
    ));
    assert!(
        w.auth
            .sign_in("alice", "a brand new password", &ip("203.0.113.3"), NOW + 3)
            .is_ok()
    );
}

#[test]
fn a_disabled_account_has_no_session_no_token_and_no_sign_in() {
    let w = world();
    let s = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let t = w
        .auth
        .issue_token(&w.alice.id, Scope::Read, "ci", 3_600, NOW)
        .unwrap();
    w.auth.disable(&w.alice.id).unwrap();
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 1)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        w.auth.principal_for_token(t.secret.as_str(), NOW + 1),
        Err(AuthError::BadCredentials)
    );
    assert!(matches!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.2"), NOW + 2),
        Err(AuthError::BadCredentials)
    ));
    w.auth.enable(&w.alice.id).unwrap();
    assert!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.3"), NOW + 3)
            .is_ok()
    );
    assert!(
        w.auth
            .principal_for_token(t.secret.as_str(), NOW + 4)
            .is_ok(),
        "the token works again"
    );
}

#[test]
fn the_address_comes_from_a_trusted_proxy_only() {
    let mut cfg = config();
    cfg.proxies = inklkunn_auth::ProxyPolicy::trusting(&["10.0.0.0/8"]).unwrap();
    let w = world_with(cfg);
    let peer = "10.0.0.5".parse().unwrap();
    assert_eq!(
        w.auth.client_address(peer, Some("198.51.100.7")).as_key(),
        "198.51.100.7"
    );
    let stranger = "192.0.2.9".parse().unwrap();
    assert_eq!(
        w.auth
            .client_address(stranger, Some("198.51.100.7"))
            .as_key(),
        "192.0.2.9"
    );
}

#[test]
fn passwords_names_and_display_names_are_checked_when_an_account_is_made() {
    use inklkunn_auth::authenticator::AccountError;
    let w = world();
    let r = |login: &str, name: &str, pw: &str| {
        w.auth.register_local(login, name, pw, NOW).unwrap_err()
    };
    assert!(matches!(
        r("al", "A", "long enough password"),
        AccountError::Login(_)
    ));
    assert!(matches!(
        r("new-one", "line\nbreak", "long enough password"),
        AccountError::DisplayName(_)
    ));
    assert!(matches!(
        r("new-one", "Name", "short"),
        AccountError::Password(_)
    ));
    assert_eq!(
        r("alice", "Another Alice", "long enough password"),
        AccountError::LoginTaken
    );
    assert_eq!(
        r("ALICE", "Another Alice", "long enough password"),
        AccountError::LoginTaken
    );
    // Nothing half-made was left behind.
    assert!(
        w.auth
            .sign_in("new-one", "long enough password", &ip("203.0.113.1"), NOW)
            .is_err()
    );
}

#[test]
fn no_debug_output_carries_a_secret() {
    let w = world();
    let signed = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let t = w
        .auth
        .issue_token(&w.alice.id, Scope::Read, "ci", 3_600, NOW)
        .unwrap();
    let shown = format!("{signed:?} {t:?} {:?} {:?}", w.auth, w.alice);
    for secret in [
        signed.session.secret().as_str(),
        signed.session.csrf().as_str(),
        t.secret.as_str(),
        ALICE_PASSWORD,
    ] {
        assert!(!shown.contains(secret), "{shown}");
    }
    assert!(!shown.contains("argon2id"), "no password hash either");
}
