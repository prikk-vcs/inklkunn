//! The witnesses, and the plan made of them (RFC 004 D-7; handoff 004 T5). **Building a plan is a pure function** of
//! what was read ([`Witness`]): it does no I/O, so every row of its decision table is a unit test with no repository.
//!
//! A plan is either **steps**, in order, each one a thing the signer or the records can do and that has a name, or a
//! **stop**: a state recovery will not act on, with a fixed word, because acting would be guessing. Nothing is
//! rewritten. A sound queued patch that matches no recorded act is not discarded, because prikk has no verb that
//! removes one (`doctor --discard-damaged-commits` acts on an acknowledged commit the log no longer holds soundly,
//! and refuses over anything else): it is a stop, and a person decides. A publication a killed `seal` left unfinished
//! (`doctor` names `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING` and nothing else) is the one state where prikk's own
//! recommendation, `seal` once more, is taken as a step, and only when the block published is the unfinished act's
//! (P15).

use inklkunn_prikk::{RepoStatus, storage::QueuedPatch};
use inklkunn_signer::recover::text::{
    ACCEPTANCE_LOCK_KINDS, DoctorReport, HeldLock, PUBLICATION_UNTRUSTED_CODE, UncommitPlan,
};
use inklkunn_signer::{RecoveryFacts, RecoveryOp, WorktreeState};
use inklkunn_store::{
    AcceptanceId, AccountId, LeaseHolder, ProposalId, ProposalState, RecoveryFlag, SystemActId,
    ViewActId, WikiId,
};
use serde_json::{Value, json};

use super::processes::Foreign;

/// The act an unfinished change belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActRef {
    /// A proposal's acceptance.
    Acceptance(AcceptanceId),
    /// An act the service makes for itself (an import, a system change).
    System(SystemActId),
}

impl ActRef {
    /// The id as the signer's acceptance token names it.
    pub fn token(&self) -> String {
        match self {
            ActRef::Acceptance(id) => id.to_string(),
            ActRef::System(id) => id.to_string(),
        }
    }

    /// `acceptance` or `system act`.
    pub const fn kind(&self) -> &'static str {
        match self {
            ActRef::Acceptance(_) => "acceptance",
            ActRef::System(_) => "system-act",
        }
    }
}

/// How an act ended, as far as its record says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// No end is recorded.
    Not,
    /// Recorded as sealed, with this block and patch.
    Sealed {
        /// The block.
        block_id: String,
        /// The patch.
        patch_id: String,
    },
    /// Recorded as failed.
    Failed,
}

/// What the history says about an act's change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chain {
    /// The change is in the history, in this block.
    Sealed {
        /// The block.
        block_id: String,
        /// The patch.
        patch_id: String,
    },
    /// The change is not in the history and the view's tip is the block it was to be written on.
    NotSealed,
    /// The change is not in the history and the tip is another block: something else was written.
    TipMoved,
    /// The history could not be read as far as the question needs (the text is for operators).
    Unreadable(String),
    /// There is nothing to look for: the act never got as far as a message.
    NoMessage,
}

/// One unfinished act and what is known of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActWitness {
    /// Which act.
    pub act: ActRef,
    /// The view the change lands on.
    pub view: String,
    /// The block the change was made on top of (from the intent), when the act got as far as an intent.
    pub base_block: Option<String>,
    /// The message that was to be sealed, when the act got as far as one.
    pub message: Option<String>,
    /// The digest of that message.
    pub message_digest: Option<String>,
    /// How the act's own record ended.
    pub ended: Ended,
    /// An intent exists and has not ended.
    pub intent_open: bool,
    /// The proposal, for an acceptance.
    pub proposal: Option<ProposalId>,
    /// The proposal's state now, for an acceptance.
    pub proposal_state: Option<ProposalState>,
    /// The acceptor, for an acceptance.
    pub acceptor: Option<AccountId>,
    /// What the history says.
    pub chain: Chain,
}

impl ActWitness {
    /// Records still to write for an act whose change is in the history: `(its end, its intent's end, its proposal's
    /// acceptance)`.
    pub fn needs_records(&self) -> (bool, bool, bool) {
        (
            !matches!(self.ended, Ended::Sealed { .. }),
            self.intent_open,
            self.proposal.is_some() && self.proposal_state == Some(ProposalState::HeldForReview),
        )
    }
}

/// The newest block of an unfinished act's view, as far as the question "is it the act's change" needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// The view.
    pub view: String,
    /// The newest block.
    pub block_id: String,
    /// The block before it (its parent), when the history has one.
    pub parent: Option<String>,
    /// The digests of the messages of the patches the newest block seals.
    pub message_digests: Vec<String>,
}

/// A `seal` published its block and stopped before it drained the queue (handoff 004, P15): `prikk doctor` names
/// exactly one thing, the error `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING`, whose recommendation is to run `seal
/// --allow-no-audit` for the ref once more. Whether that is safe is a question of whose block was published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupPending {
    /// `prikk status` refused (an integrity error) and so the queue could not be read; its absence from the witness
    /// is a placeholder, not a finding.
    pub status_refused: bool,
    /// The newest block of the one unfinished act's view; `None` when there is not exactly one unfinished act.
    pub published: Option<Published>,
}

/// What prikk shows of the view an unfinished creation was making (handoff 005 K4). The listing includes closed
/// views: a view closed since was shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewShown {
    /// The view is not among prikk's views.
    Absent,
    /// The view is among them, and its newest block is `tip`.
    Shown {
        /// The block at the view's tip.
        tip: String,
    },
    /// A view whose name differs from the creation's only by case is among them, and none of exactly that name: not
    /// this creation's view, and not told apart from it.
    InAnotherCase,
    /// The view is among them and its newest block could not be read (the text is for operators).
    Unreadable(String),
}

/// One view creation that never ended (handoff 005 K4): the act's record and what prikk shows of its view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewActWitness {
    /// The act's record.
    pub act: ViewActId,
    /// The view it was making, `heads/<name>`.
    pub view: String,
    /// The block the view was forked from, as the record kept it.
    pub from_block: String,
    /// What prikk shows.
    pub shown: ViewShown,
}

/// Everything recovery read before it made a plan.
#[derive(Debug, Clone)]
pub struct Witness {
    /// The wiki.
    pub wiki: WikiId,
    /// The wiki's flag, if it is flagged.
    pub flag: Option<RecoveryFlag>,
    /// The record of another holder that was still there when the lease was taken (that holder died holding it).
    pub dead_holder: Option<LeaseHolder>,
    /// Recovery acts that were begun and never ended (an earlier recovery that was itself interrupted).
    pub interrupted_recoveries: usize,
    /// The unfinished acts.
    pub acts: Vec<ActWitness>,
    /// The view creations that never ended (handoff 005 K4).
    pub view_acts: Vec<ViewActWitness>,
    /// `prikk status`.
    pub status: RepoStatus,
    /// `prikk doctor`. When [`Witness::retirement`] is set, the publication-trust findings a retired key accounts
    /// for are already taken out of it.
    pub doctor: DoctorReport,
    /// What the signer said of a repository that does not verify **only because of a retired key** (P2, DR-10),
    /// when that is the case: `verify` and `doctor` fail by publication trust for what the retired key sealed and
    /// by nothing else. The wiki is not frozen for it.
    pub retirement: Option<String>,
    /// A key retirement that was begun and whose record was not finished (handoff 004, Q1), as the signer says it:
    /// the key and the command that finishes it. While it is so, nothing is excused, and recovery stops on the
    /// publication-trust findings of the retired key instead of taking them for damage. The command needs the
    /// operator's key authority; recovery never runs it.
    pub retirement_unfinished: Option<String>,
    /// `prikk worktree-status`; `None` when it could not be asked because the log is torn (prikk refuses to
    /// report a worktree over a log with trailing partial bytes, and says to repair the log first): the worktree
    /// is then taken to need putting back after the torn commit is discarded.
    pub worktree: Option<WorktreeState>,
    /// The locks that are held.
    pub locks: Vec<HeldLock>,
    /// Processes of somebody else's that may be working in the repository.
    pub foreign: Vec<Foreign>,
    /// The write lease is held by this recovery. `false` for a plan that is only shown.
    pub lease_held: bool,
    /// A publication whose cleanup a killed `seal` did not finish, when `doctor` names exactly that (P15).
    pub cleanup_pending: Option<CleanupPending>,
    /// What prikk said when it refused to open the repository by a format marker this prikk is not known to refuse
    /// (maintenance M17, M20, M33). Then nothing else of prikk was read: the fields that come from prikk hold nothing.
    pub format_marker: Option<MarkerError>,
}

