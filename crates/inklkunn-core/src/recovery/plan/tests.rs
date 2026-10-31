//! The decision table of a recovery plan, row by row, with no repository: the plan is a pure function of what was
//! read (handoff 004 T5).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_prikk::RepoStatus;
use inklkunn_prikk::storage::{Operation, PathRef, Queue, QueuedPatch};
use inklkunn_signer::recover::text::{
    ACKNOWLEDGED_DAMAGE_CODE, Diagnostic, DoctorReport, HeldLock, Severity, UNREACHED_OBJECT_CODE,
};
use inklkunn_signer::{RecoveryOp, WorktreeState};
use inklkunn_store::{
    AcceptanceId, AccountId, LeaseHolder, ProposalId, ProposalState, RecoveryFlag, RecoveryReason,
    SystemActId, WikiId,
};

use super::*;
use crate::acceptance::message_digest;
use crate::recovery::processes::Foreign;

const VIEW: &str = "heads/main";
const MESSAGE: &str = "Fix it\n\nKind: proposal\n";

fn status(queue: Vec<QueuedPatch>, torn: u64) -> RepoStatus {
    RepoStatus {
        schema_version: "status-report-v1".to_owned(),
        current_branch: Some(VIEW.to_owned()),
        active_wal_records: u64::try_from(queue.len()).unwrap(),
        trailing_partial_wal_bytes: torn,
        provisional_worktree: None,
        interrupted_materialization: None,
        queue: Queue {
            count: u64::try_from(queue.len()).unwrap(),
            target_ref: if queue.is_empty() {
                None
            } else {
                Some(VIEW.to_owned())
            },
            target_ref_status: None,
            patches: queue,
        },
    }
}

fn patch(message: &str) -> QueuedPatch {
    QueuedPatch {
        patch_id: "a".repeat(64),
        message: message.to_owned(),
        operations: vec![Operation {
            kind: "edit-text".to_owned(),
            paths: vec![PathRef {
                path: Some("pages/Home.md".to_owned()),
                unresolved_node_id: None,
            }],
            content: serde_json::Value::Null,
        }],
    }
}

fn diag(severity: Severity, code: &str, recommendation: Option<&str>) -> Diagnostic {
    Diagnostic {
        severity,
        code: code.to_owned(),
        message: "m".to_owned(),
        recommendation: recommendation.map(str::to_owned),
    }
}

fn doctor(diagnostics: Vec<Diagnostic>) -> DoctorReport {
    let n = |s: Severity| {
        u32::try_from(diagnostics.iter().filter(|d| d.severity == s).count()).unwrap()
    };
    DoctorReport {
        errors: n(Severity::Error),
        warnings: n(Severity::Warning),
        info: n(Severity::Info),
        diagnostics,
    }
}

fn act(chain: Chain) -> ActWitness {
    ActWitness {
        act: ActRef::Acceptance(AcceptanceId::new()),
        view: VIEW.to_owned(),
        base_block: Some("b".repeat(64)),
        message: Some(MESSAGE.to_owned()),
        message_digest: Some(message_digest(MESSAGE)),
        ended: Ended::Not,
        intent_open: true,
        proposal: Some(ProposalId::new()),
        proposal_state: Some(ProposalState::HeldForReview),
        acceptor: Some(AccountId::new()),
        chain,
    }
}

/// A wiki that is flagged and has nothing else wrong: the base every row varies.
fn flagged() -> Witness {
    let wiki = WikiId::new();
    Witness {
        flag: Some(RecoveryFlag {
            wiki: wiki.clone(),
            reason: RecoveryReason::Interrupted,
            act: None,
            since: 1,
        }),
        wiki,
        dead_holder: None,
        interrupted_recoveries: 0,
        acts: Vec::new(),
        status: status(Vec::new(), 0),
        doctor: doctor(vec![diag(Severity::Info, "PRIKK-DOCTOR-VERIFY-OK", None)]),
        worktree: WorktreeState {
            clean: true,
            changes: 0,
            declarations: 0,
        },
        locks: Vec::new(),
        foreign: Vec::new(),
        lease_held: true,
    }
}

