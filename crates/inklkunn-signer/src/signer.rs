//! The signer (RFC 002 D-3, D-4; internal design SGN-1 … SGN-4; threat model T-1): the one component that
//! holds a wiki's keys and runs prikk's write verbs, reachable from the rest of the system through
//! [`Accept`] and nothing else.
//!
//! **What a seal is bound to.** [`Signer::accept`](Accept::accept) is given a token, a view and a prepared
//! change. Before it writes a byte it reads the acceptance record the token names through its
//! [`AcceptanceLedger`], and refuses unless that record is pending seal *for this wiki and view* and the
//! change's message says exactly what the record says: the proposal, the proposer, the acceptor, the base,
//! the kind (INV-1). Only then does it decrypt the wiki's seeds, into a directory that lives for this call.
//!
//! **What happens to the worktree.** The sequence is: nothing unfinished in the repository; the view's
//! worktree; the renames, the writes and the removals; `commit`; `seal`; read the block back; `verify`.
//! A failure **before `commit` succeeds** puts the worktree back (originals restored, renames moved back)
//! and says so, or says plainly that it could not ([`AcceptError::WorktreeDirty`]); a failure **after** it is
//! [`AcceptError::Queued`] (nothing sealed yet) or [`AcceptError::SealedUnconfirmed`] (sealed; do not retry),
//! because a patch is queued and RFC 004's recovery completes it.
//!
//! **The unguarded entry** is [`RepositoryBootstrap`], which creates a wiki's repository and keys and seals
//! the very first change. It refuses any change but a `system` message to a repository with no history
//! yet, so it cannot be used to add to a wiki that exists.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_store::{RootLayout, WikiId};

use crate::KeyAuthority;
use crate::RepositoryBootstrap;
use crate::accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, PreparedChange, SealedChange,
    Unconfirmed, ViewCreationAct,
};
use crate::key_store::{KeyStore, KeyStoreError, Meta, Secret, SeedBase, SeedDir, Seeds};
use crate::keys::Role;
use crate::keys::{KeyDir, KeyIds};
use crate::locks::{WikiLocks, WriteGuard};
use crate::message::{Kind, Message};
use crate::worktree::{Applied, Plan};
use crate::writes::{SealedIds, SeedAccess, SignerError, Writes};

/// A seed or scratch directory older than this is a dead call's: a call is bounded by the write deadline
/// (two minutes), so five is safe for another process's live one.
const STALE_AFTER: Duration = Duration::from_secs(5 * 60);

/// Where everything the signer needs lives. Build it with [`SignerConfig::new`]: the struct is
/// non-exhaustive so that the test-only knobs (which exist under a cargo feature that a workspace build
/// may or may not unify in) never change what a caller has to write.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SignerConfig {
    /// The prikk binary (a bare name is resolved through `PATH` once, at open).
    pub prikk: OsString,
    /// How prikk is confined.
    pub sandbox: Sandbox,
    /// Where the wikis' repositories are (`<root>/<shard>/<wiki id>/`).
    pub root: RootLayout,
    /// The key store's directory (mode 0700).
    pub key_dir: PathBuf,
    /// The file holding the operator's secret (absolute; 16 to 1024 bytes).
    pub secret_file: PathBuf,
    /// A directory on a tmpfs where the keys of a call live for the duration of that call.
    pub tmpfs_dir: PathBuf,
    /// Open the key store with the cheapest derivation costs (tests only).
    #[cfg(feature = "test-knobs")]
    pub cheap_derivation: bool,
    /// Treat seed directories older than this as a killed call's (tests only; five minutes otherwise).
    #[cfg(feature = "test-knobs")]
    pub stale_after: Option<Duration>,
}

impl SignerConfig {
    /// A configuration with the production defaults.
    pub fn new(
        prikk: impl Into<OsString>,
        sandbox: Sandbox,
        root: RootLayout,
        key_dir: impl Into<PathBuf>,
        secret_file: impl Into<PathBuf>,
        tmpfs_dir: impl Into<PathBuf>,
    ) -> Self {
        SignerConfig {
            prikk: prikk.into(),
            sandbox,
            root,
            key_dir: key_dir.into(),
            secret_file: secret_file.into(),
            tmpfs_dir: tmpfs_dir.into(),
            #[cfg(feature = "test-knobs")]
            cheap_derivation: false,
            #[cfg(feature = "test-knobs")]
            stale_after: None,
        }
    }
}

