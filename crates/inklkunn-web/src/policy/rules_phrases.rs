//! The rules of a wiki, said in sentences (RFC 006 D-4, D-11): for each key of the files, **a label and a value**, in the
//! reader's language and in the glossary's words (*who may accept: maintainer or above*), and never as the file writes it.
//!
//! A [`Fact`] is one rule as the policy page lists it and as the proposal page compares it (*what it said → what it says*, in
//! [`rules_change`](super::rules_change)). **The one list of facts** is [`policy_facts`] and [`constitution_facts`]: a key of the
//! schema that has no fact here is found by the test that reads the files' own keys against [`COVERED`] and fails (so adding
//! a key to the schema without a sentence is a failing test, not a page that says nothing of it).
//!
//! Nothing here is a decision: every value is read from the core's types.

use inklkunn_core::governance::{
    Constitution, Entry, GrantUpTo, Policy, Protection, Require, SelfAcceptance, Target, defaults,
};
use inklkunn_store::Role;

use crate::catalog::{Key, T};
use crate::people::sentences::role_label;

/// One rule, as a label and a value, with the keys of the files it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fact {
    /// The keys of the files this fact says (a path through the files' mappings: `rungs.accept`), so that a test can hold that
    /// every key of the schema is said by one. Never shown.
    pub(crate) paths: &'static [&'static str],
    /// The rule's name, as a sentence's beginning (*Who may accept a proposal*).
    pub(crate) label: String,
    /// What it says now.
    pub(crate) value: String,
}

/// Every key of the two files that a fact says, and the keys that are said otherwise (`protect`, in the sentences of a
/// protection). The test reads the files' own keys against this list.
#[cfg(test)]
pub(crate) const COVERED: &[&str] = &[
    "entry",
    "rungs.comment",
    "rungs.propose",
    "rungs.review",
    "rungs.create_view",
    "rungs.accept",
    "rungs.moderate",
    "rungs.decide_appeal",
    "rungs.grant_up_to",
    "acceptance.approvals",
    "acceptance.self",
    "knowledge.review_after",
    "promotion.observer",
    "promotion.questioner",
    "promotion.contributor",
    "promotion.reviewer",
    "promotion.maintainer",
    "promotion.admin",
    "protect",
    "policy_change.rung",
    "policy_change.approvals",
    "constitution_change.approvals",
];

/// A role and every role above it, as a value: *maintainer or above*; the highest alone is *admin only*.
pub(crate) fn rung_value(t: T, role: Role) -> String {
    let key = if role == Role::Admin {
        Key::PolicyValueRungTop
    } else {
        Key::PolicyValueRung
    };
    t.msg(key).with("role", role_label(t, role)).text()
}

fn approvals_value(t: T, approvals: u8) -> String {
    if approvals == 0 {
        t.get(Key::PolicyValueApprovalsNone)
    } else {
        t.msg(Key::PolicyValueApprovals)
            .n(u64::from(approvals))
            .text()
    }
}

/// The label of the promotion text of a role.
pub(crate) fn promotion_label(t: T, role: Role) -> String {
    t.msg(Key::PolicyLabelPromotion)
        .with("role", role_label(t, role))
        .text()
}

/// What *every person who signs in holds* says.
pub(crate) fn entry_value(t: T, entry: Entry) -> String {
    entry.rung().map_or_else(
        || t.get(Key::PolicyValueEntryNone),
        |role| role_label(t, role),
    )
}

/// What *the highest role a maintainer may grant* says.
pub(crate) fn grant_value(t: T, up_to: GrantUpTo) -> String {
    up_to.highest().map_or_else(
        || t.get(Key::PolicyValueGrantNone),
        |role| {
            t.msg(Key::PolicyValueGrantUpTo)
                .with("role", role_label(t, role))
                .text()
        },
    )
}

/// What *accepting your own proposal* says.
pub(crate) fn self_value(t: T, own: SelfAcceptance) -> String {
    match own {
        SelfAcceptance::Never => t.get(Key::PolicyValueSelfNever),
        SelfAcceptance::SmallWiki => t
            .msg(Key::PolicyValueSelfSmall)
            .with("limit", defaults::SELF_ACCEPTANCE_BELOW)
            .text(),
    }
}

/// What *a source is due for review after* says: the wiki's own days, or the server's.
pub(crate) fn review_value(t: T, own: Option<u32>, server_days: u32) -> String {
    match own {
        Some(days) => t.msg(Key::PolicyValueReviewDays).with("days", days).text(),
        None => t
            .msg(Key::PolicyValueReviewServer)
            .with("days", server_days)
            .text(),
    }
}

