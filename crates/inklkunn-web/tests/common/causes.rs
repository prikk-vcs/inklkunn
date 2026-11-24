//! **Every way an acceptance can be refused, once** (threat model C-3g: each refusal names its cause; readiness gap 4): the closed
//! list the tests of the web page and of the API walk, so that a cause added to [`AcceptFailure`] fails to compile here
//! ([`cause_of`] matches every variant and has no wildcard) and then fails the test that asks for a sample of each ([`samples`]),
//! and the page and the API are held to say it.
//!
//! Included by each test binary that walks the list (`#[path = "../common/causes.rs"] mod causes;`).
#![allow(dead_code)]

use std::time::Duration;

use inklkunn_core::AcceptFailure;
use inklkunn_core::authorize::RestrictionNotice;
use inklkunn_core::governance::{
    Fault, GovernanceFile, ReadFault, Refusal as GovernanceRefusal, Require, Target,
};
use inklkunn_core::lease::{Busy, BusyWhy};
use inklkunn_core::proposal::{Invalid, ProposalError, QuotaLimit, QuotaRefusal, Reset};
use inklkunn_core::protection::{Lack, Notice};
use inklkunn_store::{
    AcceptanceId, ProposalOp, ProposalState, RecoveryFlag, RecoveryReason, RestrictionEventId,
    Role, StoreError, WikiId,
};

/// The kinds of refusal, one for each way [`AcceptFailure`] can end an acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cause {
    NotAllowed,
    Restricted,
    AuthorNotAllowed,
    NotFound,
    Proposal,
    Busy,
    Refused,
    NeedsRecovery,
    InProgress,
    Overlap,
    NothingToChange,
    KnowledgeInvalid,
    GovernanceInvalid,
    GovernanceRung,
    ApprovalsMissing,
    SummaryHidden,
    Protected,
    RulesUnreadable,
    Failed,
    Incomplete,
    Unavailable,
    Store,
}

/// Every cause. A cause that is in the enum and not here is found by the test that counts them.
pub const ALL: [Cause; 22] = [
    Cause::NotAllowed,
    Cause::Restricted,
    Cause::AuthorNotAllowed,
    Cause::NotFound,
    Cause::Proposal,
    Cause::Busy,
    Cause::Refused,
    Cause::NeedsRecovery,
    Cause::InProgress,
    Cause::Overlap,
    Cause::NothingToChange,
    Cause::KnowledgeInvalid,
    Cause::GovernanceInvalid,
    Cause::GovernanceRung,
    Cause::ApprovalsMissing,
    Cause::SummaryHidden,
    Cause::Protected,
    Cause::RulesUnreadable,
    Cause::Failed,
    Cause::Incomplete,
    Cause::Unavailable,
    Cause::Store,
];

/// The cause of a failure. **No wildcard**: a variant added to [`AcceptFailure`] does not compile here until it is placed.
pub fn cause_of(failure: &AcceptFailure) -> Cause {
    match failure {
        AcceptFailure::NotAllowed => Cause::NotAllowed,
        AcceptFailure::Restricted(_) => Cause::Restricted,
        AcceptFailure::AuthorNotAllowed { .. } => Cause::AuthorNotAllowed,
        AcceptFailure::NotFound => Cause::NotFound,
        AcceptFailure::Proposal(_) => Cause::Proposal,
        AcceptFailure::Busy(_) => Cause::Busy,
        AcceptFailure::Refused(_) => Cause::Refused,
        AcceptFailure::NeedsRecovery(_) => Cause::NeedsRecovery,
        AcceptFailure::InProgress { .. } => Cause::InProgress,
        AcceptFailure::Overlap { .. } => Cause::Overlap,
        AcceptFailure::NothingToChange => Cause::NothingToChange,
        AcceptFailure::KnowledgeInvalid(_) => Cause::KnowledgeInvalid,
        AcceptFailure::GovernanceInvalid(_) => Cause::GovernanceInvalid,
        AcceptFailure::GovernanceRung(_) => Cause::GovernanceRung,
        AcceptFailure::ApprovalsMissing { .. } => Cause::ApprovalsMissing,
        AcceptFailure::SummaryHidden => Cause::SummaryHidden,
        AcceptFailure::Protected { .. } => Cause::Protected,
        AcceptFailure::RulesUnreadable(_) => Cause::RulesUnreadable,
        AcceptFailure::Failed { .. } => Cause::Failed,
        AcceptFailure::Incomplete { .. } => Cause::Incomplete,
        AcceptFailure::Unavailable(_) => Cause::Unavailable,
        AcceptFailure::Store(_) => Cause::Store,
    }
}

