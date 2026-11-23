//! Acceptance under the lease (RFC 004 D-4, D-6, D-14; internal design WR-3 … WR-7, CON-1, CON-3, CON-6; handoff
//! 004 T4): **one atomic act. The change is in the history and the proposal is accepted, or the repository is
//! unchanged and the person is told why.** And when neither can be said, the wiki says so and refuses writes
//! until recovery (T5) has looked.
//!
//! [`ProposalAcceptor::accept`] does D-4's steps in D-4's order:
//!
//! 0. `authorize(_, accept, proposal)` **first**, before the lease is taken and before anything is read.
//! 1. The wiki's [write lease](crate::lease): the queue, the bounded wait, the lock shared with the command
//!    line, the durable holder record. A waiter that waited out is told *busy* with a depth and an estimate.
//! 2. The view's tip is read again through `Storage::views` (never a held answer). If the revision's base is
//!    the tip, the change is the revision's content; if not, it is **re-authored** against the tip by the
//!    content-level merge ([`mod@crate::merge`]).
//! 3. On an **overlap** the lease is released, both texts are recorded on the proposal, the proposal is moved
//!    to *needs a revision* and the proposer and the acceptor are told. Nothing in prikk changed.
//! 4. Else the **acceptance record** (*pending seal*, with the digest of the merged content, which is the
//!    digest the signer recomputes over the files it is handed) and the **intent** (acceptance, wiki, view, the
//!    tip the change is written on, the CT-03 message and its digest) are written, **before** `Accept::accept`
//!    is called. The signer reads both back through [`StoreLedger`]; it takes nothing from this function.
//! 5. The signer's answer is read as typed ([`AcceptError`] says what state the repository is in): sealed, or
//!    nothing happened (the record says *failed*, with a word), or a patch is queued / the worktree is dirty /
//!    the seal could not be confirmed (the record stays *pending seal*, the intent stays open, the wiki is
//!    flagged *needs recovery*, and **nothing is ever committed a second time**).
//! 6. On a seal: the ids are recorded, the proposal is *accepted*, the intent completes, the wiki's caches are
//!    forgotten, the reconciliation runs, the lease is released, and then (off the lease) the notifications
//!    are queued as inbox events and the follow-ups are asked for: the title index of the new tip warmed and
//!    the verification enqueued ([`AfterSeal`]; this crate starts no thread it cannot stop).
//!
//! **A retry finds the record** (CON-6): an acceptance of a revision that sealed answers
//! [`Accepted::AlreadySigned`] and writes nothing; one that is pending seal answers [`AcceptFailure::InProgress`]
//! (or *needs recovery*); a failed one does not stand in the way. Two acceptances of one wiki serialize on the
//! lease, and a retry waiting behind the first finds its record when its turn comes.
//!
//! **One path.** [`SystemActs`] (an import, a system change; D-14) runs the same steps under the system
//! principal with `Kind: import | system`, against `system acts` records instead of an acceptance record.
//!
//! **The words** (D-12) are fixed keys, never prikk's text and never a path: [`words`].

use std::path::PathBuf;
use std::sync::Arc;

use inklkunn_prikk::Storage;
use inklkunn_signer::PreparedFile;
use inklkunn_signer::accept::{
    Accept, AcceptError, AcceptanceToken, PreparedChange, SealedChange, Unconfirmed,
};
use inklkunn_signer::message::{Actor, BlockId, Message, MessageError, Summary};
use inklkunn_store::{
    AcceptanceBegin, AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult,
    AcceptanceStore, AccountId, AccountStore, ActStore, AuditSink, DisplayName, IntentCompletion,
    IntentEnd, IntentId, IntentRecord, OverlapRecord, ProposalId, ProposalRecord, ProposalState,
    RecoveryFlag, RecoveryReason, Role, RoleStore, RootLayout, StoreError, WikiRecord,
};

use crate::authorize::{Principal, RestrictionNotice};
use crate::governance::{Fault as GovernanceFault, ReadFault, Rules};
use crate::knowledge::check::Refusal as KnowledgeRefusal;
use crate::lease::{Busy, BusyWhy, Lease, LeaseError, Purpose, WikiLeases};
use crate::notify::Notifier;
use crate::proposal::{
    Invalid, ProposalError, ProposalService, QuotaLimit, QuotaRefusal, Reset, machine,
};
use crate::read::{ReadService, Tip};
use crate::reconcile::Finding;

mod claims;
mod governance;
mod ledger;
mod prepare;
pub(crate) mod shared;
mod system;
#[cfg(test)]
pub(crate) mod testing;
mod views;

pub use ledger::{NoLedger, StoreActs, StoreLedger, digest_from_hex, message_digest};
pub use system::{SystemActFailure, SystemActs, SystemRequest, SystemSigned, SystemStep};
pub use views::{ViewCreated, ViewFailure};

use prepare::{PrepareError, Prepared};
use shared::{Blocked, TipError};

/// The policy the decision was made under (PR-12) for a wiki **with no governance files**: the bootstrap's fixed role
/// ladder, which is the default policy ([`crate::governance::defaults`]).
pub const POLICY_VERSION: &str = "bootstrap-1";

/// The policy the decision was made under for a wiki whose rules are read from `governance/` (schema version 1 of the two
/// files); the block that set them is the acceptance record's `policy_block`.
pub const GOVERNED_POLICY_VERSION: &str = "governance-1";

/// How many maintainers a wiki may have for one of them to accept their own proposal (RFC 004 D-3: the bootstrap lets a
/// maintainer of a wiki with fewer than three maintainers, so a one-person wiki is not deadlocked): **the default of
/// `acceptance.self: small-wiki`**, written once, in [`crate::governance::defaults`] (RFC 006 makes it data).
pub use crate::governance::defaults::SELF_ACCEPTANCE_BELOW;

/// The fixed words of an acceptance's outcome (RFC 004 D-12). They are keys: the glossary gives each its text
/// in each language, and a failed acceptance's record keeps the key, never prikk's text.
pub mod words {
    /// The change is in the history.
    pub const SIGNED: &str = "signed";
    /// *Stopped: changes overlap* (with both texts).
    pub const OVERLAP: &str = "overlap";
    /// *Not allowed*.
    pub const NOT_ALLOWED: &str = "not-allowed";
    /// *The wiki is busy*.
    pub const BUSY: &str = "wiki-busy";
    /// *The wiki needs recovery*.
    pub const NEEDS_RECOVERY: &str = "needs-recovery";
    /// *Refused: (the named limit)*; the limit is named by the refusal.
    pub const REFUSED_LIMIT: &str = "refused-limit";
    /// prikk or the signer refused the prepared files (nothing was committed).
    pub const REFUSED: &str = "refused";
    /// The repository could not be asked or written now (nothing was committed).
    pub const UNAVAILABLE: &str = "unavailable";
    /// A call was stopped by its memory ceiling before anything was committed.
    pub const MEMORY_LIMIT: &str = "memory-limit";
    /// The act was interrupted (its process died) and recovery found nothing of it in the repository: it is
    /// recorded as failed with this word, and the proposal stays open (handoff 004 T5).
    pub const INTERRUPTED: &str = "interrupted";
    /// The tip already has everything the revision asks for.
    pub const NOTHING_TO_CHANGE: &str = "nothing-to-change";
    /// A knowledge file the change writes, or leaves broken, is refused before the signer is asked (RFC 005 part 2
    /// D-5); the files and rules are in the refusal.
    pub const KNOWLEDGE_INVALID: &str = "knowledge-invalid";
    /// A change to the wiki's keys (a key retirement) was begun and is not finished, so the signer seals nothing new
    /// until the person who runs the wiki finishes it (handoff 004, W1). Nothing was committed; the proposal stays
    /// open.
    pub const RETIREMENT_UNFINISHED: &str = "retirement-unfinished";
    /// A change to `governance/` is not sound (RFC 006 D-4): refused before the signer is asked; the files and rules are in
    /// the refusal. Nothing was committed and the proposal stays open.
    pub const GOVERNANCE_INVALID: &str = "governance-invalid";
    /// The rules of this wiki need a higher rung to accept a change to `governance/` (RFC 006 D-1).
    pub const GOVERNANCE_RUNG: &str = "governance-rung";
    /// The rules of this wiki ask for more approvals than the revision has (RFC 006 D-2, D-4).
    pub const APPROVALS_MISSING: &str = "approvals-missing";
    /// The summary of the proposal's latest revision is hidden by moderation (RFC 006 D-8): it would be written into the change's
    /// message, so the proposer revises it first.
    pub const SUMMARY_HIDDEN: &str = "summary-hidden";
    /// A protection of a page the change touches asks what the acceptance lacks (RFC 006 D-7): a higher rung, or a review.
    pub const PROTECTED: &str = "protected";
    /// A governance file of this wiki cannot be read, so every write refuses until it is repaired (RFC 006 D-1).
    pub const RULES_UNREADABLE: &str = "rules-unreadable";
    /// A change that touches `governance/` and something else is refused: a change to the rules is its own proposal
    /// (RFC 006 D-4; handoff G-T1 correction C-1). It is the word of [`Invalid::GovernanceMixed`](crate::proposal::Invalid).
    pub const GOVERNANCE_MIXED: &str = "governance-mixed";
}

