#![forbid(unsafe_code)]
//! `inklkunn-signer` — the signer: the per-wiki key store and **the only path to prikk's write verbs**
//! (`init`, `trust`, `commit`, `seal`, `branch create`). Nothing else reaches key material; the
//! key-reachability gate (`tools/boundary-check`) lets only `inklkunn-core` depend on this crate, keeps
//! `key_store` private, and keeps the writable sandbox mounts out of every other crate.
//!
//! **What is public is small by design.** [`Accept`] is the one way into a wiki's history and [`Signer`]
//! implements it; [`RepositoryBootstrap`] is the seam *create wiki* calls; [`message`] is the attribution
//! contract the signer enforces; [`KeyIds`], [`KeyStatus`] and [`SealedIds`] carry ids and dates (never
//! seeds); [`SignerError`] says what did not happen. The write-verb runner, the key directory and the
//! key store are private to the crate.
//!
//! **The real signer** ([`Signer`]) reads the wiki's keys from the sealed key store (RFC 002 D-3), decrypts
//! them into a tmpfs directory for one call, and runs the write sequence only behind the acceptance guard
//! (D-4). The `BootstrapSigner` — the same sequence over a **plain-text key directory** — exists **only behind
//! the `bootstrap` cargo feature**, for tests; a product binary never enables it (the gate checks).

// The modules stay private: the key-reachability gate keeps every other crate from naming them.
mod flock;
mod key_store;
mod keys;
mod writes;

mod signer;
mod worktree;

pub mod accept;
pub mod message;

#[cfg(feature = "bootstrap")]
pub mod bootstrap;

use std::path::Path;

pub use accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ExpectedMessage, LedgerError,
    PreparedChange, PreparedFile, SealedChange, Unconfirmed, ViewCreationAct, ViewCreationFacts,
};
#[cfg(feature = "bootstrap")]
pub use bootstrap::BootstrapSigner;
pub use keys::KeyIds;
pub use signer::lifecycle::{OldKey, OldKeyOutcome, Rotation};
pub use signer::{KeyStatus, Signer, SignerConfig};
pub use writes::{SealedIds, SignerError, WRITE_CEILING, WRITE_DEADLINE};

/// The wiki-creating writes, in the signer's hands: the seam `inklkunn-core`'s *create wiki* calls. The
/// bootstrap signer implements it in M0; RFC 002's signer implements it for service.
pub trait RepositoryBootstrap: Send + Sync {
    /// Create the repository in `repo` (an existing, otherwise empty directory) and adopt the wiki's
    /// maintainer key; return the key ids.
    fn create_repository(&self, repo: &Path) -> Result<KeyIds, SignerError>;
    /// Commit the worktree as one patch with `message` and seal it.
    fn commit_and_seal(&self, repo: &Path, message: &str) -> Result<SealedIds, SignerError>;
    /// Remove what [`RepositoryBootstrap::create_repository`] made *outside* the repository for a creation
    /// that did not finish (the wiki's sealed key record): the repository directory itself is the caller's to
    /// remove. A signer that keeps nothing elsewhere has nothing to do.
    fn discard_unfinished(&self, repo: &Path) -> Result<(), SignerError> {
        let _ = repo;
        Ok(())
    }
}
