//! The reopening of a declined proposal and the reading of a step of the proposal log by its position (RFC 006 D-9, G-T5): the
//! two things an appeal against a decline stands on, held by both stores alike.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::{
    AccountStore, MemoryStore, ProposalOp, ProposalState, ProposalStore, SqliteStore, WikiStore,
};
use support::{account, mv, open_draft, submit, wiki};

fn the_suite(store: &(impl ProposalStore + WikiStore + AccountStore)) {
    let w = wiki(store, "one");
    let other = wiki(store, "two");
    let (author, moderator) = (account(store), account(store));
    let p = submit(store, &open_draft(store, &w.id, &author, 100), 101);
    let declined = mv(
        store,
        &p.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &moderator,
        110,
    );
    assert_eq!(declined.closed_at, Some(110));
    let position = store
        .last_transition_position(&p.id)
        .unwrap()
        .expect("the decline has a position");

    // A step is read by its position, in its own wiki only.
    let (found, step) = store
        .transition_at(&w.id, position)
        .unwrap()
        .expect("the step is there");
    assert_eq!(found, p.id);
    assert_eq!(step.op, ProposalOp::Decline);
    assert_eq!(step.actor, moderator);
    assert!(store.transition_at(&other.id, position).unwrap().is_none());
    assert!(store.transition_at(&w.id, 0).unwrap().is_none());
    assert!(store.transition_at(&w.id, -3).unwrap().is_none());
    assert!(
        store
            .transition_at(&w.id, position + 1000)
            .unwrap()
            .is_none()
    );

    // A declined proposal is reopened: held for review again, and not closed any more.
    let reopened = mv(
        store,
        &p.id,
        ProposalOp::Reopen,
        ProposalState::Declined,
        ProposalState::HeldForReview,
        &moderator,
        120,
    );
    assert_eq!(reopened.state, ProposalState::HeldForReview);
    assert_eq!(reopened.closed_at, None, "a proposal brought back is open");
    assert_eq!(reopened.updated_at, 120);
    let after = store.last_transition_position(&p.id).unwrap().unwrap();
    assert!(after > position, "the reopening is a step of its own");
    let (_, reopening) = store.transition_at(&w.id, after).unwrap().unwrap();
    assert_eq!(reopening.op, ProposalOp::Reopen);
    // The decline stays where it was in the log.
    let log = store.transitions(&p.id, None).unwrap();
    assert_eq!(
        log.iter().map(|t| t.op).collect::<Vec<_>>(),
        vec![
            ProposalOp::Create,
            ProposalOp::Submit,
            ProposalOp::Decline,
            ProposalOp::Reopen
        ]
    );

    // It can be declined again, and the second decline closes it again.
    let again = mv(
        store,
        &p.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &moderator,
        130,
    );
    assert_eq!(again.closed_at, Some(130));
}

#[test]
fn the_memory_store_reopens_a_declined_proposal_and_reads_a_step_by_its_position() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_reopens_a_declined_proposal_and_reads_a_step_by_its_position() {
    the_suite(&SqliteStore::open_in_memory().expect("a store"));
}
