#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::time::Duration;

use inklkunn_core::lease::{Busy, BusyWhy};
use inklkunn_core::proposal::QuotaLimit;
use inklkunn_store::{AcceptanceId, RecoveryFlag, RecoveryReason, WikiId};

use super::*;
use crate::catalog::{EN, T};

fn body(problem: &Problem) -> serde_json::Value {
    problem.body(T::new(EN))
}

/// A member of a problem's body; absent is `null` (indexing is denied by the workspace's lints).
fn at<'a>(body: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    static NULL: serde_json::Value = serde_json::Value::Null;
    body.get(name).unwrap_or(&NULL)
}

fn refusal(limit: QuotaLimit, reset: Reset) -> QuotaRefusal {
    QuotaRefusal {
        limit,
        allowed: 30,
        current: 30,
        asked: 1,
        reset,
    }
}

#[test]
fn a_rate_that_ends_at_a_time_says_how_long_to_wait_and_a_limit_that_waiting_cannot_lift_does_not()
{
    let now = 1_000;
    let rate = quota_problem(
        &refusal(
            QuotaLimit::ProposalsPerHourPerPrincipal,
            Reset::At(1_000 + 1_800),
        ),
        now,
    );
    assert_eq!(rate.code, Code::RefusedLimit);
    assert_eq!(rate.retry_after_secs(), Some(1_800));
    let b = body(&rate);
    assert_eq!(at(&b, "limit"), "proposals-per-hour-per-principal");
    assert_eq!(at(&b, "allowed"), 30);
    assert_eq!(at(&b, "current"), 30);
    assert_eq!(at(&b, "asked"), 1);
    assert_eq!(at(&b, "reset"), "at");
    assert_eq!(at(&b, "reset_at"), 2_800);
    assert_eq!(at(&b, "status"), 429);

    // A reset time that has passed is still told as at least one second.
    let passed = quota_problem(
        &refusal(QuotaLimit::ProposalsPerHourPerWiki, Reset::At(5)),
        now,
    );
    assert_eq!(passed.retry_after_secs(), Some(1));

    let queue = quota_problem(
        &refusal(QuotaLimit::AcceptanceQueue, Reset::WhenTheQueueMoves),
        now,
    );
    assert_eq!(queue.retry_after_secs(), Some(QUEUE_RETRY));
    for (limit, reset, word) in [
        (
            QuotaLimit::OpenProposals,
            Reset::WhenAProposalCloses,
            "when-a-proposal-closes",
        ),
        (QuotaLimit::RevisionsPerProposal, Reset::Never, "never"),
    ] {
        let p = quota_problem(&refusal(limit, reset), now);
        assert_eq!(p.retry_after_secs(), None, "{word}");
        assert_eq!(at(&body(&p), "reset"), word);
        assert_eq!(at(&body(&p), "reset_at"), &serde_json::Value::Null);
    }
}

#[test]
fn every_reason_a_proposal_can_be_unacceptable_has_a_distinct_stable_word() {
    use Invalid::*;
    let all = [
        SummaryMissing,
        SummaryTooLong,
        SummaryNotOneLine,
        TooManySources,
        BadSource,
        NothingProposed,
        TooLarge,
        PageTooLarge,
        BadPath,
        DuplicatePath,
        RenameToSelf,
        PathOutsidePages,
        PathNotCanonical,
        PathCollides,
        FrontUnreadable,
        BadBase,
        ReasonMissing,
        ReasonTooLong,
        CommentEmpty,
        CommentTooLong,
        CommentNul,
        BadAnchor,
        SupersedeSelf,
        GovernancePath,
        GovernanceMixed,
    ];
    let words: std::collections::BTreeSet<&str> = all.iter().map(|w| invalid_word(*w)).collect();
    assert_eq!(words.len(), all.len());
    for word in words {
        assert!(
            word.split('-')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase())),
            "{word}"
        );
    }
}

fn failure_problem(failure: AcceptFailure) -> (Problem, Vec<String>) {
    let logged = RefCell::new(Vec::new());
    let problem = acceptance_problem(failure, &|line| logged.borrow_mut().push(line.to_owned()));
    (problem, logged.into_inner())
}

