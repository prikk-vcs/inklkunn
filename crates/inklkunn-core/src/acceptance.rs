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
//!    content-level merge ([`crate::merge`]).
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

use std::sync::Arc;

use inklkunn_prikk::Storage;
use inklkunn_signer::accept::{
    Accept, AcceptError, AcceptanceToken, PreparedChange, SealedChange, Unconfirmed,
};
use inklkunn_signer::message::{Actor, BlockId, Message, MessageError, Summary};
use inklkunn_store::{
    AcceptanceBegin, AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult,
    AcceptanceStore, AccountId, AccountStore, ActStore, AuditSink, DisplayName, InboxEvent,
    InboxEventId, InboxEventKind, InboxStore, IntentCompletion, IntentEnd, IntentId, IntentRecord,
    OverlapRecord, ProposalId, ProposalRecord, RecoveryFlag, RecoveryReason, Role, RoleStore,
    RootLayout, StoreError, Visibility, WikiRecord,
};

use crate::authorize::{Action, Principal};
use crate::lease::{Busy, BusyWhy, Lease, LeaseError, Purpose, WikiLeases};
use crate::proposal::{ProposalError, ProposalService, QuotaLimit, QuotaRefusal, Reset, machine};
use crate::read::ReadService;
use crate::reconcile::Finding;

mod ledger;
mod prepare;
mod shared;
mod system;
#[cfg(test)]
pub(crate) mod testing;

pub use ledger::{NoLedger, StoreActs, StoreLedger, digest_from_hex, message_digest};
pub use system::{SystemActFailure, SystemActs, SystemRequest, SystemSigned};

use prepare::{PrepareError, Prepared};
use shared::{Blocked, TipError};

/// The policy the decision was made under (PR-12): the bootstrap's fixed role ladder, until RFC 006 makes the
/// policy data and gives it versions.
pub const POLICY_VERSION: &str = "bootstrap-1";

/// How many maintainers a wiki may have for one of them to accept their own proposal (RFC 004 D-3: the
/// bootstrap lets a maintainer of a wiki with fewer than three maintainers, so a one-person wiki is not
/// deadlocked; RFC 006 makes it data).
pub const SELF_ACCEPTANCE_BELOW: usize = 3;

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
    /// The tip already has everything the revision asks for.
    pub const NOTHING_TO_CHANGE: &str = "nothing-to-change";
}

/// What a change is recorded as: the message's `Kind` for a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// An ordinary proposal.
    Proposal,
    /// A rollback proposal (D-8: an undo is a proposal).
    Rollback,
}

/// Which kind of change a proposal is: the hook that rollback (handoff 004 T6) plugs into. The default says
/// every proposal is an ordinary one.
pub trait ChangeKinds: Send + Sync {
    /// The kind of the proposal's change.
    fn kind(&self, proposal: &ProposalRecord) -> ChangeKind;
}

/// Every proposal is an ordinary proposal (until rollback exists).
#[derive(Debug, Clone, Copy, Default)]
pub struct OrdinaryProposals;

impl ChangeKinds for OrdinaryProposals {
    fn kind(&self, _: &ProposalRecord) -> ChangeKind {
        ChangeKind::Proposal
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
    /// Inbox events queued.
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
            AcceptFailure::NotAllowed | AcceptFailure::NotFound => words::NOT_ALLOWED,
            AcceptFailure::Busy(_) => words::BUSY,
            AcceptFailure::Refused(_) => words::REFUSED_LIMIT,
            AcceptFailure::NeedsRecovery(_) | AcceptFailure::Incomplete { .. } => {
                words::NEEDS_RECOVERY
            }
            AcceptFailure::Overlap { .. } => words::OVERLAP,
            AcceptFailure::NothingToChange => words::NOTHING_TO_CHANGE,
            AcceptFailure::Failed { word, .. } => word,
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
        Err(e @ AcceptError::Busy { .. }) => Settled::Failed {
            word: words::BUSY,
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
    /// The inbox.
    pub inbox: Arc<dyn InboxStore>,
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
    kinds: Arc<dyn ChangeKinds>,
    audit: Option<Arc<dyn AuditSink>>,
    reconcile: bool,
    clock: fn() -> i64,
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
            kinds: Arc::new(OrdinaryProposals),
            audit: None,
            reconcile: true,
            clock: inklkunn_store::record::now_secs,
        }
    }

    /// Ask `follow_up` for the title index and the verification after a seal.
    #[must_use]
    pub fn with_follow_up(mut self, follow_up: Arc<dyn AfterSeal>) -> Self {
        self.follow_up = follow_up;
        self
    }

