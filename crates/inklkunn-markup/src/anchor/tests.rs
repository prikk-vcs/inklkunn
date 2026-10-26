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
fn duplicates_are_suffixed() {
    let mut a = Anchors::new();
    assert_eq!(a.next("Merging"), "merging");
    assert_eq!(a.next("Merging"), "merging-1");
    assert_eq!(a.next("Merging"), "merging-2");
}
