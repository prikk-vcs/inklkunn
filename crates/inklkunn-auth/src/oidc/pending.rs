//! Logins in flight: what a `state` stands for until the provider sends the browser back. Held on the
//! server (the browser carries only the opaque `state`), **one-shot** (redeeming removes it, so a replayed
//! callback finds nothing), and short-lived.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// How long a login may take, in seconds.
pub const PENDING_TTL_SECS: i64 = 600;
/// The most logins in flight. When full the oldest goes: a flood of unfinished logins costs an attacker's
/// own requests and cannot grow memory (the per-address rate limit of the web layer bounds the flood).
const MAX_PENDING: usize = 10_000;

/// What the callback needs.
#[derive(Debug, Clone)]
pub(super) struct Pending {
    pub nonce: String,
    pub verifier: String,
    created_at: i64,
}

#[derive(Default)]
pub(super) struct PendingLogins {
    inner: Mutex<HashMap<String, Pending>>,
}

impl PendingLogins {
    pub(super) fn insert(&self, state: String, nonce: String, verifier: String, now: i64) {
        let mut m = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        m.retain(|_, p| now.saturating_sub(p.created_at) < PENDING_TTL_SECS);
        if m.len() >= MAX_PENDING
            && let Some(oldest) = m
                .iter()
                .min_by_key(|(_, p)| p.created_at)
                .map(|(k, _)| k.clone())
        {
            m.remove(&oldest);
        }
        m.insert(
            state,
            Pending {
                nonce,
                verifier,
                created_at: now,
            },
        );
    }

    /// Redeem a state: once, and only while it is young.
    pub(super) fn take(&self, state: &str, now: i64) -> Option<Pending> {
        let p = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(state)?;
        (now.saturating_sub(p.created_at) < PENDING_TTL_SECS).then_some(p)
    }

    #[cfg(test)]
    pub(super) fn peek(&self, state: &str) -> Option<Pending> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(state)
            .cloned()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}
