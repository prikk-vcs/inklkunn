//! The decision table of a recovery plan, row by row, with no repository: the plan is a pure function of what was
//! read (handoff 004 T5).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_prikk::RepoStatus;
use inklkunn_prikk::storage::{Operation, PathRef, Queue, QueuedPatch};
use inklkunn_signer::recover::text::{
    ACKNOWLEDGED_DAMAGE_CODE, Diagnostic, DoctorReport, HeldLock, Severity, UNREACHED_OBJECT_CODE,
};
use inklkunn_signer::{RecoveryOp, UNTRACEABLE_SENTENCE, WorktreeState};
use inklkunn_store::{
    AcceptanceId, AccountId, LeaseHolder, ProposalId, ProposalState, RecoveryFlag, RecoveryReason,
    SystemActId, ViewActId, WikiId,
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
        view_acts: Vec::new(),
        status: status(Vec::new(), 0),
        doctor: doctor(vec![diag(Severity::Info, "PRIKK-DOCTOR-VERIFY-OK", None)]),
        retirement: None,
        worktree: Some(WorktreeState {
            clean: true,
            changes: 0,
            declarations: 0,
        }),
        locks: Vec::new(),
        foreign: Vec::new(),
        lease_held: true,
        cleanup_pending: None,
        retirement_unfinished: None,
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

/// The `i`-th of a list that must have it (tests may not index).
fn nth<T>(items: &[T], i: usize) -> &T {
    items.get(i).expect("an element")
}

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
    assert_eq!(nth(&names(&build(&w)), 0), &"seal-queued");
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
    w.worktree = None; // prikk will not compare a worktree over a torn log
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
    assert_eq!(
        names(&build(&w)).get(..2),
        Some(&["discard-damaged", "end-sealed"][..])
    );
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
    w.worktree = Some(WorktreeState {
        clean: false,
        changes: 2,
        declarations: 1,
    });
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
    assert_eq!(nth(&names(&build(&w)), 0), &"end-failed");
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
            nth(&names(&plan), 0),
            &"clear-locks",
            "locks first, so seal and discard can take them"
        );
        let Some(Step::ClearLocks { locks }) = plan.steps.first() else {
            panic!("{plan:?}");
        };
        assert_eq!(locks.len(), 2);
        // What prikk said is kept in the plan for the record.
        assert_eq!(nth(locks, 0).prikk_liveness, liveness);
        assert_eq!(nth(locks, 0).recorded_pid, 2);
        assert!(
            matches!(nth(&plan.ops(), 0), RecoveryOp::ClearLocks { paths } if paths.len() == 2)
        );
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
    // ... and it stops every step, not only the clearing of a lock (P8): with no lock held the plan still stops.
    w.locks.clear();
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::ForeignProcess));
    assert!(plan.steps.is_empty());
}

#[test]
fn a_foreign_process_stops_the_seal_the_restore_and_the_tail_too() {
    let foreign = || {
        vec![Foreign {
            pid: 77,
            tool: "bwrap".to_owned(),
        }]
    };
    // A queued patch that is the act's change: the seal waits.
    let mut w = flagged();
    w.status = status(vec![patch(MESSAGE)], 0);
    w.acts = vec![act(Chain::NotSealed)];
    w.foreign = foreign();
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::ForeignProcess));
    assert!(plan.steps.is_empty() && plan.ops().is_empty());
    // A dirty worktree: the restore waits.
    let mut w = flagged();
    w.worktree = Some(WorktreeState {
        clean: false,
        changes: 1,
        declarations: 0,
    });
    w.foreign = foreign();
    assert_eq!(build(&w).stop, Some(Stop::ForeignProcess));
    // A flag alone (the tail: verify, reconcile, clear the flag) waits as well.
    let mut w = flagged();
    w.foreign = foreign();
    assert_eq!(build(&w).stop, Some(Stop::ForeignProcess));
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
    w.worktree = Some(WorktreeState {
        clean: false,
        changes: 1,
        declarations: 0,
    });
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

// ---- a publication a killed `seal` left unfinished (handoff 004, P15) ----

const CLEANUP_CODE: &str = "PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING";
const PUBLISHED: &str = "c";

fn cleanup_error() -> Diagnostic {
    diag(
        Severity::Error,
        CLEANUP_CODE,
        Some("run signer-backed `prikk seal --allow-no-audit` for the affected ref"),
    )
}

/// The block that sits on the act's base, seals the act's message, and that the history found.
fn published_block() -> Published {
    Published {
        view: VIEW.to_owned(),
        block_id: PUBLISHED.repeat(64),
        parent: Some("b".repeat(64)),
        message_digests: vec![message_digest(MESSAGE)],
    }
}

fn published_act() -> ActWitness {
    act(Chain::Sealed {
        block_id: PUBLISHED.repeat(64),
        patch_id: "a".repeat(64),
    })
}

/// A wiki where `doctor` names exactly the pending cleanup, `status` refuses, and the one unfinished act's change is
/// the block that was published.
fn cleanup_pending() -> Witness {
    let mut w = flagged();
    w.doctor = doctor(vec![cleanup_error()]);
    w.acts = vec![published_act()];
    w.worktree = None;
    w.cleanup_pending = Some(CleanupPending {
        status_refused: true,
        published: Some(published_block()),
    });
    w
}

#[test]
fn a_pending_cleanup_of_the_acts_own_block_is_finished_with_seal_once_and_recorded_as_a_step() {
    let w = cleanup_pending();
    let a = nth(&w.acts, 0).clone();
    let plan = build(&w);
    assert_eq!(plan.stop, None, "{:#?}", plan.lines());
    assert_eq!(
        names(&plan),
        [
            "complete-publication",
            "restore-worktree",
            "end-sealed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(matches!(
        plan.steps.first(),
        Some(Step::CompletePublication { act, view, block_id })
            if *act == a.act && view == VIEW && *block_id == PUBLISHED.repeat(64)
    ));
    assert!(matches!(
        plan.steps.get(2),
        Some(Step::EndSealed { block_id: Some(b), .. }) if *b == PUBLISHED.repeat(64)
    ));
    // The signer is bound to exactly that, and the text says what it is and why.
    assert_eq!(
        plan.ops(),
        vec![
            RecoveryOp::CompletePublication {
                acceptance: a.act.token(),
                view: VIEW.to_owned()
            },
            RecoveryOp::RestoreWorktree {
                view: VIEW.to_owned()
            }
        ]
    );
    let lines = plan.lines();
    assert!(
        lines
            .iter()
            .any(|l| l
                .contains("step 1: run seal --allow-no-audit once, as prikk's doctor recommends")),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("saw: prikk status refuses (an integrity error)")),
        "{lines:#?}"
    );
    assert!(
        lines.iter().any(|l| l
            .contains("saw: prikk doctor names the error PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING")),
        "{lines:#?}"
    );
    // The step is in the recorded plan and is read back as what the signer may do, and only that.
    let facts = facts_of(&plan.wiki, &plan.to_json()).expect("a plan of this version");
    assert_eq!(facts.ops, plan.ops());
    assert!(facts.allows(&RecoveryOp::CompletePublication {
        acceptance: a.act.token(),
        view: VIEW.to_owned()
    }));
    assert!(!facts.allows(&RecoveryOp::CompletePublication {
        acceptance: AcceptanceId::new().to_string(),
        view: VIEW.to_owned()
    }));
    assert!(plan.to_json().contains("\"step\":\"complete-publication\""));
    // A stale lock the killed `seal` left is cleared first; a system act's change is finished the same way.
    let mut w = cleanup_pending();
    w.locks = vec![lock("active", 408_657, "does not appear to be running")];
    assert_eq!(
        names(&build(&w)).iter().take(2).collect::<Vec<_>>(),
        [&"clear-locks", &"complete-publication"]
    );
    let mut sys = published_act();
    sys.act = ActRef::System(SystemActId::new());
    sys.proposal = None;
    sys.proposal_state = None;
    sys.intent_open = false;
    let mut w = cleanup_pending();
    w.acts = vec![sys];
    assert_eq!(nth(&names(&build(&w)), 0), &"complete-publication");
    // A worktree that could be compared and is the baseline is not put back.
    let mut w = cleanup_pending();
    w.worktree = Some(WorktreeState {
        clean: true,
        changes: 0,
        declarations: 0,
    });
    assert!(!names(&build(&w)).contains(&"restore-worktree"));
}

