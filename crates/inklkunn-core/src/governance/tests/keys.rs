//! Each key of the policy and of the constitution: its default, a value that is not the default, and what a file
//! that says only that key means.

use super::*;
use crate::authorize::Action;

#[test]
fn entry_has_its_default_and_each_word() {
    assert_eq!(policy("entry: none\n").entry(), Entry::None);
    for (word, entry, rung) in [
        ("observer", Entry::Observer, Role::Observer),
        ("questioner", Entry::Questioner, Role::Questioner),
        ("contributor", Entry::Contributor, Role::Contributor),
    ] {
        let p = policy(&format!("entry: {word}\n"));
        assert_eq!(p.entry(), entry);
        assert_eq!(p.entry().rung(), Some(rung));
    }
    // Entry says nothing about what a rung may do.
    assert_eq!(
        policy("entry: contributor\n").minimum_role(Action::Propose),
        Role::Contributor
    );
}

#[test]
fn each_rung_has_its_default_and_takes_any_rung_of_the_ladder() {
    type Of = fn(&Policy) -> Role;
    let keys: [(&str, Of, Role); 6] = [
        ("comment", |p| p.rungs().comment(), Role::Questioner),
        ("propose", |p| p.rungs().propose(), Role::Contributor),
        ("review", |p| p.rungs().review(), Role::Reviewer),
        ("create_view", |p| p.rungs().create_view(), Role::Reviewer),
        ("accept", |p| p.rungs().accept(), Role::Maintainer),
        ("moderate", |p| p.rungs().moderate(), Role::Maintainer),
    ];
    for (key, of, default) in keys {
        assert_eq!(of(&Policy::default()), default, "{key}");
        // A key left out is its default, whatever else the file says.
        assert_eq!(of(&policy("rungs:\n  comment: observer\n")), {
            if key == "comment" {
                Role::Observer
            } else {
                default
            }
        });
        for rung in Role::ALL {
            // Set alone, a rung that obeys the ordering rule against the defaults is read; the others are refused
            // by the rule (tested by name in `refusals`).
            let text = format!("rungs:\n  {key}: {}\n", rung.as_str());
            if let Ok(p) = validate_policy(&text, &PlatformLimits::default()) {
                assert_eq!(of(&p), rung, "{key}: {}", rung.as_str());
            }
        }
    }
    // A rung that is not the default, read in a policy that holds the ordering rule.
    let p = policy(
        "rungs:\n  comment: observer\n  propose: questioner\n  review: contributor\n  create_view: maintainer\n  accept: reviewer\n  moderate: admin\n",
    );
    assert_eq!(p.minimum_role(Action::Comment), Role::Observer);
    assert_eq!(p.minimum_role(Action::Propose), Role::Questioner);
    assert_eq!(p.minimum_role(Action::Review), Role::Contributor);
    assert_eq!(p.minimum_role(Action::CreateView), Role::Maintainer);
    assert_eq!(p.minimum_role(Action::Accept), Role::Reviewer);
    assert_eq!(p.minimum_role(Action::Moderate), Role::Admin);
}

#[test]
fn decide_appeal_is_a_maintainer_or_an_admin_and_nothing_else() {
    assert_eq!(
        policy("rungs:\n  decide_appeal: maintainer\n")
            .rungs()
            .decide_appeal()
            .role(),
        Role::Maintainer
    );
    assert_eq!(
        policy("rungs:\n  decide_appeal: admin\n")
            .rungs()
            .decide_appeal(),
        AppealRung::Admin
    );
    for bad in ["reviewer", "observer", "none", "root"] {
        let f = policy_fault(&format!("rungs:\n  decide_appeal: {bad}\n"));
        assert_eq!(f.refusal, Refusal::UnknownValue, "{bad}");
        assert_eq!(f.field, "rungs.decide_appeal");
    }
}

#[test]
fn grant_up_to_runs_from_none_to_reviewer() {
    for (word, highest) in [
        ("none", None),
        ("observer", Some(Role::Observer)),
        ("questioner", Some(Role::Questioner)),
        ("contributor", Some(Role::Contributor)),
        ("reviewer", Some(Role::Reviewer)),
    ] {
        let p = policy(&format!("rungs:\n  grant_up_to: {word}\n"));
        assert_eq!(p.rungs().grant_up_to().highest(), highest, "{word}");
        for rung in Role::ALL {
            assert_eq!(
                p.rungs().grant_up_to().maintainer_may_grant(rung),
                highest.is_some_and(|h| rung <= h),
                "{word} grants {}",
                rung.as_str()
            );
        }
    }
    // A maintainer never grants the maintainer or admin rung: not a word of the key.
    for bad in ["maintainer", "admin"] {
        let f = policy_fault(&format!("rungs:\n  grant_up_to: {bad}\n"));
        assert_eq!(f.refusal, Refusal::UnknownValue, "{bad}");
    }
}

