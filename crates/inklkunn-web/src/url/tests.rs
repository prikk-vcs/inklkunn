#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn a_view_is_a_literal_tilde_and_a_title_that_starts_with_one_is_escaped() {
    assert_eq!(
        page_address("Home"),
        Some(PageAddress {
            view: None,
            title: "Home".into()
        })
    );
    assert_eq!(
        page_address("~draft/Home"),
        Some(PageAddress {
            view: Some("draft".into()),
            title: "Home".into()
        })
    );
    // A title with a slash keeps it; a view's page may be a hierarchy.
    assert_eq!(
        page_address("~draft/Guides/Setup").map(|a| a.title),
        Some("Guides/Setup".into())
    );
    // A title that starts with `~` is not a view when its tilde is escaped.
    assert_eq!(
        page_address("%7Esecret/x"),
        Some(PageAddress {
            view: None,
            title: "~secret/x".into()
        })
    );
    assert_eq!(encode_title("~secret/x"), "%7Esecret/x");
    assert_eq!(encode_title("a~b"), "a~b");
    // Every title round-trips through an address.
    for title in [
        "Home",
        "日本語/ページ",
        "a b",
        "~x",
        "100%",
        "a/b/c",
        "Q&A?",
    ] {
        let back = page_address(&encode_title(title)).expect("decodes");
        assert_eq!(back.view, None, "{title}");
        assert_eq!(back.title, title);
    }
}

#[test]
fn bad_escapes_and_bad_utf8_are_not_addresses() {
    assert!(decode("%").is_none());
    assert!(decode("%4").is_none());
    assert!(decode("%zz").is_none());
    assert!(decode("%ff").is_none(), "not UTF-8");
    assert_eq!(decode("a+b").as_deref(), Some("a+b"));
    assert!(page_address("%ff").is_none());
    assert!(page_address("~dr%ffaft/x").is_none());
}

#[test]
fn only_a_plain_name_is_a_view() {
    assert_eq!(view_ref("main").as_deref(), Some("heads/main"));
    assert_eq!(view_ref("v1.2_x-y").as_deref(), Some("heads/v1.2_x-y"));
    for bad in [
        "",
        ".hidden",
        "-x",
        "a/b",
        "a b",
        "ü",
        "..",
        &"x".repeat(101),
    ] {
        assert!(view_ref(bad).is_none(), "{bad}");
    }
}

#[test]
fn the_addresses_of_a_pages_past_carry_the_view_and_the_place_and_escape_the_title() {
    let b = "ab".repeat(32);
    assert_eq!(history_path("w", None, "Home", None), "/w/-/history/Home");
    assert_eq!(
        history_path("w", Some("draft"), "~x/日本", Some(7)),
        "/w/-/history/%7Ex/%E6%97%A5%E6%9C%AC?view=draft&after=7"
    );
    assert_eq!(change_path("w", None, &b), format!("/w/-/change/{b}"));
    assert_eq!(
        change_path("w", Some("draft"), &b),
        format!("/w/-/change/{b}?view=draft")
    );
    assert_eq!(
        diff_path("w", None, &b, &b, "a b"),
        format!("/w/-/diff/{b}/{b}/a%20b")
    );
    assert_eq!(
        at_path("w", None, &b, "Guides/Setup"),
        format!("/w/-/at/{b}/Guides/Setup")
    );
}