/// A format marker prikk will not open the repository by, and that this prikk is not known to refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerError {
    /// prikk named the format (maintenance M17).
    Numbered(u64),
    /// prikk's own integrity error: the marker holds something that is not a format number (`8\r\n`, an empty file;
    /// maintenance M20).
    NotANumber,
}

/// Why recovery will not act: each a fixed word. None names a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stop {
    /// A queued patch that no recorded act describes, and that prikk has no verb to remove.
    ForeignQueuedPatch,
    /// A queued patch beside a recorded act that it is not the change of.
    QueuedPatchIsNotTheAct,
    /// More than one patch is queued.
    SeveralQueued,
    /// A killed `seal` left a publication to finish, and the block it published is not the change of the one
    /// unfinished act (or there is no such act, or it ended, or the block was made on another base).
    PublishedBlockIsNotTheAct,
    /// More than one act is unfinished.
    SeveralActs,
    /// The view moved past the block the act was made on, and the act's change is not in the history.
    TipMoved,
    /// An interrupted checkout or materialization: prikk's own state, which a person decides about.
    InterruptedState,
    /// `doctor` reports something recovery has no answer for.
    DoctorTrouble,
    /// `doctor` reports publication-trust findings, or a patch waits to be sealed, and a key retirement was begun and
    /// is not finished: the operator finishes it with `inklkunn wiki keys finish-retirement <wiki>`, or gives it up with
    /// `inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>` (recovery cannot: it needs the key authority),
    /// and then recovers. Nothing new is sealed meanwhile (W1).
    RetirementUnfinished,
    /// A killed `seal` left a publication to finish, and `doctor` also reports publication-trust findings that nothing
    /// excuses: while the cleanup is pending `verify` fails for the ref publication as well as for publication trust,
    /// and only a failure by publication trust alone can be excused by a retirement's record (Q6). Recovery does not
    /// finish the cleanup beside findings it cannot account for.
    CleanupBesideTrustFindings,
    /// A lock of a kind an acceptance does not leave.
    UnknownLock,
    /// A `prikk` or `bwrap` of somebody else's may be working in the repository, so no step is taken.
    ForeignProcess,
    /// The history could not be read as far as the question needs.
    HistoryUnreadable,
    /// prikk will not open the repository because of the format marker (`.prikk/FORMAT`) it holds, and this prikk is not
    /// known to refuse that format: the marker is damaged, or names a format this build does not know (maintenance
    /// M17). Nothing else could be read, so nothing is concluded about the rest.
    FormatMarker,
    /// A view's creation never ended, and what prikk shows of the view is neither of the two states recovery knows
    /// (the view is there and `doctor` is clean; or it is there at the block it was forked from, with exactly its own
    /// ref's publication unfinished; or it is not there and nothing is unfinished): the view is at another block, or
    /// its tip cannot be read, or it is not listed while `doctor` names its publication (handoff 005 K4).
    ViewCreationUnclear,
}

impl Stop {
    /// The fixed word.
    pub const fn as_str(self) -> &'static str {
        match self {
            Stop::ForeignQueuedPatch => "foreign-queued-patch",
            Stop::QueuedPatchIsNotTheAct => "queued-patch-is-not-the-act",
            Stop::SeveralQueued => "several-queued",
            Stop::PublishedBlockIsNotTheAct => "published-block-is-not-the-act",
            Stop::SeveralActs => "several-acts",
            Stop::TipMoved => "tip-moved",
            Stop::InterruptedState => "interrupted-state",
            Stop::DoctorTrouble => "doctor-trouble",
            Stop::RetirementUnfinished => "retirement-unfinished",
            Stop::CleanupBesideTrustFindings => "cleanup-beside-trust-findings",
            Stop::UnknownLock => "unknown-lock",
            Stop::ForeignProcess => "foreign-process",
            Stop::HistoryUnreadable => "history-unreadable",
            Stop::FormatMarker => "format-marker",
            Stop::ViewCreationUnclear => "view-creation-unclear",
        }
    }

    /// What the stop means, for the operator who has to decide.
    pub const fn meaning(self) -> &'static str {
        match self {
            Stop::ForeignQueuedPatch => {
                "a patch is queued that no recorded act describes; sealing it would put an unaccepted change in the history, and recovery does not discard a patch on its own judgement (prikk has no verb that removes a sound queued patch before 0.57.0; from 0.57.0 `prikk uncommit` does, and a person confirms it by the patch's id: see the note)"
            }
            Stop::QueuedPatchIsNotTheAct => {
                "a patch is queued that is not the change of the recorded act (its message, its base or its files differ)"
            }
            Stop::SeveralQueued => "more than one patch is queued",
            Stop::PublishedBlockIsNotTheAct => {
                "prikk reports a publication whose cleanup a killed seal did not finish, and the block it published is not the change of the one unfinished act (its message, its base or its patch differ, or no such act is unfinished)"
            }
            Stop::SeveralActs => "more than one act is unfinished",
            Stop::TipMoved => {
                "the view has moved past the block the unfinished act was made on, and the act's change is not in the history"
            }
            Stop::InterruptedState => "prikk reports an interrupted checkout or materialization",
            Stop::DoctorTrouble => "prikk's doctor reports damage that recovery has no answer for",
            Stop::CleanupBesideTrustFindings => {
                "prikk reports a publication whose cleanup a killed seal did not finish, and publication-trust findings besides (a retired key's blocks, say): while the cleanup is pending verify fails for the ref publication too, and only a failure by publication trust alone is excused by a retirement's record, so recovery does not finish the cleanup here"
            }
            Stop::RetirementUnfinished => {
                "a key retirement was begun and is not finished, so nothing new is sealed, and prikk's doctor may report publication-trust findings that nothing excuses; finish it with `inklkunn wiki keys finish-retirement <wiki>` or give it up with `inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>` (the old key stays trusted and nothing is retired; `inklkunn wiki keys retire <wiki> --key <id> --confirm <slug> --accept-unverifiable-history` retires it later); these commands need the operator's key authority and recovery never runs them; then recover again. When the note names findings that cannot be traced to what the key sealed for this service, finishing refuses and recovery does not cure them either: the note names the way out"
            }
            Stop::UnknownLock => "a lock is held of a kind an acceptance does not leave",
            Stop::ForeignProcess => {
                "a prikk or bwrap of somebody else's may be working in the repository"
            }
            Stop::HistoryUnreadable => "the history could not be read as far as recovery needs",
            Stop::FormatMarker => {
                "prikk will not open the repository because of its format marker (`.prikk/FORMAT`), and this prikk is not known to refuse that format: the marker is damaged, or names a format this build does not know; compare it with a backup (docs/RECOVERY.md, format-marker); nothing else could be read, so nothing is concluded about the rest of the repository"
            }
            Stop::ViewCreationUnclear => {
                "the creation of a view never ended, and what prikk shows is not a state recovery can tell apart: the view is at another block than the one it was forked from, or its block cannot be read, or the view is not listed while doctor names its publication"
            }
        }
    }
}

