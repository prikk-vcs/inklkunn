//! Acts made at the same moment: **exactly one decision lands, and exactly one appeal**. Each test runs the pair many times on the
//! SQLite store, each pair released by one barrier. A decision that lost is told by name, and what the winner decided is what the
//! records show, for every kind of action: an overturn that won reversed the action once, an upheld one did not.

use std::sync::{Arc, Barrier};
use std::thread;

use inklkunn_store::{AppealState, AppealedKind, ProposalState, Role, Visibility};

use super::scenes::{Scene, scene};
use super::support::{Kind, World, steady};
use crate::appeal::{AppealError, AppealLimits, Outcome};

const ROUNDS: usize = 40;

fn world() -> World {
    World::with(
        Kind::Sqlite,
        Visibility::Public,
        None,
        AppealLimits::default(),
        steady,
    )
}

/// Run `a` and `b` at the same moment and give what each answered.
fn together<A: Send + 'static, B: Send + 'static>(
    a: impl FnOnce() -> A + Send + 'static,
    b: impl FnOnce() -> B + Send + 'static,
) -> (A, B) {
    let gate = Arc::new(Barrier::new(2));
    let (g1, g2) = (gate.clone(), gate);
    let one = thread::spawn(move || {
        g1.wait();
        a()
    });
    let two = thread::spawn(move || {
        g2.wait();
        b()
    });
    (one.join().expect("a thread"), two.join().expect("a thread"))
}

/// Is the action of the scene reversed now?
fn reversed(w: &World, what: AppealedKind, s: &Scene) -> bool {
    match what {
        AppealedKind::Decline => {
            w.state_of(s.proposal.as_ref().expect("a proposal")) == ProposalState::HeldForReview
        }
        AppealedKind::Tombstone => !w.hidden(s.comment.as_ref().expect("a comment")),
        AppealedKind::Restriction => !w.restricted(&s.appellant),
        AppealedKind::Protection => !w
            .store
            .active_protections(&w.wiki.id, steady())
            .unwrap()
            .iter()
            .any(|p| p.id.to_string() == s.contest.id),
        AppealedKind::Grant => w.grant_of(&s.appellant) == Some(Role::Maintainer),
    }
}

#[test]
fn two_deciders_at_once_leave_one_decision_and_what_it_decided_for_each_kind_of_action() {
    for what in AppealedKind::ALL {
        let w = world();
        let one = w.person("Dee", Some(Role::Admin));
        let two = w.person("Eve", Some(Role::Admin));
        for round in 0..ROUNDS {
            let s = scene(&w, what);
            let filed = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, "Not me.")
                .unwrap();
            // One says overturned and the other upheld, or both overturned: the order the threads run in is not the test's.
            let decide = |by: &super::support::Person, outcome: Outcome| {
                let (appeals, wiki, principal, id) = (
                    w.appeals.clone(),
                    w.wiki.clone(),
                    by.principal.clone(),
                    filed.id.clone(),
                );
                move || appeals.decide(&principal, &wiki, &id, outcome, "Because.")
            };
            let second = if round % 2 == 0 {
                Outcome::Upheld
            } else {
                Outcome::Overturned
            };
            let (a, b) = together(decide(&one, Outcome::Overturned), decide(&two, second));
            let results = [a, b];
            let made = results.iter().filter(|r| r.is_ok()).count();
            assert_eq!(made, 1, "{what:?} round {round}: {results:?}");
            assert!(
                results
                    .iter()
                    .filter_map(|r| r.as_ref().err())
                    .all(|e| *e == AppealError::AlreadyDecided),
                "{what:?} round {round}: the late one is told it was decided: {results:?}"
            );
            let won = results
                .iter()
                .find_map(|r| r.as_ref().ok())
                .expect("one won")
                .outcome;
            let state = w.appeal(&filed.id).state;
            match (won, &state) {
                (Outcome::Overturned, AppealState::Overturned(..))
                | (Outcome::Upheld, AppealState::Upheld(_)) => {}
                other => panic!(
                    "{what:?} round {round}: the winner said {won:?}, the record says {other:?}"
                ),
            }
            assert_eq!(
                reversed(&w, what, &s),
                won == Outcome::Overturned,
                "{what:?} round {round}: the action is reversed exactly when the decision that landed overturned it"
            );
        }
    }
}

