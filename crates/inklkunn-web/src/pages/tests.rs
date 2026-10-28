#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::OnceLock;

use crate::catalog::{Dir, EN, EN_DEF, Entry, LangDef, Plural, Review};

use super::*;

fn other_only(_: u64) -> Plural {
    Plural::Other
}

/// English wrapped in guillemets, right to left: every word a page takes from the catalog shows, and a word
/// that does not come from the catalog does not.
fn qaa() -> &'static LangDef {
    static DEF: OnceLock<&'static LangDef> = OnceLock::new();
    DEF.get_or_init(|| {
        let entries: Vec<Entry> = EN_DEF
            .entries
            .iter()
            .map(|e| {
                let text: &'static str = Box::leak(format!("‹{}›", e.other).into_boxed_str());
                Entry {
                    key: e.key,
                    one: text,
                    other: text,
                }
            })
            .collect();
        Box::leak(Box::new(LangDef {
            tag: "qaa",
            endonym: "Test language",
            dir: Dir::Rtl,
            plural: other_only,
            group_separator: '.',
            review: Review::Preview,
            entries: Box::leak(entries.into_boxed_slice()),
        }))
    })
}

fn page_in(tag: &str) -> Page {
    let language = if tag == "qaa" {
        Lang::from_def(qaa())
    } else {
        Lang::from_tag(tag).unwrap()
    };
    Page::new(T::new(language), "/a/b?c=d")
}

/// The text a reader gets from `html`: what is outside tags, leaving out text marked as being in another
/// language (a language's name is data, not a word of the catalog).
fn visible_text(html: &str) -> Vec<String> {
    let mut stripped = String::new();
    let mut rest = html;
    while let Some(i) = rest.find("<span lang=\"") {
        let (before, from) = rest.split_at(i);
        stripped.push_str(before);
        let end = from
            .find("</span>")
            .map_or(from.len(), |j| j + "</span>".len());
        rest = from.get(end..).unwrap_or("");
    }
    stripped.push_str(rest);
    let mut nodes = Vec::new();
    let mut current = String::new();
    let mut in_tag = false;
    for c in stripped.chars() {
        match c {
            '<' => {
                in_tag = true;
                nodes.push(std::mem::take(&mut current));
            }
            '>' if in_tag => in_tag = false,
            c if !in_tag => current.push(c),
            _ => {}
        }
    }
    nodes.push(current);
    nodes
        .into_iter()
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .collect()
}

#[test]
fn a_page_in_the_test_language_has_its_direction_and_takes_every_word_from_the_catalog() {
    let page = page_in("qaa");
    for (name, html) in [
        ("not found", render_not_found(&page).unwrap()),
        (
            "language",
            render_language(&page.clone().without_switcher(), Some("/x")).unwrap(),
        ),
        (
            "a notice with a link",
            render_language_refused(&page).unwrap(),
        ),
        ("a refusal", render_form_refused(&page).unwrap()),
    ] {
        assert!(
            html.contains("<html lang=\"qaa\" dir=\"rtl\">"),
            "{name}: {html}"
        );
        for node in visible_text(&html) {
            let catalog_word = node.starts_with('‹') && node.ends_with('›');
            assert!(
                catalog_word || node == PRODUCT,
                "{name}: the text {node:?} does not come from the catalog"
            );
        }
        for label in html.split("aria-label=\"").skip(1) {
            assert!(
                label.starts_with('‹'),
                "{name}: an aria-label from nowhere: {label}"
            );
        }
    }
}

#[test]
fn the_footer_switcher_lists_every_language_and_marks_the_pages_own() {
    let en = page_in("en").languages();
    assert_eq!(
        en.iter()
            .map(|c| (c.tag, c.current, c.preview))
            .collect::<Vec<_>>(),
        [("en", true, false), ("ja", false, true)]
    );
    let ja = page_in("ja").languages();
    assert_eq!(
        ja.iter()
            .map(|c| (c.tag, c.current, c.preview))
            .collect::<Vec<_>>(),
        [("en", false, false), ("ja", true, true)]
    );
    // The name of each language is its own, marked with its own `lang`; a preview says so in the page's words.
    assert_eq!(
        en.first().unwrap().label.as_str(),
        "<span lang=\"en\">English</span>"
    );
    assert_eq!(
        en.get(1).unwrap().label.as_str(),
        "<span lang=\"ja\">日本語</span> (preview)"
    );
    assert_eq!(
        ja.get(1).unwrap().label.as_str(),
        "<span lang=\"ja\">日本語</span>（プレビュー版）"
    );
}

