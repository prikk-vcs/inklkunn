//! *Create wiki* (RFC 001 D-5, D-8, D-9): the record, the root layout, the repository, the bundle files,
//! the first page sealed as a system change, and the first verification. **All or nothing**: the record is
//! built first but stored last, after the repository exists, is sealed and verifies; a failure anywhere
//! before that removes the directories the attempt made (the id was fresh and nothing referenced them) and
//! says so. The write steps go through [`RepositoryBootstrap`], the signer's seam.
//!
//! **Kill-safe (handoff 003 T7; hold note IS-13).** A failure that the process survives is cleaned up by the
//! code below; a `SIGKILL` runs no code at all. So the creation is bracketed by a **creation mark**
//! ([`CreationMark`]: made durable before the first directory, locked while the creation runs, removed after the
//! record is stored), and [`sweep_unfinished_creations`], run at every start, removes whole whatever a creation
//! whose process is gone left: the repository directory, the scratch directory, the lock file and the sealed
//! key record. A kill at any step therefore leaves either a complete wiki (the record is stored) or nothing
//! that survives the next start, **except** that the sweep removes nothing from a repository that already
//! holds a sealed block, nor when the store is one this start made (R9): those wait for the operator.
//! [`create_wiki_observed`] names the steps so that a test can stop the process
//! (or panic, which runs no cleanup either) after each one.
//!
//! **The first change is a recorded act** (handoff 004 T4b): once the wiki's record is stored, the creation is
//! kept as a sealed system act (`act`), so the reconciliation finds a record behind the first block. A creation
//! killed between the two leaves a stored wiki whose act is missing, and the sweep writes it from the history
//! before it removes the mark.

use std::path::{Path, PathBuf};

use inklkunn_prikk::{Point, Storage, StorageError};
pub use inklkunn_signer::RepositoryBootstrap;
use inklkunn_signer::message::{Message, Summary, SystemLabel};
use inklkunn_signer::{CreationMark, KeyAuthority, find_unfinished};
use inklkunn_store::{
    ActStore, NewWiki, RootLayout, StoreError, Verification, VerificationState, WikiId, WikiRecord,
    WikiStore,
};

use crate::bundle;

mod act;
#[cfg(test)]
mod tests;

/// What happened to the directories of a failed creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cleanup {
    /// The repository and scratch directories were removed; nothing is left of the attempt.
    Removed,
    /// Removal failed; the directory (the id's, never the name's) and the error, for the operator.
    LeftBehind {
        /// The repository directory.
        repo: PathBuf,
        /// Why it could not be removed.
        error: String,
    },
    /// The directories are gone but the wiki's sealed key record could not be removed from the key store;
    /// the id (never the name) and the error, for the operator.
    KeysLeftBehind {
        /// The unfinished wiki's id: its record is `<id>.key.json` in the key directory.
        wiki: WikiId,
        /// Why it could not be removed.
        error: String,
    },
}

impl std::fmt::Display for Cleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cleanup::Removed => write!(f, "nothing was left on disk"),
            Cleanup::LeftBehind { repo, error } => write!(
                f,
                "the unfinished repository at {} could not be removed ({error}); remove it by hand",
                repo.display()
            ),
            Cleanup::KeysLeftBehind { wiki, error } => write!(
                f,
                "the key record {wiki}.key.json of the unfinished wiki could not be removed ({error}); remove it from the key directory by hand"
            ),
        }
    }
}

/// Why a wiki was not created. No variant leaves a record behind; the ones that touched the disk say what
/// became of it.
#[derive(Debug)]
pub enum CreateWikiError {
    /// The creation could not be marked (see the module documentation): **nothing was made.** Another act holds
    /// this wiki's mark, or the root could not be written.
    Marked(inklkunn_signer::SignerError),
    /// The request was refused before anything was made: the slug is taken or invalid, a field is
    /// invalid, or the store failed.
    Store(StoreError),
    /// The root directories could not be made.
    Layout(std::io::Error),
    /// The signer could not create the repository or seal the first change.
    Signer {
        /// `create` (init and trust) or `seal` (commit and seal).
        step: &'static str,
        /// What the signer said.
        error: inklkunn_signer::SignerError,
        /// What became of the directories.
        cleanup: Cleanup,
    },
    /// The bundle files could not be written.
    Files {
        /// The I/O error.
        error: std::io::Error,
        /// What became of the directories.
        cleanup: Cleanup,
    },
    /// The new repository did not verify (`None`), or verification could not run (`Some`).
    Verification {
        /// The storage error, when the call did not complete (boxed: a storage error carries prikk's text,
        /// and the result of *create wiki* is returned by value).
        error: Option<Box<StorageError>>,
        /// What became of the directories.
        cleanup: Cleanup,
    },
}

