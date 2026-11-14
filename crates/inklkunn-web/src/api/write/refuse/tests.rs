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
