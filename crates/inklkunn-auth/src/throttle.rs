//! Failed sign-ins, counted (threat model T-2, C-2b): after `max_failures` consecutive failures for one key
//! the key is **locked** for `lock_secs`, and a correct password during the lock is refused too. A success
//! clears the count. Two throttles run side by side: one keyed by the **sign-in name as typed** (so an
//! attacker cannot tell a real name from a made-up one by whether it locks) and one by the **client
//! address** (so one client cannot spread guesses over many names).
//!
//! The tables are bounded: a flood of made-up keys cannot grow memory without limit. When one is full the
//! entries that mean nothing (not locked, not seen within the window) go first, then the lock that ends
//! soonest. State is per process (residual risk RR-7): a restart forgets it, as a lock that long is a nuisance
//! rather than a defence.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// How a throttle counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThrottlePolicy {
    /// Consecutive failures that lock a key.
    pub max_failures: u32,
    /// How long a lock lasts, in seconds.
    pub lock_secs: i64,
    /// A key not seen for this long starts again from zero failures, in seconds.
    pub forget_secs: i64,
    /// The most keys tracked at once.
    pub max_tracked: usize,
}

impl ThrottlePolicy {
    /// Per sign-in name: 5 failures lock it for 15 minutes.
    pub const PER_LOGIN: ThrottlePolicy = ThrottlePolicy {
        max_failures: 5,
        lock_secs: 15 * 60,
        forget_secs: 15 * 60,
        max_tracked: 10_000,
    };

    /// Per client address: 30 failures lock it for 15 minutes (many people can sit behind one address).
    pub const PER_ADDRESS: ThrottlePolicy = ThrottlePolicy {
        max_failures: 30,
        lock_secs: 15 * 60,
        forget_secs: 15 * 60,
        max_tracked: 10_000,
    };
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    failures: u32,
    locked_until: i64,
    last_seen: i64,
}

/// Failures per key, with lockout.
#[derive(Debug)]
pub struct Throttle {
    policy: ThrottlePolicy,
    state: Mutex<HashMap<String, Entry>>,
}

impl Throttle {
    /// A throttle with a policy (`max_failures` and `max_tracked` are at least 1).
    pub fn new(policy: ThrottlePolicy) -> Self {
        Throttle {
            policy: ThrottlePolicy {
                max_failures: policy.max_failures.max(1),
                max_tracked: policy.max_tracked.max(1),
                ..policy
            },
            state: Mutex::new(HashMap::new()),
        }
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// May this key attempt a sign-in at `now`? `Err(until)` while it is locked.
    pub fn check(&self, key: &str, now: i64) -> Result<(), i64> {
        match self.map().get(key) {
            Some(e) if e.locked_until > now => Err(e.locked_until),
            _ => Ok(()),
        }
    }

    /// Count a failure; `Some(until)` when this one locked the key.
    pub fn record_failure(&self, key: &str, now: i64) -> Option<i64> {
        let mut map = self.map();
        if !map.contains_key(key) && map.len() >= self.policy.max_tracked {
            self.make_room(&mut map, now);
        }
        let e = map.entry(key.to_owned()).or_insert(Entry {
            failures: 0,
            locked_until: 0,
            last_seen: now,
        });
        // A lock that ended, or a key not seen for the window, starts again.
        if (e.locked_until != 0 && e.locked_until <= now)
            || now.saturating_sub(e.last_seen) >= self.policy.forget_secs
        {
            e.failures = 0;
            e.locked_until = 0;
        }
        e.failures = e.failures.saturating_add(1);
        e.last_seen = now;
        if e.failures >= self.policy.max_failures {
            e.locked_until = now.saturating_add(self.policy.lock_secs);
            e.failures = 0;
            Some(e.locked_until)
        } else {
            None
        }
    }

    /// A successful sign-in clears the key's count.
    pub fn record_success(&self, key: &str) {
        self.map().remove(key);
    }

    fn make_room(&self, map: &mut HashMap<String, Entry>, now: i64) {
        map.retain(|_, e| {
            e.locked_until > now || now.saturating_sub(e.last_seen) < self.policy.forget_secs
        });
        if map.len() < self.policy.max_tracked {
            return;
        }
        // Still full: the key whose hold on us ends soonest (an unlocked key counts as 0, so it goes first),
        // the least recently seen among equals.
        let victim = map
            .iter()
            .min_by_key(|(_, e)| (e.locked_until.max(0), e.last_seen))
            .map(|(k, _)| k.clone());
        if let Some(victim) = victim {
            map.remove(&victim);
        }
    }

    /// How many keys are tracked (tests, and a gauge later).
    pub fn tracked(&self) -> usize {
        self.map().len()
    }
}

#[cfg(test)]
mod tests;
