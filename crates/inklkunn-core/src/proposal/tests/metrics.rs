//! The participation metrics (RFC 004 D-10) against a hand count: a small fixture built through the service,
//! every figure counted on paper first, on both stores.

use std::collections::BTreeMap;

use inklkunn_store::{DecisionKind, ProposalState, Role};

use super::support::{Env, edit, on_both, set_now};
use crate::proposal::{Metrics, Period, ProposalError, Ratio};

/// Which fixture proposal is which.
struct Fixture {
    a: super::support::Actor,
    b: super::support::Actor,
    c: super::support::Actor,
}

fn at(e: &Env, t: i64) {
    let _ = e;
    set_now(t);
}

fn submitted(e: &Env, who: &super::support::Actor, t: i64, n: u32) -> inklkunn_store::ProposalId {
    at(e, t - 1);
    let d = e
        .service
        .create(
            &who.principal,
            &e.wiki,
            None,
            edit(&format!("Change {n}"), &format!("pages/m{n}.md"), "x"),
        )
        .unwrap();
    at(e, t);
    e.service.submit(&who.principal, &e.wiki, &d.id).unwrap();
    d.id
}

/// The fixture, with its hand count in the comments.
///
/// Before the period `[1000, 2000)`: A submits P0 at 500, accepted at 600.
///
/// In it:
/// - P1 by A, submitted 1000; R1 approves at 1100, R2 approves at 1300; accepted at 1500.
///   first review 100 s, decision 500 s, two reviewers.
/// - P2 by B, submitted 1200; nobody reviews; declined at 1260. decision 60 s, no reviewer.
/// - P3 by B, submitted 1400; B decides on it herself at 1450 (not a review); R1 asks for changes at 1700;
///   withdrawn at 1800. first review 300 s, no decision (withdrawn), one reviewer.
/// - P4 by C, submitted 1900; R2 approves at 1950; still held. first review 50 s, one reviewer.
///
/// Around it, and in no figure: a draft of A's opened at 1500 and never sent; a draft of C's that was
/// withdrawn unsent; P6 by C, submitted at 2500 (after the period).
fn fixture(e: &Env) -> Fixture {
    let a = e.person(Some(Role::Contributor));
    // B reviews too (a reviewer is a contributor and more), to be able to decide on her own proposal.
    let b = e.person(Some(Role::Reviewer));
    let c = e.person(Some(Role::Contributor));
    let r1 = e.person(Some(Role::Reviewer));
    let r2 = e.person(Some(Role::Reviewer));
    let (w, s) = (&e.wiki, &e.service);

    let p0 = submitted(e, &a, 500, 0);
    at(e, 600);
    s.record_accepted(w, &p0, &e.maintainer.id).unwrap();

    let p1 = submitted(e, &a, 1000, 1);
    at(e, 1100);
    s.decide(&r1.principal, w, &p1, 1, DecisionKind::Approve, "")
        .unwrap();
    at(e, 1300);
    s.decide(&r2.principal, w, &p1, 1, DecisionKind::Approve, "")
        .unwrap();
    at(e, 1500);
    s.record_accepted(w, &p1, &e.maintainer.id).unwrap();

    let p2 = submitted(e, &b, 1200, 2);
    at(e, 1260);
    s.decline(&e.maintainer.principal, w, &p2, "Not now.")
        .unwrap();

    let p3 = submitted(e, &b, 1400, 3);
    at(e, 1450);
    s.decide(&b.principal, w, &p3, 1, DecisionKind::Approve, "")
        .unwrap();
    at(e, 1700);
    s.decide(&r1.principal, w, &p3, 1, DecisionKind::RequestChanges, "")
        .unwrap();
    at(e, 1800);
    s.withdraw(&b.principal, w, &p3, "Superseded by my own work elsewhere.")
        .unwrap();

    let p4 = submitted(e, &c, 1900, 4);
    at(e, 1950);
    s.decide(&r2.principal, w, &p4, 1, DecisionKind::Approve, "")
        .unwrap();

    at(e, 1500);
    s.create(
        &a.principal,
        w,
        None,
        edit("Unsent", "pages/unsent.md", "x"),
    )
    .unwrap();
    at(e, 1600);
    let unsent = s
        .create(
            &c.principal,
            w,
            None,
            edit("Unsent too", "pages/unsent2.md", "x"),
        )
        .unwrap();
    s.withdraw(&c.principal, w, &unsent.id, "Never mind.")
        .unwrap();

    submitted(e, &c, 2500, 6);
    Fixture { a, b, c }
}

