//! Recovery (RFC 004 D-7; internal design CON-4, WR-8; external design FL-10; handoff 004 T5): **no half-state
//! survives a crash, and nothing is guessed.**
//!
//! [`Recovery::recover`] runs at start and on demand, per wiki that needs it, **under the wiki's write lease**:
//!
//! 1. *Read the witnesses* ([`plan::Witness`]): the wiki's flag, the open intents, the acceptances and system acts
//!    that never ended, the lease record of a holder that died, `prikk status` (the queue, the torn bytes), `prikk
//!    doctor`, the locks that are held, the history, the worktree, and the processes of somebody else's that may be
//!    working in the repository.
//! 2. *Make the plan* ([`plan::build`], a pure function): either steps in order, or a **stop** with a fixed word.
//!    The plan is shown (`--plan` shows it and does nothing else) and, before any step is taken, **recorded** as a
//!    recovery act ([`RecoveryActRecord`]), which is what the signer's recovery operations are bound to.
//! 3. *Take the steps*, each preceded by an audit record: stale locks of the kinds an acceptance leaves are cleared
//!    on the lease record's word and never prikk's liveness line (ledger PL-8); a queued patch that is the change of
//!    the unfinished act is **completed** with `seal` alone (never committed again); a commit prikk says is torn is
//!    **discarded** with `doctor --discard-damaged-commits`; a worktree that is not the baseline is put back; the act's
//!    records are ended as what happened; `verify`; the reconciliation (RFC 002 D-7); the flag is cleared.
//!
//! A queued patch that no recorded act describes is **not** discarded: prikk has no verb that removes a sound queued
//! patch, and sealing it would put an unaccepted change in the history. It is a stop, and a person decides. Nothing
//! is rewritten; the history only grows by the one patch the act meant to seal.
//!
//! `doctor`'s unreached-object warnings are recorded and do not block (ledger PL-12).

use std::path::Path;
use std::sync::Arc;

use inklkunn_prikk::{Point, Storage};
use inklkunn_signer::{AcceptanceToken, Recover, RecoveryToken, Witnesses};
use inklkunn_store::{
    AcceptanceOutcome, AcceptanceResult, AcceptanceStore, ActStore, AuditEvent, AuditOutcome,
    AuditSink, IntentCompletion, IntentEnd, IntentId, LeaseHolder, ProposalChange, ProposalOp,
    ProposalState, ProposalStore, ProposalTransition, RecoveryActEnd, RecoveryActId,
    RecoveryActRecord, RecoveryEnding, RecoveryFlag, RecoveryReason, RootLayout, StoreError,
    SystemActEnd, WikiId, WikiRecord,
};

use crate::acceptance::{AfterSeal, NoFollowUp, Reconciled, message_digest, shared, words};
use crate::lease::{Busy, LeaseError, Purpose, WikiLeases};
use crate::notify::Notifier;
use crate::proposal::machine;
use crate::read::ReadService;

pub mod plan;
pub mod processes;

pub use plan::{
    ActRef, ActWitness, Chain, Ended, LockEntry, Plan, Step, Stop, Witness, build, facts_of,
};
pub use processes::{Foreign, ProcFs, ProcessInfo, ProcessTable};

/// Who recovery is in the audit trail: the service, never a person.
pub const PRINCIPAL: &str = "system:recovery";

/// How many blocks of a view are read to find an act's change: more than any wiki makes between the block an act was
/// made on and the tip, while the wiki is flagged and refusing writes.
const HISTORY_WINDOW: u64 = 1_000;

/// Whether recovery acts or only shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Read the witnesses and show the plan. Takes no lease, writes no record, changes nothing.
    PlanOnly,
    /// Take the lease, record the plan, take the steps.
    Run,
}

/// Why recovery could not even begin or be recorded. (A recovery that began and stopped is a [`Outcome::Stopped`],
/// not an error.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryError {
    /// The wiki's write lease is not free: another act holds it. Nothing was done. Recovery never runs without it.
    Busy(Busy),
    /// The lease could not be taken at all (the text is for operators).
    Lease(String),
    /// A witness could not be read (the text is for operators): nothing is concluded from what could not be seen.
    Witness(String),
    /// The audit trail could not take the record that must come first: nothing was done.
    Audit(String),
    /// A store failed.
    Store(StoreError),
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryError::Busy(b) => write!(
                f,
                "the wiki is busy ({} ahead, about {} s): recovery runs only under the write lease; nothing was done",
                b.depth,
                b.estimate.as_secs()
            ),
            RecoveryError::Lease(why) => write!(f, "the write lease could not be taken: {why}"),
            RecoveryError::Witness(why) => {
                write!(
                    f,
                    "a witness could not be read, so nothing is concluded: {why}"
                )
            }
            RecoveryError::Audit(why) => write!(
                f,
                "the audit trail could not be written ({why}); nothing was done"
            ),
            RecoveryError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RecoveryError {}

