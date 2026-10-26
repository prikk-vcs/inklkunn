//! Key directories and key ids. prikk writes `<seed path>.key-id` beside a seed made with `key generate
//! --out` (`author.seed.key-id`), and reads that name when `PRIKK_*_SEED_FILE` points at the seed. A seed never leaves its file and never enters a message or a log; this
//! module only ever names paths and ids.

use std::path::{Path, PathBuf};

/// The two key ids of a wiki (never seeds).
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
/// prikk writes them (`prikk key generate --out`). **Plain text on disk**: for tests and a developer's first wiki
/// only (RFC 001 D-9); RFC 002 D-3's store replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyDir {
    dir: PathBuf,
}

impl KeyDir {
    /// Name a key directory (absolute).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        KeyDir { dir: dir.into() }
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// `<dir>/<role>.seed`.
    pub fn seed_file(&self, role: &str) -> PathBuf {
        self.dir.join(format!("{role}.seed"))
    }

    /// `<dir>/<role>.seed.key-id`, the name prikk writes beside a generated seed.
    pub fn key_id_file(&self, role: &str) -> PathBuf {
        self.dir.join(format!("{role}.seed.key-id"))
    }

    /// The key id recorded beside a seed, if present.
    pub fn key_id(&self, role: &str) -> Option<String> {
        std::fs::read_to_string(self.key_id_file(role))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    /// The environment prikk needs to sign with these keys, as `--setenv` pairs: seed-file paths and key
    /// ids only (a seed never travels through the environment, prikk's rule since 0.40).
    pub fn env(&self) -> Vec<(String, String)> {
        let mut env = vec![
            (
                "PRIKK_AUTHOR_SEED_FILE".to_owned(),
                self.seed_file("author").to_string_lossy().into_owned(),
            ),
            (
                "PRIKK_MAINTAINER_SEED_FILE".to_owned(),
                self.seed_file("maintainer").to_string_lossy().into_owned(),
            ),
        ];
        if let Some(id) = self.key_id("author") {
            env.push(("PRIKK_AUTHOR_KEY_ID".to_owned(), id));
        }
        if let Some(id) = self.key_id("maintainer") {
            env.push(("PRIKK_MAINTAINER_KEY_ID".to_owned(), id));
        }
        env
    }
}
