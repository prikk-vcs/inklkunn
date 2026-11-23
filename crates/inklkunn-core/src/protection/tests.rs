#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The pure half of protections (RFC 006 D-7; handoff G-T3): which protections cover which titles, how the clock ends a
//! temporary one, and what the stricter of several asks. Where they are kept and decided is in `authorize/protections.rs`'s
//! and the acceptance's tests.

use inklkunn_store::{
    AccountId, Actor, MeasureEvent, ProtectionEvent, ProtectionEventId, ProtectionKind,
    ProtectionRequirement, Role, WikiId,
};

use super::*;
use crate::governance::{PlatformLimits, validate_policy};

const NOW: i64 = 1_000_000;

fn policy(text: &str) -> Policy {
    validate_policy(text, &PlatformLimits::default()).expect("a valid policy")
}

fn record(
    kind: ProtectionKind,
    target: &str,
    requirement: ProtectionRequirement,
    reason: &str,
    expires_at: i64,
) -> ProtectionEvent {
    ProtectionEvent {
        id: ProtectionEventId::new(),
        wiki: WikiId::new(),
        at: NOW - 10,
        event: MeasureEvent::Set,
        kind,
        target: target.to_owned(),
        requirement: Some(requirement),
        by: Actor {
            account: AccountId::new(),
            rung: Role::Maintainer,
        },
        policy_block: None,
        reason: Some(reason.to_owned()),
        expires_at: Some(expires_at),
        of_event: None,
        appeal: None,
    }
}

fn titles(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| (*t).to_owned()).collect()
}

#[test]
fn a_page_title_comes_from_a_page_path_and_nothing_else_has_one() {
    assert_eq!(
        titles_of(["pages/Help/Start.md", "pages/Home.md", "pages/Home.md"]),
        titles(&["Help/Start", "Home"])
    );
    // A governance file, a knowledge file and a path that is not a page's have no title, so no protection.
    assert!(
        titles_of([
            "governance/policy.yaml",
            "knowledge/claims/c-1.yaml",
            "pages/.md",
            "Home.md"
        ])
        .is_empty()
    );
    // A rename's two paths are two titles.
    assert_eq!(
        titles_of(["pages/Old.md", "pages/New.md"]),
        titles(&["Old", "New"])
    );
}

#[test]
fn a_namespace_covers_whole_segments_below_it_and_never_the_page_of_its_own_name() {
    let p =
        policy("protect:\n  - namespace: Help\n    require: review\n    reason: \"Read first.\"\n");
    let found = |title: &str| !notices_for(&p, &[], &titles(&[title]), NOW).is_empty();
    assert!(found("Help/x"));
    assert!(found("Help/x/y"));
    // Off by one segment, either way: a namespace that is a prefix of text, a page of the namespace's own name.
    assert!(!found("Helpful/x"));
    assert!(!found("Help"));
    assert!(!found("Hel/p"));
    let shorter =
        policy("protect:\n  - namespace: Hel\n    require: review\n    reason: \"Read first.\"\n");
    assert!(notices_for(&shorter, &[], &titles(&["Help/x"]), NOW).is_empty());
    assert!(notices_for(&shorter, &[], &titles(&["Help"]), NOW).is_empty());
}

#[test]
fn a_page_protection_covers_that_page_alone() {
    let p = policy("protect:\n  - path: Help\n    require: review\n    reason: \"Read first.\"\n");
    assert_eq!(notices_for(&p, &[], &titles(&["Help"]), NOW).len(), 1);
    assert!(notices_for(&p, &[], &titles(&["Help/x"]), NOW).is_empty());
    assert!(notices_for(&p, &[], &titles(&["help"]), NOW).is_empty());
}

#[test]
fn a_temporary_protection_ends_at_its_expiry_by_the_clock_and_not_a_second_later() {
    let until = NOW + 100;
    let event = record(
        ProtectionKind::Page,
        "Home",
        ProtectionRequirement::Review,
        "Vandalism.",
        until,
    );
    let at = |t: i64| {
        notices_for(
            &Policy::default(),
            std::slice::from_ref(&event),
            &titles(&["Home"]),
            t,
        )
    };
    assert_eq!(at(until - 1).len(), 1, "the second before");
    assert!(at(until).is_empty(), "the instant of expiry");
    assert!(at(until + 1).is_empty(), "after");
    let notice = at(until - 1).pop().unwrap();
    assert_eq!(notice.kind(), Kind::Temporary { until });
    assert_eq!(notice.until(), Some(until));
    assert_eq!(notice.reason(), "Vandalism.");
    assert_eq!(notice.require(), Require::Review);
}

#[test]
fn a_record_that_ends_a_protection_is_not_one() {
    let mut lifted = record(
        ProtectionKind::Page,
        "Home",
        ProtectionRequirement::Review,
        "x",
        NOW + 100,
    );
    lifted.event = MeasureEvent::Lifted;
    assert_eq!(Notice::of_record(&lifted, NOW), None);
    let mut renewed = lifted.clone();
    renewed.event = MeasureEvent::Renewed;
    assert!(Notice::of_record(&renewed, NOW).is_some());
}

#[test]
fn both_kinds_are_listed_once_each_and_the_stricter_decides() {
    let p = policy(
        "protect:\n  - namespace: Policies\n    require:\n      rung: maintainer\n    reason: \"Standing.\"\n",
    );
    let temporary = [
        record(
            ProtectionKind::Page,
            "Policies/Conduct",
            ProtectionRequirement::Rung(Role::Admin),
            "Dispute.",
            NOW + 5,
        ),
        // The same protection read twice is listed once.
        record(
            ProtectionKind::Page,
            "Policies/Conduct",
            ProtectionRequirement::Rung(Role::Admin),
            "Dispute.",
            NOW + 5,
        ),
        record(
            ProtectionKind::Namespace,
            "Policies",
            ProtectionRequirement::Review,
            "Look first.",
            NOW + 9,
        ),
        // Another page: not listed.
        record(
            ProtectionKind::Page,
            "Home",
            ProtectionRequirement::Rung(Role::Admin),
            "Elsewhere.",
            NOW + 9,
        ),
    ];
    let notices = notices_for(&p, &temporary, &titles(&["Policies/Conduct"]), NOW);
    assert_eq!(notices.len(), 3);
    assert_eq!(notices.first().map(Notice::kind), Some(Kind::Standing));
    let demand = Demand::of(&notices);
    assert_eq!(demand.rung, Some(Role::Admin), "the higher rung wins");
    assert!(demand.review, "a review is asked by one of them");
    // A lower rung beside a higher one does not lower it.
    let only_low = notices_for(&p, &[], &titles(&["Policies/Conduct"]), NOW);
    assert_eq!(Demand::of(&only_low).rung, Some(Role::Maintainer));
}

#[test]
fn what_is_lacking_is_the_rung_first_then_the_review() {
    let demand = Demand {
        rung: Some(Role::Maintainer),
        review: true,
    };
    assert_eq!(
        demand.lacks(Some(Role::Reviewer), 5),
        Some(Lack::Rung(Role::Maintainer))
    );
    assert_eq!(demand.lacks(None, 5), Some(Lack::Rung(Role::Maintainer)));
    assert_eq!(demand.lacks(Some(Role::Maintainer), 0), Some(Lack::Review));
    assert_eq!(demand.lacks(Some(Role::Admin), 1), None);
    assert!(Demand::default().is_empty());
    assert_eq!(Demand::default().lacks(None, 0), None);
}