fn names(plan: &Plan) -> Vec<&'static str> {
    plan.steps.iter().map(Step::name).collect()
}

fn lock(kind: &str, pid: u32, liveness: &str) -> HeldLock {
    HeldLock {
        path: format!("/srv/r/.prikk/{kind}.lock"),
        kind: kind.to_owned(),
        recorded_pid: pid,
        liveness: liveness.to_owned(),
    }
}

const TAIL: [&str; 3] = ["verify", "reconcile", "clear-flag"];

#[test]
fn a_wiki_that_needs_nothing_has_an_empty_plan_and_a_flagged_one_with_nothing_else_is_only_checked()
{
    let mut w = flagged();
    w.flag = None;
    assert!(build(&w).is_empty(), "nothing flagged, nothing unfinished");
    // Flagged (the seal could not be confirmed): nothing to complete or discard; check and clear.
    let plan = build(&flagged());
    assert_eq!(plan.stop, None);
    assert_eq!(names(&plan), TAIL);
    // A dead holder alone, or an interrupted recovery alone, is the same.
    let mut w = flagged();
    w.flag = None;
    w.dead_holder = Some(LeaseHolder {
        wiki: w.wiki.clone(),
        holder: "inklkunn-1".to_owned(),
        purpose: "acceptance".to_owned(),
        acceptance: None,
        since: 1,
    });
    assert_eq!(names(&build(&w)), TAIL);
    let mut w = flagged();
    w.flag = None;
    w.interrupted_recoveries = 1;
    assert_eq!(names(&build(&w)), TAIL);
}

#[test]
fn a_queued_patch_that_is_the_change_of_the_unfinished_act_is_completed_with_seal_and_never_committed_again()
 {
    let mut w = flagged();
    let a = act(Chain::NotSealed);
    w.status = status(vec![patch(MESSAGE)], 0);
    w.acts = vec![a.clone()];
    let plan = build(&w);
    assert_eq!(plan.stop, None);
    assert_eq!(
        names(&plan),
        [
            "seal-queued",
            "end-sealed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(matches!(
        plan.steps.first(),
        Some(Step::SealQueued { act, view, .. }) if *act == a.act && view == VIEW
    ));
    // The signer is bound to exactly that, and to nothing more.
    assert_eq!(
        plan.ops(),
        vec![RecoveryOp::SealQueued {
            acceptance: a.act.token(),
            view: VIEW.to_owned()
        }]
    );
    // A system act (an import) is completed the same way.
    let mut sys = act(Chain::NotSealed);
    sys.act = ActRef::System(SystemActId::new());
    sys.proposal = None;
    sys.proposal_state = None;
    sys.intent_open = false;
    w.acts = vec![sys];
    assert_eq!(names(&build(&w))[0], "seal-queued");
}