impl std::fmt::Display for CreateWikiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateWikiError::Marked(e) => {
                write!(f, "the wiki was not created, and nothing was made: {e}")
            }
            CreateWikiError::Store(e) => write!(f, "the wiki was not created: {e}"),
            CreateWikiError::Layout(e) => {
                write!(f, "the wiki's directories could not be created: {e}")
            }
            CreateWikiError::Signer {
                step,
                error,
                cleanup,
            } => write!(
                f,
                "the repository's `{step}` step failed: {error}; {cleanup}"
            ),
            CreateWikiError::Files { error, cleanup } => {
                write!(
                    f,
                    "the bundle files could not be written: {error}; {cleanup}"
                )
            }
            CreateWikiError::Verification {
                error: Some(e),
                cleanup,
            } => write!(
                f,
                "the new repository could not be verified: {e}; {cleanup}"
            ),
            CreateWikiError::Verification {
                error: None,
                cleanup,
            } => write!(f, "the new repository failed verification; {cleanup}"),
        }
    }
}

impl std::error::Error for CreateWikiError {}

/// A created wiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWiki {
    /// The record, with key ids and the first verification recorded.
    pub record: WikiRecord,
    /// The repository directory.
    pub repo: PathBuf,
    /// The first sealed block.
    pub first_block_id: String,
    /// The creation is kept as a sealed system act. `false`: the wiki is whole and its history is sound, the
    /// record could not be kept now (the store failed), the creation mark stays and the next start's sweep
    /// keeps it. The caller says so to the operator.
    pub first_change_recorded: bool,
}

pub use inklkunn_signer::message::SUMMARY_MAX_CHARS;

/// A CT-03 message for a change the service itself makes (`Kind: system`): the summary line (made one line
/// and cut to [`SUMMARY_MAX_CHARS`] characters by [`Summary::from_text`]), an empty line, then the trailers
/// in RFC 002 D-5's order. A system change has no proposal and no base, so those trailers are absent. The
/// text is [`Message::to_text`] of a system message: one grammar writes it and reads it back.
pub fn system_message(summary: &str) -> String {
    let summary = Summary::from_text(summary).unwrap_or_else(|_| Summary::system_change());
    Message::system(summary, SystemLabel::inklkunn()).to_text()
}

/// The message of the change that creates a wiki.
pub fn creation_message(record: &WikiRecord) -> String {
    system_message(&format!("Create the wiki {}", record.display_name))
}

/// Remove everything a creation that did not finish made: whatever the signer kept elsewhere (the sealed key
/// record), then the directories (the repository, the scratch directory, the lock file; the lock file last,
/// because the signer's removal of the key record takes it). Both are tried; the first thing left behind is
/// what is reported.
fn discard(layout: &RootLayout, signer: &dyn RepositoryBootstrap, id: &WikiId) -> Cleanup {
    let keys = signer.discard_unfinished(&layout.repo_dir(id));
    let directories = layout.discard_unfinished(id);
    match (directories, keys) {
        (Err(e), _) => Cleanup::LeftBehind {
            repo: layout.repo_dir(id),
            error: e.to_string(),
        },
        (Ok(()), Err(e)) => Cleanup::KeysLeftBehind {
            wiki: id.clone(),
            error: e.to_string(),
        },
        (Ok(()), Ok(())) => Cleanup::Removed,
    }
}

/// A creation that ends here is over: when everything it made is gone its mark is removed, and when something
/// was left the mark stays, so that the next start's sweep finishes the removal. Returns `cleanup` unchanged.
fn settle(mark: CreationMark, cleanup: Cleanup) -> Cleanup {
    if cleanup == Cleanup::Removed {
        // A mark that cannot be removed is harmless: the sweep finds no record and no directories and clears it.
        let _ = mark.finish();
    }
    cleanup
}

/// The steps of a creation, each named **after it completed**, for [`create_wiki_observed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreationStep {
    /// The creation mark is durable; nothing else exists.
    Marked,
    /// The repository and scratch directories exist.
    Prepared,
    /// `prikk init` ran, the keys exist and their sealed record is in the key store.
    RepositoryCreated,
    /// The bundle files are in the worktree.
    FilesWritten,
    /// The first change is committed and sealed.
    Sealed,
    /// The new repository verified.
    Verified,
    /// The wiki's record is stored. The wiki exists from here; the record of its first change is still to be
    /// kept (a kill now is made whole by the next start's sweep).
    Stored,
    /// The first change is kept as a sealed system act: from here the wiki is complete and only the mark remains.
    ActRecorded,
}

