//! What the acceptance leaves besides its own record (RFC 004 D-4, D-6, D-14; handoff 004 T4): the flag a wiki
//! carries while an acceptance is incomplete, the two texts of an overlap kept on the proposal, and the records
//! of the acts the service makes for itself (an import, a system change), which have no proposal and so no
//! acceptance record. Store schema version 10.
//!
//! **The flag is the one thing here that changes**: it is set when an act ends in a state a person must look
//! at, and cleared by recovery (handoff 004 T5). The overlaps and the system acts are written once, like the
//! acceptance records they sit beside: the SQLite store's triggers refuse an `UPDATE`, a `DELETE` and a
//! replacing `INSERT`.

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::acceptance::AcceptanceOutcome;
use crate::record::{ProposalId, RecoveryActId, SystemActId, WikiId};

/// Why a wiki is flagged *needs recovery* (RFC 004 D-6): which incomplete state the last act left. Every
/// reason is a fixed word; none is prikk's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RecoveryReason {
    /// `commit` succeeded and `seal` did not: one patch is queued (the signer's `Queued`). Recovery seals it.
    Queued,
    /// The change is sealed but could not be confirmed (the signer's `SealedUnconfirmed`). A person looks;
    /// the change is never repeated.
    SealedUnconfirmed,
    /// A failure before `commit` could not be undone: the worktree holds uncommitted changes.
    WorktreeDirty,
    /// An earlier act never ended (its process died holding the lease, or its intent is open): what it did
    /// is not known until recovery reads the witnesses.
    Interrupted,
}

impl RecoveryReason {
    /// Every reason.
    pub const ALL: [RecoveryReason; 4] = [
        RecoveryReason::Queued,
        RecoveryReason::SealedUnconfirmed,
        RecoveryReason::WorktreeDirty,
        RecoveryReason::Interrupted,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            RecoveryReason::Queued => "queued",
            RecoveryReason::SealedUnconfirmed => "sealed-unconfirmed",
            RecoveryReason::WorktreeDirty => "worktree-dirty",
            RecoveryReason::Interrupted => "interrupted",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        RecoveryReason::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// A wiki that *needs recovery*: it refuses writes and still serves reads (RFC 004 D-6, OP-03) until recovery
/// clears the flag. One per wiki; the first reason stays until the flag is cleared, because a second incident
/// does not make the first one less in need of a look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryFlag {
    /// The wiki.
    pub wiki: WikiId,
    /// Why.
    pub reason: RecoveryReason,
    /// The id of the acceptance (or system act) that ended incomplete, as text; `None` when none is known.
    pub act: Option<String>,
    /// Unix seconds.
    pub since: i64,
}

/// One place where the proposal and the tip changed the same thing: both texts, and where (RFC 004 D-4 step
/// 3, D-5: an overlap is never resolved by the service, and a person revises with both in front of them).
/// `place` is a short fixed description (`frontmatter key title`, `block 3`, `page`, `path`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlapText {
    /// The page's path.
    pub page: String,
    /// Where in the page.
    pub place: String,
    /// The text in the base the proposal was written against; `None` when there is none.
    pub base: Option<String>,
    /// The text at the tip; `None` when the tip removed it.
    pub theirs: Option<String>,
    /// The text in the proposal; `None` when the proposal removed it.
    pub ours: Option<String>,
}

/// The overlaps an acceptance found, kept on the proposal (written once; an acceptance that finds them
/// again writes another).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlapRecord {
    /// The proposal.
    pub proposal: ProposalId,
    /// The revision that was being accepted.
    pub revision: u32,
    /// The block of the tip the revision was merged against.
    pub tip_block: String,
    /// Every overlap, in page and place order.
    pub texts: Vec<OverlapText>,
    /// Unix seconds.
    pub created_at: i64,
}

/// What kind of change the service makes for itself (the message's `Kind`, RFC 002 D-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemActKind {
    /// `import-folder` (RFC 003 D-12).
    Import,
    /// Any other service-made change.
    System,
}

impl SystemActKind {
    /// The stored name (the message's `Kind` value).
    pub const fn as_str(self) -> &'static str {
        match self {
            SystemActKind::Import => "import",
            SystemActKind::System => "system",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        [SystemActKind::Import, SystemActKind::System]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

/// The start of an act the service makes for itself: the acceptance record and the intent in one, since there
/// is no proposal to accept. Written before the signer is asked (RFC 004 D-14), so a crash leaves it behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemActRecord {
    /// The id; the signer's acceptance token names it.
    pub id: SystemActId,
    /// The wiki.
    pub wiki: WikiId,
    /// The view.
    pub view: String,
    /// What kind of change.
    pub kind: SystemActKind,
    /// The block the change is written on top of (the tip it was planned against).
    pub base_block: String,
    /// The CT-03 message that will be sealed, exactly.
    pub message: String,
    /// The digest of the message.
    pub message_digest: String,
    /// The digest of the files, as the acceptance token carries it.
    pub digest: String,
    /// The size of the change in bytes.
    pub bytes: u64,
    /// Unix seconds.
    pub created_at: i64,
}

/// The end of a system act, written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemActEnd {
    /// The act.
    pub act: SystemActId,
    /// How it ended: sealed, or failed with a word.
    pub outcome: AcceptanceOutcome,
    /// Unix seconds.
    pub at: i64,
}