/// What a proposal's change is recorded as: the message's `Kind`. A rollback is a proposal that was created as
/// one (RFC 004 D-8; `ProposalService::rollback_of`); its revision's base is the block it undoes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeKind {
    Proposal,
    Rollback,
}

/// The steps of D-4, named so that a test can stop an acceptance at any of them: the kill test of handoff 004 T5
/// runs the acceptance in a child process and `SIGKILL`s it at each step in turn. Observing is all the hook
/// does; nothing in production sets one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcceptStep {
    /// Step 1 is done: the lease is held (and its holder recorded).
    LeaseTaken,
    /// Step 2 is done: the tip was read and the change authored (or found as it was); no record exists yet.
    Prepared,
    /// The acceptance record (*pending seal*) is written; the intent is not.
    AcceptanceRecorded,
    /// The intent is written and the lease is attached to the acceptance: the signer has not been asked.
    IntentRecorded,
    /// The signer answered (whatever it answered); nothing of the answer is recorded yet.
    SignerReturned,
    /// The acceptance's end (*sealed*, with the block and the patch) is written.
    AcceptanceSealed,
    /// The intent's end (*completed*) is written.
    IntentCompleted,
    /// The proposal is *accepted*.
    ProposalAccepted,
    /// The caches are forgotten and the reconciliation is done; the lease is still held.
    Reconciled,
}

impl AcceptStep {
    /// Every step, in the order an acceptance takes them.
    pub const ALL: [AcceptStep; 9] = [
        AcceptStep::LeaseTaken,
        AcceptStep::Prepared,
        AcceptStep::AcceptanceRecorded,
        AcceptStep::IntentRecorded,
        AcceptStep::SignerReturned,
        AcceptStep::AcceptanceSealed,
        AcceptStep::IntentCompleted,
        AcceptStep::ProposalAccepted,
        AcceptStep::Reconciled,
    ];

    /// The step's name.
    pub const fn as_str(self) -> &'static str {
        match self {
            AcceptStep::LeaseTaken => "lease-taken",
            AcceptStep::Prepared => "prepared",
            AcceptStep::AcceptanceRecorded => "acceptance-recorded",
            AcceptStep::IntentRecorded => "intent-recorded",
            AcceptStep::SignerReturned => "signer-returned",
            AcceptStep::AcceptanceSealed => "acceptance-sealed",
            AcceptStep::IntentCompleted => "intent-completed",
            AcceptStep::ProposalAccepted => "proposal-accepted",
            AcceptStep::Reconciled => "reconciled",
        }
    }

    /// The step a name is of.
    pub fn parse(name: &str) -> Option<Self> {
        AcceptStep::ALL.into_iter().find(|s| s.as_str() == name)
    }
}

/// What the service asks for after a seal, **off the lease**: the caller decides how and where these run (a
/// queue, a task, nothing). This crate starts no thread it cannot stop (handoff 004 T4).
pub trait AfterSeal: Send + Sync {
    /// The wiki's caches were forgotten: warm the title index of the new tip before a reader pays for it (PERF-1,
    /// lever 2). Enqueue; do not do the work here.
    fn warm_title_index(&self, wiki: &WikiRecord);

    /// Verify the wiki's repository (OP-06). Enqueue; `Verifier::check` claims and guards the run.
    fn enqueue_verification(&self, wiki: &WikiRecord);
}

/// Asks for nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoFollowUp;

impl AfterSeal for NoFollowUp {
    fn warm_title_index(&self, _: &WikiRecord) {}
    fn enqueue_verification(&self, _: &WikiRecord) {}
}

/// What the reconciliation (RFC 002 D-7) said after the seal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciled {
    /// The history and the records agree everywhere.
    Clean,
    /// They disagree; the findings are in the audit trail as well. A person looks (INV-1, INV-10).
    Findings(Vec<Finding>),
    /// It could not be done now (the text is for operators). Nothing is concluded.
    NotRun(String),
    /// It was not asked for.
    Skipped,
}

/// A change in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    /// The acceptance record.
    pub acceptance: AcceptanceId,
    /// The sealed block.
    pub block_id: String,
    /// The sealed patch.
    pub patch_id: String,
    /// The view had moved since the revision was made, and the change was re-authored against the tip.
    pub reauthored: bool,
    /// The signer could not confirm the seal afterwards (the change **is** sealed). The wiki is flagged for a
    /// look, and the change is never made again.
    pub unconfirmed: Option<Unconfirmed>,
    /// A temporary directory of decrypted seeds could not be removed; the operator removes it.
    pub seeds_left: bool,
    /// Blocks that got a `^id` because a comment anchored to them (CM-5).
    pub ids_attached: usize,
    /// What the reconciliation found.
    pub reconciliation: Reconciled,
    /// The pages the change wrote, removed or renamed to (paths in the repository).
    pub paths: Vec<String>,
    /// Accounts told that a page they watch changed ([`Notifier::page_changed`]).
    pub notified: usize,
}

/// What a successful call to [`ProposalAcceptor::accept`] says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accepted {
    /// This call put the change in the history.
    Signed(Signed),
    /// An earlier call did (CON-6): nothing was written, and this is its record.
    AlreadySigned {
        /// The acceptance.
        acceptance: AcceptanceId,
        /// Its block.
        block_id: String,
        /// Its patch.
        patch_id: String,
    },
}