#[test]
fn a_queued_patch_that_is_not_the_act_is_a_stop_and_so_is_one_with_no_act_and_none_is_discarded() {
    // Another message.
    let mut w = flagged();
    w.status = status(vec![patch("Something else\n\nKind: proposal\n")], 0);
    w.acts = vec![act(Chain::NotSealed)];
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::QueuedPatchIsNotTheAct));
    assert!(plan.steps.is_empty());
    // The message of the act, for another view.
    let mut w = flagged();
    w.status = status(vec![patch(MESSAGE)], 0);
    w.status.queue.target_ref = Some("heads/other".to_owned());
    w.acts = vec![act(Chain::NotSealed)];
    assert_eq!(build(&w).stop, Some(Stop::QueuedPatchIsNotTheAct));
    // The message of the act, but the tip moved (the base is not the tip).
    let mut w = flagged();
    w.status = status(vec![patch(MESSAGE)], 0);
    w.acts = vec![act(Chain::TipMoved)];
    assert_eq!(build(&w).stop, Some(Stop::QueuedPatchIsNotTheAct));
    // The message of the act, whose record already ended.
    let mut ended = act(Chain::NotSealed);
    ended.ended = Ended::Failed;
    let mut w = flagged();
    w.status = status(vec![patch(MESSAGE)], 0);
    w.acts = vec![ended];
    assert_eq!(build(&w).stop, Some(Stop::QueuedPatchIsNotTheAct));
    // No act at all: a sound queued patch that nothing describes. prikk has no verb to remove it.
    let mut w = flagged();
    w.status = status(vec![patch(MESSAGE)], 0);
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(
        plan.steps.is_empty() && plan.ops().is_empty(),
        "no step, no operation"
    );
    assert!(
        plan.lines()
            .iter()
            .any(|l| l.contains("STOP (foreign-queued-patch)"))
    );
    // Two patches.
    let mut w = flagged();
    w.status = status(vec![patch(MESSAGE), patch(MESSAGE)], 0);
    w.acts = vec![act(Chain::NotSealed)];
    assert_eq!(build(&w).stop, Some(Stop::SeveralQueued));
}

fn torn() -> DoctorReport {
    doctor(vec![
        diag(
            Severity::Warning,
            "PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS",
            Some("no repair is required"),
        ),
        diag(
            Severity::Error,
            ACKNOWLEDGED_DAMAGE_CODE,
            Some("run `prikk doctor --discard-damaged-commits` to remove it"),
        ),
    ])
}

#[test]
fn a_torn_commit_is_discarded_with_the_act_ended_as_discarded_or_reported_when_no_act_has_it() {
    let mut w = flagged();
    w.status = status(Vec::new(), 900);
    w.doctor = torn();
    w.worktree = WorktreeState {
        clean: false,
        changes: 1,
        declarations: 0,
    };
    w.acts = vec![act(Chain::NotSealed)];
    let plan = build(&w);
    assert_eq!(plan.stop, None);
    assert_eq!(
        names(&plan),
        [
            "discard-damaged",
            "restore-worktree",
            "end-failed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(matches!(
        plan.steps
            .iter()
            .find(|s| matches!(s, Step::EndFailed { .. })),
        Some(Step::EndFailed {
            discarded: true,
            ..
        })
    ));
    assert_eq!(
        plan.ops(),
        vec![
            RecoveryOp::DiscardDamaged,
            RecoveryOp::RestoreWorktree {
                view: VIEW.to_owned()
            }
        ]
    );
    // No act has it: discarded and reported.
    w.acts.clear();
    let plan = build(&w);
    assert_eq!(
        names(&plan),
        [
            "discard-damaged",
            "restore-worktree",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("matches no recorded act"))
    );
    // An act whose change is in the history is recorded as sealed, though a torn commit is also discarded.
    let mut w = flagged();
    w.doctor = torn();
    w.status = status(Vec::new(), 900);
    w.acts = vec![act(Chain::Sealed {
        block_id: "c".repeat(64),
        patch_id: "d".repeat(64),
    })];
    assert_eq!(names(&build(&w))[..2], ["discard-damaged", "end-sealed"]);
    // Other trouble beside the damage: a stop.
    let mut w = flagged();
    w.doctor = doctor(vec![
        diag(Severity::Error, ACKNOWLEDGED_DAMAGE_CODE, None),
        diag(Severity::Error, "PRIKK-DOCTOR-OBJECT-MISSING", None),
    ]);
    assert_eq!(build(&w).stop, Some(Stop::DoctorTrouble));
}

