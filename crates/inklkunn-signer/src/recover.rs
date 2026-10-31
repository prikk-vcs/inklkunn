//! `Recover`: the signer's recovery operations, and the read-only witnesses recovery plans from (RFC 004 D-7; internal
//! design CON-4, WR-8; handoff 004 T5; threat model C-1b, INV-7).
//!
//! **Recovery is bound to a recorded act, as an acceptance is.** [`Recover`] has four operations: `seal` alone (complete
//! a queued patch), `doctor --discard-damaged-commits`, `unlock --lock <path>` for the locks an acceptance leaves, and
//! putting a worktree back to its baseline. None takes a seed, a key directory or a prikk argument. Each is given a
//! [`RecoveryToken`], an id, and nothing else; the signer reads the record the id names itself, through its
//! [`AcceptanceLedger`](crate::AcceptanceLedger), and refuses unless the record is the wiki's, still open, and **names
//! this very operation** ([`RecoveryOp`]). The plan the record holds was written by the service before it asked, so a
//! caller cannot invent what the signer is bound to. Sealing is further bound to the acceptance (or system act) whose
//! change is queued: the queued patch's message, files and base must be the ones that record describes.
//!
//! **Recovery never runs without the wiki's write lease.** Every operation refuses ([`RecoverError::NoLease`]) unless the
//! calling thread has been lent the lease ([`WriteLease::scope`](crate::WriteLease::scope)); none takes the wiki's lock
//! itself. A recovery that could race an acceptance is a second writer.
//!
//! **A lock's liveness is decided by inklkunn's lease record, never by prikk's process check** (ledger PL-8): prikk
//! records only the writer's process id, and inside the sandbox's PID namespace that number means nothing. The
//! lock listing's `liveness` line is read and kept for the record, and nothing here looks at it. What clears a lock is
//! the recorded act, which the service wrote only after it held the lease and found no live holder.
//!
//! [`Witnesses`] is the keyless, read-only side: `doctor` and the lock listing, in prikk's text, read by strict readers
//! ([`text`]) that refuse a shape they do not know.

use std::ffi::OsString;

use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_store::{RootLayout, WikiId};

use crate::accept::AcceptanceToken;
use crate::writes::{SignerError, Writes};

pub mod text;

pub use text::{
    ACCEPTANCE_LOCK_KINDS, Diagnostic, Discarded, DoctorReport, HeldLock, Severity,
    parse_doctor_output, parse_unlock_output,
};

/// The id of a recorded recovery act. Holding one proves nothing by itself: the signer re-reads the record the id
/// names and refuses unless it is the wiki's, open, and names the operation asked for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecoveryToken(String);

impl RecoveryToken {
    /// Name a recovery act.
    pub fn new(record_id: impl Into<String>) -> Self {
        RecoveryToken(record_id.into())
    }

    /// The record id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One thing a recorded recovery act allows the signer to do. The service writes the list into the act's record
/// (its plan) before it asks; the signer does an operation only if it is in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryOp {
    /// Seal the one queued patch that is the change of the acceptance (or system act) `acceptance` names, on `view`.
    SealQueued {
        /// The acceptance or system act, as the token names it.
        acceptance: String,
        /// The view (`heads/<name>`).
        view: String,
    },
    /// `doctor --discard-damaged-commits`: remove an acknowledged commit the active WAL no longer holds soundly.
    DiscardDamaged,
    /// Clear these locks (absolute paths as `unlock` lists them), each of a kind an acceptance leaves.
    ClearLocks {
        /// The lock files.
        paths: Vec<String>,
    },
    /// Put the worktree of `view` back to what the tip says.
    RestoreWorktree {
        /// The view (`heads/<name>`).
        view: String,
    },
}

/// What the ledger says a recorded recovery act is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryFacts {
    /// The wiki the act is for.
    pub wiki: WikiId,
    /// What the act allows.
    pub ops: Vec<RecoveryOp>,
}

impl RecoveryFacts {
    /// Does the act allow `op`? A lock-clearing is allowed when every lock it names is one the plan named.
    pub fn allows(&self, op: &RecoveryOp) -> bool {
        match op {
            RecoveryOp::ClearLocks { paths } => paths.iter().all(|p| {
                self.ops.iter().any(|o| {
                    matches!(o, RecoveryOp::ClearLocks { paths: planned } if planned.contains(p))
                })
            }),
            other => self.ops.contains(other),
        }
    }
}

/// Why a recovery operation did not happen (or did not complete). **Each variant says what state the repository is
/// in**; the text is for operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverError {
    /// The act is not recorded, is not open, is another wiki's, or does not allow this operation (or the
    /// acceptance it is bound to is not pending). Nothing was done.
    NotRecorded,
    /// The wiki's write lease is not held by this thread. Nothing was done.
    NoLease,
    /// The repository does not hold what the record says it should (a queued patch that is another change, a tip that
    /// moved, a lock of a kind an acceptance does not leave, a worktree that will not come clean). Nothing was done,
    /// or what was done is said. The text names the rule, never a path.
    DoesNotMatch(String),
    /// prikk refused, and said why (operator text). Nothing is known to have changed.
    Refused(String),
    /// The call could not run to completion, or its output could not be read.
    Unavailable(String),
    /// A call was stopped by its memory ceiling (threat model C-9a): not a refusal.
    CeilingExceeded {
        /// The prikk verb.
        verb: String,
        /// The ceiling it ran under, in bytes.
        bytes: u64,
    },
    /// The call completed, but the temporary directory of decrypted keys could not be removed.
    SeedsNotRemoved,
}

