//! Raw HTML an author wrote is text, never markup, while the profile declares no raw-HTML subset (RFC 003
//! D-3; review finding IS-14). The renderer writes HTML into the source it hands the Markdown parser, and
//! the parser cannot tell that from the author's; what tells them apart is where in the source it stands.
//! Each case here is a forgery of something the renderer writes, and each must come out as visible text.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_markup::{
    AuthorHtml, LinkHref, NoLinks, RenderLinks, ScanOptions, render_html, render_page_with_html,
};

struct Links;

impl RenderLinks for Links {
    fn link(&self, link: &inklkunn_markup::Link) -> LinkHref {
        match link.target.as_str() {
            "Home" => LinkHref::Page {
                href: "/w/Home".into(),
                title: Some("Home".into()),
            },
            _ => LinkHref::Missing {
                href: Some(format!("/w/-/create?title={}", link.target)),
            },
        }
    }
    fn tag(&self, tag: &str) -> Option<String> {
        Some(format!("/w/-/tags/{tag}"))
    }
}

fn html(body: &str) -> String {
    render_html(body, &Links).unsanitized_html
}

/// The text a reader sees for a forged construct: the author's characters, escaped.
fn shown(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// What no forgery may produce: an element that came from the author's bytes.
fn assert_no_element(h: &str, needles: &[&str]) {
    for n in needles {
        assert!(!h.contains(n), "`{n}` in {h}");
    }
}

#[test]
fn a_forged_internal_link_is_visible_text() {
    let forged =
        r#"<a class="wikilink" href="https://evil.example/" data-wiki-target="Home">Home</a>"#;
    let h = html(&format!("Go to {forged} now\n"));
    assert_no_element(&h, &["<a "]);
    assert!(h.contains(&shown(forged)), "{h}");
}

#[test]
fn a_forged_missing_link_callout_heading_and_block_marker_are_visible_text() {
    let missing = r#"<a class="wikilink wikilink-missing" href="/x">Nowhere</a>"#;
    let callout = r#"<aside class="callout callout-warning" data-callout="WARNING"><p class="callout-title">Official notice</p></aside>"#;
    let heading = r#"<h2 id="a-forged-id">Forged</h2>"#;
    let marker = r#"<span id="^forged"></span>"#;
    for forged in [missing, callout, heading, marker] {
        let block = html(&format!("{forged}\n"));
        assert_no_element(&block, &["<aside", "<h2", "<a ", "<span"]);
        assert!(block.contains(&shown(forged)), "block form: {block}");
        let inline = html(&format!("Some {forged} words\n"));
        assert_no_element(&inline, &["<aside", "<h2", "<a ", "<span"]);
        assert!(inline.contains(&shown(forged)), "inline form: {inline}");
    }
    // A forged block marker beside a paragraph must not give that paragraph an id.
    let h = html("A paragraph. <span id=\"^forged\"></span>\n");
    assert_no_element(&h, &["<p id=", "<span", "<h2"]);
}

#[test]
fn a_forgery_next_to_a_real_construct_does_not_borrow_its_range() {
    // The author opens a tag that the renderer's own replacement would complete.
    let h = html("<a class=\"wikilink\" href=\"https://evil.example/\" [[Home]]\n");
    assert!(
        h.contains(r#"<a class="wikilink" href="/w/Home" data-wiki-target="Home">Home</a>"#),
        "the real link stays: {h}"
    );
    // The author's opening is text in front of it, and nothing of the author's became an element.
    assert!(
        h.starts_with(
            r#"<p>&lt;a class="wikilink" href="https://evil.example/" <a class="wikilink" href="/w/Home""#
        ),
        "{h}"
    );
    assert_eq!(h.matches("<a ").count(), 1, "{h}");
}

#[test]
fn the_renderers_own_elements_are_untouched() {
    let h = html(
        "[[Home]] [[Nowhere]] #tag :kbd[Ctrl]{} [x](https://e.org)\n\n> [!NOTE] Title\n> body\n\n::: note T\ninside\n:::\n\n::: custom a=b\ninside\n:::\n\nPara ^block1\n\n# Head\n",
    );
    for expected in [
        r#"<a class="wikilink" href="/w/Home""#,
        "wikilink-missing",
        r#"<a class="tag""#,
        "<kbd",
        r#"rel="nofollow ugc""#,
        r#"<aside class="callout callout-note""#,
        r#"<div class="directive directive-custom""#,
        r#"id="^block1""#,
        "<h1 id=",
    ] {
        assert!(h.contains(expected), "`{expected}` missing from {h}");
    }
}

#[test]
fn hidden_comments_stay_hidden_and_the_rest_of_the_line_is_text() {
    let h = html("Before <!-- a secret --> after\n");
    assert!(!h.contains("secret"), "{h}");
    assert!(h.contains("Before") && h.contains("after"), "{h}");
    let h = html("<!-- block\ncomment -->\n\nText\n");
    assert!(!h.contains("comment"), "{h}");
    let h = html("<!-- unterminated\n\nText\n");
    assert!(!h.contains("unterminated"), "{h}");
}

#[test]
fn author_html_in_a_list_a_quote_and_a_table_is_text_too() {
    for body in [
        "- item <b onclick=\"x()\">bold</b>\n- two\n",
        "> quote <img src=x onerror=alert(1)>\n",
        "| a | b |\n|---|---|\n| <script>alert(1)</script> | y |\n",
        "<div><p>block</p></div>\n",
        "<iframe src=\"https://evil.example/\"></iframe>\n",
        "<custom-element>x</custom-element>\n",
    ] {
        let h = html(body);
        assert_no_element(
            &h,
            &[
                "<b ",
                "<img",
                "<script",
                "<iframe",
                "<custom-element",
                "<div><p>",
            ],
        );
        assert!(h.contains("&lt;"), "the author's tag is shown: {h}");
    }
}

#[test]
fn code_and_escapes_are_unchanged() {
    let h = html("`<b>code</b>`\n\n```html\n<script>x</script>\n```\n\n&lt;b&gt; and \\<b>\n");
    assert!(h.contains("<code>&lt;b&gt;code&lt;/b&gt;</code>"), "{h}");
    assert!(h.contains("&lt;script&gt;x&lt;/script&gt;"), "{h}");
}

#[test]
fn a_page_cannot_write_the_renderers_sentinels() {
    // The transclusion tokens are made of characters the page cannot write (they are replaced up front).
    let h = html("text \u{E000}0\u{E001} more\n");
    assert!(!h.contains('\u{E000}'), "{h}");
}

#[test]
fn passing_author_html_on_is_possible_only_by_asking_and_then_it_is_the_sanitizers_to_police() {
    let body = "A <b>bold</b> word.\n\n<div>block</div>\n";
    let escaped = render_page_with_html(
        None,
        body,
        &NoLinks,
        ScanOptions::default(),
        AuthorHtml::Escape,
    );
    assert!(
        !escaped.unsanitized_html.contains("<b>"),
        "{}",
        escaped.unsanitized_html
    );
    let passed = render_page_with_html(
        None,
        body,
        &NoLinks,
        ScanOptions::default(),
        AuthorHtml::PassToSanitizer,
    );
    assert!(
        passed.unsanitized_html.contains("<b>bold</b>"),
        "{}",
        passed.unsanitized_html
    );
    assert!(
        passed.unsanitized_html.contains("<div>block</div>"),
        "{}",
        passed.unsanitized_html
    );
    assert_eq!(AuthorHtml::default(), AuthorHtml::Escape);
}