#[test]
fn a_switcher_form_posts_to_the_language_path_with_no_script_and_returns_to_the_page() {
    let html = render_not_found(&page_in("en")).unwrap();
    assert_eq!(
        html.matches("<form method=\"post\" action=\"/-/lang\">")
            .count(),
        2
    );
    assert!(html.contains("<input type=\"hidden\" name=\"next\" value=\"/a/b?c=d\">"));
    assert!(html.contains(
        "<button type=\"submit\" aria-current=\"true\"><span lang=\"en\">English</span></button>"
    ));
    for forbidden in ["<script", "onclick", "onsubmit", "javascript:", "style="] {
        assert!(!html.contains(forbidden), "{forbidden}: {html}");
    }
}

#[test]
fn the_language_page_has_its_own_choices_and_no_footer_switcher() {
    let html = render_language(&page_in("en").without_switcher(), None).unwrap();
    assert!(!html.contains("<nav"), "{html}");
    assert_eq!(html.matches("<form method=\"post\"").count(), 2, "{html}");
    // With nowhere to go back to, a choice stays on this page and there is no link back.
    assert!(html.contains("name=\"next\" value=\"/-/lang\""), "{html}");
    assert!(!html.contains("Go back to the page you were on"), "{html}");
    // The note about a preview is beside the language it is about, and only there.
    assert_eq!(html.matches("This translation is a preview.").count(), 1);
    assert_eq!(html.matches("In use now.").count(), 1);
}

#[test]
fn the_language_page_offers_a_way_back_only_to_somewhere_else() {
    let page = page_in("en").without_switcher();
    let html = render_language(&page, Some("/handbook/Home")).unwrap();
    assert!(
        html.contains("<a href=\"/handbook/Home\">Go back to the page you were on</a>"),
        "{html}"
    );
    assert!(
        html.contains("name=\"next\" value=\"/handbook/Home\""),
        "{html}"
    );
    let html = render_language(&page, Some("/-/lang?next=/x")).unwrap();
    assert!(!html.contains("Go back to the page you were on"), "{html}");
}

#[test]
fn a_back_address_that_is_not_local_is_replaced_by_the_start() {
    for bad in [
        "//evil.example",
        "https://evil.example/",
        "javascript:x",
        "/a b",
        "",
    ] {
        let page = Page::new(T::new(EN), bad);
        let html = render_not_found(&page).unwrap();
        assert!(
            html.contains("name=\"next\" value=\"/\""),
            "{bad:?}: {html}"
        );
    }
    // Escaped on its way into the page, whatever it holds.
    let page = Page::new(T::new(EN), "/a?b=1&c=2");
    assert!(
        render_not_found(&page)
            .unwrap()
            .contains("value=\"/a?b=1&#38;c=2\"")
    );
}

#[test]
fn no_placeholder_is_left_unfilled_and_no_key_is_missing_in_a_real_language() {
    for tag in ["en", "ja"] {
        let page = page_in(tag);
        for html in [
            render_not_found(&page).unwrap(),
            render_language(&page.clone().without_switcher(), Some("/x")).unwrap(),
            render_language_refused(&page).unwrap(),
            render_form_refused(&page).unwrap(),
        ] {
            assert!(!html.contains('{') && !html.contains('}'), "{tag}: {html}");
            assert!(
                !html.contains("[title.") && !html.contains(".heading]"),
                "{tag}: {html}"
            );
            for node in visible_text(&html) {
                assert!(
                    !(node.starts_with('[') && node.ends_with(']')),
                    "{tag}: a missing-key marker {node:?}"
                );
            }
        }
    }
}

#[test]
fn every_page_links_the_one_stylesheet_the_site_serves_and_nothing_else_is_loaded() {
    let page = page_in("en");
    for html in [
        render_not_found(&page).unwrap(),
        render_language(&page.clone().without_switcher(), None).unwrap(),
        render_language_refused(&page).unwrap(),
        render_form_refused(&page).unwrap(),
    ] {
        let link = format!("<link rel=\"stylesheet\" href=\"{}\">", style::path());
        assert_eq!(html.matches("<link").count(), 1, "{html}");
        assert!(html.contains(&link), "{html}");
        for loads in [
            "<img", "<script", "<iframe", "<object", "<embed", "@import", "src=",
        ] {
            assert!(!html.contains(loads), "{loads}: {html}");
        }
    }
}

#[test]
fn the_page_title_follows_the_pattern_and_names_the_page() {
    let en = page_in("en");
    assert_eq!(en.title(Key::TitleNotFound), "Not found · inklkunn");
    assert_eq!(en.title(Key::TitleLanguage), "Language · inklkunn");
    let ja = page_in("ja");
    assert_eq!(ja.title(Key::TitleNotFound), "見つかりません · inklkunn");
}
