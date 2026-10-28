//! The per-wiki key store (RFC 002 D-3; threat model C-1a, C-1c, C-1d, INV-2): one sealed record per wiki
//! holding its author and maintainer seeds, readable only with the operator's secret, decrypted into a
//! tmpfs directory for one write call and nowhere else.
//!
//! **This module is private to the crate, and so is everything in it.** Nothing here is `pub` to the outside:
//! the key-reachability gate (`tools/boundary-check`) keeps every other crate from naming it, and a second
//! rule keeps a public item of this crate from mentioning a seed, a secret or a key directory at all. What
//! the rest of the crate may do with it is small: open the store, create or replace a wiki's record, read its
//! plain status (ids and dates), and [`KeyStore::checkout`] the seeds for one call.
//!
//! **What is on disk** under the key directory (mode 0700):
//!
//! - `store.json`, the header: the Argon2id cost parameters, a random salt, and a *verifier*, a known
//!   plaintext sealed under the derived master key, so that a wrong secret is reported when the store is
//!   opened and not at the first signature (and so that an altered header cannot be told from a wrong
//!   secret, which is the point).
//! - `<wiki id>.key.json` per wiki: see [`record`]. Files are mode 0600, written to a temporary name and
//!   moved into place, so a crash leaves the old record or the new one and never half of one.
//!
//! **The operator's secret** is a file named by the operator (an environment variable names it; it is never
//! stored beside the records): its content without the trailing line break, 16 to 1024 bytes. It is held in
//! memory only as long as it takes to derive the master key.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use inklkunn_store::WikiId;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

mod crypto;
mod random;
mod record;
mod seeds;

use crypto::{KdfParams, MasterKey, NONCE_LEN, SALT_LEN};
pub(crate) use record::{Meta, Seeds};
pub(crate) use seeds::{DIR_PREFIX, SeedBase, SeedDir, touch};

/// The header's file name.
const HEADER_FILE: &str = "store.json";
/// The format name of the header.
const HEADER_FORMAT: &str = "inklkunn-key-store/1";
/// The largest header there is.
const MAX_HEADER_BYTES: u64 = 4 * 1024;
/// The suffix of a record's file name.
const RECORD_SUFFIX: &str = ".key.json";
/// What the verifier seals: anything fixed; the point is that only the right key opens it.
const VERIFIER_PLAIN: &[u8] = b"inklkunn key store: this opens only with the operator's secret";
/// The shortest operator secret, in bytes.
pub(crate) const SECRET_MIN: usize = 16;
/// The longest operator secret, in bytes.
pub(crate) const SECRET_MAX: usize = 1024;