/// What a wiki's keys look like from outside: ids and dates, never a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyStatus {
    /// The wiki.
    pub wiki: WikiId,
    /// 1 for the first pair of keys, one more for each rotation.
    pub generation: u32,
    /// The author key's id.
    pub author_key_id: String,
    /// The maintainer key's id.
    pub maintainer_key_id: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds of the latest rotation.
    pub rotated_at: Option<i64>,
}

/// The signer.
pub struct Signer {
    writes: Writes,
    store: KeyStore,
    seed_base: SeedBase,
    root: RootLayout,
    ledger: Arc<dyn AcceptanceLedger>,
    locks: WikiLocks,
    swept: usize,
}

impl Signer {
    /// Take the wiki's write lock for one act, or say who holds it.
    fn write_lock(&self, wiki: &WikiId) -> Result<WriteGuard<'_>, SignerError> {
        self.locks.take(&self.root, wiki)
    }
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signer(..)")
    }
}

/// The wiki's seeds for one operation, one role per signing call: each call decrypts the seed it signs with
/// into a directory of its own, on the tmpfs, and removes it when the call returns. A removal that fails does
/// not stop the sequence (a sealed change must be read back and verified whatever happened to a directory); it
/// is remembered, and the operation reports it when it ends ([`StoreSeeds::left_over`]).
struct StoreSeeds<'a> {
    store: &'a KeyStore,
    base: &'a SeedBase,
    wiki: &'a WikiId,
    leftover: std::cell::Cell<bool>,
}

impl StoreSeeds<'_> {
    /// Did a seed directory fail to be removed?
    fn left_over(&self) -> bool {
        self.leftover.get()
    }
}

impl SeedAccess for StoreSeeds<'_> {
    fn with_role<T>(
        &self,
        role: Role,
        f: impl FnOnce(&KeyDir) -> Result<T, SignerError>,
    ) -> Result<T, SignerError> {
        let (_meta, dir): (Meta, SeedDir) = self
            .store
            .checkout(self.wiki, self.base, role)
            .map_err(ks)?;
        let result = f(dir.key_dir());
        if dir.finish().is_err() {
            self.leftover.set(true);
        }
        result
    }
}

/// A key-store failure, in the signer's terms.
fn ks(e: KeyStoreError) -> SignerError {
    match e {
        KeyStoreError::NoKeys | KeyStoreError::Exists => SignerError::Refused(e.to_string()),
        KeyStoreError::Record(_) => SignerError::Damaged(e.to_string()),
        KeyStoreError::SeedsNotRemoved => SignerError::SeedsNotRemoved,
        KeyStoreError::Secret(_)
        | KeyStoreError::Io(..)
        | KeyStoreError::Locked
        | KeyStoreError::Header(_)
        | KeyStoreError::NoStore(_)
        | KeyStoreError::CreationBusy
        | KeyStoreError::SeedBase(_)
        | KeyStoreError::Random => SignerError::Unavailable(e.to_string()),
    }
}

/// A signer failure, in the acceptance's terms (before anything is committed).
fn accept_error(e: SignerError) -> AcceptError {
    match e {
        SignerError::Refused(m) => AcceptError::Refused(m),
        SignerError::Unavailable(m) => AcceptError::Unavailable(m),
        SignerError::Damaged(m) => {
            AcceptError::Unavailable(format!("prikk's output could not be read: {m}"))
        }
        SignerError::SeedsNotRemoved => AcceptError::SeedsNotRemoved { sealed: None },
        SignerError::CeilingExceeded { verb, bytes } => {
            AcceptError::CeilingExceeded { verb, bytes }
        }
        SignerError::Busy { holder, .. } => AcceptError::Busy { holder },
        other => AcceptError::Unavailable(other.to_string()),
    }
}

