#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

const PAGE: &str = "---\ntitle: Guide\n---\n\nIntro.\n\n## One\n\nOne text.\n\n### Deeper\n\nDeep text.\n\n## Two\n\nTwo text.\n";

#[test]
fn sections_run_from_a_heading_to_the_next_of_the_same_or_a_higher_level() {
    let s = sections(PAGE);
    let texts: Vec<&str> = s
        .iter()
        .map(|f| PAGE.get(f.range.clone()).unwrap())
        .collect();
    assert_eq!(
        texts,
        [
            "## One\n\nOne text.\n\n### Deeper\n\nDeep text.",
            "### Deeper\n\nDeep text.",
            "## Two\n\nTwo text."
        ]
    );
    assert_eq!(s.first().unwrap().excerpt, "One");
}

#[test]
fn parts_are_the_top_level_blocks_after_the_information_at_the_top() {
    let p = parts(PAGE);
    assert_eq!(p.first().unwrap().excerpt, "Intro.");
    assert_eq!(PAGE.get(p.first().unwrap().range.clone()), Some("Intro."));
    assert_eq!(p.len(), 7);
}

#[test]
fn an_edited_part_goes_back_exactly_where_it_came_from() {
    let found = find(PAGE, Selection::Block(2)).unwrap();
    let new = splice(PAGE, &found, "One text, better.").unwrap();
    assert_eq!(new, PAGE.replace("One text.", "One text, better."));
    let found = find(PAGE, Selection::Section(2)).unwrap();
    let new = splice(PAGE, &found, "## Two\n\nTwo text, more.").unwrap();
    assert!(new.ends_with("## Two\n\nTwo text, more.\n"));
    assert!(new.starts_with("---\ntitle: Guide\n---\n\nIntro."));
}

#[test]
fn the_whole_page_is_its_whole_text_and_a_part_that_is_not_there_is_none() {
    assert_eq!(find(PAGE, Selection::Whole).unwrap().range, 0..PAGE.len());
    assert!(find(PAGE, Selection::Section(9)).is_none());
    assert!(find(PAGE, Selection::Block(99)).is_none());
}

#[test]
fn an_excerpt_is_one_line_and_cut() {
    let long = "x".repeat(200);
    let e = excerpt(&format!("{long}\nsecond"));
    assert_eq!(e.chars().count(), EXCERPT_CHARS + 1);
    assert!(e.ends_with('…'));
    assert_eq!(excerpt("short\nline"), "short");
}