    /// Ask `kinds` which kind of change a proposal is (rollback, handoff 004 T6).
    #[must_use]
    pub fn with_kinds(mut self, kinds: Arc<dyn ChangeKinds>) -> Self {
        self.kinds = kinds;
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
        let proposal = self.parts.proposals.get(principal, wiki, id)?;
        // A retry of an acceptance that finished answers without taking the lease.
        if let Some(done) = self.sealed_before(&proposal, revision)? {
            return Ok(done);
        }
        self.check_revision(principal, wiki, &proposal, revision)?;
        self.check_self_acceptance(wiki, &proposal, &acceptor)?;

        // 1. The lease.
        let lease = self
            .parts
            .leases
            .acquire(&wiki.id, Purpose::Acceptance, None)
            .map_err(|e| self.lease_failure(e))?;
        match self.under_lease(principal, wiki, id, revision, &acceptor, &lease)? {
            Step::Done(signed) => {
                let notified = self.after_lease(wiki, &proposal, &acceptor, lease, &signed);
                Ok(Accepted::Signed(Signed { notified, ..signed }))
            }
            Step::Stopped(failure) => {
                drop(lease);
                if let AcceptFailure::Overlap { .. } = failure {
                    self.notify_overlap(wiki, &proposal, &acceptor);
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

    /// An acceptance of the revision that sealed, if there is one: the answer of a retry.
    fn sealed_before(
        &self,
        proposal: &ProposalRecord,
        revision: u32,
    ) -> Result<Option<Accepted>, AcceptFailure> {
        let entries = self
            .parts
            .acceptances
            .acceptances_of(&proposal.id, revision)?;
        for e in entries {
            if let Some(AcceptanceOutcome::Sealed { block_id, patch_id }) =
                e.result.map(|r| r.outcome)
            {
                return Ok(Some(Accepted::AlreadySigned {
                    acceptance: e.acceptance.id,
                    block_id,
                    patch_id,
                }));
            }
        }
        Ok(None)
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
            .revisions(principal, wiki, &proposal.id)?
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

    /// A maintainer accepts their own proposal only in a wiki with fewer than [`SELF_ACCEPTANCE_BELOW`]
    /// maintainers (RFC 004 D-3, the bootstrap policy).
    fn check_self_acceptance(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        acceptor: &AccountId,
    ) -> Result<(), AcceptFailure> {
        if proposal.author != *acceptor {
            return Ok(());
        }
        let maintainers = self
            .parts
            .roles
            .grants_in(&wiki.id)?
            .iter()
            .filter(|g| g.role >= Role::Maintainer)
            .count();
        if maintainers < SELF_ACCEPTANCE_BELOW {
            Ok(())
        } else {
            Err(AcceptFailure::NotAllowed)
        }
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

    /// The view's tip block, read now.
    fn tip_block(&self, wiki: &WikiRecord, view: &str) -> Result<String, AcceptFailure> {
        let repo = self.parts.layout.repo_dir(&wiki.id);
        shared::tip_block(&*self.parts.storage, &repo, view).map_err(|e| match e {
            TipError::NoSuchView => {
                AcceptFailure::Proposal(ProposalError::NoSuchView(view.to_owned()))
            }
            TipError::Unavailable(why) => AcceptFailure::Unavailable(why),
        })
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

    fn message(
        &self,
        proposal: &ProposalRecord,
        proposer: Actor,
        acceptor: Actor,
        base: &str,
    ) -> Result<String, AcceptFailure> {
        let described = |e: MessageError| AcceptFailure::Unavailable(format!("the message: {e}"));
        let summary = Summary::from_text(&proposal.summary).map_err(described)?;
        let base = BlockId::new(base).map_err(described)?;
        let message = match self.kinds.kind(proposal) {
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
        let proposal = self.parts.proposals.get(principal, wiki, id)?;
        if let Some(done) = self.sealed_before(&proposal, revision)? {
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
        let content = self
            .parts
            .proposals
            .revisions(principal, wiki, id)?
            .into_iter()
            .find(|r| r.seq == revision)
            .ok_or(AcceptFailure::NotFound)?
            .content;

        // 2. The tip, read now; the change, as it is or re-authored.
        let repo = self.parts.layout.repo_dir(&wiki.id);
        let tip = self.tip_block(wiki, &proposal.view)?;
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
        // CM-5: the blocks the proposal's comments are about carry their ids into the history.
        let comments = self.parts.proposals.comments(principal, wiki, id)?;
        let ids_attached = prepare::attach_ids(&mut files, &comments);

        // The accepted-bytes quota, on what is about to be written.
        let bytes = prepare::change_bytes(&files, &renames);
        self.parts
            .proposals
            .quotas()
            .check_accepted_bytes(&wiki.id, &proposal.author, bytes)?;

        // 4. The records, before the signer is asked.
        let role = self
            .parts
            .roles
            .role(acceptor, &wiki.id)?
            .ok_or(AcceptFailure::NotAllowed)?;
        let text = self.message(
            &proposal,
            self.actor(&proposal.author)?,
            self.actor(acceptor)?,
            &content.base.block_id,
        )?;
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
            acceptor_role: role,
            policy_version: POLICY_VERSION.to_owned(),
            proposer: proposal.author.clone(),
            digest: change.digest().to_hex(),
            bytes,
            created_at: (self.clock)(),
        };
        match self.parts.acceptances.begin_acceptance(&record)? {
            AcceptanceBegin::Started => {}
            AcceptanceBegin::Existing(found) => {
                return match found.result.map(|r| r.outcome) {
                    Some(AcceptanceOutcome::Sealed { block_id, patch_id }) => {
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

        // 5. The signer, on this thread, inside the lease.
        let token = AcceptanceToken::new(record.id.as_str());
        let answer = {
            let _scope = lease.scope();
            self.parts
                .signer
                .accept(&wiki.id, &proposal.view, &token, &change)
        };
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
                let reconciliation = self.reconcile(wiki);
                Ok(Step::Done(Signed {
                    acceptance: record.id,
                    block_id,
                    patch_id,
                    reauthored,
                    unconfirmed,
                    seeds_left,
                    ids_attached,
                    reconciliation,
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
        self.parts.acceptances.complete_intent(&IntentCompletion {
            intent: intent.id.clone(),
            end: IntentEnd::Completed,
            at,
        })?;
        self.parts
            .proposals
            .record_accepted(wiki, &proposal.id, acceptor)
            .map(|_| ())
            .map_err(|e| match e {
                ProposalError::Store(s) => s,
                other => StoreError::Backend(other.to_string()),
            })
    }

    /// The reconciliation (RFC 002 D-7), with the lease still held.
    fn reconcile(&self, wiki: &WikiRecord) -> Reconciled {
        if !self.reconcile {
            return Reconciled::Skipped;
        }
        shared::reconcile_wiki(
            &*self.parts.storage,
            &self.parts.layout,
            wiki,
            &self.parts.acceptances,
            &self.parts.acts,
            self.audit.as_ref(),
            (self.clock)(),
        )
    }

    /// After the seal and the reconciliation: release the lease, then queue what the people involved are
    /// owed and ask for the follow-ups. Returns how many events were queued.
    fn after_lease(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        acceptor: &AccountId,
        lease: Lease,
        _signed: &Signed,
    ) -> usize {
        drop(lease);
        let notified = self.notify(
            wiki,
            proposal,
            InboxEventKind::Accepted,
            std::slice::from_ref(&proposal.author),
            acceptor,
        );
        self.follow_up.warm_title_index(wiki);
        self.follow_up.enqueue_verification(wiki);
        notified
    }

    fn notify_overlap(&self, wiki: &WikiRecord, proposal: &ProposalRecord, acceptor: &AccountId) {
        let _ = self.notify(
            wiki,
            proposal,
            InboxEventKind::NeedsRevision,
            &[proposal.author.clone(), acceptor.clone()],
            acceptor,
        );
    }

    /// Queue `kind` for each recipient who is not the actor and who can read the wiki: an event carries ids and
    /// a kind, never content, and goes to no one who could not read what it is about (RFC 004 D-9). A
    /// failure to queue an event is not a failure of the act it tells of.
    fn notify(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        kind: InboxEventKind,
        recipients: &[AccountId],
        actor: &AccountId,
    ) -> usize {
        let mut queued = 0;
        let mut told: Vec<&AccountId> = Vec::new();
        for who in recipients {
            // The acceptor is told of an overlap by the answer itself, and of an acceptance by having made it.
            if told.contains(&who) || (who == actor && kind == InboxEventKind::Accepted) {
                continue;
            }
            told.push(who);
            if !self.can_read(&wiki.id, wiki.visibility, who) {
                continue;
            }
            let event = InboxEvent {
                id: InboxEventId::new(),
                account: who.clone(),
                wiki: wiki.id.clone(),
                proposal: Some(proposal.id.clone()),
                kind,
                actor: Some(actor.clone()),
                created_at: (self.clock)(),
                read_at: None,
            };
            if self.parts.inbox.push_event(&event).is_ok() {
                queued += 1;
            }
        }
        queued
    }

    /// Whether the account may read the wiki, by the rule `authorize` applies to a person with a full session
    /// (RFC 002 D-2): a role in the wiki, or a wiki that is not restricted.
    fn can_read(
        &self,
        wiki: &inklkunn_store::WikiId,
        visibility: Visibility,
        who: &AccountId,
    ) -> bool {
        match self.parts.roles.role(who, wiki) {
            Ok(Some(role)) => role >= Action::Read.minimum_role(),
            Ok(None) => visibility != Visibility::Restricted,
            // An answer that cannot be had is no.
            Err(_) => false,
        }
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
