//! One wiki's key record (RFC 002 D-3): the plain fields anyone may read, and the two seeds sealed under the
//! master key. The plain fields are **bound into the seal** as associated data, so a changed key id, a
//! changed generation or a record moved to another wiki's file no longer authenticates; there is no field
//! of the record that can be edited without the master key being noticed.
//!
//! File format `inklkunn-key-record/5`, one JSON object, lower-case hex for the binary fields:
//! `format`, `wiki`, `generation`, `author_key_id`, `maintainer_key_id`, `created_at`, `rotated_at`
//! (`null` until a rotation), `superseded` (see below), `retiring` (see below; absent unless a retirement is
//! unfinished), `nonce` (24 bytes), `sealed` (the ciphertext and its
//! tag). The plaintext is the two seed files exactly as prikk wrote them, each behind a two-byte big-endian
//! length.
//!
//! **`superseded`** (corrections handoff C11, DR-10) keeps, for every maintainer key a rotation replaced, its
//! id, when it was replaced, whether it was retired, and the tip block id of every view at that moment: the
//! point after which a seal by that key is not the service's. The ids and block ids are not secret, and are
//! bound into the seal with everything else, so they cannot be edited either. Format `/1` (no `superseded`)
//! still opens, as a record that has never been rotated under this rule.
//!
//! **`excused`** (corrections handoff 004, P2′; format `/3`) is kept inside a superseded entry that was also
//! **retired**: the exact set of blocks, ref states and ref updates `prikk verify` named when the retirement had
//! just been made (every one of them failing publication trust for want of the retired key), and nothing else.
//! It is what a later failing `verify` may be excused by, whole or not at all. It is bound into the seal like the
//! rest. Format `/2` (no `excused`) still opens, and a retirement recorded in it **excuses nothing**: the set was
//! never taken, and nothing is guessed to stand in for it.
//!
//! **`retiring`** (corrections handoff 004, Q1; format `/4`) is written **before** a retired key leaves the trust
//! store and cleared when the retirement's findings are recorded: the key's id, the blocks the `verify` just before
//! the removal listed as signed by it, how many records that `verify` checked, and the tip of every view at that
//! moment. A retirement that ends without its findings recorded (`verify` could not run after the removal, the two
//! looks disagreed, the record could not be written) leaves it in place, so that `inklkunn wiki keys
//! finish-retirement` can finish the record **from it**, and only while the views' tips are still those it kept (the
//! blocks signed by the retired key are still the ones it names). It is bound into the seal like the rest. Formats
//! `/1` to `/3` still open and mean that nothing is retiring.
//!
//! **Format `/5`** (handoff 004, W3 and W5) adds two things to a `retiring` state, both bound into the seal: the retired
//! key's **public** key (so that `inklkunn wiki keys abandon-retirement` can put it back into the repository's trust
//! after it was removed; the seed is not kept anywhere, a public key is not a secret) and the ids of the maintainer
//! keys the repository trusted when the state was written (so that finishing can require the trust list to be exactly
//! that list without the retired key). A `/4` record still opens, and a `/4` state, which has neither, opens with both
//! empty: it can be finished (the trust list is not compared) and cannot be abandoned after the key was removed.
//! Every record written is `/5`.

use inklkunn_store::WikiId;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::KeyStoreError;
use super::crypto::{CryptoError, MasterKey, NONCE_LEN};
use super::random::{hex, unhex};

/// The record format name, also the first thing bound into the associated data.
pub(super) const RECORD_FORMAT: &str = "inklkunn-key-record/5";
/// The format before the retired key's public key and the trust list were kept in `retiring`: still read, never
/// written (a state in it has neither).
const RECORD_FORMAT_V4: &str = "inklkunn-key-record/4";
/// The format before `retiring` existed: still read, never written (nothing is retiring in it).
const RECORD_FORMAT_V3: &str = "inklkunn-key-record/3";
/// The format before `excused` existed: still read, never written (a retirement in it excuses nothing).
const RECORD_FORMAT_V2: &str = "inklkunn-key-record/2";
/// The format before `superseded` existed: still read, never written.
const RECORD_FORMAT_V1: &str = "inklkunn-key-record/1";
/// A record larger than this is not one (a retired key's excused set is three 64-character ids for each block a
/// wiki has, and is why this is not smaller).
pub(super) const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;
/// The most findings of one retirement a record keeps (about 12 MiB of the record); a wiki whose retirement would
/// name more is refused before the key is removed from trust.
pub(crate) const MAX_EXCUSED_SUBJECTS: usize = 150_000;
/// The most views a rotation records the tips of (a wiki with more refuses to rotate, saying why).
pub(crate) const MAX_RECORDED_VIEWS: usize = 1024;
/// The most superseded keys a record keeps.
const MAX_SUPERSEDED: usize = 256;
/// The most maintainer keys a `retiring` state keeps the ids of.
pub(crate) const MAX_TRUSTED_MAINTAINERS: usize = 1024;

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
    /// A retirement that was begun and whose findings are not recorded yet (Q1); only ever about the newest
    /// superseded key.
    pub(crate) retiring: Option<Retiring>,
}