/// Why the key store could not do what was asked. **No variant carries a secret, a seed, a derived key, a
/// plaintext, or the path of a seed**; the texts are fixed phrases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyStoreError {
    /// The operator's secret file is unusable, for the stated reason.
    Secret(&'static str),
    /// A file operation failed (what, and the kind of failure).
    Io(&'static str, std::io::ErrorKind),
    /// The secret does not open the key store: it is the wrong secret, or the header was altered.
    Locked,
    /// The store's header is missing, unreadable or of a format this build does not read.
    Header(&'static str),
    /// A record is unreadable, altered, or not for this wiki.
    Record(&'static str),
    /// The wiki has no keys here.
    NoKeys,
    /// The wiki already has keys here.
    Exists,
    /// The seed directory's base is unusable (not a tmpfs, not there).
    SeedBase(&'static str),
    /// The temporary directory of decrypted seeds could not be removed. The operator removes the
    /// directories named `inklkunn-seeds-*` under the seed base.
    SeedsNotRemoved,
    /// The system's randomness could not be read.
    Random,
}

impl std::fmt::Display for KeyStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyStoreError::Secret(why) => write!(f, "the key secret is unusable: {why}"),
            KeyStoreError::Io(what, kind) => write!(f, "{what} ({kind})"),
            KeyStoreError::Locked => write!(
                f,
                "the key secret does not open the key store: it is the wrong secret, or the store's header was altered"
            ),
            KeyStoreError::Header(why) => write!(f, "the key store's header: {why}"),
            KeyStoreError::Record(why) => write!(f, "a key record: {why}"),
            KeyStoreError::NoKeys => write!(f, "the wiki has no keys in the key store"),
            KeyStoreError::Exists => write!(f, "the wiki already has keys in the key store"),
            KeyStoreError::SeedBase(why) => write!(f, "the seed directory: {why}"),
            KeyStoreError::SeedsNotRemoved => write!(
                f,
                "a temporary directory of decrypted seeds could not be removed; remove the directories named `{}*` under the seed directory",
                seeds::DIR_PREFIX
            ),
            KeyStoreError::Random => write!(f, "the system's randomness could not be read"),
        }
    }
}

impl std::error::Error for KeyStoreError {}

fn io(what: &'static str) -> impl Fn(std::io::Error) -> KeyStoreError {
    move |e| KeyStoreError::Io(what, e.kind())
}

/// The operator's secret: the bytes of the file the operator named. Held in a buffer that is zeroed when
/// dropped, never printed.
pub(crate) struct Secret(Zeroizing<Vec<u8>>);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Secret {
    /// Read the secret from `path` (absolute; a symbolic link is followed, since secret mounts are often
    /// links; the file must be a regular file).
    pub(crate) fn from_file(path: &Path) -> Result<Self, KeyStoreError> {
        if !path.is_absolute() {
            return Err(KeyStoreError::Secret("its path must be absolute"));
        }
        let meta = std::fs::metadata(path)
            .map_err(|_| KeyStoreError::Secret("the file could not be read"))?;
        if !meta.is_file() {
            return Err(KeyStoreError::Secret("it is not a regular file"));
        }
        if meta.len() > u64::try_from(SECRET_MAX + 2).unwrap_or(0) {
            return Err(KeyStoreError::Secret("it is longer than 1024 bytes"));
        }
        let raw = Zeroizing::new(
            std::fs::read(path).map_err(|_| KeyStoreError::Secret("the file could not be read"))?,
        );
        Self::from_bytes(&raw)
    }

    /// A secret from file content: the bytes without trailing line breaks, 16 to 1024 of them. (An editor or
    /// `echo` adds a line break; the same passphrase with and without one must not be two secrets.)
    pub(crate) fn from_bytes(raw: &[u8]) -> Result<Self, KeyStoreError> {
        let mut end = raw.len();
        while end > 0 && matches!(raw.get(end - 1), Some(b'\n' | b'\r')) {
            end -= 1;
        }
        let body = raw.get(..end).unwrap_or_default();
        if body.len() < SECRET_MIN {
            return Err(KeyStoreError::Secret("it is shorter than 16 bytes"));
        }
        if body.len() > SECRET_MAX {
            return Err(KeyStoreError::Secret("it is longer than 1024 bytes"));
        }
        Ok(Secret(Zeroizing::new(body.to_vec())))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderFile {
    format: String,
    kdf: String,
    version: u32,
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
    salt: String,
    verifier_nonce: String,
    verifier: String,
}

impl HeaderFile {
    /// The associated data of the verifier: every plain field of the header.
    fn aad(&self) -> Vec<u8> {
        format!(
            "{}\0{}\0{}\0{}\0{}\0{}\0{}",
            self.format,
            self.kdf,
            self.version,
            self.m_cost_kib,
            self.t_cost,
            self.p_cost,
            self.salt
        )
        .into_bytes()
    }

    fn params(&self) -> Result<KdfParams, KeyStoreError> {
        if self.format != HEADER_FORMAT || self.kdf != "argon2id" || self.version != 0x13 {
            return Err(KeyStoreError::Header(
                "it is not a format or key derivation this build reads",
            ));
        }
        KdfParams {
            m_cost_kib: self.m_cost_kib,
            t_cost: self.t_cost,
            p_cost: self.p_cost,
        }
        .check()
        .map_err(|_| KeyStoreError::Header("its key derivation costs are outside the bounds"))
    }
}

fn salt_of(h: &HeaderFile) -> Result<[u8; SALT_LEN], KeyStoreError> {
    random::unhex(&h.salt)
        .and_then(|v| v.try_into().ok())
        .ok_or(KeyStoreError::Header("its salt is not 16 bytes of hex"))
}

/// The unlocked key store.
pub(crate) struct KeyStore {
    dir: PathBuf,
    master: MasterKey,
}

impl std::fmt::Debug for KeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyStore(..)")
    }
}

/// A file's text if it exists; `None` if not. It must be a regular file (never a link) of at most `max`
/// bytes.
fn read_small(path: &Path, max: u64, what: &'static str) -> Result<Option<String>, KeyStoreError> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(KeyStoreError::Io(what, e.kind())),
    };
    if !meta.is_file() {
        return Err(KeyStoreError::Io(what, std::io::ErrorKind::InvalidInput));
    }
    if meta.len() > max {
        return Err(KeyStoreError::Io(what, std::io::ErrorKind::InvalidData));
    }
    std::fs::read_to_string(path).map(Some).map_err(io(what))
}

