#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::read::{Sanitizer, highlight};

use super::*;

fn html(text: &str, folded_query: &str) -> String {
    snippet(&Sanitizer::profile(), &highlight(text, folded_query))
        .0
        .as_str()
        .to_owned()
}

#[test]
fn the_matching_part_is_marked_and_the_rest_is_plain() {
    assert_eq!(
        html("Getting Started", "start"),
        "Getting <strong>Start</strong>ed"
    );
    assert_eq!(html("Home", "zzz"), "Home");
}

#[test]
fn an_authors_markup_in_a_title_or_alias_is_shown_as_its_letters() {
    // The sanitizer test of the page body, repeated on a snippet (handoff 003 T6).
    for hostile in [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<a href=\"javascript:alert(1)\">go</a>",
        "<iframe src=\"https://evil.example/\"></iframe>",
        "<a class=\"wikilink\" href=\"https://evil.example/\">Home</a>",
        "\"><svg onload=alert(1)>",
    ] {
        for query in ["alert", "go", "home", "svg", "zzz"] {
            let out = html(hostile, query);
            assert!(!out.contains("<script"), "{hostile}: {out}");
            assert!(!out.contains("<img"), "{hostile}: {out}");
            assert!(!out.contains("<iframe"), "{hostile}: {out}");
            assert!(!out.contains("<svg"), "{hostile}: {out}");
            assert!(!out.contains("<a "), "{hostile}: {out}");
            assert!(!out.contains("onerror=alert(1)>"), "{hostile}: {out}");
            // What the author wrote is still there, as text.
            assert!(out.contains("&lt;"), "{hostile}: {out}");
        }
    }
}
