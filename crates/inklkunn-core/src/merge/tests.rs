#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::time::{Duration, Instant};

use super::line::would_be_clean;
use super::page::merge_text;
use super::*;

// ------------------------------------------------------------------------------------------- helpers

const ID_A: &str = "019a0000-0000-7000-8000-00000000000a";
const ID_B: &str = "019a0000-0000-7000-8000-00000000000b";

/// A page with an id and the given extra frontmatter lines and body.
fn page(id: &str, front: &str, body: &str) -> String {
    format!("---\ntitle: T\nid: {id}\n{front}---\n{body}")
}

fn pt(path: &str, text: &str) -> PageText {
    PageText::new(path, text)
}

/// Merge one page present on every side at one path.
fn merge_one(base: &str, theirs: &str, ours: &str) -> MergeOutcome {
    merge(
        &[pt("pages/A.md", base)],
        &[pt("pages/A.md", theirs)],
        &[pt("pages/A.md", ours)],
    )
}

fn clean_text(outcome: MergeOutcome) -> String {
    match outcome {
        MergeOutcome::Clean { pages } => {
            assert_eq!(pages.len(), 1, "{pages:?}");
            pages.into_iter().next().unwrap().text
        }
        other => panic!("expected a clean merge, got {other:?}"),
    }
}

fn overlaps(outcome: MergeOutcome) -> Vec<Overlap> {
    match outcome {
        MergeOutcome::Overlaps { overlaps } => overlaps,
        other => panic!("expected overlaps, got {other:?}"),
    }
}

const BASE_BODY: &str = "First paragraph. ^p1\n\nSecond paragraph. ^p2\n\nThird paragraph. ^p3\n";

// ----------------------------------------------------------------------------------- the body, by block

#[test]
fn a_block_changed_on_one_side_only_is_taken() {
    let base = page(ID_A, "", BASE_BODY);
    let theirs = page(
        ID_A,
        "",
        &BASE_BODY.replace("Second paragraph.", "Second, revised."),
    );
    let ours = page(
        ID_A,
        "",
        &BASE_BODY.replace("Third paragraph.", "Third, revised."),
    );
    let merged = clean_text(merge_one(&base, &theirs, &ours));
    assert_eq!(
        merged,
        page(
            ID_A,
            "",
            "First paragraph. ^p1\n\nSecond, revised. ^p2\n\nThird, revised. ^p3\n"
        )
    );
}

#[test]
fn the_same_change_on_both_sides_is_taken_once() {
    let base = page(ID_A, "", BASE_BODY);
    let both = page(
        ID_A,
        "",
        &BASE_BODY.replace("Second paragraph.", "Second, revised."),
    );
    assert_eq!(clean_text(merge_one(&base, &both, &both)), both);
    // The same block removed on both sides, and the same block added on both.
    let removed = page(ID_A, "", "First paragraph. ^p1\n\nThird paragraph. ^p3\n");
    assert_eq!(clean_text(merge_one(&base, &removed, &removed)), removed);
    let added = page(ID_A, "", &format!("{BASE_BODY}\nNew block. ^p4\n"));
    assert_eq!(clean_text(merge_one(&base, &added, &added)), added);
}

#[test]
fn a_block_changed_differently_on_both_sides_is_an_overlap_with_both_texts() {
    let base = page(ID_A, "", BASE_BODY);
    let theirs = page(
        ID_A,
        "",
        &BASE_BODY.replace("Second paragraph.", "Second, by them."),
    );
    let ours = page(
        ID_A,
        "",
        &BASE_BODY.replace("Second paragraph.", "Second, by us."),
    );
    let found = overlaps(merge_one(&base, &theirs, &ours));
    assert_eq!(found.len(), 1);
    let o = found.first().unwrap();
    assert_eq!(o.page.id.as_deref(), Some(ID_A));
    assert_eq!(o.page.path, "pages/A.md");
    assert_eq!(
        o.place,
        Place::Block {
            id: Some("p2".into()),
            base: Some(2),
            theirs: Some(2),
            ours: Some(2)
        }
    );
    assert_eq!(o.base.as_deref(), Some("Second paragraph. ^p2"));
    assert_eq!(o.theirs.as_deref(), Some("Second, by them. ^p2"));
    assert_eq!(o.ours.as_deref(), Some("Second, by us. ^p2"));
}

#[test]
fn an_overlap_stops_the_whole_page_and_every_overlap_is_reported() {
    let base = page(ID_A, "", BASE_BODY);
    let theirs = page(
        ID_A,
        "",
        &BASE_BODY
            .replace("First paragraph.", "First, theirs.")
            .replace("Third paragraph.", "Third, theirs."),
    );
    let ours = page(
        ID_A,
        "",
        &BASE_BODY
            .replace("First paragraph.", "First, ours.")
            .replace("Third paragraph.", "Third, ours."),
    );
    let found = overlaps(merge_one(&base, &theirs, &ours));
    let ids: Vec<_> = found
        .iter()
        .map(|o| match &o.place {
            Place::Block { id, .. } => id.clone().unwrap(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(ids, ["p1", "p3"]);
}

#[test]
fn blocks_without_ids_are_matched_by_content_and_order() {
    let base = page(ID_A, "", "Alpha.\n\nBeta.\n\nGamma.\n");
    let theirs = page(ID_A, "", "Alpha.\n\nGamma.\n");
    let ours = page(ID_A, "", "Alpha.\n\nBeta.\n\nGamma.\n\nDelta.\n");
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &ours)),
        page(ID_A, "", "Alpha.\n\nGamma.\n\nDelta.\n")
    );
}

