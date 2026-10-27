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
}

#[test]
fn the_line_index_answers_lines_and_ranges() {
    let ix = LineIndex::new("ab\ncd\n\nef");
    assert_eq!(ix.count(), 4);
    assert_eq!(ix.line_of(0), 1);
    assert_eq!(ix.line_of(2), 1);
    assert_eq!(ix.line_of(3), 2);
    assert_eq!(ix.line_of(6), 3);
    assert_eq!(ix.line_of(7), 4);
    assert_eq!(ix.range(1), Some((0, 2)));
    assert_eq!(ix.range(3), Some((6, 6)));
    assert_eq!(ix.range(4), Some((7, 9)));
    assert_eq!(ix.range(5), None);
    assert_eq!(LineIndex::new("").count(), 1);
}

#[test]
fn hrefs_are_allowed_by_scheme() {
    for ok in [
        "/wiki/A",
        "A#frag",
        "#x",
        "https://e.org/p?q#f",
        "HTTP://e.org",
        "mailto:a@b",
        "a/b:c",
    ] {
        assert!(href_is_allowed(ok), "{ok}");
    }
    for bad in [
        "javascript:alert(1)",
        "JavaScript:x",
        "data:text/html,x",
        "vbscript:x",
        "ftp://x",
        "a\nb",
    ] {
        assert!(!href_is_allowed(bad), "{bad}");
    }
}
