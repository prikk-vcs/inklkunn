//! The claim paragraph's marker, held by a form and put back (RFC 005 part 1 D-1 b).

use std::collections::BTreeSet;

use crate::knowledge::marker::{held_part, hide_markers, reattach_all, reattach_part};

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

const PAGE: &str = "---\ntitle: T\nid: p1\n---\n\nOne said. ^c1\n\nPlain. ^other\n\n# Head\n\nTwo said.  ^c2\n\nLast.\n";

#[test]
fn the_markers_of_claim_paragraphs_are_hidden_and_nothing_else_is() {
    let shown = hide_markers(PAGE, &ids(&["c1", "c2"]), true);
    assert_eq!(
        shown,
        "---\ntitle: T\nid: p1\n---\n\nOne said.\n\nPlain. ^other\n\n# Head\n\nTwo said.\n\nLast.\n"
    );
    // A marker that is not a claim's stays where the person can see it.
    assert!(shown.contains("^other"));
    // With no claim, the text is the page.
    assert_eq!(hide_markers(PAGE, &ids(&[]), true), PAGE);
}

#[test]
fn a_marker_in_a_code_block_is_not_hidden() {
    let text = "```\nnot a marker ^c1\n```\n\nReal. ^c1\n";
    // Only the paragraph that holds the marker is cut; the code block is one block that holds none.
    assert_eq!(
        hide_markers(text, &ids(&["c1"]), false),
        "```\nnot a marker ^c1\n```\n\nReal.\n"
    );
}

#[test]
fn a_text_nobody_touched_comes_back_byte_for_byte() {
    for claims in [ids(&["c1"]), ids(&["c1", "c2"]), ids(&["c2"])] {
        let shown = hide_markers(PAGE, &claims, true);
        let back = reattach_all(PAGE, &shown, &claims, true);
        assert_eq!(back.text, PAGE);
        assert!(back.lost.is_empty());
    }
}

#[test]
fn a_changed_paragraph_is_lost_and_a_removed_one_is_lost_and_the_others_keep_their_markers() {
    let claims = ids(&["c1", "c2"]);
    let shown = hide_markers(PAGE, &claims, true);
    let changed = shown.replace("One said.", "One said, differently.");
    let got = reattach_all(PAGE, &changed, &claims, true);
    assert_eq!(got.lost, ["c1"]);
    assert!(got.text.contains("Two said.  ^c2"), "{}", got.text);
    assert!(!got.text.contains("^c1"));

    let removed = shown.replace("Two said.\n\n", "");
    let got = reattach_all(PAGE, &removed, &claims, true);
    assert_eq!(got.lost, ["c2"]);
    assert!(got.text.contains("One said. ^c1"));
}

#[test]
fn a_moved_paragraph_keeps_its_marker_by_being_the_same_words() {
    let claims = ids(&["c1"]);
    let shown = hide_markers(PAGE, &claims, true);
    let moved = shown
        .replace("One said.\n\n", "")
        .replace("Last.", "Last.\n\nOne said.");
    let got = reattach_all(PAGE, &moved, &claims, true);
    assert!(got.lost.is_empty());
    assert!(got.text.ends_with("One said. ^c1\n"), "{}", got.text);
}

#[test]
fn two_paragraphs_with_the_same_words_keep_their_own_markers_in_order() {
    let text = "Same. ^a\n\nSame. ^b\n";
    let claims = ids(&["a", "b"]);
    let shown = hide_markers(text, &claims, false);
    assert_eq!(shown, "Same.\n\nSame.\n");
    let back = reattach_all(text, &shown, &claims, false);
    assert_eq!(back.text, text);
    // One of them removed: the first keeps the first marker, the second is lost.
    let one = reattach_all(text, "Same.\n", &claims, false);
    assert_eq!(one.text, "Same. ^a\n");
    assert_eq!(one.lost, ["b"]);
}

#[test]
fn a_marker_the_text_already_carries_is_left_alone() {
    // The form was opened before the claim was made: the marker was in the text box.
    let claims = ids(&["c1"]);
    let got = reattach_all(PAGE, PAGE, &claims, true);
    assert_eq!(got.text, PAGE);
    assert!(got.lost.is_empty());
}

#[test]
fn a_form_for_one_paragraph_holds_the_marker_of_a_claim_paragraph_only() {
    let claims = ids(&["c1"]);
    assert_eq!(held_part("Said. ^c1", &claims).as_deref(), Some("c1"));
    assert_eq!(held_part("Said. ^other", &claims), None);
    assert_eq!(held_part("Said.", &claims), None);
    // More than one block is not one paragraph.
    assert_eq!(held_part("Said. ^c1\n\nMore.", &claims), None);
    // A marker that is all the paragraph says is nothing to hide.
    assert_eq!(held_part("^c1", &claims), None);
}

#[test]
fn the_marker_goes_to_the_first_paragraph_of_what_the_person_wrote() {
    assert_eq!(
        reattach_part("Reworded entirely.", "c1").as_deref(),
        Some("Reworded entirely. ^c1")
    );
    // Two paragraphs: the first is the one the form was opened on.
    assert_eq!(
        reattach_part("First.\n\nSecond.", "c1").as_deref(),
        Some("First. ^c1\n\nSecond.")
    );
    // A paragraph of several lines takes it on its last line.
    assert_eq!(
        reattach_part("Line one\nline two", "c1").as_deref(),
        Some("Line one\nline two ^c1")
    );
}

#[test]
fn a_marker_cannot_be_put_back_on_nothing_on_a_heading_or_over_another_marker() {
    assert_eq!(reattach_part("", "c1"), None);
    assert_eq!(reattach_part("  \n", "c1"), None);
    assert_eq!(reattach_part("# A heading", "c1"), None);
    assert_eq!(reattach_part("Written ^mine", "c1"), None);
    // The person wrote the same marker themselves: it is theirs and the claim's.
    assert_eq!(
        reattach_part("Written ^c1", "c1").as_deref(),
        Some("Written ^c1")
    );
}