#[test]
fn a_block_edited_without_an_id_beside_an_edit_of_another_block_is_clean() {
    // Neither edited block has an id; each is the only one that changed in its stretch.
    let base = page(ID_A, "", "Alpha.\n\nBeta.\n\nGamma.\n\nDelta.\n");
    let theirs = page(ID_A, "", "Alpha, theirs.\n\nBeta.\n\nGamma.\n\nDelta.\n");
    let ours = page(ID_A, "", "Alpha.\n\nBeta.\n\nGamma.\n\nDelta, ours.\n");
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &ours)),
        page(
            ID_A,
            "",
            "Alpha, theirs.\n\nBeta.\n\nGamma.\n\nDelta, ours.\n"
        )
    );
}

#[test]
fn neighbouring_blocks_without_ids_changed_one_on_each_side_are_clean() {
    // Each side changed one block and no id says which block it was; a stretch with one block on each side is
    // that block changed. Two blocks changed in one stretch on one side are not guessed at (below).
    let base = page(ID_A, "", "Alpha.\n\nBeta.\n\nGamma.\n");
    let theirs = page(ID_A, "", "Alpha, theirs.\n\nBeta.\n\nGamma.\n");
    let ours = page(ID_A, "", "Alpha.\n\nBeta, ours.\n\nGamma.\n");
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &ours)),
        page(ID_A, "", "Alpha, theirs.\n\nBeta, ours.\n\nGamma.\n")
    );
    // The same block changed on both sides is still an overlap.
    let ours = page(ID_A, "", "Alpha, ours.\n\nBeta.\n\nGamma.\n");
    assert_eq!(overlaps(merge_one(&base, &theirs, &ours)).len(), 1);
    // Two neighbours rewritten on one side and one of them on the other: the stretch is the overlap.
    let theirs = page(ID_A, "", "Alpha, theirs.\n\nBeta, theirs.\n\nGamma.\n");
    let ours = page(ID_A, "", "Alpha.\n\nBeta, ours.\n\nGamma.\n");
    let found = overlaps(merge_one(&base, &theirs, &ours));
    assert_eq!(found.len(), 1);
    assert_eq!(
        found.first().and_then(|o| o.theirs.as_deref()),
        Some("Alpha, theirs.\n\nBeta, theirs.")
    );
}

#[test]
fn a_block_removed_on_one_side_and_changed_on_the_other_is_an_overlap() {
    let base = page(ID_A, "", BASE_BODY);
    let removed = page(ID_A, "", "First paragraph. ^p1\n\nThird paragraph. ^p3\n");
    let changed = page(
        ID_A,
        "",
        &BASE_BODY.replace("Second paragraph.", "Second, changed."),
    );
    let found = overlaps(merge_one(&base, &removed, &changed));
    assert_eq!(found.len(), 1);
    let o = found.first().unwrap();
    assert_eq!(o.theirs, None);
    assert_eq!(o.ours.as_deref(), Some("Second, changed. ^p2"));
    // The sides swapped: the same overlap, the texts on the other sides.
    let found = overlaps(merge_one(&base, &changed, &removed));
    assert_eq!(
        found
            .first()
            .map(|o| (o.theirs.is_some(), o.ours.is_none())),
        Some((true, true))
    );
}

#[test]
fn a_block_removed_on_one_side_and_untouched_on_the_other_is_removed() {
    let base = page(ID_A, "", BASE_BODY);
    let removed = page(ID_A, "", "First paragraph. ^p1\n\nThird paragraph. ^p3\n");
    assert_eq!(clean_text(merge_one(&base, &removed, &base)), removed);
    assert_eq!(clean_text(merge_one(&base, &base, &removed)), removed);
}

#[test]
fn blocks_added_in_different_places_are_both_taken_and_in_one_place_overlap() {
    let base = page(ID_A, "", "Alpha. ^a\n\nOmega. ^o\n");
    let at_start = page(ID_A, "", "Prologue. ^s\n\nAlpha. ^a\n\nOmega. ^o\n");
    let at_end = page(ID_A, "", "Alpha. ^a\n\nOmega. ^o\n\nEpilogue. ^e\n");
    assert_eq!(
        clean_text(merge_one(&base, &at_start, &at_end)),
        page(
            ID_A,
            "",
            "Prologue. ^s\n\nAlpha. ^a\n\nOmega. ^o\n\nEpilogue. ^e\n"
        )
    );
    // Two different blocks between the same two: which comes first is for a person.
    let one = page(ID_A, "", "Alpha. ^a\n\nOne. ^1\n\nOmega. ^o\n");
    let two = page(ID_A, "", "Alpha. ^a\n\nTwo. ^2\n\nOmega. ^o\n");
    let found = overlaps(merge_one(&base, &one, &two));
    assert_eq!(found.len(), 1);
    let o = found.first().unwrap();
    assert_eq!(
        (o.base.as_deref(), o.theirs.as_deref(), o.ours.as_deref()),
        (None, Some("One. ^1"), Some("Two. ^2"))
    );
}

#[test]
fn a_container_is_one_unit() {
    let body = "Intro. ^i\n\n::: note Heads up\nInside one.\n\nInside two.\n:::\n\nOutro. ^o\n";
    let base = page(ID_A, "", body);
    let theirs = page(ID_A, "", &body.replace("Inside one.", "Inside, theirs."));
    let ours = page(ID_A, "", &body.replace("Inside two.", "Inside, ours."));
    // Both changed the one container: an overlap, with the whole container as the text.
    let found = overlaps(merge_one(&base, &theirs, &ours));
    assert_eq!(found.len(), 1);
    assert!(
        found
            .first()
            .unwrap()
            .theirs
            .as_deref()
            .unwrap()
            .starts_with("::: note")
    );
    // A change outside it is independent.
    let outside = page(ID_A, "", &body.replace("Outro.", "Outro, revised."));
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &outside)),
        page(
            ID_A,
            "",
            &body
                .replace("Inside one.", "Inside, theirs.")
                .replace("Outro.", "Outro, revised.")
        )
    );
}