#[test]
fn an_act_with_nothing_queued_is_abandoned_when_not_in_the_history_and_recorded_when_it_is() {
    // Killed before anything was committed (the signer never asked, or asked and stopped before `commit`).
    let mut w = flagged();
    w.acts = vec![act(Chain::NotSealed)];
    let plan = build(&w);
    assert_eq!(
        names(&plan),
        ["end-failed", "verify", "reconcile", "clear-flag"]
    );
    assert!(matches!(
        plan.steps.first(),
        Some(Step::EndFailed {
            discarded: false,
            ..
        })
    ));
    // Killed with the files written and not committed: the worktree is put back first.
    w.worktree = WorktreeState {
        clean: false,
        changes: 2,
        declarations: 1,
    };
    assert_eq!(
        names(&build(&w)),
        [
            "restore-worktree",
            "end-failed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    // An acceptance with no intent: nothing was committed by it.
    let mut no_intent = act(Chain::NoMessage);
    no_intent.message = None;
    no_intent.message_digest = None;
    no_intent.base_block = None;
    no_intent.intent_open = false;
    let mut w = flagged();
    w.acts = vec![no_intent];
    assert_eq!(names(&build(&w))[0], "end-failed");
    // Killed after the seal and before the records: the change is in the history.
    let mut w = flagged();
    w.acts = vec![act(Chain::Sealed {
        block_id: "c".repeat(64),
        patch_id: "d".repeat(64),
    })];
    let plan = build(&w);
    assert_eq!(
        names(&plan),
        ["end-sealed", "verify", "reconcile", "clear-flag"]
    );
    assert!(matches!(
        plan.steps.first(),
        Some(Step::EndSealed { block_id: Some(b), .. }) if b == &"c".repeat(64)
    ));
    // Only the proposal is left to accept (the intent and the acceptance's end were written).
    let mut sealed = act(Chain::Sealed {
        block_id: "c".repeat(64),
        patch_id: "d".repeat(64),
    });
    sealed.ended = Ended::Sealed {
        block_id: "c".repeat(64),
        patch_id: "d".repeat(64),
    };
    sealed.intent_open = false;
    assert_eq!(sealed.needs_records(), (false, false, true));
    // The tip moved and the change is not there; the history could not be read: stops.
    for (chain, stop) in [
        (Chain::TipMoved, Stop::TipMoved),
        (Chain::Unreadable("x".to_owned()), Stop::HistoryUnreadable),
    ] {
        let mut w = flagged();
        w.acts = vec![act(chain)];
        assert_eq!(build(&w).stop, Some(stop));
    }
    // Two unfinished acts: a person decides.
    let mut w = flagged();
    w.acts = vec![act(Chain::NotSealed), act(Chain::NotSealed)];
    assert_eq!(build(&w).stop, Some(Stop::SeveralActs));
}

#[test]
fn stale_locks_are_cleared_first_on_the_lease_record_and_prikks_liveness_words_change_nothing() {
    for liveness in [
        "appears running -- do not clear",
        "does not appear to be running (not proof it is safe to clear)",
        "unknown (not proof it is safe to clear)",
    ] {
        let mut w = flagged();
        w.locks = vec![
            lock("active", 2, liveness),
            lock("container:object-store", 2, liveness),
        ];
        w.acts = vec![act(Chain::NotSealed)];
        let plan = build(&w);
        assert_eq!(plan.stop, None, "{liveness}");
        assert_eq!(
            names(&plan)[0],
            "clear-locks",
            "locks first, so seal and discard can take them"
        );
        let Some(Step::ClearLocks { locks }) = plan.steps.first() else {
            panic!("{plan:?}");
        };
        assert_eq!(locks.len(), 2);
        // What prikk said is kept in the plan for the record.
        assert_eq!(locks[0].prikk_liveness, liveness);
        assert_eq!(locks[0].recorded_pid, 2);
        assert!(matches!(&plan.ops()[0], RecoveryOp::ClearLocks { paths } if paths.len() == 2));
    }
    // A lock of a kind an acceptance does not leave: a stop, and nothing is cleared.
    let mut w = flagged();
    w.locks = vec![
        lock("active", 1, "x"),
        lock("container:trust-policy", 1, "x"),
    ];
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::UnknownLock));
    assert!(plan.steps.is_empty());
    // A prikk or bwrap of somebody else's that may be in the repository: no lock is cleared.
    let mut w = flagged();
    w.locks = vec![lock("active", 1, "x")];
    w.foreign = vec![Foreign {
        pid: 77,
        tool: "prikk".to_owned(),
    }];
    assert_eq!(build(&w).stop, Some(Stop::ForeignProcess));
    // ... but such a process is no reason to stop when no lock is held.
    w.locks.clear();
    assert_eq!(build(&w).stop, None);
}