/// One lock to clear, with what prikk said about it (kept; never used to decide).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    /// The lock file.
    pub path: String,
    /// Its kind.
    pub kind: String,
    /// The process id it records.
    pub recorded_pid: u32,
    /// prikk's advisory words, **not evidence** (ledger PL-8).
    pub prikk_liveness: String,
}

/// One step of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Clear stale locks of the kinds an acceptance leaves, on the lease record's word.
    ClearLocks {
        /// The locks.
        locks: Vec<LockEntry>,
    },
    /// Seal the one queued patch, which is the change of `act`.
    SealQueued {
        /// The act.
        act: ActRef,
        /// The view.
        view: String,
        /// The queued patch.
        patch_id: String,
    },
    /// Finish the cleanup of the publication of `block_id`, which is the change of `act`: `seal --allow-no-audit`
    /// once more for `view`, as prikk's own `doctor` recommends (P15). It makes no block.
    CompletePublication {
        /// The act.
        act: ActRef,
        /// The view.
        view: String,
        /// The block that was published.
        block_id: String,
    },
    /// Finish the interrupted publication of `view`, the view the creation `act` was making at `block_id`:
    /// `prikk ref complete <view>` once, as prikk's own `doctor` recommends for it (not `seal`: handoff 005 K4). It
    /// makes no block.
    CompleteViewCreation {
        /// The view-creation act.
        act: ViewActId,
        /// The view, `heads/<name>`.
        view: String,
        /// The block the view points at.
        block_id: String,
    },
    /// Record the view creation `act` as created: `view` is among prikk's views and nothing of its publication is
    /// unfinished.
    EndViewCreated {
        /// The view-creation act.
        act: ViewActId,
        /// The view, `heads/<name>`.
        view: String,
    },
    /// Record the view creation `act` as failed (`interrupted`): `view` is not among prikk's views and nothing is
    /// unfinished, so nothing of the creation happened.
    EndViewFailed {
        /// The view-creation act.
        act: ViewActId,
        /// The view, `heads/<name>`.
        view: String,
    },
    /// `doctor --discard-damaged-commits`.
    DiscardDamaged,
    /// `prikk uncommit <patch>` (prikk 0.57.0, ledger PK-43): take the one queued patch back. **Only when a person
    /// confirmed that patch by its id** (`--discard-queued-patch`); never on recovery's own judgement.
    DiscardQueued {
        /// The patch.
        patch_id: String,
    },
    /// Put the worktree of `view` back to the tip.
    RestoreWorktree {
        /// The view.
        view: String,
    },
    /// Record `act` as failed (`interrupted`) and end its intent: nothing of it is in the history.
    EndFailed {
        /// The act.
        act: ActRef,
        /// The act's intent ends as `discarded` (a damaged commit was removed) and not `abandoned`.
        discarded: bool,
    },
    /// Record `act` as sealed: the block is found in the history, and every record not yet written is written.
    EndSealed {
        /// The act.
        act: ActRef,
        /// The block, when it is already known.
        block_id: Option<String>,
    },
    /// `prikk verify`.
    Verify,
    /// RFC 002 D-7's reconciliation.
    Reconcile,
    /// Clear the wiki's flag.
    ClearFlag,
}

impl Step {
    /// The step's name, a fixed word.
    pub const fn name(&self) -> &'static str {
        match self {
            Step::ClearLocks { .. } => "clear-locks",
            Step::SealQueued { .. } => "seal-queued",
            Step::CompletePublication { .. } => "complete-publication",
            Step::CompleteViewCreation { .. } => "complete-view-creation",
            Step::EndViewCreated { .. } => "end-view-created",
            Step::EndViewFailed { .. } => "end-view-failed",
            Step::DiscardDamaged => "discard-damaged",
            Step::DiscardQueued { .. } => "discard-queued",
            Step::RestoreWorktree { .. } => "restore-worktree",
            Step::EndFailed { .. } => "end-failed",
            Step::EndSealed { .. } => "end-sealed",
            Step::Verify => "verify",
            Step::Reconcile => "reconcile",
            Step::ClearFlag => "clear-flag",
        }
    }

    /// The step as one line for a person.
    pub fn line(&self) -> String {
        match self {
            Step::ClearLocks { locks } => format!(
                "clear {} stale lock(s) ({}); the lease record shows no live holder, and prikk's own liveness line is not consulted",
                locks.len(),
                locks
                    .iter()
                    .map(|l| format!("{} recorded pid {} prikk says \"{}\"", l.kind, l.recorded_pid, l.prikk_liveness))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Step::SealQueued { act, patch_id, .. } => format!(
                "seal the queued patch {patch_id}, which is the change of {} {}; it is not committed again",
                act.kind(),
                act.token()
            ),
            Step::CompletePublication {
                act, block_id, ..
            } => format!(
                "run seal --allow-no-audit once, as prikk's doctor recommends, to finish the cleanup of block {block_id}, which is the change of {} {}; no block is made. Before it, the signer checks the queue's content as well as its size: prikk's verify reports this state only when the queue holds exactly the patches of the tip block (PK-44), and the tip block must hold exactly the one published patch",
                act.kind(),
                act.token()
            ),
            Step::CompleteViewCreation {
                act,
                view,
                block_id,
            } => format!(
                "run ref complete {view} once, as prikk's doctor recommends, to finish the publication of the view that view creation {act} was making at block {block_id}; no block is made"
            ),
            Step::EndViewCreated { act, view } => format!(
                "record view creation {act} as created: {view} is among prikk's views and nothing of its publication is unfinished"
            ),
            Step::EndViewFailed { act, view } => format!(
                "record view creation {act} as failed (interrupted): {view} is not among prikk's views and nothing is unfinished, so nothing of it happened"
            ),
            Step::DiscardDamaged => {
                "run doctor --discard-damaged-commits: remove an acknowledged commit the log no longer holds soundly (prikk saves the bytes first)".to_owned()
            }
            Step::DiscardQueued { patch_id } => format!(
                "take the queued patch {patch_id} back with `prikk uncommit` because a person confirmed it by its id: it leaves the queue and stays in prikk's recovery log, where `prikk doctor --recovery-restore <run>` returns it"
            ),
            Step::RestoreWorktree { view } => {
                format!("put the worktree of {view} back to what its tip says")
            }
            Step::EndFailed { act, discarded } => format!(
                "record {} {} as failed (interrupted): nothing of it is in the history; its intent ends as {}",
                act.kind(),
                act.token(),
                if *discarded { "discarded" } else { "abandoned" }
            ),
            Step::EndSealed { act, block_id } => format!(
                "record {} {} as sealed{}: write whichever of its end, its intent's end and its proposal's acceptance is not written",
                act.kind(),
                act.token(),
                block_id
                    .as_ref()
                    .map(|b| format!(" in block {b}"))
                    .unwrap_or_default()
            ),
            Step::Verify => "run prikk verify".to_owned(),
            Step::Reconcile => "reconcile the history with the records".to_owned(),
            Step::ClearFlag => "clear the wiki's needs-recovery flag".to_owned(),
        }
    }
}