#[test]
fn whitespace_around_a_block_is_not_a_change() {
    let base = page(ID_A, "", BASE_BODY);
    let respaced = page(
        ID_A,
        "",
        "First paragraph. ^p1\n\n\n\nSecond paragraph. ^p2\n\nThird paragraph. ^p3\n\n\n",
    );
    let edited = page(
        ID_A,
        "",
        &BASE_BODY.replace("Third paragraph.", "Third, revised."),
    );
    let merged = clean_text(merge_one(&base, &respaced, &edited));
    // The edit lands, and the layout the tip changed stays.
    assert_eq!(
        merged,
        page(
            ID_A,
            "",
            "First paragraph. ^p1\n\n\n\nSecond paragraph. ^p2\n\nThird, revised. ^p3\n\n\n"
        )
    );
}

#[test]
fn a_block_that_moved_is_not_guessed_at() {
    let base = page(ID_A, "", "One. ^1\n\nTwo. ^2\n\nThree. ^3\n");
    let moved = page(ID_A, "", "Three. ^3\n\nOne. ^1\n\nTwo. ^2\n");
    let edited = page(ID_A, "", "One. ^1\n\nTwo. ^2\n\nThree, edited. ^3\n");
    // The tip moved Three; the proposal edited it where it was: they met in one block, so they overlap.
    assert!(!merge_one(&base, &moved, &edited).is_clean());
    // A move on one side alone is taken.
    assert_eq!(clean_text(merge_one(&base, &moved, &base)), moved);
}

#[test]
fn ids_keep_neighbouring_changes_apart() {
    // The tip changed two neighbouring blocks, the proposal one of them: only that one overlaps. Without the
    // ids there is no telling which block became which, and the whole stretch would be the overlap.
    let base = page(
        ID_A,
        "",
        "One. ^p1\n\nTwo. ^p2\n\nThree. ^p3\n\nFour. ^p4\n",
    );
    let theirs = page(
        ID_A,
        "",
        "One, theirs. ^p1\n\nTwo, theirs. ^p2\n\nThree. ^p3\n\nFour. ^p4\n",
    );
    let ours = page(
        ID_A,
        "",
        "One. ^p1\n\nTwo, ours. ^p2\n\nThree. ^p3\n\nFour. ^p4\n",
    );
    let found = overlaps(merge_one(&base, &theirs, &ours));
    assert_eq!(found.len(), 1);
    let o = found.first().unwrap();
    assert_eq!(o.theirs.as_deref(), Some("Two, theirs. ^p2"));
    assert_eq!(o.ours.as_deref(), Some("Two, ours. ^p2"));
    // And where the proposal's change is to the other one, the merge is clean: both are taken.
    let ours = page(
        ID_A,
        "",
        "One. ^p1\n\nTwo. ^p2\n\nThree. ^p3\n\nFour, ours. ^p4\n",
    );
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &ours)),
        page(
            ID_A,
            "",
            "One, theirs. ^p1\n\nTwo, theirs. ^p2\n\nThree. ^p3\n\nFour, ours. ^p4\n"
        )
    );
}

#[test]
fn two_blocks_are_never_glued_when_layout_is_taken_from_different_sides() {
    // The tip dropped the blank line before the block, the proposal put another block in front of it: the
    // layouts disagree, and the block must still start on a line of its own.
    let base = "\nB. ^b\n";
    let theirs = "B. ^b\n";
    let ours = "N. ^n\n\nB. ^b\n";
    let merged = clean_text(merge_one(base, theirs, ours));
    assert_eq!(merged, "N. ^n\n\nB. ^b\n");
}

#[test]
fn a_closing_fence_with_no_line_break_does_not_swallow_the_body() {
    // The tip's frontmatter ends the file (no line break after the closing fence); the proposal added a body.
    let base = "---\nid: x\n---\n";
    let theirs = "---\nid: x\nk: 1\n---";
    let ours = "---\nid: x\n---\nBody.\n";
    let merged = clean_text(merge(
        &[pt("pages/A.md", base)],
        &[pt("pages/A.md", theirs)],
        &[pt("pages/A.md", ours)],
    ));
    assert_eq!(merged, "---\nid: x\nk: 1\n---\nBody.\n");
}

// ------------------------------------------------------------------------------------- the frontmatter

#[test]
fn a_frontmatter_key_changed_on_one_side_only_is_taken() {
    let base = page(ID_A, "license: CC0\ntags: [a]\n", BASE_BODY);
    let theirs = page(ID_A, "license: CC0\ntags: [a, b]\n", BASE_BODY);
    let ours = page(ID_A, "license: MIT\ntags: [a]\n", BASE_BODY);
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &ours)),
        page(ID_A, "license: MIT\ntags: [a, b]\n", BASE_BODY)
    );
}

