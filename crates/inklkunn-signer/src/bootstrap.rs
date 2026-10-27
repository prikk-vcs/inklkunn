//! The bootstrap signer (RFC 001 D-9, handoff T5's "test-only signer"): the write sequence over a
//! plain-text key directory. **Behind the `bootstrap` cargo feature; never in a product binary.** It
//! creates a wiki's repository and seals system changes; it has **no acceptance guard**, so it must never
//! be reachable from a request path. RFC 002 D-3/D-4 supersede it for service.
//!
//! One directory, one key pair: every wiki a bootstrap signer creates is signed by the same pair, which is
//! fine for a developer's machine and tests and is one of the reasons it is not a service signer (C-1d
//! wants per-wiki keys).

use std::path::{Path, PathBuf};

use inklkunn_prikk::sandbox::Sandbox;

use crate::RepositoryBootstrap;
use crate::keys::{KeyDir, KeyIds};
use crate::writes::{SealedIds, SignerError, Writes};

/// The bootstrap signer.
#[derive(Debug, Clone)]
pub struct BootstrapSigner {
    writes: Writes,
    keys: KeyDir,
}

impl BootstrapSigner {
    /// A signer over the key directory `key_dir` (absolute, UTF-8; it may be empty:
    /// [`BootstrapSigner::ensure_keys`] fills it with mode 0700).
    pub fn new(
        binary: impl Into<std::ffi::OsString>,
        sandbox: Sandbox,
        key_dir: impl Into<PathBuf>,
    ) -> Result<Self, SignerError> {
        Ok(BootstrapSigner {
            writes: Writes::new(binary, sandbox),
            keys: KeyDir::new(key_dir)?,
        })
    }

    /// Generate the author and maintainer keys when the directory has none, and return their ids. The
    /// directory is created with mode 0700; prikk writes the seeds with mode 0600.
    pub fn ensure_keys(&self) -> Result<KeyIds, SignerError> {
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(self.keys.path())
                .map_err(|e| SignerError::Unavailable(e.to_string()))?;
        }
        for role in ["author", "maintainer"] {
            if !self.keys.seed_file(role).is_file() {
                match self.writes.generate_key(&self.keys, role) {
                    Ok(()) => {}
                    // Two creations racing: the other one wrote the pair between our check and our
                    // generate; prikk refuses to overwrite, and the files are now there.
                    Err(SignerError::Refused(m))
                        if m.contains("overwrite") && self.keys.seed_file(role).is_file() => {}
                    Err(e) => return Err(e),
                }
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
}

impl RepositoryBootstrap for BootstrapSigner {
    fn create_repository(&self, repo: &Path) -> Result<KeyIds, SignerError> {
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

    fn commit_and_seal(&self, repo: &Path, message: &str) -> Result<SealedIds, SignerError> {
        self.writes.commit_and_seal(repo, &self.keys, message)
    }
}