/// The plan: what was read, and what will be done or why nothing will.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The wiki.
    pub wiki: WikiId,
    /// The view in question, when there is one.
    pub view: Option<String>,
    /// What was read, one fixed-vocabulary line each.
    pub witnesses: Vec<String>,
    /// The steps, in order. Empty when the plan stops, or when there is nothing to do.
    pub steps: Vec<Step>,
    /// Why recovery will not act, when it will not.
    pub stop: Option<Stop>,
    /// Things recorded that do not block: unreached objects, a held-open stale lease record.
    pub notes: Vec<String>,
    /// Why the patch id a person typed with `--discard-queued-patch` was **not used**, when one was typed and no
    /// `discard-queued` step results (maintenance M4): the audit trail says what happened, not what was asked.
    pub discard_unused: Option<String>,
}

impl Plan {
    /// Nothing to do and nothing in the way.
    pub fn is_empty(&self) -> bool {
        self.stop.is_none() && self.steps.is_empty()
    }

    /// The operations the signer may be asked to do under this plan (what the recorded act binds it to).
    pub fn ops(&self) -> Vec<RecoveryOp> {
        self.steps
            .iter()
            .filter_map(|s| match s {
                Step::ClearLocks { locks } => Some(RecoveryOp::ClearLocks {
                    paths: locks.iter().map(|l| l.path.clone()).collect(),
                }),
                Step::SealQueued { act, view, .. } => Some(RecoveryOp::SealQueued {
                    acceptance: act.token(),
                    view: view.clone(),
                }),
                Step::CompletePublication { act, view, .. } => {
                    Some(RecoveryOp::CompletePublication {
                        acceptance: act.token(),
                        view: view.clone(),
                    })
                }
                Step::CompleteViewCreation { act, view, .. } => {
                    Some(RecoveryOp::CompleteViewCreation {
                        creation: act.to_string(),
                        view: view.clone(),
                    })
                }
                Step::DiscardDamaged => Some(RecoveryOp::DiscardDamaged),
                Step::DiscardQueued { patch_id } => Some(RecoveryOp::DiscardQueued {
                    patch: patch_id.clone(),
                }),
                Step::RestoreWorktree { view } => {
                    Some(RecoveryOp::RestoreWorktree { view: view.clone() })
                }
                _ => None,
            })
            .collect()
    }

    /// The plan as the JSON the recorded act keeps. Read back by [`facts_of`].
    pub fn to_json(&self) -> String {
        let step = |s: &Step| -> Value {
            match s {
                Step::ClearLocks { locks } => json!({
                    "step": "clear-locks",
                    "locks": locks.iter().map(|l| json!({
                        "path": l.path, "kind": l.kind, "recorded_pid": l.recorded_pid,
                        "prikk_liveness": l.prikk_liveness,
                    })).collect::<Vec<_>>(),
                }),
                Step::SealQueued {
                    act,
                    view,
                    patch_id,
                } => json!({
                    "step": "seal-queued", "acceptance": act.token(), "view": view, "patch": patch_id,
                }),
                Step::CompletePublication {
                    act,
                    view,
                    block_id,
                } => json!({
                    "step": "complete-publication", "acceptance": act.token(), "view": view, "block": block_id,
                }),
                Step::CompleteViewCreation {
                    act,
                    view,
                    block_id,
                } => json!({
                    "step": "complete-view-creation", "creation": act.to_string(), "view": view, "block": block_id,
                }),
                Step::EndViewCreated { act, view } => json!({
                    "step": "end-view-created", "creation": act.to_string(), "view": view,
                }),
                Step::EndViewFailed { act, view } => json!({
                    "step": "end-view-failed", "creation": act.to_string(), "view": view,
                }),
                Step::DiscardDamaged => json!({"step": "discard-damaged"}),
                Step::DiscardQueued { patch_id } => {
                    json!({"step": "discard-queued", "patch": patch_id})
                }
                Step::RestoreWorktree { view } => json!({"step": "restore-worktree", "view": view}),
                Step::EndFailed { act, discarded } => json!({
                    "step": "end-failed", "act": act.token(), "kind": act.kind(), "discarded": discarded,
                }),
                Step::EndSealed { act, block_id } => json!({
                    "step": "end-sealed", "act": act.token(), "kind": act.kind(), "block": block_id,
                }),
                other => json!({"step": other.name()}),
            }
        };
        json!({
            "v": 1,
            "wiki": self.wiki.to_string(),
            "view": self.view,
            "stop": self.stop.map(Stop::as_str),
            "witnesses": self.witnesses,
            "notes": self.notes,
            "steps": self.steps.iter().map(step).collect::<Vec<_>>(),
        })
        .to_string()
    }

    /// The plan as lines for a person.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!(
            "recovery plan for wiki {}{}",
            self.wiki,
            self.view
                .as_ref()
                .map(|v| format!(" ({v})"))
                .unwrap_or_default()
        )];
        out.extend(self.witnesses.iter().map(|w| format!("  saw: {w}")));
        out.extend(self.notes.iter().map(|n| format!("  note: {n}")));
        match (&self.stop, self.steps.is_empty()) {
            (Some(stop), true) => {
                out.push(format!(
                    "  STOP ({}): {}; recovery changes nothing and the wiki stays flagged for a person to decide",
                    stop.as_str(),
                    stop.meaning()
                ));
            }
            // Steps that come before a stop (a stale lock cleared beside an orphan patch, M6).
            (Some(stop), false) => {
                for (i, s) in self.steps.iter().enumerate() {
                    out.push(format!("  step {}: {}", i + 1, s.line()));
                }
                out.push(format!(
                    "  STOP ({}): {}; after the step(s) above recovery changes nothing more and the wiki stays flagged for a person to decide",
                    stop.as_str(),
                    stop.meaning()
                ));
            }
            (None, true) => out.push("  nothing to do".to_owned()),
            (None, false) => {
                for (i, s) in self.steps.iter().enumerate() {
                    out.push(format!("  step {}: {}", i + 1, s.line()));
                }
            }
        }
        out
    }
}

/// What the recorded recovery act `plan` allows the signer to do, read back from the JSON the plan was kept as. A
/// text that is not a plan of this version allows nothing.
pub fn facts_of(wiki: &WikiId, plan: &str) -> Option<RecoveryFacts> {
    let v: Value = serde_json::from_str(plan).ok()?;
    if v.get("v")?.as_u64()? != 1 || v.get("wiki")?.as_str()? != wiki.as_str() {
        return None;
    }
    let text = |s: &Value, k: &str| s.get(k).and_then(Value::as_str).map(str::to_owned);
    let mut ops = Vec::new();
    for s in v.get("steps")?.as_array()? {
        match s.get("step")?.as_str()? {
            "seal-queued" => ops.push(RecoveryOp::SealQueued {
                acceptance: text(s, "acceptance")?,
                view: text(s, "view")?,
            }),
            "complete-publication" => ops.push(RecoveryOp::CompletePublication {
                acceptance: text(s, "acceptance")?,
                view: text(s, "view")?,
            }),
            "complete-view-creation" => ops.push(RecoveryOp::CompleteViewCreation {
                creation: text(s, "creation")?,
                view: text(s, "view")?,
            }),
            "discard-damaged" => ops.push(RecoveryOp::DiscardDamaged),
            "discard-queued" => ops.push(RecoveryOp::DiscardQueued {
                patch: text(s, "patch")?,
            }),
            "restore-worktree" => ops.push(RecoveryOp::RestoreWorktree {
                view: text(s, "view")?,
            }),
            "clear-locks" => ops.push(RecoveryOp::ClearLocks {
                paths: s
                    .get("locks")?
                    .as_array()?
                    .iter()
                    .map(|l| text(l, "path"))
                    .collect::<Option<Vec<_>>>()?,
            }),
            _ => {}
        }
    }
    Some(RecoveryFacts {
        wiki: wiki.clone(),
        ops,
    })
}