#[test]
fn a_frontmatter_key_changed_differently_on_both_sides_is_an_overlap() {
    let base = page(ID_A, "license: CC0\n", BASE_BODY);
    let theirs = page(ID_A, "license: MIT\n", BASE_BODY);
    let ours = page(ID_A, "license: GPL\n", BASE_BODY);
    let found = overlaps(merge_one(&base, &theirs, &ours));
    assert_eq!(found.len(), 1);
    let o = found.first().unwrap();
    assert_eq!(
        o.place,
        Place::Frontmatter {
            key: "license".into()
        }
    );
    assert_eq!(o.base.as_deref(), Some("license: CC0\n"));
    assert_eq!(o.theirs.as_deref(), Some("license: MIT\n"));
    assert_eq!(o.ours.as_deref(), Some("license: GPL\n"));
}

#[test]
fn a_frontmatter_key_changed_alike_is_taken_once_and_keys_added_apart_both_land() {
    let base = page(ID_A, "license: CC0\n", BASE_BODY);
    let theirs = page(ID_A, "license: MIT\nlang: en\n", BASE_BODY);
    let ours = page(ID_A, "license: MIT\nstatus: draft\n", BASE_BODY);
    let merged = clean_text(merge_one(&base, &theirs, &ours));
    assert_eq!(
        merged,
        page(ID_A, "license: MIT\nlang: en\nstatus: draft\n", BASE_BODY)
    );
}

#[test]
fn a_frontmatter_key_removed_on_one_side_and_changed_on_the_other_is_an_overlap() {
    let base = page(ID_A, "license: CC0\n", BASE_BODY);
    let removed = page(ID_A, "", BASE_BODY);
    let changed = page(ID_A, "license: MIT\n", BASE_BODY);
    assert_eq!(overlaps(merge_one(&base, &removed, &changed)).len(), 1);
    assert_eq!(clean_text(merge_one(&base, &removed, &base)), removed);
}

#[test]
fn a_nested_key_is_one_unit_with_its_lines() {
    let base = page(ID_A, "ext:\n  a: 1\n  b: 2\nlicense: CC0\n", BASE_BODY);
    let theirs = page(ID_A, "ext:\n  a: 1\n  b: 3\nlicense: CC0\n", BASE_BODY);
    let ours = page(ID_A, "ext:\n  a: 1\n  b: 2\nlicense: MIT\n", BASE_BODY);
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &ours)),
        page(ID_A, "ext:\n  a: 1\n  b: 3\nlicense: MIT\n", BASE_BODY)
    );
}

#[test]
fn frontmatter_the_parser_refuses_is_one_unit() {
    // A repeated key is no mapping to the parser: the frontmatter is compared as a whole.
    let bad = |v: &str| format!("---\nid: {ID_A}\nk: 1\nk: {v}\n---\n{BASE_BODY}");
    assert_eq!(
        clean_text(merge_one(&bad("1"), &bad("2"), &bad("1"))),
        bad("2")
    );
    let found = overlaps(merge_one(&bad("1"), &bad("2"), &bad("3")));
    assert_eq!(
        found.first().map(|o| o.place.clone()),
        Some(Place::Frontmatter { key: String::new() })
    );
}

#[test]
fn a_page_with_no_frontmatter_is_merged_by_its_blocks_and_keeps_its_byte_order_mark() {
    let plain = |body: &str| format!("\u{feff}{body}");
    let base = plain("First. ^a\n\nSecond. ^b\n");
    let theirs = plain("First, theirs. ^a\n\nSecond. ^b\n");
    let ours = plain("First. ^a\n\nSecond, ours. ^b\n");
    assert_eq!(
        clean_text(merge_one(&base, &theirs, &ours)),
        plain("First, theirs. ^a\n\nSecond, ours. ^b\n")
    );
}

// --------------------------------------------------------------------------------------- the page set

#[test]
fn a_rename_beside_an_edit_lands_the_edit_on_the_new_path() {
    let base = page(ID_A, "", BASE_BODY);
    let edited = page(
        ID_A,
        "",
        &BASE_BODY.replace("Second paragraph.", "Second, revised."),
    );
    let outcome = merge(
        &[pt("pages/Old.md", &base)],
        &[pt("pages/New.md", &base)],
        &[pt("pages/Old.md", &edited)],
    );
    let MergeOutcome::Clean { pages } = outcome else {
        panic!("a rename and an edit overlapped");
    };
    assert_eq!(
        pages,
        [MergedPage {
            path: "pages/New.md".into(),
            text: edited.clone()
        }]
    );
    // The same with the sides swapped: the proposal renames, the tip edited.
    let outcome = merge(
        &[pt("pages/Old.md", &base)],
        &[pt("pages/Old.md", &edited)],
        &[pt("pages/New.md", &base)],
    );
    assert_eq!(
        outcome,
        MergeOutcome::Clean {
            pages: vec![MergedPage {
                path: "pages/New.md".into(),
                text: edited
            }]
        }
    );
}

#[test]
fn a_page_moved_to_two_places_is_an_overlap_and_the_same_move_is_taken_once() {
    let base = page(ID_A, "", BASE_BODY);
    let found = overlaps(merge(
        &[pt("pages/Old.md", &base)],
        &[pt("pages/One.md", &base)],
        &[pt("pages/Two.md", &base)],
    ));
    assert_eq!(found.len(), 1);
    let o = found.first().unwrap();
    assert_eq!(o.place, Place::Path);
    assert_eq!(
        (o.base.as_deref(), o.theirs.as_deref(), o.ours.as_deref()),
        (
            Some("pages/Old.md"),
            Some("pages/One.md"),
            Some("pages/Two.md")
        )
    );
    assert_eq!(o.line_level, LineLevel::NotText);
    let outcome = merge(
        &[pt("pages/Old.md", &base)],
        &[pt("pages/One.md", &base)],
        &[pt("pages/One.md", &base)],
    );
    assert!(outcome.is_clean());
}

