//! A proposal made and sent in one step (handoff 005 corrections, fourth round, K38; the shape of the reconfirm step's,
//! K26), on both stores: it is checked as a submission is before anything is written, written with its submission in
//! one transaction, and a refusal leaves nothing behind (no draft that counts against its author).

use inklkunn_store::{ProposalFilter, ProposalOp, ProposalState, Role};

use super::support::{Actor, Env, T0, edit, on_both, on_both_with, set_now};
use crate::proposal::{Invalid, ProposalError, QuotaConfig, QuotaLimit};

fn send(
    e: &Env,
    who: &Actor,
    summary: &str,
) -> Result<inklkunn_store::ProposalRecord, ProposalError> {
    e.service.create_and_submit(
        &who.principal,
        &e.wiki,
        None,
        edit(summary, "pages/curated.md", "text"),
    )
}

fn own(e: &Env, who: &Actor) -> usize {
    e.store
        .proposals(&e.wiki.id, &ProposalFilter::default(), Some(&who.id))
        .unwrap()
        .iter()
        .filter(|p| p.author == who.id)
        .count()
}

#[test]
fn the_proposal_is_held_for_review_with_the_records_of_a_creation_and_a_submission() {
    on_both(|e| {
        let who = e.person(Some(Role::Contributor));
        set_now(T0 + 5);
        let made = send(e, &who, "Curate the claim").unwrap();
        assert_eq!(made.state, ProposalState::HeldForReview);
        assert_eq!(made.submitted_at, Some(T0 + 5));
        assert_eq!(
            e.store.proposal(&made.id, None).unwrap(),
            Some(made.clone())
        );
        let steps: Vec<_> = e
            .store
            .transitions(&made.id, None)
            .unwrap()
            .into_iter()
            .map(|t| (t.op, t.from, t.to))
            .collect();
        assert_eq!(
            steps,
            vec![
                (ProposalOp::Create, None, ProposalState::Draft),
                (
                    ProposalOp::Submit,
                    Some(ProposalState::Draft),
                    ProposalState::HeldForReview
                ),
            ]
        );
        assert_eq!(e.store.revisions(&made.id, None).unwrap().len(), 1);
        assert!(e.store.draft(&made.id, Some(&who.id)).unwrap().is_none());
        // Nothing else names it: it re-confirms no change.
        assert_eq!(
            e.service
                .reconfirm_of(&who.principal, &e.wiki, &made.id)
                .unwrap(),
            None
        );
    });
}

#[test]
fn a_refused_send_leaves_nothing_behind() {
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
        // A summary a submission would refuse (a draft may be unfinished, a sent proposal may not).
        assert!(matches!(
            send(e, &who, ""),
            Err(ProposalError::Invalid(Invalid::SummaryMissing))
        ));
        assert_eq!(own(e, &who), 0, "an unfinished summary");
        // The wiki's hourly limit, taken up by someone else's submission just now.
        set_now(T0 + 10);
        e.held(&other, "Another", "pages/another.md");
        match send(e, &who, "Curate the claim") {
            Err(ProposalError::Quota(q)) => {
                assert_eq!(q.limit, QuotaLimit::ProposalsPerHourPerWiki)
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(own(e, &who), 0, "no draft");
        assert_eq!(e.store.open_count(&e.wiki.id, &who.id).unwrap(), 0);
        assert!(
            e.store
                .opened_since(&e.wiki.id, Some(&who.id), 0)
                .unwrap()
                .is_empty()
        );
    });
}
