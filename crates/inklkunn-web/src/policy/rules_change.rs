//! A change to the rules, said as **before and after sentences** (RFC 006 D-4): *who may accept a proposal: maintainer or above →
//! reviewer or above*, one line for each rule that differs, never as YAML. The proposal page says the change a governance
//! proposal makes, and the policy page says each change in the history of the rules, **with this one function**.
//!
//! The text of a file is read by the core's own reader ([`read_policy`], [`read_constitution`]); a text that cannot be read as
//! rules has no sentences, and the caller says why with the fault.

use inklkunn_core::governance::{
    Constitution, Fault, GovernanceFile, Policy, Protection, read_constitution, read_policy,
};

use super::rules_phrases::{Fact, constitution_facts, policy_facts, protection_sentence};
use crate::catalog::{Key, T};

/// What a file's text is, for a comparison: the rules it says, or that it cannot be read as rules.
pub(crate) enum Side<T> {
    /// There is no file: the default rules.
    Absent,
    /// A file that reads.
    Rules(T),
    /// A file that cannot be read as rules.
    Unreadable(Fault),
}

/// The policy a file's text says, as a [`Side`].
pub(crate) fn policy_side(text: Option<&str>) -> Side<Policy> {
    match text {
        None => Side::Absent,
        Some(text) => match read_policy(text.as_bytes()) {
            Ok(policy) => Side::Rules(policy),
            Err(fault) => Side::Unreadable(fault.fault().clone()),
        },
    }
}

/// The constitution a file's text says, as a [`Side`].
pub(crate) fn constitution_side(text: Option<&str>) -> Side<Constitution> {
    match text {
        None => Side::Absent,
        Some(text) => match read_constitution(text.as_bytes()) {
            Ok(constitution) => Side::Rules(constitution),
            Err(fault) => Side::Unreadable(fault.fault().clone()),
        },
    }
}

/// What comparing two sides came to.
pub(crate) enum Compared {
    /// The lines that differ, one sentence each (empty: no rule says anything different).
    Lines(Vec<String>),
    /// One side cannot be read as rules.
    Unreadable(Fault),
}

fn differing(t: T, before: &[Fact], after: &[Fact]) -> Vec<String> {
    before
        .iter()
        .zip(after)
        .filter(|(b, a)| b.paths == a.paths && b.value != a.value)
        .map(|(b, a)| {
            t.msg(Key::PolicyChangeLine)
                .with("label", &a.label)
                .with("before", &b.value)
                .with("after", &a.value)
                .text()
        })
        .collect()
}

fn same_target(a: &Protection, b: &Protection) -> bool {
    a.target() == b.target()
}

/// The protections that were removed, added or changed, one sentence each.
fn protection_lines(t: T, before: &Policy, after: &Policy) -> Vec<String> {
    let mut out = Vec::new();
    for old in before.protections() {
        match after.protections().iter().find(|p| same_target(old, p)) {
            None => out.push(
                t.msg(Key::PolicyChangeProtectionRemoved)
                    .with("protection", protection_sentence(t, old))
                    .text(),
            ),
            Some(new) if new != old => out.push(
                t.msg(Key::PolicyChangeProtectionChanged)
                    .with("before", protection_sentence(t, old))
                    .with("after", protection_sentence(t, new))
                    .text(),
            ),
            Some(_) => {}
        }
    }
    for new in after.protections() {
        if !before.protections().iter().any(|p| same_target(p, new)) {
            out.push(
                t.msg(Key::PolicyChangeProtectionAdded)
                    .with("protection", protection_sentence(t, new))
                    .text(),
            );
        }
    }
    out
}

/// What a change to `governance/policy.yaml` does, in sentences: from the rules `before` the change to the rules `after`.
pub(crate) fn policy_lines(
    t: T,
    before: &Side<Policy>,
    after: &Side<Policy>,
    server_days: u32,
) -> Compared {
    let default = Policy::default();
    let side = |side: &Side<Policy>| -> Result<Policy, Fault> {
        match side {
            Side::Absent => Ok(default.clone()),
            Side::Rules(policy) => Ok(policy.clone()),
            Side::Unreadable(fault) => Err(fault.clone()),
        }
    };
    let (before, after) = match (side(before), side(after)) {
        (Ok(b), Ok(a)) => (b, a),
        (Err(fault), _) | (_, Err(fault)) => return Compared::Unreadable(fault),
    };
    let mut lines = differing(
        t,
        &policy_facts(t, &before, server_days),
        &policy_facts(t, &after, server_days),
    );
    lines.extend(protection_lines(t, &before, &after));
    Compared::Lines(lines)
}

/// What a change to `governance/constitution.yaml` does, in sentences.
pub(crate) fn constitution_lines(
    t: T,
    before: &Side<Constitution>,
    after: &Side<Constitution>,
) -> Compared {
    let default = Constitution::default();
    let side = |side: &Side<Constitution>| -> Result<Constitution, Fault> {
        match side {
            Side::Absent => Ok(default),
            Side::Rules(constitution) => Ok(*constitution),
            Side::Unreadable(fault) => Err(fault.clone()),
        }
    };
    let (before, after) = match (side(before), side(after)) {
        (Ok(b), Ok(a)) => (b, a),
        (Err(fault), _) | (_, Err(fault)) => return Compared::Unreadable(fault),
    };
    Compared::Lines(differing(
        t,
        &constitution_facts(t, &before, 0),
        &constitution_facts(t, &after, 0),
    ))
}

/// What a change to one governance file does, from its text before and after (`None` is no file).
pub(crate) fn file_lines(
    t: T,
    file: GovernanceFile,
    before: Option<&str>,
    after: Option<&str>,
    server_days: u32,
) -> Compared {
    match file {
        GovernanceFile::Policy => {
            policy_lines(t, &policy_side(before), &policy_side(after), server_days)
        }
        GovernanceFile::Constitution => {
            constitution_lines(t, &constitution_side(before), &constitution_side(after))
        }
    }
}