/// Write `text` to `dir/name` through a temporary file in `dir` (mode 0600, synced), so the destination is
/// never partly written. `exclusive` refuses to replace an existing file ([`KeyStoreError::Exists`]).
fn write_private(dir: &Path, name: &str, text: &str, exclusive: bool) -> Result<(), KeyStoreError> {
    let tmp = dir.join(format!(
        ".{name}.tmp-{}",
        random::hex(&random::bytes::<8>().map_err(|_| KeyStoreError::Random)?)
    ));
    let write = || -> Result<(), KeyStoreError> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(io("a key file could not be made"))?;
        f.write_all(text.as_bytes())
            .map_err(io("a key file could not be written"))?;
        f.sync_all().map_err(io("a key file could not be synced"))
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let dest = dir.join(name);
    let moved = if exclusive {
        // A hard link fails if the destination exists: the exclusive create, atomically.
        let r = std::fs::hard_link(&tmp, &dest);
        let _ = std::fs::remove_file(&tmp);
        r
    } else {
        std::fs::rename(&tmp, &dest)
    };
    match moved {
        Ok(()) => {}
        Err(e) if exclusive && e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(KeyStoreError::Exists);
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(KeyStoreError::Io(
                "a key file could not be moved into place",
                e.kind(),
            ));
        }
    }
    std::fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(io("the key directory could not be synced"))
}

/// The key directory: made with mode 0700 when absent; when present, a real directory (not a link) that
/// nobody else can enter.
fn prepare_dir(dir: &Path) -> Result<(), KeyStoreError> {
    if !dir.is_absolute() {
        return Err(KeyStoreError::Io(
            "the key directory must be an absolute path",
            std::io::ErrorKind::InvalidInput,
        ));
    }
    match std::fs::symlink_metadata(dir) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(io("the key directory could not be made"))?;
        }
        Err(e) => {
            return Err(KeyStoreError::Io(
                "the key directory could not be read",
                e.kind(),
            ));
        }
    }
    let meta = std::fs::symlink_metadata(dir).map_err(io("the key directory could not be read"))?;
    if !meta.is_dir() {
        return Err(KeyStoreError::Io(
            "the key directory is not a directory",
            std::io::ErrorKind::InvalidInput,
        ));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(KeyStoreError::Io(
            "the key directory is open to other users; make it mode 0700",
            std::io::ErrorKind::PermissionDenied,
        ));
    }
    Ok(())
}