#[test]
fn a_page_removed_on_one_side_and_edited_on_the_other_is_an_overlap() {
    let base = page(ID_A, "", BASE_BODY);
    let edited = page(
        ID_A,
        "",
        &BASE_BODY.replace("Second paragraph.", "Second, revised."),
    );
    for (theirs, ours) in [
        (vec![], vec![pt("pages/A.md", &edited)]),
        (vec![pt("pages/A.md", &edited)], vec![]),
    ] {
        let found = overlaps(merge(&[pt("pages/A.md", &base)], &theirs, &ours));
        assert_eq!(found.len(), 1);
        let o = found.first().unwrap();
        assert_eq!(o.place, Place::Page);
        assert_eq!(o.base.as_deref(), Some(base.as_str()));
        assert!(o.theirs.is_some() ^ o.ours.is_some());
    }
    // A removal of a page nobody else touched is taken; a rename counts as a touch.
    let gone = merge(&[pt("pages/A.md", &base)], &[pt("pages/A.md", &base)], &[]);
    assert_eq!(gone, MergeOutcome::Clean { pages: vec![] });
    let renamed = overlaps(merge(
        &[pt("pages/A.md", &base)],
        &[pt("pages/B.md", &base)],
        &[],
    ));
    assert_eq!(renamed.first().map(|o| o.place.clone()), Some(Place::Page));
}

#[test]
fn pages_added_apart_are_both_kept_and_two_pages_at_one_path_overlap() {
    let a = page(ID_A, "", "A.\n");
    let b = page(ID_B, "", "B.\n");
    let outcome = merge(&[], &[pt("pages/A.md", &a)], &[pt("pages/B.md", &b)]);
    assert_eq!(
        outcome,
        MergeOutcome::Clean {
            pages: vec![
                MergedPage {
                    path: "pages/A.md".into(),
                    text: a.clone()
                },
                MergedPage {
                    path: "pages/B.md".into(),
                    text: b.clone()
                },
            ]
        }
    );
    let found = overlaps(merge(&[], &[pt("pages/X.md", &a)], &[pt("pages/X.md", &b)]));
    assert_eq!(found.len(), 1);
    let o = found.first().unwrap();
    assert_eq!(
        (o.place.clone(), o.theirs.as_deref(), o.ours.as_deref()),
        (Place::Path, Some(a.as_str()), Some(b.as_str()))
    );
}

#[test]
fn pages_without_an_id_are_matched_by_path_and_a_shared_id_names_neither() {
    let plain = |body: &str| format!("# Title\n\n{body}\n");
    let merged = merge(
        &[pt("pages/P.md", &plain("one"))],
        &[pt("pages/P.md", &plain("one"))],
        &[pt("pages/P.md", &plain("two"))],
    );
    assert_eq!(
        merged,
        MergeOutcome::Clean {
            pages: vec![MergedPage {
                path: "pages/P.md".into(),
                text: plain("two")
            }]
        }
    );
    // Two pages of one side with one id fall back to their paths, so neither is taken for the other.
    let x = page(ID_A, "", "X.\n");
    let y = page(ID_A, "", "Y.\n");
    let outcome = merge(
        &[pt("pages/X.md", &x), pt("pages/Y.md", &y)],
        &[pt("pages/X.md", &x), pt("pages/Y.md", &y)],
        &[pt("pages/X.md", &x), pt("pages/Y.md", &y)],
    );
    assert!(outcome.is_clean());
}

#[test]
fn the_pages_come_back_in_path_order_whatever_order_they_went_in() {
    let (a, b) = (page(ID_A, "", "A.\n"), page(ID_B, "", "B.\n"));
    let outcome = merge(
        &[pt("pages/B.md", &b), pt("pages/A.md", &a)],
        &[pt("pages/B.md", &b), pt("pages/A.md", &a)],
        &[pt("pages/B.md", &b), pt("pages/A.md", &a)],
    );
    let MergeOutcome::Clean { pages } = outcome else {
        panic!()
    };
    assert_eq!(
        pages.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
        ["pages/A.md", "pages/B.md"]
    );
}

// ------------------------------------------------------------------------------ the measurement hook

#[test]
fn the_hook_counts_overlaps_a_line_level_merge_would_have_made_clean() {
    let lines = |a: &str, b: &str, c: &str| format!("{a}\n{b}\n{c}");
    // Different lines of one paragraph: a line-level merge is clean.
    assert!(would_be_clean(
        &lines("a", "b", "c"),
        &lines("A", "b", "c"),
        &lines("a", "b", "C")
    ));
    // The same line, or lines that touch: it is not.
    assert!(!would_be_clean(
        &lines("a", "b", "c"),
        &lines("A", "b", "c"),
        &lines("X", "b", "c")
    ));
    assert!(!would_be_clean(
        &lines("a", "b", "c"),
        &lines("A", "b", "c"),
        &lines("a", "B", "c")
    ));
    // The same change on both sides is no conflict.
    assert!(would_be_clean(
        &lines("a", "b", "c"),
        &lines("A", "b", "c"),
        &lines("A", "b", "c")
    ));

    let body = |l: &str| format!("Line one of the paragraph.\nLine two.\nLine three of it. {l}\n");
    let base = page(ID_A, "", &body("^p"));
    let theirs = page(ID_A, "", &body("^p").replace("Line one", "Line 1"));
    let ours = page(ID_A, "", &body("^p").replace("Line two", "Line 2"));
    // Adjacent lines: a conflict at line level too.
    let outcome = merge_one(&base, &theirs, &ours);
    assert_eq!(outcome.line_level_clean(), 0);
    let ours = page(
        ID_A,
        "",
        &body("^p").replace("Line three of it.", "Line 3."),
    );
    let outcome = merge_one(&base, &theirs, &ours);
    // The block overlaps (the unit is the block) and the hook says a line merge would not have.
    assert!(!outcome.is_clean());
    assert_eq!(outcome.line_level_clean(), 1);
    // The hook never changes what the merge does.
    assert_eq!(overlaps(outcome).len(), 1);
}