fn governance_fault() -> inklkunn_core::governance::Fault {
    inklkunn_core::governance::Fault {
        line: Some(4),
        ..inklkunn_core::governance::Fault::at(
            inklkunn_core::governance::GovernanceFile::Policy,
            inklkunn_core::governance::Refusal::UnknownKey,
            "rungs",
        )
    }
}

fn unreadable_rules() -> inklkunn_core::governance::ReadFault {
    inklkunn_core::governance::ReadFault::from_stored(
        inklkunn_core::governance::GovernanceFile::Constitution,
        Some(2),
        inklkunn_core::governance::Refusal::WrongShape,
    )
}

fn knowledge_refusal() -> inklkunn_core::knowledge::check::Refusal {
    inklkunn_core::knowledge::check::Refusal::of(
        "knowledge/stray.txt",
        inklkunn_core::knowledge::Fault::whole(inklkunn_core::knowledge::Rule::Path),
    )
}

#[test]
fn each_way_an_acceptance_can_stop_is_one_problem_a_program_can_act_on() {
    let flag = RecoveryFlag {
        wiki: WikiId::new(),
        reason: RecoveryReason::Queued,
        act: None,
        since: 0,
    };
    let busy = Busy {
        depth: 3,
        estimate: Duration::from_secs(12),
        why: BusyWhy::Waited,
    };
    let cases: Vec<(AcceptFailure, Code, Option<u64>)> = vec![
        (AcceptFailure::NotAllowed, Code::Forbidden, None),
        (AcceptFailure::NotFound, Code::NotFound, None),
        (AcceptFailure::Busy(busy), Code::Busy, Some(12)),
        (
            AcceptFailure::NeedsRecovery(flag.clone()),
            Code::NeedsRecovery,
            Some(RECOVERY_RETRY),
        ),
        (
            AcceptFailure::InProgress {
                acceptance: AcceptanceId::new(),
            },
            Code::AcceptanceInProgress,
            None,
        ),
        (AcceptFailure::Overlap { overlaps: 2 }, Code::Overlap, None),
        (AcceptFailure::NothingToChange, Code::NothingToChange, None),
        (
            AcceptFailure::Failed {
                acceptance: AcceptanceId::new(),
                word: "refused",
                detail: "secret detail".to_owned(),
            },
            Code::AcceptanceFailed,
            None,
        ),
        (
            AcceptFailure::Incomplete {
                acceptance: AcceptanceId::new(),
                reason: RecoveryReason::Queued,
                flagged: true,
                detail: "secret detail".to_owned(),
            },
            Code::NeedsRecovery,
            Some(RECOVERY_RETRY),
        ),
        (
            AcceptFailure::Unavailable("secret cause".to_owned()),
            Code::Unavailable,
            None,
        ),
        (
            AcceptFailure::KnowledgeInvalid(vec![knowledge_refusal()]),
            Code::AcceptanceFailed,
            None,
        ),
        (
            AcceptFailure::GovernanceInvalid(vec![governance_fault()]),
            Code::Unacceptable,
            None,
        ),
        (
            AcceptFailure::GovernanceRung(inklkunn_store::Role::Admin),
            Code::Forbidden,
            None,
        ),
        (
            AcceptFailure::ApprovalsMissing { needed: 2, have: 1 },
            Code::Forbidden,
            None,
        ),
        (protected_failure(), Code::Forbidden, None),
        (
            AcceptFailure::RulesUnreadable(unreadable_rules()),
            Code::Forbidden,
            None,
        ),
    ];
    for (failure, code, wait) in cases {
        let shown = failure.to_string();
        let (problem, logged) = failure_problem(failure);
        assert_eq!(problem.code, code, "{shown}");
        assert_eq!(problem.retry_after_secs(), wait, "{shown}");
        // What is the operator's goes to the log and never to the caller.
        for line in &logged {
            assert!(!body(&problem).to_string().contains(line), "{shown}");
        }
        assert!(
            !body(&problem).to_string().contains("secret"),
            "{shown}: {}",
            body(&problem)
        );
    }
    let (busy, _) = failure_problem(AcceptFailure::Busy(Busy {
        depth: 3,
        estimate: Duration::from_secs(12),
        why: BusyWhy::Waited,
    }));
    assert_eq!(at(&body(&busy), "waiting"), 3);
    assert!(
        at(&body(&busy), "detail")
            .as_str()
            .unwrap()
            .contains("ahead of this one: 3"),
        "{}",
        body(&busy)
    );
    let (overlap, _) = failure_problem(AcceptFailure::Overlap { overlaps: 2 });
    assert_eq!(at(&body(&overlap), "overlaps"), 2);
    // A knowledge file the change would write is refused before the signer: the caller gets the word, the files and
    // rules go to the log.
    let (refused, logged) =
        failure_problem(AcceptFailure::KnowledgeInvalid(vec![knowledge_refusal()]));
    assert_eq!(at(&body(&refused), "word"), "knowledge-invalid");
    assert_eq!(logged.len(), 1);
    assert!(
        logged.first().unwrap().contains("knowledge/stray.txt"),
        "{logged:?}"
    );
    assert!(!body(&refused).to_string().contains("stray"));
    // The rules of the wiki: what a program needs to correct the file or understand the refusal, as names and numbers.
    let (invalid, logged) =
        failure_problem(AcceptFailure::GovernanceInvalid(vec![governance_fault()]));
    assert_eq!(at(&body(&invalid), "reason"), "governance-invalid");
    let invalid_body = body(&invalid);
    let refusals = at(&invalid_body, "refusals");
    let first = refusals.as_array().and_then(|a| a.first()).unwrap();
    assert_eq!(first["file"], "governance/policy.yaml");
    assert_eq!(first["rule"], "unknown-key");
    assert_eq!(first["field"], "rungs");
    assert_eq!(first["line"], 4);
    assert_eq!(logged.len(), 1);
    assert!(
        logged
            .first()
            .unwrap()
            .contains("governance/policy.yaml, line 4")
    );
    let (rung, _) = failure_problem(AcceptFailure::GovernanceRung(inklkunn_store::Role::Admin));
    assert_eq!(at(&body(&rung), "reason"), "governance-rung");
    assert_eq!(at(&body(&rung), "needs"), "admin");
    let (approvals, _) = failure_problem(AcceptFailure::ApprovalsMissing { needed: 2, have: 1 });
    assert_eq!(at(&body(&approvals), "reason"), "approvals-missing");
    assert_eq!(at(&body(&approvals), "needed"), 2);
    assert_eq!(at(&body(&approvals), "have"), 1);
    let (protected, _) = failure_problem(protected_failure());
    let protected = body(&protected);
    assert_eq!(at(&protected, "reason"), "protected");
    assert_eq!(at(&protected, "lacks"), "role");
    assert_eq!(at(&protected, "needs"), "admin");
    let listed = at(&protected, "protections").as_array().unwrap().clone();
    assert_eq!(listed.len(), 2);
    assert_eq!(
        *listed.first().unwrap(),
        serde_json::json!({
            "kind": "temporary", "on": "page", "title": "Home", "requires": "rung", "role": "admin",
            "reason": "Dispute.", "until": "2023-11-14T22:13:20Z",
        })
    );
    assert_eq!(
        *listed.get(1).unwrap(),
        serde_json::json!({
            "kind": "standing", "on": "namespace", "title": "Policies", "requires": "review", "role": null,
            "reason": "Rules are read first.", "until": null,
        })
    );
    let (review, _) = failure_problem(AcceptFailure::Protected {
        notices: Vec::new(),
        lack: Lack::Review,
    });
    assert_eq!(at(&body(&review), "lacks"), "approval");
    assert!(body(&review).get("needs").is_none());
    // A proposal that replaces a file the wiki cannot read, whose author no longer may moderate (G-T6 correction S-2): the fact, and
    // whether a restriction is the cause, and never the restriction's reason.
    for restricted in [true, false] {
        let (author, logged) = failure_problem(AcceptFailure::AuthorNotAllowed { restricted });
        let author = body(&author);
        assert_eq!(at(&author, "reason"), "author-not-allowed");
        assert_eq!(at(&author, "author_restricted"), restricted);
        assert!(author.get("restriction_reason").is_none());
        assert!(logged.is_empty());
    }
    let (unreadable, logged) = failure_problem(AcceptFailure::RulesUnreadable(unreadable_rules()));
    assert_eq!(at(&body(&unreadable), "reason"), "rules-unreadable");
    assert_eq!(
        at(&body(&unreadable), "file"),
        "governance/constitution.yaml"
    );
    assert_eq!(at(&body(&unreadable), "rule"), "wrong-shape");
    assert_eq!(at(&body(&unreadable), "line"), 2);
    // A fault of any other rule lists nothing: no member `found` and none `found_more`.
    assert!(body(&unreadable).get("found").is_none());
    assert!(body(&unreadable).get("found_more").is_none());
    assert_eq!(logged.len(), 1);
    // Where something stands in the file's place: what it is, in order, and how many more (E-1).
    let stands = inklkunn_core::governance::ReadFault::from_stored(
        inklkunn_core::governance::GovernanceFile::Policy,
        None,
        inklkunn_core::governance::Refusal::NotAFile,
    );
    let (listed, _) = failure_problem(AcceptFailure::RulesUnreadable(stands.clone().with_found(
        vec![
            "governance/policy.yaml/a.txt".to_owned(),
            "governance/policy.yaml/b/c.txt".to_owned(),
        ],
        7,
    )));
    assert_eq!(at(&body(&listed), "rule"), "not-a-file");
    assert_eq!(
        *at(&body(&listed), "found"),
        serde_json::json!([
            "governance/policy.yaml/a.txt",
            "governance/policy.yaml/b/c.txt"
        ])
    );
    assert_eq!(*at(&body(&listed), "found_more"), serde_json::json!(7));
    // With no more, no count.
    let (counted, _) = failure_problem(AcceptFailure::RulesUnreadable(
        stands.with_found(vec!["governance/policy.yaml/a.txt".to_owned()], 0),
    ));
    assert!(body(&counted).get("found_more").is_none());
    assert_eq!(
        at(&body(&counted), "found").as_array().map(Vec::len),
        Some(1)
    );
    let (failed, logged) = failure_problem(AcceptFailure::Failed {
        acceptance: AcceptanceId::new(),
        word: "memory-limit",
        detail: "d".to_owned(),
    });
    assert_eq!(at(&body(&failed), "word"), "memory-limit");
    assert_eq!(logged.len(), 1);
}