impl From<StoreError> for RecoveryError {
    fn from(e: StoreError) -> Self {
        RecoveryError::Store(e)
    }
}

/// How a recovery ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing needed recovering.
    NothingToDo,
    /// `--plan`: the plan was shown and nothing was done.
    Planned,
    /// Every step was taken and the wiki is cleared to take writes again.
    Recovered,
    /// A person decides: the plan stopped, or a step was refused or failed. The wiki stays flagged and nothing was
    /// guessed. `at` is the step, when a step failed.
    Stopped {
        /// The step that failed, if one did.
        at: Option<&'static str>,
        /// Why (operator text; names no path of the repository).
        why: String,
    },
}

/// One step that was taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepDone {
    /// The step's name.
    pub step: &'static str,
    /// What it did, in a few words.
    pub detail: String,
}

/// What a recovery did.
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    /// The wiki.
    pub wiki: WikiId,
    /// The plan (shown before; recorded unless this was only a plan).
    pub plan: Plan,
    /// The recorded recovery act, when one was recorded.
    pub act: Option<RecoveryActId>,
    /// How it ended.
    pub outcome: Outcome,
    /// The steps that were taken, in order.
    pub done: Vec<StepDone>,
    /// What the reconciliation said, when it ran.
    pub reconciliation: Option<Reconciled>,
    /// The lease was held for the length of the run (false for a plan only).
    pub under_lease: bool,
}

impl RecoveryReport {
    /// The wiki is clear to take writes again (or never needed recovery).
    pub fn is_clear(&self) -> bool {
        matches!(self.outcome, Outcome::Recovered | Outcome::NothingToDo)
    }

    /// The report as lines for a person: the plan before, the record after.
    pub fn lines(&self) -> Vec<String> {
        let mut out = self.plan.lines();
        if let Some(act) = &self.act {
            out.push(format!("  recorded as recovery act {act}"));
        }
        for d in &self.done {
            out.push(format!("  done: {}: {}", d.step, d.detail));
        }
        match &self.reconciliation {
            Some(Reconciled::Clean) => {
                out.push("  the history and the records agree".to_owned());
            }
            Some(Reconciled::Findings(f)) => out.push(format!(
                "  the reconciliation reports {} finding(s), written to the audit trail: a person looks at them",
                f.len()
            )),
            Some(Reconciled::NotRun(why)) => {
                out.push(format!("  the reconciliation could not be run: {why}"));
            }
            Some(Reconciled::Skipped) | None => {}
        }
        out.push(match &self.outcome {
            Outcome::NothingToDo => "nothing needed recovering".to_owned(),
            Outcome::Planned => "plan only: nothing was done".to_owned(),
            Outcome::Recovered => "recovered: the wiki takes writes again".to_owned(),
            Outcome::Stopped { at, why } => format!(
                "stopped{}: {why}; the wiki stays flagged and nothing was guessed",
                at.map(|s| format!(" at {s}")).unwrap_or_default()
            ),
        });
        out
    }
}

/// Everything recovery is made of.
#[derive(Clone)]
pub struct RecoveryParts {
    /// The acceptance, intent and lease-holder records.
    pub acceptances: Arc<dyn AcceptanceStore>,
    /// The flag, the system acts and the recovery acts.
    pub acts: Arc<dyn ActStore>,
    /// The proposals (an accepted one is recorded as accepted).
    pub proposals: Arc<dyn ProposalStore>,
    /// prikk, read-only: the history, `status`, `verify`.
    pub storage: Arc<dyn Storage>,
    /// `doctor`, the lock listing and the worktree's state.
    pub witnesses: Arc<Witnesses>,
    /// The signer's recovery operations. Its leases must be the ones [`RecoveryParts::leases`] is built on.
    pub signer: Arc<dyn Recover>,
    /// The write lease.
    pub leases: Arc<WikiLeases>,
    /// Where the wikis are.
    pub layout: RootLayout,
    /// The audit trail: a step is recorded before it is taken.
    pub audit: Arc<dyn AuditSink>,
}

/// Recovery.
pub struct Recovery {
    parts: RecoveryParts,
    reads: Option<Arc<ReadService>>,
    notifier: Option<Arc<Notifier>>,
    follow_up: Arc<dyn AfterSeal>,
    processes: Arc<dyn ProcessTable>,
    clock: fn() -> i64,
}

impl std::fmt::Debug for Recovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Recovery")
    }
}

