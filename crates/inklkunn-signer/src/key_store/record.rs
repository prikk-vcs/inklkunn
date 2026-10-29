//! One wiki's key record (RFC 002 D-3): the plain fields anyone may read, and the two seeds sealed under the
//! master key. The plain fields are **bound into the seal** as associated data, so a changed key id, a
//! changed generation or a record moved to another wiki's file no longer authenticates; there is no field
//! of the record that can be edited without the master key being noticed.
//!
//! File format `inklkunn-key-record/2`, one JSON object, lower-case hex for the binary fields:
//! `format`, `wiki`, `generation`, `author_key_id`, `maintainer_key_id`, `created_at`, `rotated_at`
//! (`null` until a rotation), `superseded` (see below), `nonce` (24 bytes), `sealed` (the ciphertext and its
//! tag). The plaintext is the two seed files exactly as prikk wrote them, each behind a two-byte big-endian
//! length.
//!
//! **`superseded`** (corrections handoff C11, DR-10) keeps, for every maintainer key a rotation replaced, its
//! id, when it was replaced, whether it was retired, and the tip block id of every view at that moment: the
//! point after which a seal by that key is not the service's. The ids and block ids are not secret, and are
//! bound into the seal with everything else, so they cannot be edited either. Format `/1` (no `superseded`)
//! still opens, as a record that has never been rotated under this rule; every record written is `/2`.

use inklkunn_store::WikiId;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::KeyStoreError;
use super::crypto::{CryptoError, MasterKey, NONCE_LEN};
use super::random::{hex, unhex};

/// The record format name, also the first thing bound into the associated data.
pub(super) const RECORD_FORMAT: &str = "inklkunn-key-record/2";
/// The format before `superseded` existed: still read, never written.
const RECORD_FORMAT_V1: &str = "inklkunn-key-record/1";
/// A record larger than this is not one.
pub(super) const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;
/// The most views a rotation records the tips of (a wiki with more refuses to rotate, saying why).
pub(crate) const MAX_RECORDED_VIEWS: usize = 1024;
/// The most superseded keys a record keeps.
const MAX_SUPERSEDED: usize = 256;

/// A key id as prikk accepts one: ASCII letters, digits, `-` and `_` (its `path-safety` rule for trust key
/// ids), at most 128 characters.
pub(super) fn valid_key_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A seed as `prikk key generate` writes it: 64 lower-case hex characters and a line feed (or without it).
pub(super) fn valid_seed(s: &[u8]) -> bool {
    let body = s.strip_suffix(b"\n").unwrap_or(s);
    body.len() == 64 && body.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The plain fields of a record: what `status` shows, and what the seal binds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Meta {
    /// The wiki.
    pub(crate) wiki: WikiId,
    /// 1 for the first pair of keys, one more for each rotation.
    pub(crate) generation: u32,
    /// The author key's id, as prikk is told it.
    pub(crate) author_key_id: String,
    /// The maintainer key's id.
    pub(crate) maintainer_key_id: String,
    /// Unix seconds.
    pub(crate) created_at: i64,
    /// Unix seconds of the latest rotation.
    pub(crate) rotated_at: Option<i64>,
    /// The maintainer keys earlier rotations replaced, oldest first, each with the point where it stopped.
    pub(crate) superseded: Vec<Superseded>,
}

/// A maintainer key a rotation replaced, and where it stopped being the service's (corrections handoff C11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Superseded {
    /// The generation of the pair this key belonged to.
    pub(crate) generation: u32,
    /// The maintainer key's id.
    pub(crate) maintainer_key_id: String,
    /// Unix seconds of the rotation that replaced it.
    pub(crate) superseded_at: i64,
    /// Was it also removed from the repository's trust (`--retire-old`)?
    pub(crate) retired: bool,
    /// `(view, tip block id)` of every view when it was replaced.
    pub(crate) views: Vec<RecordedTip>,
}

/// A view and its newest block at the moment of a rotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedTip {
    /// `heads/<name>`.
    pub(crate) view: String,
    /// The tip block id; empty for a view with no block yet.
    pub(crate) block_id: String,
}