/// A system act with its end, if it has one. `end` is `None` while it is **open**: the thing recovery looks
/// for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemActEntry {
    /// The start.
    pub act: SystemActRecord,
    /// The end.
    pub end: Option<SystemActEnd>,
}

/// The longest plan a recovery act keeps, in bytes: a bound against a runaway, generous for any plan (one line
/// per witness and per step).
pub const MAX_RECOVERY_PLAN_BYTES: usize = 256 * 1024;

/// A recovery act's plan, recorded **before** the signer is asked to do any of it (RFC 004 D-7; handoff 004 T5):
/// recovery is recorded like an acceptance. `plan` is the text the service made (the witnesses it read and the
/// steps it will take, in its own fixed vocabulary); the store keeps it whole and does not read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryActRecord {
    /// The id; the signer's recovery operations name it.
    pub id: RecoveryActId,
    /// The wiki.
    pub wiki: WikiId,
    /// The plan, as the service wrote it (at most [`MAX_RECOVERY_PLAN_BYTES`]).
    pub plan: String,
    /// Unix seconds.
    pub created_at: i64,
}

/// How a recovery act ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryEnding {
    /// Every step of the plan was done and the wiki was cleared to take writes again.
    Recovered,
    /// A step was refused or failed, or the witnesses did not allow the plan: the wiki stays flagged and a
    /// person decides. Nothing was guessed.
    Stopped,
}

impl RecoveryEnding {
    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            RecoveryEnding::Recovered => "recovered",
            RecoveryEnding::Stopped => "stopped",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        [RecoveryEnding::Recovered, RecoveryEnding::Stopped]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

/// The end of a recovery act, written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryActEnd {
    /// The act.
    pub act: RecoveryActId,
    /// How it ended.
    pub ending: RecoveryEnding,
    /// Unix seconds.
    pub at: i64,
}

/// A recovery act with its end, if it has one. `end` is `None` while it is **open**: a recovery that was itself
/// interrupted, which the next recovery reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryActEntry {
    /// The start.
    pub act: RecoveryActRecord,
    /// The end.
    pub end: Option<RecoveryActEnd>,
}

/// The flag, the overlaps, the system acts and the recovery acts. Every implementation passes the same invariant
/// tests (`tests/acts.rs`). The errors are the plain facts of [`crate::StoreError`].
pub trait ActStore: Send + Sync {
    /// The wiki's recovery flag, if it is flagged.
    fn recovery_flag(&self, wiki: &WikiId) -> Result<Option<RecoveryFlag>>;

    /// Flag the wiki. `true` when the flag is new; `false` when the wiki was already flagged, in which case
    /// nothing changes and the first reason stays. The wiki must be there
    /// ([`NotFound`](crate::StoreError::NotFound)).
    fn set_recovery_flag(&self, flag: &RecoveryFlag) -> Result<bool>;

    /// Clear the flag (recovery's act); `true` when there was one.
    fn clear_recovery_flag(&self, wiki: &WikiId) -> Result<bool>;

    /// Keep the overlaps an acceptance found, on the proposal. The proposal and the revision must be there
    /// ([`NotFound`](crate::StoreError::NotFound)).
    fn record_overlap(&self, overlap: &OverlapRecord) -> Result<()>;

    /// The overlaps recorded on the proposal, oldest first.
    fn overlaps_of(&self, proposal: &ProposalId) -> Result<Vec<OverlapRecord>>;

    /// Start a system act. The wiki must be there; the id must be new
    /// ([`AlreadyRecorded`](crate::StoreError::AlreadyRecorded) otherwise).
    fn begin_system_act(&self, act: &SystemActRecord) -> Result<()>;

    /// Record how a system act ended. Once: a second end is `AlreadyRecorded`.
    fn finish_system_act(&self, end: &SystemActEnd) -> Result<()>;

    /// The system act `id` names, **only if it has not ended**: the question the signer's guard asks.
    fn open_system_act(&self, id: &SystemActId) -> Result<Option<SystemActRecord>>;

    /// Every system act of the wiki with its end, oldest first.
    fn system_acts(&self, wiki: &WikiId) -> Result<Vec<SystemActEntry>>;

    /// Start a recovery act: its plan, written before the signer is asked to do any of it (RFC 004 D-7). The wiki
    /// must be there; the id must be new ([`AlreadyRecorded`](crate::StoreError::AlreadyRecorded) otherwise); a
    /// plan longer than [`MAX_RECOVERY_PLAN_BYTES`] is refused ([`Backend`](crate::StoreError::Backend), nothing
    /// written).
    fn begin_recovery_act(&self, act: &RecoveryActRecord) -> Result<()>;

    /// Record how a recovery act ended. Once: a second end is `AlreadyRecorded`.
    fn finish_recovery_act(&self, end: &RecoveryActEnd) -> Result<()>;

    /// The recovery act `id` names, **only if it has not ended**: the question the signer's recovery guard asks.
    fn open_recovery_act(&self, id: &RecoveryActId) -> Result<Option<RecoveryActRecord>>;

    /// Every recovery act of the wiki with its end, oldest first.
    fn recovery_acts(&self, wiki: &WikiId) -> Result<Vec<RecoveryActEntry>>;
}