impl CreationStep {
    /// Every step, in order.
    pub const ALL: [CreationStep; 8] = [
        CreationStep::Marked,
        CreationStep::Prepared,
        CreationStep::RepositoryCreated,
        CreationStep::FilesWritten,
        CreationStep::Sealed,
        CreationStep::Verified,
        CreationStep::Stored,
        CreationStep::ActRecorded,
    ];
}

/// Create a wiki (an act that brings keys into being, so it takes the process's [`KeyAuthority`]): check the
/// slug, build the record, **mark the creation**, make the directories, create the repository, write the
/// bundle files, seal the first change, verify, and only then store the record, keep the first change as a
/// system act in `acts` and remove the mark.
#[allow(clippy::too_many_arguments)]
pub fn create_wiki(
    store: &dyn WikiStore,
    acts: &dyn ActStore,
    layout: &RootLayout,
    signer: &dyn RepositoryBootstrap,
    storage: &dyn Storage,
    new: NewWiki,
    inklkunn_version: &str,
    authority: &KeyAuthority,
) -> Result<CreatedWiki, CreateWikiError> {
    create_wiki_observed(
        store,
        acts,
        layout,
        signer,
        storage,
        new,
        inklkunn_version,
        authority,
        &|_| {},
    )
}

/// As [`create_wiki`], calling `observe` after each completed step. A test makes `observe` panic at the step it
/// wants to stop after: a panic runs none of the cleanup above (it is explicit code on the error paths, not a
/// destructor), exactly as `SIGKILL` runs none, and what is left on disk is what the next start's sweep must
/// find. Production code passes a function that does nothing.
#[allow(clippy::too_many_arguments)]
pub fn create_wiki_observed(
    store: &dyn WikiStore,
    acts: &dyn ActStore,
    layout: &RootLayout,
    signer: &dyn RepositoryBootstrap,
    storage: &dyn Storage,
    new: NewWiki,
    inklkunn_version: &str,
    authority: &KeyAuthority,
    observe: &dyn Fn(CreationStep),
) -> Result<CreatedWiki, CreateWikiError> {
    // 1. A taken slug is refused before anything touches the disk (the insert at the end is the guarantee
    //    against a race; this is the early, cheap answer).
    match store.by_slug(&new.slug) {
        Ok(_) => {
            return Err(CreateWikiError::Store(StoreError::SlugTaken(
                new.slug.to_string(),
            )));
        }
        Err(StoreError::NotFound) => {}
        Err(e) => return Err(CreateWikiError::Store(e)),
    }
    // 2. The record, validated, with a fresh id; not stored yet.
    let mut record = WikiRecord::from_new(WikiId::new(), new, inklkunn_store::record::now_secs())
        .map_err(|e| CreateWikiError::Store(StoreError::InvalidRequest(e)))?;
    let id = record.id.clone();
    // 3. The mark: durable before the first directory, held until the end.
    let mark = CreationMark::begin(layout, &id).map_err(CreateWikiError::Marked)?;
    observe(CreationStep::Marked);
    // 4. The directories.
    let repo = match layout.prepare(&id) {
        Ok(r) => r,
        Err(e) => {
            settle(mark, discard(layout, signer, &id));
            return Err(CreateWikiError::Layout(e));
        }
    };
    observe(CreationStep::Prepared);
    // 5. The repository and the keys.
    let keys = match signer.create_repository(&repo, authority) {
        Ok(k) => k,
        Err(error) => {
            return Err(CreateWikiError::Signer {
                step: "create",
                error,
                cleanup: settle(mark, discard(layout, signer, &id)),
            });
        }
    };
    observe(CreationStep::RepositoryCreated);
    record.author_key_id = Some(keys.author);
    record.maintainer_key_id = Some(keys.maintainer);
    // 6. The bundle files.
    if let Err(error) = bundle::write_layout(&repo, &record, inklkunn_version) {
        return Err(CreateWikiError::Files {
            error,
            cleanup: settle(mark, discard(layout, signer, &id)),
        });
    }
    observe(CreationStep::FilesWritten);
    // 7. The first change, sealed.
    let sealed = match signer.commit_and_seal(&repo, &creation_message(&record)) {
        Ok(s) => s,
        Err(error) => {
            return Err(CreateWikiError::Signer {
                step: "seal",
                error,
                cleanup: settle(mark, discard(layout, signer, &id)),
            });
        }
    };
    observe(CreationStep::Sealed);
    // 8. Verification: a fresh repository that does not verify is not a wiki.
    let verification = match storage.verify(&repo) {
        Ok(v) => v,
        Err(e) => {
            return Err(CreateWikiError::Verification {
                error: Some(Box::new(e)),
                cleanup: settle(mark, discard(layout, signer, &id)),
            });
        }
    };
    if !verification.verdict.ok {
        return Err(CreateWikiError::Verification {
            error: None,
            cleanup: settle(mark, discard(layout, signer, &id)),
        });
    }
    observe(CreationStep::Verified);
    record.verification = Verification {
        state: VerificationState::Verified,
        at: Some(inklkunn_store::record::now_secs()),
    };
    // 9. The record, last. A lost slug race is reported as the slug being taken, and the attempt removed.
    let record = match store.insert(record) {
        Ok(r) => r,
        Err(e) => {
            settle(mark, discard(layout, signer, &id));
            return Err(CreateWikiError::Store(e));
        }
    };
    observe(CreationStep::Stored);
    // 10. The first change, kept as a sealed system act (it refers to the wiki's record, so it cannot come
    //     before it). If the store cannot take it now the mark stays, and the next start's sweep keeps it.
    let message = creation_message(&record);
    let first_change_recorded = act::record(
        acts,
        &record,
        &message,
        &sealed.block_id,
        sealed.patch_ids.first().map_or("", String::as_str),
        inklkunn_store::record::now_secs(),
    )
    .is_ok();
    if first_change_recorded {
        observe(CreationStep::ActRecorded);
        // 11. The mark goes last. One that cannot be removed is harmless: the record and the act exist, so the
        //     sweep removes the mark and nothing else.
        let _ = mark.finish();
    }
    Ok(CreatedWiki {
        record,
        repo,
        first_block_id: sealed.block_id,
        first_change_recorded,
    })
}

