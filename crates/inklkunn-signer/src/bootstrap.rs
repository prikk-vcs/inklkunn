//! The bootstrap signer (RFC 001 D-9, handoff T5's "test-only signer"): the write sequence over a
//! plain-text [`KeyDir`]. It creates a wiki's repository and seals system changes; it has **no acceptance
//! guard** and so must never be reachable from a request path. RFC 002 D-3/D-4 supersede it for service.

use std::path::Path;

use inklkunn_prikk::sandbox::Sandbox;

use crate::keys::{KeyDir, KeyIds};
use crate::writes::{SealedIds, SignerError, Writes};

/// The bootstrap signer.
#[derive(Debug, Clone)]
pub struct BootstrapSigner {
    writes: Writes,
    keys: KeyDir,
}

impl BootstrapSigner {
    /// A signer over `keys` (which may be empty: [`BootstrapSigner::ensure_keys`] fills it).
    pub fn new(binary: impl Into<std::ffi::OsString>, sandbox: Sandbox, keys: KeyDir) -> Self {
        BootstrapSigner {
            writes: Writes::new(binary, sandbox),
            keys,
        }
    }

    /// The key directory.
    pub fn keys(&self) -> &KeyDir {
        &self.keys
    }

    /// Generate the author and maintainer keys when the directory has none, and return their ids.
    pub fn ensure_keys(&self) -> Result<KeyIds, SignerError> {
        std::fs::create_dir_all(self.keys.path())
            .map_err(|e| SignerError::Unavailable(e.to_string()))?;
        for role in ["author", "maintainer"] {
            if !self.keys.seed_file(role).is_file() {
                self.writes.generate_key(&self.keys, role)?;
            }
        }
        let author = self.keys.key_id("author").ok_or_else(|| {
            SignerError::Damaged("no author.seed.key-id beside the seed".to_owned())
        })?;
        let maintainer = self.keys.key_id("maintainer").ok_or_else(|| {
            SignerError::Damaged("no maintainer.seed.key-id beside the seed".to_owned())
        })?;
        let maintainer_public_key = self.writes.public_key(&self.keys, "maintainer")?;
        Ok(KeyIds {
            author,
            maintainer,
            maintainer_public_key,
        })
    }

    /// Create a repository in `repo` (an existing, otherwise empty directory) and adopt the maintainer key.
    pub fn create_repository(&self, repo: &Path) -> Result<KeyIds, SignerError> {
        let ids = self.ensure_keys()?;
        self.writes.init(repo, &self.keys)?;
        self.writes.trust_maintainer(
            repo,
            &self.keys,
            &ids.maintainer,
            &ids.maintainer_public_key,
        )?;
        Ok(ids)
    }

    /// Commit the worktree as one patch with `message` and seal it.
    pub fn commit_and_seal(&self, repo: &Path, message: &str) -> Result<SealedIds, SignerError> {
        self.writes.commit_and_seal(repo, &self.keys, message)
    }
}
