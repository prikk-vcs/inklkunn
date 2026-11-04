#![allow(clippy::expect_used, clippy::unwrap_used)]
use inklkunn_core::Sanitizer;

use super::*;
use crate::catalog::{EN, Lang};

fn ja() -> T {
    T::new(Lang::from_tag("ja").expect("the Japanese preview is compiled in"))
}

fn body(html: &str) -> SafeHtml {
    Sanitizer::profile().clean(html)
}

fn claim(block: &str, claim: &str) -> ParagraphClaim {
    ParagraphClaim {
        block: block.to_owned(),
        claim: claim.to_owned(),
    }
}

fn cut(html: &SafeHtml, claims: &[ParagraphClaim]) -> Vec<BodyPart> {
    parts(html, claims, T::new(EN), "handbook", None)
}

/// What the parts print with the links left out: always the body, byte for byte.
fn printed(parts: &[BodyPart]) -> String {
    parts.iter().map(|p| p.html.to_string()).collect()
}

const C1: &str = "01930000-0000-7000-8000-000000000001";
const C2: &str = "01930000-0000-7000-8000-000000000002";

#[test]
fn a_page_with_no_claims_is_one_part_that_is_the_whole_body() {
    let html = body("<p id=\"h-^b1\">Said.</p>\n<p>Other.</p>\n");
    for none in [&[][..], &[claim("elsewhere", C1)][..]] {
        let cut = cut(&html, none);
        assert_eq!(cut.len(), 1);
        assert_eq!(printed(&cut), html.as_str());
        assert!(cut.iter().all(|p| p.tools.is_empty()));
    }
}

#[test]
fn a_claimed_paragraph_gets_one_link_inside_its_end_and_nothing_else_changes() {
    let html = body("<p id=\"h-^b1\">Said <em>so</em>.</p>\n<p id=\"h-^b2\">Not claimed.</p>\n");
    let parts = cut(&html, &[claim("b1", C1)]);
    assert_eq!(printed(&parts), html.as_str(), "no byte of the body moves");
    assert_eq!(parts.len(), 2);
    let first = parts.first().unwrap();
    // The first stretch ends just before the paragraph's closing tag: the link goes inside it.
    assert_eq!(first.html.to_string(), "<p id=\"h-^b1\">Said <em>so</em>.");
    assert_eq!(first.tools.len(), 1);
    let tool = first.tools.first().unwrap();
    assert_eq!(tool.href, format!("/handbook/-/claim/{C1}"));
    assert_eq!(tool.text, "Sources and views");
    assert_eq!(
        tool.name,
        "Sources and views: paragraph beginning “Said so.”"
    );
    assert!(parts.get(1).unwrap().tools.is_empty());
}

#[test]
fn the_link_stays_in_the_view_the_page_is_read_in() {
    let html = body("<p id=\"h-^b1\">Said.</p>\n");
    let parts = parts(
        &html,
        &[claim("b1", C1)],
        T::new(EN),
        "handbook",
        Some("draft"),
    );
    assert_eq!(
        parts.first().unwrap().tools.first().unwrap().href,
        format!("/handbook/-/claim/{C1}?view=draft")
    );
}

#[test]
fn a_list_item_is_closed_by_its_own_closing_tag_not_by_one_nested_inside_it() {
    let html = body(
        "<ul>\n<li id=\"h-^outer\">Outer<ul>\n<li id=\"h-^inner\">Inner</li>\n</ul>\n</li>\n</ul>\n",
    );
    let parts = cut(&html, &[claim("outer", C1), claim("inner", C2)]);
    assert_eq!(printed(&parts), html.as_str());
    assert_eq!(parts.len(), 3);
    // The inner item's link is inside the inner item; the outer's after the nested list, inside the outer.
    let htmls: Vec<String> = parts.iter().map(|p| p.html.to_string()).collect();
    assert!(
        htmls
            .first()
            .unwrap()
            .ends_with("<li id=\"h-^inner\">Inner"),
        "{htmls:?}"
    );
    assert!(
        htmls.get(1).unwrap().ends_with("</li>\n</ul>\n"),
        "{htmls:?}"
    );
    assert!(
        parts
            .first()
            .unwrap()
            .tools
            .first()
            .unwrap()
            .href
            .ends_with(C2)
    );
    assert!(
        parts
            .get(1)
            .unwrap()
            .tools
            .first()
            .unwrap()
            .href
            .ends_with(C1)
    );
}

#[test]
fn a_paragraph_that_holds_several_claims_has_one_link_for_each_named_by_its_number() {
    let html = body("<p id=\"h-^b1\">Said.</p>\n");
    let parts = cut(&html, &[claim("b1", C1), claim("b1", C2)]);
    let tools = &parts.first().unwrap().tools;
    assert_eq!(tools.len(), 2);
    assert_eq!(
        tools.first().unwrap().name,
        "Sources and views, claim 1 of 2: paragraph beginning “Said.”"
    );
    assert_eq!(
        tools.get(1).unwrap().name,
        "Sources and views, claim 2 of 2: paragraph beginning “Said.”"
    );
    assert!(tools.first().unwrap().href.ends_with(C1));
    assert!(tools.get(1).unwrap().href.ends_with(C2));
}

