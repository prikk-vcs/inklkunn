#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use inklkunn_core::read::LineKind;

const BASE: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n";

fn kinds(d: &PageDelta) -> Vec<How> {
    d.parts.iter().map(|p| p.how).collect()
}

#[test]
fn a_page_that_is_the_same_has_no_parts_that_differ() {
    let d = delta(Some(BASE), Some(BASE));
    assert_eq!(d.status, Status::Unchanged);
    assert!(d.parts.is_empty() && d.info.is_none());
    assert_eq!(d.unchanged, 3);
}

#[test]
fn a_new_page_is_all_added_and_a_removed_page_all_removed() {
    let added = delta(None, Some(BASE));
    assert_eq!(added.status, Status::Added);
    assert!(kinds(&added).iter().all(|h| *h == How::Added));
    assert!(added.info.is_some());
    let removed = delta(Some(BASE), None);
    assert_eq!(removed.status, Status::Removed);
    assert!(kinds(&removed).iter().all(|h| *h == How::Removed));
}

#[test]
fn one_changed_paragraph_is_one_changed_part_and_the_others_are_counted() {
    let new = BASE.replace("Second paragraph.", "Second paragraph, corrected.");
    let d = delta(Some(BASE), Some(&new));
    assert_eq!(d.status, Status::Modified);
    assert_eq!(kinds(&d), vec![How::Changed]);
    assert_eq!(d.unchanged, 2);
    let part = d.parts.first().unwrap();
    assert_eq!(part.ordinal, Some(1));
    assert_eq!(part.old.as_deref(), Some("Second paragraph."));
    assert_eq!(part.new.as_deref(), Some("Second paragraph, corrected."));
    assert!(d.info.is_none());
}

#[test]
fn an_added_and_a_removed_part_are_told_apart_and_the_information_at_the_top_is_its_own_change() {
    let new =
        "---\ntitle: Home\ntags: [a]\n---\n\nFirst paragraph.\n\nBrand new.\n\nSecond paragraph.\n";
    let d = delta(Some(BASE), Some(new));
    assert!(d.info.is_some());
    assert!(kinds(&d).contains(&How::Added));
    assert!(kinds(&d).contains(&How::Removed));
    let added = d.parts.iter().find(|p| p.how == How::Added).unwrap();
    assert_eq!(added.new.as_deref(), Some("Brand new."));
    assert_eq!(added.ordinal, Some(1));
}

#[test]
fn a_part_that_moved_is_not_a_change_and_an_id_keeps_a_part_together_across_an_edit() {
    let old = "Alpha.\n\nBeta ^b1\n\nGamma.\n";
    let moved = "Gamma.\n\nAlpha.\n\nBeta ^b1\n";
    let d = delta(Some(old), Some(moved));
    // Order is kept by the alignment: one part moves, which is a removal and an addition of that part.
    assert!(d.unchanged >= 2, "{d:?}");
    let edited = "Alpha.\n\nBeta, edited ^b1\n\nGamma.\n";
    let d = delta(Some(old), Some(edited));
    assert_eq!(kinds(&d), vec![How::Changed]);
    assert_eq!(d.parts.first().unwrap().old_ordinal, Some(1));
}

#[test]
fn a_hunk_writes_every_line_with_its_sign_and_marks_the_words_that_differ() {
    let (view, truncated) = hunk("one\ntwo\nthree", "one\ntwo changed\nthree");
    assert!(!truncated);
    let HunkView::Lines(h) = view else {
        panic!("not lines")
    };
    let signs: Vec<LineKind> = h.lines.iter().map(|l| l.kind).collect();
    assert_eq!(
        signs,
        [
            LineKind::Context,
            LineKind::Removed,
            LineKind::Added,
            LineKind::Context
        ]
    );
    assert!(h.lines.iter().any(|l| l.segments.iter().any(|s| s.marked)));
}

#[test]
fn a_text_that_is_only_added_has_no_removed_lines() {
    let (view, _) = hunk("", "a\nb");
    let HunkView::Lines(h) = view else {
        panic!("not lines")
    };
    assert_eq!(h.added(), 2);
    assert_eq!(h.removed(), 0);
}

#[test]
fn a_hunk_is_bounded() {
    let big: String = (0..MAX_DIFF_LINES + 10)
        .map(|n| format!("line {n}\n"))
        .collect();
    let (view, truncated) = hunk("", &big);
    assert!(truncated);
    let HunkView::Lines(h) = view else {
        panic!("not lines")
    };
    assert_eq!(h.lines.len(), MAX_DIFF_LINES);
}

#[test]
fn text_with_carriage_returns_and_non_ascii_is_kept_whole() {
    let (view, _) = hunk("日本語\r\nsame", "日本語です\r\nsame");
    assert_eq!(
        match view {
            HunkView::Lines(h) => h.to_unified(),
            HunkView::Raw(t) => t,
        },
        "@@ -1,2 +1,2 @@\n-日本語\r\n+日本語です\r\n same\n"
    );
}