fn witness_error(what: &str, e: impl std::fmt::Display) -> RecoveryError {
    RecoveryError::Witness(format!("{what}: {e}"))
}

impl Recovery {
    /// Recovery over these parts.
    pub fn new(parts: RecoveryParts) -> Self {
        Recovery {
            parts,
            reads: None,
            notifier: None,
            follow_up: Arc::new(NoFollowUp),
            processes: Arc::new(ProcFs),
            clock: inklkunn_store::record::now_secs,
        }
    }

    /// Forget this read service's caches of the wiki after a change recovery completed (a process that serves
    /// pages has one; the command line has none).
    #[must_use]
    pub fn with_reads(mut self, reads: Arc<ReadService>) -> Self {
        self.reads = Some(reads);
        self
    }

    /// Tell the proposer of an acceptance that recovery completed that it was accepted. Best effort.
    #[must_use]
    pub fn with_notifier(mut self, notifier: Arc<Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// Ask `follow_up` for the title index and the verification after a change recovery completed, off the lease.
    #[must_use]
    pub fn with_follow_up(mut self, follow_up: Arc<dyn AfterSeal>) -> Self {
        self.follow_up = follow_up;
        self
    }

    /// Look for foreign processes in `table` instead of `/proc` (tests put a process on it).
    #[must_use]
    pub fn with_processes(mut self, table: Arc<dyn ProcessTable>) -> Self {
        self.processes = table;
        self
    }

    /// The same, reading the time from `clock` (Unix seconds).
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    // --- does a wiki need it?

    /// Why the wiki needs recovery, as fixed words; empty when it does not. Cheap: a few queries, no prikk. A record
    /// of a lease holder that is not this process is a holder that died (a live one holds the lock, and the lease
    /// cannot be taken); an export's or a refresh's leaves nothing in the repository and is not a reason.
    pub fn needs_recovery(&self, wiki: &WikiId) -> Result<Vec<&'static str>, RecoveryError> {
        let p = &self.parts;
        Ok(needs_recovery(
            &*p.acceptances,
            &*p.acts,
            wiki,
            Some(p.leases.holder_name()),
        )?)
    }