impl KeyStore {
    /// Open the key store in `dir` with `secret`, creating it (empty) if `dir` is new or holds nothing.
    pub(crate) fn open(dir: &Path, secret: &Secret) -> Result<Self, KeyStoreError> {
        Self::open_with(dir, secret, KdfParams::DEFAULT)
    }

    /// Open with the cheapest derivation costs the bounds allow: for tests, behind the `test-knobs`
    /// feature. A store made this way keeps those costs in its header.
    #[cfg(any(test, feature = "test-knobs"))]
    pub(crate) fn open_cheap(dir: &Path, secret: &Secret) -> Result<Self, KeyStoreError> {
        Self::open_with(
            dir,
            secret,
            KdfParams {
                m_cost_kib: 8,
                t_cost: 1,
                p_cost: 1,
            },
        )
    }

    /// The same, with the key derivation costs a **new** store is made with (an existing store keeps the
    /// costs its header records). Tests use cheap ones.
    fn open_with(dir: &Path, secret: &Secret, params: KdfParams) -> Result<Self, KeyStoreError> {
        prepare_dir(dir)?;
        let header = dir.join(HEADER_FILE);
        if let Some(text) = read_small(
            &header,
            MAX_HEADER_BYTES,
            "the key store header could not be read",
        )? {
            return Self::unlock(dir, &text, secret);
        }
        let mut entries =
            std::fs::read_dir(dir).map_err(io("the key directory could not be listed"))?;
        if entries.next().is_some() {
            return Err(KeyStoreError::Header(
                "the directory holds files but no header; it is not a key store (or its header was removed)",
            ));
        }
        Self::initialize(dir, secret, params)
    }

    fn unlock(dir: &Path, header_text: &str, secret: &Secret) -> Result<Self, KeyStoreError> {
        let h: HeaderFile = serde_json::from_str(header_text)
            .map_err(|_| KeyStoreError::Header("it is not readable"))?;
        let params = h.params()?;
        let salt = salt_of(&h)?;
        let master = MasterKey::derive(&secret.0, &salt, params)
            .map_err(|_| KeyStoreError::Header("its key derivation failed"))?;
        let nonce: [u8; NONCE_LEN] = random::unhex(&h.verifier_nonce)
            .and_then(|v| v.try_into().ok())
            .ok_or(KeyStoreError::Header(
                "its verifier nonce is not 24 bytes of hex",
            ))?;
        let sealed =
            random::unhex(&h.verifier).ok_or(KeyStoreError::Header("its verifier is not hex"))?;
        let plain = master
            .open(&nonce, &sealed, &h.aad())
            .map_err(|_| KeyStoreError::Locked)?;
        if plain.as_slice() != VERIFIER_PLAIN {
            return Err(KeyStoreError::Locked);
        }
        Ok(KeyStore {
            dir: dir.to_path_buf(),
            master,
        })
    }

    fn initialize(dir: &Path, secret: &Secret, params: KdfParams) -> Result<Self, KeyStoreError> {
        let params = params.check().map_err(|_| {
            KeyStoreError::Header("the key derivation costs are outside the bounds")
        })?;
        let salt = random::bytes::<SALT_LEN>().map_err(|_| KeyStoreError::Random)?;
        let master = MasterKey::derive(&secret.0, &salt, params)
            .map_err(|_| KeyStoreError::Header("its key derivation failed"))?;
        let mut h = HeaderFile {
            format: HEADER_FORMAT.to_owned(),
            kdf: "argon2id".to_owned(),
            version: 0x13,
            m_cost_kib: params.m_cost_kib,
            t_cost: params.t_cost,
            p_cost: params.p_cost,
            salt: random::hex(&salt),
            verifier_nonce: String::new(),
            verifier: String::new(),
        };
        let (nonce, sealed) = master
            .seal(VERIFIER_PLAIN, &h.aad())
            .map_err(|_| KeyStoreError::Random)?;
        h.verifier_nonce = random::hex(&nonce);
        h.verifier = random::hex(&sealed);
        let mut text = serde_json::to_string_pretty(&h)
            .map_err(|_| KeyStoreError::Header("it could not be written"))?;
        text.push('\n');
        match write_private(dir, HEADER_FILE, &text, true) {
            Ok(()) => Ok(KeyStore {
                dir: dir.to_path_buf(),
                master,
            }),
            // Another process initialized the store between our check and our write: use its header.
            Err(KeyStoreError::Exists) => {
                let text = read_small(
                    &dir.join(HEADER_FILE),
                    MAX_HEADER_BYTES,
                    "the key store header could not be read",
                )?
                .ok_or(KeyStoreError::Header("it vanished while it was being made"))?;
                Self::unlock(dir, &text, secret)
            }
            Err(e) => Err(e),
        }
    }