#[test]
fn a_pending_cleanup_is_a_stop_unless_doctor_names_exactly_that_code_and_the_block_is_the_acts() {
    let stop_of = |w: &Witness| {
        let plan = build(w);
        assert!(
            plan.steps.is_empty() && plan.ops().is_empty(),
            "a stop takes no step: {:?}",
            names(&plan)
        );
        plan.stop
    };
    // Any other code beside it, or instead of it, is doctor-trouble: nothing is finished.
    let mut w = cleanup_pending();
    w.doctor = doctor(vec![
        cleanup_error(),
        diag(Severity::Error, "PRIKK-DOCTOR-OBJECT-MISSING", None),
    ]);
    assert_eq!(stop_of(&w), Some(Stop::DoctorTrouble));
    let mut w = cleanup_pending();
    w.doctor = doctor(vec![
        cleanup_error(),
        diag(
            Severity::Warning,
            "PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS",
            None,
        ),
    ]);
    assert_eq!(stop_of(&w), Some(Stop::DoctorTrouble));
    let mut w = cleanup_pending();
    w.doctor = doctor(vec![diag(
        Severity::Error,
        "PRIKK-VERIFY-REF-SOMETHING-ELSE",
        None,
    )]);
    assert_eq!(stop_of(&w), Some(Stop::DoctorTrouble));
    // Two of the code itself are not "exactly that".
    let mut w = cleanup_pending();
    w.doctor = doctor(vec![cleanup_error(), cleanup_error()]);
    assert_eq!(stop_of(&w), Some(Stop::DoctorTrouble));
    // The same error with no witness that says it is the state (the reader did not recognise it) is trouble as ever.
    let mut w = cleanup_pending();
    w.cleanup_pending = None;
    assert_eq!(stop_of(&w), Some(Stop::DoctorTrouble));
    // Unreached-object notes beside it do not block.
    let mut w = cleanup_pending();
    w.doctor = doctor(vec![
        cleanup_error(),
        diag(
            Severity::Warning,
            UNREACHED_OBJECT_CODE,
            Some("the bytes stay; no command removes them yet"),
        ),
    ]);
    assert_eq!(build(&w).stop, None);
    assert_eq!(nth(&names(&build(&w)), 0), &"complete-publication");

    // The block published must be the act's: each of its marks, one at a time.
    type Spoil = Box<dyn Fn(&mut Witness)>;
    let wrong: Vec<(&str, Spoil)> = vec![
        (
            "another message",
            Box::new(|w: &mut Witness| {
                if let Some(p) = w
                    .cleanup_pending
                    .as_mut()
                    .and_then(|c| c.published.as_mut())
                {
                    p.message_digests = vec![message_digest("Something else\n\nKind: proposal\n")];
                }
            }),
        ),
        (
            "two patches",
            Box::new(|w: &mut Witness| {
                if let Some(p) = w
                    .cleanup_pending
                    .as_mut()
                    .and_then(|c| c.published.as_mut())
                {
                    p.message_digests = vec![message_digest(MESSAGE), message_digest(MESSAGE)];
                }
            }),
        ),
        (
            "no patch",
            Box::new(|w: &mut Witness| {
                if let Some(p) = w
                    .cleanup_pending
                    .as_mut()
                    .and_then(|c| c.published.as_mut())
                {
                    p.message_digests.clear();
                }
            }),
        ),
        (
            "another base",
            Box::new(|w: &mut Witness| {
                if let Some(p) = w
                    .cleanup_pending
                    .as_mut()
                    .and_then(|c| c.published.as_mut())
                {
                    p.parent = Some("d".repeat(64));
                }
            }),
        ),
        (
            "no parent",
            Box::new(|w: &mut Witness| {
                if let Some(p) = w
                    .cleanup_pending
                    .as_mut()
                    .and_then(|c| c.published.as_mut())
                {
                    p.parent = None;
                }
            }),
        ),
        (
            "another view",
            Box::new(|w: &mut Witness| {
                if let Some(p) = w
                    .cleanup_pending
                    .as_mut()
                    .and_then(|c| c.published.as_mut())
                {
                    p.view = "heads/other".to_owned();
                }
            }),
        ),
        (
            "the history found the act's message in another block",
            Box::new(|w: &mut Witness| {
                if let Some(a) = w.acts.first_mut() {
                    a.chain = Chain::Sealed {
                        block_id: "e".repeat(64),
                        patch_id: "a".repeat(64),
                    };
                }
            }),
        ),
        (
            "the history did not find it",
            Box::new(|w: &mut Witness| {
                if let Some(a) = w.acts.first_mut() {
                    a.chain = Chain::NotSealed;
                }
            }),
        ),
        (
            "the act ended",
            Box::new(|w: &mut Witness| {
                if let Some(a) = w.acts.first_mut() {
                    a.ended = Ended::Failed;
                }
            }),
        ),
        (
            "the act has no message",
            Box::new(|w: &mut Witness| {
                if let Some(a) = w.acts.first_mut() {
                    a.message_digest = None;
                }
            }),
        ),
        (
            "no block was read",
            Box::new(|w: &mut Witness| {
                if let Some(c) = w.cleanup_pending.as_mut() {
                    c.published = None;
                }
            }),
        ),
        (
            "no act at all",
            Box::new(|w: &mut Witness| {
                w.acts.clear();
                if let Some(c) = w.cleanup_pending.as_mut() {
                    c.published = None;
                }
            }),
        ),
    ];
    for (what, mutate) in wrong {
        let mut w = cleanup_pending();
        mutate(&mut w);
        assert_eq!(stop_of(&w), Some(Stop::PublishedBlockIsNotTheAct), "{what}");
    }
    // The stop is named in the plan's text, and it is a stop of the plan, not of the reader.
    let mut w = cleanup_pending();
    w.acts.clear();
    assert!(
        build(&w)
            .lines()
            .iter()
            .any(|l| l.contains("STOP (published-block-is-not-the-act)"))
    );
    // More than one unfinished act, or more than one patch queued, stops before the row is reached.
    let mut w = cleanup_pending();
    w.acts = vec![published_act(), published_act()];
    assert_eq!(stop_of(&w), Some(Stop::SeveralActs));
    let mut w = cleanup_pending();
    w.status = status(vec![patch(MESSAGE), patch(MESSAGE)], 0);
    assert_eq!(stop_of(&w), Some(Stop::SeveralQueued));
    // A foreign process stops it like every step.
    let mut w = cleanup_pending();
    w.foreign = vec![Foreign {
        pid: 4_242,
        tool: "prikk".to_owned(),
    }];
    assert_eq!(stop_of(&w), Some(Stop::ForeignProcess));
}

