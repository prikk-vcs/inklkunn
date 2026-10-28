//! The bounded gate in front of password verification (RFC 002 D-1): **every verification takes one permit**,
//! and there are few. Argon2id is heavy on purpose (19 MiB and tens of milliseconds a call), so unbounded
//! concurrent sign-ins would be an unbounded memory and CPU bill for anyone who can send requests. A caller
//! that cannot get a permit within the wait is told the service is busy; it is never told anything about the
//! credential.
//!
//! The wait blocks the calling thread, which is why this crate is called from a blocking context and never
//! from an async worker.

use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// A fixed number of permits.
#[derive(Debug)]
pub struct WorkGate {
    free: Mutex<usize>,
    freed: Condvar,
    max_wait: Duration,
}

/// A permit; dropping it returns it to the gate.
#[derive(Debug)]
pub struct Permit<'a> {
    gate: &'a WorkGate,
}

impl WorkGate {
    /// A gate with `permits` permits (at least one) and the longest a caller waits for one.
    pub fn new(permits: usize, max_wait: Duration) -> Self {
        WorkGate {
            free: Mutex::new(permits.max(1)),
            freed: Condvar::new(),
            max_wait,
        }
    }

    /// Take a permit, waiting up to the gate's wait; `None` when none came free in time.
    pub fn acquire(&self) -> Option<Permit<'_>> {
        let deadline = Instant::now() + self.max_wait;
        let mut free = self.free.lock().unwrap_or_else(PoisonError::into_inner);
        while *free == 0 {
            let left = deadline.checked_duration_since(Instant::now())?;
            let (guard, timeout) = self
                .freed
                .wait_timeout(free, left)
                .unwrap_or_else(PoisonError::into_inner);
            free = guard;
            if timeout.timed_out() && *free == 0 {
                return None;
            }
        }
        *free -= 1;
        Some(Permit { gate: self })
    }

    /// Permits not in use right now.
    pub fn available(&self) -> usize {
        *self.free.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut free = self
            .gate
            .free
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *free += 1;
        self.gate.freed.notify_one();
    }
}

#[cfg(test)]
mod tests;
