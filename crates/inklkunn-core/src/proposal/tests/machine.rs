//! The state machine as a table, walked exhaustively: no sequence of acts reaches a state the RFC does not
//! name, and every pair of (state, act) that is not a named transition is refused.

use std::collections::BTreeSet;

use inklkunn_store::{ProposalOp, ProposalState};

use crate::proposal::machine::{Illegal, TRANSITIONS, apply, is_logged, legal_in};

use ProposalOp::{
    Accept, AskForRevision, Comment, Create, Decide, Decline, Revise, SaveDraft, Submit, Supersede,
    Withdraw,
};
use ProposalState::{
    Accepted, Declined, Draft, HeldForReview, NeedsRevision, Superseded, Withdrawn,
};

/// The RFC's life, written out a second time and independently of the table in the code: D-1 and D-8.
fn named(state: Option<ProposalState>, op: ProposalOp) -> Option<ProposalState> {
    match (state, op) {
        (None, Create) => Some(Draft),
        (Some(Draft), SaveDraft) => Some(Draft),
        (Some(Draft), Submit) => Some(HeldForReview),
        (Some(HeldForReview | NeedsRevision), Revise) => Some(HeldForReview),
        (Some(HeldForReview), Comment | Decide) => Some(HeldForReview),
        (Some(NeedsRevision), Comment) => Some(NeedsRevision),
        (Some(HeldForReview), AskForRevision) => Some(NeedsRevision),
        (Some(HeldForReview), Accept) => Some(Accepted),
        (Some(HeldForReview | NeedsRevision), Decline) => Some(Declined),
        (Some(Draft | HeldForReview | NeedsRevision), Withdraw) => Some(Withdrawn),
        (Some(HeldForReview | NeedsRevision), Supersede) => Some(Superseded),
        _ => None,
    }
}

fn states() -> Vec<Option<ProposalState>> {
    std::iter::once(None)
        .chain(ProposalState::ALL.into_iter().map(Some))
        .collect()
}

#[test]
fn every_pair_of_state_and_act_is_either_a_named_transition_or_refused() {
    let mut legal = 0;
    for state in states() {
        for op in ProposalOp::ALL {
            match (apply(state, op), named(state, op)) {
                (Ok(got), Some(want)) => {
                    assert_eq!(got, want, "{state:?} {op}");
                    legal += 1;
                }
                (Err(Illegal { op: o, state: s }), None) => {
                    assert_eq!(
                        (o, s),
                        (op, state),
                        "the refusal says what was asked, and where"
                    );
                }
                (got, want) => {
                    panic!("{state:?} {op}: the code says {got:?}, the RFC says {want:?}")
                }
            }
        }
    }
    assert_eq!(
        legal,
        TRANSITIONS.len(),
        "the table holds exactly the named transitions, once each"
    );
    let unique: BTreeSet<_> = TRANSITIONS
        .iter()
        .map(|(op, from, _)| (*op, *from))
        .collect();
    assert_eq!(unique.len(), TRANSITIONS.len(), "no pair is listed twice");
}

#[test]
fn the_four_end_states_have_no_way_out_and_a_creation_is_only_ever_from_nothing() {
    for end in [Accepted, Declined, Withdrawn, Superseded] {
        assert!(legal_in(end).is_empty(), "{end} is an end");
        assert!(!end.is_open());
        for op in ProposalOp::ALL {
            assert!(apply(Some(end), op).is_err(), "{end} {op}");
        }
    }
    for open in [Draft, HeldForReview, NeedsRevision] {
        assert!(open.is_open());
        assert!(!legal_in(open).is_empty(), "{open} can go somewhere");
        assert!(
            apply(Some(open), Create).is_err(),
            "a proposal is created once"
        );
    }
    for op in ProposalOp::ALL.into_iter().filter(|op| *op != Create) {
        assert!(apply(None, op).is_err(), "{op} on nothing");
    }
}

/// Every sequence of acts to depth 5 from the creation, then the set of states reachable in each of the next
/// 40 steps: the states reached are among the seven, each step is one the RFC names, an end state is never
/// left, and nothing new appears however far it goes.
#[test]
fn no_sequence_of_acts_reaches_a_state_the_rfc_does_not_name() {
    let all: BTreeSet<_> = ProposalState::ALL.into_iter().collect();
    let mut reached = BTreeSet::new();

    // Every sequence to depth 5.
    fn walk(
        state: ProposalState,
        depth: usize,
        reached: &mut BTreeSet<ProposalState>,
        all: &BTreeSet<ProposalState>,
    ) {
        reached.insert(state);
        assert!(all.contains(&state));
        if depth == 0 {
            return;
        }
        for op in ProposalOp::ALL {
            match apply(Some(state), op) {
                Ok(to) => {
                    assert_eq!(named(Some(state), op), Some(to));
                    assert!(state.is_open(), "an end state was left by {op}");
                    walk(to, depth - 1, reached, all);
                }
                Err(_) => assert_eq!(named(Some(state), op), None),
            }
        }
    }
    let start = apply(None, Create).unwrap();
    walk(start, 5, &mut reached, &all);

    // The frontier to depth 40: nothing new appears.
    let mut frontier: BTreeSet<ProposalState> = BTreeSet::from([start]);
    for _ in 0..40 {
        let mut next = BTreeSet::new();
        for s in &frontier {
            for op in ProposalOp::ALL {
                if let Ok(to) = apply(Some(*s), op) {
                    next.insert(to);
                }
            }
        }
        reached.extend(next.iter().copied());
        assert!(next.is_subset(&all));
        frontier = next;
    }
    assert_eq!(reached, all, "all seven are reachable, and nothing else is");
}

#[test]
fn only_the_acts_that_change_the_proposal_are_logged() {
    let logged: Vec<_> = ProposalOp::ALL
        .into_iter()
        .filter(|op| is_logged(*op))
        .collect();
    assert_eq!(
        logged,
        vec![
            Create,
            Submit,
            Revise,
            AskForRevision,
            Accept,
            Decline,
            Withdraw,
            Supersede
        ]
    );
}
