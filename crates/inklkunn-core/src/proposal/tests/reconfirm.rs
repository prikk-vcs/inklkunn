//! The reconfirm step's curation proposal is made **already held for review** (handoff 005 K26), on both stores: it is
//! checked as a submission is, before anything is written; it is written with its submission in one step; so that a
//! refusal leaves nothing behind (no draft, no note of the change) and no unsent draft can hold the change's one open
//! place against everyone else.

use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, ProposalFilter,
    ProposalOp, ProposalRecord, ProposalState, Role,
};

use super::support::{Actor, Env, T0, edit, on_both, on_both_with, set_now};
use crate::proposal::{Invalid, ProposalError, QuotaConfig, QuotaLimit};

const HOUR: i64 = 3600;

/// A change in the history: a proposal of the maintainer's, accepted, and sealed.
fn sealed_change(e: &Env, n: usize) -> AcceptanceId {
    let proposal = e.held(&e.maintainer, "Accepted", &format!("pages/c{n}.md"));
    let a = AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: proposal.id,
        revision: 1,
        wiki: e.wiki.id.clone(),
        view: "heads/main".into(),
        acceptor: e.maintainer.id.clone(),
        acceptor_role: Role::Maintainer,
        policy_version: "default-1".into(),
        proposer: e.maintainer.id.clone(),
        digest: "d".repeat(64),
        bytes: 10,
        created_at: T0,
    };
    e.store.begin_acceptance(&a).unwrap();
    e.store
        .finish_acceptance(&AcceptanceResult {
            acceptance: a.id.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: "b".repeat(64),
                patch_id: "c".repeat(64),
            },
            at: T0,
        })
        .unwrap();
    a.id
}

fn press(
    e: &Env,
    who: &Actor,
    summary: &str,
    change: &AcceptanceId,
) -> Result<ProposalRecord, ProposalError> {
    e.service.create_reconfirm(
        &who.principal,
        &e.wiki,
        None,
        edit(summary, "pages/judged.md", "text"),
        change,
    )
}

fn all(e: &Env, who: &Actor) -> Vec<ProposalRecord> {
    e.service
        .list(&who.principal, &e.wiki, &ProposalFilter::default())
        .unwrap()
}

/// Everything that is left of a press: the proposals anyone can list, the author's own (drafts included), what counts
/// toward the quotas, and the notes of the change.
fn left(e: &Env, who: &Actor, change: &AcceptanceId) -> (usize, usize, u64, usize, usize) {
    (
        all(e, who).len(),
        // Their own, drafts included (a draft is its author's alone).
        e.store
            .proposals(&e.wiki.id, &ProposalFilter::default(), Some(&who.id))
            .unwrap()
            .iter()
            .filter(|p| p.author == who.id)
            .count(),
        e.store.open_count(&e.wiki.id, &who.id).unwrap(),
        e.store
            .opened_since(&e.wiki.id, Some(&who.id), 0)
            .unwrap()
            .len(),
        e.store
            .reconfirmations_of(change, Some(&who.id))
            .unwrap()
            .len(),
    )
}

fn steps(e: &Env, p: &ProposalRecord) -> Vec<(ProposalOp, Option<ProposalState>, ProposalState)> {
    e.store
        .transitions(&p.id, None)
        .unwrap()
        .into_iter()
        .map(|t| (t.op, t.from, t.to))
        .collect()
}

#[test]
fn the_proposal_is_made_held_for_review_with_the_records_of_a_submission() {
    on_both(|e| {
        let (who, other) = (
            e.person(Some(Role::Contributor)),
            e.person(Some(Role::Contributor)),
        );
        let change = sealed_change(e, 0);
        set_now(T0 + 5);
        let made = press(e, &who, "Judge them again", &change).unwrap();
        assert_eq!(made.state, ProposalState::HeldForReview);
        assert_eq!(made.submitted_at, Some(T0 + 5));
        // The record returned is the record kept, seen by anyone who may read the proposals, from the first moment.
        assert_eq!(
            e.store.proposal(&made.id, None).unwrap(),
            Some(made.clone())
        );
        assert_eq!(all(e, &other).last(), Some(&made));
        // The same steps and the same revision as a draft opened and then submitted leaves.
        set_now(T0 + 6);
        let plain = e.held(&other, "Judge them", "pages/plain.md");
        assert_eq!(steps(e, &made), steps(e, &plain));
        assert_eq!(
            steps(e, &made),
            vec![
                (ProposalOp::Create, None, ProposalState::Draft),
                (
                    ProposalOp::Submit,
                    Some(ProposalState::Draft),
                    ProposalState::HeldForReview
                ),
            ]
        );
        let revisions = |p: &ProposalRecord| {
            e.store
                .revisions(&p.id, None)
                .unwrap()
                .iter()
                .map(|r| r.seq)
                .collect::<Vec<_>>()
        };
        assert_eq!(revisions(&made), revisions(&plain));
        assert_eq!(revisions(&made), vec![1]);
        assert!(
            e.store.draft(&made.id, Some(&who.id)).unwrap().is_none(),
            "it was never a draft"
        );
        // It is counted as a submission is, and names its change.
        assert_eq!(
            e.store
                .submitted_since(&e.wiki.id, T0)
                .unwrap()
                .into_iter()
                .filter(|(a, at)| *a == who.id && *at == T0 + 5)
                .count(),
            1
        );
        assert_eq!(
            e.service
                .reconfirm_of(&who.principal, &e.wiki, &made.id)
                .unwrap(),
            Some(change)
        );
        // There is nothing left to submit.
        assert!(matches!(
            e.service.submit(&who.principal, &e.wiki, &made.id),
            Err(ProposalError::NotInThisState { .. })
        ));
    });
}