    fn reasons(
        &self,
        wiki: &WikiId,
        dead: Option<&LeaseHolder>,
    ) -> Result<Vec<&'static str>, StoreError> {
        reasons_of(&*self.parts.acceptances, &*self.parts.acts, wiki, dead)
    }

    // --- the witnesses

    /// What the history says about an act's change (see [`Chain`]).
    fn chain_of(&self, repo: &Path, a: &ActWitness) -> Chain {
        match &a.ended {
            Ended::Sealed { block_id, patch_id } => {
                return Chain::Sealed {
                    block_id: block_id.clone(),
                    patch_id: patch_id.clone(),
                };
            }
            Ended::Failed => return Chain::NoMessage,
            Ended::Not => {}
        }
        let (Some(base), Some(digest)) = (&a.base_block, &a.message_digest) else {
            return Chain::NoMessage;
        };
        let history = match self.parts.storage.history(
            repo,
            &Point::View(a.view.clone()),
            Some(HISTORY_WINDOW),
        ) {
            Ok(h) => h,
            Err(e) => return Chain::Unreadable(e.to_string()),
        };
        let Some(at) = history.blocks.iter().position(|b| &b.block_id == base) else {
            return Chain::Unreadable(
                "the block the act was made on is not in the part of the history that was read"
                    .to_owned(),
            );
        };
        for block in history.blocks.iter().take(at) {
            if let Some(m) = block
                .patch_messages
                .iter()
                .find(|m| &message_digest(&m.message) == digest)
            {
                return Chain::Sealed {
                    block_id: block.block_id.clone(),
                    patch_id: m.patch_id.clone(),
                };
            }
        }
        if at == 0 {
            Chain::NotSealed
        } else {
            Chain::TipMoved
        }
    }

    /// An act as its records and the history say it is now.
    fn act_witness(
        &self,
        wiki: &WikiRecord,
        act: &ActRef,
    ) -> Result<(ActWitness, Option<IntentId>), RecoveryError> {
        let p = &self.parts;
        let repo = p.layout.repo_dir(&wiki.id);
        let mut w = match act {
            ActRef::Acceptance(id) => {
                let entry = p
                    .acceptances
                    .acceptance_entry(id)?
                    .ok_or(RecoveryError::Store(StoreError::NotFound))?;
                let rec = entry.acceptance;
                let intent = p
                    .acceptances
                    .intents(&wiki.id)?
                    .into_iter()
                    .find(|i| i.intent.acceptance == *id);
                let ended = match entry.result.map(|r| r.outcome) {
                    None => Ended::Not,
                    Some(AcceptanceOutcome::Sealed { block_id, patch_id }) => {
                        Ended::Sealed { block_id, patch_id }
                    }
                    Some(AcceptanceOutcome::Failed { .. }) => Ended::Failed,
                };
                let proposal_state = p
                    .proposals
                    .proposal(&rec.proposal, None)?
                    .map(|record| record.state);
                let (message, digest, base, open, intent_id) = match &intent {
                    Some(i) => (
                        Some(i.intent.message.clone()).filter(|m| !m.is_empty()),
                        Some(i.intent.message_digest.clone()),
                        Some(i.intent.base_block.clone()),
                        i.completion.is_none(),
                        Some(i.intent.id.clone()),
                    ),
                    None => (None, None, None, false, None),
                };
                let w = ActWitness {
                    act: act.clone(),
                    view: rec.view.clone(),
                    base_block: base,
                    message,
                    message_digest: digest,
                    ended,
                    intent_open: open,
                    proposal: Some(rec.proposal.clone()),
                    proposal_state,
                    acceptor: Some(rec.acceptor.clone()),
                    chain: Chain::NoMessage,
                };
                (w, intent_id)
            }
            ActRef::System(id) => {
                let entry = p
                    .acts
                    .system_acts(&wiki.id)?
                    .into_iter()
                    .find(|e| e.act.id == *id)
                    .ok_or(RecoveryError::Store(StoreError::NotFound))?;
                let ended = match entry.end.map(|e| e.outcome) {
                    None => Ended::Not,
                    Some(AcceptanceOutcome::Sealed { block_id, patch_id }) => {
                        Ended::Sealed { block_id, patch_id }
                    }
                    Some(AcceptanceOutcome::Failed { .. }) => Ended::Failed,
                };
                let w = ActWitness {
                    act: act.clone(),
                    view: entry.act.view.clone(),
                    base_block: Some(entry.act.base_block.clone()),
                    message: Some(entry.act.message.clone()),
                    message_digest: Some(entry.act.message_digest.clone()),
                    ended,
                    intent_open: false,
                    proposal: None,
                    proposal_state: None,
                    acceptor: None,
                    chain: Chain::NoMessage,
                };
                (w, None)
            }
        };
        w.0.chain = self.chain_of(&repo, &w.0);
        Ok(w)
    }

    /// The acts that are not finished: an open intent, an acceptance with no end (intent or not), a system act with
    /// no end, and the act a dead lease holder was holding the lease for when that act's records are incomplete.
    fn unfinished_acts(
        &self,
        wiki: &WikiRecord,
        dead: Option<&LeaseHolder>,
    ) -> Result<Vec<ActWitness>, RecoveryError> {
        let p = &self.parts;
        let mut sources: Vec<ActRef> = Vec::new();
        let mut add = |a: ActRef| {
            if !sources.contains(&a) {
                sources.push(a);
            }
        };
        for i in p.acceptances.open_intents(&wiki.id)? {
            add(ActRef::Acceptance(i.intent.acceptance));
        }
        for a in p.acceptances.pending_acceptances(&wiki.id)? {
            add(ActRef::Acceptance(a.id));
        }
        for e in p.acts.system_acts(&wiki.id)? {
            if e.end.is_none() {
                add(ActRef::System(e.act.id));
            }
        }
        if let Some(a) = dead.and_then(|h| h.acceptance.clone()) {
            add(ActRef::Acceptance(a));
        }
        let mut out = Vec::new();
        for src in sources {
            let (w, _) = self.act_witness(wiki, &src)?;
            let (_, _, proposal_open) = w.needs_records();
            let unfinished = matches!(w.ended, Ended::Not)
                || w.intent_open
                || (matches!(w.ended, Ended::Sealed { .. }) && proposal_open);
            if unfinished {
                out.push(w);
            }
        }
        Ok(out)
    }

    fn gather(
        &self,
        wiki: &WikiRecord,
        dead: Option<LeaseHolder>,
        lease_held: bool,
    ) -> Result<Witness, RecoveryError> {
        let p = &self.parts;
        let repo = p.layout.repo_dir(&wiki.id);
        let dead = dead.filter(|h| {
            h.purpose != Purpose::Export.as_str() && h.purpose != Purpose::Refresh.as_str()
        });
        let acts = self.unfinished_acts(wiki, dead.as_ref())?;
        let status = p
            .storage
            .status(&repo)
            .map_err(|e| witness_error("prikk status", e))?;
        let torn = status.trailing_partial_wal_bytes > 0;
        let table = self.processes.processes();
        Ok(Witness {
            wiki: wiki.id.clone(),
            flag: p.acts.recovery_flag(&wiki.id)?,
            dead_holder: dead,
            interrupted_recoveries: p
                .acts
                .recovery_acts(&wiki.id)?
                .iter()
                .filter(|e| e.end.is_none())
                .count(),
            acts,
            status,
            doctor: p
                .witnesses
                .doctor(&wiki.id)
                .map_err(|e| witness_error("prikk doctor", e))?,
            worktree: if torn {
                // prikk refuses to compare a worktree over a torn log; the plan puts it back after the discard.
                None
            } else {
                Some(
                    p.witnesses
                        .worktree(&wiki.id)
                        .map_err(|e| witness_error("prikk worktree-status", e))?,
                )
            },
            locks: p
                .witnesses
                .locks(&wiki.id)
                .map_err(|e| witness_error("prikk unlock", e))?,
            foreign: processes::foreign(&table, &repo, std::process::id()),
            lease_held,
        })
    }

    // --- the run

    fn event(
        &self,
        action: &str,
        resource: &str,
        outcome: AuditOutcome,
        detail: &str,
    ) -> AuditEvent {
        AuditEvent::new((self.clock)(), PRINCIPAL, action, resource, outcome, detail)
    }

    /// Record an event; an error is the trail's refusal, which stops the recovery before the step it precedes.
    fn trail(
        &self,
        action: &str,
        resource: &str,
        outcome: AuditOutcome,
        detail: &str,
    ) -> Result<(), RecoveryError> {
        self.parts
            .audit
            .record(&self.event(action, resource, outcome, detail))
            .map_err(|e| RecoveryError::Audit(e.to_string()))
    }

    /// Recover `wiki`, or show how it would be recovered (see [`Mode`]).
    pub fn recover(&self, wiki: &WikiRecord, mode: Mode) -> Result<RecoveryReport, RecoveryError> {
        let p = &self.parts;
        if mode == Mode::PlanOnly {
            let dead = p
                .acceptances
                .lease_holder(&wiki.id)?
                .filter(|h| h.holder != p.leases.holder_name());
            let witness = self.gather(wiki, dead, false)?;
            let plan = build(&witness);
            return Ok(RecoveryReport {
                wiki: wiki.id.clone(),
                plan,
                act: None,
                outcome: Outcome::Planned,
                done: Vec::new(),
                reconciliation: None,
                under_lease: false,
            });
        }

        // 1. The lease: recovery is a writer, and never runs without it.
        let lease = p
            .leases
            .acquire(&wiki.id, Purpose::Recovery, None)
            .map_err(|e| match e {
                LeaseError::Busy(b) => RecoveryError::Busy(b),
                LeaseError::Unavailable(why) => RecoveryError::Lease(why),
            })?;
        let scope = lease.scope();
        let dead = lease.replaced().cloned();
        let reasons = self.reasons(&wiki.id, dead.as_ref())?;
        if reasons.is_empty() {
            return Ok(RecoveryReport {
                wiki: wiki.id.clone(),
                plan: build_empty(&wiki.id),
                act: None,
                outcome: Outcome::NothingToDo,
                done: Vec::new(),
                reconciliation: None,
                under_lease: true,
            });
        }

        // 2. The witnesses and the plan. The wiki refuses writes from here until recovery clears the flag, whatever
        //    becomes of this run.
        let resource = format!("wiki:{}", wiki.id);
        self.trail(
            "recovery-begin",
            &resource,
            AuditOutcome::Allowed,
            &reasons.join(","),
        )?;
        if p.acts.recovery_flag(&wiki.id)?.is_none() {
            p.acts.set_recovery_flag(&RecoveryFlag {
                wiki: wiki.id.clone(),
                reason: RecoveryReason::Interrupted,
                act: dead
                    .as_ref()
                    .and_then(|h| h.acceptance.as_ref().map(ToString::to_string)),
                since: (self.clock)(),
            })?;
        }
        let witness = match self.gather(wiki, dead, true) {
            Ok(w) => w,
            Err(e) => {
                // Nothing is concluded from what could not be read; the flag stays, and the trail says it ended.
                let _ = self.trail(
                    "recovery",
                    &resource,
                    AuditOutcome::Failed,
                    "a witness could not be read",
                );
                return Err(e);
            }
        };
        let plan = build(&witness);

        // 3. The plan, recorded before any of it is done; an earlier recovery that was itself interrupted is ended.
        let act = RecoveryActId::new();
        for e in p.acts.recovery_acts(&wiki.id)? {
            if e.end.is_none() {
                p.acts.finish_recovery_act(&RecoveryActEnd {
                    act: e.act.id,
                    ending: RecoveryEnding::Stopped,
                    at: (self.clock)(),
                })?;
            }
        }
        let resource = format!("wiki:{}:{act}", wiki.id);
        self.trail(
            "recovery-plan",
            &resource,
            AuditOutcome::Allowed,
            &match plan.stop {
                Some(stop) => format!("stop: {}", stop.as_str()),
                None => format!("{} step(s)", plan.steps.len()),
            },
        )?;
        p.acts.begin_recovery_act(&RecoveryActRecord {
            id: act.clone(),
            wiki: wiki.id.clone(),
            plan: plan.to_json(),
            created_at: (self.clock)(),
        })?;
        let unreached = witness.doctor.unreached_notes();
        if unreached > 0 {
            self.trail(
                "recovery-note",
                &resource,
                AuditOutcome::Reported,
                &format!("unreached objects: {unreached} (not damage)"),
            )?;
        }

        let mut report = RecoveryReport {
            wiki: wiki.id.clone(),
            plan: plan.clone(),
            act: Some(act.clone()),
            outcome: Outcome::Recovered,
            done: Vec::new(),
            reconciliation: None,
            under_lease: true,
        };
        let finish = |ending: RecoveryEnding| -> Result<(), RecoveryError> {
            p.acts.finish_recovery_act(&RecoveryActEnd {
                act: act.clone(),
                ending,
                at: (self.clock)(),
            })?;
            Ok(())
        };
        if let Some(stop) = plan.stop {
            report.outcome = Outcome::Stopped {
                at: None,
                why: stop.meaning().to_owned(),
            };
            self.trail(
                "recovery",
                &resource,
                AuditOutcome::Failed,
                &format!("stopped: {}", stop.as_str()),
            )?;
            finish(RecoveryEnding::Stopped)?;
            return Ok(report);
        }

        // 4. The steps, each one recorded before it is taken.
        let token = RecoveryToken::new(act.to_string());
        let mut sealed_something = false;
        for step in &plan.steps {
            self.trail(
                "recovery-step",
                &resource,
                AuditOutcome::Allowed,
                step.name(),
            )?;
            match self.run_step(wiki, &token, step, &mut report.reconciliation) {
                Ok(detail) => {
                    sealed_something |= matches!(step, Step::EndSealed { .. });
                    report.done.push(StepDone {
                        step: step.name(),
                        detail,
                    });
                }
                Err(why) => {
                    self.trail(
                        "recovery-step",
                        &resource,
                        AuditOutcome::Failed,
                        &format!("{} failed", step.name()),
                    )?;
                    report.outcome = Outcome::Stopped {
                        at: Some(step.name()),
                        why,
                    };
                    self.trail("recovery", &resource, AuditOutcome::Failed, "stopped")?;
                    finish(RecoveryEnding::Stopped)?;
                    return Ok(report);
                }
            }
        }
        self.trail("recovery", &resource, AuditOutcome::Allowed, "recovered")?;
        finish(RecoveryEnding::Recovered)?;
        drop(scope);
        drop(lease);
        if sealed_something {
            self.follow_up.warm_title_index(wiki);
            self.follow_up.enqueue_verification(wiki);
        }
        Ok(report)
    }

    /// Take one step. The answer is what it did, or why it could not (operator text).
    fn run_step(
        &self,
        wiki: &WikiRecord,
        token: &RecoveryToken,
        step: &Step,
        reconciliation: &mut Option<Reconciled>,
    ) -> Result<String, String> {
        let p = &self.parts;
        let repo = p.layout.repo_dir(&wiki.id);
        match step {
            Step::ClearLocks { locks } => {
                let paths: Vec<String> = locks.iter().map(|l| l.path.clone()).collect();
                let cleared = p
                    .signer
                    .clear_locks(&wiki.id, token, &paths)
                    .map_err(|e| e.to_string())?;
                let left = p.witnesses.locks(&wiki.id).map_err(|e| e.to_string())?;
                if left.iter().any(|l| paths.contains(&l.path)) {
                    return Err("a lock is still held after it was cleared".to_owned());
                }
                Ok(format!("{} lock(s) cleared", cleared.len()))
            }
            Step::SealQueued {
                act,
                view,
                patch_id,
            } => {
                p.signer
                    .seal_queued(&wiki.id, view, token, &AcceptanceToken::new(act.token()))
                    .map_err(|e| e.to_string())?;
                Ok(format!("patch {patch_id} sealed"))
            }
            Step::DiscardDamaged => {
                let d = p
                    .signer
                    .discard_damaged(&wiki.id, token)
                    .map_err(|e| e.to_string())?;
                Ok(format!(
                    "damaged commit discarded (patch {}, prikk's recovery run {})",
                    d.patch_id.as_deref().unwrap_or("not named"),
                    d.recovery_run.as_deref().unwrap_or("not named")
                ))
            }
            Step::RestoreWorktree { view } => {
                let n = p
                    .signer
                    .restore_worktree(&wiki.id, view, token)
                    .map_err(|e| e.to_string())?;
                Ok(format!("{n} path(s) put back"))
            }
            Step::EndFailed { act, discarded } => self.end_failed(wiki, act, *discarded),
            Step::EndSealed { act, .. } => self.end_sealed(wiki, act),
            Step::Verify => {
                let v = p.storage.verify(&repo).map_err(|e| e.to_string())?;
                if v.verdict.ok {
                    Ok("the repository verifies".to_owned())
                } else {
                    Err("prikk verify reports failed conditions; the flag is kept".to_owned())
                }
            }
            Step::Reconcile => {
                let r = shared::reconcile_wiki(
                    &*p.storage,
                    &p.layout,
                    wiki,
                    &p.acceptances,
                    &p.acts,
                    Some(&p.audit),
                    (self.clock)(),
                );
                let text = match &r {
                    Reconciled::Clean => Ok("the history and the records agree".to_owned()),
                    Reconciled::Findings(f) => Ok(format!(
                        "{} finding(s) reported to the audit trail",
                        f.len()
                    )),
                    Reconciled::NotRun(why) => {
                        Err(format!("the reconciliation could not be run: {why}"))
                    }
                    Reconciled::Skipped => Ok("skipped".to_owned()),
                };
                *reconciliation = Some(r);
                text
            }
            Step::ClearFlag => {
                let had = p
                    .acts
                    .clear_recovery_flag(&wiki.id)
                    .map_err(|e| e.to_string())?;
                Ok(if had {
                    "flag cleared"
                } else {
                    "there was no flag"
                }
                .to_owned())
            }
        }
    }

    /// Record an act that put nothing in the history as failed (`interrupted`), and end its intent. Refused if the
    /// history now has the act's change: a sealed act is never recorded as failed.
    fn end_failed(
        &self,
        wiki: &WikiRecord,
        act: &ActRef,
        discarded: bool,
    ) -> Result<String, String> {
        let p = &self.parts;
        let (w, intent) = self.act_witness(wiki, act).map_err(|e| e.to_string())?;
        if matches!(w.chain, Chain::Sealed { .. }) && !matches!(w.ended, Ended::Failed) {
            return Err(
                "the act's change is in the history; it is not recorded as failed".to_owned(),
            );
        }
        let at = (self.clock)();
        let mut did = Vec::new();
        match act {
            ActRef::Acceptance(id) => {
                if matches!(w.ended, Ended::Not) {
                    p.acceptances
                        .finish_acceptance(&AcceptanceResult {
                            acceptance: id.clone(),
                            outcome: AcceptanceOutcome::Failed {
                                word: words::INTERRUPTED.to_owned(),
                            },
                            at,
                        })
                        .map_err(|e| e.to_string())?;
                    did.push("the acceptance is recorded as failed (interrupted)");
                }
                if let (true, Some(intent)) = (w.intent_open, intent) {
                    p.acceptances
                        .complete_intent(&IntentCompletion {
                            intent,
                            end: if discarded {
                                IntentEnd::Discarded
                            } else {
                                IntentEnd::Abandoned
                            },
                            at,
                        })
                        .map_err(|e| e.to_string())?;
                    did.push("its intent is ended");
                }
            }
            ActRef::System(id) => {
                if matches!(w.ended, Ended::Not) {
                    p.acts
                        .finish_system_act(&SystemActEnd {
                            act: id.clone(),
                            outcome: AcceptanceOutcome::Failed {
                                word: words::INTERRUPTED.to_owned(),
                            },
                            at,
                        })
                        .map_err(|e| e.to_string())?;
                    did.push("the system act is recorded as failed (interrupted)");
                }
            }
        }
        Ok(if did.is_empty() {
            "the records were already ended".to_owned()
        } else {
            did.join("; ")
        })
    }

    /// Record an act whose change is in the history as sealed: whichever of its end, its intent's end and its
    /// proposal's acceptance is not written yet.
    fn end_sealed(&self, wiki: &WikiRecord, act: &ActRef) -> Result<String, String> {
        let p = &self.parts;
        let (w, intent) = self.act_witness(wiki, act).map_err(|e| e.to_string())?;
        let (block_id, patch_id) = match (&w.ended, &w.chain) {
            (Ended::Sealed { block_id, patch_id }, _)
            | (_, Chain::Sealed { block_id, patch_id }) => (block_id.clone(), patch_id.clone()),
            _ => return Err("the act's change is not in the history".to_owned()),
        };
        let at = (self.clock)();
        let mut did = Vec::new();
        match act {
            ActRef::Acceptance(id) => {
                if !matches!(w.ended, Ended::Sealed { .. }) {
                    p.acceptances
                        .finish_acceptance(&AcceptanceResult {
                            acceptance: id.clone(),
                            outcome: AcceptanceOutcome::Sealed {
                                block_id: block_id.clone(),
                                patch_id: patch_id.clone(),
                            },
                            at,
                        })
                        .map_err(|e| e.to_string())?;
                    did.push("the acceptance is recorded as sealed");
                }
                if let (true, Some(intent)) = (w.intent_open, intent) {
                    p.acceptances
                        .complete_intent(&IntentCompletion {
                            intent,
                            end: IntentEnd::Completed,
                            at,
                        })
                        .map_err(|e| e.to_string())?;
                    did.push("its intent is completed");
                }
                if let (Some(proposal), Some(acceptor), Some(ProposalState::HeldForReview)) =
                    (&w.proposal, &w.acceptor, w.proposal_state)
                {
                    let to = machine::apply(Some(ProposalState::HeldForReview), ProposalOp::Accept)
                        .map_err(|e| format!("{e:?}"))?;
                    let record = p
                        .proposals
                        .change_proposal(&ProposalChange {
                            transition: ProposalTransition {
                                proposal: proposal.clone(),
                                op: ProposalOp::Accept,
                                from: Some(ProposalState::HeldForReview),
                                to,
                                actor: acceptor.clone(),
                                reason: None,
                                at,
                            },
                            text: None,
                            revision: None,
                            superseded_by: None,
                        })
                        .map_err(|e| e.to_string())?;
                    did.push("its proposal is accepted");
                    if let Some(notifier) = &self.notifier {
                        notifier.accepted(wiki, &record, acceptor);
                    }
                }
            }
            ActRef::System(id) => {
                if !matches!(w.ended, Ended::Sealed { .. }) {
                    p.acts
                        .finish_system_act(&SystemActEnd {
                            act: id.clone(),
                            outcome: AcceptanceOutcome::Sealed {
                                block_id: block_id.clone(),
                                patch_id: patch_id.clone(),
                            },
                            at,
                        })
                        .map_err(|e| e.to_string())?;
                    did.push("the system act is recorded as sealed");
                }
            }
        }
        if let Some(reads) = &self.reads {
            reads.forget_wiki(wiki);
        }
        Ok(if did.is_empty() {
            format!("the records already agree (block {block_id})")
        } else {
            format!("block {block_id}: {}", did.join("; "))
        })
    }
}

