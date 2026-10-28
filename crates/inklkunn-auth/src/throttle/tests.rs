#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Barrier};

use super::*;

fn policy(max_failures: u32, lock_secs: i64) -> ThrottlePolicy {
    ThrottlePolicy {
        max_failures,
        lock_secs,
        forget_secs: lock_secs,
        max_tracked: 10_000,
    }
}

const FREE: Begin = Begin::Admitted { locks_now: None };

#[test]
fn a_key_locks_at_its_limit_and_the_lock_ends() {
    let t = Throttle::new(policy(3, 100));
    assert_eq!(t.begin("alice", 1), FREE);
    assert_eq!(t.begin("alice", 2), FREE);
    assert_eq!(
        t.begin("alice", 3),
        Begin::Admitted {
            locks_now: Some(103)
        },
        "the third attempt is admitted and engages the lock"
    );
    assert_eq!(
        t.begin("alice", 50),
        Begin::Locked(103),
        "a correct password would be refused now"
    );
    assert_eq!(t.begin("alice", 102), Begin::Locked(103));
    assert_eq!(
        t.begin("alice", 103),
        FREE,
        "the lock ended and a fresh count started"
    );
    assert_eq!(t.begin("bob", 50), FREE, "other keys are unaffected");
}

#[test]
fn a_refused_attempt_is_not_counted() {
    let t = Throttle::new(policy(2, 100));
    t.begin("k", 1);
    t.begin("k", 2); // locked until 102
    for now in 3..90 {
        assert_eq!(t.begin("k", now), Begin::Locked(102));
    }
    // Hammering a locked key did not extend or deepen the lock.
    assert_eq!(t.begin("k", 102), FREE);
    assert_eq!(
        t.begin("k", 103),
        Begin::Admitted {
            locks_now: Some(203)
        }
    );
}

#[test]
fn a_success_clears_the_key_and_a_refund_gives_back_one_attempt() {
    let t = Throttle::new(policy(3, 100));
    t.begin("carol", 1);
    t.begin("carol", 2);
    t.clear("carol");
    assert_eq!(t.begin("carol", 3), FREE);
    assert_eq!(t.begin("carol", 4), FREE);

    // A refund takes back the one attempt it belongs to and nothing more.
    let u = Throttle::new(policy(3, 100));
    u.begin("dave", 1); // count 1
    u.begin("dave", 2); // count 2
    u.refund("dave"); // an attempt that turned out right: count 1
    assert_eq!(u.begin("dave", 3), FREE); // count 2
    assert_eq!(
        u.begin("dave", 4),
        Begin::Admitted {
            locks_now: Some(104)
        }
    ); // count 3: the lock
    u.refund("nobody-by-that-name");
    assert_eq!(u.tracked(), 1);
}

#[test]
fn a_refund_of_the_only_attempt_leaves_no_trace() {
    let t = Throttle::new(policy(5, 100));
    t.begin("x", 1);
    t.refund("x");
    assert_eq!(t.tracked(), 0);
}

#[test]
fn attempts_far_apart_do_not_add_up() {
    let t = Throttle::new(policy(3, 100));
    t.begin("dave", 0);
    t.begin("dave", 10);
    // Not seen for the window: the earlier two are forgotten.
    assert_eq!(t.begin("dave", 500), FREE);
    assert_eq!(t.begin("dave", 501), FREE);
    assert_eq!(
        t.begin("dave", 502),
        Begin::Admitted {
            locks_now: Some(602)
        }
    );
}

#[test]
fn a_burst_of_simultaneous_attempts_admits_no_more_than_the_limit() {
    // The defect this design closes: sixty threads all pass a check before any of them is recorded.
    let t = Arc::new(Throttle::new(policy(5, 1_000)));
    let start = Arc::new(Barrier::new(60));
    let handles: Vec<_> = (0..60)
        .map(|_| {
            let (t, start) = (Arc::clone(&t), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                t.begin("alice", 10)
            })
        })
        .collect();
    let results: Vec<Begin> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let admitted = results
        .iter()
        .filter(|r| matches!(r, Begin::Admitted { .. }))
        .count();
    assert_eq!(
        admitted, 5,
        "exactly the limit gets through, however many ask at once"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Begin::Locked(_)))
            .count(),
        55
    );
}

#[test]
fn the_table_is_bounded_and_a_real_lock_outlives_a_flood_of_noise() {
    let t = Throttle::new(ThrottlePolicy {
        max_failures: 2,
        lock_secs: 1_000,
        forget_secs: 1_000,
        max_tracked: 50,
    });
    t.begin("victim", 1);
    t.begin("victim", 2); // locked until 1002
    for i in 0..500 {
        t.begin(&format!("noise-{i}"), 3);
        assert!(t.tracked() <= 50);
    }
    assert_eq!(
        t.begin("victim", 10),
        Begin::Locked(1002),
        "the lock was not pushed out by unlocked noise"
    );
}

#[test]
fn a_full_table_of_locks_gives_way_to_the_one_that_ends_soonest() {
    let t = Throttle::new(ThrottlePolicy {
        max_failures: 1,
        lock_secs: 100,
        forget_secs: 100,
        max_tracked: 3,
    });
    t.begin("a", 1); // until 101
    t.begin("b", 2); // until 102
    t.begin("c", 3); // until 103
    t.begin("d", 4); // table full of live locks: "a" (soonest) goes
    assert_eq!(t.tracked(), 3);
    assert_eq!(t.begin("b", 5), Begin::Locked(102));
    assert_eq!(t.begin("d", 5), Begin::Locked(104));
}

#[test]
fn a_policy_cannot_be_zero() {
    let t = Throttle::new(ThrottlePolicy {
        max_failures: 0,
        lock_secs: 10,
        forget_secs: 10,
        max_tracked: 0,
    });
    assert_eq!(
        t.begin("x", 1),
        Begin::Admitted {
            locks_now: Some(11)
        },
        "a policy of no failures is a policy of one"
    );
    assert!(t.tracked() <= 1);
}