    fn record_name(wiki: &WikiId) -> String {
        format!("{wiki}{RECORD_SUFFIX}")
    }

    fn record_text(&self, wiki: &WikiId) -> Result<String, KeyStoreError> {
        read_small(
            &self.dir.join(Self::record_name(wiki)),
            record::MAX_RECORD_BYTES,
            "a key record could not be read",
        )?
        .ok_or(KeyStoreError::NoKeys)
    }

    /// Give a wiki its keys. Refused if it has some: an existing pair is replaced only by [`replace`].
    ///
    /// [`replace`]: KeyStore::replace
    pub(crate) fn create(&self, meta: &Meta, seeds: &Seeds) -> Result<(), KeyStoreError> {
        let text = record::seal(&self.master, meta, seeds)?;
        write_private(&self.dir, &Self::record_name(&meta.wiki), &text, true)
    }

    /// Replace a wiki's keys (a rotation). The wiki must already have some.
    pub(crate) fn replace(&self, meta: &Meta, seeds: &Seeds) -> Result<(), KeyStoreError> {
        let _ = self.record_text(&meta.wiki)?;
        let text = record::seal(&self.master, meta, seeds)?;
        write_private(&self.dir, &Self::record_name(&meta.wiki), &text, false)
    }

    /// A wiki's plain facts: key ids, generation, dates. The record is opened and authenticated to read them,
    /// so an altered record is an error here and never a plausible-looking status.
    pub(crate) fn status(&self, wiki: &WikiId) -> Result<Meta, KeyStoreError> {
        let text = self.record_text(wiki)?;
        record::open(&self.master, &text, wiki).map(|(meta, _seeds)| meta)
    }

    /// Decrypt a wiki's seeds into a fresh seed directory under `base`, for one call. The caller **must**
    /// call [`SeedDir::finish`] when the call is over.
    pub(crate) fn checkout(
        &self,
        wiki: &WikiId,
        base: &SeedBase,
    ) -> Result<(Meta, SeedDir), KeyStoreError> {
        let text = self.record_text(wiki)?;
        let (meta, seeds) = record::open(&self.master, &text, wiki)?;
        let dir = SeedDir::create(base, &seeds, &meta.author_key_id, &meta.maintainer_key_id)?;
        Ok((meta, dir))
    }

    /// Remove a wiki's record (the cleanup of a creation that did not finish). `true` if there was one.
    pub(crate) fn remove(&self, wiki: &WikiId) -> Result<bool, KeyStoreError> {
        match std::fs::remove_file(self.dir.join(Self::record_name(wiki))) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(KeyStoreError::Io(
                "a key record could not be removed",
                e.kind(),
            )),
        }
    }

    /// The wikis that have a record, by name order.
    #[cfg(test)]
    pub(crate) fn wikis(&self) -> Result<Vec<WikiId>, KeyStoreError> {
        let mut out: Vec<WikiId> = std::fs::read_dir(&self.dir)
            .map_err(io("the key directory could not be listed"))?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                WikiId::parse(name.strip_suffix(RECORD_SUFFIX)?)
            })
            .collect();
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
