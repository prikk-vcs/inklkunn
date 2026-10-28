//! Shared by the integration tests: a hasher that counts the work it is asked for, and a world with a few
//! accounts in an in-memory store.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use inklkunn_auth::oidc::OidcProvider;
use inklkunn_auth::{
    Argon2idHasher, AuthConfig, AuthError, Authenticator, ClientAddress, PasswordHasher,
    ProxyPolicy,
};
use inklkunn_store::{Account, AccountId, MemoryStore};

pub const NOW: i64 = 1_800_000_000;
pub const ALICE_PASSWORD: &str = "correct horse battery staple";

/// A cheap real Argon2id that counts every `hash` and `verify` it is asked for.
pub struct Counting {
    inner: Argon2idHasher,
    pub hashes: AtomicUsize,
    pub verifies: AtomicUsize,
}

impl Counting {
    pub fn new() -> Arc<Self> {
        Arc::new(Counting {
            inner: Argon2idHasher::with_cost(64, 1, 1).unwrap(),
            hashes: AtomicUsize::new(0),
            verifies: AtomicUsize::new(0),
        })
    }

    pub fn verifies(&self) -> usize {
        self.verifies.load(Ordering::SeqCst)
    }
}

impl PasswordHasher for Counting {
    fn hash(&self, password: &str) -> Result<String, AuthError> {
        self.hashes.fetch_add(1, Ordering::SeqCst);
        self.inner.hash(password)
    }

    fn is_verifiable(&self, stored: &str) -> bool {
        self.inner.is_verifiable(stored)
    }

    fn verify(&self, password: &str, stored: &str) -> bool {
        self.verifies.fetch_add(1, Ordering::SeqCst);
        self.inner.verify(password, stored)
    }
}

pub struct World {
    pub store: Arc<MemoryStore>,
    pub auth: Authenticator,
    pub hasher: Arc<Counting>,
    pub alice: Account,
    pub bob: Account,
}

/// The client address for a connection from `s` with no trusted proxy in the way.
pub fn ip(s: &str) -> ClientAddress {
    ProxyPolicy::none().client(s.parse::<IpAddr>().unwrap(), None)
}

pub fn config() -> AuthConfig {
    AuthConfig {
        hash_permits: 4,
        gate_wait: Duration::from_millis(200),
        ..AuthConfig::default()
    }
}

pub fn world_with(config: AuthConfig) -> World {
    world_built(config, |auth| auth)
}

/// A world whose authenticator is passed through `finish` (to attach an OpenID Connect provider, an audit
/// sink) before the two accounts are registered.
pub fn world_built(
    config: AuthConfig,
    finish: impl FnOnce(Authenticator) -> Authenticator,
) -> World {
    let store = Arc::new(MemoryStore::new());
    let hasher = Counting::new();
    let auth = finish(Authenticator::with_hasher(store.clone(), config, hasher.clone()).unwrap());
    let alice = auth
        .register_local("alice", "Alice", ALICE_PASSWORD, NOW)
        .unwrap();
    let bob = auth
        .register_local("bob", "Bob", "another long password", NOW)
        .unwrap();
    World {
        store,
        auth,
        hasher,
        alice,
        bob,
    }
}

pub fn world() -> World {
    world_with(config())
}

pub fn world_with_oidc(provider: OidcProvider) -> World {
    world_built(config(), |auth| auth.with_oidc(provider))
}

pub fn id(account: &Account) -> AccountId {
    account.id.clone()
}
