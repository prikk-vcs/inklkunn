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
        Rule::Ordering(OrderingPair::DecideAppealBelowModerate) => {
            Key::GovernanceRuleOrderingDecideAppealBelowModerate
        }
        Rule::PlatformLimit(Limit::EntryMax) => Key::GovernanceRulePlatformLimitEntryMax,
        Rule::PlatformLimit(Limit::LimitDays) => Key::GovernanceRulePlatformLimitLimitDays,
        Rule::OutsideDefaultView => Key::GovernanceRuleOutsideDefaultView,
        Rule::UnknownFile => Key::GovernanceRuleUnknownFile,
        Rule::NotAFile => Key::GovernanceRuleNotAFile,
        Rule::PrefixIsNamespace => Key::GovernanceRulePrefixIsNamespace,
        Rule::TemplateScheme => Key::GovernanceRuleTemplateNotHttps,
        Rule::TemplateHost => Key::GovernanceRuleTemplateHost,
        Rule::TemplateUserInfo => Key::GovernanceRuleTemplateUserinfo,
        Rule::TemplatePlaceholder => Key::GovernanceRuleTemplatePlaceholder,
        Rule::TemplateCharacter => Key::GovernanceRuleTemplateCharacter,
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
    let mut out = fault_paragraphs(t, faults);
    out.push(t.get(Key::OutcomeNoteGovernanceInvalid));
    out
}

/// Each fault of a change to the rules once, and nothing else: what a proposal that is saved, sent or revised is told when it
/// touches `governance/` in a view that is not the default one (the acceptance's note, *held for review, accept again*, is not
/// true of a draft).
pub(crate) fn fault_paragraphs(t: T, faults: &[Fault]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for fault in faults {
        let said = fault_text(t, fault);
        if !out.contains(&said) {
            out.push(said);
        }
    }
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
    let mut out = vec![read_fault_text(t, fault)];
    // Where something that is not a file stands in the file's place, say what it is and what repairs it (handoff G-T1 correction
    // E-1): the paths the loader read, up to 50, and how many more. The words of the repair are the catalog's; the names are
    // the repository's, shown as text with the characters a name may not carry replaced.
    if fault.refusal() == Rule::NotAFile && !fault.found().is_empty() {
        out.push(found_text(t, fault));
    }
    out.push(t.get(Key::OutcomeNoteRulesUnreadable));
    out
}

/// The sentence that names what stands where a governance file belongs: its paths, then *and N more* when the list is cut.
fn found_text(t: T, fault: &ReadFault) -> String {
    let mut paths = fault
        .found()
        .iter()
        .map(|path| shown_path(path))
        .collect::<Vec<_>>()
        .join(", ");
    if fault.more() > 0 {
        paths.push_str(", ");
        paths.push_str(
            &t.msg(Key::GovernanceFoundMore)
                .with("more", fault.more())
                .text(),
        );
    }
    t.msg(Key::OutcomeNoteRulesUnreadableFound)
        .with("list", paths)
        .text()
}

/// A repository path as the page shows it: at most 120 characters (then `…`), and a control, direction or line-separator
/// character shown as `?`, so that a name cannot make the line read as another.
fn shown_path(path: &str) -> String {
    const LONGEST: usize = 120;
    let mut out: String = path
        .chars()
        .take(LONGEST)
        .map(|c| {
            if inklkunn_core::governance::holds_only_allowed_characters(&c.to_string()) {
                c
            } else {
                '?'
            }
        })
        .collect();
    if path.chars().count() > LONGEST {
        out.push('…');
    }
    out
}