#[test]
fn two_deciders_who_both_overturn_reverse_the_action_once() {
    for what in [AppealedKind::Decline, AppealedKind::Tombstone] {
        let w = world();
        let one = w.person("Dee", Some(Role::Admin));
        let two = w.person("Eve", Some(Role::Admin));
        for round in 0..ROUNDS {
            let s = scene(&w, what);
            let filed = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, "Not me.")
                .unwrap();
            let decide = |by: &super::support::Person| {
                let (appeals, wiki, principal, id) = (
                    w.appeals.clone(),
                    w.wiki.clone(),
                    by.principal.clone(),
                    filed.id.clone(),
                );
                move || appeals.decide(&principal, &wiki, &id, Outcome::Overturned, "Because.")
            };
            let (a, b) = together(decide(&one), decide(&two));
            assert_eq!(
                [&a, &b].iter().filter(|r| r.is_ok()).count(),
                1,
                "{what:?} round {round}: {a:?} {b:?}"
            );
            if what == AppealedKind::Decline {
                let proposal = s.proposal.clone().expect("a proposal");
                let steps = w.store.transitions(&proposal, None).unwrap();
                assert_eq!(
                    steps
                        .iter()
                        .filter(|t| t.op == inklkunn_store::ProposalOp::Reopen)
                        .count(),
                    1,
                    "round {round}: one reopening, not two"
                );
            }
        }
    }
}

#[test]
fn two_appeals_of_the_same_person_against_the_same_action_at_once_leave_one() {
    let w = world();
    for round in 0..ROUNDS {
        let s = scene(&w, AppealedKind::Restriction);
        let file = || {
            let (appeals, wiki, principal, contest) = (
                w.appeals.clone(),
                w.wiki.clone(),
                s.appellant.principal.clone(),
                s.contest.clone(),
            );
            move || appeals.file(&principal, &wiki, &contest, "Not me.")
        };
        let (a, b) = together(file(), file());
        let results = [a, b];
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "round {round}: {results:?}"
        );
        assert!(
            results
                .iter()
                .filter_map(|r| r.as_ref().err())
                .all(|e| matches!(e, AppealError::AlreadyOpen { .. })),
            "round {round}: the late one is told there is one open: {results:?}"
        );
        assert_eq!(
            w.store
                .appeals_about(&w.wiki.id, &s.contest.action())
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn two_people_filing_at_once_cannot_both_take_the_last_place_in_a_wiki_s_queue() {
    let w = World::with(
        Kind::Sqlite,
        Visibility::Public,
        None,
        AppealLimits {
            per_person_per_day: 10,
            open_per_wiki: 1,
            ..AppealLimits::default()
        },
        steady,
    );
    for round in 0..ROUNDS {
        // Empty the queue: decide whatever is open.
        let admin = w.person("Dee", Some(Role::Admin));
        for open in w.store.open_appeals(&w.wiki.id, 10).unwrap() {
            w.appeals
                .decide(
                    &admin.principal,
                    &w.wiki,
                    &open.id,
                    Outcome::Upheld,
                    "Done.",
                )
                .unwrap();
        }
        let (s1, s2) = (
            scene(&w, AppealedKind::Restriction),
            scene(&w, AppealedKind::Restriction),
        );
        let file = |s: &Scene| {
            let (appeals, wiki, principal, contest) = (
                w.appeals.clone(),
                w.wiki.clone(),
                s.appellant.principal.clone(),
                s.contest.clone(),
            );
            move || appeals.file(&principal, &wiki, &contest, "Not me.")
        };
        let (a, b) = together(file(&s1), file(&s2));
        let results = [a, b];
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "round {round}: {results:?}"
        );
        assert!(
            results
                .iter()
                .filter_map(|r| r.as_ref().err())
                .all(|e| matches!(e, AppealError::Quota(_))),
            "round {round}: {results:?}"
        );
    }
}
