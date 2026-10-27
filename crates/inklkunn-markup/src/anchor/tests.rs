#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn github_style_anchors() {
    assert_eq!(
        github_anchor("Title with `code` and *emphasis*"),
        "title-with-code-and-emphasis"
    );
    assert_eq!(github_anchor("2.1 Numbered heading"), "21-numbered-heading");
    assert_eq!(
        github_anchor("Cunningham's Law & more"),
        "cunninghams-law--more"
    );
    assert_eq!(github_anchor("編集の競合"), "編集の競合");
    assert_eq!(
        github_anchor("Heading with [[Link]] and [md](x.md)"),
        "heading-with-link-and-md"
    );
}

#[test]
fn duplicates_are_suffixed_and_every_id_is_unique() {
    let mut a = Anchors::new();
    assert_eq!(a.next("Merging"), "merging");
    assert_eq!(a.next("Merging"), "merging-1");
    assert_eq!(a.next("Merging"), "merging-2");
    // GitHub's rule: `a`, `a-1`, `a` → `a`, `a-1`, `a-2`, never two `a-1`s.
    let mut b = Anchors::new();
    assert_eq!(b.next("a"), "a");
    assert_eq!(b.next("a-1"), "a-1");
    assert_eq!(b.next("a"), "a-2");
    let mut c = Anchors::new();
    c.reserve("merging");
    assert_eq!(c.next("Merging"), "merging-1");
}

#[test]
fn the_reduction_matches_what_the_renderer_shows() {
    assert_eq!(github_anchor("_em_"), "em");
    assert_eq!(github_anchor("snake_case stays"), "snake_case-stays");
    assert_eq!(github_anchor("A <b>b</b>"), "a-b");
    assert_eq!(github_anchor("[[T#Sec]]"), "t");
    assert_eq!(github_anchor("[[Guides/Deep|shown]]"), "shown");
    assert_eq!(github_anchor("![[i.png]]"), "ipng");
}