/// What a start-up sweep did (see [`sweep_unfinished_creations`]).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    /// Unfinished creations removed whole (the repository, the scratch directory, the lock file, the key record
    /// and the mark): there was no record, so there was no wiki.
    pub removed: Vec<WikiId>,
    /// Creations that had finished storing their record and only lacked their mark's removal: the wiki is
    /// complete and the mark is gone.
    pub completed: Vec<WikiId>,
    /// Creations running right now in another process, left alone.
    pub live: Vec<WikiId>,
    /// Unfinished creations that could not be removed whole, and why. Their marks stay, so the next start tries
    /// again; the text is for the operator.
    pub left_behind: Vec<(WikiId, String)>,
}

/// The audit trail of a sweep, kept by the caller (it knows the principal and the sink). **The act is audited
/// before it is done** (S4), as every administrative act is: [`SweepAudit::before`] is asked ahead of each removal and a
/// refusal means nothing is removed; [`SweepAudit::after`] says what became of it, and may fail without
/// undoing anything (the caller reports it).
pub trait SweepAudit {
    /// Record that the unfinished creation `id` is about to be removed. `Err` (the trail cannot take it) stops
    /// the removal: the creation is left behind, its mark kept, the reason is the error text.
    fn before(&self, id: &WikiId) -> Result<(), String>;
    /// Record the outcome for `id`: `removed`, or left behind (including a creation the sweep would not touch,
    /// for which `before` was never asked).
    fn after(&self, id: &WikiId, removed: bool);
}

/// A sweep that audits nowhere (tests of the sweep itself).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSweepAudit;

impl SweepAudit for NoSweepAudit {
    fn before(&self, _: &WikiId) -> Result<(), String> {
        Ok(())
    }
    fn after(&self, _: &WikiId, _: bool) {}
}

impl SweepReport {
    /// Did the sweep find anything at all?
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty()
            && self.completed.is_empty()
            && self.live.is_empty()
            && self.left_behind.is_empty()
    }
}