/// A retirement begun and not recorded (handoff 004, Q1): what the `verify` just before the key left the trust store
/// said, and where the repository was then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Retiring {
    /// The key being retired: the newest superseded key.
    pub(crate) maintainer_key_id: String,
    /// The blocks the `verify` before listed as sealed by that key.
    pub(crate) signed_by_key: Vec<String>,
    /// `checked publication trust records` of that `verify`: a record is made only if the `verify` after counts the
    /// same (nothing was published in between).
    pub(crate) checked_records: u64,
    /// `(view, tip block id)` of every view at that moment.
    pub(crate) views: Vec<RecordedTip>,
    /// The key's **public** key, 64 lower-case hex characters, as the repository's trust list holds it: what
    /// `abandon-retirement` adopts again. Empty in a state written by format `/4`.
    #[serde(default)]
    pub(crate) maintainer_public_key: String,
    /// The ids of the maintainer keys the repository trusted when the state was written (the retired key and the
    /// new one among them), sorted: finishing requires the trust list now to be exactly these without the retired
    /// key (W5). Empty in a state written by format `/4`.
    #[serde(default)]
    pub(crate) trusted_maintainers: Vec<String>,
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
    /// What `prikk verify` named just after the key was removed from trust (a retirement only; empty for a key that
    /// was kept, and for a retirement recorded before this was taken, which excuses nothing).
    #[serde(default, skip_serializing_if = "Excused::is_empty")]
    pub(crate) excused: Excused,
}

/// The findings of publication trust a retirement recorded: ids of the blocks, ref states and ref updates `prikk
/// verify` named right after the retired key left the trust store, each 64 lower-case hex characters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Excused {
    /// Blocks.
    #[serde(default)]
    pub(crate) blocks: Vec<String>,
    /// Ref states.
    #[serde(default)]
    pub(crate) ref_states: Vec<String>,
    /// Ref updates.
    #[serde(default)]
    pub(crate) ref_updates: Vec<String>,
}

impl Excused {
    /// Nothing recorded.
    pub(crate) fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.ref_states.is_empty() && self.ref_updates.is_empty()
    }

    /// How many findings.
    pub(crate) fn len(&self) -> usize {
        self.blocks.len() + self.ref_states.len() + self.ref_updates.len()
    }

    fn lists(&self) -> [&Vec<String>; 3] {
        [&self.blocks, &self.ref_states, &self.ref_updates]
    }
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
    s.is_empty() || valid_id(s)
}