/// A restriction as its holder is told of it.
pub fn restriction() -> RestrictionNotice {
    RestrictionNotice {
        id: RestrictionEventId::new(),
        reason: "Too many edits in a day.".to_owned(),
        until: 1_700_086_400,
        by_rung: Role::Maintainer,
        policy_block: None,
    }
}

/// One failure of each cause, and the ones of a cause that says more than one thing (a protection asks a rung or a review; an
/// author is restricted or is not allowed; a rule file cannot be read in one way or another).
pub fn samples() -> Vec<AcceptFailure> {
    vec![
        AcceptFailure::NotAllowed,
        AcceptFailure::Restricted(restriction()),
        AcceptFailure::AuthorNotAllowed { restricted: true },
        AcceptFailure::AuthorNotAllowed { restricted: false },
        AcceptFailure::NotFound,
        AcceptFailure::Proposal(ProposalError::NotInThisState {
            op: ProposalOp::Accept,
            state: ProposalState::Declined,
        }),
        AcceptFailure::Proposal(ProposalError::Invalid(Invalid::GovernanceMixed)),
        AcceptFailure::Busy(Busy {
            depth: 2,
            estimate: Duration::from_secs(45),
            why: BusyWhy::Waited,
        }),
        AcceptFailure::Refused(QuotaRefusal {
            limit: QuotaLimit::AcceptanceQueue,
            allowed: 64,
            current: 64,
            asked: 1,
            reset: Reset::WhenTheQueueMoves,
        }),
        AcceptFailure::NeedsRecovery(RecoveryFlag {
            wiki: WikiId::new(),
            reason: RecoveryReason::Queued,
            act: None,
            since: 0,
        }),
        AcceptFailure::InProgress {
            acceptance: AcceptanceId::new(),
        },
        AcceptFailure::Overlap { overlaps: 2 },
        AcceptFailure::NothingToChange,
        AcceptFailure::KnowledgeInvalid(vec![inklkunn_core::knowledge::check::Refusal::of(
            "knowledge/stray.txt",
            inklkunn_core::knowledge::Fault::whole(inklkunn_core::knowledge::Rule::Path),
        )]),
        AcceptFailure::GovernanceInvalid(vec![Fault {
            line: Some(4),
            ..Fault::at(
                GovernanceFile::Policy,
                GovernanceRefusal::UnknownKey,
                "rungs",
            )
        }]),
        AcceptFailure::GovernanceRung(Role::Admin),
        AcceptFailure::ApprovalsMissing { needed: 2, have: 1 },
        AcceptFailure::SummaryHidden,
        AcceptFailure::Protected {
            notices: vec![Notice::temporary(
                Target::Path("Home".to_owned()),
                Require::Rung(Role::Admin),
                "Only the admin, this week.",
                1_700_086_400,
            )],
            lack: Lack::Rung(Role::Admin),
        },
        AcceptFailure::Protected {
            notices: vec![Notice::standing(
                Target::Path("Home".to_owned()),
                Require::Review,
                "The front page is looked at twice.",
            )],
            lack: Lack::Review,
        },
        AcceptFailure::RulesUnreadable(ReadFault::from_stored(
            GovernanceFile::Constitution,
            Some(2),
            GovernanceRefusal::WrongShape,
        )),
        AcceptFailure::Failed {
            acceptance: AcceptanceId::new(),
            word: inklkunn_core::acceptance::words::REFUSED,
            detail: "prikk said: the patch does not apply".to_owned(),
        },
        AcceptFailure::Incomplete {
            acceptance: AcceptanceId::new(),
            reason: RecoveryReason::WorktreeDirty,
            flagged: true,
            detail: "prikk said /srv/wikis/x/worktree is dirty".to_owned(),
        },
        AcceptFailure::Unavailable("the lock file /srv/x/lock".to_owned()),
        AcceptFailure::Store(StoreError::NotFound),
    ]
}

#[test]
fn there_is_a_sample_of_every_cause_and_the_list_of_causes_is_whole() {
    let mut have: Vec<Cause> = samples().iter().map(cause_of).collect();
    have.sort();
    have.dedup();
    let mut all = ALL.to_vec();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), ALL.len(), "a cause is listed twice");
    assert_eq!(
        have, all,
        "a cause of a refusal has no sample here (or ALL does not list it)"
    );
}
