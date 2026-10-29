#![forbid(unsafe_code)]
//! `inklkunn-signer` — the signer: the per-wiki key store and **the only path to prikk's write verbs**
//! (`init`, `trust`, `commit`, `seal`, `branch create`). Nothing else reaches key material; the
//! key-reachability gate (`tools/boundary-check`) lets only `inklkunn-core` depend on this crate, keeps
//! `key_store` private, and keeps the writable sandbox mounts out of every other crate.
//!
//! **What is public is small by design.** [`Accept`] is the one way into a wiki's history and [`Signer`]
//! implements it; [`RepositoryBootstrap`] is the seam *create wiki* calls; [`Maintenance`] is the **keyless
//! writable mode** (no key store, no secret, no seed): `archive export` and the checkpoint refresh, nothing
//! else (handoff 003 T7); [`CreationMark`] is the state a wiki is in while it is being created, which the
//! start-up sweep reads (a kill at any step leaves nothing behind); [`message`] is the attribution
//! contract the signer enforces; [`KeyIds`], [`KeyStatus`] and [`SealedIds`] carry ids and dates (never
//! seeds); [`SignerError`] says what did not happen. The write-verb runner, the key directory and the
//! key store are private to the crate.
//!
//! **The real signer** ([`Signer`]) reads the wiki's keys from the sealed key store (RFC 002 D-3), decrypts
//! them into a tmpfs directory for one call, and runs the write sequence only behind the acceptance guard
//! (D-4). The `BootstrapSigner` — the same sequence over a **plain-text key directory** — exists **only behind
//! the `bootstrap` cargo feature**, for tests; a product binary never enables it (the gate checks).

// The modules stay private: the key-reachability gate keeps every other crate from naming them.
mod cost;
mod flock;
mod key_store;
mod keys;
mod locks;
mod writes;

mod signer;
mod worktree;

pub mod accept;
pub mod creation;
pub mod message;

mod maintenance;

#[cfg(feature = "bootstrap")]
pub mod bootstrap;

use std::path::Path;

pub use accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ChangeDigest, ExpectedMessage,
    LedgerError, PreparedChange, PreparedFile, SealedChange, Unconfirmed, ViewCreationAct,
    ViewCreationFacts,
};
#[cfg(feature = "bootstrap")]
pub use bootstrap::BootstrapSigner;
pub use creation::{CreationMark, Unfinished, find_unfinished};
pub use keys::KeyIds;
pub use maintenance::{ArchiveExported, Maintenance};
pub use signer::lifecycle::{OldKey, OldKeyOutcome, Rotation};
pub use signer::superseded::{LateSeal, SupersededKey};
pub use signer::{KeyStatus, Signer, SignerConfig};
pub use writes::{SealedIds, SignerError, WRITE_DEADLINE};

/// The rule every path of a change must obey before the signer writes a byte (ASCII, prikk's path rules and caps,
/// under `pages/`, `attachments/`, `knowledge/` or `governance/` or the manifest), as a pure check: the answer
/// is the rule that was broken, never the path. What a dry run of an import reports is exactly what the signer
/// would refuse.
pub fn check_change_path(path: &str) -> Result<(), &'static str> {
    worktree::repo_path(Path::new(path)).map(|_| ())
}

/// Make a named pipe at `path`, for the tests of a reader that must not wait on one (`import-folder`'s). Behind the
/// `test-knobs` feature, which a product crate never enables and a gate checks: this crate has the file-system
/// crate (`rustix`) that the crates which may not spawn a process lack, and the tests may not spawn `mkfifo`.
#[cfg(feature = "test-knobs")]
pub fn make_fifo_for_tests(path: &Path) -> std::io::Result<()> {
    rustix::fs::mknodat(
        rustix::fs::CWD,
        path,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_bits_truncate(0o600),
        0,
    )
    .map_err(std::io::Error::from)
}

/// The most files one acceptance may carry.
pub const MAX_CHANGE_FILES: usize = worktree::MAX_FILES;

/// The most bytes one acceptance may carry.
pub const MAX_CHANGE_BYTES: u64 = worktree::MAX_TOTAL_BYTES;

/// The authority to make a wiki's keys and to rotate them (corrections handoff C7): [`Signer::create_keys`],
/// [`Signer::rotate_keys`] and [`RepositoryBootstrap::create_repository`] each need one. **There is one per
/// process**: the binary claims it once, for the administrative acts that are meant to make or change keys (the
/// key commands, *create wiki*), and a surface never holds it, so none can reach key operations by naming the
/// signer. (`claim` answers `None` the second time, whoever asks.)
///
/// ```compile_fail
/// use inklkunn_signer::KeyAuthority;
/// // No public constructor but `claim`.
/// let _ = KeyAuthority(());
/// ```
#[derive(Debug)]
pub struct KeyAuthority(());

impl KeyAuthority {
    /// Claim the process's one key authority: `Some` the first time, `None` ever after.
    pub fn claim() -> Option<KeyAuthority> {
        static CLAIMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        (!CLAIMED.swap(true, std::sync::atomic::Ordering::SeqCst)).then_some(KeyAuthority(()))
    }

    /// An authority for a test (any number of them). Behind the `test-knobs` feature, which a product crate
    /// never enables and a gate checks.
    #[cfg(any(test, feature = "test-knobs"))]
    pub fn for_tests() -> KeyAuthority {
        KeyAuthority(())
    }
}

/// The wiki-creating writes, in the signer's hands: the seam `inklkunn-core`'s *create wiki* calls. The
/// bootstrap signer implements it in M0; RFC 002's signer implements it for service.
pub trait RepositoryBootstrap: Send + Sync {
    /// Create the repository in `repo` (an existing, otherwise empty directory) and adopt the wiki's
    /// maintainer key; return the key ids.
    fn create_repository(
        &self,
        repo: &Path,
        authority: &KeyAuthority,
    ) -> Result<KeyIds, SignerError>;
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
