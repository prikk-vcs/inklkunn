#![forbid(unsafe_code)]
//! `inklkunn-signer` — the signer: the per-wiki key store and **the only path to prikk's write verbs**
//! (`init`, `trust`, `commit`, `seal`, `branch create`). Nothing else reaches key material; the
//! key-reachability gate (`tools/boundary-check`) lets only `inklkunn-core` depend on this crate, keeps
//! `key_store` private, and keeps the writable sandbox mounts out of every other crate.
//!
//! **What is public is small by design.** [`RepositoryBootstrap`] is the seam *create wiki* calls;
//! [`KeyIds`] and [`SealedIds`] carry ids (never seeds); [`SignerError`] says what did not happen. The
//! write-verb runner and the key directory are private to the crate.
//!
//! **M0 (RFC 001 D-9) holds the shape, not the real thing.** `key_store` is empty so the gate has a target.
//! The `BootstrapSigner` — the write sequence over a **plain-text key directory** — exists **only behind
//! the `bootstrap` cargo feature**, for tests and for the first wiki on a developer's machine; a product
//! binary never enables it (the gate checks). RFC 002 D-3 replaces the directory with encrypted records
//! decrypted into a private tmpfs for one call, and D-4 adds the acceptance guard in front of the sequence.
//! The sequence itself (`writes`, private) is what RFC 002 keeps.

// The key store is built and tested here and wired to `Accept` by handoff 002 T4; until then the product
// build constructs none of it. The module stays private: the key-reachability gate keeps every other crate
// from naming it.
#[cfg_attr(not(test), allow(dead_code, unused_imports))]
mod key_store;
// Without the `bootstrap` feature nothing in M0 constructs the key directory or runs the write sequence:
// RFC 002's handoff (T3, T4) wires them to the real key store and `Accept`. Until then they are kept, not
// dead, and the product build says nothing about them.
#[cfg_attr(not(feature = "bootstrap"), allow(dead_code))]
mod keys;
#[cfg_attr(not(feature = "bootstrap"), allow(dead_code))]
mod writes;

pub mod message;

#[cfg(feature = "bootstrap")]
pub mod bootstrap;

use std::path::Path;

#[cfg(feature = "bootstrap")]
pub use bootstrap::BootstrapSigner;
pub use keys::KeyIds;
pub use writes::{SealedIds, SignerError, WRITE_CEILING, WRITE_DEADLINE};

/// The wiki-creating writes, in the signer's hands: the seam `inklkunn-core`'s *create wiki* calls. The
/// bootstrap signer implements it in M0; RFC 002's signer implements it for service.
pub trait RepositoryBootstrap: Send + Sync {
    /// Create the repository in `repo` (an existing, otherwise empty directory) and adopt the wiki's
    /// maintainer key; return the key ids.
    fn create_repository(&self, repo: &Path) -> Result<KeyIds, SignerError>;
    /// Commit the worktree as one patch with `message` and seal it.
    fn commit_and_seal(&self, repo: &Path, message: &str) -> Result<SealedIds, SignerError>;
}