// ---- the decision table ----

/// Would recovery take the orphan patch back if a person typed its id: the queue targets the branch the worktree is
/// on, and prikk's own plan names that same ref (maintenance M7: the same test decides the offer and the confirmation).
fn discard_honoured(w: &Witness, target: Option<&str>, offer: &UncommitPlan) -> bool {
    let current = w.status.current_branch.as_deref();
    target.is_some() && target == current && current == Some(offer.target.as_str())
}

/// Does the queue target the branch the worktree is on (so that a run would offer the command)?
fn on_the_worktrees_branch(w: &Witness, target: Option<&str>) -> bool {
    target.is_some() && target == w.status.current_branch.as_deref()
}

/// What the plan says about taking an orphan queued patch back, when it stops on it. The answer is why a patch id the
/// person typed was not used, when one was typed and it is already clear it will not be (maintenance M4).
fn discard_notes(
    plan: &mut Plan,
    patch_id: &str,
    target: Option<&str>,
    w: &Witness,
    discard: &DiscardInput,
) -> Option<String> {
    let given = discard.confirmed.as_deref();
    // A run with a lock held does not take the patch back: it clears the lock and stops with the preview (M21).
    let offer = match (&discard.offer, w.locks.is_empty()) {
        (DiscardOffer::InARun, false) => &DiscardOffer::AfterLocks,
        (offer, _) => offer,
    };
    match offer {
        DiscardOffer::Offered(offer) => {
            let honoured = discard_honoured(w, target, offer);
            let lines = offer.lines.join(" | ");
            if honoured {
                plan.notes.push(format!(
                    "prikk can take the queued patch {patch_id} back (`prikk uncommit`; it stays in prikk's recovery log and the worktree's files are put back to the tip). prikk says: {lines}. To do it, run `inklkunn wiki recover {} --discard-queued-patch {patch_id}`: typing the patch id is the confirmation, and nothing is taken back without it",
                    w.wiki
                ));
            } else {
                plan.notes.push(format!(
                    "prikk's own preview of taking the queued patch {patch_id} back (`prikk uncommit`): {lines}. Recovery will not take it back: the queue targets a branch other than the one the worktree is on (or prikk's plan names another), so no command is offered"
                ));
            }
            match given {
                Some(given) if given != patch_id => Some(format!(
                    "it names a patch that is not the queued one ({patch_id}); nothing was taken back"
                )),
                Some(_) if !honoured => Some(
                    "the queue targets a branch other than the one the worktree is on, so recovery will not take the patch back"
                        .to_owned(),
                ),
                _ => None,
            }
        }
        DiscardOffer::NoVerb => {
            plan.notes.push(
                "this prikk has no `uncommit` (0.57.0 and later have), so the queued patch cannot be taken back"
                    .to_owned(),
            );
            given.map(|_| "this prikk has no `uncommit` (0.57.0 and later have)".to_owned())
        }
        DiscardOffer::Unreadable(why) => {
            plan.notes.push(format!(
                "prikk's plan for taking the queued patch back could not be read ({why}); nothing is taken back"
            ));
            given.map(|_| {
                format!("prikk's plan for taking the queued patch back could not be read ({why})")
            })
        }
        DiscardOffer::InARun => {
            // The command is promised only when a run would offer it: the queue targets the worktree's branch (R-f).
            let command = if on_the_worktrees_branch(w, target) {
                "and the command that does it"
            } else {
                "(the queue targets a branch other than the one the worktree is on, so it offers no command)"
            };
            plan.notes.push(format!(
                "this prikk has `uncommit`: a run of `inklkunn wiki recover {}` shows prikk's own preview of taking the queued patch {patch_id} back {command}; `recover --plan` does not ask prikk for it, because prikk takes its lock to make the preview and a plan shown writes nothing",
                w.wiki
            ));
            given.map(|_| {
                if on_the_worktrees_branch(w, target) {
                    "`recover --plan` only shows; a run takes the patch back".to_owned()
                } else {
                    "`recover --plan` only shows, and the queue targets a branch other than the one the worktree is on, so recovery will not take the patch back".to_owned()
                }
            })
        }
        // A holder prikk says appears running is one `unlock --yes` does not clear (prikk 0.57.0): the run tries, and stops
        // at the lock with no preview, as the plan says (M31). A negative or unknown answer promises the clear: the
        // run makes it (the lease record decides, not prikk's line: PL-8).
        DiscardOffer::AfterLocks if w.locks.iter().any(HeldLock::appears_running) => {
            let held: Vec<String> = w
                .locks
                .iter()
                .filter(|l| l.appears_running())
                .map(|l| format!("{} (recorded pid {})", l.kind, l.recorded_pid))
                .collect();
            plan.notes.push(format!(
                "prikk's preview of taking the queued patch {patch_id} back needs prikk's lock, which a lock held by a process prikk says appears running holds ({}): prikk's `unlock --yes` does not clear such a holder, so the run tries to clear it (step 1) and stops at that step, with no preview, and takes nothing back",
                held.join(", ")
            ));
            given.map(|_| {
                "a lock held by a process prikk says appears running stands in the way: a run stops at clear-locks, shows no preview, and takes nothing back".to_owned()
            })
        }
        DiscardOffer::AfterLocks => {
            let (then, word) = if on_the_worktrees_branch(w, target) {
                (
                    "stops here with the preview and the command",
                    "a stale lock stands in the way: a run clears it and stops with the preview, and the next run takes the patch back",
                )
            } else {
                (
                    "stops here with the preview (the queue targets a branch other than the one the worktree is on, so it offers no command)",
                    "a stale lock stands in the way, and the queue targets a branch other than the one the worktree is on, so recovery will not take the patch back",
                )
            };
            plan.notes.push(format!(
                "prikk's preview of taking the queued patch {patch_id} back needs prikk's lock, which a stale lock holds: the run clears it first (step 1) and then reads the preview, and {then}"
            ));
            given.map(|_| word.to_owned())
        }
        DiscardOffer::NotAsked => None,
    }
}

/// Why a patch id typed with `--discard-queued-patch` cannot be used in this state, when the plan did not say already.
fn unused_reason(w: &Witness, plan: &Plan) -> String {
    let q = &w.status.queue;
    if q.patches.is_empty() {
        "nothing is queued, so there is nothing to take back".to_owned()
    } else if q.patches.len() > 1 {
        "more than one patch is queued; `uncommit` takes the newest only, and a person has not named which".to_owned()
    } else if !w.acts.is_empty() || !w.view_acts.is_empty() {
        "an act is unfinished, and a patch queued beside one is sealed when it is that act's and otherwise stops the plan: it is never taken back".to_owned()
    } else if let Some(stop) = plan.stop {
        format!("the plan stops first ({})", stop.as_str())
    } else {
        "the repository is not in the state this is for (one queued patch that no recorded act describes)"
            .to_owned()
    }
}

/// Say that the patch id a person typed was not used, and why: a note, and the reason kept for the audit trail.
pub fn word_unused(plan: &mut Plan, given: &str, why: &str) {
    plan.notes.push(format!(
        "`--discard-queued-patch {given}` was not used: {why}"
    ));
    plan.discard_unused = Some(why.to_owned());
}

fn matches_queued(act: &ActWitness, patch: &QueuedPatch, target: Option<&str>) -> bool {
    let digest = crate::acceptance::message_digest(&patch.message);
    act.message_digest.as_deref() == Some(digest.as_str()) && target == Some(act.view.as_str())
}