/// Why an acceptance did not put its change in the history. Each variant says what state things are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptFailure {
    /// *Not allowed* (also: not the person it is for; also: a proposer of a wiki with three maintainers
    /// accepting their own proposal). Nothing was done.
    NotAllowed,
    /// **The person is restricted** in this wiki, and without the restriction they could have done this (RFC 006 D-7): the
    /// reason and the end, told to them alone. Nothing was done.
    Restricted(RestrictionNotice),
    /// **The proposal replaces, or takes away, a knowledge file the wiki cannot read, and its author no longer holds the rung that
    /// may** (RFC 006 D-10; G-T6 correction S-2): the `moderate` rung is asked of the proposal's author again under the lease, by
    /// the rules of the state the change is signed on. `restricted` says that a restriction is the cause (the person would hold
    /// the rung without it); its reason and its end are the restricted person's alone, so they are not here: when the acceptor is
    /// the author, the failure is [`AcceptFailure::Restricted`] (their own restriction, told to them) or
    /// [`AcceptFailure::NotAllowed`]. Nothing was signed, and the proposal is as it was.
    AuthorNotAllowed {
        /// A restriction is the cause.
        restricted: bool,
    },
    /// There is no such proposal in this wiki, or it is not the principal's to see. Nothing was done.
    NotFound,
    /// The proposal's state or revision does not allow it (the state machine's word). Nothing was done.
    Proposal(ProposalError),
    /// *The wiki is busy*, with how many acts are ahead and about how long. Nothing was done.
    Busy(Busy),
    /// *Refused: (the named limit)*. Nothing was done; the proposal stays open.
    Refused(QuotaRefusal),
    /// *The wiki needs recovery*: it refuses writes until recovery has looked, and reads are served. Nothing was
    /// done by this call.
    NeedsRecovery(RecoveryFlag),
    /// An earlier acceptance of this revision is pending seal. This call did nothing; recovery, or its owner's
    /// end, settles it.
    InProgress {
        /// The acceptance that is.
        acceptance: AcceptanceId,
    },
    /// *Stopped: changes overlap.* Both texts are recorded on the proposal, which now *needs a revision*.
    /// Nothing in prikk changed.
    Overlap {
        /// How many places overlap.
        overlaps: usize,
    },
    /// The tip already has everything the revision asks for. Nothing was done.
    NothingToChange,
    /// A knowledge file (a claim or a source) the change writes is not sound, or the change leaves one broken
    /// (RFC 005 part 2 D-5): each is named with the rule it breaks and never with what it said. Refused **before the
    /// signer was asked**; nothing in prikk changed and the proposal stays open.
    KnowledgeInvalid(Vec<KnowledgeRefusal>),
    /// A change to `governance/` is not sound (RFC 006 D-4): a file of the wrong shape, a key or a value the schema does not
    /// know, the ordering rule broken, a platform limit exceeded, a governance file in a view that is not the default one, or
    /// a path under `governance/` that is neither of the two files. **Each is named with the file, the rule and where it can
    /// be said the line, and never with what the file said**, and refused before the signer was asked: nothing in prikk
    /// changed and the proposal stays open.
    GovernanceInvalid(Vec<GovernanceFault>),
    /// The constitution of this wiki asks a higher rung than the acceptor's for a change to `governance/` (RFC 006 D-1). The
    /// rung is named. Nothing was done.
    GovernanceRung(Role),
    /// The rules of this wiki ask for more approvals, from people other than the proposer, on the proposal's latest revision
    /// (RFC 006 D-2, D-4): `needed` of them, and `have` is how many it has now, counting only people who still hold the
    /// review rung. Nothing was done.
    ApprovalsMissing {
        /// What the rules ask for.
        needed: u8,
        /// What the latest revision has.
        have: usize,
    },
    /// **The summary of the proposal's latest revision is hidden by moderation** (RFC 006 D-8; handoff G-T4 correction H-2): it
    /// would be signed into the history as the change's message, so the proposer revises it first (a revision whose summary is not
    /// hidden makes acceptance possible again). A hidden *author* does not refuse: the message names the proposer for attribution,
    /// and the name is masked wherever the change is shown. Nothing was done.
    SummaryHidden,
    /// **A protection** of a page the change touches asks what this acceptance lacks (RFC 006 D-7; threat model C-3g): a higher
    /// rung of the acceptor, or one approval of the latest revision from someone other than the proposer. Every protection that
    /// applies is carried, each once, with its kind, its reason and its end, so that the refusal can say what stopped the change
    /// and why. Nothing was done.
    Protected {
        /// Every protection in force on the pages the change touches.
        notices: Vec<crate::protection::Notice>,
        /// What the acceptance lacks of what they ask.
        lack: crate::protection::Lack,
    },
    /// A governance file of this wiki cannot be read, so **every write refuses** until it is repaired (RFC 006 D-1, threat
    /// model C-3d), and this change is not the repair: the refusal names the file and the line. Only whoever may read the wiki
    /// is told. Nothing was done.
    RulesUnreadable(ReadFault),
    /// Failed **before `commit`**: nothing in prikk changed, the acceptance is recorded as failed with `word`,
    /// and the proposal stays open.
    Failed {
        /// The acceptance.
        acceptance: AcceptanceId,
        /// The fixed word ([`words`]).
        word: &'static str,
        /// What happened, for operators.
        detail: String,
    },
    /// Failed **after** something may have been committed (or the worktree could not be put back, or the seal
    /// could not be confirmed and its ids are unknown): the acceptance stays pending seal, the intent stays
    /// open, the wiki is flagged *needs recovery* (`flagged` says whether the flag could be recorded), and the
    /// change is never made again by this path.
    Incomplete {
        /// The acceptance.
        acceptance: AcceptanceId,
        /// Why the wiki needs recovery.
        reason: RecoveryReason,
        /// Whether the wiki's flag was recorded.
        flagged: bool,
        /// What happened, for operators.
        detail: String,
    },
    /// The repository or the records could not be asked before anything was written. Nothing was done.
    Unavailable(String),
    /// A store failed (the text is for operators). What was written before is in the records.
    Store(StoreError),
}

impl AcceptFailure {
    /// The fixed word a person acts on ([`words`]). `Refused` is one word; the limit it names is in the
    /// refusal.
    pub fn word(&self) -> &'static str {
        match self {
            AcceptFailure::NotAllowed
            | AcceptFailure::Restricted(_)
            | AcceptFailure::AuthorNotAllowed { .. }
            | AcceptFailure::NotFound => words::NOT_ALLOWED,
            AcceptFailure::Busy(_) => words::BUSY,
            AcceptFailure::Refused(_) => words::REFUSED_LIMIT,
            AcceptFailure::NeedsRecovery(_) | AcceptFailure::Incomplete { .. } => {
                words::NEEDS_RECOVERY
            }
            AcceptFailure::Overlap { .. } => words::OVERLAP,
            AcceptFailure::NothingToChange => words::NOTHING_TO_CHANGE,
            AcceptFailure::KnowledgeInvalid(_) => words::KNOWLEDGE_INVALID,
            AcceptFailure::GovernanceInvalid(_) => words::GOVERNANCE_INVALID,
            AcceptFailure::GovernanceRung(_) => words::GOVERNANCE_RUNG,
            AcceptFailure::ApprovalsMissing { .. } => words::APPROVALS_MISSING,
            AcceptFailure::SummaryHidden => words::SUMMARY_HIDDEN,
            AcceptFailure::Protected { .. } => words::PROTECTED,
            AcceptFailure::RulesUnreadable(_) => words::RULES_UNREADABLE,
            AcceptFailure::Failed { word, .. } => word,
            AcceptFailure::Proposal(ProposalError::Invalid(Invalid::GovernanceMixed)) => {
                words::GOVERNANCE_MIXED
            }
            AcceptFailure::Proposal(_)
            | AcceptFailure::InProgress { .. }
            | AcceptFailure::Unavailable(_)
            | AcceptFailure::Store(_) => words::UNAVAILABLE,
        }
    }
}

