//! The claim paragraph's marker, held by a form and put back (RFC 005 part 1 D-1 b).

use std::collections::BTreeSet;

use crate::knowledge::marker::{
    held_part, hide_markers, reattach_all, reattach_part, words_with_id,
};

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// The block a one-paragraph form is opened on in most of these tests: the marker after a space.
const SAID: &str = "Said. ^c1";

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
    // One of them removed: the text no longer says which, so neither marker is put on the one that is left (K10):
    // both are lost, which asks the person to confirm the removal of each.
    let one = reattach_all(text, "Same.\n", &claims, false);
    assert_eq!(one.text, "Same.\n");
    assert_eq!(one.lost, ["a", "b"]);
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
        reattach_part(SAID, "Reworded entirely.", "c1").as_deref(),
        Some("Reworded entirely. ^c1")
    );
    // Two paragraphs: the first is the one the form was opened on.
    assert_eq!(
        reattach_part(SAID, "First.\n\nSecond.", "c1").as_deref(),
        Some("First. ^c1\n\nSecond.")
    );
    // A paragraph of several lines takes it on its last line.
    assert_eq!(
        reattach_part(SAID, "Line one\nline two", "c1").as_deref(),
        Some("Line one\nline two ^c1")
    );
}

#[test]
fn a_marker_cannot_be_put_back_on_nothing_on_a_heading_or_over_another_marker() {
    assert_eq!(reattach_part(SAID, "", "c1"), None);
    assert_eq!(reattach_part(SAID, "  \n", "c1"), None);
    assert_eq!(reattach_part(SAID, "# A heading", "c1"), None);
    assert_eq!(reattach_part(SAID, "Written ^mine", "c1"), None);
    // The person wrote the same marker themselves: it is theirs and the claim's.
    assert_eq!(
        reattach_part(SAID, "Written ^c1", "c1").as_deref(),
        Some("Written ^c1")
    );
}

#[test]
fn the_words_of_a_paragraph_are_found_by_its_marker_without_it() {
    assert_eq!(
        words_with_id(PAGE, "c2", true).as_deref(),
        Some("Two said.")
    );
    assert_eq!(words_with_id(PAGE, "none", true), None);
    assert_eq!(
        words_with_id("A. ^x\n\nB. ^x\n", "x", false),
        None,
        "twice is not found"
    );
    // A heading line that looks like a marker is not a paragraph's.
    assert_eq!(words_with_id("# Head ^h\n", "h", false), None);
}

// ------------------------------------------------------------------------------------ K10: markers

/// The form's text for `started`, and what comes back of `posted` against it.
fn back(started: &str, posted: &str, held: &[&str]) -> (String, Vec<String>) {
    let claims = ids(held);
    let got = reattach_all(started, posted, &claims, false);
    (got.text, got.lost)
}

#[test]
fn a_marker_goes_back_to_its_own_occurrence_of_equal_words_and_never_to_the_first() {
    // The second `Same.` is the claim's.
    let started = "Same.\n\nSame. ^c1\n";
    let claims = ids(&["c1"]);
    let shown = hide_markers(started, &claims, false);
    assert_eq!(shown, "Same.\n\nSame.\n");
    // Untouched: the page byte for byte, the marker where it was.
    assert_eq!(back(started, &shown, &["c1"]), (started.to_owned(), vec![]));
    // Another paragraph edited, and one added between: the claim's paragraph is still the second `Same.`.
    let edited = "Before.\n\nSame.\n\nMiddle.\n\nSame.\n";
    assert_eq!(
        back(started, edited, &["c1"]),
        (
            "Before.\n\nSame.\n\nMiddle.\n\nSame. ^c1\n".to_owned(),
            vec![]
        )
    );
    // The claim first: the first.
    let first = "Same. ^c1\n\nSame.\n";
    assert_eq!(
        back(first, &hide_markers(first, &claims, false), &["c1"]),
        (first.to_owned(), vec![])
    );
    // Three, the claim's in the middle.
    let three = "Same.\n\nSame. ^c1\n\nSame.\n";
    assert_eq!(
        back(three, &hide_markers(three, &claims, false), &["c1"]),
        (three.to_owned(), vec![])
    );
}