#[test]
fn the_name_of_each_link_is_distinct_even_for_paragraphs_that_begin_alike() {
    let html = body(
        "<p id=\"h-^a\">Same words.</p>\n<p id=\"h-^b\">Same words.</p>\n<p id=\"h-^c\">Other.</p>\n",
    );
    let parts = cut(&html, &[claim("a", C1), claim("b", C2), claim("c", C1)]);
    let names: Vec<&str> = parts
        .iter()
        .flat_map(|p| p.tools.iter().map(|t| t.name.as_str()))
        .collect();
    assert_eq!(names.len(), 3);
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 3, "{names:?}");
    // Every name begins with the words the link shows (a screen reader's name contains the visible label).
    assert!(names.iter().all(|n| n.starts_with("Sources and views")));
}

#[test]
fn a_paragraph_with_no_words_is_named_so() {
    let html = body("<p id=\"h-^b1\"><img src=\"missing.png\" alt=\"\"></p>\n");
    let parts = cut(&html, &[claim("b1", C1)]);
    assert_eq!(
        parts.first().unwrap().tools.first().unwrap().name,
        "Sources and views: paragraph without text"
    );
}

#[test]
fn the_name_is_in_the_readers_language() {
    let html = body("<p id=\"h-^b1\">約束の言葉。</p>\n");
    let parts = parts(&html, &[claim("b1", C1)], ja(), "handbook", None);
    let tool = parts.first().unwrap().tools.first().unwrap();
    assert_eq!(tool.text, "出典とビュー");
    assert_eq!(tool.name, "出典とビュー: 「約束の言葉。」で始まる段落");
}

#[test]
fn a_block_id_written_twice_is_ambiguous_and_is_offered_no_link() {
    let html =
        body("<p id=\"h-^b1\">One.</p>\n<p id=\"h-^b1\">Two.</p>\n<p id=\"h-^b2\">Three.</p>\n");
    let parts = cut(&html, &[claim("b1", C1), claim("b2", C2)]);
    assert_eq!(printed(&parts), html.as_str());
    let linked: Vec<&str> = parts
        .iter()
        .flat_map(|p| p.tools.iter().map(|t| t.href.as_str()))
        .collect();
    assert_eq!(linked, [format!("/handbook/-/claim/{C2}")]);
}

#[test]
fn words_that_look_like_a_paragraph_are_text_and_find_nothing() {
    // The sanitizer's text is escaped: a page cannot write the opening tag of a claimed paragraph in its words.
    let html = body("<p id=\"h-^b1\">see &lt;p id=\"h-^b2\"&gt; here</p>\n");
    let parts = cut(&html, &[claim("b2", C1)]);
    assert!(parts.iter().all(|p| p.tools.is_empty()));
    assert_eq!(printed(&parts), html.as_str());
}

#[test]
fn a_block_that_is_not_a_paragraph_or_a_list_item_is_offered_no_link() {
    let html =
        body("<blockquote id=\"h-^q\">\n<p>Quoted.</p>\n</blockquote>\n<h2 id=\"h\">Head</h2>\n");
    let parts = cut(&html, &[claim("q", C1)]);
    assert!(parts.iter().all(|p| p.tools.is_empty()));
    assert_eq!(printed(&parts), html.as_str());
}

#[test]
fn the_beginning_is_plain_text_of_a_bounded_length() {
    assert_eq!(beginning("Said <em>so</em> &amp; more"), "Said so & more");
    assert_eq!(beginning("  a\n\n   b\t c "), "a b c");
    assert_eq!(
        beginning("x &lt;y&gt; &quot;z&quot; it&#39;s"),
        "x <y> \"z\" it's"
    );
    assert_eq!(beginning(""), "");
    let long = "a".repeat(START_CHARS + 1);
    assert_eq!(beginning(&long), format!("{}…", "a".repeat(START_CHARS)));
    let exact = "a".repeat(START_CHARS);
    assert_eq!(beginning(&exact), exact);
    // Characters, not bytes: Japanese is cut between characters.
    let wide = "あ".repeat(START_CHARS + 5);
    assert_eq!(beginning(&wide), format!("{}…", "あ".repeat(START_CHARS)));
}

#[test]
fn a_name_is_made_distinct_by_a_count_from_the_second_use() {
    let mut used = HashMap::new();
    assert_eq!(distinct(&mut used, "a".to_owned()), "a");
    assert_eq!(distinct(&mut used, "b".to_owned()), "b");
    assert_eq!(distinct(&mut used, "a".to_owned()), "a (2)");
    assert_eq!(distinct(&mut used, "a".to_owned()), "a (3)");
}

#[test]
fn an_element_that_is_never_closed_has_no_end_to_put_a_link_at() {
    assert_eq!(closing("<p id=\"h-^b\">open", 12, "p"), None);
    assert_eq!(closing("abc</p>", 0, "p"), Some(3));
    // `<pre>` is not a paragraph inside a paragraph.
    assert_eq!(closing("a<pre>x</pre>b</p>", 0, "p"), Some(14));
}
