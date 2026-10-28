//! Password sign-in (RFC 002 D-1): constant work, throttles counted before they are judged, the gate,
//! sessions, CSRF, credential recovery.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier, Mutex, mpsc};
use std::time::{Duration, Instant};

use common::*;
use inklkunn_auth::session::{cookie_value, secret_from_cookie_header};
use inklkunn_auth::{
    AccountError, AuthConfig, AuthError, Authenticator, PasswordHasher, SessionCheck,
};
use inklkunn_core::Principal;
use inklkunn_store::{
    Account, AccountId, AccountStore, DisplayName, LocalCredential, LoginName, MemoryAudit,
    MemoryStore, Scope,
};

fn cost_of(phc: &str) -> String {
    phc.split('$').nth(3).unwrap_or_default().to_owned()
}

#[test]
fn the_dummy_hash_is_made_once_at_start_and_costs_what_a_real_hash_costs() {
    let w = world();
    // One hash for the dummy, one each for the two registered accounts.
    assert_eq!(w.hasher.hashes.load(Ordering::SeqCst), 3);
    let alice = w.store.account(&w.alice.id).unwrap().unwrap();
    let real = alice.local.unwrap().password_hash;
    assert_eq!(
        cost_of(w.auth.dummy_hash_for_test()),
        cost_of(&real),
        "a dummy cheaper than a real hash would tell a name that exists from one that does not"
    );
}