#[test]
fn deleting_the_claims_paragraph_never_moves_its_marker_to_a_duplicate() {
    let claims = ids(&["c1"]);
    for started in ["Same.\n\nSame. ^c1\n", "Same. ^c1\n\nSame.\n"] {
        // Either paragraph deleted: the text cannot say which one the claim was, so the marker is lost (the form
        // asks the person to confirm a removal) and no paragraph is given a marker it may not have had.
        let (text, lost) = back(started, "Same.\n", &["c1"]);
        assert_eq!(text, "Same.\n", "{started:?}");
        assert_eq!(lost, ["c1"], "{started:?}");
        // A duplicate added: the same.
        let shown = hide_markers(started, &claims, false);
        let more = format!("{shown}\nSame.\n");
        let (text, lost) = back(started, &more, &["c1"]);
        assert!(!text.contains("^c1"), "{text}");
        assert_eq!(lost, ["c1"]);
    }
    // Words that were shown once and are sent once are matched as they always were.
    let (text, lost) = back("Alone. ^c1\n\nOther.\n", "Alone.\n\nOther.\n", &["c1"]);
    assert_eq!(text, "Alone. ^c1\n\nOther.\n");
    assert!(lost.is_empty());
}

#[test]
fn a_paragraph_that_shows_its_marker_is_not_an_occurrence_of_the_words_without_it() {
    // `Same. ^zz` is another claim's... no, nobody's held marker: the form showed it with its marker, so the words
    // `Same.` were shown once.
    let started = "Same. ^zz\n\nSame. ^c1\n";
    let claims = ids(&["c1"]);
    let shown = hide_markers(started, &claims, false);
    assert_eq!(shown, "Same. ^zz\n\nSame.\n");
    assert_eq!(back(started, &shown, &["c1"]), (started.to_owned(), vec![]));
}

#[test]
fn the_one_paragraph_form_puts_the_marker_back_after_the_whitespace_it_had() {
    // A marker on a line of its own: a text nobody touched is not a whitespace-only edit.
    for started in [
        "Said.\n^c1",
        "Said. ^c1",
        "Said.  ^c1",
        "Said.\t^c1",
        "Line one\nline two\n^c1",
        "Line one\nline two ^c1",
    ] {
        let claims = ids(&["c1"]);
        let held = held_part(started, &claims).expect("held");
        let shown = hide_markers(started, &claims, false);
        assert_eq!(
            reattach_part(started, &shown, &held).as_deref(),
            Some(started),
            "{started:?}"
        );
    }
    // Edited words keep it too.
    assert_eq!(
        reattach_part("Said.\n^c1", "Said differently.", "c1").as_deref(),
        Some("Said differently.\n^c1")
    );
    // A paragraph the form had no marker for (the claim was made after the form opened): a space.
    assert_eq!(
        reattach_part("Said.", "Said again.", "c1").as_deref(),
        Some("Said again. ^c1")
    );
}

// -- property-style: random pages with duplicated words

use super::summary::Dice;

/// A body of 2 to 7 paragraphs drawn from a pool of three wordings, so that duplicates are common, with a random
/// subset of the paragraphs a claim's (each its own marker).
fn random_body(dice: &mut Dice) -> (String, Vec<String>) {
    let n = 2 + dice.below(6);
    let mut held = Vec::new();
    let mut paragraphs = Vec::new();
    for i in 0..n {
        let words = dice.pick(&["Same.", "Alike.", "Other words."]);
        if dice.chance() {
            let id = format!("c{i}");
            let sep = dice.pick(&[" ", "  ", "\n"]);
            paragraphs.push(format!("{words}{sep}^{id}"));
            held.push(id);
        } else {
            paragraphs.push(words.to_owned());
        }
    }
    (paragraphs.join("\n\n") + "\n", held)
}