// ------------------------------------------------------------------------- properties, on generated text

/// A page's body: what follows its frontmatter.
fn body_of(text: &str) -> &str {
    inklkunn_markup::frontmatter::split(text).1
}

/// xorshift64*: the same pages on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        *items.get(self.below(items.len())).unwrap()
    }
}

/// A generated page: some frontmatter keys and a run of blocks with the gaps between them.
#[derive(Clone)]
struct Doc {
    front: Option<Vec<(String, String)>>,
    blocks: Vec<String>,
    gaps: Vec<&'static str>,
    tail: &'static str,
}

const GAPS: &[&str] = &["\n\n", "\n\n", "\n\n\n", "\n \n", "\n\n\n\n"];
const TAILS: &[&str] = &["\n", "", "\n\n", "  \n"];

impl Doc {
    fn render(&self) -> String {
        let mut out = String::new();
        if let Some(front) = &self.front {
            out.push_str("---\n");
            for (k, v) in front {
                out.push_str(&format!("{k}: {v}\n"));
            }
            out.push_str("---\n");
        }
        for (i, b) in self.blocks.iter().enumerate() {
            out.push_str(if i == 0 {
                "\n"
            } else {
                self.gaps.get(i).copied().unwrap_or("\n\n")
            });
            out.push_str(b);
        }
        out.push_str(self.tail);
        out
    }
}

fn fresh_block(rng: &mut Rng, counter: &mut usize) -> String {
    *counter += 1;
    let n = *counter;
    match rng.below(9) {
        0 => format!("## Heading {n}"),
        1 => format!("- item {n}\n- item {n}b"),
        2 => format!("> quote {n}"),
        3 => format!("```\ncode {n}\n\nmore {n}\n```"),
        4 => format!("| a | b |\n|---|---|\n| {n} | 2 |\n^t{n}"),
        5 => format!("::: note\ninside {n}\n\ninside {n}b\n:::"),
        6 => format!("Plain paragraph {n} without an id."),
        7 => format!("Two lines {n}\nof one paragraph ^k{n}"),
        _ => format!("Paragraph {n} with an id. ^k{n}"),
    }
}

fn fresh_doc(rng: &mut Rng, counter: &mut usize) -> Doc {
    let front = rng.below(4) != 0;
    let keys = ["license", "lang", "status", "tags", "description"];
    let mut kv = vec![("id".to_owned(), ID_A.to_owned())];
    for k in keys {
        if rng.below(2) == 0 {
            kv.push((k.to_owned(), format!("v{}", rng.below(4))));
        }
    }
    let n = rng.below(8);
    let blocks: Vec<String> = (0..n).map(|_| fresh_block(rng, counter)).collect();
    let gaps = (0..n).map(|_| rng.pick(GAPS)).collect();
    Doc {
        front: front.then_some(kv),
        blocks,
        gaps,
        tail: rng.pick(TAILS),
    }
}

/// A few edits to a page: change, remove or add a block, change a key, or re-space it.
fn mutate(rng: &mut Rng, counter: &mut usize, doc: &Doc) -> Doc {
    let mut d = doc.clone();
    for _ in 0..=rng.below(3) {
        match rng.below(7) {
            0 | 1 if !d.blocks.is_empty() => {
                let i = rng.below(d.blocks.len());
                if let Some(b) = d.blocks.get_mut(i) {
                    // Keep the id and the shape: the text before it changes.
                    *b = match b.rsplit_once(" ^") {
                        Some((text, id)) if !b.contains('\n') => {
                            format!("{text} (edit {}) ^{id}", rng.below(5))
                        }
                        _ => format!("{b}\n\nand a sentence {}", rng.below(5)),
                    };
                }
            }
            2 if !d.blocks.is_empty() => {
                let i = rng.below(d.blocks.len());
                d.blocks.remove(i);
                d.gaps.remove(i);
            }
            3 | 4 => {
                let i = rng.below(d.blocks.len() + 1);
                d.blocks.insert(i, fresh_block(rng, counter));
                d.gaps.insert(i, rng.pick(GAPS));
            }
            5 => {
                if let Some(front) = &mut d.front {
                    let i = 1 + rng.below(front.len().max(2) - 1);
                    if let Some(kv) = front.get_mut(i) {
                        kv.1 = format!("w{}", rng.below(4));
                    }
                }
            }
            _ => d.tail = rng.pick(TAILS),
        }
    }
    d
}

#[test]
fn merging_with_nothing_changed_on_a_side_gives_the_other_side_exactly() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut counter = 0;
    for round in 0..400 {
        let base = fresh_doc(&mut rng, &mut counter);
        let x = if round % 4 == 0 {
            fresh_doc(&mut rng, &mut counter)
        } else {
            mutate(&mut rng, &mut counter, &base)
        };
        let (b, x) = (base.render(), x.render());
        // The engine itself, not the page-level shortcuts of `merge`.
        assert_eq!(
            merge_text(&b, &b, &x),
            Ok(x.clone()),
            "merge(base, base, x) in round {round}\nbase: {b:?}\nx: {x:?}"
        );
        assert_eq!(
            merge_text(&b, &x, &b),
            Ok(x.clone()),
            "merge(base, x, base) in round {round}\nbase: {b:?}\nx: {x:?}"
        );
        assert_eq!(
            merge_text(&b, &x, &x),
            Ok(x.clone()),
            "merge(base, x, x) in round {round}\nbase: {b:?}\nx: {x:?}"
        );
        // And through the page set.
        assert_eq!(clean_text(merge_one(&b, &b, &x)), x);
        assert_eq!(clean_text(merge_one(&b, &x, &b)), x);
        assert_eq!(clean_text(merge_one(&b, &x, &x)), x);
    }
}

