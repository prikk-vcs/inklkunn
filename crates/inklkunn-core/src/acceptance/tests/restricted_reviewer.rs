#![allow(clippy::expect_used, clippy::unwrap_used)]
//! A restricted reviewer's approval stops counting (handoff G-T1 correction F-6). The approvals a policy asks for are counted at
//! acceptance, by the rung each reviewer holds **then**, through the proposal service's authorizer (a restriction caps a
//! reviewer at observer), and they count again when the restriction ends. A controlled clock begins and ends the restriction.

use std::sync::atomic::{AtomicI64, Ordering};

use inklkunn_store::{
    Actor, DecisionKind, GovernanceStore, MeasureEvent, RestrictionEvent, RestrictionEventId,
};

use super::super::testing::{HOME, Rig};
use super::*;

const T0: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

static NOW: AtomicI64 = AtomicI64::new(T0);

fn clock() -> i64 {
    NOW.load(Ordering::SeqCst)
}

#[test]
fn an_approval_counts_stops_counting_while_its_giver_is_restricted_and_counts_again_when_the_restriction_ends()
 {
    let rig = Rig::new("restricted-reviewer");
    rig.fake.derive_changes();
    rig.fake.set_files(&[(
        "governance/policy.yaml".to_owned(),
        "acceptance:\n  approvals: 1\n".to_owned(),
    )]);
    rig.reads.forget_wiki(&rig.wiki);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let (reviewer_id, reviewer) = rig.person("Rin", Some(Role::Reviewer));
    let (moderator, _) = rig.person("Ada", Some(Role::Admin));
    let p = rig.propose(
        &author,
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    rig.proposals
        .decide(&reviewer, &rig.wiki, &p.id, 1, DecisionKind::Approve, "")
        .unwrap();
    // The reviewer is restricted for a day, from T0.
    NOW.store(T0 + 10, Ordering::SeqCst);
    rig.store
        .record_restriction_event(&RestrictionEvent {
            id: RestrictionEventId::new(),
            wiki: rig.wiki.id.clone(),
            at: T0,
            event: MeasureEvent::Set,
            account: reviewer_id,
            by: Actor {
                account: moderator,
                rung: Role::Admin,
            },
            policy_block: None,
            reason: Some("Edit-warring.".to_owned()),
            expires_at: Some(T0 + DAY),
            of_event: None,
            appeal: None,
        })
        .unwrap();
    let acceptor = rig.acceptor_at(clock);
    // The approval is on record and does not count: the acceptance is refused for the missing approval.
    let result = acceptor.accept(&maintainer, &rig.wiki, &p.id, 1);
    assert!(
        matches!(
            result,
            Err(AcceptFailure::ApprovalsMissing { needed: 1, have: 0 })
        ),
        "{result:?}"
    );
    assert!(rig.double.calls().is_empty(), "nothing is signed");
    assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
    // The second before the end it still does not; at the end it counts again.
    NOW.store(T0 + DAY - 1, Ordering::SeqCst);
    assert!(matches!(
        acceptor.accept(&maintainer, &rig.wiki, &p.id, 1),
        Err(AcceptFailure::ApprovalsMissing { .. })
    ));
    NOW.store(T0 + DAY, Ordering::SeqCst);
    assert!(matches!(
        acceptor.accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}