impl std::fmt::Display for AcceptFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcceptFailure::NotAllowed => write!(f, "not allowed"),
            AcceptFailure::Restricted(notice) => {
                write!(f, "restricted until {}: {}", notice.until, notice.reason)
            }
            AcceptFailure::AuthorNotAllowed { restricted } => write!(
                f,
                "the proposal's author may no longer replace a file that cannot be read ({})",
                if *restricted {
                    "restricted"
                } else {
                    "not allowed"
                }
            ),
            AcceptFailure::NotFound => write!(f, "no such proposal"),
            AcceptFailure::Proposal(e) => write!(f, "{e}"),
            AcceptFailure::Busy(b) => write!(
                f,
                "the wiki is busy ({} ahead, about {} s)",
                b.depth,
                b.estimate.as_secs()
            ),
            AcceptFailure::Refused(q) => write!(f, "refused: {}", q.limit.name()),
            AcceptFailure::NeedsRecovery(flag) => {
                write!(f, "the wiki needs recovery ({})", flag.reason.as_str())
            }
            AcceptFailure::InProgress { acceptance } => {
                write!(
                    f,
                    "acceptance {acceptance} of this revision is pending seal"
                )
            }
            AcceptFailure::Overlap { overlaps } => {
                write!(f, "stopped: changes overlap ({overlaps} place(s))")
            }
            AcceptFailure::NothingToChange => write!(f, "the tip already has these changes"),
            AcceptFailure::KnowledgeInvalid(refusals) => {
                write!(
                    f,
                    "refused: {} knowledge file(s) are not sound",
                    refusals.len()
                )?;
                for r in refusals {
                    write!(f, "; {r}")?;
                }
                Ok(())
            }
            AcceptFailure::GovernanceInvalid(faults) => {
                write!(
                    f,
                    "refused: {} governance file(s) are not sound",
                    faults.len()
                )?;
                for fault in faults {
                    write!(f, "; {fault}")?;
                }
                Ok(())
            }
            AcceptFailure::GovernanceRung(rung) => write!(
                f,
                "refused: a change to the rules of this wiki needs the {} rung",
                rung.as_str()
            ),
            AcceptFailure::ApprovalsMissing { needed, have } => write!(
                f,
                "refused: the rules of this wiki ask for {needed} approval(s) of the latest revision, and it has {have}"
            ),
            AcceptFailure::SummaryHidden => f.write_str(
                "refused: the summary of the proposal's latest revision is hidden by moderation, so it would be written into the change; the proposer revises it first",
            ),
            AcceptFailure::Protected { notices, lack } => write!(
                f,
                "refused: {} protection(s) of the pages this change touches ask for {}",
                notices.len(),
                match lack {
                    crate::protection::Lack::Rung(rung) => format!("the {} rung", rung.as_str()),
                    crate::protection::Lack::Review => "a review".to_owned(),
                }
            ),
            AcceptFailure::RulesUnreadable(fault) => {
                write!(f, "refused: the rules of this wiki cannot be read: {fault}")
            }
            AcceptFailure::Failed {
                word,
                detail,
                acceptance,
            } => write!(f, "acceptance {acceptance} failed ({word}): {detail}"),
            AcceptFailure::Incomplete {
                acceptance,
                reason,
                detail,
                ..
            } => write!(
                f,
                "acceptance {acceptance} is incomplete ({}); the wiki needs recovery: {detail}",
                reason.as_str()
            ),
            AcceptFailure::Unavailable(why) => write!(f, "could not be done now: {why}"),
            AcceptFailure::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AcceptFailure {}

impl From<StoreError> for AcceptFailure {
    fn from(e: StoreError) -> Self {
        AcceptFailure::Store(e)
    }
}

impl From<ProposalError> for AcceptFailure {
    fn from(e: ProposalError) -> Self {
        match e {
            ProposalError::NotAllowed => AcceptFailure::NotAllowed,
            ProposalError::Restricted(notice) => AcceptFailure::Restricted(notice),
            ProposalError::RulesUnreadable(fault) => AcceptFailure::RulesUnreadable(fault),
            ProposalError::NotFound => AcceptFailure::NotFound,
            ProposalError::Quota(q) => AcceptFailure::Refused(q),
            ProposalError::Store(e) => AcceptFailure::Store(e),
            other => AcceptFailure::Proposal(other),
        }
    }
}

/// The signer's answer, read as typed (RFC 002 D-4's contract; handoff 004 T4's binding note b).
#[derive(Debug)]
pub(crate) enum Settled {
    /// The change is in the history.
    Sealed {
        block_id: String,
        patch_id: String,
        unconfirmed: Option<Unconfirmed>,
        seeds_left: bool,
    },
    /// Nothing was committed; the repository is as it was.
    Failed { word: &'static str, detail: String },
    /// Something may have been committed, or the worktree is not as it was, or the seal's ids are unknown: the
    /// wiki needs recovery and nothing is done again.
    Incomplete {
        reason: RecoveryReason,
        detail: String,
    },
}

/// Read what [`Accept::accept`] answered.
pub(crate) fn settle(result: Result<SealedChange, AcceptError>) -> Settled {
    let sealed = |s: SealedChange, unconfirmed, seeds_left| Settled::Sealed {
        patch_id: s.patch_ids.first().cloned().unwrap_or_default(),
        block_id: s.block_id,
        unconfirmed,
        seeds_left,
    };
    match result {
        Ok(s) => sealed(s, None, false),
        Err(AcceptError::SeedsNotRemoved { sealed: Some(s) }) => sealed(s, None, true),
        Err(AcceptError::SeedsNotRemoved { sealed: None }) => Settled::Failed {
            word: words::UNAVAILABLE,
            detail: "the seeds' directory was not removed and nothing was sealed".to_owned(),
        },
        Err(AcceptError::SealedUnconfirmed {
            sealed: Some(s),
            why,
            ..
        }) => sealed(s, Some(why), false),
        Err(e @ AcceptError::SealedUnconfirmed { sealed: None, .. }) => Settled::Incomplete {
            reason: RecoveryReason::SealedUnconfirmed,
            detail: e.to_string(),
        },
        Err(e @ AcceptError::Queued { .. }) => Settled::Incomplete {
            reason: RecoveryReason::Queued,
            detail: e.to_string(),
        },
        Err(e @ AcceptError::WorktreeDirty(_)) => Settled::Incomplete {
            reason: RecoveryReason::WorktreeDirty,
            detail: e.to_string(),
        },
        // The repository was not clean when the signer began (P8): the wiki is flagged, nothing is guessed.
        Err(e @ AcceptError::Unclean { queued, .. }) => Settled::Incomplete {
            reason: if queued {
                RecoveryReason::Queued
            } else {
                RecoveryReason::WorktreeDirty
            },
            detail: e.to_string(),
        },
        Err(e @ AcceptError::Busy { .. }) => Settled::Failed {
            word: words::BUSY,
            detail: e.to_string(),
        },
        Err(e @ AcceptError::RetirementUnfinished(_)) => Settled::Failed {
            word: words::RETIREMENT_UNFINISHED,
            detail: e.to_string(),
        },
        Err(e @ AcceptError::CeilingExceeded { .. }) => Settled::Failed {
            word: words::MEMORY_LIMIT,
            detail: e.to_string(),
        },
        Err(e @ AcceptError::Unavailable(_)) => Settled::Failed {
            word: words::UNAVAILABLE,
            detail: e.to_string(),
        },
        Err(
            e @ (AcceptError::Refused(_)
            | AcceptError::NotPending
            | AcceptError::MessageMismatch
            | AcceptError::ChangeMismatch),
        ) => Settled::Failed {
            word: words::REFUSED,
            detail: e.to_string(),
        },
    }
}

/// Everything the acceptance is made of.
#[derive(Clone)]
pub struct AcceptanceParts {
    /// The proposal service: authorization, the records of the proposals, the state machine.
    pub proposals: Arc<ProposalService>,
    /// The acceptance, intent and holder records.
    pub acceptances: Arc<dyn AcceptanceStore>,
    /// The flag, the overlaps and the system acts.
    pub acts: Arc<dyn ActStore>,
    /// Roles (the acceptor's role is recorded with the acceptance).
    pub roles: Arc<dyn RoleStore>,
    /// Accounts (the names a message carries).
    pub accounts: Arc<dyn AccountStore>,
    /// prikk, read-only: the tip, the sides of a merge, the history the reconciliation reads.
    pub storage: Arc<dyn Storage>,
    /// Where the wikis are.
    pub layout: RootLayout,
    /// The signer.
    pub signer: Arc<dyn Accept>,
    /// The write lease. Its [`WikiLeases::signer_leases`] must be the ones `signer` was opened over
    /// (`Signer::with_leases`).
    pub leases: Arc<WikiLeases>,
    /// The read service, whose caches are forgotten after a seal.
    pub reads: Arc<ReadService>,
}

/// The acceptance of proposals.
pub struct ProposalAcceptor {
    parts: AcceptanceParts,
    follow_up: Arc<dyn AfterSeal>,
    notifier: Option<Arc<Notifier>>,
    audit: Option<Arc<dyn AuditSink>>,
    reconcile: bool,
    clock: fn() -> i64,
    observer: Option<Arc<dyn Fn(AcceptStep) + Send + Sync>>,
}

impl std::fmt::Debug for ProposalAcceptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProposalAcceptor")
    }
}

