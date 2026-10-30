//! The service held to the state machine: **every sequence of acts up to a depth**, run against a real store,
//! must do exactly what the table says (the act happens and the proposal is where the table puts it, or it is
//! refused and nothing changed), and the log the store keeps must be a path through the table. This is the
//! property "no sequence of operations reaches a state the RFC does not name", walked exhaustively
//! (`proptest` is not a dependency of this crate, and the alphabet is small enough to enumerate).

use inklkunn_store::{DecisionKind, ProposalId, ProposalOp, ProposalState, Role};

use super::support::{Env, Kind, edit};
use crate::proposal::machine::{self, TRANSITIONS};
use crate::proposal::{NewComment, ProposalError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    SaveDraft,
    Submit,
    Revise,
    Comment,
    Decide,
    AskForRevision,
    Accept,
    Decline,
    Withdraw,
    Supersede,
}

const ACTS: [Act; 10] = [
    Act::SaveDraft,
    Act::Submit,
    Act::Revise,
    Act::Comment,
    Act::Decide,
    Act::AskForRevision,
    Act::Accept,
    Act::Decline,
    Act::Withdraw,
    Act::Supersede,
];

impl Act {
    fn op(self) -> ProposalOp {
        match self {
            Act::SaveDraft => ProposalOp::SaveDraft,
            Act::Submit => ProposalOp::Submit,
            Act::Revise => ProposalOp::Revise,
            Act::Comment => ProposalOp::Comment,
            Act::Decide => ProposalOp::Decide,
            Act::AskForRevision => ProposalOp::AskForRevision,
            Act::Accept => ProposalOp::Accept,
            Act::Decline => ProposalOp::Decline,
            Act::Withdraw => ProposalOp::Withdraw,
            Act::Supersede => ProposalOp::Supersede,
        }
    }

    /// Is it the proposer's act?
    fn by_proposer(self) -> bool {
        matches!(
            self,
            Act::SaveDraft | Act::Submit | Act::Revise | Act::Withdraw | Act::Supersede
        )
    }
}

/// What the table says about the proposal under test.
struct Model {
    state: ProposalState,
    /// Has it ever been submitted? Until then only its author can see it.
    submitted: bool,
    revisions: u32,
}

fn perform(
    e: &Env,
    id: &ProposalId,
    act: Act,
    m: &Model,
    superseder: &mut Option<ProposalId>,
) -> Result<(), ProposalError> {
    let (w, s) = (&e.wiki, &e.service);
    match act {
        Act::SaveDraft => s
            .save_draft(
                &e.author.principal,
                w,
                id,
                edit("Saved", "pages/a.md", "saved"),
            )
            .map(|_| ()),
        Act::Submit => s.submit(&e.author.principal, w, id).map(|_| ()),
        Act::Revise => s
            .revise(
                &e.author.principal,
                w,
                id,
                edit("Revised", "pages/a.md", "revised"),
            )
            .map(|_| ()),
        Act::Comment => s
            .comment(
                &e.reviewer.principal,
                w,
                id,
                NewComment {
                    body: "A remark".into(),
                    ..NewComment::default()
                },
            )
            .map(|_| ()),
        Act::Decide => {
            // A new reviewer each time: one decision per reviewer per revision is its own test.
            let reviewer = e.person(Some(Role::Reviewer));
            s.decide(
                &reviewer.principal,
                w,
                id,
                m.revisions,
                DecisionKind::Approve,
                "",
            )
            .map(|_| ())
        }
        Act::AskForRevision => s
            .ask_for_revision(&e.maintainer.principal, w, id, "Please rebase.")
            .map(|_| ()),
        Act::Accept => s.record_accepted(w, id, &e.maintainer.id).map(|_| ()),
        Act::Decline => s
            .decline(&e.maintainer.principal, w, id, "Not now.")
            .map(|_| ()),
        Act::Withdraw => s
            .withdraw(&e.author.principal, w, id, "Changed my mind.")
            .map(|_| ()),
        Act::Supersede => {
            let by = superseder
                .get_or_insert_with(|| e.held(&e.author, "The later one", "pages/b.md").id)
                .clone();
            s.supersede(&e.author.principal, w, id, &by).map(|_| ())
        }
    }
}