/// The rules of `policy`, one fact each, in the order the page lists them. `server_days` is the operator's review interval,
/// which a policy with no interval of its own has.
pub(crate) fn policy_facts(t: T, policy: &Policy, server_days: u32) -> Vec<Fact> {
    let r = policy.rungs();
    let who = |paths, label: Key, role: Role| Fact {
        paths,
        label: t.get(label),
        value: rung_value(t, role),
    };
    let mut out = vec![
        who(&["rungs.comment"], Key::PolicyLabelComment, r.comment()),
        who(&["rungs.propose"], Key::PolicyLabelPropose, r.propose()),
        who(&["rungs.review"], Key::PolicyLabelReview, r.review()),
        who(
            &["rungs.create_view"],
            Key::PolicyLabelCreateView,
            r.create_view(),
        ),
        who(&["rungs.accept"], Key::PolicyLabelAccept, r.accept()),
        who(&["rungs.moderate"], Key::PolicyLabelModerate, r.moderate()),
        who(
            &["rungs.decide_appeal"],
            Key::PolicyLabelDecideAppeal,
            r.decide_appeal().role(),
        ),
        Fact {
            paths: &["rungs.grant_up_to"],
            label: t.get(Key::PolicyLabelGrant),
            value: grant_value(t, r.grant_up_to()),
        },
        Fact {
            paths: &["entry"],
            label: t.get(Key::PolicyLabelEntry),
            value: entry_value(t, policy.entry()),
        },
        Fact {
            paths: &["acceptance.approvals"],
            label: t.get(Key::PolicyLabelApprovals),
            value: approvals_value(t, policy.acceptance().approvals()),
        },
        Fact {
            paths: &["acceptance.self"],
            label: t.get(Key::PolicyLabelSelf),
            value: self_value(t, policy.acceptance().self_acceptance()),
        },
        Fact {
            paths: &["knowledge.review_after"],
            label: t.get(Key::PolicyLabelReviewAfter),
            value: review_value(t, policy.review_after(), server_days),
        },
    ];
    for role in Role::ALL {
        out.push(Fact {
            paths: promotion_paths(role),
            label: promotion_label(t, role),
            value: policy
                .promotion(role)
                .map_or_else(|| t.get(Key::PolicyChangePromotionNone), str::to_owned),
        });
    }
    out
}

/// The path of a role's promotion text in the files.
pub(crate) const fn promotion_paths(role: Role) -> &'static [&'static str] {
    match role {
        Role::Observer => &["promotion.observer"],
        Role::Questioner => &["promotion.questioner"],
        Role::Contributor => &["promotion.contributor"],
        Role::Reviewer => &["promotion.reviewer"],
        Role::Maintainer => &["promotion.maintainer"],
        Role::Admin => &["promotion.admin"],
    }
}

/// The rule of who may accept a change to the rules, as a value: the role and the approvals. **The approvals are the greater of
/// the constitution's and the policy's** (`floor`: `acceptance.approvals`), because the acceptance asks both of a change to the
/// rules (the hatch excepted, which asks none): what a person is told is what the acceptance will ask.
pub(crate) fn change_rules_value(t: T, constitution: &Constitution, floor: u8) -> String {
    let need = constitution.policy_change();
    t.msg(Key::PolicyValueChangeRules)
        .with("role", role_label(t, need.rung))
        .n(u64::from(need.approvals.max(floor)))
        .text()
}

/// The rule of who may accept a change to who may change the rules, as a value (the approvals as [`change_rules_value`] says).
pub(crate) fn change_who_value(t: T, constitution: &Constitution, floor: u8) -> String {
    let need = constitution.constitution_change();
    t.msg(Key::PolicyValueChangeWho)
        .with("role", role_label(t, need.rung))
        .n(u64::from(need.approvals.max(floor)))
        .text()
}

/// The rules of `constitution`, one fact each; `floor` is the policy's `acceptance.approvals`, which a change to the rules needs
/// too ([`change_rules_value`]); a comparison of two constitutions asks for none (0).
pub(crate) fn constitution_facts(t: T, constitution: &Constitution, floor: u8) -> Vec<Fact> {
    vec![
        Fact {
            paths: &["policy_change.rung", "policy_change.approvals"],
            label: t.get(Key::PolicyLabelChangeRules),
            value: change_rules_value(t, constitution, floor),
        },
        Fact {
            paths: &["constitution_change.approvals"],
            label: t.get(Key::PolicyLabelChangeWho),
            value: change_who_value(t, constitution, floor),
        },
    ]
}

/// What a protection asks, as the end of its sentence.
fn asks(t: T, require: Require) -> String {
    match require {
        Require::Review => t.get(Key::PolicyProtectionReview),
        Require::Rung(role) => t
            .msg(Key::PolicyProtectionRung)
            .with("role", role_label(t, role))
            .text(),
    }
}

/// One standing protection in a sentence: what it covers (*the pages under `Help/`, not the page `Help`*), what it asks and
/// why.
pub(crate) fn protection_sentence(t: T, protection: &Protection) -> String {
    let key = match protection.target() {
        Target::Path(_) => Key::PolicyProtectionPage,
        Target::Namespace(_) => Key::PolicyProtectionNamespace,
    };
    t.msg(key)
        .with("title", protection.target().title())
        .with("asks", asks(t, protection.require()))
        .with("reason", protection.reason())
        .text()
}