// ---- a key retirement that was begun and not finished (handoff 004, Q1)

fn untrusted_block() -> Diagnostic {
    diag(Severity::Error, "PRIKK-TRUST-PUBLICATION-UNTRUSTED", None)
}

const UNFINISHED: &str = "the retirement of key k was begun and its record not finished, so nothing is excused: run `inklkunn wiki keys finish-retirement w`";

#[test]
fn an_unfinished_retirement_stops_the_plan_on_the_findings_it_leaves_and_names_the_command() {
    let mut w = flagged();
    w.doctor = doctor(vec![untrusted_block()]);
    // Without the signer's word the findings are damage recovery has no answer for.
    assert_eq!(build(&w).stop, Some(Stop::DoctorTrouble));
    // With it, the stop names what to do, and nothing is planned.
    w.retirement_unfinished = Some(UNFINISHED.to_owned());
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::RetirementUnfinished));
    assert!(plan.steps.is_empty());
    assert!(plan.notes.iter().any(|n| n.contains("finish-retirement")));
    assert!(
        Stop::RetirementUnfinished
            .meaning()
            .contains("finish-retirement")
    );
    assert_eq!(Stop::RetirementUnfinished.as_str(), "retirement-unfinished");
}

#[test]
fn findings_that_cannot_be_traced_stop_the_plan_as_retirement_unfinished_and_never_as_doctor_trouble()
 {
    // Z2. The retirement found acts it cannot trace to what the key sealed for this service (a late block, a closed
    // branch, a tag): the key is out of trust, nothing is recorded, and `doctor` reports findings nothing excuses.
    // The stop is `retirement-unfinished`, never `doctor-trouble`, whatever else `doctor` reports beside them, and the
    // note carries the one sentence and sends nobody to `recover` to cure it.
    let note = format!(
        "the retirement of key k was begun and cannot be finished, so nothing is excused and nothing new is sealed: 1 finding(s) of the key k cannot be traced: tag t; {UNTRACEABLE_SENTENCE} (`inklkunn wiki keys abandon-retirement w --confirm <the wiki's slug>`)"
    );
    for others in [
        vec![untrusted_block()],
        vec![
            untrusted_block(),
            diag(Severity::Error, "PRIKK-DOCTOR-OBJECT-MISSING", None),
        ],
    ] {
        let mut w = flagged();
        w.doctor = doctor(others);
        w.retirement_unfinished = Some(note.clone());
        let plan = build(&w);
        assert_eq!(plan.stop, Some(Stop::RetirementUnfinished));
        assert_ne!(plan.stop, Some(Stop::DoctorTrouble));
        assert!(plan.steps.is_empty());
        assert!(
            plan.notes.iter().any(|n| n.contains(UNTRACEABLE_SENTENCE)),
            "{:?}",
            plan.notes
        );
        assert!(
            !plan.notes.iter().any(|n| n.contains("wiki recover")),
            "{:?}",
            plan.notes
        );
    }
    // The stop's own words do not promise that recovery cures it.
    assert!(
        Stop::RetirementUnfinished
            .meaning()
            .contains("recovery does not cure them either")
    );
}

#[test]
fn an_unfinished_retirement_stops_nothing_that_it_did_not_cause() {
    // The key was never removed from trust (the retirement stopped before), so `doctor` reports no such finding: the
    // plan is the ordinary one, and the state is only a note for the operator.
    let mut w = flagged();
    w.retirement_unfinished = Some(UNFINISHED.to_owned());
    let plan = build(&w);
    assert_eq!(plan.stop, None);
    assert_eq!(names(&plan), ["verify", "reconcile", "clear-flag"]);
    // Another error beside the findings is still the doctor's trouble to name, after the retirement is finished.
    w.doctor = doctor(vec![
        untrusted_block(),
        diag(Severity::Error, "PRIKK-DOCTOR-OBJECT-MISSING", None),
    ]);
    assert_eq!(build(&w).stop, Some(Stop::RetirementUnfinished));
}

#[test]
fn a_pending_cleanup_beside_publication_trust_findings_nothing_excused_is_a_stop_with_its_own_word()
{
    // Q6. A retired key's blocks are reported by `doctor`; while the cleanup is pending `verify` fails for the ref
    // publication too, so no retirement's record excuses them (the signer says no). The plan does not offer the
    // step, and says why in a word of its own, where it used to say `doctor-trouble`.
    let mut w = cleanup_pending();
    w.doctor = doctor(vec![cleanup_error(), untrusted_block()]);
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::CleanupBesideTrustFindings));
    assert!(plan.steps.is_empty());
    assert_eq!(
        Stop::CleanupBesideTrustFindings.as_str(),
        "cleanup-beside-trust-findings"
    );
    assert!(
        Stop::CleanupBesideTrustFindings
            .meaning()
            .contains("ref publication")
    );
    // Another code beside the cleanup is still the doctor's trouble.
    w.doctor = doctor(vec![
        cleanup_error(),
        diag(Severity::Error, "PRIKK-DOCTOR-OBJECT-MISSING", None),
    ]);
    assert_eq!(build(&w).stop, Some(Stop::DoctorTrouble));
    // And the cleanup alone (the findings taken out because the signer excused them) is the step.
    w.doctor = doctor(vec![cleanup_error()]);
    assert_eq!(build(&w).stop, None);
}

// ---- a view creation that never ended (handoff 005 K4) ----

const DRAFT: &str = "heads/draft";
const FORKED_AT: char = 'f';

/// prikk's own words for an interrupted publication of `view` (measured on 0.55.0 for a killed `branch create`).
fn lead_error(view: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "PRIKK-VERIFY-REF-POINTER-LEADS-LOG".to_owned(),
        message: format!(
            "authoritative pointer leads committed ref log by one transition; run `prikk ref complete {view}`"
        ),
        recommendation: Some(format!(
            "run `prikk ref complete {view}`; doctor does not sign or append"
        )),
    }
}

fn view_act(shown: ViewShown) -> ViewActWitness {
    ViewActWitness {
        act: ViewActId::new(),
        view: DRAFT.to_owned(),
        from_block: FORKED_AT.to_string().repeat(64),
        shown,
    }
}