fn valid_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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
        if format != RECORD_FORMAT_V1 {
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
                if format == RECORD_FORMAT
                    || format == RECORD_FORMAT_V4
                    || format == RECORD_FORMAT_V3
                {
                    // The ids are 64 characters each, so counts and bytes are unambiguous.
                    for list in entry.excused.lists() {
                        out.push(0);
                        out.extend_from_slice(&(list.len() as u32).to_be_bytes());
                        for id in list {
                            out.extend_from_slice(id.as_bytes());
                        }
                    }
                }
            }
        }
        if format == RECORD_FORMAT || format == RECORD_FORMAT_V4 {
            match &self.retiring {
                None => out.push(0),
                Some(r) => {
                    out.push(1);
                    out.extend_from_slice(r.maintainer_key_id.as_bytes());
                    out.push(0);
                    out.extend_from_slice(&r.checked_records.to_be_bytes());
                    out.extend_from_slice(&(r.signed_by_key.len() as u32).to_be_bytes());
                    for id in &r.signed_by_key {
                        out.extend_from_slice(id.as_bytes());
                    }
                    out.extend_from_slice(&(r.views.len() as u32).to_be_bytes());
                    for tip in &r.views {
                        out.push(0);
                        out.extend_from_slice(tip.view.as_bytes());
                        out.push(0);
                        out.extend_from_slice(tip.block_id.as_bytes());
                    }
                    if format == RECORD_FORMAT {
                        out.push(0);
                        out.extend_from_slice(r.maintainer_public_key.as_bytes());
                        out.push(0);
                        out.extend_from_slice(&(r.trusted_maintainers.len() as u32).to_be_bytes());
                        for id in &r.trusted_maintainers {
                            // A key id cannot contain NUL, so the separator is unambiguous.
                            out.push(0);
                            out.extend_from_slice(id.as_bytes());
                        }
                    }
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
            if entry.excused.len() > MAX_EXCUSED_SUBJECTS
                || entry
                    .excused
                    .lists()
                    .iter()
                    .any(|l| l.iter().any(|id| !valid_id(id)))
            {
                return Err(KeyStoreError::Record(
                    "a retired key's recorded findings are too many or are not ids prikk issues",
                ));
            }
            if !entry.retired && !entry.excused.is_empty() {
                return Err(KeyStoreError::Record(
                    "a key that was not retired cannot carry recorded findings",
                ));
            }
        }
        if let Some(r) = &self.retiring {
            let tips_ok = r.views.len() <= MAX_RECORDED_VIEWS
                && r.views.iter().all(|t| {
                    !t.view.is_empty()
                        && t.view.len() <= 256
                        && t.view.bytes().all(|b| b != 0)
                        && valid_block_id(&t.block_id)
                });
            let public_ok =
                r.maintainer_public_key.is_empty() || valid_id(&r.maintainer_public_key);
            let trusted_ok = r.trusted_maintainers.len() <= MAX_TRUSTED_MAINTAINERS
                && r.trusted_maintainers.iter().all(|id| valid_key_id(id));
            if !tips_ok
                || !public_ok
                || !trusted_ok
                || r.signed_by_key.len() > MAX_EXCUSED_SUBJECTS
                || r.signed_by_key.iter().any(|id| !valid_id(id))
            {
                return Err(KeyStoreError::Record(
                    "an unfinished retirement's record has a value prikk would not issue",
                ));
            }
            // It is about the newest superseded key, which has not been marked retired yet and holds no findings.
            if !self.superseded.last().is_some_and(|n| {
                n.maintainer_key_id == r.maintainer_key_id && !n.retired && n.excused.is_empty()
            }) {
                return Err(KeyStoreError::Record(
                    "an unfinished retirement is not about the newest superseded key",
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
    /// Absent before format `/4`, and whenever no retirement is unfinished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retiring: Option<Retiring>,
    nonce: String,
    sealed: String,
}

/// What a `retiring` state must hold in a format: a `/5` state names the retired key's public key and the trusted ids
/// (sorted, and the retired key among them); a `/4` state has neither.
fn state_matches_format(r: &Retiring, format: &str) -> Result<(), KeyStoreError> {
    let complete = valid_id(&r.maintainer_public_key)
        && r.trusted_maintainers.contains(&r.maintainer_key_id)
        && r.trusted_maintainers
            .windows(2)
            .all(|w| matches!(w, [a, b] if a < b));
    let empty = r.maintainer_public_key.is_empty() && r.trusted_maintainers.is_empty();
    let fits = if format == RECORD_FORMAT {
        complete
    } else {
        empty
    };
    if fits {
        Ok(())
    } else {
        Err(KeyStoreError::Record(
            "an unfinished retirement does not hold what its record format holds",
        ))
    }
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

/// A record in the format before the retired key's public key and the trust list were kept (tests: it must still
/// open; a state in it has neither).
#[cfg(test)]
pub(super) fn seal_v4(
    master: &MasterKey,
    meta: &Meta,
    seeds: &Seeds,
) -> Result<String, KeyStoreError> {
    seal_in_format(master, meta, seeds, RECORD_FORMAT_V4)
}

/// A record in the format before `retiring` (tests: it must still open, and nothing is retiring in it).
#[cfg(test)]
pub(super) fn seal_v3(
    master: &MasterKey,
    meta: &Meta,
    seeds: &Seeds,
) -> Result<String, KeyStoreError> {
    seal_in_format(master, meta, seeds, RECORD_FORMAT_V3)
}

/// A record in the format before `excused` (tests: it must still open, and a retirement in it excuses nothing).
#[cfg(test)]
pub(super) fn seal_v2(
    master: &MasterKey,
    meta: &Meta,
    seeds: &Seeds,
) -> Result<String, KeyStoreError> {
    seal_in_format(master, meta, seeds, RECORD_FORMAT_V2)
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
    if format != RECORD_FORMAT && format != RECORD_FORMAT_V4 && meta.retiring.is_some() {
        return Err(KeyStoreError::Record(
            "a record before format 4 cannot carry an unfinished retirement",
        ));
    }
    if let Some(r) = &meta.retiring {
        state_matches_format(r, format)?;
    }
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
        retiring: meta.retiring.clone(),
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
    if file.format != RECORD_FORMAT
        && file.format != RECORD_FORMAT_V4
        && file.format != RECORD_FORMAT_V3
        && file.format != RECORD_FORMAT_V2
        && file.format != RECORD_FORMAT_V1
    {
        return Err(bad("the record's format is not one this build reads"));
    }
    if file.format == RECORD_FORMAT_V1 && !file.superseded.is_empty() {
        return Err(bad("a format 1 record cannot carry superseded keys"));
    }
    if file.format != RECORD_FORMAT
        && file.format != RECORD_FORMAT_V4
        && file.format != RECORD_FORMAT_V3
        && file.superseded.iter().any(|s| !s.excused.is_empty())
    {
        return Err(bad(
            "a record before format 3 cannot carry recorded findings",
        ));
    }
    if file.format != RECORD_FORMAT && file.format != RECORD_FORMAT_V4 && file.retiring.is_some() {
        return Err(bad(
            "a record before format 4 cannot carry an unfinished retirement",
        ));
    }
    if let Some(r) = &file.retiring {
        state_matches_format(r, &file.format)?;
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
        retiring: file.retiring,
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