#[test]
fn the_order_of_the_two_sides_never_changes_the_verdict() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut counter = 0;
    let (mut clean, mut overlap) = (0, 0);
    for round in 0..600 {
        let base = fresh_doc(&mut rng, &mut counter);
        let t = mutate(&mut rng, &mut counter, &base).render();
        let o = mutate(&mut rng, &mut counter, &base).render();
        let b = base.render();
        let (ab, ba) = (merge_text(&b, &t, &o), merge_text(&b, &o, &t));
        assert_eq!(
            ab.is_ok(),
            ba.is_ok(),
            "round {round}\nbase: {b:?}\nt: {t:?}\no: {o:?}"
        );
        match (ab, ba) {
            (Ok(_), Ok(_)) => clean += 1,
            (Err(x), Err(y)) => {
                overlap += 1;
                // The same overlaps, each with its two texts the other way round.
                assert_eq!(x.len(), y.len());
                for (p, q) in x.iter().zip(&y) {
                    assert_eq!(p.theirs, q.ours, "round {round}");
                    assert_eq!(p.ours, q.theirs, "round {round}");
                    assert_eq!(p.base, q.base);
                }
            }
            _ => unreachable!(),
        }
    }
    // The generator exercises both outcomes.
    assert!(
        clean > 50 && overlap > 50,
        "clean {clean}, overlap {overlap}"
    );
}

#[test]
fn a_clean_merge_keeps_every_block_both_sides_wrote() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut counter = 0;
    let mut checked = 0;
    for _ in 0..600 {
        let base = fresh_doc(&mut rng, &mut counter);
        let t = mutate(&mut rng, &mut counter, &base);
        let o = mutate(&mut rng, &mut counter, &base);
        let Ok(merged) = merge_text(&base.render(), &t.render(), &o.render()) else {
            continue;
        };
        checked += 1;
        // A block a side added or changed (one no block of the base is) is in the result.
        let base_text = base.render();
        let known: Vec<&str> = inklkunn_markup::blocks(body_of(&base_text))
            .iter()
            .map(|b| b.text)
            .collect();
        for side in [&t, &o] {
            let text = side.render();
            for block in inklkunn_markup::blocks(body_of(&text)) {
                if !known.contains(&block.text) {
                    assert!(
                        merged.contains(block.text),
                        "lost {:?} from {merged:?}",
                        block.text
                    );
                }
            }
        }
    }
    assert!(checked > 100, "{checked}");
}

// ------------------------------------------------------------------------------ it never panics

#[test]
fn nothing_panics_on_construct_soup_in_any_of_the_three_places() {
    let atoms = [
        "---\n",
        "id: x\n",
        "id: y\n",
        "k: 1\n",
        "k: 2\n",
        "- a\n",
        "\n",
        "\n\n",
        "::: n\n",
        ":::\n",
        "```\n",
        "<!--",
        "-->",
        "^id",
        " ^id\n",
        "> ",
        "| a |\n|-|\n",
        "# H\n",
        "[r]: /u\n",
        "text",
        "é",
        "編",
        "\r\n",
        "    ",
        "\u{feff}",
        "...\n",
        "a: &x 1\n",
        ": \n",
        "\t",
    ];
    let mut rng = Rng(0xA076_1D64_78BD_642F);
    let soup = |rng: &mut Rng| -> String {
        let n = rng.below(40);
        (0..n).map(|_| rng.pick(&atoms)).collect()
    };
    for _ in 0..1500 {
        let (b, t, o) = (soup(&mut rng), soup(&mut rng), soup(&mut rng));
        let _ = merge_text(&b, &t, &o);
        let _ = merge(
            &[pt("pages/A.md", &b)],
            &[pt("pages/A.md", &t)],
            &[pt("pages/B.md", &o)],
        );
        let _ = would_be_clean(&b, &t, &o);
        // Identity holds on arbitrary text as well.
        assert_eq!(merge_text(&b, &b, &o), Ok(o.clone()), "{b:?} {o:?}");
        assert_eq!(merge_text(&b, &o, &b), Ok(o.clone()), "{b:?} {o:?}");
    }
}

#[test]
fn nothing_panics_on_arbitrary_unicode() {
    let mut rng = Rng(0xE703_7ED1_A0B4_28DB);
    let alphabet: Vec<char> = "ab^:-#>|`[]\n\r \t\u{feff}\u{2028}é編😀".chars().collect();
    for _ in 0..1500 {
        let text =
            |rng: &mut Rng| -> String { (0..rng.below(60)).map(|_| rng.pick(&alphabet)).collect() };
        let (b, t, o) = (text(&mut rng), text(&mut rng), text(&mut rng));
        let _ = merge_text(&b, &t, &o);
    }
}

// --------------------------------------------------------------------------------------------- scaling