fn at_forked_block() -> ViewShown {
    ViewShown::Shown {
        tip: FORKED_AT.to_string().repeat(64),
    }
}

/// A flagged wiki whose only unfinished act is a view creation, shown as given.
fn view_creation(shown: ViewShown, lead: bool) -> Witness {
    let mut w = flagged();
    w.view_acts = vec![view_act(shown)];
    if lead {
        w.doctor = doctor(vec![lead_error(DRAFT)]);
    }
    w
}

#[test]
fn a_view_creation_whose_view_is_there_and_whose_publication_is_whole_ends_created() {
    let w = view_creation(at_forked_block(), false);
    let act = nth(&w.view_acts, 0).act.clone();
    let plan = build(&w);
    assert_eq!(plan.stop, None, "{:#?}", plan.lines());
    assert_eq!(
        names(&plan),
        ["end-view-created", "verify", "reconcile", "clear-flag"]
    );
    assert!(matches!(
        plan.steps.first(),
        Some(Step::EndViewCreated { act: a, view }) if *a == act && view == DRAFT
    ));
    // The signer is asked for nothing.
    assert!(plan.ops().is_empty());
    assert_eq!(plan.view.as_deref(), Some(DRAFT));
    assert!(
        plan.lines().iter().any(|l| l.contains(
            "never ended (prikk shows: the view, at the block it was forked from; doctor: its publication is not named)"
        )),
        "{:#?}",
        plan.lines()
    );
}

#[test]
fn a_view_creation_whose_publication_prikk_names_unfinished_is_completed_with_ref_complete_and_then_ends_created()
 {
    let w = view_creation(at_forked_block(), true);
    let act = nth(&w.view_acts, 0).act.clone();
    let plan = build(&w);
    assert_eq!(plan.stop, None, "{:#?}", plan.lines());
    assert_eq!(
        names(&plan),
        [
            "complete-view-creation",
            "end-view-created",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(matches!(
        plan.steps.first(),
        Some(Step::CompleteViewCreation { act: a, view, block_id })
            if *a == act && view == DRAFT && *block_id == FORKED_AT.to_string().repeat(64)
    ));
    // The signer is bound to exactly that operation for exactly that view, and it is in the recorded plan, which
    // the ledger reads back.
    let op = RecoveryOp::CompleteViewCreation {
        creation: act.to_string(),
        view: DRAFT.to_owned(),
    };
    assert_eq!(plan.ops(), vec![op.clone()]);
    assert!(
        plan.to_json()
            .contains("\"step\":\"complete-view-creation\"")
    );
    let facts = facts_of(&w.wiki, &plan.to_json()).expect("facts");
    assert!(facts.allows(&op));
    assert!(!facts.allows(&RecoveryOp::CompleteViewCreation {
        creation: ViewActId::new().to_string(),
        view: DRAFT.to_owned(),
    }));
    assert!(!facts.allows(&RecoveryOp::CompleteViewCreation {
        creation: act.to_string(),
        view: "heads/other".to_owned(),
    }));
    // It is not the step of a killed `seal`.
    assert!(!facts.allows(&RecoveryOp::CompletePublication {
        acceptance: act.to_string(),
        view: DRAFT.to_owned(),
    }));
    assert!(
        plan.lines().iter().any(|l| l
            .contains("step 1: run ref complete heads/draft once, as prikk's doctor recommends")),
        "{:#?}",
        plan.lines()
    );
}

#[test]
fn a_view_creation_that_needs_the_signer_waits_for_an_unfinished_retirement_as_a_queued_patch_does()
{
    // Completing the creation appends a record the maintainer key signs, and nothing new is signed while a key
    // retirement is unfinished (W1; handoff 005 K19): the plan stops with the retirement's own word and command, plans
    // no step and asks the signer for nothing.
    let mut w = view_creation(at_forked_block(), true);
    w.retirement_unfinished = Some(UNFINISHED.to_owned());
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::RetirementUnfinished));
    assert!(plan.steps.is_empty() && plan.ops().is_empty());
    assert!(plan.notes.iter().any(|n| n.contains("finish-retirement")));
    // A creation that signs nothing (the publication is whole, or the view is not there) is not held up by it.
    for (shown, first) in [
        (at_forked_block(), "end-view-created"),
        (ViewShown::Absent, "end-view-failed"),
    ] {
        let mut w = view_creation(shown, false);
        w.retirement_unfinished = Some(UNFINISHED.to_owned());
        let plan = build(&w);
        assert_eq!(plan.stop, None, "{:#?}", plan.lines());
        assert_eq!(names(&plan).first().copied(), Some(first));
    }
}

#[test]
fn a_view_creation_whose_view_is_not_there_and_nothing_is_unfinished_ends_failed() {
    let w = view_creation(ViewShown::Absent, false);
    let plan = build(&w);
    assert_eq!(plan.stop, None, "{:#?}", plan.lines());
    assert_eq!(
        names(&plan),
        ["end-view-failed", "verify", "reconcile", "clear-flag"]
    );
    assert!(plan.ops().is_empty());
}

#[test]
fn a_view_creation_in_any_other_state_is_a_stop_with_its_own_word_and_nothing_is_done() {
    let stop_of = |w: &Witness| {
        let plan = build(w);
        assert!(
            plan.steps.is_empty() && plan.ops().is_empty(),
            "{:?}",
            names(&plan)
        );
        plan.stop
    };
    let other_block = ViewShown::Shown {
        tip: "e".repeat(64),
    };
    let wrong: Vec<(&str, Witness)> = vec![
        (
            "the view is at another block",
            view_creation(other_block.clone(), false),
        ),
        (
            "the view is at another block, beside its own lead",
            view_creation(other_block, true),
        ),
        (
            "the view's block cannot be read",
            view_creation(ViewShown::Unreadable("x".to_owned()), false),
        ),
        (
            "the view's block cannot be read, beside its own lead",
            view_creation(ViewShown::Unreadable("x".to_owned()), true),
        ),
        (
            "the view is not listed while doctor names its publication",
            view_creation(ViewShown::Absent, true),
        ),
        (
            "a view of the name in another case is there",
            view_creation(ViewShown::InAnotherCase, false),
        ),
        (
            "a view of the name in another case is there, beside a lead",
            view_creation(ViewShown::InAnotherCase, true),
        ),
        ("a patch is queued beside it", {
            let mut w = view_creation(at_forked_block(), false);
            w.status = status(vec![patch(MESSAGE)], 0);
            w
        }),
        ("a pending cleanup beside it", {
            let mut w = view_creation(at_forked_block(), false);
            w.doctor = doctor(vec![cleanup_error()]);
            w.cleanup_pending = Some(CleanupPending {
                status_refused: true,
                published: None,
            });
            w
        }),
        ("an acknowledged damage beside it", {
            let mut w = view_creation(at_forked_block(), false);
            w.doctor = doctor(vec![diag(Severity::Error, ACKNOWLEDGED_DAMAGE_CODE, None)]);
            w
        }),
    ];
    for (what, w) in wrong {
        assert_eq!(stop_of(&w), Some(Stop::ViewCreationUnclear), "{what}");
    }
    // The stop is named in the plan's text with its meaning, and the flag stays.
    let lines = build(&view_creation(ViewShown::Absent, true)).lines();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("STOP (view-creation-unclear)")),
        "{lines:#?}"
    );
    assert_eq!(Stop::ViewCreationUnclear.as_str(), "view-creation-unclear");
}