#[test]
fn unreached_object_warnings_are_noted_and_do_not_block_and_other_trouble_does() {
    let mut w = flagged();
    w.doctor = doctor(vec![
        diag(
            Severity::Warning,
            UNREACHED_OBJECT_CODE,
            Some("the bytes stay"),
        ),
        diag(Severity::Info, "PRIKK-DOCTOR-VERIFY-OK", None),
    ]);
    let plan = build(&w);
    assert_eq!(plan.stop, None);
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("1 unreached-object warning"))
    );
    assert_eq!(names(&plan), TAIL);
    // A warning of another code, or an error of that one, blocks.
    for d in [
        diag(Severity::Warning, "PRIKK-DOCTOR-SOMETHING-NEW", None),
        diag(Severity::Error, UNREACHED_OBJECT_CODE, None),
        diag(
            Severity::Warning,
            UNREACHED_OBJECT_CODE,
            Some("run `prikk doctor --repair-index`"),
        ),
    ] {
        let mut w = flagged();
        w.doctor = doctor(vec![d]);
        assert_eq!(build(&w).stop, Some(Stop::DoctorTrouble));
    }
    // An interrupted checkout is prikk's own state: a stop.
    let mut w = flagged();
    w.status.provisional_worktree = Some(serde_json::json!({"ref": "heads/main"}));
    assert_eq!(build(&w).stop, Some(Stop::InterruptedState));
    let mut w = flagged();
    w.status.interrupted_materialization = Some(serde_json::json!("x"));
    assert_eq!(build(&w).stop, Some(Stop::InterruptedState));
}

#[test]
fn the_plan_is_kept_as_json_and_read_back_as_what_the_signer_may_do_and_nothing_else() {
    let mut w = flagged();
    w.locks = vec![lock("active", 2, "appears running -- do not clear")];
    w.status = status(vec![patch(MESSAGE)], 0);
    let a = act(Chain::NotSealed);
    w.acts = vec![a.clone()];
    w.worktree = WorktreeState {
        clean: false,
        changes: 1,
        declarations: 0,
    };
    let plan = build(&w);
    let facts = facts_of(&plan.wiki, &plan.to_json()).expect("a plan of this version");
    assert_eq!(facts.wiki, plan.wiki);
    assert_eq!(facts.ops, plan.ops());
    assert!(facts.allows(&RecoveryOp::SealQueued {
        acceptance: a.act.token(),
        view: VIEW.to_owned()
    }));
    assert!(
        !facts.allows(&RecoveryOp::DiscardDamaged),
        "the plan did not name it"
    );
    assert!(!facts.allows(&RecoveryOp::SealQueued {
        acceptance: AcceptanceId::new().to_string(),
        view: VIEW.to_owned()
    }));
    // Another wiki's plan, another version, and text that is not a plan allow nothing.
    assert!(facts_of(&WikiId::new(), &plan.to_json()).is_none());
    assert!(facts_of(&plan.wiki, &plan.to_json().replace("\"v\":1", "\"v\":2")).is_none());
    assert!(facts_of(&plan.wiki, "not json").is_none());
    assert!(facts_of(&plan.wiki, "{}").is_none());
    // A stop is a plan with no operations.
    let mut w = flagged();
    w.status = status(vec![patch(MESSAGE)], 0);
    let stopped = build(&w);
    let facts = facts_of(&stopped.wiki, &stopped.to_json()).unwrap();
    assert!(facts.ops.is_empty());
    assert!(stopped.to_json().contains("foreign-queued-patch"));
}