impl ProposalAcceptor {
    /// An acceptor over these parts. Nothing follows a seal until [`ProposalAcceptor::with_follow_up`]; the
    /// reconciliation runs after every seal (and reports to the audit trail once [`ProposalAcceptor::with_audit`]
    /// gives it one).
    pub fn new(parts: AcceptanceParts) -> Self {
        ProposalAcceptor {
            parts,
            follow_up: Arc::new(NoFollowUp),
            notifier: None,
            audit: None,
            reconcile: true,
            clock: inklkunn_store::record::now_secs,
            observer: None,
        }
    }

    /// Call `observer` as each step of D-4 is done (see [`AcceptStep`]). For tests that stop an acceptance at a
    /// step; the observer runs on the acceptance's thread and may never return.
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn Fn(AcceptStep) + Send + Sync>) -> Self {
        self.observer = Some(observer);
        self
    }

    fn observe(&self, step: AcceptStep) {
        if let Some(observer) = &self.observer {
            observer(step);
        }
    }

    /// Ask `follow_up` for the title index and the verification after a seal.
    #[must_use]
    pub fn with_follow_up(mut self, follow_up: Arc<dyn AfterSeal>) -> Self {
        self.follow_up = follow_up;
        self
    }

    /// Tell the people involved through `notifier` (RFC 004 D-9): the proposer when the proposal is accepted or
    /// sent back for an overlap, and the accounts that watch the pages the change wrote. Telling is best effort
    /// and never changes the act.
    #[must_use]
    pub fn with_notifier(mut self, notifier: Arc<Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// Append the reconciliation's findings to this audit trail.
    #[must_use]
    pub fn with_audit(mut self, audit: Arc<dyn AuditSink>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Run the reconciliation after a seal (the default) or not.
    #[must_use]
    pub fn with_reconciliation(mut self, on: bool) -> Self {
        self.reconcile = on;
        self
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    // --- the steps

    /// Accept revision `revision` of the proposal, as `principal`. Blocks the calling thread (the lease waits
    /// its turn; prikk is asked); an async caller runs it on a blocking thread.
    pub fn accept(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        revision: u32,
    ) -> Result<Accepted, AcceptFailure> {
        // 0. Authorization, first: before the lease, before any record is read.
        let acceptor = self.parts.proposals.authorize_accept(principal, wiki, id)?;
        let proposal = self.parts.proposals.get_raw(principal, wiki, id)?;
        // A retry of an acceptance that finished answers without taking the lease.
        if let Some(done) = self.sealed_before(wiki, &proposal, revision)? {
            return Ok(done);
        }
        self.check_revision(principal, wiki, &proposal, revision)?;
        self.check_summary_shown(principal, wiki, id)?;
        // The rung, the approvals and the proposer's own acceptance, judged by the rules of the wiki, from what the revision
        // writes (an early refusal; the one that counts is made again under the lease, from what is about to be signed).
        let touched = self.revision_paths(principal, wiki, id, revision)?;
        governance::refuse_mixed(&touched)?;
        // The rules as the service holds them (the tip is held for a few seconds): advisory. The lease's judge reads them anew.
        let held = self.parts.proposals.rules(wiki)?;
        self.judge(principal, wiki, &proposal, &acceptor, &touched, &held)?;

        // 1. The lease.
        let lease = self
            .parts
            .leases
            .acquire(&wiki.id, Purpose::Acceptance, None)
            .map_err(|e| self.lease_failure(e))?;
        self.observe(AcceptStep::LeaseTaken);
        match self.under_lease(principal, wiki, id, revision, &acceptor, &lease)? {
            Step::Done(signed) => {
                let notified = self.after_lease(wiki, &proposal, &acceptor, lease, &signed);
                Ok(Accepted::Signed(Signed { notified, ..signed }))
            }
            Step::Stopped(failure) => {
                drop(lease);
                if let (AcceptFailure::Overlap { .. }, Some(notifier)) = (&failure, &self.notifier)
                {
                    notifier.needs_revision(wiki, &proposal, &acceptor);
                }
                Err(failure)
            }
            Step::AlreadySigned(done) => Ok(done),
        }
    }

    fn lease_failure(&self, e: LeaseError) -> AcceptFailure {
        match e {
            // The queue is full: refused by name, with the limit and how many wait (RFC 004 D-11, D-12).
            LeaseError::Busy(Busy {
                why: BusyWhy::QueueFull { bound, waiting },
                ..
            }) => AcceptFailure::Refused(QuotaRefusal {
                limit: QuotaLimit::AcceptanceQueue,
                allowed: bound,
                current: waiting,
                asked: 1,
                reset: Reset::WhenTheQueueMoves,
            }),
            LeaseError::Busy(busy) => AcceptFailure::Busy(busy),
            LeaseError::Unavailable(why) => AcceptFailure::Unavailable(why),
        }
    }

    /// An acceptance of the revision that sealed, if there is one: the answer of a retry. **A proposal that is
    /// still *held for review* beside a sealed acceptance is marked accepted here** (handoff 004, P8): the seal
    /// is in the history and its record says so, and the writes that move the proposal on (the end of the
    /// intent, the proposal's state) can both fail after it; a retry is how the proposal is finally settled.
    fn sealed_before(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        revision: u32,
    ) -> Result<Option<Accepted>, AcceptFailure> {
        let entries = self
            .parts
            .acceptances
            .acceptances_of(&proposal.id, revision)?;
        for e in entries {
            let acceptor = e.acceptance.acceptor.clone();
            if let Some(AcceptanceOutcome::Sealed { block_id, patch_id }) =
                e.result.map(|r| r.outcome)
            {
                self.settle_signed_proposal(wiki, proposal, &acceptor)?;
                return Ok(Some(Accepted::AlreadySigned {
                    acceptance: e.acceptance.id,
                    block_id,
                    patch_id,
                }));
            }
        }
        Ok(None)
    }

    /// A proposal whose acceptance sealed is *accepted*; one that is still *held for review* is moved there, by the
    /// acceptor of the sealed acceptance.
    fn settle_signed_proposal(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        acceptor: &AccountId,
    ) -> Result<(), AcceptFailure> {
        if proposal.state == ProposalState::HeldForReview {
            self.parts
                .proposals
                .record_accepted(wiki, &proposal.id, acceptor)?;
        }
        Ok(())
    }

    /// The revision is the latest and the proposal may be accepted in the state it is in.
    fn check_revision(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        revision: u32,
    ) -> Result<(), AcceptFailure> {
        machine::apply(Some(proposal.state), inklkunn_store::ProposalOp::Accept)
            .map_err(ProposalError::from)?;
        let latest = self
            .parts
            .proposals
            .revisions_raw(principal, wiki, &proposal.id)?
            .last()
            .map(|r| r.seq)
            .ok_or(AcceptFailure::NotFound)?;
        if revision != latest {
            return Err(AcceptFailure::Proposal(ProposalError::StaleRevision {
                latest,
            }));
        }
        Ok(())
    }

    /// The paths the revision writes, removes or renames to or from, as the acceptance judges who may accept it before it has
    /// read the tip.
    fn revision_paths(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        revision: u32,
    ) -> Result<Vec<String>, AcceptFailure> {
        let content = self
            .parts
            .proposals
            .revisions_raw(principal, wiki, id)?
            .into_iter()
            .find(|r| r.seq == revision)
            .ok_or(AcceptFailure::NotFound)?
            .content;
        Ok(content
            .files
            .iter()
            .map(|f| f.path.clone())
            .chain(
                content
                    .renames
                    .iter()
                    .flat_map(|r| [r.from.clone(), r.to.clone()]),
            )
            .collect())
    }

    /// Set the wiki's flag; `true` when it was recorded (now or before).
    fn flag(&self, wiki: &WikiRecord, reason: RecoveryReason, act: Option<String>) -> bool {
        shared::flag(&*self.parts.acts, wiki, reason, act, (self.clock)())
    }

    /// The wiki must be free of an unfinished act (see [`shared::recovery_guard`]).
    fn recovery_guard(&self, wiki: &WikiRecord, lease: &Lease) -> Result<(), AcceptFailure> {
        shared::recovery_guard(
            &*self.parts.acts,
            &*self.parts.acceptances,
            wiki,
            lease,
            (self.clock)(),
        )
        .map_err(|b| match b {
            Blocked::Flagged(flag) => AcceptFailure::NeedsRecovery(flag),
            Blocked::Unavailable(why) => AcceptFailure::Unavailable(why),
            Blocked::Store(e) => AcceptFailure::Store(e),
        })
    }

    /// Judge the knowledge files the change writes, and those it leaves behind (RFC 005 part 2 D-5).
    fn check_knowledge(
        &self,
        wiki: &WikiRecord,
        repo: &std::path::Path,
        tip: &str,
        files: &[PreparedFile],
        renames: &[(PathBuf, PathBuf)],
        who: (
            Option<crate::knowledge::check::Stating<'_>>,
            &ProposalRecord,
            &AccountId,
            &Arc<Rules>,
        ),
    ) -> Result<(), AcceptFailure> {
        let (curation, proposal, acceptor, rules) = who;
        let checked = crate::knowledge::check::check_change_report(
            &self.parts.reads,
            &*self.parts.storage,
            repo,
            wiki,
            tip,
            &crate::knowledge::check::Written {
                files,
                renames,
                curation,
                by_service: false,
            },
        )
        .map_err(|e| AcceptFailure::Unavailable(e.to_string()))?;
        if !checked.refusals.is_empty() {
            return Err(AcceptFailure::KnowledgeInvalid(checked.refusals));
        }
        // **Replacing a file the wiki cannot read is the `moderate` rung's** (RFC 006 D-10), and the proposal was judged for it when it
        // was sent: what has changed since (a restriction, a demotion, a rule that asks more) is asked again here, under the lease,
        // of the proposal's author and by the rules of the state the change is signed on (G-T6 correction S-2, decision 17). A
        // refusal signs nothing and leaves the proposal as it was.
        if checked.replaces_unreadable
            && let Err(restriction) =
                self.parts
                    .proposals
                    .author_may_moderate_under(&proposal.author, wiki, rules)
        {
            // The reason and the end of a restriction are the restricted person's alone: told to the author when they are the one
            // accepting, and said to anyone else only as a fact.
            return Err(if proposal.author == *acceptor {
                restriction.map_or(AcceptFailure::NotAllowed, AcceptFailure::Restricted)
            } else {
                AcceptFailure::AuthorNotAllowed {
                    restricted: restriction.is_some(),
                }
            });
        }
        Ok(())
    }

    /// The view's tip block, read now, with the state it belongs to.
    fn tip_block(&self, wiki: &WikiRecord, view: &str) -> Result<(String, String), AcceptFailure> {
        let repo = self.parts.layout.repo_dir(&wiki.id);
        shared::tip_pair(&*self.parts.storage, &repo, view).map_err(|e| match e {
            TipError::NoSuchView => {
                AcceptFailure::Proposal(ProposalError::NoSuchView(view.to_owned()))
            }
            TipError::Unavailable(why) => AcceptFailure::Unavailable(why),
        })
    }

    /// The rules in force for the wiki **now**, with the lease held (C-2): those of the default view's state as the repository has
    /// it, read for this act and never taken from the tip the service holds for a few seconds (which a seal by another
    /// process, or a state put back after a forgotten wiki, leaves behind). `read` is the tip of the proposal's view, just read
    /// as `(block, state)`: when that is the default view it is the state itself; otherwise the default view's is read here.
    fn rules_now(
        &self,
        wiki: &WikiRecord,
        proposal_view: &str,
        read: (&str, &str),
    ) -> Result<Arc<Rules>, AcceptFailure> {
        let (block_id, state_id) = if proposal_view == wiki.default_view {
            (read.0.to_owned(), read.1.to_owned())
        } else {
            self.tip_block(wiki, &wiki.default_view)?
        };
        let tip = Tip {
            view: wiki.default_view.clone(),
            state_id,
            block_id,
        };
        self.parts
            .reads
            .rules_at(wiki, &tip)
            .map_err(|e| AcceptFailure::Unavailable(e.to_string()))
    }

    fn actor(&self, id: &AccountId) -> Result<Actor, AcceptFailure> {
        let account = self
            .parts
            .accounts
            .account(id)?
            .ok_or(StoreError::AccountNotFound)?;
        Ok(Actor::Account {
            id: id.clone(),
            name: DisplayName::new(account.display_name.as_str()).map_err(|_| {
                AcceptFailure::Unavailable("an account's name is not usable".to_owned())
            })?,
        })
    }

    /// Refuse while the summary of the latest revision is hidden: the message of the change is built from the record's own
    /// summary (RFC 006 D-8; H-2), so a hidden summary would be signed into the history.
    fn check_summary_shown(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<(), AcceptFailure> {
        if self
            .parts
            .proposals
            .latest_summary_hidden(principal, wiki, id)?
        {
            return Err(AcceptFailure::SummaryHidden);
        }
        Ok(())
    }

    fn message(
        &self,
        proposal: &ProposalRecord,
        proposer: Actor,
        acceptor: Actor,
        base: &str,
        kind: ChangeKind,
    ) -> Result<String, AcceptFailure> {
        let described = |e: MessageError| AcceptFailure::Unavailable(format!("the message: {e}"));
        let summary = Summary::from_text(&proposal.summary).map_err(described)?;
        let base = BlockId::new(base).map_err(described)?;
        let message = match kind {
            ChangeKind::Proposal => {
                Message::proposal(summary, proposal.id.clone(), proposer, acceptor, base)
            }
            ChangeKind::Rollback => {
                Message::rollback(summary, proposal.id.clone(), proposer, acceptor, base)
            }
        }
        .map_err(described)?;
        Ok(message.to_text())
    }

    /// Steps 2 to 6, with the lease held.
    fn under_lease(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        revision: u32,
        acceptor: &AccountId,
        lease: &Lease,
    ) -> Result<Step, AcceptFailure> {
        self.recovery_guard(wiki, lease)?;
        // What the lease waited for may have settled it: a retry behind the first finds its record.
        let proposal = self.parts.proposals.get_raw(principal, wiki, id)?;
        if let Some(done) = self.sealed_before(wiki, &proposal, revision)? {
            return Ok(Step::AlreadySigned(done));
        }
        if let Some(pending) = self
            .parts
            .acceptances
            .acceptances_of(id, revision)?
            .into_iter()
            .find(|e| e.result.is_none())
        {
            return Err(AcceptFailure::InProgress {
                acceptance: pending.acceptance.id,
            });
        }
        self.check_revision(principal, wiki, &proposal, revision)?;
        // Hidden words are never written into a new block: asked again, now that the lease is held (a hide made while it waited).
        self.check_summary_shown(principal, wiki, id)?;
        let content = self
            .parts
            .proposals
            .revisions_raw(principal, wiki, id)?
            .into_iter()
            .find(|r| r.seq == revision)
            .ok_or(AcceptFailure::NotFound)?
            .content;

        // 2. The tip, read now; the change, as it is or re-authored.
        let repo = self.parts.layout.repo_dir(&wiki.id);
        let (tip, tip_state) = self.tip_block(wiki, &proposal.view)?;
        let prepared =
            prepare::prepare(&*self.parts.storage, &repo, &content, &tip).map_err(|e| match e {
                PrepareError::Storage(e) => AcceptFailure::Unavailable(e.to_string()),
                PrepareError::Unreadable => AcceptFailure::Unavailable(
                    "a page at the base or the tip is not text the merge can read".to_owned(),
                ),
            })?;
        let (mut files, renames, reauthored) = match prepared {
            // 3. An overlap: both texts on the proposal; the repository is untouched.
            Prepared::Overlaps(texts) => {
                let count = texts.len();
                self.parts.acts.record_overlap(&OverlapRecord {
                    proposal: id.clone(),
                    revision,
                    tip_block: tip,
                    texts,
                    created_at: (self.clock)(),
                })?;
                self.parts
                    .proposals
                    .record_needs_revision(wiki, id, acceptor, words::OVERLAP)?;
                return Ok(Step::Stopped(AcceptFailure::Overlap { overlaps: count }));
            }
            Prepared::NothingToChange => return Err(AcceptFailure::NothingToChange),
            Prepared::Change {
                files,
                renames,
                reauthored,
            } => (files, renames, reauthored),
        };
        // RFC 005 part 2 D-3, D-5: a curation proposal names itself as the decision of the standing and the review
        // it sets, and the knowledge files are judged before the signer is asked.
        let curation = crate::knowledge::curation::is_curation(&content);
        if curation {
            crate::knowledge::curation::stamp(
                &*self.parts.storage,
                &repo,
                &tip,
                &mut files,
                id.as_str(),
            )
            .map_err(|e| AcceptFailure::Unavailable(e.to_string()))?;
        }
        // A curation proposal states its checks by naming itself (K77), and holds them to the day it is now.
        let stating = curation.then(|| crate::knowledge::check::Stating {
            proposal: id.as_str(),
            now: (self.clock)(),
        });
        // RFC 006 D-4: the governance files the change writes are judged before the signer is asked (and before the knowledge
        // files, which a change to `governance/` has none of); then who may accept what is about to be signed, by the rules in
        // force *now*, which the lease has kept from changing under it.
        self.check_governance(wiki, &proposal, &files, &renames)?;
        // The rules are those of the default view's state read just now, with the lease held: the state the change is signed on
        // when it is the default view's, and never the tip the service holds (C-2).
        let rules = self.rules_now(wiki, &proposal.view, (&tip, &tip_state))?;
        let judged = self.judge(
            principal,
            wiki,
            &proposal,
            acceptor,
            &governance::touched_paths(&files, &renames),
            &rules,
        )?;
        self.check_knowledge(
            wiki,
            &repo,
            &tip,
            &files,
            &renames,
            (stating, &proposal, acceptor, &rules),
        )?;
        self.observe(AcceptStep::Prepared);
        // CM-5: the blocks the proposal's comments are about carry their ids into the history.
        let comments = self.parts.proposals.comments(principal, wiki, id)?;
        let ids_attached = prepare::attach_ids(&mut files, &comments);

        // RFC 005 part 1 D-1 d (K3 b): what the change does to the claims it touches, and which claims of an earlier
        // change a reconfirm curation judges again. Read now, written with the acceptance record.
        let outcomes = self.claim_outcomes(wiki, &proposal.view, &tip, &files)?;
        let reconfirmed =
            self.reconfirmed(principal, wiki, (&tip, &tip_state), id, curation, &files)?;

        // The accepted-bytes quota, on what is about to be written.
        let bytes = prepare::change_bytes(&files, &renames);
        self.parts
            .proposals
            .quotas()
            .check_accepted_bytes(&wiki.id, &proposal.author, bytes)?;

        // 4. The records, before the signer is asked.
        let kind = match self.parts.proposals.rollback_of(principal, wiki, id)? {
            Some(_) => ChangeKind::Rollback,
            None => ChangeKind::Proposal,
        };
        let text = self.message(
            &proposal,
            self.actor(&proposal.author)?,
            self.actor(acceptor)?,
            &content.base.block_id,
            kind,
        )?;
        let paths: Vec<String> = files
            .iter()
            .map(|f| f.path.to_string_lossy().into_owned())
            .chain(
                renames
                    .iter()
                    .flat_map(|(a, b)| [a, b])
                    .map(|p| p.to_string_lossy().into_owned()),
            )
            .collect();
        let change = PreparedChange {
            files,
            renames,
            message: text.clone(),
        };
        let record = AcceptanceRecord {
            id: AcceptanceId::new(),
            proposal: id.clone(),
            revision,
            wiki: wiki.id.clone(),
            view: proposal.view.clone(),
            acceptor: acceptor.clone(),
            acceptor_role: judged.rung,
            policy_version: if judged.policy_block.is_some() {
                GOVERNED_POLICY_VERSION
            } else {
                POLICY_VERSION
            }
            .to_owned(),
            policy_block: judged.policy_block.clone(),
            hatch: judged.hatch,
            proposer: proposal.author.clone(),
            digest: change.digest().to_hex(),
            bytes,
            created_at: (self.clock)(),
        };
        match self.parts.acceptances.begin_acceptance(&record)? {
            AcceptanceBegin::Started => self.observe(AcceptStep::AcceptanceRecorded),
            AcceptanceBegin::Existing(found) => {
                return match found.result.map(|r| r.outcome) {
                    Some(AcceptanceOutcome::Sealed { block_id, patch_id }) => {
                        self.settle_signed_proposal(wiki, &proposal, &found.acceptance.acceptor)?;
                        Ok(Step::AlreadySigned(Accepted::AlreadySigned {
                            acceptance: found.acceptance.id,
                            block_id,
                            patch_id,
                        }))
                    }
                    _ => Err(AcceptFailure::InProgress {
                        acceptance: found.acceptance.id,
                    }),
                };
            }
        }
        // The review record, between the acceptance record and the intent: a record that cannot be kept stops the
        // acceptance here, as failed, with nothing committed; the retry writes its own.
        if let Err(e) = self.record_reviews(&record.id, &outcomes, reconfirmed.as_ref()) {
            self.fail_record(&record, None, words::UNAVAILABLE);
            return Err(AcceptFailure::Failed {
                acceptance: record.id,
                word: words::UNAVAILABLE,
                detail: e.to_string(),
            });
        }
        let intent = IntentRecord {
            id: IntentId::new(),
            acceptance: record.id.clone(),
            wiki: wiki.id.clone(),
            view: proposal.view.clone(),
            base_block: tip,
            message_digest: message_digest(&text),
            message: text,
            created_at: (self.clock)(),
        };
        if let Err(e) = lease
            .attach(&record.id)
            .and_then(|()| self.parts.acceptances.record_intent(&intent))
        {
            // Nothing was committed and no intent is open: the acceptance ends here.
            self.fail_record(&record, None, words::UNAVAILABLE);
            return Err(AcceptFailure::Failed {
                acceptance: record.id,
                word: words::UNAVAILABLE,
                detail: e.to_string(),
            });
        }
        self.observe(AcceptStep::IntentRecorded);

        // 5. The signer, on this thread, inside the lease.
        let token = AcceptanceToken::new(record.id.as_str());
        let answer = {
            let _scope = lease.scope();
            self.parts
                .signer
                .accept(&wiki.id, &proposal.view, &token, &change)
        };
        self.observe(AcceptStep::SignerReturned);
        match settle(answer) {
            Settled::Failed { word, detail } => {
                self.fail_record(&record, Some(&intent), word);
                Err(AcceptFailure::Failed {
                    acceptance: record.id,
                    word,
                    detail,
                })
            }
            Settled::Incomplete { reason, detail } => {
                let flagged = self.flag(wiki, reason, Some(record.id.to_string()));
                Err(AcceptFailure::Incomplete {
                    acceptance: record.id,
                    reason,
                    flagged,
                    detail,
                })
            }
            Settled::Sealed {
                block_id,
                patch_id,
                unconfirmed,
                seeds_left,
            } => {
                // 6. The ids, the state, the caches.
                let recorded = self.record_seal(
                    wiki, &proposal, acceptor, &record, &intent, &block_id, &patch_id,
                );
                let mut flagged = true;
                if unconfirmed.is_some() {
                    // The change is sealed, and a person looks at why it could not be confirmed.
                    flagged = self.flag(
                        wiki,
                        RecoveryReason::SealedUnconfirmed,
                        Some(record.id.to_string()),
                    );
                }
                if let Err(e) = recorded {
                    let flagged = self.flag(
                        wiki,
                        RecoveryReason::SealedUnconfirmed,
                        Some(record.id.to_string()),
                    );
                    return Err(AcceptFailure::Incomplete {
                        acceptance: record.id,
                        reason: RecoveryReason::SealedUnconfirmed,
                        flagged,
                        detail: format!(
                            "the change is sealed (block {block_id}) and its records could not be completed: {e}"
                        ),
                    });
                }
                let _ = flagged;
                self.parts.reads.forget_wiki(wiki);
                let reconciliation = self.reconcile(wiki, &intent.view, &intent.base_block);
                self.observe(AcceptStep::Reconciled);
                Ok(Step::Done(Signed {
                    acceptance: record.id,
                    block_id,
                    patch_id,
                    reauthored,
                    unconfirmed,
                    seeds_left,
                    ids_attached,
                    reconciliation,
                    paths,
                    notified: 0,
                }))
            }
        }
    }

    /// End an acceptance that committed nothing: *failed* with its word, its intent abandoned.
    fn fail_record(
        &self,
        record: &AcceptanceRecord,
        intent: Option<&IntentRecord>,
        word: &'static str,
    ) {
        let at = (self.clock)();
        let _ = self.parts.acceptances.finish_acceptance(&AcceptanceResult {
            acceptance: record.id.clone(),
            outcome: AcceptanceOutcome::Failed {
                word: word.to_owned(),
            },
            at,
        });
        if let Some(intent) = intent {
            let _ = self.parts.acceptances.complete_intent(&IntentCompletion {
                intent: intent.id.clone(),
                end: IntentEnd::Abandoned,
                at,
            });
        }
    }

    /// The seal's ids, the intent's end and the proposal's new state, together in the order that leaves the least
    /// to guess if one of them fails.
    #[allow(clippy::too_many_arguments)]
    fn record_seal(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        acceptor: &AccountId,
        record: &AcceptanceRecord,
        intent: &IntentRecord,
        block_id: &str,
        patch_id: &str,
    ) -> Result<(), StoreError> {
        let at = (self.clock)();
        self.parts
            .acceptances
            .finish_acceptance(&AcceptanceResult {
                acceptance: record.id.clone(),
                outcome: AcceptanceOutcome::Sealed {
                    block_id: block_id.to_owned(),
                    patch_id: patch_id.to_owned(),
                },
                at,
            })?;
        self.observe(AcceptStep::AcceptanceSealed);
        self.parts.acceptances.complete_intent(&IntentCompletion {
            intent: intent.id.clone(),
            end: IntentEnd::Completed,
            at,
        })?;
        self.observe(AcceptStep::IntentCompleted);
        self.parts
            .proposals
            .record_accepted(wiki, &proposal.id, acceptor)
            .map(|_| self.observe(AcceptStep::ProposalAccepted))
            .map_err(|e| match e {
                ProposalError::Store(s) => s,
                other => StoreError::Backend(other.to_string()),
            })
    }

    /// The reconciliation (RFC 002 D-7), with the lease still held: **bounded** to the blocks of `view` since the
    /// intent's base (handoff 004, P7). The full pass is recovery's and the schedule's.
    fn reconcile(&self, wiki: &WikiRecord, view: &str, base: &str) -> Reconciled {
        if !self.reconcile {
            return Reconciled::Skipped;
        }
        shared::reconcile_since_wiki(
            &*self.parts.storage,
            &self.parts.layout,
            wiki,
            &self.parts.acceptances,
            &self.parts.acts,
            self.audit.as_ref(),
            (view, base),
            (self.clock)(),
        )
    }

    /// After the seal and the reconciliation: release the lease, then tell the proposer and the watchers and ask
    /// for the follow-ups. Returns how many watchers were told.
    fn after_lease(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        acceptor: &AccountId,
        lease: Lease,
        signed: &Signed,
    ) -> usize {
        drop(lease);
        let notified = match &self.notifier {
            Some(notifier) => {
                notifier.accepted(wiki, proposal, acceptor);
                notifier.page_changed(wiki, &proposal.id, &signed.paths, acceptor)
            }
            None => 0,
        };
        self.follow_up.warm_title_index(wiki);
        self.follow_up.enqueue_verification(wiki);
        notified
    }
}

/// How the steps under the lease ended.
enum Step {
    Done(Signed),
    Stopped(AcceptFailure),
    AlreadySigned(Accepted),
}

#[cfg(test)]
mod tests;