/// Is the block `published` the change of `act`: the act has not ended, the block sits on the view and the base the
/// act was made on, it seals exactly one patch, and that patch's message is the act's, to the digest; and the
/// history found it there (P15).
fn is_the_act(act: &ActWitness, published: &Published) -> bool {
    matches!(act.ended, Ended::Not)
        && published.view == act.view
        && published.parent.is_some()
        && published.parent == act.base_block
        && act.message_digest.is_some()
        && published.message_digests.len() == 1
        && published.message_digests.first() == act.message_digest.as_ref()
        && matches!(&act.chain, Chain::Sealed { block_id, .. } if *block_id == published.block_id)
}

/// Does `doctor` name the interrupted publication of exactly the ref `view` (prikk's own wording; see
/// [`inklkunn_signer::recover::Diagnostic::lead_ref`])?
fn lead_of(w: &Witness, view: &str) -> bool {
    w.doctor
        .diagnostics
        .iter()
        .any(|d| d.lead_ref() == Some(view))
}

/// What prikk offers for a queued patch that no recorded act describes (maintenance M3, prikk 0.57.0, ledger PK-43).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DiscardOffer {
    /// Not asked: this is not the state it is for (an act is unfinished, or not exactly one patch is queued).
    #[default]
    NotAsked,
    /// Not asked, because the plan is only shown and prikk's preview takes prikk's lock (maintenance M5): a run asks.
    /// It is the offer only for a prikk that has the verb and a wiki with no lock held; with a lock held the plan
    /// says [`DiscardOffer::AfterLocks`] (a run clears the lock and stops with the preview), whatever it was given
    /// (maintenance M21).
    InARun,
    /// Not asked yet, because a stale lock is held and the preview needs prikk's lock: the run clears the lock, then
    /// asks (maintenance M6), and stops with the preview. The plan promises the clear only when no holder is one prikk
    /// says appears running (`unlock --yes` does not clear that one): then it says the run stops at the lock (M31).
    AfterLocks,
    /// Asked, and prikk has no `uncommit` (before 0.57.0).
    NoVerb,
    /// Asked, and the answer could not be read; the text is for operators.
    Unreadable(String),
    /// prikk's own plan for taking the patch back (``prikk uncommit <patch> --plan-only``).
    Offered(UncommitPlan),
}

/// The two inputs of the discard that are not witnesses of the repository: what prikk offers, and which patch a person
/// confirmed by its id (`--discard-queued-patch <patch id>`). The default is no offer and no confirmation, which is
/// every plan before 0.57.0 had.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiscardInput {
    /// What prikk offers.
    pub offer: DiscardOffer,
    /// The patch id a person typed to confirm taking it back, when they did.
    pub confirmed: Option<String>,
}

/// Make the plan.
pub fn build(w: &Witness) -> Plan {
    build_with(w, &DiscardInput::default())
}

/// Make the plan, with what prikk offers for an orphan queued patch and what a person confirmed (see [`DiscardInput`]).
/// Whenever a patch id was typed and no `discard-queued` step results, the plan says it was not used, and why (M4).
pub fn build_with(w: &Witness, discard: &DiscardInput) -> Plan {
    let mut why = None;
    let mut plan = build_plan(w, discard, &mut why);
    if let Some(given) = discard.confirmed.as_deref()
        && !plan
            .steps
            .iter()
            .any(|s| matches!(s, Step::DiscardQueued { .. }))
    {
        let why = why.unwrap_or_else(|| unused_reason(w, &plan));
        word_unused(&mut plan, given, &why);
    }
    plan
}