#[test]
fn a_refused_press_leaves_nothing_behind_and_the_next_press_works() {
    let config = QuotaConfig {
        proposals_per_hour_per_wiki: 1,
        open_proposals: 100,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        set_now(T0);
        let (who, other) = (
            e.person(Some(Role::Contributor)),
            e.person(Some(Role::Contributor)),
        );
        let change = sealed_change(e, 0);
        let unsealed = {
            // A change that never sealed is no change to re-confirm.
            let p = e.held(&e.maintainer, "Failed", "pages/failed.md");
            let a = AcceptanceId::new();
            e.store
                .begin_acceptance(&AcceptanceRecord {
                    id: a.clone(),
                    proposal: p.id,
                    revision: 1,
                    wiki: e.wiki.id.clone(),
                    view: "heads/main".into(),
                    acceptor: e.maintainer.id.clone(),
                    acceptor_role: Role::Maintainer,
                    policy_version: "default-1".into(),
                    proposer: e.maintainer.id.clone(),
                    digest: "d".repeat(64),
                    bytes: 10,
                    created_at: T0,
                })
                .unwrap();
            a
        };
        let before = left(e, &who, &change);

        // A summary the submission would refuse (a draft may be unfinished, a submission may not).
        assert!(matches!(
            press(e, &who, "", &change),
            Err(ProposalError::Invalid(Invalid::SummaryMissing))
        ));
        assert_eq!(left(e, &who, &change), before, "an unfinished summary");
        // A change that did not seal.
        assert!(matches!(
            press(e, &who, "Judge them again", &unsealed),
            Err(ProposalError::NotFound)
        ));
        assert_eq!(
            left(e, &who, &unsealed).4,
            0,
            "no note of a change that did not seal"
        );
        assert_eq!(left(e, &who, &change), before, "a change that did not seal");
        // The wiki's hourly limit, taken up by someone else's submission just now.
        set_now(T0 + 10);
        e.held(&other, "Another change", "pages/another.md");
        let refused = press(e, &who, "Judge them again", &change);
        match refused {
            Err(ProposalError::Quota(q)) => {
                assert_eq!(q.limit, QuotaLimit::ProposalsPerHourPerWiki)
            }
            other => panic!("{other:?}"),
        }
        let (listed, own, open, opened, notes) = left(e, &who, &change);
        assert_eq!((own, open, opened, notes), (0, 0, 0, 0), "no draft");
        assert_eq!(listed, before.0 + 1, "only the other person's submission");

        // Nothing holds the change's place: once the hour has passed, the next press makes the proposal.
        set_now(T0 + HOUR + 20);
        let made = press(e, &who, "Judge them again", &change).unwrap();
        assert_eq!(made.state, ProposalState::HeldForReview);
        // And now it does: a second press (by people the hourly limit does not bind) is told so, and nothing is made.
        let count = all(e, &other).len();
        for presser in [&e.reviewer, &e.maintainer] {
            assert!(matches!(
                press(e, presser, "Judge them again", &change),
                Err(ProposalError::Store(
                    inklkunn_store::StoreError::AlreadyRecorded
                ))
            ));
        }
        assert_eq!(all(e, &other).len(), count);
        assert_eq!(
            e.service
                .reconfirmations_of(&other.principal, &e.wiki, &change)
                .unwrap(),
            vec![made.id.clone()],
            "the one that is open is there for everyone to see and link"
        );
    });
}

#[test]
fn a_press_is_judged_as_a_submission_is_of_its_pages_and_its_text() {
    on_both(|e| {
        let who = e.person(Some(Role::Contributor));
        let change = sealed_change(e, 0);
        let before = left(e, &who, &change);
        // Someone who may only read cannot press; nothing is made.
        let observer = &e.observer;
        assert!(matches!(
            press(e, observer, "Judge them again", &change),
            Err(ProposalError::NotAllowed)
        ));
        // A path outside the pages, a summary that is too long: refused before anything is written.
        let wrong = e.service.create_reconfirm(
            &who.principal,
            &e.wiki,
            None,
            edit("Judge them again", "elsewhere/judged.md", "text"),
            &change,
        );
        assert!(matches!(
            wrong,
            Err(ProposalError::Invalid(Invalid::PathOutsidePages))
        ));
        let long = "x".repeat(500);
        assert!(matches!(
            press(e, &who, &long, &change),
            Err(ProposalError::Invalid(Invalid::SummaryTooLong))
        ));
        assert_eq!(left(e, &who, &change), before);
    });
}