fn held_set(held: &[String]) -> BTreeSet<String> {
    held.iter().cloned().collect()
}

/// The paragraphs of a text, in order, markers of the `ids` left out: what the form shows.
fn shown_paragraphs(text: &str) -> Vec<String> {
    text.trim_end().split("\n\n").map(str::to_owned).collect()
}

#[test]
fn whatever_was_shown_and_sent_back_untouched_is_the_page_byte_for_byte() {
    let mut dice = Dice(0xdead_beef_cafe_f00d);
    for _ in 0..3000 {
        let (started, held) = random_body(&mut dice);
        let claims = held_set(&held);
        let shown = hide_markers(&started, &claims, false);
        let got = reattach_all(&started, &shown, &claims, false);
        assert_eq!(got.text, started, "{started:?}");
        assert!(got.lost.is_empty(), "{started:?}");
    }
}

#[test]
fn a_marker_never_lands_on_a_paragraph_that_is_not_its_own_occurrence() {
    let mut dice = Dice(0x0123_4567_89ab_cdef);
    let mut placed_somewhere = 0;
    for _ in 0..3000 {
        let (started, held) = random_body(&mut dice);
        let claims = held_set(&held);
        let shown = hide_markers(&started, &claims, false);
        let mut posted = shown_paragraphs(&shown);
        // One random edit of the text the form showed: a paragraph deleted, one added, or one reworded.
        let at = dice.below(posted.len());
        match dice.below(3) {
            0 => {
                posted.remove(at);
            }
            1 => posted.insert(at, dice.pick(&["Same.", "Alike.", "Fresh."]).to_owned()),
            _ => {
                if let Some(p) = posted.get_mut(at) {
                    *p = "Reworded.".to_owned();
                }
            }
        }
        let posted = posted.join("\n\n") + "\n";
        let got = reattach_all(&started, &posted, &claims, false);
        // The words are never touched: hiding the markers again gives what was sent.
        assert_eq!(
            hide_markers(&got.text, &claims, false),
            posted,
            "{started:?} -> {posted:?}"
        );
        // Every marker is either in the text once or lost, never both and never twice.
        for id in &held {
            let n = got.text.matches(&format!("^{id}")).count();
            assert_eq!(
                n + got.lost.iter().filter(|l| *l == id).count(),
                1,
                "{id} in {started:?} -> {posted:?}: {got:?}"
            );
            placed_somewhere += n;
        }
        // A marker that is placed is on a paragraph that has its words, as the form showed them.
        let before = shown_paragraphs(&hide_markers(&started, &claims, false));
        for (i, p) in shown_paragraphs(&got.text).iter().enumerate() {
            for id in &held {
                if p.ends_with(&format!("^{id}")) {
                    let words = p.trim_end_matches(&format!("^{id}")).trim_end().to_owned();
                    // Its place among equal words in what was sent is its place among them in the form.
                    let sent: Vec<String> = shown_paragraphs(&posted);
                    let k = sent.iter().take(i).filter(|s| **s == words).count();
                    let original = started
                        .split("\n\n")
                        .position(|s| s.contains(&format!("^{id}")))
                        .expect("held");
                    let k0 = before
                        .iter()
                        .take(original)
                        .filter(|s| **s == words)
                        .count();
                    assert_eq!(k, k0, "{id}: {started:?} -> {posted:?}");
                    assert_eq!(
                        sent.iter().filter(|s| **s == words).count(),
                        before.iter().filter(|s| **s == words).count(),
                        "{id}: the count of equal words changed and it still went back"
                    );
                }
            }
        }
    }
    assert!(
        placed_somewhere > 3000,
        "the draws must put markers back too"
    );
}