/// How many times a shape is measured before a ratio above the bound is believed (see the markup crate's
/// robustness tests, which this follows): load can only slow a run down, so one measurement within the bound
/// is the evidence.
const ATTEMPTS: usize = 5;
/// The time may grow this many times faster than the input's length.
const SLACK: f64 = 1.5;
/// Runs under this are noise.
const NOISE: Duration = Duration::from_millis(40);
const CEILING: Duration = Duration::from_secs(20);

fn time(f: &dyn Fn()) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..2 {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed());
    }
    best
}

/// Build the three texts at `n` and `2n`, merge each, and require near-linear scaling in one of up to
/// [`ATTEMPTS`] measurements.
fn linear(label: &str, n: usize, build: impl Fn(usize) -> (String, String, String)) {
    let (small, large) = (build(n), build(2 * n));
    let len = |s: &(String, String, String)| (s.0.len() + s.1.len() + s.2.len()) as f64;
    let size_ratio = len(&large) / len(&small).max(1.0);
    let work = |s: &(String, String, String)| {
        let _ = merge_text(&s.0, &s.1, &s.2);
    };
    let mut worst = String::new();
    for _ in 0..ATTEMPTS {
        let t_small = time(&|| work(&small));
        let t_large = time(&|| work(&large));
        assert!(t_large < CEILING, "{label}: {t_large:?} for 2n");
        if t_large <= NOISE {
            return;
        }
        let time_ratio = t_large.as_secs_f64() / t_small.as_secs_f64().max(0.000_5);
        if time_ratio <= SLACK * size_ratio {
            return;
        }
        worst = format!(
            "{label}: the input grew {size_ratio:.1}x and the time {time_ratio:.1}x ({t_small:?} -> {t_large:?}) in each of {ATTEMPTS} measurements"
        );
    }
    panic!("{worst}");
}

#[test]
fn the_merge_is_linear_in_the_length_of_the_pages() {
    // Many blocks with ids, each side editing every tenth.
    linear("blocks with ids", 20_000, |n| {
        let body = |edit: usize| -> String {
            (0..n)
                .map(|i| {
                    let text = if edit > 0 && i % 10 == edit {
                        "edited"
                    } else {
                        "text"
                    };
                    format!("Paragraph {i} {text}. ^p{i}\n\n")
                })
                .collect()
        };
        (body(0), body(3), body(7))
    });
    // Many blocks without ids, so the content and the alignment stages do the work.
    linear("blocks without ids", 20_000, |n| {
        let body = |edit: usize| -> String {
            (0..n)
                .map(|i| {
                    let text = if edit > 0 && i % 10 == edit {
                        "edited"
                    } else {
                        "text"
                    };
                    format!("Paragraph {i} {text}.\n\n")
                })
                .collect()
        };
        (body(0), body(3), body(7))
    });
    // Many keys.
    linear("frontmatter keys", 20_000, |n| {
        let front = |edit: usize| -> String {
            let keys: String = (0..n)
                .map(|i| {
                    format!(
                        "k{i}: {}\n",
                        if edit > 0 && i % 10 == edit { "2" } else { "1" }
                    )
                })
                .collect();
            format!("---\n{keys}---\nBody.\n")
        };
        (front(0), front(3), front(7))
    });
    // Everything different: no block in common, the worst case for the alignment (bounded, never quadratic).
    linear("nothing in common", 4_000, |n| {
        let body = |tag: &str| -> String { (0..n).map(|i| format!("{tag} {i}.\n\n")).collect() };
        (body("base"), body("theirs"), body("ours"))
    });
    // Containers, and lines that look like them.
    linear("containers", 5_000, |n| {
        let body = |t: &str| "::: note\n".to_owned() + &format!("{t}\n:::\n\n").repeat(n);
        (body("a"), body("b"), body("a"))
    });
}

#[test]
fn align_blocks_is_the_alignment_the_merge_makes_between_a_base_and_a_side() {
    use super::align_blocks;
    let base = "---\ntitle: T\n---\n\nThe bridge opened in 1950.\n\nIt crosses the river.\n\nIt is 300 m long.\n";
    // Identical: every block is itself.
    assert_eq!(align_blocks(base, base), [Some(0), Some(1), Some(2)]);
    // A reworded block between two unchanged ones is the same block, changed (a stretch of one and one).
    let reworded = "---\ntitle: T\n---\n\nThe bridge was opened in 1950.\n\nIt crosses the river.\n\nIt is 300 m long.\n";
    assert_eq!(align_blocks(base, &reworded), [Some(0), Some(1), Some(2)]);
    // A removed block has no counterpart; the others keep theirs, renumbered.
    let removed = "---\ntitle: T\n---\n\nThe bridge opened in 1950.\n\nIt is 300 m long.\n";
    assert_eq!(align_blocks(base, removed), [Some(0), None, Some(1)]);
    // A block added is not in the base's map; the base's blocks still find theirs.
    let added = "---\ntitle: T\n---\n\nThe bridge opened in 1950.\n\nA new fact.\n\nIt crosses the river.\n\nIt is 300 m long.\n";
    assert_eq!(align_blocks(base, added), [Some(0), Some(2), Some(3)]);
    // An `^id` keeps a block through a rewording that unique text could not follow.
    let with_ids =
        "The bridge opened in 1950. ^b1\n\nIt crosses the river. ^b2\n\nIt is 300 m long. ^b3\n";
    let rewritten =
        "Completely different words. ^b1\n\nAnother sentence. ^b2\n\nAnd a third. ^b3\n";
    assert_eq!(
        align_blocks(with_ids, rewritten),
        [Some(0), Some(1), Some(2)]
    );
    // No page, no blocks.
    assert_eq!(align_blocks("", ""), Vec::<Option<usize>>::new());
}
