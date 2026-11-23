//! RFC 006 acceptance criterion 1, the rule above every task: **a wiki with no `governance/` decides every action
//! exactly as 0.3.0 does.** The table below is 0.3.0's authorization (RFC 002 D-2), written out as plain data and **not
//! derived from the code under test**: it is what the 0.3.0 release does, and it does not move when a default does.

use super::*;
use crate::authorize::Action;

/// For each of the ten actions, the lowest role that may do it in 0.3.0.
const TABLE_0_3_0: [(Action, Role); 10] = [
    (Action::Read, Role::Observer),
    (Action::Comment, Role::Questioner),
    (Action::Propose, Role::Contributor),
    (Action::Review, Role::Reviewer),
    (Action::CreateView, Role::Reviewer),
    (Action::Accept, Role::Maintainer),
    (Action::Moderate, Role::Maintainer),
    (Action::Admin, Role::Admin),
    (Action::Export, Role::Admin),
    (Action::Import, Role::Admin),
];

/// 0.3.0's self-acceptance rule: a maintainer may accept their own proposal only while the wiki has fewer than three
/// maintainers. (Maintainers counted, may accept their own.)
const SELF_ACCEPTANCE_0_3_0: [(usize, bool); 7] = [
    (0, true),
    (1, true),
    (2, true),
    (3, false),
    (4, false),
    (10, false),
    (usize::MAX, false),
];

#[test]
fn the_table_has_every_action_once() {
    for action in Action::ALL {
        assert_eq!(
            TABLE_0_3_0.iter().filter(|(a, _)| *a == action).count(),
            1,
            "{} is in the table once",
            action.name()
        );
    }
    assert_eq!(TABLE_0_3_0.len(), Action::ALL.len());
}

#[test]
fn the_default_policy_answers_the_0_3_0_table() {
    let policy = Policy::default();
    for (action, lowest) in TABLE_0_3_0 {
        assert_eq!(
            policy.minimum_role(action),
            lowest,
            "the default policy for {}",
            action.name()
        );
    }
}

#[test]
fn todays_minimum_role_agrees_with_the_same_table() {
    for (action, lowest) in TABLE_0_3_0 {
        assert_eq!(
            action.minimum_role(),
            lowest,
            "Action::minimum_role for {}",
            action.name()
        );
    }
}

#[test]
fn the_default_policy_accepts_with_no_approvals_and_the_small_wiki_rule() {
    let policy = Policy::default();
    assert_eq!(policy.acceptance().approvals(), 0);
    for (maintainers, may) in SELF_ACCEPTANCE_0_3_0 {
        assert_eq!(
            policy.acceptance().may_self_accept(maintainers),
            may,
            "{maintainers} maintainers"
        );
    }
}

#[test]
fn the_other_defaults_are_what_0_3_0_does() {
    let policy = Policy::default();
    // No entry rung: an account holds what it was granted and nothing else.
    assert_eq!(policy.entry(), Entry::None);
    assert_eq!(policy.entry().rung(), None);
    // No appeals existed; an admin decides. Only an admin grants.
    assert_eq!(policy.rungs().decide_appeal().role(), Role::Admin);
    // The ordering rule holds for the defaults (criterion 1): whoever decides appeals may moderate (admin >= maintainer).
    assert!(policy.rungs().decide_appeal().role() >= policy.rungs().moderate());
    for rung in Role::ALL {
        assert!(!policy.rungs().grant_up_to().maintainer_may_grant(rung));
    }
    // The operator's interval is the interval; no text, no protection.
    assert_eq!(policy.review_after(), None);
    assert_eq!(policy.review_after_days(1096), 1096);
    assert_eq!(policy.promotions().count(), 0);
    assert!(policy.protections().is_empty());
    // Constitution: a maintainer changes the policy and an admin the constitution, with no approvals.
    let c = Constitution::default();
    assert_eq!(
        (c.policy_change().rung, c.policy_change().approvals),
        (Role::Maintainer, 0)
    );
    assert_eq!(
        (
            c.constitution_change().rung,
            c.constitution_change().approvals
        ),
        (Role::Admin, 0)
    );
    // The platform: contributor at most, and ninety days.
    let limits = PlatformLimits::default();
    assert_eq!(
        (limits.entry_max, limits.limit_days),
        (Entry::Contributor, 90)
    );
}

#[test]
fn no_file_and_an_empty_file_and_the_written_out_defaults_are_one_policy() {
    let written = Policy::default().to_yaml();
    for text in ["", "\n", "# nothing\n", "{}\n", written.as_str()] {
        assert_eq!(policy(text), Policy::default(), "{text:?}");
    }
    assert_eq!(validate_constitution("").unwrap(), Constitution::default());
    assert_eq!(
        validate_constitution(&Constitution::default().to_yaml()).unwrap(),
        Constitution::default()
    );
}

#[test]
fn the_self_acceptance_number_is_acceptances_own() {
    // The governance default is acceptance.rs's constant, restated (that file is not this module's to edit);
    // acceptance should re-export this one when it reads the policy.
    assert_eq!(
        crate::governance::defaults::SELF_ACCEPTANCE_BELOW,
        crate::acceptance::SELF_ACCEPTANCE_BELOW
    );
}

#[test]
fn the_admin_actions_and_reading_are_not_the_policys_to_change() {
    // Every rung the ladder has, set for the six actions the policy governs: reading, admin, export and import do
    // not move.
    for rung in Role::ALL {
        let moderate = rung.max(Role::Reviewer);
        let text = format!(
            "rungs:\n  comment: {r}\n  propose: {r}\n  review: {r}\n  create_view: {r}\n  accept: {r}\n  moderate: {m}\n",
            r = rung.as_str(),
            m = moderate.as_str(),
        );
        let p = policy(&text);
        assert_eq!(p.minimum_role(Action::Read), Role::Observer);
        for action in [Action::Admin, Action::Export, Action::Import] {
            assert_eq!(p.minimum_role(action), Role::Admin, "{text}");
        }
    }
}