/// The start-up sweep (handoff 003 T7; IS-13): find every creation mark under `layout` whose creating process
/// is gone and settle it. **No record in `store`**: the creation never finished, and everything it made is
/// removed whole (the sealed key record, the repository, the scratch directory, the lock
/// file) and then the mark. **A record**: the wiki is complete (the process died between storing the record and
/// removing the mark) and only the mark is removed; if the record of its first change was not yet kept, that is
/// written first, from the history, and the mark stays when it cannot be. A mark whose creation is still running
/// is left alone.
///
/// **An empty store proves nothing, and a sealed block is not an attempt (R9).** Nothing is removed when
/// `store_is_new` (this start made the store, so a wiki whose record was in a lost one would look unfinished),
/// and nothing is removed from a repository that already holds a sealed block (`storage` is asked: a creation
/// that got that far has made a history, and only a person may decide whether to discard it). Both are
/// reported as left behind, with the mark kept and the reason in words for the operator.
///
/// Run it before anything serves or creates, with the signer that holds the key store, so the key records can be
/// removed. It never removes anything it cannot tie to an unfinished creation: it acts on ids that have a mark.
pub fn sweep_unfinished_creations(
    store: &dyn WikiStore,
    acts: &dyn ActStore,
    layout: &RootLayout,
    signer: &dyn RepositoryBootstrap,
    storage: &dyn Storage,
    store_is_new: bool,
    audit: &dyn SweepAudit,
) -> Result<SweepReport, String> {
    let found = find_unfinished(layout).map_err(|e| e.to_string())?;
    let mut report = SweepReport {
        live: found.live,
        ..SweepReport::default()
    };
    for mark in found.dead {
        let id = mark.id().clone();
        match store.get(&id) {
            Ok(record) => {
                // The wiki is whole once its first change is kept as an act; a creation killed between the
                // record and the act has it written now, from the history, and its mark stays if that fails.
                let now = inklkunn_store::record::now_secs();
                match act::repair(acts, storage, layout, &record, now) {
                    Ok(_) => {
                        let _ = mark.finish();
                        report.completed.push(id);
                    }
                    Err(why) => {
                        audit.after(&id, false);
                        report.left_behind.push((id, why));
                    }
                }
            }
            Err(StoreError::NotFound) => {
                if let Some(why) = refuse_removal(storage, layout, &id, store_is_new) {
                    audit.after(&id, false);
                    report.left_behind.push((id, why));
                    continue;
                }
                // The trail first: an act it cannot take is not done.
                if let Err(why) = audit.before(&id) {
                    audit.after(&id, false);
                    report.left_behind.push((id, why));
                    continue;
                }
                match settle(mark, discard(layout, signer, &id)) {
                    Cleanup::Removed => {
                        audit.after(&id, true);
                        report.removed.push(id);
                    }
                    other => {
                        audit.after(&id, false);
                        report.left_behind.push((id, other.to_string()));
                    }
                }
            }
            Err(e) => {
                // The record store cannot say: removing a wiki that might exist would be worse than waiting.
                audit.after(&id, false);
                report
                    .left_behind
                    .push((id, format!("the record store could not be read ({e})")));
            }
        }
    }
    Ok(report)
}

/// Why the sweep must not remove the unfinished creation `id` (no record in the store), or `None` when it may.
fn refuse_removal(
    storage: &dyn Storage,
    layout: &RootLayout,
    id: &WikiId,
    store_is_new: bool,
) -> Option<String> {
    if store_is_new {
        return Some(
            "this start made the store, so the lack of a record proves nothing; nothing was removed"
                .to_owned(),
        );
    }
    match holds_sealed_block(storage, &layout.repo_dir(id)) {
        Ok(false) => None,
        Ok(true) => Some(
            "its repository holds a sealed block, so it may be a wiki whose record was lost; nothing was removed"
                .to_owned(),
        ),
        Err(e) => Some(format!(
            "its repository could not be examined for a sealed block ({e}); nothing was removed"
        )),
    }
}

/// Does the repository at `repo` hold a sealed block in any view? A directory that is not (yet) a prikk
/// repository holds none. `branch list` naming a view, or the default view's `log` listing a block, says yes.
fn holds_sealed_block(storage: &dyn Storage, repo: &Path) -> Result<bool, StorageError> {
    // A creation killed before `prikk init` left a directory prikk would refuse; that is not a failure to read.
    if !repo.join(".prikk").exists() {
        return Ok(false);
    }
    if !storage.views(repo)?.views.is_empty() {
        return Ok(true);
    }
    let main = Point::View("heads/main".to_owned());
    Ok(!storage.history(repo, &main, Some(1))?.blocks.is_empty())
}

/// The repository directory of a wiki, for callers that hold a record.
pub fn repo_of(layout: &RootLayout, record: &WikiRecord) -> PathBuf {
    layout.repo_dir(&record.id)
}

/// Is `p` the repository directory of `record` under `layout`?
pub fn is_repo_of(layout: &RootLayout, record: &WikiRecord, p: &Path) -> bool {
    layout.repo_dir(&record.id) == p
}