fn build_plan(w: &Witness, discard: &DiscardInput, unused: &mut Option<String>) -> Plan {
    let mut plan = Plan {
        wiki: w.wiki.clone(),
        view: None,
        witnesses: Vec::new(),
        steps: Vec::new(),
        stop: None,
        notes: Vec::new(),
        discard_unused: None,
    };
    // --- What was read.
    let wit = &mut plan.witnesses;
    wit.push(match &w.flag {
        // The store has one reason for a flag whose act is not known, and a check that meets a damaged format marker
        // records it (M20): the line says so rather than let "interrupted" read as an act that never ended.
        Some(f) if w.format_marker.is_some() && f.reason == inklkunn_store::RecoveryReason::Interrupted => {
            "the wiki is flagged needs-recovery (interrupted: the reason the store gives a flag whose act is not known, which is also what a check records for a damaged format marker)".to_owned()
        }
        Some(f) => format!("the wiki is flagged needs-recovery ({})", f.reason.as_str()),
        None => "the wiki is not flagged".to_owned(),
    });
    if let Some(h) = &w.dead_holder {
        wit.push(format!(
            "the lease record named another holder ({}, for {}) that died holding it",
            h.holder, h.purpose
        ));
    }
    if w.interrupted_recoveries > 0 {
        wit.push(format!(
            "{} earlier recovery act(s) were begun and never ended",
            w.interrupted_recoveries
        ));
    }
    if let Some(format) = w.format_marker {
        wit.push(match format {
            MarkerError::Numbered(format) => format!(
                "prikk refuses the repository by its format marker (format {format}), which this prikk is not known to refuse: nothing else of prikk could be read"
            ),
            MarkerError::NotANumber => "prikk refuses the repository because its format marker holds something that is not a format number: nothing else of prikk could be read".to_owned(),
        });
        plan.stop = Some(Stop::FormatMarker);
        return plan;
    }
    if w.acts.is_empty() {
        wit.push("no act is unfinished".to_owned());
    }
    for a in &w.acts {
        wit.push(format!(
            "{} {} is unfinished (intent {}; the history says: {})",
            a.act.kind(),
            a.act.token(),
            if a.message.is_some() {
                if a.intent_open { "open" } else { "closed" }
            } else {
                "never written"
            },
            match &a.chain {
                Chain::Sealed { block_id, .. } => format!("sealed in block {block_id}"),
                Chain::NotSealed => "not sealed, the tip is the block it was made on".to_owned(),
                Chain::TipMoved => "not sealed, the tip has moved".to_owned(),
                Chain::Unreadable(_) => "unreadable".to_owned(),
                Chain::NoMessage => "no message to look for".to_owned(),
            }
        ));
    }
    for v in &w.view_acts {
        wit.push(format!(
            "view creation {} for {} never ended (prikk shows: {}; doctor: {})",
            v.act,
            v.view,
            match &v.shown {
                ViewShown::Absent => "the view is not among its views".to_owned(),
                ViewShown::Shown { tip } if *tip == v.from_block =>
                    "the view, at the block it was forked from".to_owned(),
                ViewShown::Shown { .. } => {
                    "the view, at another block than the one it was forked from".to_owned()
                }
                ViewShown::InAnotherCase => "a view of that name in another case".to_owned(),
                ViewShown::Unreadable(_) => "the view, whose block could not be read".to_owned(),
            },
            if lead_of(w, &v.view) {
                "its publication is unfinished"
            } else {
                "its publication is not named"
            }
        ));
    }
    let q = &w.status.queue;
    let refused = w.cleanup_pending.as_ref().is_some_and(|c| c.status_refused);
    wit.push(if refused {
        "prikk status refuses (an integrity error): the queue cannot be read".to_owned()
    } else {
        format!(
            "prikk status: {} patch(es) queued, {} torn byte(s) at the end of the log, view {}",
            q.count,
            w.status.trailing_partial_wal_bytes,
            w.status.current_branch.as_deref().unwrap_or("(none)")
        )
    });
    if w.cleanup_pending.is_some() {
        wit.push(
            "prikk doctor names the error PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING: a seal published its block and did not drain the queue"
                .to_owned(),
        );
    }
    wit.push(format!(
        "prikk doctor: {} error(s), {} warning(s){}",
        w.doctor.errors,
        w.doctor.warnings,
        if w.doctor.has_acknowledged_damage() {
            ", an acknowledged commit the log no longer holds soundly"
        } else {
            ""
        }
    ));
    wit.push(match &w.worktree {
        Some(s) if s.clean => "the worktree is the baseline".to_owned(),
        Some(s) => format!(
            "the worktree differs from its baseline ({} path(s), {} declared rename(s))",
            s.changes, s.declarations
        ),
        None if w.cleanup_pending.is_some() => {
            "the worktree was not compared with its baseline: prikk refuses to report it until the publication is finished"
                .to_owned()
        }
        None => "the worktree could not be compared with its baseline: the log is torn".to_owned(),
    });
    wit.push(format!("{} lock(s) held", w.locks.len()));
    if !w.foreign.is_empty() {
        wit.push(format!(
            "{} process(es) of somebody else's may be working in the repository",
            w.foreign.len()
        ));
    }
    if let Some(note) = &w.retirement {
        plan.notes.push(format!(
            "prikk does not verify the repository, and only because of a retired maintainer key (DR-10): {note}"
        ));
    }
    if w.doctor.diagnostics.iter().any(|d| d.is_metadata_debris()) {
        plan.notes
            .push("prikk's stale ref metadata; the next accepted change clears it".to_owned());
    }
    let unreached = w.doctor.unreached_notes();
    if unreached > 0 {
        plan.notes.push(format!(
            "doctor reports {unreached} unreached-object warning(s): recorded; they are not damage and do not block (ledger PL-12)"
        ));
    }

    // --- Stops that concern the repository as a whole.
    let stop = |plan: &mut Plan, why: Stop| {
        plan.steps.clear();
        plan.stop = Some(why);
    };
    plan.view = w
        .acts
        .first()
        .map(|a| a.view.clone())
        .or_else(|| w.view_acts.first().map(|v| v.view.clone()))
        .or_else(|| w.status.current_branch.clone());
    // A prikk or bwrap of somebody else's that may be working in the repository stops **every** step, not only the
    // clearing of a lock (P8): a seal, a restore or a discard beside it would be a second writer.
    if !w.foreign.is_empty() {
        stop(&mut plan, Stop::ForeignProcess);
        return plan;
    }
    if w.status.provisional_worktree.is_some() || w.status.interrupted_materialization.is_some() {
        stop(&mut plan, Stop::InterruptedState);
        return plan;
    }
    // A retirement that was begun and not finished (Q1): the retired key's blocks are reported by `doctor` and nothing
    // excuses them yet. The way out is the operator's command, which recovery names and never runs.
    if let Some(unfinished) = &w.retirement_unfinished
        && w.doctor
            .others()
            .iter()
            .any(|d| d.code == PUBLICATION_UNTRUSTED_CODE)
    {
        plan.notes.push(unfinished.clone());
        stop(&mut plan, Stop::RetirementUnfinished);
        return plan;
    }
    // A pending cleanup beside publication-trust findings that nothing excused (Q6): a named stop rather than a
    // generic one. (When the signer excuses them, `gather` has already taken them out of `doctor`.)
    if w.cleanup_pending.is_some()
        && w.doctor
            .others()
            .iter()
            .any(|d| d.code == PUBLICATION_UNTRUSTED_CODE)
    {
        stop(&mut plan, Stop::CleanupBesideTrustFindings);
        return plan;
    }
    // The debris warning is nobody's business (M16, PK-46), and the signer's two steps that ask for "exactly this one
    // finding" (a pending cleanup, an unfinished view creation) read it through the same rule as `others()`, so it is
    // never in their way either (M23): no stop for it here.
    // The one error a pending cleanup stands for is that row's own business (P15); anything beside it is trouble.
    // The one error an unfinished view creation stands for is the interrupted publication of that view's own ref (K4);
    // a lead for any other ref, or a second one, is trouble.
    let mut cleanup_error = usize::from(w.cleanup_pending.is_some());
    let mut view_lead = usize::from(!w.view_acts.is_empty());
    if w.doctor.others().iter().any(|d| {
        if d.is_cleanup_pending() && cleanup_error > 0 {
            cleanup_error -= 1;
            false
        } else if view_lead > 0
            && d.lead_ref()
                .is_some_and(|r| w.view_acts.iter().any(|v| v.view == r))
        {
            view_lead -= 1;
            false
        } else {
            true
        }
    }) {
        stop(&mut plan, Stop::DoctorTrouble);
        return plan;
    }
    if w.acts.len() + w.view_acts.len() > 1 {
        stop(&mut plan, Stop::SeveralActs);
        return plan;
    }
    if q.patches.len() > 1 {
        stop(&mut plan, Stop::SeveralQueued);
        return plan;
    }
    let mut steps: Vec<Step> = Vec::new();
    if !w.locks.is_empty() {
        if w.locks
            .iter()
            .any(|l| !ACCEPTANCE_LOCK_KINDS.contains(&l.kind.as_str()))
        {
            stop(&mut plan, Stop::UnknownLock);
            return plan;
        }
        steps.push(Step::ClearLocks {
            locks: w
                .locks
                .iter()
                .map(|l| LockEntry {
                    path: l.path.clone(),
                    kind: l.kind.clone(),
                    recorded_pid: l.recorded_pid,
                    prikk_liveness: l.liveness.clone(),
                })
                .collect(),
        });
    }
    let view = plan.view.clone();
    let restore = |steps: &mut Vec<Step>| {
        if w.worktree.is_none_or(|s| !s.clean)
            && let Some(view) = &view
        {
            steps.push(Step::RestoreWorktree { view: view.clone() });
        }
    };
    let act = w.acts.first();

    // --- The decision table: the queue first, then the act.
    match (
        w.view_acts.first(),
        w.cleanup_pending.as_ref(),
        q.patches.first(),
        act,
    ) {
        // A view's creation never ended (handoff 005 K4). It is the one unfinished act (`SeveralActs` is behind us),
        // and nothing may be queued, torn or half-sealed beside it. What prikk shows decides, and only these three
        // states are known: the view is there at the block it was forked from and nothing is unfinished (the act ended
        // *created*); the same with exactly its own ref's publication unfinished (prikk's own recommendation, `ref
        // complete`, then the same end); or the view is not there and nothing is unfinished (nothing happened:
        // *failed*). Anything else is a stop with its own word.
        (Some(v), c, patch, _) => {
            if c.is_some() || patch.is_some() || w.doctor.has_acknowledged_damage() {
                stop(&mut plan, Stop::ViewCreationUnclear);
                return plan;
            }
            let lead = lead_of(w, &v.view);
            match (&v.shown, lead) {
                (ViewShown::Shown { tip }, false) if *tip == v.from_block => {
                    steps.push(Step::EndViewCreated {
                        act: v.act.clone(),
                        view: v.view.clone(),
                    });
                }
                // Completing the creation appends a record signed by the maintainer key, and nothing new is signed
                // while a key retirement is unfinished (W1; K19): as for a queued patch, the plan says so first.
                (ViewShown::Shown { tip }, true)
                    if *tip == v.from_block && w.retirement_unfinished.is_some() =>
                {
                    if let Some(unfinished) = &w.retirement_unfinished {
                        plan.notes.push(unfinished.clone());
                    }
                    stop(&mut plan, Stop::RetirementUnfinished);
                    return plan;
                }
                (ViewShown::Shown { tip }, true) if *tip == v.from_block => {
                    steps.push(Step::CompleteViewCreation {
                        act: v.act.clone(),
                        view: v.view.clone(),
                        block_id: tip.clone(),
                    });
                    steps.push(Step::EndViewCreated {
                        act: v.act.clone(),
                        view: v.view.clone(),
                    });
                }
                (ViewShown::Absent, false) => {
                    steps.push(Step::EndViewFailed {
                        act: v.act.clone(),
                        view: v.view.clone(),
                    });
                }
                _ => {
                    stop(&mut plan, Stop::ViewCreationUnclear);
                    return plan;
                }
            }
            // `branch create` never touches the worktree, which belongs to the current branch, not to the new view.
            if w.worktree.is_none_or(|s| !s.clean)
                && let Some(current) = &w.status.current_branch
            {
                steps.push(Step::RestoreWorktree {
                    view: current.clone(),
                });
            }
        }
        // A `seal` published its block and stopped before it drained the queue (P15): prikk's own recommendation is
        // `seal` once more, taken only when the block published is the change of the one unfinished act.
        (None, Some(c), _, a) => {
            let published = c.published.as_ref();
            match a.zip(published).filter(|(a, p)| is_the_act(a, p)) {
                Some((a, p)) => {
                    steps.push(Step::CompletePublication {
                        act: a.act.clone(),
                        view: a.view.clone(),
                        block_id: p.block_id.clone(),
                    });
                    restore(&mut steps);
                    steps.push(Step::EndSealed {
                        act: a.act.clone(),
                        block_id: Some(p.block_id.clone()),
                    });
                }
                None => {
                    stop(&mut plan, Stop::PublishedBlockIsNotTheAct);
                    return plan;
                }
            }
        }
        // A patch is queued and is the act's, but a key retirement is unfinished: sealing makes a block, and nothing new
        // is sealed until the operator has finished or given up the retirement (W1). The signer would refuse the step
        // with the same words; the plan says so first.
        (None, None, Some(patch), Some(a))
            if w.retirement_unfinished.is_some()
                && matches_queued(a, patch, q.target_ref.as_deref())
                && a.chain == Chain::NotSealed
                && matches!(a.ended, Ended::Not) =>
        {
            if let Some(unfinished) = &w.retirement_unfinished {
                plan.notes.push(unfinished.clone());
            }
            stop(&mut plan, Stop::RetirementUnfinished);
            return plan;
        }
        // A patch is queued, and it is the change of the one unfinished act.
        (None, None, Some(patch), Some(a))
            if matches_queued(a, patch, q.target_ref.as_deref())
                && a.chain == Chain::NotSealed
                && matches!(a.ended, Ended::Not) =>
        {
            steps.push(Step::SealQueued {
                act: a.act.clone(),
                view: a.view.clone(),
                patch_id: patch.patch_id.clone(),
            });
            steps.push(Step::EndSealed {
                act: a.act.clone(),
                block_id: None,
            });
        }
        (None, None, Some(_), Some(_)) => {
            stop(&mut plan, Stop::QueuedPatchIsNotTheAct);
            return plan;
        }
        (None, None, Some(patch), None) => {
            // The one state a person may take a patch back from, and only by typing its id (PK-43): the offer is
            // prikk's own plan for exactly this patch, on the branch the worktree belongs to.
            let confirmed = match (&discard.offer, discard.confirmed.as_deref()) {
                (DiscardOffer::Offered(offer), Some(id))
                    if id == patch.patch_id
                        && offer.patch_id == patch.patch_id
                        && discard_honoured(w, q.target_ref.as_deref(), offer) =>
                {
                    Some(id.to_owned())
                }
                _ => None,
            };
            let Some(patch_id) = confirmed else {
                *unused = discard_notes(
                    &mut plan,
                    &patch.patch_id,
                    q.target_ref.as_deref(),
                    w,
                    discard,
                );
                // A stale lock beside the orphan patch is cleared all the same (M6) **when a preview follows**: it is
                // the one thing in the way of reading prikk's preview, and only a prikk that has `uncommit` has one
                // (M12). On a prikk without the verb, or whose answer could not be read, this stop clears nothing, as
                // every other stop; so does a plan that did not ask.
                let preview_follows = matches!(
                    discard.offer,
                    DiscardOffer::InARun | DiscardOffer::AfterLocks
                );
                let clearing: Vec<Step> = std::mem::take(&mut steps)
                    .into_iter()
                    .filter(|s| preview_follows && matches!(s, Step::ClearLocks { .. }))
                    .collect();
                if !preview_follows && !w.locks.is_empty() {
                    plan.notes.push(
                        "a stale lock is held, and recovery clears it here only when a preview of taking the patch back can follow (prikk 0.57.0 or later): nothing is cleared"
                            .to_owned(),
                    );
                }
                stop(&mut plan, Stop::ForeignQueuedPatch);
                plan.steps = clearing;
                return plan;
            };
            plan.notes.push(format!(
                "a person confirmed taking the queued patch {patch_id} back; prikk keeps it in its recovery log, and `prikk doctor --recovery-restore <run>` returns it (the run is printed when it is done)"
            ));
            steps.push(Step::DiscardQueued { patch_id });
            // `uncommit` leaves the files in the worktree as they are (the patch's result): put them back to the tip.
            if let Some(view) = &view {
                steps.push(Step::RestoreWorktree { view: view.clone() });
            }
        }
        // Nothing is queued, and prikk says an acknowledged commit is torn: discard it.
        (None, None, None, a) if w.doctor.has_acknowledged_damage() => {
            steps.push(Step::DiscardDamaged);
            restore(&mut steps);
            match a {
                Some(a) => match &a.chain {
                    Chain::Sealed { block_id, .. } => steps.push(Step::EndSealed {
                        act: a.act.clone(),
                        block_id: Some(block_id.clone()),
                    }),
                    Chain::NotSealed | Chain::NoMessage => steps.push(Step::EndFailed {
                        act: a.act.clone(),
                        discarded: true,
                    }),
                    Chain::TipMoved => {
                        stop(&mut plan, Stop::TipMoved);
                        return plan;
                    }
                    Chain::Unreadable(_) => {
                        stop(&mut plan, Stop::HistoryUnreadable);
                        return plan;
                    }
                },
                None => plan.notes.push(
                    "the discarded commit matches no recorded act; it is discarded and reported"
                        .to_owned(),
                ),
            }
        }
        // Nothing is queued and nothing is torn: the act, if there is one, is in the history or it is not.
        (None, None, None, Some(a)) => match &a.chain {
            Chain::Sealed { block_id, .. } => steps.push(Step::EndSealed {
                act: a.act.clone(),
                block_id: Some(block_id.clone()),
            }),
            Chain::NotSealed | Chain::NoMessage => {
                restore(&mut steps);
                steps.push(Step::EndFailed {
                    act: a.act.clone(),
                    discarded: false,
                });
            }
            Chain::TipMoved => {
                stop(&mut plan, Stop::TipMoved);
                return plan;
            }
            Chain::Unreadable(_) => {
                stop(&mut plan, Stop::HistoryUnreadable);
                return plan;
            }
        },
        (None, None, None, None) => restore(&mut steps),
    }

    // --- The tail every recovery has: the repository must verify, the records must agree, the flag goes.
    let something = !steps.is_empty()
        || w.flag.is_some()
        || w.dead_holder.is_some()
        || w.interrupted_recoveries > 0;
    if something {
        steps.push(Step::Verify);
        steps.push(Step::Reconcile);
        steps.push(Step::ClearFlag);
    }
    plan.steps = steps;
    plan
}

#[cfg(test)]
mod tests;