/// Run one sequence on a fresh world and hold every step to the table.
fn run(kind: Kind, sequence: &[Act]) {
    let e = Env::new(kind);
    let draft = e.draft(&e.author, "Under test", "pages/a.md");
    let mut m = Model {
        state: draft.state,
        submitted: false,
        revisions: 0,
    };
    let mut superseder = None;
    for (step, act) in sequence.iter().enumerate() {
        let before = e
            .service
            .get(&e.author.principal, &e.wiki, &draft.id)
            .unwrap();
        let got = perform(&e, &draft.id, *act, &m, &mut superseder);
        // The table's answer. Someone who is not the proposer cannot see a proposal that was never sent: for
        // them it is not there, whatever state it is in.
        let table = machine::apply(Some(m.state), act.op());
        let sees_it = act.by_proposer() || m.submitted;
        let want = if !sees_it {
            Err(ProposalError::NotFound)
        } else {
            match table {
                Ok(_) => Ok(()),
                Err(i) => Err(ProposalError::NotInThisState {
                    op: i.op,
                    state: m.state,
                }),
            }
        };
        assert_eq!(
            got, want,
            "{sequence:?} at step {step} ({act:?}, proposal {})",
            m.state
        );

        let after = e
            .service
            .get(&e.author.principal, &e.wiki, &draft.id)
            .unwrap();
        match (&got, table) {
            (Ok(()), Ok(to)) => {
                assert_eq!(after.state, to, "{sequence:?} step {step}");
                m.state = to;
                if *act == Act::Submit {
                    m.submitted = true;
                    m.revisions = 1;
                }
                if *act == Act::Revise {
                    m.revisions += 1;
                }
            }
            _ => assert_eq!(
                after, before,
                "a refused act changed the proposal: {sequence:?} step {step}"
            ),
        }
    }

    // What the store holds is a path through the table, from the creation to where the model says it is.
    let log = e
        .service
        .transitions(&e.author.principal, &e.wiki, &draft.id)
        .unwrap();
    let mut at: Option<ProposalState> = None;
    for t in &log {
        assert_eq!(t.from, at, "{sequence:?}: the log skips a step");
        assert!(
            TRANSITIONS.contains(&(t.op, t.from, t.to)),
            "{sequence:?}: {:?} is not a named transition",
            (t.op, t.from, t.to)
        );
        assert!(machine::is_logged(t.op));
        at = Some(t.to);
    }
    assert_eq!(
        at,
        Some(m.state),
        "{sequence:?}: the log ends where the proposal is"
    );
    let stored = e
        .service
        .get(&e.author.principal, &e.wiki, &draft.id)
        .unwrap();
    assert!(ProposalState::ALL.contains(&stored.state));
    assert_eq!(
        stored.closed_at.is_some(),
        !m.state.is_open(),
        "{sequence:?}"
    );
    assert_eq!(stored.submitted_at.is_some(), m.submitted, "{sequence:?}");
}

/// Every sequence of `ACTS` of length 1 up to `depth`.
fn sequences(depth: usize) -> impl Iterator<Item = Vec<Act>> {
    (1..=depth).flat_map(|len| {
        let total = ACTS.len().pow(u32::try_from(len).unwrap());
        (0..total).map(move |mut n| {
            let mut seq = Vec::with_capacity(len);
            for _ in 0..len {
                seq.extend(ACTS.get(n % ACTS.len()).copied());
                n /= ACTS.len();
            }
            seq
        })
    })
}

#[test]
fn every_sequence_of_four_acts_does_what_the_table_says_on_the_memory_store() {
    let mut count = 0;
    for seq in sequences(4) {
        run(Kind::Memory, &seq);
        count += 1;
    }
    assert_eq!(count, 10 + 100 + 1_000 + 10_000);
}

#[test]
fn every_sequence_of_three_acts_does_what_the_table_says_on_the_sqlite_store() {
    let mut count = 0;
    for seq in sequences(3) {
        run(Kind::Sqlite, &seq);
        count += 1;
    }
    assert_eq!(count, 10 + 100 + 1_000);
}

/// The walk reaches the interesting places: all seven states, and a refusal for every act in some state. (A
/// walk that never got anywhere would pass the tests above for nothing.)
#[test]
fn the_walk_reaches_every_state_and_refuses_every_act_somewhere() {
    let mut states = std::collections::BTreeSet::new();
    let mut refused = std::collections::BTreeSet::new();
    for seq in sequences(3) {
        let mut state = Some(ProposalState::Draft);
        for act in &seq {
            match state.map(|s| machine::apply(Some(s), act.op())) {
                Some(Ok(to)) => {
                    states.insert(to);
                    state = Some(to);
                }
                Some(Err(_)) => {
                    refused.insert(act.op());
                }
                None => {}
            }
        }
    }
    let all: std::collections::BTreeSet<_> = ProposalState::ALL.into_iter().collect();
    assert_eq!(states, all, "every state is reached");
    let acts: std::collections::BTreeSet<_> = ACTS.iter().map(|a| a.op()).collect();
    assert_eq!(refused, acts);
}