#[test]
fn only_the_lead_of_the_creations_own_ref_in_prikks_own_words_is_accounted_for() {
    let stop_with = |diagnostics: Vec<Diagnostic>| {
        let mut w = view_creation(at_forked_block(), false);
        w.doctor = doctor(diagnostics);
        build(&w).stop
    };
    // A lead for another ref is trouble, and so is a second error beside the right one.
    assert_eq!(
        stop_with(vec![lead_error("heads/other")]),
        Some(Stop::DoctorTrouble)
    );
    assert_eq!(
        stop_with(vec![lead_error(DRAFT), lead_error("heads/other")]),
        Some(Stop::DoctorTrouble)
    );
    // One lead stands for the one creation; a second one of the same ref is something prikk says twice.
    assert_eq!(
        stop_with(vec![lead_error(DRAFT), lead_error(DRAFT)]),
        Some(Stop::DoctorTrouble)
    );
    assert_eq!(
        stop_with(vec![
            lead_error(DRAFT),
            diag(Severity::Error, "PRIKK-VERIFY-REF-DIVERGENCE", None)
        ]),
        Some(Stop::DoctorTrouble)
    );
    // The same code with another severity, another wording, or a message naming another ref, is not the lead.
    let mut warning = lead_error(DRAFT);
    warning.severity = Severity::Warning;
    assert_eq!(stop_with(vec![warning]), Some(Stop::DoctorTrouble));
    let mut reworded = lead_error(DRAFT);
    reworded.recommendation = Some("run `prikk seal --allow-no-audit` for the ref".to_owned());
    assert_eq!(stop_with(vec![reworded]), Some(Stop::DoctorTrouble));
    let mut mismatched = lead_error(DRAFT);
    mismatched.message =
        "authoritative pointer leads committed ref log by one transition; run `prikk ref complete heads/other`"
            .to_owned();
    assert_eq!(stop_with(vec![mismatched]), Some(Stop::DoctorTrouble));
    // Without a view creation, a lead is trouble as it always was.
    let mut w = flagged();
    w.doctor = doctor(vec![lead_error(DRAFT)]);
    assert_eq!(build(&w).stop, Some(Stop::DoctorTrouble));
}

#[test]
fn a_view_creation_beside_another_unfinished_act_or_a_foreign_process_stops_as_every_step_does() {
    let mut w = view_creation(at_forked_block(), false);
    w.view_acts.push(view_act(ViewShown::Absent));
    assert_eq!(build(&w).stop, Some(Stop::SeveralActs));
    let mut w = view_creation(at_forked_block(), false);
    w.acts = vec![act(Chain::NotSealed)];
    assert_eq!(build(&w).stop, Some(Stop::SeveralActs));
    let mut w = view_creation(at_forked_block(), false);
    w.foreign = vec![Foreign {
        pid: 4_242,
        tool: "prikk".to_owned(),
    }];
    assert_eq!(build(&w).stop, Some(Stop::ForeignProcess));
}

#[test]
fn a_worktree_that_is_not_the_baseline_is_put_back_for_the_current_branch_not_the_new_view() {
    let mut w = view_creation(at_forked_block(), false);
    w.worktree = Some(WorktreeState {
        clean: false,
        changes: 1,
        declarations: 0,
    });
    let plan = build(&w);
    assert_eq!(
        names(&plan),
        [
            "end-view-created",
            "restore-worktree",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(matches!(
        plan.steps.get(1),
        Some(Step::RestoreWorktree { view }) if view == VIEW
    ));
}

// ---- taking an orphan queued patch back, on a person's word (maintenance M3; prikk 0.57.0; ledger PK-43)

fn uncommit_plan_for(patch_id: &str, target: &str) -> inklkunn_signer::recover::text::UncommitPlan {
    inklkunn_signer::recover::text::UncommitPlan {
        patch_id: patch_id.to_owned(),
        target: target.to_owned(),
        lines: vec![
            "message: m".to_owned(),
            "  edit-text pages/Home.md".to_owned(),
        ],
    }
}

/// The state: one sound patch queued, no act describes it.
fn orphan() -> (Witness, String) {
    let mut w = flagged();
    let p = patch(MESSAGE);
    let id = p.patch_id.clone();
    w.status = status(vec![p], 0);
    (w, id)
}

fn offered(id: &str) -> DiscardInput {
    DiscardInput {
        offer: DiscardOffer::Offered(uncommit_plan_for(id, VIEW)),
        confirmed: None,
    }
}

#[test]
fn an_orphan_queued_patch_is_offered_and_never_taken_back_without_the_persons_word() {
    let (w, id) = orphan();
    // The offer alone: still the stop, no step, no operation for the signer; the note carries prikk's own words and
    // the command that is the confirmation.
    let plan = build_with(&w, &offered(&id));
    assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(plan.steps.is_empty() && plan.ops().is_empty());
    let note = plan
        .notes
        .iter()
        .find(|n| n.contains("prikk can take the queued patch"))
        .expect("the offer is a note");
    assert!(
        note.contains(&format!("--discard-queued-patch {id}")) && note.contains("message: m | "),
        "{note}"
    );
    // The same state with no offer at all (every plan before 0.57.0): exactly the stop it always was.
    let plain = build(&w);
    assert_eq!(plain.stop, Some(Stop::ForeignQueuedPatch));
    assert!(plain.steps.is_empty() && plain.notes.iter().all(|n| !n.contains("uncommit")));
    assert_eq!(plain.witnesses, plan.witnesses);
}

#[test]
fn the_persons_word_is_the_patchs_id_and_it_takes_that_patch_back_and_puts_the_worktree_right() {
    let (w, id) = orphan();
    let mut d = offered(&id);
    d.confirmed = Some(id.clone());
    let plan = build_with(&w, &d);
    assert_eq!(plan.stop, None);
    assert_eq!(
        names(&plan),
        [
            "discard-queued",
            "restore-worktree",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(matches!(
        plan.steps.first(),
        Some(Step::DiscardQueued { patch_id }) if *patch_id == id
    ));
    // The signer is bound to that patch and to putting the worktree back; nothing more.
    assert_eq!(
        plan.ops(),
        vec![
            RecoveryOp::DiscardQueued { patch: id.clone() },
            RecoveryOp::RestoreWorktree {
                view: VIEW.to_owned()
            }
        ]
    );
    // What the record keeps names the patch, and reads back as exactly that permission.
    let json = plan.to_json();
    assert!(
        json.contains("discard-queued") && json.contains(&id),
        "{json}"
    );
    let facts = facts_of(&w.wiki, &json).expect("a plan of this version");
    assert_eq!(facts.ops, plan.ops());
    assert!(facts.allows(&RecoveryOp::DiscardQueued { patch: id.clone() }));
    assert!(!facts.allows(&RecoveryOp::DiscardQueued {
        patch: "b".repeat(64)
    }));
    assert!(!facts.allows(&RecoveryOp::DiscardDamaged));
    // The plan says so for the person: the step and the note.
    let lines = plan.lines().join("\n");
    assert!(
        lines.contains("a person confirmed taking the queued patch")
            && lines.contains("prikk uncommit"),
        "{lines}"
    );
}

#[test]
fn a_word_that_is_not_the_queued_patchs_id_or_has_no_offer_behind_it_takes_nothing_back() {
    let (w, id) = orphan();
    // Another id, a prefix, upper case: none is the patch.
    for given in [
        "b".repeat(64),
        id[..8].to_owned(),
        id.to_uppercase(),
        String::new(),
    ] {
        let mut d = offered(&id);
        d.confirmed = Some(given.clone());
        let plan = build_with(&w, &d);
        assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch), "{given}");
        assert!(plan.steps.is_empty() && plan.ops().is_empty(), "{given}");
        assert!(
            plan.notes
                .iter()
                .any(|n| n.contains("names a patch that is not the queued one")),
            "{given}: {:?}",
            plan.notes
        );
    }
    // The right id with no offer behind it (a prikk before 0.57.0, or a plan that could not be read): ignored, and said.
    for offer in [
        DiscardOffer::NoVerb,
        DiscardOffer::Unreadable("prikk's output could not be read".to_owned()),
        DiscardOffer::NotAsked,
    ] {
        let plan = build_with(
            &w,
            &DiscardInput {
                offer: offer.clone(),
                confirmed: Some(id.clone()),
            },
        );
        assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch), "{offer:?}");
        assert!(plan.steps.is_empty() && plan.ops().is_empty(), "{offer:?}");
        // M4: the word typed is never left unaccounted for, whatever the reason it was not used.
        assert!(
            plan.notes
                .iter()
                .any(|n| n.starts_with(&format!("`--discard-queued-patch {id}` was not used: "))),
            "{offer:?}: {:?}",
            plan.notes
        );
        assert!(plan.discard_unused.is_some(), "{offer:?}");
    }
    // An offer for another patch than the queued one is not an offer for this one.
    let mut d = offered(&"c".repeat(64));
    d.confirmed = Some(id.clone());
    assert_eq!(build_with(&w, &d).stop, Some(Stop::ForeignQueuedPatch));
    // A queue that targets another branch than the worktree's is not taken back.
    let mut elsewhere = w.clone();
    elsewhere.status.queue.target_ref = Some("heads/other".to_owned());
    let mut d = offered(&id);
    d.confirmed = Some(id.clone());
    let plan = build_with(&elsewhere, &d);
    assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(plan.steps.is_empty());
}