#[test]
fn an_acceptance_the_signer_paused_for_an_unfinished_retirement_is_a_problem_of_its_own() {
    // W1: the code `retirement-unfinished`, not `acceptance-failed`: the caller can tell it from a failure that may
    // pass by itself. The command the operator reads goes to the log, never to the caller.
    let (problem, logged) = failure_problem(AcceptFailure::Failed {
        acceptance: AcceptanceId::new(),
        word: "retirement-unfinished",
        detail: "run `inklkunn wiki keys finish-retirement secret-wiki`".to_owned(),
    });
    assert_eq!(problem.code, Code::RetirementUnfinished);
    assert_eq!(problem.code.slug(), "retirement-unfinished");
    assert_eq!(problem.code.status().as_u16(), 503);
    assert_eq!(problem.retry_after_secs(), None);
    assert_eq!(logged.len(), 1);
    assert!(logged.first().unwrap().contains("finish-retirement"));
    let shown = body(&problem).to_string();
    assert!(
        !shown.contains("finish-retirement") && !shown.contains("secret-wiki"),
        "{shown}"
    );
    // A failure with another word is still the plain failure.
    let (plain, _) = failure_problem(AcceptFailure::Failed {
        acceptance: AcceptanceId::new(),
        word: "refused",
        detail: "d".to_owned(),
    });
    assert_eq!(plain.code, Code::AcceptanceFailed);
}

#[test]
fn a_busy_wiki_is_never_told_to_retry_in_no_time() {
    let (busy, _) = failure_problem(AcceptFailure::Busy(Busy {
        depth: 1,
        estimate: Duration::ZERO,
        why: BusyWhy::Waited,
    }));
    assert_eq!(busy.retry_after_secs(), Some(1));
}

/// A refusal by two protections: a temporary one on a page and a standing one on a namespace.
fn protected_failure() -> AcceptFailure {
    use inklkunn_core::governance::{Require, Target};
    use inklkunn_core::protection::Notice;
    AcceptFailure::Protected {
        notices: vec![
            Notice::temporary(
                Target::Path("Home".to_owned()),
                Require::Rung(inklkunn_store::Role::Admin),
                "Dispute.",
                1_700_000_000,
            ),
            Notice::standing(
                Target::Namespace("Policies".to_owned()),
                Require::Review,
                "Rules are read first.",
            ),
        ],
        lack: Lack::Rung(inklkunn_store::Role::Admin),
    }
}
