//! The line a refusal points at: the key it is about, found in the file's block structure, or the nearest enclosing key
//! when the file writes the mapping on one line, and no line rather than a wrong one.

use super::*;

fn line(text: &str) -> Option<usize> {
    policy_fault(text).line
}

#[test]
fn a_key_is_found_under_its_parent_among_comments_and_blank_lines() {
    let text = "# the rules of this wiki\n\nentry: none\n\n# who does what\nrungs:\n  # comment first\n  comment: observer\n  accept: owner\n  moderate: admin\n";
    assert_eq!(line(text), Some(9));
    // The same key name under another parent is not the one.
    let text = "acceptance:\n  self: never\nrungs:\n  comment: nobody\n";
    assert_eq!(line(text), Some(4));
    let text = "knowledge:\n  review_after: 0\nacceptance:\n  approvals: 1\n";
    assert_eq!(line(text), Some(2));
}

#[test]
fn a_protections_entry_and_key_are_found_by_position() {
    let text = "protect:\n  - path: A\n    require: review\n    reason: one\n  - path: B\n    require: nobody\n    reason: two\n  - path: C\n    require: review\n    reason: three\n";
    let f = policy_fault(text);
    assert_eq!(
        (f.item, f.field, f.line),
        (Some(2), "protect.require", Some(6))
    );
    // A list written at the parent key's indent.
    let text = "protect:\n- path: A\n  require: review\n  reason: one\n- path: B\n  require: review\n  reason: [x]\n";
    let f = policy_fault(text);
    assert_eq!(
        (f.item, f.field, f.line),
        (Some(2), "protect.reason", Some(7))
    );
    // The entry's own line when its key is missing.
    let text = "protect:\n  - path: A\n    require: review\n    reason: one\n  - path: B\n    require: review\n";
    assert_eq!(line(text), Some(5));
    // Nested under `require`.
    let text = "protect:\n  - path: A\n    reason: one\n    require:\n      rung: owner\n";
    assert_eq!(line(text), Some(5));
}

#[test]
fn a_one_line_mapping_points_at_its_key() {
    assert_eq!(line("entry: none\nrungs: {accept: owner}\n"), Some(2));
    assert_eq!(line("acceptance: {approvals: 9}\n"), Some(1));
}

#[test]
fn a_quoted_key_and_a_crlf_file_are_found() {
    assert_eq!(line("\"rungs\":\r\n  \"accept\": owner\r\n"), Some(2));
}

#[test]
fn the_ordering_rule_points_at_the_first_key_of_the_pair() {
    let text = "entry: none\nrungs:\n  propose: admin\n  review: reviewer\n";
    let f = policy_fault(text);
    assert_eq!((f.field, f.line), ("rungs.propose", Some(3)));
    // A pair broken by a key the file does not write points at the key it does write, or at `rungs:` when none.
    let text = "rungs:\n  review: maintainer\n  accept: reviewer\n";
    assert_eq!(line(text), Some(2));
    let f = policy_fault("rungs:\n  propose: maintainer\n");
    assert_eq!(f.line, Some(2));
}

#[test]
fn no_line_is_better_than_a_wrong_one() {
    // The key is not in the file at all: nothing to point at but the file.
    let f = policy_fault("protect:\n  - path: A\n    require: review\n");
    assert_eq!((f.field, f.item), ("protect.reason", Some(1)));
    assert_eq!(f.line, Some(2), "the entry");
    let f = validate_policy(
        "entry: contributor\n",
        &PlatformLimits {
            entry_max: Entry::None,
            limit_days: 1,
        },
    )
    .unwrap_err();
    assert_eq!(f.line, Some(1));
}