impl std::fmt::Display for RecoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoverError::NotRecorded => write!(
                f,
                "the recovery act is not recorded for this operation; nothing was done"
            ),
            RecoverError::NoLease => write!(
                f,
                "recovery runs only under the wiki's write lease, which is not held; nothing was done"
            ),
            RecoverError::DoesNotMatch(why) => {
                write!(f, "the repository is not as the record says: {why}")
            }
            RecoverError::Refused(m) => write!(f, "prikk refused: {m}"),
            RecoverError::Unavailable(m) => {
                write!(f, "the operation could not run to completion: {m}")
            }
            RecoverError::CeilingExceeded { verb, bytes } => write!(
                f,
                "`{verb}` was stopped by its memory ceiling ({bytes} bytes); it is not refused and not failed"
            ),
            RecoverError::SeedsNotRemoved => write!(
                f,
                "the call completed but a temporary directory of decrypted keys could not be removed; \
                 remove the directories named `inklkunn-seeds-*` under the seed directory"
            ),
        }
    }
}

impl std::error::Error for RecoverError {}

impl From<SignerError> for RecoverError {
    fn from(e: SignerError) -> Self {
        match e {
            SignerError::Refused(m) => RecoverError::Refused(m),
            SignerError::CeilingExceeded { verb, bytes } => {
                RecoverError::CeilingExceeded { verb, bytes }
            }
            SignerError::SeedsNotRemoved => RecoverError::SeedsNotRemoved,
            other => RecoverError::Unavailable(other.to_string()),
        }
    }
}

/// The signer's recovery operations. Every one needs the wiki's write lease lent to the calling thread and a recorded
/// act that names it (see the module documentation).
pub trait Recover: Send + Sync {
    /// Seal the one queued patch of `wiki`, which must be the change the pending acceptance (or system act)
    /// `acceptance` describes: its message, its base, and its files, to the digest. `commit` is not run again.
    fn seal_queued(
        &self,
        wiki: &WikiId,
        view: &str,
        act: &RecoveryToken,
        acceptance: &AcceptanceToken,
    ) -> Result<(), RecoverError>;

    /// `doctor --discard-damaged-commits`. prikk removes an acknowledged commit that the WAL no longer holds
    /// soundly (saving the bytes first) and refuses over anything else.
    fn discard_damaged(
        &self,
        wiki: &WikiId,
        act: &RecoveryToken,
    ) -> Result<Discarded, RecoverError>;

    /// Clear the locks `locks` names (as `unlock` lists them). Each must still be held, of a kind an acceptance
    /// leaves ([`ACCEPTANCE_LOCK_KINDS`]), and inside the wiki's own `.prikk`. Returns those it cleared; a lock that
    /// is no longer held is not an error and is not listed.
    fn clear_locks(
        &self,
        wiki: &WikiId,
        act: &RecoveryToken,
        locks: &[String],
    ) -> Result<Vec<String>, RecoverError>;

    /// Put the worktree back to what the tip of the view it is on says: declared renames moved back, changed and
    /// missing files written from the tip, files the baseline does not have removed. Refuses while a patch is queued.
    /// Returns how many paths it put back. The history is not touched.
    fn restore_worktree(
        &self,
        wiki: &WikiId,
        view: &str,
        act: &RecoveryToken,
    ) -> Result<usize, RecoverError>;
}

/// The keyless, read-only side of recovery: prikk's `doctor` and its lock listing, which have no JSON form, read by
/// the strict readers of [`text`]. Nothing here writes, signs or takes a lock. Runs confined with the repository
/// read-only.
pub struct Witnesses {
    writes: Writes,
    root: RootLayout,
}

impl std::fmt::Debug for Witnesses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Witnesses(..)")
    }
}

impl Witnesses {
    /// Witnesses over `root`'s wikis, run through `prikk` under `sandbox`.
    pub fn new(prikk: impl Into<OsString>, sandbox: Sandbox, root: RootLayout) -> Self {
        Witnesses {
            writes: Writes::new(prikk, sandbox),
            root,
        }
    }

    /// `prikk doctor`: the diagnostics of the wiki's repository.
    pub fn doctor(&self, wiki: &WikiId) -> Result<DoctorReport, SignerError> {
        self.writes.doctor(&self.root.repo_dir(wiki))
    }

    /// `prikk unlock` with no argument: the locks that are held, with their recorded process ids.
    pub fn locks(&self, wiki: &WikiId) -> Result<Vec<HeldLock>, SignerError> {
        self.writes.locks(&self.root.repo_dir(wiki))
    }
}