#[test]
fn every_sign_in_that_reaches_the_hasher_does_exactly_one_verification() {
    let w = world();
    let c = ip("203.0.113.1");
    let carol = w
        .auth
        .register_local("carol", "Carol", "carol has a long one", NOW)
        .unwrap();
    w.auth.disable(&carol.id, NOW).unwrap();
    // A row whose hash is well shaped but asks for more than the ceiling allows (a damaged or hostile row):
    // it cannot be verified, so a dummy is verified in its place.
    w.store
        .add_account(&Account {
            id: AccountId::new(),
            display_name: DisplayName::new("Dave").unwrap(),
            local: Some(LocalCredential {
                login: LoginName::parse("dave").unwrap(),
                password_hash:
                    "$argon2id$v=19$m=4194304,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNo"
                        .to_owned(),
            }),
            created_at: NOW,
            disabled: false,
        })
        .unwrap();

    let cases: Vec<(&str, &str, &str, bool)> = vec![
        ("the right password", "alice", ALICE_PASSWORD, true),
        ("a wrong password", "alice", "not the password", false),
        (
            "the right password, name in capitals",
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
        (
            "an account whose stored hash cannot be verified",
            "dave",
            "anything at all",
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
fn an_oversized_password_is_refused_without_hashing_and_it_counts_as_a_failure() {
    let w = world();
    let huge = "a".repeat(2_000);
    let before = w.hasher.verifies();
    for i in 0..5 {
        let r = w.auth.sign_in(
            "alice",
            &huge,
            &ip(&format!("203.0.113.{}", i + 1)),
            NOW + i,
        );
        assert!(matches!(r, Err(AuthError::BadCredentials)), "{r:?}");
    }
    assert_eq!(w.hasher.verifies(), before, "nothing was hashed");
    // Five failures lock the name, so the sixth attempt is refused even with the right password.
    assert!(matches!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.99"), NOW + 10),
        Err(AuthError::Locked { .. })
    ));
}

#[test]
fn unknown_account_and_wrong_password_cost_the_same_within_noise() {
    // The real Argon2id at a cost that dominates a map lookup. Every sample is a first attempt under its own
    // name and address, so each is really judged: no throttle shortcut can make a sample fast.
    let store = Arc::new(MemoryStore::new());
    let hasher = Arc::new(inklkunn_auth::Argon2idHasher::with_cost(4096, 2, 1).unwrap());
    let auth = Authenticator::with_hasher(store, config(), hasher).unwrap();
    for i in 0..7 {
        auth.register_local(&format!("user{i}"), "A user", ALICE_PASSWORD, NOW)
            .unwrap();
    }
    let sample = |login: String, i: usize| {
        let c = ip(&format!("198.51.100.{}", i + 1));
        let started = Instant::now();
        let r = auth.sign_in(&login, "some password here", &c, NOW);
        assert!(
            matches!(r, Err(AuthError::BadCredentials)),
            "{login}: {r:?}"
        );
        started.elapsed()
    };
    let median = |mut v: Vec<Duration>| {
        v.sort();
        v.get(v.len() / 2).copied().unwrap()
    };
    let unknown = median((0..7).map(|i| sample(format!("ghost{i}"), i)).collect());
    let wrong = median((0..7).map(|i| sample(format!("user{i}"), i + 10)).collect());
    assert!(
        unknown > Duration::from_micros(500),
        "{unknown:?}: a sample that cheap was not hashed"
    );
    assert!(wrong > Duration::from_micros(500), "{wrong:?}");
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
fn a_burst_of_simultaneous_wrong_guesses_is_held_to_the_limit() {
    // Sixty guesses at one name at the same instant, each from its own address. Counting after the work
    // would let every one of them through; counted first, five are judged and the rest are turned away.
    let w = Arc::new(world_with(AuthConfig {
        gate_wait: Duration::from_secs(10),
        ..config()
    }));
    let start = Arc::new(Barrier::new(60));
    let before = w.hasher.verifies();
    let handles: Vec<_> = (0..60)
        .map(|i| {
            let (w, start) = (Arc::clone(&w), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                w.auth.sign_in(
                    "alice",
                    "wrong wrong wrong",
                    &ip(&format!("203.0.{}.{}", i / 200, i % 200 + 1)),
                    NOW,
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let judged = results
        .iter()
        .filter(|r| matches!(r, Err(AuthError::BadCredentials)))
        .count();
    let locked = results
        .iter()
        .filter(|r| matches!(r, Err(AuthError::Locked { .. })))
        .count();
    assert_eq!((judged, locked), (5, 55), "{results:?}");
    assert_eq!(
        w.hasher.verifies() - before,
        5,
        "only the admitted five were hashed"
    );
    // And the name is locked for a correct password too, queued or not.
    assert!(matches!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("198.18.0.1"), NOW + 1),
        Err(AuthError::Locked { .. })
    ));
}

#[test]
fn a_burst_from_one_address_across_many_names_is_held_to_the_limit() {
    let w = Arc::new(world_with(AuthConfig {
        gate_wait: Duration::from_secs(10),
        ..config()
    }));
    let start = Arc::new(Barrier::new(120));
    let before = w.hasher.verifies();
    let handles: Vec<_> = (0..120)
        .map(|i| {
            let (w, start) = (Arc::clone(&w), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                w.auth.sign_in(
                    &format!("nobody{i}"),
                    "wrong wrong wrong",
                    &ip("203.0.113.77"),
                    NOW,
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let judged = results
        .iter()
        .filter(|r| matches!(r, Err(AuthError::BadCredentials)))
        .count();
    assert_eq!(judged, 30, "{judged} judged");
    assert_eq!(w.hasher.verifies() - before, 30);
}

#[test]
fn a_busy_gate_says_busy_gives_the_attempt_back_and_decides_nothing_about_the_credential() {
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
        fn is_verifiable(&self, s: &str) -> bool {
            self.inner.is_verifiable(s)
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
        inner: inklkunn_auth::Argon2idHasher::with_cost(64, 1, 1).unwrap(),
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
                gate_wait: Duration::from_millis(50),
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
    // The only permit is held. More Busy answers than the name's limit of five, and than... any address's:
    // had a Busy been counted, the name would be locked after the fifth.
    for i in 0..8 {
        let started = Instant::now();
        let r = auth.sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.2"), NOW + i);
        assert_eq!(r.unwrap_err(), AuthError::Busy, "attempt {i}");
        assert!(started.elapsed() >= Duration::from_millis(40));
    }
    release_tx.send(()).unwrap();
    assert!(
        first.join().unwrap().is_ok(),
        "the one holding the permit finished normally"
    );
    hasher.hold.store(false, Ordering::SeqCst);
    assert!(
        auth.sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.3"), NOW + 20)
            .is_ok(),
        "eight Busy answers locked nothing"
    );
}

#[test]
fn a_lock_is_written_to_the_audit_trail_without_a_name_or_an_address() {
    let audit = Arc::new(MemoryAudit::new());
    let w = world_built(config(), |a| a.with_audit(audit.clone()));
    for i in 0..5 {
        let _ = w.auth.sign_in(
            "alice",
            "wrong wrong wrong",
            &ip(&format!("203.0.113.{}", i + 1)),
            NOW + i,
        );
    }
    for i in 0..30 {
        let _ = w.auth.sign_in(
            &format!("user{i}"),
            "wrong wrong wrong",
            &ip("198.51.100.9"),
            NOW + 100 + i,
        );
    }
    let events = audit.events();
    let locks: Vec<_> = events
        .iter()
        .filter(|e| e.action == "sign-in-lock")
        .collect();
    assert_eq!(locks.len(), 2, "{events:?}");
    assert!(locks.iter().any(|e| e.resource == "login-throttle"));
    assert!(locks.iter().any(|e| e.resource == "address-throttle"));
    let shown = format!("{events:?}");
    assert!(
        !shown.contains("alice") && !shown.contains("198.51.100.9") && !shown.contains("203.0.113"),
        "{shown}"
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
fn a_request_that_changes_something_cannot_skip_the_csrf_check() {
    let w = world();
    let signed = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let secret = signed.session.secret().as_str();
    let csrf = signed.session.csrf().as_str().to_owned();
    assert!(matches!(
        w.auth.principal_for_write(secret, &csrf, NOW + 1).unwrap(),
        SessionCheck::Valid(_)
    ));
    for wrong in ["", "forged", &csrf[..10]] {
        assert!(
            matches!(
                w.auth.principal_for_write(secret, wrong, NOW + 1).unwrap(),
                SessionCheck::CsrfMismatch
            ),
            "{wrong:?}"
        );
    }
    // Another session's token is not this session's.
    let other = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.2"), NOW)
        .unwrap();
    assert!(matches!(
        w.auth
            .principal_for_write(secret, other.session.csrf().as_str(), NOW + 1)
            .unwrap(),
        SessionCheck::CsrfMismatch
    ));
    let gone = "a".repeat(43);
    assert!(matches!(
        w.auth.principal_for_write(&gone, &csrf, NOW + 1).unwrap(),
        SessionCheck::SignedOut
    ));
    w.auth.sign_out(secret).unwrap();
    assert!(matches!(
        w.auth.principal_for_write(secret, &csrf, NOW + 2).unwrap(),
        SessionCheck::SignedOut
    ));
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
fn an_administrators_reset_ends_every_session_and_revokes_every_token() {
    let w = world();
    let s = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let t = w
        .auth
        .issue_token(&w.alice.id, Scope::Admin, "ci", 3_600, NOW)
        .unwrap();
    assert!(
        w.auth
            .principal_for_token(t.secret.as_str(), NOW + 1)
            .is_ok()
    );
    w.auth
        .reset_password(&w.alice.id, "alice", "a brand new password", NOW + 2)
        .unwrap();
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 3)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        w.auth.principal_for_token(t.secret.as_str(), NOW + 3),
        Err(AuthError::BadCredentials)
    );
    assert!(matches!(
        w.auth
            .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.2"), NOW + 4),
        Err(AuthError::BadCredentials)
    ));
    assert!(
        w.auth
            .sign_in("alice", "a brand new password", &ip("203.0.113.3"), NOW + 5)
            .is_ok()
    );
}

#[test]
fn changing_ones_own_password_proves_the_old_one_is_throttled_and_ends_everything() {
    let w = world();
    let c = ip("203.0.113.1");
    let s = w.auth.sign_in("alice", ALICE_PASSWORD, &c, NOW).unwrap();
    let t = w
        .auth
        .issue_token(&w.alice.id, Scope::Propose, "ci", 3_600, NOW)
        .unwrap();
    // The wrong old password changes nothing.
    let r = w.auth.change_password(
        "alice",
        "not my password",
        "a brand new password",
        &c,
        NOW + 1,
    );
    assert_eq!(
        r.unwrap_err(),
        AccountError::Auth(AuthError::BadCredentials)
    );
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 2)
            .unwrap()
            .is_some()
    );
    // A new password that is not acceptable changes nothing either.
    let r = w
        .auth
        .change_password("alice", ALICE_PASSWORD, "short", &c, NOW + 3);
    assert!(matches!(r, Err(AccountError::Password(_))), "{r:?}");
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 4)
            .unwrap()
            .is_some()
    );
    // The right one changes it, and ends the session and the token, the changer's own included.
    w.auth
        .change_password("alice", ALICE_PASSWORD, "a brand new password", &c, NOW + 5)
        .unwrap();
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 6)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        w.auth.principal_for_token(t.secret.as_str(), NOW + 6),
        Err(AuthError::BadCredentials)
    );
    assert!(
        w.auth
            .sign_in("alice", "a brand new password", &ip("203.0.113.9"), NOW + 7)
            .is_ok()
    );
}

#[test]
fn changing_a_password_is_throttled_like_signing_in() {
    let w = world();
    for i in 0..5 {
        let r = w.auth.change_password(
            "alice",
            "wrong wrong wrong",
            "a brand new password",
            &ip(&format!("203.0.113.{}", i + 1)),
            NOW + i,
        );
        assert_eq!(
            r.unwrap_err(),
            AccountError::Auth(AuthError::BadCredentials)
        );
    }
    let r = w.auth.change_password(
        "alice",
        ALICE_PASSWORD,
        "a brand new password",
        &ip("203.0.113.99"),
        NOW + 9,
    );
    assert!(
        matches!(r, Err(AccountError::Auth(AuthError::Locked { .. }))),
        "{r:?}"
    );
}

#[test]
fn reauthenticating_checks_a_password_without_starting_a_session() {
    let w = world();
    let c = ip("203.0.113.1");
    assert_eq!(
        w.auth
            .reauthenticate("alice", ALICE_PASSWORD, &c, NOW)
            .unwrap(),
        id(&w.alice)
    );
    assert_eq!(
        w.auth.reauthenticate("alice", "wrong", &c, NOW + 1),
        Err(AuthError::BadCredentials)
    );
    // It is a sign-in's cousin in every way that matters: counted, so a script cannot use it as an oracle.
    for i in 0..5 {
        let _ = w.auth.reauthenticate(
            "bob",
            "wrong wrong wrong",
            &ip(&format!("203.0.113.{}", i + 20)),
            NOW + i,
        );
    }
    assert!(matches!(
        w.auth
            .reauthenticate("bob", "another long password", &ip("203.0.113.90"), NOW + 9),
        Err(AuthError::Locked { .. })
    ));
}

#[test]
fn a_disabled_account_has_no_session_no_token_and_no_sign_in_and_enabling_does_not_bring_them_back()
{
    let w = world();
    let s = w
        .auth
        .sign_in("alice", ALICE_PASSWORD, &ip("203.0.113.1"), NOW)
        .unwrap();
    let t = w
        .auth
        .issue_token(&w.alice.id, Scope::Read, "ci", 3_600, NOW)
        .unwrap();
    w.auth.disable(&w.alice.id, NOW + 1).unwrap();
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 2)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        w.auth.principal_for_token(t.secret.as_str(), NOW + 2),
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
    assert_eq!(
        w.auth.principal_for_token(t.secret.as_str(), NOW + 4),
        Err(AuthError::BadCredentials),
        "the token was revoked by the disabling; enabling is not a restore"
    );
    assert!(
        w.auth
            .principal_for_session(s.session.secret().as_str(), NOW + 4)
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_address_comes_from_a_trusted_proxy_only_and_a_failure_to_read_it_is_visible() {
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
    let unreadable = w.auth.client_address(peer, Some("198.51.100.7:5555"));
    assert_eq!(unreadable.as_key(), "10.0.0.5");
    assert!(
        unreadable.fell_back(),
        "every client behind the proxy now shares one budget; say so"
    );
}

#[test]
fn passwords_names_and_display_names_are_checked_when_an_account_is_made() {
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
fn a_configuration_that_cannot_work_is_refused_at_start() {
    let store = || Arc::new(MemoryStore::new());
    let cheap = || Arc::new(inklkunn_auth::Argon2idHasher::with_cost(64, 1, 1).unwrap());
    let refused = |cfg: AuthConfig| {
        Authenticator::with_hasher(store(), cfg, cheap())
            .map(|_| ())
            .unwrap_err()
    };
    for ttl in [0, -1, 59, 90 * 24 * 3600 + 1, i64::MAX] {
        assert!(
            matches!(
                refused(AuthConfig {
                    session_ttl_secs: ttl,
                    ..config()
                }),
                AuthError::Misconfigured(_)
            ),
            "{ttl}"
        );
    }
    assert!(matches!(
        refused(AuthConfig {
            hash_permits: 0,
            ..config()
        }),
        AuthError::Misconfigured(_)
    ));
    for ttl in [60, 90 * 24 * 3600] {
        assert!(
            Authenticator::with_hasher(
                store(),
                AuthConfig {
                    session_ttl_secs: ttl,
                    ..config()
                },
                cheap()
            )
            .is_ok()
        );
    }
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