#[test]
fn approvals_run_from_zero_to_five() {
    assert_eq!(policy("").acceptance().approvals(), 0);
    for n in 0..=5u8 {
        let p = policy(&format!("acceptance:\n  approvals: {n}\n"));
        assert_eq!(p.acceptance().approvals(), n);
    }
    for bad in ["6", "-1", "100", "9223372036854775807"] {
        let f = policy_fault(&format!("acceptance:\n  approvals: {bad}\n"));
        assert_eq!(f.refusal, Refusal::OutOfRange, "{bad}");
        assert_eq!(f.field, "acceptance.approvals");
    }
    for bad in ["two", "1.5", "\"2\"", "yes", "[2]"] {
        let f = policy_fault(&format!("acceptance:\n  approvals: {bad}\n"));
        assert_eq!(f.refusal, Refusal::WrongShape, "{bad}");
    }
}

#[test]
fn acceptance_self_is_never_or_small_wiki() {
    assert_eq!(
        policy("").acceptance().self_acceptance(),
        SelfAcceptance::SmallWiki
    );
    let never = policy("acceptance:\n  self: never\n");
    assert_eq!(never.acceptance().self_acceptance(), SelfAcceptance::Never);
    for maintainers in 0..10 {
        assert!(!never.acceptance().may_self_accept(maintainers));
    }
    let small = policy("acceptance:\n  self: small-wiki\n");
    assert!(small.acceptance().may_self_accept(2));
    assert!(!small.acceptance().may_self_accept(3));
    for bad in ["always", "small_wiki", "Never", "small"] {
        let f = policy_fault(&format!("acceptance:\n  self: {bad}\n"));
        assert_eq!(f.refusal, Refusal::UnknownValue, "{bad}");
    }
}

#[test]
fn knowledge_review_after_is_the_wikis_own_or_the_operators() {
    assert_eq!(policy("").review_after_days(1096), 1096);
    assert_eq!(policy("").review_after(), None);
    let p = policy("knowledge:\n  review_after: 30\n");
    assert_eq!(p.review_after(), Some(30));
    assert_eq!(p.review_after_days(1096), 30);
    assert_eq!(
        policy("knowledge:\n  review_after: 1\n").review_after(),
        Some(1)
    );
    assert_eq!(
        policy("knowledge:\n  review_after: 36500\n").review_after(),
        Some(36_500)
    );
    for bad in ["0", "-5", "36501"] {
        let f = policy_fault(&format!("knowledge:\n  review_after: {bad}\n"));
        assert_eq!(f.refusal, Refusal::OutOfRange, "{bad}");
        assert_eq!(f.field, "knowledge.review_after");
    }
    assert_eq!(
        policy_fault("knowledge:\n  review_after: soon\n").refusal,
        Refusal::WrongShape
    );
}

#[test]
fn promotion_texts_are_for_any_rung_and_nothing_promotes_by_them() {
    assert_eq!(policy("").promotion(Role::Contributor), None);
    let p = policy(
        "promotion:\n  contributor: \"A few edits that were accepted.\"\n  admin: \"Asked by the maintainers.\"\n",
    );
    assert_eq!(
        p.promotion(Role::Contributor),
        Some("A few edits that were accepted.")
    );
    assert_eq!(p.promotion(Role::Admin), Some("Asked by the maintainers."));
    assert_eq!(p.promotion(Role::Reviewer), None);
    assert_eq!(p.promotions().count(), 2);
    // Every one of the six rungs may carry one.
    for rung in Role::ALL {
        let p = policy(&format!("promotion:\n  {}: \"Text.\"\n", rung.as_str()));
        assert_eq!(p.promotion(rung), Some("Text."));
    }
    // Longest allowed, and one more.
    let longest = "a".repeat(MAX_PROMOTION_CHARS);
    assert!(
        validate_policy(
            &format!("promotion:\n  reviewer: \"{longest}\"\n"),
            &PlatformLimits::default()
        )
        .is_ok()
    );
    let f = policy_fault(&format!("promotion:\n  reviewer: \"{longest}a\"\n"));
    assert_eq!(
        (f.refusal, f.field),
        (Refusal::Bounds, "promotion.reviewer")
    );
    // A name that is not a rung is not a promotion.
    let f = policy_fault("promotion:\n  owner: \"x\"\n");
    assert_eq!((f.refusal, f.field), (Refusal::UnknownKey, "promotion"));
    // Empty is missing, a number is not a text.
    assert_eq!(
        policy_fault("promotion:\n  reviewer: \"  \"\n").refusal,
        Refusal::Missing
    );
    assert_eq!(
        policy_fault("promotion:\n  reviewer: 4\n").refusal,
        Refusal::WrongShape
    );
}