fn expected_by_state(
    held: u64,
    accepted: u64,
    declined: u64,
    withdrawn: u64,
) -> BTreeMap<ProposalState, u64> {
    BTreeMap::from([
        (ProposalState::Draft, 0),
        (ProposalState::HeldForReview, held),
        (ProposalState::NeedsRevision, 0),
        (ProposalState::Accepted, accepted),
        (ProposalState::Declined, declined),
        (ProposalState::Withdrawn, withdrawn),
        (ProposalState::Superseded, 0),
    ])
}

#[test]
fn the_metrics_agree_with_a_hand_count_on_a_fixture() {
    on_both(|e| {
        let _fx = fixture(e);
        let period = Period {
            from: 1000,
            to: 2000,
        };
        set_now(3000);
        let m: Metrics = e
            .service
            .metrics(&e.admin.principal, &e.wiki, period)
            .unwrap();

        // Submitted in [1000, 2000): P1, P2, P3, P4. P0 was before; P6 after; the drafts were never sent.
        assert_eq!(m.submitted, 4);
        // By the state each is in now: P1 accepted, P2 declined, P3 withdrawn, P4 held.
        assert_eq!(m.by_state, expected_by_state(1, 1, 1, 1));
        // To the first review: P1 100, P3 300 (B's own decision at 1450 is not a review), P4 50; P2 had none.
        // Sorted 50, 100, 300: the middle is 100.
        assert_eq!(m.median_secs_to_first_review, Some(100));
        // To a decision: only those accepted or declined. P1 500, P2 60: the lower middle of (60, 500) is 60.
        assert_eq!(m.median_secs_to_decision, Some(60));
        // Of the two decided, one was accepted and one declined.
        assert_eq!(m.acceptance_rate, Some(Ratio { num: 1, den: 2 }));
        assert_eq!(m.decline_rate, Some(Ratio { num: 1, den: 2 }));
        // A, B and C proposed; A had proposed before the period, B and C had not.
        assert_eq!(m.distinct_proposers, 3);
        assert_eq!(m.first_time_proposers, 2);
        // Distinct reviewers other than the author: P1 two, P2 none, P3 one (R1), P4 one: four over four.
        assert_eq!(m.reviewers_per_proposal, Some(Ratio { num: 4, den: 4 }));
        assert_eq!(m.period, period);
        assert_eq!(m.wiki, e.wiki.id);
    });
}

#[test]
fn another_period_over_the_same_records_gives_its_own_figures() {
    on_both(|e| {
        let _fx = fixture(e);
        set_now(3000);
        // Before the period: only P0, accepted at 600 after submission at 500.
        let early = e
            .service
            .metrics(&e.admin.principal, &e.wiki, Period { from: 0, to: 1000 })
            .unwrap();
        assert_eq!(early.submitted, 1);
        assert_eq!(early.by_state, expected_by_state(0, 1, 0, 0));
        assert_eq!(
            early.median_secs_to_first_review, None,
            "nobody reviewed it"
        );
        assert_eq!(early.median_secs_to_decision, Some(100));
        assert_eq!(early.acceptance_rate, Some(Ratio { num: 1, den: 1 }));
        assert_eq!(early.decline_rate, Some(Ratio { num: 0, den: 1 }));
        assert_eq!(
            (early.distinct_proposers, early.first_time_proposers),
            (1, 1)
        );
        assert_eq!(early.reviewers_per_proposal, Some(Ratio { num: 0, den: 1 }));
        // After it: P6 alone, by C, who proposed in the period already: not a first-time proposer.
        let late = e
            .service
            .metrics(
                &e.admin.principal,
                &e.wiki,
                Period {
                    from: 2000,
                    to: 9999,
                },
            )
            .unwrap();
        assert_eq!(late.submitted, 1);
        assert_eq!(late.by_state, expected_by_state(1, 0, 0, 0));
        assert_eq!((late.distinct_proposers, late.first_time_proposers), (1, 0));
        assert_eq!(
            late.acceptance_rate, None,
            "a share of nothing is not a number"
        );
        // An empty period.
        let none = e
            .service
            .metrics(
                &e.admin.principal,
                &e.wiki,
                Period {
                    from: 5000,
                    to: 6000,
                },
            )
            .unwrap();
        assert_eq!(none.submitted, 0);
        assert_eq!(none.by_state, expected_by_state(0, 0, 0, 0));
        assert_eq!(none.reviewers_per_proposal, None);
        assert_eq!(none.median_secs_to_first_review, None);
        // The period is half-open: the second a proposal was submitted is in, the second it ends is out.
        let edge = e
            .service
            .metrics(
                &e.admin.principal,
                &e.wiki,
                Period {
                    from: 1000,
                    to: 1001,
                },
            )
            .unwrap();
        assert_eq!(edge.submitted, 1);
        let before = e
            .service
            .metrics(
                &e.admin.principal,
                &e.wiki,
                Period {
                    from: 999,
                    to: 1000,
                },
            )
            .unwrap();
        assert_eq!(before.submitted, 0);
    });
}