#[test]
fn the_persons_word_turns_no_other_state_into_a_take_back() {
    // The permission is for one state: a sound patch queued with no act describing it, alone. In every other state the
    // plan is what it was; the id typed is not looked at.
    let confirm = |w: &Witness, id: &str| {
        build_with(
            w,
            &DiscardInput {
                offer: DiscardOffer::Offered(uncommit_plan_for(id, VIEW)),
                confirmed: Some(id.to_owned()),
            },
        )
    };
    let (mut w, id) = orphan();
    // The patch is the change of the unfinished act: it is sealed, not discarded.
    w.acts = vec![act(Chain::NotSealed)];
    let p = confirm(&w, &id);
    assert_eq!(nth(&names(&p), 0), &"seal-queued");
    assert!(
        p.steps
            .iter()
            .all(|s| !matches!(s, Step::DiscardQueued { .. }))
    );
    // It is queued beside an act it is not the change of.
    w.status = status(vec![patch("Something else\n\nKind: proposal\n")], 0);
    let p = confirm(&w, &"a".repeat(64));
    assert_eq!(p.stop, Some(Stop::QueuedPatchIsNotTheAct));
    // Two patches are queued: `uncommit` takes the newest only, and a person has not named which.
    let (mut w, id) = orphan();
    w.status = status(vec![patch(MESSAGE), patch(MESSAGE)], 0);
    assert_eq!(confirm(&w, &id).stop, Some(Stop::SeveralQueued));
    // A torn log, an interrupted checkout, a publication a killed seal left: other rows.
    let (mut w, id) = orphan();
    w.status.provisional_worktree = Some(serde_json::json!({"ref": "heads/main"}));
    assert_eq!(confirm(&w, &id).stop, Some(Stop::InterruptedState));
    let (mut w, id) = orphan();
    w.foreign = vec![Foreign {
        pid: 7,
        tool: "prikk".to_owned(),
    }];
    assert_eq!(confirm(&w, &id).stop, Some(Stop::ForeignProcess));
    // Nothing is queued: nothing to take back, and no step invented for the id.
    let w = flagged();
    let p = confirm(&w, &"a".repeat(64));
    assert!(
        p.steps
            .iter()
            .all(|s| !matches!(s, Step::DiscardQueued { .. }))
    );
}

// ---- the audit trail says what happened; a plan shown writes nothing; a stale lock is not a dead end (M4 - M7)

fn typed(w: &Witness, offer: DiscardOffer, given: &str) -> Plan {
    build_with(
        w,
        &DiscardInput {
            offer,
            confirmed: Some(given.to_owned()),
        },
    )
}

#[test]
fn a_word_that_no_step_uses_is_said_to_be_unused_with_its_reason_and_one_that_is_used_is_not() {
    // M4. The id typed is the person's request; whether a patch is taken back is what the plan then finds. Every plan
    // in which no `discard-queued` step results says so, with the reason, in the notes and in `discard_unused`.
    let (w, id) = orphan();
    let reasons = |plan: &Plan| {
        plan.notes
            .iter()
            .filter(|n| n.starts_with("`--discard-queued-patch"))
            .cloned()
            .collect::<Vec<_>>()
    };
    // Used: the take-back is a step, and nothing says it was not used.
    let used = typed(&w, DiscardOffer::Offered(uncommit_plan_for(&id, VIEW)), &id);
    assert!(
        used.steps
            .iter()
            .any(|s| matches!(s, Step::DiscardQueued { .. }))
    );
    assert!(reasons(&used).is_empty() && used.discard_unused.is_none());
    // No word: nothing to say.
    assert!(build_with(&w, &offered(&id)).discard_unused.is_none());

    // The patch is the unfinished act's own, and is sealed: the word is not used.
    let mut sealed = orphan().0;
    sealed.acts = vec![act(Chain::NotSealed)];
    let plan = typed(&sealed, DiscardOffer::NotAsked, &id);
    assert_eq!(nth(&names(&plan), 0), &"seal-queued");
    assert_eq!(reasons(&plan).len(), 1, "{:?}", plan.notes);
    assert!(
        nth(&reasons(&plan), 0).contains("an act is unfinished")
            && nth(&reasons(&plan), 0).contains(&id),
        "{:?}",
        plan.notes
    );
    // Nothing is queued.
    let none = typed(&flagged(), DiscardOffer::NotAsked, &id);
    assert!(
        nth(&reasons(&none), 0).contains("nothing is queued"),
        "{:?}",
        none.notes
    );
    // Several patches.
    let mut several = orphan().0;
    several.status = status(vec![patch(MESSAGE), patch(MESSAGE)], 0);
    let plan = typed(&several, DiscardOffer::NotAsked, &id);
    assert!(nth(&reasons(&plan), 0).contains("more than one patch"));
    // The offer was not honoured: another branch; a name that is not the queued patch's; no verb.
    let mut elsewhere = orphan().0;
    elsewhere.status.queue.target_ref = Some("heads/other".to_owned());
    let plan = typed(
        &elsewhere,
        DiscardOffer::Offered(uncommit_plan_for(&id, VIEW)),
        &id,
    );
    assert!(nth(&reasons(&plan), 0).contains("targets a branch other than"));
    let plan = typed(
        &w,
        DiscardOffer::Offered(uncommit_plan_for(&id, VIEW)),
        &"b".repeat(64),
    );
    assert!(nth(&reasons(&plan), 0).contains("not the queued one"));
    let plan = typed(&w, DiscardOffer::NoVerb, &id);
    assert!(nth(&reasons(&plan), 0).contains("no `uncommit`"));
    // A plan that is only shown does not take a patch back, and says it is why.
    let shown = typed(&w, DiscardOffer::InARun, &id);
    assert!(
        nth(&reasons(&shown), 0).contains("only shows"),
        "{:?}",
        shown.notes
    );
    // Every reason is kept for the end event, and exactly one note carries it.
    for p in [&none, &shown] {
        assert_eq!(reasons(p).len(), 1);
        assert!(p.discard_unused.is_some());
    }
}

