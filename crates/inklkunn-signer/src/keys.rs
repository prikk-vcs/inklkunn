//! Key directories and key ids. prikk writes `<seed path>.key-id` beside a seed made with `key generate
//! --out` (`author.seed.key-id`), and reads that name when `PRIKK_*_SEED_FILE` points at the seed. A seed
//! never leaves its file and never enters a message, an error or a log; this module only ever names paths
//! and ids. The type is private to the crate: nothing outside the signer can name a seed path.

use std::path::{Path, PathBuf};

use crate::writes::SignerError;

/// The two key ids of a wiki and the maintainer's public key (never seeds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyIds {
    /// `ed25519-<16 hex>` of the author key.
    pub author: String,
    /// `ed25519-<16 hex>` of the maintainer key.
    pub maintainer: String,
    /// The maintainer public key, hex, for `trust maintainer add`.
    pub maintainer_public_key: String,
}

/// A directory holding `author.seed`, `author.seed.key-id`, `maintainer.seed`, `maintainer.seed.key-id` as
/// prikk writes them (`prikk key generate --out`). The bootstrap signer keeps it on disk; RFC 002's store
/// creates one on a private tmpfs for the duration of a call. The path is UTF-8 and absolute.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KeyDir {
    dir: PathBuf,
    /// The author and maintainer key ids, when the owner of the directory states them (the key store does:
    /// it writes no `.key-id` files, and prikk is told the ids through its environment). Without them the
    /// ids are read from the `.key-id` files prikk wrote beside generated seeds (the bootstrap signer).
    ids: Option<(String, String)>,
}

impl std::fmt::Debug for KeyDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The path is where seeds live: it stays out of debug output and logs (RFC 002 D-3).
        f.write_str("KeyDir(..)")
    }
}

impl KeyDir {
    /// Name a key directory: absolute and UTF-8, or refused before anything runs.
    pub(crate) fn new(dir: impl Into<PathBuf>) -> Result<Self, SignerError> {
        let dir = dir.into();
        if !dir.is_absolute() {
            return Err(SignerError::Unavailable(
                "the key directory must be an absolute path".to_owned(),
            ));
        }
        if dir.to_str().is_none() {
            return Err(SignerError::Unavailable(
                "the key directory path must be UTF-8".to_owned(),
            ));
        }
        Ok(KeyDir { dir, ids: None })
    }

    /// Name a key directory whose key ids are stated rather than read from files.
    pub(crate) fn with_ids(
        dir: impl Into<PathBuf>,
        author_key_id: &str,
        maintainer_key_id: &str,
    ) -> Result<Self, SignerError> {
        let mut key_dir = Self::new(dir)?;
        key_dir.ids = Some((author_key_id.to_owned(), maintainer_key_id.to_owned()));
        Ok(key_dir)
    }

    /// The directory.
    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }

    /// Say the call that uses this directory is alive (see [`crate::key_store::touch`]).
    pub(crate) fn touch(&self) {
        crate::key_store::touch(&self.dir);
    }

    /// `<dir>/<role>.seed`.
    pub(crate) fn seed_file(&self, role: &str) -> PathBuf {
        self.dir.join(format!("{role}.seed"))
    }

    /// `<dir>/<role>.seed` as text (the path is UTF-8 by construction).
    pub(crate) fn seed_file_text(&self, role: &str) -> String {
        self.seed_file(role).to_string_lossy().into_owned()
    }

    /// `<dir>/<role>.seed.key-id`, the name prikk writes beside a generated seed.
    pub(crate) fn key_id_file(&self, role: &str) -> PathBuf {
        self.dir.join(format!("{role}.seed.key-id"))
    }

    /// The key id recorded beside a seed, if present.
    pub(crate) fn key_id(&self, role: &str) -> Option<String> {
        std::fs::read_to_string(self.key_id_file(role))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    /// The environment prikk needs to sign with these keys, as `--setenv` pairs: seed-file paths and key
    /// ids only (a seed never travels through the environment, prikk's rule since 0.40).
    pub(crate) fn env(&self) -> Vec<(String, String)> {
        let mut env = vec![
            (
                "PRIKK_AUTHOR_SEED_FILE".to_owned(),
                self.seed_file_text("author"),
            ),
            (
                "PRIKK_MAINTAINER_SEED_FILE".to_owned(),
                self.seed_file_text("maintainer"),
            ),
        ];
        let (author, maintainer) = match &self.ids {
            Some((a, m)) => (Some(a.clone()), Some(m.clone())),
            None => (self.key_id("author"), self.key_id("maintainer")),
        };
        if let Some(id) = author {
            env.push(("PRIKK_AUTHOR_KEY_ID".to_owned(), id));
        }
        if let Some(id) = maintainer {
            env.push(("PRIKK_MAINTAINER_KEY_ID".to_owned(), id));
        }
        env
    }
}
