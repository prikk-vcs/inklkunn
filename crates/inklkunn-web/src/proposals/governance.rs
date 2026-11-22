//! What the pages say when the rules of a wiki stop a change (RFC 006 D-1, D-4; handoff G-T1 part C): the sentence of each
//! rule a governance file breaks, the line that names the file and where in it, the roles a change to the rules needs, the
//! approvals it lacks, and the warning that more approvals are asked for than people can give.
//!
//! **Nothing here repeats what a file said.** A fault is a file's own fixed name (`governance/policy.yaml`), a line number
//! and a rule's sentence from the catalog; the words are *the rules of this wiki*, never *official*.

use inklkunn_core::governance::{Fault, Limit, OrderingPair, ReadFault, Refusal as Rule};
use inklkunn_store::Role;

use crate::catalog::{Key, T};

/// The catalog's sentence for a rule a governance file breaks. **Exhaustive on purpose**: a rule with no sentence does not
/// compile.
pub(crate) fn rule_key(rule: Rule) -> Key {
    match rule {
        Rule::NotText => Key::GovernanceRuleNotText,
        Rule::TooLong => Key::GovernanceRuleTooLong,
        Rule::WrongShape => Key::GovernanceRuleWrongShape,
        Rule::UnknownKey => Key::GovernanceRuleUnknownKey,
        Rule::RepeatedKey => Key::GovernanceRuleRepeatedKey,
        Rule::UnknownValue => Key::GovernanceRuleUnknownValue,
        Rule::OutOfRange => Key::GovernanceRuleOutOfRange,
        Rule::Bounds => Key::GovernanceRuleBounds,
        Rule::ForbiddenCharacter => Key::GovernanceRuleForbiddenCharacter,
        Rule::Missing => Key::GovernanceRuleMissingKey,
        Rule::Duplicate => Key::GovernanceRuleDuplicate,
        Rule::Ordering(OrderingPair::ProposeAboveReview) => {
            Key::GovernanceRuleOrderingProposeAboveReview
        }
        Rule::Ordering(OrderingPair::ReviewAboveAccept) => {
            Key::GovernanceRuleOrderingReviewAboveAccept
        }
        Rule::Ordering(OrderingPair::ModerateBelowReviewer) => {
            Key::GovernanceRuleOrderingModerateBelowReviewer
        }
        Rule::PlatformLimit(Limit::EntryMax) => Key::GovernanceRulePlatformLimitEntryMax,
        Rule::PlatformLimit(Limit::LimitDays) => Key::GovernanceRulePlatformLimitLimitDays,
        Rule::OutsideDefaultView => Key::GovernanceRuleOutsideDefaultView,
        Rule::UnknownFile => Key::GovernanceRuleUnknownFile,
    }
}

/// One line: the file, the line in it when there is one, and the rule's sentence.
fn line(t: T, path: &str, line: Option<usize>, rule: Rule) -> String {
    let said = t.get(rule_key(rule));
    match line {
        Some(n) => t
            .msg(Key::GovernanceFaultLine)
            .with("file", path)
            .with("line", n)
            .with("rule", said)
            .text(),
        None => t
            .msg(Key::GovernanceFaultFile)
            .with("file", path)
            .with("rule", said)
            .text(),
    }
}

/// What a refused change to a governance file says: the file and the rule.
pub(crate) fn fault_text(t: T, fault: &Fault) -> String {
    line(t, fault.path(), fault.line, fault.refusal)
}

/// What a wiki that cannot read its governance says: the file, the line and the rule.
pub(crate) fn read_fault_text(t: T, fault: &ReadFault) -> String {
    line(t, fault.file().path(), fault.line(), fault.refusal())
}

/// The paragraphs of the page that says a change to the rules was refused before signing: each fault once, then that nothing
/// was changed.
pub(crate) fn invalid_paragraphs(t: T, faults: &[Fault]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for fault in faults {
        let said = fault_text(t, fault);
        if !out.contains(&said) {
            out.push(said);
        }
    }
    out.push(t.get(Key::OutcomeNoteGovernanceInvalid));
    out
}

/// The note under *this change to the rules needs a higher role*: a maintainer or an admin, or an admin.
pub(crate) fn rung_note(t: T, rung: Role) -> String {
    t.get(if rung >= Role::Admin {
        Key::OutcomeNoteGovernanceRungAdmin
    } else {
        Key::OutcomeNoteGovernanceRungMaintainer
    })
}

/// The note under *it needs more approvals*.
pub(crate) fn approvals_note(t: T, needed: u8, have: usize) -> String {
    t.msg(Key::OutcomeNoteApprovalsMissing)
        .n(u64::from(needed))
        .with("have", have)
        .text()
}

/// The paragraphs of the page that says every change is refused while a governance file cannot be read: the fault, then
/// what can be done.
pub(crate) fn unreadable_paragraphs(t: T, fault: &ReadFault) -> Vec<String> {
    vec![
        read_fault_text(t, fault),
        t.get(Key::OutcomeNoteRulesUnreadable),
    ]
}