fn valid_block_id(s: &str) -> bool {
    s.is_empty() || (s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

impl Meta {
    /// The associated data: every plain field, unambiguously (no field may contain NUL, which the ids and
    /// the wiki id cannot), under the format name.
    fn aad(&self, format: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(format.as_bytes());
        for field in [
            self.wiki.as_str(),
            &self.author_key_id,
            &self.maintainer_key_id,
        ] {
            out.push(0);
            out.extend_from_slice(field.as_bytes());
        }
        out.push(0);
        out.extend_from_slice(&self.generation.to_be_bytes());
        out.extend_from_slice(&self.created_at.to_be_bytes());
        match self.rotated_at {
            Some(t) => {
                out.push(1);
                out.extend_from_slice(&t.to_be_bytes());
            }
            None => out.push(0),
        }
        if format == RECORD_FORMAT {
            out.extend_from_slice(&(self.superseded.len() as u32).to_be_bytes());
            for entry in &self.superseded {
                out.push(0);
                out.extend_from_slice(&entry.generation.to_be_bytes());
                out.push(0);
                out.extend_from_slice(entry.maintainer_key_id.as_bytes());
                out.push(0);
                out.extend_from_slice(&entry.superseded_at.to_be_bytes());
                out.push(u8::from(entry.retired));
                out.extend_from_slice(&(entry.views.len() as u32).to_be_bytes());
                for tip in &entry.views {
                    out.push(0);
                    out.extend_from_slice(tip.view.as_bytes());
                    out.push(0);
                    out.extend_from_slice(tip.block_id.as_bytes());
                }
            }
        }
        out
    }

    fn check(&self) -> Result<(), KeyStoreError> {
        if self.generation == 0 {
            return Err(KeyStoreError::Record("a generation starts at 1"));
        }
        if !valid_key_id(&self.author_key_id) || !valid_key_id(&self.maintainer_key_id) {
            return Err(KeyStoreError::Record(
                "a key id has a character prikk refuses",
            ));
        }
        if self.author_key_id == self.maintainer_key_id {
            return Err(KeyStoreError::Record(
                "the author and maintainer ids are the same",
            ));
        }
        if self.superseded.len() > MAX_SUPERSEDED {
            return Err(KeyStoreError::Record("too many superseded keys"));
        }
        for entry in &self.superseded {
            if !valid_key_id(&entry.maintainer_key_id)
                || entry.views.len() > MAX_RECORDED_VIEWS
                || entry.views.iter().any(|t| {
                    t.view.is_empty()
                        || t.view.len() > 256
                        || t.view.bytes().any(|b| b == 0)
                        || !valid_block_id(&t.block_id)
                })
            {
                return Err(KeyStoreError::Record(
                    "a superseded key's record has a value prikk would not issue",
                ));
            }
        }
        Ok(())
    }
}

/// The two seeds of a wiki, each exactly as prikk's seed file holds it. Zeroized when dropped; never printed.
pub(crate) struct Seeds {
    /// The author seed file's bytes.
    pub(crate) author: Zeroizing<Vec<u8>>,
    /// The maintainer seed file's bytes.
    pub(crate) maintainer: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Seeds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Seeds(..)")
    }
}

impl Seeds {
    /// Seeds from the bytes of two seed files; each must be a seed as prikk writes it.
    pub(crate) fn new(author: &[u8], maintainer: &[u8]) -> Result<Self, KeyStoreError> {
        if !valid_seed(author) || !valid_seed(maintainer) {
            return Err(KeyStoreError::Record("a seed is not 64 hex characters"));
        }
        if author == maintainer {
            return Err(KeyStoreError::Record(
                "the author and maintainer seeds are the same",
            ));
        }
        Ok(Seeds {
            author: Zeroizing::new(author.to_vec()),
            maintainer: Zeroizing::new(maintainer.to_vec()),
        })
    }

    fn pack(&self) -> Result<Zeroizing<Vec<u8>>, KeyStoreError> {
        let mut out = Zeroizing::new(Vec::with_capacity(
            self.author.len() + self.maintainer.len() + 4,
        ));
        for seed in [&self.author, &self.maintainer] {
            let len = u16::try_from(seed.len())
                .map_err(|_| KeyStoreError::Record("a seed is too long"))?;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(seed);
        }
        Ok(out)
    }

    fn unpack(plain: &[u8]) -> Result<Self, KeyStoreError> {
        let bad = || KeyStoreError::Record("the sealed seeds are not in the expected layout");
        let (author, rest) = take(plain).ok_or_else(bad)?;
        let (maintainer, rest) = take(rest).ok_or_else(bad)?;
        if !rest.is_empty() {
            return Err(bad());
        }
        Seeds::new(author, maintainer)
    }
}

/// A length-prefixed chunk and the rest.
fn take(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    let (len, rest) = buf.split_first_chunk::<2>()?;
    rest.split_at_checked(usize::from(u16::from_be_bytes(*len)))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    format: String,
    wiki: String,
    generation: u32,
    author_key_id: String,
    maintainer_key_id: String,
    created_at: i64,
    rotated_at: Option<i64>,
    /// Absent in format `/1`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    superseded: Vec<Superseded>,
    nonce: String,
    sealed: String,
}

fn crypto(e: CryptoError) -> KeyStoreError {
    match e {
        CryptoError::Authentication => KeyStoreError::Record("the record did not authenticate"),
        CryptoError::Random => KeyStoreError::Random,
        CryptoError::Params | CryptoError::Derivation | CryptoError::Seal => {
            KeyStoreError::Record("the record could not be sealed")
        }
    }
}

/// The text of a record file: `seeds` sealed under `master`, bound to `meta`.
pub(super) fn seal(
    master: &MasterKey,
    meta: &Meta,
    seeds: &Seeds,
) -> Result<String, KeyStoreError> {
    seal_in_format(master, meta, seeds, RECORD_FORMAT)
}

/// A record in the format before `superseded` (tests: it must still open).
#[cfg(test)]
pub(super) fn seal_v1(
    master: &MasterKey,
    meta: &Meta,
    seeds: &Seeds,
) -> Result<String, KeyStoreError> {
    seal_in_format(master, meta, seeds, RECORD_FORMAT_V1)
}

fn seal_in_format(
    master: &MasterKey,
    meta: &Meta,
    seeds: &Seeds,
    format: &str,
) -> Result<String, KeyStoreError> {
    meta.check()?;
    let plain = seeds.pack()?;
    let (nonce, sealed) = master.seal(&plain, &meta.aad(format)).map_err(crypto)?;
    let file = File {
        format: format.to_owned(),
        wiki: meta.wiki.to_string(),
        generation: meta.generation,
        author_key_id: meta.author_key_id.clone(),
        maintainer_key_id: meta.maintainer_key_id.clone(),
        created_at: meta.created_at,
        rotated_at: meta.rotated_at,
        superseded: meta.superseded.clone(),
        nonce: hex(&nonce),
        sealed: hex(&sealed),
    };
    let mut text = serde_json::to_string_pretty(&file)
        .map_err(|_| KeyStoreError::Record("the record could not be written"))?;
    text.push('\n');
    if text.len() as u64 > MAX_RECORD_BYTES {
        return Err(KeyStoreError::Record(
            "the record would be larger than a record may be",
        ));
    }
    Ok(text)
}

/// Open a record file's text for `wiki`: parse it strictly, authenticate it against its own plain fields,
/// and return them with the seeds. A record for another wiki, with a field changed, or sealed under another
/// key is the same refusal.
pub(super) fn open(
    master: &MasterKey,
    text: &str,
    wiki: &WikiId,
) -> Result<(Meta, Seeds), KeyStoreError> {
    let bad = |what: &'static str| KeyStoreError::Record(what);
    let file: File = serde_json::from_str(text).map_err(|_| bad("the record is not readable"))?;
    if file.format != RECORD_FORMAT && file.format != RECORD_FORMAT_V1 {
        return Err(bad("the record's format is not one this build reads"));
    }
    if file.format == RECORD_FORMAT_V1 && !file.superseded.is_empty() {
        return Err(bad("a format 1 record cannot carry superseded keys"));
    }
    if file.wiki != wiki.as_str() {
        return Err(bad("the record is for another wiki"));
    }
    let meta = Meta {
        wiki: wiki.clone(),
        generation: file.generation,
        author_key_id: file.author_key_id,
        maintainer_key_id: file.maintainer_key_id,
        created_at: file.created_at,
        rotated_at: file.rotated_at,
        superseded: file.superseded,
    };
    meta.check()?;
    let nonce: [u8; NONCE_LEN] = unhex(&file.nonce)
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| bad("the record's nonce is not 24 bytes of hex"))?;
    let sealed = unhex(&file.sealed).ok_or_else(|| bad("the record's ciphertext is not hex"))?;
    let plain = master
        .open(&nonce, &sealed, &meta.aad(&file.format))
        .map_err(crypto)?;
    Ok((meta, Seeds::unpack(&plain)?))
}