#[test]
fn the_figures_are_an_administrators_and_a_draft_is_in_none_of_them() {
    on_both(|e| {
        let fx = fixture(e);
        let period = Period {
            from: 0,
            to: 10_000,
        };
        for who in [&e.maintainer, &e.reviewer, &fx.a, &fx.b, &fx.c, &e.outsider] {
            assert_eq!(
                e.service.metrics(&who.principal, &e.wiki, period),
                Err(ProposalError::NotAllowed)
            );
        }
        let m = e
            .service
            .metrics(&e.admin.principal, &e.wiki, period)
            .unwrap();
        // P0 to P4 and P6 were sent: six. The two drafts that were not are nowhere.
        assert_eq!(m.submitted, 6);
        assert_eq!(m.by_state.get(&ProposalState::Draft), Some(&0));
        assert_eq!(m.by_state.values().sum::<u64>(), 6);
    });
}

/// One member of a parsed JSON record by pointer (tests may not index).
fn member<'a>(v: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    v.pointer(pointer).unwrap()
}

#[test]
fn the_operator_gets_the_same_figures_as_one_line_of_json() {
    on_both(|e| {
        let _fx = fixture(e);
        let period = Period {
            from: 1000,
            to: 2000,
        };
        set_now(3000);
        let by_service = e
            .service
            .metrics(&e.admin.principal, &e.wiki, period)
            .unwrap();
        let by_operator = crate::proposal::operator_metrics(
            &*e.store as &dyn inklkunn_store::ProposalStore,
            &e.wiki,
            period,
        )
        .unwrap();
        assert_eq!(
            by_operator, by_service,
            "the command line asks no principal, not a different question"
        );

        let line = by_operator.json_line();
        assert!(!line.contains('\n'), "one record is one line: {line}");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(member(&v, "/kind"), "participation");
        assert_eq!(member(&v, "/wiki"), e.wiki.id.as_str());
        assert_eq!(
            (member(&v, "/from").as_i64(), member(&v, "/to").as_i64()),
            (Some(1000), Some(2000))
        );
        assert_eq!(member(&v, "/submitted"), 4);
        assert_eq!(member(&v, "/median_secs_to_first_review"), 100);
        assert_eq!(member(&v, "/median_secs_to_decision"), 60);
        // A share is the two whole numbers, never a rounded figure.
        assert_eq!(member(&v, "/acceptance_rate/num"), 1);
        assert_eq!(member(&v, "/acceptance_rate/den"), 2);
        assert_eq!(member(&v, "/reviewers_per_proposal/den"), 4);
        // Every state is present, by its stored name.
        let states = member(&v, "/by_state").as_object().unwrap();
        assert_eq!(states.len(), 7, "{states:?}");
        assert_eq!(states.get("accepted").unwrap(), 1);
        assert_eq!(states.get("draft").unwrap(), 0);
        assert_eq!(
            (
                member(&v, "/distinct_proposers").as_u64(),
                member(&v, "/first_time_proposers").as_u64()
            ),
            (Some(3), Some(2))
        );

        // Nothing to say is null, not zero: a period with no proposals has no median and no rate.
        let empty = crate::proposal::operator_metrics(
            &*e.store as &dyn inklkunn_store::ProposalStore,
            &e.wiki,
            Period { from: 10, to: 20 },
        )
        .unwrap()
        .json_line();
        let v: serde_json::Value = serde_json::from_str(&empty).unwrap();
        assert!(
            member(&v, "/median_secs_to_first_review").is_null()
                && member(&v, "/acceptance_rate").is_null()
        );
        assert_eq!(member(&v, "/submitted"), 0);
    });
}
