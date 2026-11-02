//! Logins in flight: what a `state` stands for until the provider sends the browser back. Held on the
//! server (the browser carries only the opaque `state`), **one-shot** (redeeming removes it, so a replayed
//! callback finds nothing), and short-lived. Each remembers the client address that started it, which gets its
//! start back when the login succeeds (corrections G2) and is bounded in what it may leave in flight.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// How long a login may take, in seconds.
pub const PENDING_TTL_SECS: i64 = 600;
/// The most logins in flight. When full, **the oldest of the client address with the most in flight goes**: a
/// flood of unfinished logins costs an attacker's own requests, cannot grow memory, and (with the per-address
/// bound below) cannot push out the logins of people who started one.
const MAX_PENDING: usize = 10_000;
/// The most logins one client address may have in flight; one more ends its own oldest. A person starts one at a
/// time; the start throttle (30 per 15 minutes) admits no more than this many within a login's life anyway.
pub(super) const MAX_PENDING_PER_CLIENT: usize = 32;

/// What a login in flight is for. It is kept here, on the server: the browser carries only the opaque `state`,
/// so an invitation's secret and the landing path are never in a cookie, a callback address or the provider's
/// logs. (`Debug` shows no secret.)
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// Sign in, or join where joining through the provider is open. `next` is where an existing account lands,
    /// `after_joining` where a new one does.
    SignIn {
        next: Option<String>,
        after_joining: Option<String>,
    },
    /// Make the account an invitation is for, bound to the provider identity.
    Invitation {
        secret: String,
        next: Option<String>,
    },
}

impl std::fmt::Debug for Purpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Purpose::SignIn { .. } => f.write_str("Purpose::SignIn"),
            Purpose::Invitation { .. } => f.write_str("Purpose::Invitation(…)"),
        }
    }
}

/// What the callback needs.
#[derive(Debug, Clone)]
pub(super) struct Pending {
    pub nonce: String,
    pub verifier: String,
    pub purpose: Purpose,
    /// The key of the client address that started this login (`ClientAddress::as_key`).
    pub client: String,
    created_at: i64,
}

#[derive(Default)]
pub(super) struct PendingLogins {
    inner: Mutex<HashMap<String, Pending>>,
}

impl PendingLogins {
    pub(super) fn insert(
        &self,
        state: String,
        nonce: String,
        verifier: String,
        purpose: Purpose,
        client: &str,
        now: i64,
    ) {
        let mut m = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        m.retain(|_, p| now.saturating_sub(p.created_at) < PENDING_TTL_SECS);
        // One address cannot hold more than its share: its own oldest goes first.
        let mut own: Vec<(&String, i64)> = m
            .iter()
            .filter(|(_, p)| p.client == client)
            .map(|(k, p)| (k, p.created_at))
            .collect();
        if own.len() >= MAX_PENDING_PER_CLIENT {
            own.sort_by_key(|(_, at)| *at);
            let drop: Vec<String> = own
                .iter()
                .take(own.len() + 1 - MAX_PENDING_PER_CLIENT)
                .map(|(k, _)| (*k).clone())
                .collect();
            for key in drop {
                m.remove(&key);
            }
        }
        if m.len() >= MAX_PENDING
            && let Some(victim) = Self::heaviest_clients_oldest(&m)
        {
            m.remove(&victim);
        }
        m.insert(
            state,
            Pending {
                nonce,
                verifier,
                purpose,
                client: client.to_owned(),
                created_at: now,
            },
        );
    }

    /// The oldest login of the client address that has the most in flight (the oldest overall among equals).
    fn heaviest_clients_oldest(m: &HashMap<String, Pending>) -> Option<String> {
        let mut per_client: HashMap<&str, usize> = HashMap::new();
        for p in m.values() {
            *per_client.entry(p.client.as_str()).or_default() += 1;
        }
        m.iter()
            .min_by_key(|(_, p)| {
                (
                    std::cmp::Reverse(per_client.get(p.client.as_str()).copied().unwrap_or(0)),
                    p.created_at,
                )
            })
            .map(|(k, _)| k.clone())
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