/// Why `wiki` needs recovery, as fixed words; empty when it does not. The records alone, no prikk: cheap enough to
/// ask of every wiki at every start. A lease-holder record is a holder that died when it is not `own_holder`'s (a
/// process that holds a lease says so under its own name; at start-up it has none, and passes `None`).
pub fn needs_recovery(
    acceptances: &dyn AcceptanceStore,
    acts: &dyn ActStore,
    wiki: &WikiId,
    own_holder: Option<&str>,
) -> Result<Vec<&'static str>, StoreError> {
    let dead = acceptances
        .lease_holder(wiki)?
        .filter(|h| Some(h.holder.as_str()) != own_holder);
    reasons_of(acceptances, acts, wiki, dead.as_ref())
}

fn reasons_of(
    acceptances: &dyn AcceptanceStore,
    acts: &dyn ActStore,
    wiki: &WikiId,
    dead: Option<&LeaseHolder>,
) -> Result<Vec<&'static str>, StoreError> {
    let mut out = Vec::new();
    if acts.recovery_flag(wiki)?.is_some() {
        out.push("flagged");
    }
    if !acceptances.open_intents(wiki)?.is_empty() {
        out.push("open-intent");
    }
    if !acceptances.pending_acceptances(wiki)?.is_empty() {
        out.push("pending-acceptance");
    }
    if acts.system_acts(wiki)?.iter().any(|e| e.end.is_none()) {
        out.push("open-system-act");
    }
    // An export's or a refresh's record that was left leaves nothing in the repository to recover.
    if dead.is_some_and(|h| {
        h.purpose != Purpose::Export.as_str() && h.purpose != Purpose::Refresh.as_str()
    }) {
        out.push("dead-holder");
    }
    if acts.recovery_acts(wiki)?.iter().any(|e| e.end.is_none()) {
        out.push("interrupted-recovery");
    }
    Ok(out)
}

/// A plan that has nothing to do: what recovery shows for a wiki that needs none.
fn build_empty(wiki: &WikiId) -> Plan {
    Plan {
        wiki: wiki.clone(),
        view: None,
        witnesses: vec![
            "the wiki is not flagged, no act is unfinished, and no lease holder died".to_owned(),
        ],
        steps: Vec::new(),
        stop: None,
        notes: Vec::new(),
    }
}