/// `heads/<name>`: the shape of a view's ref, checked before prikk sees it.
fn is_view_ref(s: &str) -> bool {
    !s.contains("//") && s.strip_prefix("heads/").is_some_and(is_ref_tail)
}

/// `heads/<name>` or `tags/<name>`: a published ref a view may start from.
fn is_start_ref(s: &str) -> bool {
    !s.contains("//")
        && s.strip_prefix("heads/")
            .or_else(|| s.strip_prefix("tags/"))
            .is_some_and(is_ref_tail)
}

fn is_ref_tail(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['.', '-'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
        && !name.contains("//")
        && !name.contains("/.")
        && !name.ends_with('/')
}

impl Signer {
    /// Open the signer: read the operator's secret, open the key store with it, check that the seed
    /// directory is on a tmpfs, and remove the seed directories a killed call left behind.
    ///
    /// **It never makes a key store**: a key directory with no store in it (a mistyped path, a store that was
    /// lost) is refused with the path named. The acts that were asked to create keys use
    /// [`Signer::open_creating_store`].
    pub fn open(
        config: SignerConfig,
        ledger: Arc<dyn AcceptanceLedger>,
    ) -> Result<Self, SignerError> {
        Self::open_inner(config, ledger, false)
    }

    /// As [`Signer::open`], but a key directory that is new or empty becomes a key store. For `inklkunn wiki
    /// keys create` and for creating a wiki, the two acts that are asked to bring keys into being; two
    /// processes doing it at once end with one store.
    pub fn open_creating_store(
        config: SignerConfig,
        ledger: Arc<dyn AcceptanceLedger>,
    ) -> Result<Self, SignerError> {
        Self::open_inner(config, ledger, true)
    }

    fn open_inner(
        config: SignerConfig,
        ledger: Arc<dyn AcceptanceLedger>,
        create_store: bool,
    ) -> Result<Self, SignerError> {
        let secret = Secret::from_file(&config.secret_file).map_err(ks)?;
        #[cfg(feature = "test-knobs")]
        let store = if config.cheap_derivation {
            KeyStore::open_cheap(&config.key_dir, &secret, create_store)
        } else if create_store {
            KeyStore::create_or_open(&config.key_dir, &secret)
        } else {
            KeyStore::open(&config.key_dir, &secret)
        };
        #[cfg(not(feature = "test-knobs"))]
        let store = if create_store {
            KeyStore::create_or_open(&config.key_dir, &secret)
        } else {
            KeyStore::open(&config.key_dir, &secret)
        };
        let store = store.map_err(ks)?;
        let seed_base = SeedBase::tmpfs(&config.tmpfs_dir).map_err(ks)?;
        #[cfg(feature = "test-knobs")]
        let stale_after = config.stale_after.unwrap_or(STALE_AFTER);
        #[cfg(not(feature = "test-knobs"))]
        let stale_after = STALE_AFTER;
        let swept = seed_base.sweep(stale_after).map_err(ks)?;
        Ok(Signer {
            writes: Writes::new(config.prikk, config.sandbox),
            store,
            seed_base,
            root: config.root,
            ledger,
            locks: WikiLocks::default(),
            swept,
        })
    }

    /// How many temporary directories of a killed call (the keys a write had on the tmpfs) were found and
    /// removed when the signer was opened. Nonzero means a previous process died in the middle of a write.
    pub fn leftover_call_directories_removed(&self) -> usize {
        self.swept
    }

    /// A wiki's key ids, generation and dates. The record is authenticated to read them.
    pub fn key_status(&self, wiki: &WikiId) -> Result<KeyStatus, SignerError> {
        let m = self.store.status(wiki).map_err(ks)?;
        Ok(KeyStatus {
            wiki: m.wiki,
            generation: m.generation,
            author_key_id: m.author_key_id,
            maintainer_key_id: m.maintainer_key_id,
            created_at: m.created_at,
            rotated_at: m.rotated_at,
        })
    }

    /// The wiki a repository directory belongs to: its name is a wiki id and it is where the root layout
    /// puts that wiki.
    fn wiki_of(&self, repo: &Path) -> Result<WikiId, SignerError> {
        let id = repo
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(WikiId::parse)
            .ok_or_else(|| {
                SignerError::Refused("that directory is not a wiki's repository".to_owned())
            })?;
        if self.root.repo_dir(&id) != repo {
            return Err(SignerError::Refused(
                "that directory is not where the root layout keeps the wiki".to_owned(),
            ));
        }
        Ok(id)
    }

    /// The wiki's seeds, handed out one role per call (see [`SeedAccess`]).
    fn seeds<'a>(&'a self, wiki: &'a WikiId) -> StoreSeeds<'a> {
        StoreSeeds {
            store: &self.store,
            base: &self.seed_base,
            wiki,
            leftover: std::cell::Cell::new(false),
        }
    }

    /// The renames, the writes and the removals of `plan`, in that order, recorded in `applied`/`moved`.
    fn apply(
        &self,
        repo: &Path,
        plan: &Plan,
        applied: &mut Applied,
        moved: &mut Vec<(String, String)>,
    ) -> Result<(), SignerError> {
        for (from, to) in &plan.renames {
            applied
                .ensure_parent(repo, to)
                .map_err(SignerError::Unavailable)?;
            self.writes.mv(repo, from, to)?;
            moved.push((from.clone(), to.clone()));
        }
        for (path, bytes) in &plan.writes {
            applied
                .write(repo, path, bytes)
                .map_err(SignerError::Unavailable)?;
        }
        for path in &plan.deletes {
            applied
                .delete(repo, path)
                .map_err(SignerError::Unavailable)?;
        }
        Ok(())
    }

    /// Put the worktree back after a failure before `commit` succeeded, and report `cause` with whether it
    /// was put back.
    fn restore(
        &self,
        repo: &Path,
        applied: Applied,
        moved: &[(String, String)],
        cause: SignerError,
    ) -> AcceptError {
        let files_back = applied.undo();
        let mut renames_back = true;
        for (from, to) in moved.iter().rev() {
            if self.writes.mv(repo, to, from).is_err() {
                renames_back = false;
            }
        }
        let clean = files_back && renames_back && self.writes.worktree_clean(repo).unwrap_or(false);
        let cause_text = match &cause {
            SignerError::Refused(m) | SignerError::Unavailable(m) | SignerError::Damaged(m) => {
                m.clone()
            }
            other => other.to_string(),
        };
        if !clean {
            return AcceptError::WorktreeDirty(cause_text);
        }
        match cause {
            SignerError::Refused(_) => AcceptError::Refused(cause_text),
            SignerError::CeilingExceeded { verb, bytes } => {
                AcceptError::CeilingExceeded { verb, bytes }
            }
            _ => AcceptError::Unavailable(format!("{cause_text}; the worktree is as it was")),
        }
    }

    /// The whole write sequence of an acceptance, with the seeds on disk.
    fn write_sequence(
        &self,
        repo: &Path,
        seeds: &StoreSeeds<'_>,
        view: &str,
        plan: &Plan,
        message: &str,
    ) -> Result<SealedChange, AcceptError> {
        let w = &self.writes;
        // Nothing unfinished: a queued patch would be sealed together with this change, and a dirty worktree
        // would be committed with it. Both are a previous call's leftovers, which recovery owns.
        let status = w.status(repo).map_err(accept_error)?;
        if status.queue.count != 0 {
            return Err(AcceptError::Refused(
                "the repository holds unsealed work; recovery completes or discards it first"
                    .to_owned(),
            ));
        }
        if !w.worktree_clean(repo).map_err(accept_error)? {
            return Err(AcceptError::Refused(
                "the worktree has uncommitted changes; recovery restores it first".to_owned(),
            ));
        }
        if status.current_branch.as_deref() != Some(view) {
            w.switch(repo, view).map_err(accept_error)?;
        }
        // Apply, then commit. A failure here puts the worktree back.
        let mut applied = Applied::default();
        let mut moved: Vec<(String, String)> = Vec::new();
        if let Err(e) = self.apply(repo, plan, &mut applied, &mut moved) {
            return Err(self.restore(repo, applied, &moved, e));
        }
        if let Err(e) = w.commit(repo, seeds, view, message, plan.change_bytes()) {
            return Err(self.restore(repo, applied, &moved, e));
        }
        // From here a patch is queued: until the seal succeeds a failure is `Queued`, which recovery completes;
        // after it, a failure is `SealedUnconfirmed`, which must never be retried as a new acceptance.
        w.seal(repo, seeds, view, plan.change_bytes())
            .map_err(|e| AcceptError::Queued {
                step: "seal",
                ceiling: match &e {
                    SignerError::CeilingExceeded { bytes, .. } => Some(*bytes),
                    _ => None,
                },
                detail: e.to_string(),
            })?;
        let unconfirmed = |sealed: Option<SealedChange>, why: Unconfirmed, detail: String| {
            AcceptError::SealedUnconfirmed {
                sealed,
                why,
                detail,
            }
        };
        let tip = w
            .tip(repo, view)
            .map_err(|e| {
                let why = if matches!(e, SignerError::CeilingExceeded { .. }) {
                    Unconfirmed::CeilingExceeded
                } else {
                    Unconfirmed::BlockNotRead
                };
                unconfirmed(None, why, e.to_string())
            })?
            .ok_or_else(|| {
                unconfirmed(
                    None,
                    Unconfirmed::BlockNotRead,
                    "the view has no block".to_owned(),
                )
            })?;
        // The block read back must be this change: a foreign write between our seal and our read would
        // otherwise be reported as ours.
        let ours = tip
            .patch_messages
            .first()
            .is_some_and(|m| m.message == message);
        if !ours {
            return Err(unconfirmed(
                None,
                Unconfirmed::NotOurs,
                "the newest block carries another message".to_owned(),
            ));
        }
        let sealed = SealedChange {
            block_id: tip.block_id,
            patch_ids: tip
                .patch_messages
                .iter()
                .map(|m| m.patch_id.clone())
                .collect(),
        };
        let verification = w.verify(repo).map_err(|e| {
            let why = if matches!(e, SignerError::CeilingExceeded { .. }) {
                Unconfirmed::CeilingExceeded
            } else {
                Unconfirmed::VerifyCouldNotRun
            };
            unconfirmed(Some(sealed.clone()), why, e.to_string())
        })?;
        if !verification.verdict.ok {
            return Err(unconfirmed(
                Some(sealed),
                Unconfirmed::VerifyFailed,
                "prikk's verify reported failed conditions".to_owned(),
            ));
        }
        Ok(sealed)
    }
}

impl Accept for Signer {
    fn accept(
        &self,
        wiki: &WikiId,
        view: &str,
        token: &AcceptanceToken,
        change: &PreparedChange,
    ) -> Result<SealedChange, AcceptError> {
        if !is_view_ref(view) {
            return Err(AcceptError::Refused("a view is `heads/<name>`".to_owned()));
        }
        // Held from the guard to the end: the guard's answer stays true while the files are written.
        let _held = self.write_lock(wiki).map_err(accept_error)?;
        // 1. The guard: the record the token names, read here, and what the message says. Nothing is
        //    decrypted or written until both agree.
        let expected = self
            .ledger
            .pending(token)
            .map_err(|e| AcceptError::Unavailable(e.to_string()))?
            .ok_or(AcceptError::NotPending)?;
        if expected.wiki != *wiki || expected.view != view {
            return Err(AcceptError::NotPending);
        }
        let message = Message::parse(&change.message).map_err(|_| AcceptError::MessageMismatch)?;
        if !expected.matches(&message) {
            return Err(AcceptError::MessageMismatch);
        }
        // 1b. The files are the ones that were accepted: the record's digest against the change in hand.
        if change.digest() != expected.change_digest {
            return Err(AcceptError::ChangeMismatch);
        }
        // 2. The change itself, validated whole before the worktree is touched.
        let plan = Plan::new(change).map_err(|why| AcceptError::Refused(why.to_owned()))?;
        // 3. The sequence. Each signing call has the one seed it signs with on disk for exactly its duration.
        let repo = self.root.repo_dir(wiki);
        let seeds = self.seeds(wiki);
        let result = self.write_sequence(&repo, &seeds, view, &plan, &change.message);
        match (result, seeds.left_over()) {
            (Ok(sealed), false) => Ok(sealed),
            (Ok(sealed), true) => Err(AcceptError::SeedsNotRemoved {
                sealed: Some(sealed),
            }),
            (Err(e), false) => Err(e),
            (Err(e), true) => Err(note_leftover(e)),
        }
    }

    fn create_view(
        &self,
        wiki: &WikiId,
        name: &str,
        from: &str,
        act: &ViewCreationAct,
    ) -> Result<(), AcceptError> {
        if !is_view_ref(name) || !is_start_ref(from) {
            return Err(AcceptError::Refused(
                "a view is `heads/<name>` and starts from a `heads/` or `tags/` ref".to_owned(),
            ));
        }
        let _held = self.write_lock(wiki).map_err(accept_error)?;
        let facts = self
            .ledger
            .view_creation(act)
            .map_err(|e| AcceptError::Unavailable(e.to_string()))?
            .ok_or(AcceptError::NotPending)?;
        if facts.wiki != *wiki || facts.name != name || facts.from != from {
            return Err(AcceptError::NotPending);
        }
        let repo = self.root.repo_dir(wiki);
        let seeds = self.seeds(wiki);
        let result = self.writes.branch_create(&repo, &seeds, name, from);
        match (result, seeds.left_over()) {
            (Ok(()), false) => Ok(()),
            (Ok(()), true) => Err(AcceptError::SeedsNotRemoved { sealed: None }),
            (Err(e), false) => Err(accept_error(e)),
            (Err(e), true) => Err(note_leftover(accept_error(e))),
        }
    }
}

/// Add to an operator-facing failure that the seeds' temporary directory is also still there.
fn note_leftover(e: AcceptError) -> AcceptError {
    const NOTE: &str = "; also, a temporary directory of decrypted seeds could not be removed (the next start removes it)";
    match e {
        AcceptError::Refused(m) => AcceptError::Refused(format!("{m}{NOTE}")),
        AcceptError::Unavailable(m) => AcceptError::Unavailable(format!("{m}{NOTE}")),
        AcceptError::WorktreeDirty(m) => AcceptError::WorktreeDirty(format!("{m}{NOTE}")),
        AcceptError::Queued {
            step,
            detail,
            ceiling,
        } => AcceptError::Queued {
            step,
            detail: format!("{detail}{NOTE}"),
            ceiling,
        },
        AcceptError::SealedUnconfirmed {
            sealed,
            why,
            detail,
        } => AcceptError::SealedUnconfirmed {
            sealed,
            why,
            detail: format!("{detail}{NOTE}"),
        },
        other => other,
    }
}

impl Signer {
    /// Generate a wiki's two keys with prikk in a scratch directory, adopt the maintainer key in `repo`, and
    /// store the sealed record **last**: a failure before that leaves no record. `init` first makes the
    /// repository (wiki creation); without it the repository must already exist (keys for a wiki that has
    /// none). The key ids are `inklkunn-<wiki id>-author` and `-maintainer`.
    fn provision(&self, repo: &Path, init: bool) -> Result<KeyIds, SignerError> {
        let wiki = self.wiki_of(repo)?;
        let _held = self.write_lock(&wiki)?;
        if self.store.status(&wiki).is_ok() {
            return Err(SignerError::Refused("the wiki already has keys".to_owned()));
        }
        let author_id = format!("inklkunn-{wiki}-author");
        let maintainer_id = format!("inklkunn-{wiki}-maintainer");
        let scratch = self.seed_base.scratch().map_err(ks)?;
        let result = (|| -> Result<KeyIds, SignerError> {
            let keys = KeyDir::with_ids(scratch.path(), &author_id, &maintainer_id)?;
            if init {
                self.writes.init(repo)?;
            }
            self.writes.generate_key(&keys, "author")?;
            self.writes.generate_key(&keys, "maintainer")?;
            let public = self.writes.public_key(&keys, "maintainer")?;
            self.writes
                .trust_maintainer(repo, &maintainer_id, &public)?;
            let seeds = read_seeds(&keys)?;
            let meta = Meta {
                wiki: wiki.clone(),
                generation: 1,
                author_key_id: author_id.clone(),
                maintainer_key_id: maintainer_id.clone(),
                created_at: inklkunn_store::record::now_secs(),
                rotated_at: None,
                superseded: Vec::new(),
            };
            self.store.create(&meta, &seeds).map_err(ks)?;
            Ok(KeyIds {
                author: author_id.clone(),
                maintainer: maintainer_id.clone(),
                maintainer_public_key: public,
            })
        })();
        match (result, scratch.finish()) {
            (Ok(ids), Ok(())) => Ok(ids),
            (Ok(_), Err(e)) => Err(ks(e)),
            (Err(e), _) => Err(e),
        }
    }
}

/// The two generated seeds of a key directory, as the key store wants them. The bytes read in between are
/// wiped when they go out of scope, as the seeds are.
fn read_seeds(keys: &KeyDir) -> Result<Seeds, SignerError> {
    let read = |role: &str| {
        std::fs::read(keys.seed_file(role))
            .map(zeroize::Zeroizing::new)
            .map_err(|e| {
                SignerError::Unavailable(format!(
                    "a generated key could not be read ({})",
                    e.kind()
                ))
            })
    };
    let (author, maintainer) = (read("author")?, read("maintainer")?);
    Seeds::new(&author, &maintainer).map_err(|e| SignerError::Damaged(e.to_string()))
}

impl RepositoryBootstrap for Signer {
    /// Create the repository in `repo` (the directory the root layout keeps a wiki in, empty), generate the
    /// wiki's two keys with prikk, adopt the maintainer key, and store the sealed record **last**: a
    /// failure before that leaves no record.
    fn create_repository(
        &self,
        repo: &Path,
        _authority: &KeyAuthority,
    ) -> Result<KeyIds, SignerError> {
        self.provision(repo, true)
    }

    /// Remove the sealed key record `create_repository` stored for a creation that did not finish. Only for a
    /// wiki whose creation failed: it is called with the id's own repository directory, which the caller has
    /// just removed, and it removes nothing but that wiki's record.
    fn discard_unfinished(&self, repo: &Path) -> Result<(), SignerError> {
        let wiki = self.wiki_of(repo)?;
        let _held = self.write_lock(&wiki)?;
        self.store.remove(&wiki).map(|_| ()).map_err(ks)
    }

    /// Commit the worktree as the repository's **first** change and seal it. Refused unless the message is a
    /// `system` message and the repository has no history yet: this entry has no acceptance record behind it,
    /// so it can only ever create the first block of a wiki that is being created.
    fn commit_and_seal(&self, repo: &Path, message: &str) -> Result<SealedIds, SignerError> {
        let wiki = self.wiki_of(repo)?;
        let _held = self.write_lock(&wiki)?;
        let parsed = Message::parse(message).map_err(|_| {
            SignerError::Refused("the first change's message is not a patch message".to_owned())
        })?;
        if parsed.kind() != Kind::System {
            return Err(SignerError::Refused(
                "the first change of a wiki is a system change".to_owned(),
            ));
        }
        if self.writes.head(repo)?.is_some() {
            return Err(SignerError::Refused(
                "the repository already has history; changes go through an acceptance".to_owned(),
            ));
        }
        let seeds = self.seeds(&wiki);
        let result = self.writes.commit_and_seal(repo, &seeds, message);
        match (result, seeds.left_over()) {
            (Ok(ids), false) => Ok(ids),
            (Ok(_), true) => Err(SignerError::SeedsNotRemoved),
            (Err(e), _) => Err(e),
        }
    }
}

pub(crate) mod lifecycle;
pub(crate) mod superseded;

#[cfg(test)]
mod tests;