#[test]
fn the_constitution_has_its_defaults_and_each_key() {
    let d = Constitution::default();
    assert_eq!(validate_constitution("").unwrap(), d);
    let c = validate_constitution(
        "policy_change:\n  rung: admin\n  approvals: 2\nconstitution_change:\n  approvals: 3\n",
    )
    .unwrap();
    assert_eq!(c.policy_change().rung, Role::Admin);
    assert_eq!(c.policy_change().approvals, 2);
    assert_eq!(c.constitution_change().approvals, 3);
    // The constitution's rung is always admin.
    assert_eq!(c.constitution_change().rung, Role::Admin);
    // Each key alone leaves the others at their default.
    let c = validate_constitution("policy_change:\n  approvals: 1\n").unwrap();
    assert_eq!(c.policy_change().rung, Role::Maintainer);
    assert_eq!(c.constitution_change(), d.constitution_change());
    let c = validate_constitution("constitution_change:\n  approvals: 5\n").unwrap();
    assert_eq!(c.policy_change(), d.policy_change());
    for bad in ["6", "-1"] {
        let f = constitution_fault(&format!("policy_change:\n  approvals: {bad}\n"));
        assert_eq!(f.refusal, Refusal::OutOfRange);
        assert_eq!(f.field, "policy_change.approvals");
        let f = constitution_fault(&format!("constitution_change:\n  approvals: {bad}\n"));
        assert_eq!(f.field, "constitution_change.approvals");
    }
}

#[test]
fn a_policy_changes_rung_is_a_maintainer_or_an_admin() {
    for ok in ["maintainer", "admin"] {
        assert!(validate_constitution(&format!("policy_change:\n  rung: {ok}\n")).is_ok());
    }
    for bad in ["reviewer", "contributor", "observer", "owner"] {
        let f = constitution_fault(&format!("policy_change:\n  rung: {bad}\n"));
        assert_eq!(f.refusal, Refusal::UnknownValue, "{bad}");
        assert_eq!(f.field, "policy_change.rung");
    }
}

#[test]
fn a_constitution_changes_rung_is_fixed_and_writing_it_is_an_unknown_key() {
    for rung in ["admin", "maintainer", "reviewer"] {
        let f = constitution_fault(&format!("constitution_change:\n  rung: {rung}\n"));
        assert_eq!(f.refusal, Refusal::UnknownKey, "{rung}");
        assert_eq!(f.field, "constitution_change");
        assert_eq!(f.line, Some(2));
    }
}

#[test]
fn what_a_change_needs_is_the_constitutions_word() {
    let c = validate_constitution(
        "policy_change:\n  rung: admin\n  approvals: 2\nconstitution_change:\n  approvals: 4\n",
    )
    .unwrap();
    assert_eq!(
        GovernanceChange::Policy.required(&c),
        Requirement {
            rung: Role::Admin,
            approvals: 2
        }
    );
    assert_eq!(
        GovernanceChange::Constitution.required(&c),
        Requirement {
            rung: Role::Admin,
            approvals: 4
        }
    );
    let d = Constitution::default();
    assert_eq!(GovernanceChange::Policy.required(&d).rung, Role::Maintainer);
    assert_eq!(GovernanceChange::Policy.required(&d).approvals, 0);
    assert_eq!(
        GovernanceChange::Constitution.required(&d).rung,
        Role::Admin
    );
    assert_eq!(
        GovernanceChange::of_path(POLICY_PATH),
        Some(GovernanceChange::Policy)
    );
    assert_eq!(
        GovernanceChange::of_path(CONSTITUTION_PATH),
        Some(GovernanceChange::Constitution)
    );
    assert_eq!(GovernanceChange::of_path("governance/other.yaml"), None);
}

#[test]
fn a_written_out_policy_is_read_back_as_the_same_policy() {
    let text = "entry: questioner\nrungs:\n  comment: observer\n  propose: questioner\n  review: contributor\n  create_view: maintainer\n  accept: reviewer\n  moderate: reviewer\n  decide_appeal: maintainer\n  grant_up_to: contributor\nacceptance:\n  approvals: 3\n  self: never\nknowledge:\n  review_after: 90\npromotion:\n  reviewer: \"Read \\\"carefully\\\" # always.\"\n  maintainer: \"Trusted for a year.\"\nprotect:\n  - namespace: Policies\n    require: review\n    reason: \"Rules are read first.\"\n  - path: Home\n    require:\n      rung: admin\n    reason: \"The front page: one hand.\"\n";
    let p = policy(text);
    assert_eq!(p.entry(), Entry::Questioner);
    assert_eq!(p.protections().len(), 2);
    let again = policy(&p.to_yaml());
    assert_eq!(again, p);
    assert_eq!(
        again.promotion(Role::Reviewer),
        Some("Read \"carefully\" # always.")
    );
    // And it is canonical: writing it again changes nothing.
    assert_eq!(again.to_yaml(), p.to_yaml());
    let c = validate_constitution(
        "policy_change:\n  rung: admin\n  approvals: 5\nconstitution_change:\n  approvals: 1\n",
    )
    .unwrap();
    assert_eq!(validate_constitution(&c.to_yaml()).unwrap(), c);
}
