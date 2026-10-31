//! The witnesses, and the plan made of them (RFC 004 D-7; handoff 004 T5). **Building a plan is a pure function** of
//! what was read ([`Witness`]): it does no I/O, so every row of its decision table is a unit test with no repository.
//!
//! A plan is either **steps**, in order, each one a thing the signer or the records can do and that has a name, or a
//! **stop**: a state recovery will not act on, with a fixed word, because acting would be guessing. Nothing is
//! rewritten. A sound queued patch that matches no recorded act is not discarded, because prikk has no verb that
//! removes one (`doctor --discard-damaged-commits` acts on an acknowledged commit the log no longer holds soundly,
//! and refuses over anything else): it is a stop, and a person decides.

use inklkunn_prikk::{RepoStatus, storage::QueuedPatch};
use inklkunn_signer::recover::text::{ACCEPTANCE_LOCK_KINDS, DoctorReport, HeldLock};
use inklkunn_signer::{RecoveryFacts, RecoveryOp, WorktreeState};
use inklkunn_store::{
    AcceptanceId, AccountId, LeaseHolder, ProposalId, ProposalState, RecoveryFlag, SystemActId,
    WikiId,
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
    /// `prikk status`.
    pub status: RepoStatus,
    /// `prikk doctor`.
    pub doctor: DoctorReport,
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
    /// More than one act is unfinished.
    SeveralActs,
    /// The view moved past the block the act was made on, and the act's change is not in the history.
    TipMoved,
    /// An interrupted checkout or materialization: prikk's own state, which a person decides about.
    InterruptedState,
    /// `doctor` reports something recovery has no answer for.
    DoctorTrouble,
    /// A lock of a kind an acceptance does not leave.
    UnknownLock,
    /// A `prikk` or `bwrap` of somebody else's may be working in the repository, so a lock is not cleared.
    ForeignProcess,
    /// The history could not be read as far as the question needs.
    HistoryUnreadable,
}

impl Stop {
    /// The fixed word.
    pub const fn as_str(self) -> &'static str {
        match self {
            Stop::ForeignQueuedPatch => "foreign-queued-patch",
            Stop::QueuedPatchIsNotTheAct => "queued-patch-is-not-the-act",
            Stop::SeveralQueued => "several-queued",
            Stop::SeveralActs => "several-acts",
            Stop::TipMoved => "tip-moved",
            Stop::InterruptedState => "interrupted-state",
            Stop::DoctorTrouble => "doctor-trouble",
            Stop::UnknownLock => "unknown-lock",
            Stop::ForeignProcess => "foreign-process",
            Stop::HistoryUnreadable => "history-unreadable",
        }
    }

    /// What the stop means, for the operator who has to decide.
    pub const fn meaning(self) -> &'static str {
        match self {
            Stop::ForeignQueuedPatch => {
                "a patch is queued that no recorded act describes; prikk has no verb that removes a sound queued patch, and sealing it would put an unaccepted change in the history"
            }
            Stop::QueuedPatchIsNotTheAct => {
                "a patch is queued that is not the change of the recorded act (its message, its base or its files differ)"
            }
            Stop::SeveralQueued => "more than one patch is queued",
            Stop::SeveralActs => "more than one act is unfinished",
            Stop::TipMoved => {
                "the view has moved past the block the unfinished act was made on, and the act's change is not in the history"
            }
            Stop::InterruptedState => "prikk reports an interrupted checkout or materialization",
            Stop::DoctorTrouble => "prikk's doctor reports damage that recovery has no answer for",
            Stop::UnknownLock => "a lock is held of a kind an acceptance does not leave",
            Stop::ForeignProcess => {
                "a prikk or bwrap of somebody else's may be working in the repository"
            }
            Stop::HistoryUnreadable => "the history could not be read as far as recovery needs",
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
    /// `doctor --discard-damaged-commits`.
    DiscardDamaged,
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
            Step::DiscardDamaged => "discard-damaged",
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
            Step::DiscardDamaged => {
                "run doctor --discard-damaged-commits: remove an acknowledged commit the log no longer holds soundly (prikk saves the bytes first)".to_owned()
            }
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
                Step::DiscardDamaged => Some(RecoveryOp::DiscardDamaged),
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
                Step::DiscardDamaged => json!({"step": "discard-damaged"}),
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
            (Some(stop), _) => {
                out.push(format!(
                    "  STOP ({}): {}; recovery changes nothing and the wiki stays flagged for a person to decide",
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
            "discard-damaged" => ops.push(RecoveryOp::DiscardDamaged),
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

fn matches_queued(act: &ActWitness, patch: &QueuedPatch, target: Option<&str>) -> bool {
    let digest = crate::acceptance::message_digest(&patch.message);
    act.message_digest.as_deref() == Some(digest.as_str()) && target == Some(act.view.as_str())
}

/// Make the plan.
pub fn build(w: &Witness) -> Plan {
    let mut plan = Plan {
        wiki: w.wiki.clone(),
        view: None,
        witnesses: Vec::new(),
        steps: Vec::new(),
        stop: None,
        notes: Vec::new(),
    };
    // --- What was read.
    let wit = &mut plan.witnesses;
    wit.push(match &w.flag {
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
    let q = &w.status.queue;
    wit.push(format!(
        "prikk status: {} patch(es) queued, {} torn byte(s) at the end of the log, view {}",
        q.count,
        w.status.trailing_partial_wal_bytes,
        w.status.current_branch.as_deref().unwrap_or("(none)")
    ));
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
        None => "the worktree could not be compared with its baseline: the log is torn".to_owned(),
    });
    wit.push(format!("{} lock(s) held", w.locks.len()));
    if !w.foreign.is_empty() {
        wit.push(format!(
            "{} process(es) of somebody else's may be working in the repository",
            w.foreign.len()
        ));
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
        .or_else(|| w.status.current_branch.clone());
    if w.status.provisional_worktree.is_some() || w.status.interrupted_materialization.is_some() {
        stop(&mut plan, Stop::InterruptedState);
        return plan;
    }
    if !w.doctor.others().is_empty() {
        stop(&mut plan, Stop::DoctorTrouble);
        return plan;
    }
    if w.acts.len() > 1 {
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
        if !w.foreign.is_empty() {
            stop(&mut plan, Stop::ForeignProcess);
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
    match (q.patches.first(), act) {
        // A patch is queued, and it is the change of the one unfinished act.
        (Some(patch), Some(a))
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
        (Some(_), Some(_)) => {
            stop(&mut plan, Stop::QueuedPatchIsNotTheAct);
            return plan;
        }
        (Some(_), None) => {
            stop(&mut plan, Stop::ForeignQueuedPatch);
            return plan;
        }
        // Nothing is queued, and prikk says an acknowledged commit is torn: discard it.
        (None, a) if w.doctor.has_acknowledged_damage() => {
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
        (None, Some(a)) => match &a.chain {
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
        (None, None) => restore(&mut steps),
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
