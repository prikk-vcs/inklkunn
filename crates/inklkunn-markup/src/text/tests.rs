#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn normalize_collapses_separators_and_case() {
    assert_eq!(normalize("  Getting   Started "), "getting started");
    assert_eq!(
        normalize("guides/getting-started"),
        "guides/getting started"
    );
    assert_eq!(normalize("EDIT_Conflicts"), "edit conflicts");
    assert_eq!(nfc_trim("Cafe\u{301} "), "Café");
}

#[test]
fn escapes_and_percent_encoding() {
    assert_eq!(escape_html("a<b>&\"c'"), "a&lt;b&gt;&amp;&quot;c&#x27;");
    assert_eq!(percent_encode("Edit conflict"), "Edit%20conflict");
    assert_eq!(percent_encode("編"), "%E7%B7%A8");
    assert_eq!(
        escape_href("/wiki/Edit Conflicts#^blk"),
        "/wiki/Edit%20Conflicts#%5Eblk"
    );
}

#[test]
fn relative_targets_follow_posix_normpath() {
    assert_eq!(
        relative_target("Guides/Intro.md", "../Edit Conflicts"),
        "Edit Conflicts"
    );
    assert_eq!(
        relative_target("Guides/Intro.md", "./Getting Started.md"),
        "Guides/Getting Started"
    );
    assert_eq!(relative_target("Home.md", "../Up"), "../Up");
    assert_eq!(line_of("a\nb\nc", 4), 3);
}
