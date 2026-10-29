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

/// Which of a wiki's two keys a call signs with (RFC 002 C-1a, C-1c: a call sees only the seed it signs with).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// The author key: signs patches (`commit`).
    Author,
    /// The maintainer key: signs blocks, ref states and ref updates (`seal`, `branch create`).
    Maintainer,
}

impl Role {
    /// The role's name in file names and prikk's variables.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Role::Author => "author",
            Role::Maintainer => "maintainer",
        }
    }
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
    /// When set, the call this directory is handed to signs with this role only: the other role's variables are
    /// not exported (and, for a directory made by the key store, its seed is not in it).
    only: Option<Role>,
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
        Ok(KeyDir {
            dir,
            ids: None,
            only: None,
        })
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

    /// A directory that holds one role's seed only, with that role's key id.
    pub(crate) fn single(
        dir: impl Into<PathBuf>,
        role: Role,
        key_id: &str,
    ) -> Result<Self, SignerError> {
        let (author, maintainer) = match role {
            Role::Author => (key_id, ""),
            Role::Maintainer => ("", key_id),
        };
        let mut key_dir = Self::with_ids(dir, author, maintainer)?;
        key_dir.only = Some(role);
        Ok(key_dir)
    }

    /// The same directory, for a call that signs with `role` only: the other role's variables are not exported.
    /// (A directory the key store made holds one seed already; this is for a plain directory that holds both.)
    #[cfg(any(test, feature = "bootstrap"))]
    pub(crate) fn restricted_to(&self, role: Role) -> Self {
        KeyDir {
            only: Some(role),
            ..self.clone()
        }
    }

    /// The directory.
    pub(crate) fn path(&self) -> &Path {
        &self.dir
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
        let mut env = Vec::new();
        for role in [Role::Author, Role::Maintainer] {
            if self.only.is_some_and(|only| only != role) {
                continue;
            }
            let name = role.name().to_ascii_uppercase();
            env.push((
                format!("PRIKK_{name}_SEED_FILE"),
                self.seed_file_text(role.name()),
            ));
            let stated = self.ids.as_ref().map(|(a, m)| match role {
                Role::Author => a.clone(),
                Role::Maintainer => m.clone(),
            });
            let id = match stated {
                Some(id) => Some(id).filter(|id| !id.is_empty()),
                None => self.key_id(role.name()),
            };
            if let Some(id) = id {
                env.push((format!("PRIKK_{name}_KEY_ID"), id));
            }
        }
        env
    }
}
