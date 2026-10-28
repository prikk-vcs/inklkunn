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
//! and says so, or says plainly that it could not; a failure **after** it is [`AcceptError::Incomplete`],
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

use crate::RepositoryBootstrap;
use crate::accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, PreparedChange, SealedChange,
    ViewCreationAct,
};
use crate::key_store::{KeyStore, KeyStoreError, Meta, Secret, SeedBase, SeedDir, Seeds};
use crate::keys::{KeyDir, KeyIds};
use crate::message::{Kind, Message};
use crate::worktree::{Applied, Plan};
use crate::writes::{SealedIds, SignerError, Writes};

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
    swept: usize,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signer(..)")
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
    pub fn open(
        config: SignerConfig,
        ledger: Arc<dyn AcceptanceLedger>,
    ) -> Result<Self, SignerError> {
        let secret = Secret::from_file(&config.secret_file).map_err(ks)?;
        #[cfg(feature = "test-knobs")]
        let store = if config.cheap_derivation {
            KeyStore::open_cheap(&config.key_dir, &secret)
        } else {
            KeyStore::open(&config.key_dir, &secret)
        };
        #[cfg(not(feature = "test-knobs"))]
        let store = KeyStore::open(&config.key_dir, &secret);
        let store = store.map_err(ks)?;
        let seed_base = SeedBase::tmpfs(&config.tmpfs_dir).map_err(ks)?;
        #[cfg(feature = "test-knobs")]
        let stale_after = config.stale_after.unwrap_or(STALE_AFTER);
        #[cfg(not(feature = "test-knobs"))]
        let stale_after = STALE_AFTER;
        let swept = seed_base.sweep_stale(stale_after).map_err(ks)?;
        Ok(Signer {
            writes: Writes::new(config.prikk, config.sandbox),
            store,
            seed_base,
            root: config.root,
            ledger,
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

    /// Run `f` with the wiki's seeds on disk for exactly its duration. The directory is removed afterwards
    /// and a removal that fails is [`SignerError::SeedsNotRemoved`] unless `f` failed first (then `f`'s
    /// failure is the story and the leftover is the next start's sweep).
    fn with_seeds<T>(
        &self,
        wiki: &WikiId,
        f: impl FnOnce(&KeyDir) -> Result<T, SignerError>,
    ) -> Result<T, SignerError> {
        let (_meta, dir): (Meta, SeedDir) =
            self.store.checkout(wiki, &self.seed_base).map_err(ks)?;
        let result = f(dir.key_dir());
        let cleanup = dir.finish();
        match (result, cleanup) {
            (Ok(v), Ok(())) => Ok(v),
            (Ok(_), Err(e)) => Err(ks(e)),
            (Err(e), _) => Err(e),
        }
    }

    /// The renames, the writes and the removals of `plan`, in that order, recorded in `applied`/`moved`.
    fn apply(
        &self,
        repo: &Path,
        keys: &KeyDir,
        plan: &Plan,
        applied: &mut Applied,
        moved: &mut Vec<(String, String)>,
    ) -> Result<(), SignerError> {
        for (from, to) in &plan.renames {
            applied
                .ensure_parent(repo, to)
                .map_err(SignerError::Unavailable)?;
            self.writes.mv(repo, keys, from, to)?;
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
        keys: &KeyDir,
        applied: Applied,
        moved: &[(String, String)],
        cause: SignerError,
    ) -> AcceptError {
        let files_back = applied.undo();
        let mut renames_back = true;
        for (from, to) in moved.iter().rev() {
            if self.writes.mv(repo, keys, to, from).is_err() {
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
            return AcceptError::Unavailable(format!(
                "{cause_text}; the worktree could not be put back as it was, and recovery must restore it"
            ));
        }
        match cause {
            SignerError::Refused(_) => AcceptError::Refused(cause_text),
            _ => AcceptError::Unavailable(format!("{cause_text}; the worktree is as it was")),
        }
    }

    /// The whole write sequence of an acceptance, with the seeds on disk.
    fn write_sequence(
        &self,
        repo: &Path,
        keys: &KeyDir,
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
            w.switch(repo, keys, view).map_err(accept_error)?;
        }
        // Apply, then commit. A failure here puts the worktree back.
        let mut applied = Applied::default();
        let mut moved: Vec<(String, String)> = Vec::new();
        if let Err(e) = self.apply(repo, keys, plan, &mut applied, &mut moved) {
            return Err(self.restore(repo, keys, applied, &moved, e));
        }
        if let Err(e) = w.commit(repo, keys, view, message) {
            return Err(self.restore(repo, keys, applied, &moved, e));
        }
        // From here a patch is queued: every failure is `Incomplete`, which recovery completes.
        w.seal(repo, keys, view)
            .map_err(|e| AcceptError::Incomplete(format!("seal: {e}")))?;
        let tip = w
            .tip(repo, view)
            .map_err(|e| {
                AcceptError::Incomplete(format!(
                    "sealed, but the block could not be read back: {e}"
                ))
            })?
            .ok_or_else(|| {
                AcceptError::Incomplete("sealed, but the view has no block".to_owned())
            })?;
        // The block read back must be this change: a foreign write between our seal and our read would
        // otherwise be reported as ours.
        let ours = tip
            .patch_messages
            .first()
            .is_some_and(|m| m.message == message);
        if !ours {
            return Err(AcceptError::Incomplete(
                "sealed, but the newest block is not the change that was written".to_owned(),
            ));
        }
        let verification = w.verify(repo).map_err(|e| {
            AcceptError::Incomplete(format!("sealed, but verification could not run: {e}"))
        })?;
        if !verification.verdict.ok {
            return Err(AcceptError::Incomplete(
                "sealed, but the repository did not verify afterwards".to_owned(),
            ));
        }
        Ok(SealedChange {
            block_id: tip.block_id,
            patch_ids: tip
                .patch_messages
                .iter()
                .map(|m| m.patch_id.clone())
                .collect(),
        })
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
        // 2. The change itself, validated whole before the worktree is touched.
        let plan = Plan::new(change).map_err(|why| AcceptError::Refused(why.to_owned()))?;
        // 3. The sequence, with the seeds on disk for exactly its duration.
        let repo = self.root.repo_dir(wiki);
        let (_meta, seeds) = self
            .store
            .checkout(wiki, &self.seed_base)
            .map_err(|e| accept_error(ks(e)))?;
        let result = self.write_sequence(&repo, seeds.key_dir(), view, &plan, &change.message);
        match (result, seeds.finish()) {
            (Ok(sealed), Ok(())) => Ok(sealed),
            (Ok(sealed), Err(_)) => Err(AcceptError::SeedsNotRemoved {
                sealed: Some(sealed),
            }),
            (Err(e), Ok(())) => Err(e),
            (Err(e), Err(_)) => Err(note_leftover(e)),
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
        let facts = self
            .ledger
            .view_creation(act)
            .map_err(|e| AcceptError::Unavailable(e.to_string()))?
            .ok_or(AcceptError::NotPending)?;
        if facts.wiki != *wiki || facts.name != name || facts.from != from {
            return Err(AcceptError::NotPending);
        }
        let repo = self.root.repo_dir(wiki);
        self.with_seeds(wiki, |keys| {
            self.writes.branch_create(&repo, keys, name, from)
        })
        .map_err(|e| match e {
            SignerError::SeedsNotRemoved => AcceptError::SeedsNotRemoved { sealed: None },
            other => accept_error(other),
        })
    }
}

/// Add to an operator-facing failure that the seeds' temporary directory is also still there.
fn note_leftover(e: AcceptError) -> AcceptError {
    const NOTE: &str = "; also, a temporary directory of decrypted seeds could not be removed (the next start removes it)";
    match e {
        AcceptError::Refused(m) => AcceptError::Refused(format!("{m}{NOTE}")),
        AcceptError::Incomplete(m) => AcceptError::Incomplete(format!("{m}{NOTE}")),
        AcceptError::Unavailable(m) => AcceptError::Unavailable(format!("{m}{NOTE}")),
        other => other,
    }
}

impl RepositoryBootstrap for Signer {
    /// Create the repository in `repo` (the directory the root layout keeps a wiki in, empty), generate the
    /// wiki's two keys with prikk, adopt the maintainer key, and store the sealed record **last**: a
    /// failure before that leaves no record. The key ids are `inklkunn-<wiki id>-author` and `-maintainer`.
    fn create_repository(&self, repo: &Path) -> Result<KeyIds, SignerError> {
        let wiki = self.wiki_of(repo)?;
        if self.store.status(&wiki).is_ok() {
            return Err(SignerError::Refused("the wiki already has keys".to_owned()));
        }
        let author_id = format!("inklkunn-{wiki}-author");
        let maintainer_id = format!("inklkunn-{wiki}-maintainer");
        let scratch = self.seed_base.scratch().map_err(ks)?;
        let result = (|| -> Result<KeyIds, SignerError> {
            let keys = KeyDir::with_ids(scratch.path(), &author_id, &maintainer_id)?;
            self.writes.init(repo, &keys)?;
            self.writes.generate_key(&keys, "author")?;
            self.writes.generate_key(&keys, "maintainer")?;
            let public = self.writes.public_key(&keys, "maintainer")?;
            self.writes
                .trust_maintainer(repo, &keys, &maintainer_id, &public)?;
            let read = |role: &str| {
                std::fs::read(keys.seed_file(role)).map_err(|e| {
                    SignerError::Unavailable(format!(
                        "a generated key could not be read ({})",
                        e.kind()
                    ))
                })
            };
            let seeds = Seeds::new(&read("author")?, &read("maintainer")?)
                .map_err(|e| SignerError::Damaged(e.to_string()))?;
            let meta = Meta {
                wiki: wiki.clone(),
                generation: 1,
                author_key_id: author_id.clone(),
                maintainer_key_id: maintainer_id.clone(),
                created_at: inklkunn_store::record::now_secs(),
                rotated_at: None,
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

    /// Commit the worktree as the repository's **first** change and seal it. Refused unless the message is a
    /// `system` message and the repository has no history yet: this entry has no acceptance record behind it,
    /// so it can only ever create the first block of a wiki that is being created.
    fn commit_and_seal(&self, repo: &Path, message: &str) -> Result<SealedIds, SignerError> {
        let wiki = self.wiki_of(repo)?;
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
        self.with_seeds(&wiki, |keys| {
            self.writes.commit_and_seal(repo, keys, message)
        })
    }
}

#[cfg(test)]
mod tests;
