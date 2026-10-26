#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn plain_and_hierarchical_titles() {
    assert_eq!(title_to_path("Home").unwrap(), "pages/Home.md");
    assert_eq!(
        title_to_path("Guides/Getting Started").unwrap(),
        "pages/Guides/Getting Started.md"
    );
    assert_eq!(
        title_to_path(" 編集の競合 ").unwrap(),
        "pages/編集の競合.md"
    );
    assert_eq!(title_to_path("/Target").unwrap(), "pages/Target.md");
}

#[test]
fn forbidden_characters_are_substituted_and_reserved_names_refused() {
    assert_eq!(title_to_path("a:b*c?").unwrap(), "pages/a_b_c_.md");
    assert_eq!(title_to_path(".hidden").unwrap(), "pages/_hidden.md");
    assert_eq!(
        title_to_path("a/../b"),
        Err(PathError::Reserved("..".into()))
    );
    assert_eq!(title_to_path("a//b"), Err(PathError::Empty));
    assert_eq!(title_to_path(""), Err(PathError::Empty));
    assert!(matches!(
        title_to_path(".prikk"),
        Err(PathError::Reserved(_))
    ));
    assert!(matches!(
        title_to_path(&"x".repeat(300)),
        Err(PathError::TooLong(_))
    ));
}

#[test]
fn inverse_and_suffix() {
    assert_eq!(
        path_to_title("pages/Guides/Getting Started.md").as_deref(),
        Some("Guides/Getting Started")
    );
    assert_eq!(path_to_title("attachments/a.png"), None);
    assert_eq!(with_suffix("pages/a/b.md", 2), "pages/a/b-2.md");
    assert!(is_inside_pages("pages/a/b.md"));
    assert!(!is_inside_pages("pages/../x.md"));
    assert!(!is_inside_pages("knowledge/c.md"));
}