#[test]
fn a_plan_that_is_only_shown_asks_nothing_of_prikk_and_says_the_preview_is_shown_in_a_run() {
    // M5 (the library half): the plan built for `--plan` carries no preview of prikk's, the stop it always was, and
    // tells the person where to see it, with the wiki's id in the command.
    let (w, id) = orphan();
    let plan = build_with(
        &w,
        &DiscardInput {
            offer: DiscardOffer::InARun,
            confirmed: None,
        },
    );
    assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(plan.steps.is_empty() && plan.ops().is_empty());
    let note = plan
        .notes
        .iter()
        .find(|n| n.contains("`recover --plan` does not ask prikk"))
        .expect("the note says where the preview is shown");
    assert!(
        note.contains(&format!("inklkunn wiki recover {}`", w.wiki)) && note.contains(&id),
        "{note}"
    );
    assert!(plan.notes.iter().all(|n| !n.contains("prikk says")));
}

#[test]
fn a_stale_lock_beside_an_orphan_patch_is_cleared_and_the_stop_stays() {
    // M6. The stop used to clear every step, the clearing of the lock included, and the lock stood in the way of the
    // preview for good. Now the clearing is the plan's one step and the stop follows it.
    let (mut w, id) = orphan();
    w.locks = vec![lock(
        "active",
        2_147_483_000,
        "unknown (not proof it is safe to clear)",
    )];
    let plan = build_with(
        &w,
        &DiscardInput {
            offer: DiscardOffer::AfterLocks,
            confirmed: None,
        },
    );
    assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
    assert_eq!(names(&plan), ["clear-locks"]);
    assert!(matches!(nth(&plan.ops(), 0), RecoveryOp::ClearLocks { paths } if paths.len() == 1));
    let lines = plan.lines().join("\n");
    assert!(
        lines.contains("step 1:") && lines.contains("STOP (foreign-queued-patch)"),
        "{lines}"
    );
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("the run clears it first")),
        "{:?}",
        plan.notes
    );
    // The same plan with the preview in hand (what the run shows once the lock is gone) still has the stop, and the
    // command, and no step is a take-back without the word.
    w.locks.clear();
    let after = build_with(&w, &offered(&id));
    assert_eq!(after.stop, Some(Stop::ForeignQueuedPatch));
    assert!(after.steps.is_empty());
    // Only the clearing survives a stop: a lock of a kind an acceptance does not leave is still the stop it was.
    let (mut w, _) = orphan();
    w.locks = vec![lock("container:trust-policy", 1, "x")];
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::UnknownLock));
    assert!(plan.steps.is_empty());
    // And a foreign process stops it all, the lock too.
    let (mut w, _) = orphan();
    w.locks = vec![lock("active", 1, "x")];
    w.foreign = vec![Foreign {
        pid: 9,
        tool: "prikk".to_owned(),
    }];
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::ForeignProcess));
    assert!(plan.steps.is_empty());
    // Other stops with a lock held are not changed by this: several queued patches clear nothing.
    let (mut w, _) = orphan();
    w.status = status(vec![patch(MESSAGE), patch(MESSAGE)], 0);
    w.locks = vec![lock("active", 1, "x")];
    let plan = build(&w);
    assert_eq!(plan.stop, Some(Stop::SeveralQueued));
    assert!(plan.steps.is_empty());
}

#[test]
fn the_command_is_offered_only_when_it_would_be_honoured_and_names_the_wiki() {
    // M7. The note is the offer; the confirmation is judged by the same test. A queue that targets another branch, or
    // a plan of prikk's that names another ref, gets the preview and the reason, and no command to type.
    let (w, id) = orphan();
    let plan = build_with(&w, &offered(&id));
    let offer = plan
        .notes
        .iter()
        .find(|n| n.contains("--discard-queued-patch"))
        .expect("the command is offered");
    assert!(
        offer.contains(&format!(
            "inklkunn wiki recover {} --discard-queued-patch {id}",
            w.wiki
        )) && !offer.contains("<wiki>"),
        "{offer}"
    );
    let not_offered = |plan: &Plan| {
        plan.notes
            .iter()
            .all(|n| !n.contains("--discard-queued-patch"))
            && plan
                .notes
                .iter()
                .any(|n| n.contains("Recovery will not take it back"))
            && plan.notes.iter().any(|n| n.contains("message: m | "))
    };
    // The queue targets another branch than the worktree's.
    let mut elsewhere = w.clone();
    elsewhere.status.queue.target_ref = Some("heads/other".to_owned());
    let plan = build_with(&elsewhere, &offered(&id));
    assert!(not_offered(&plan), "{:?}", plan.notes);
    // The queue has no target at all.
    let mut untargeted = w.clone();
    untargeted.status.queue.target_ref = None;
    let plan = build_with(&untargeted, &offered(&id));
    assert!(not_offered(&plan), "{:?}", plan.notes);
    // prikk's own plan names another ref than the one the worktree is on.
    let plan = build_with(
        &w,
        &DiscardInput {
            offer: DiscardOffer::Offered(uncommit_plan_for(&id, "heads/other")),
            confirmed: None,
        },
    );
    assert!(not_offered(&plan), "{:?}", plan.notes);
    assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
    // The offer and the confirmation agree: wherever the command is offered, the word takes the patch back.
    let mut d = offered(&id);
    d.confirmed = Some(id.clone());
    assert!(!build_with(&w, &d).steps.is_empty());
}

// ---- maintenance corrections, second round (M12, M14, M16)

