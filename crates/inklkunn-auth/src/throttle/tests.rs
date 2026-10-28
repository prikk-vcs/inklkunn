#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn policy(max_failures: u32, lock_secs: i64) -> ThrottlePolicy {
    ThrottlePolicy {
        max_failures,
        lock_secs,
        forget_secs: lock_secs,
        max_tracked: 10_000,
    }
}

#[test]
fn a_key_locks_after_its_failures_and_the_lock_ends() {
    let t = Throttle::new(policy(3, 100));
    assert!(t.check("alice", 0).is_ok());
    assert_eq!(t.record_failure("alice", 1), None);
    assert_eq!(t.record_failure("alice", 2), None);
    assert_eq!(
        t.record_failure("alice", 3),
        Some(103),
        "the third failure locks"
    );
    assert_eq!(
        t.check("alice", 50),
        Err(103),
        "a correct password would be refused now"
    );
    assert_eq!(t.check("alice", 102), Err(103));
    assert!(t.check("alice", 103).is_ok(), "the lock ended");
    assert_eq!(
        t.record_failure("alice", 104),
        None,
        "a fresh count, not an immediate re-lock"
    );
    assert!(t.check("bob", 50).is_ok(), "other keys are unaffected");
}

#[test]
fn a_success_clears_the_count() {
    let t = Throttle::new(policy(3, 100));
    t.record_failure("carol", 1);
    t.record_failure("carol", 2);
    t.record_success("carol");
    assert_eq!(t.record_failure("carol", 3), None);
    assert_eq!(t.record_failure("carol", 4), None);
}

#[test]
fn failures_far_apart_do_not_add_up() {
    let t = Throttle::new(policy(3, 100));
    t.record_failure("dave", 0);
    t.record_failure("dave", 10);
    // Not seen for the window: the earlier two are forgotten.
    assert_eq!(t.record_failure("dave", 500), None);
    assert_eq!(t.record_failure("dave", 501), None);
    assert_eq!(t.record_failure("dave", 502), Some(602));
}

#[test]
fn the_table_is_bounded_and_a_real_lock_outlives_a_flood_of_noise() {
    let t = Throttle::new(ThrottlePolicy {
        max_failures: 2,
        lock_secs: 1_000,
        forget_secs: 1_000,
        max_tracked: 50,
    });
    t.record_failure("victim", 1);
    t.record_failure("victim", 2); // locked until 1002
    for i in 0..500 {
        t.record_failure(&format!("noise-{i}"), 3);
        assert!(t.tracked() <= 50);
    }
    assert_eq!(
        t.check("victim", 10),
        Err(1002),
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
    t.record_failure("a", 1); // until 101
    t.record_failure("b", 2); // until 102
    t.record_failure("c", 3); // until 103
    t.record_failure("d", 4); // table full of live locks: "a" (soonest) goes
    assert_eq!(t.tracked(), 3);
    assert!(t.check("a", 5).is_ok());
    assert_eq!(t.check("b", 5), Err(102));
    assert_eq!(t.check("d", 5), Err(104));
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
        t.record_failure("x", 1),
        Some(11),
        "a policy of no failures is a policy of one"
    );
    assert!(t.tracked() <= 1);
}