/// The state of M6: one orphan patch and a stale lock held.
fn orphan_with_lock() -> (Witness, String) {
    let (mut w, id) = orphan();
    w.locks = vec![lock(
        "active",
        2_147_483_000,
        "unknown (not proof it is safe to clear)",
    )];
    (w, id)
}

fn with_offer(w: &Witness, offer: DiscardOffer) -> Plan {
    build_with(
        w,
        &DiscardInput {
            offer,
            confirmed: None,
        },
    )
}

#[test]
fn a_stale_lock_beside_an_orphan_patch_is_cleared_only_when_a_preview_can_follow() {
    // M12 (R-e): prikk before 0.57.0 has no `uncommit`, so clearing the lock buys nothing there and the stop clears
    // nothing; the same when the question could not be answered or was not asked.
    let (w, id) = orphan_with_lock();
    for offer in [DiscardOffer::InARun, DiscardOffer::AfterLocks] {
        let plan = with_offer(&w, offer.clone());
        assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
        assert_eq!(names(&plan), ["clear-locks"], "{offer:?}");
    }
    for offer in [
        DiscardOffer::NoVerb,
        DiscardOffer::Unreadable("no".to_owned()),
        DiscardOffer::NotAsked,
    ] {
        let plan = with_offer(&w, offer.clone());
        assert_eq!(plan.stop, Some(Stop::ForeignQueuedPatch));
        assert!(
            plan.steps.is_empty() && plan.ops().is_empty(),
            "{offer:?}: {:?}",
            plan.steps
        );
        assert!(
            plan.notes
                .iter()
                .any(|n| n.contains("a stale lock is held") && n.contains("nothing is cleared")),
            "{offer:?}: {:?}",
            plan.notes
        );
    }
    // The word typed on a prikk without the verb is not used, and no step is planned for it.
    let said = typed(&w, DiscardOffer::NoVerb, &id);
    assert!(said.steps.is_empty());
    assert!(said.discard_unused.is_some());
}

#[test]
fn no_other_stop_clears_a_lock() {
    // M12: a stop that waits for a person clears nothing; the orphan patch's preview is the one exception.
    let held = || vec![lock("active", 2_147_483_000, "x")];
    let mut cases: Vec<(Witness, Stop)> = Vec::new();
    let (mut w, _) = orphan();
    w.status = status(vec![patch(MESSAGE), patch(MESSAGE)], 0);
    w.locks = held();
    cases.push((w, Stop::SeveralQueued));
    let mut w = flagged();
    w.status = status(vec![patch("Something else\n\nKind: proposal\n")], 0);
    w.acts = vec![act(Chain::NotSealed)];
    w.locks = held();
    cases.push((w, Stop::QueuedPatchIsNotTheAct));
    let mut w = flagged();
    w.acts = vec![act(Chain::TipMoved)];
    w.locks = held();
    cases.push((w, Stop::TipMoved));
    let mut w = flagged();
    w.doctor = doctor(vec![diag(
        Severity::Error,
        "PRIKK-DOCTOR-OBJECT-MISSING",
        None,
    )]);
    w.locks = held();
    cases.push((w, Stop::DoctorTrouble));
    for (w, stop) in cases {
        let plan = with_offer(&w, DiscardOffer::AfterLocks);
        assert_eq!(plan.stop, Some(stop), "{:#?}", plan.lines());
        assert!(
            plan.steps.is_empty() && plan.ops().is_empty(),
            "{stop:?} cleared something: {:?}",
            plan.steps
        );
    }
}

#[test]
fn a_plan_promises_the_command_only_when_a_run_would_offer_it() {
    // M12 (R-f): the queue targets the worktree's branch, or the notes promise no command and the word says why.
    let (w, id) = orphan_with_lock();
    for offer in [DiscardOffer::InARun, DiscardOffer::AfterLocks] {
        let on = with_offer(&w, offer.clone());
        let notes = on.notes.join("\n");
        assert!(
            notes.contains("the command that does it") || notes.contains("and the command"),
            "{offer:?}: {notes}"
        );
        assert!(!notes.contains("offers no command"), "{offer:?}: {notes}");
        let mut elsewhere = w.clone();
        elsewhere.status.queue.target_ref = Some("heads/other".to_owned());
        let off = with_offer(&elsewhere, offer.clone());
        let notes = off.notes.join("\n");
        assert!(
            notes.contains("other than the one the worktree is on")
                && notes.contains("offers no command")
                && !notes.contains("the command that does it")
                && !notes.contains("and the command"),
            "{offer:?}: {notes}"
        );
        let said = typed(&elsewhere, offer.clone(), &id);
        assert!(
            said.discard_unused
                .as_deref()
                .is_some_and(|why| why.contains("other than the one the worktree is on")
                    && !why.contains("the next run takes the patch back")),
            "{offer:?}: {:?}",
            said.discard_unused
        );
    }
}

#[test]
fn a_lone_metadata_debris_warning_is_noted_and_in_nobodys_way() {
    // M16 (PK-46): harmless, and the next commit clears it; the plan used to stop at `doctor-trouble` for it.
    let debris = || {
        diag(
            Severity::Warning,
            "PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS",
            Some("no repair is required"),
        )
    };
    let mut w = flagged();
    w.doctor = doctor(vec![debris()]);
    let plan = build(&w);
    assert_eq!(plan.stop, None, "{:#?}", plan.lines());
    assert_eq!(names(&plan), TAIL);
    assert!(
        plan.notes
            .contains(&"prikk's stale ref metadata; the next accepted change clears it".to_owned()),
        "{:?}",
        plan.notes
    );
    // An unfinished act whose change is in the history is ended as sealed, as without the warning.
    let mut w = flagged();
    w.doctor = doctor(vec![debris()]);
    w.acts = vec![published_act()];
    let plan = build(&w);
    assert_eq!(plan.stop, None, "{:#?}", plan.lines());
    assert_eq!(nth(&names(&plan), 0), &"end-sealed");
    // Nothing else is let through by it: another warning or an error beside it is still trouble.
    for other in [
        diag(Severity::Warning, "PRIKK-DOCTOR-SOMETHING-ELSE", None),
        diag(Severity::Error, "PRIKK-DOCTOR-OBJECT-MISSING", None),
    ] {
        let mut w = flagged();
        w.doctor = doctor(vec![debris(), other]);
        assert_eq!(build(&w).stop, Some(Stop::DoctorTrouble));
    }
    // Beside a pending cleanup the signer's step asks for exactly one finding, so the plan does not offer it.
    let mut w = cleanup_pending();
    w.doctor = doctor(vec![cleanup_error(), debris()]);
    assert_eq!(build(&w).stop, Some(Stop::DoctorTrouble));
}

#[test]
fn the_cleanup_pending_step_says_that_the_queues_content_is_checked() {
    // M14 (R-i): prikk reports the code only when the queue equals the tip block's patches (PK-44).
    let plan = build(&cleanup_pending());
    let lines = plan.lines().join("\n");
    assert!(
        lines.contains("checks the queue's content as well as its size")
            && lines.contains("PK-44")
            && lines.contains("exactly the one published patch"),
        "{lines}"
    );
}
