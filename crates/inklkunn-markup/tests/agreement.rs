//! The scanner, the renderer and the page agree with each other and with CommonMark (the 2026-10-27
//! review's findings M7–M15), and the HTML the renderer writes is what a sanitizer expects to see.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_markup::{
    Index, LinkHref, LintContext, NoLinks, Page, PageEntry, RenderLinks, lint, render_html, scan,
};

struct Links;

impl RenderLinks for Links {
    fn link(&self, link: &inklkunn_markup::Link) -> LinkHref {
        match link.target.as_str() {
            "Exists" => LinkHref::Page {
                href: "/wiki/Exists".into(),
                title: Some("Exists".into()),
            },
            "Evil" => LinkHref::Page {
                href: "javascript:alert(1)".into(),
                title: None,
            },
            "Data" => LinkHref::External("data:text/html,x".into()),
            t if t.ends_with(".png") => LinkHref::Attachment(format!("/att/{t}")),
            _ => LinkHref::Missing { href: None },
        }
    }
    fn tag(&self, tag: &str) -> Option<String> {
        Some(format!("/tags/{tag}"))
    }
}

#[test]
fn the_metadata_alone_is_the_pages_metadata() {
    for text in [
        "---\ntitle: T\naliases: [a, b]\nredirect: Target\nid: 019a0000-0000-7000-8000-000000000001\n---\n\nBody [[X]] ^id\n",
        "no frontmatter at all\n",
        "---\n---\nempty frontmatter\n",
        "---\ntitle: \"Quoted: yes\"\nkind: redirect\n...\nafter\n",
    ] {
        let alone = inklkunn_markup::parse_metadata(text).expect("metadata");
        let page = Page::parse(text).expect("page");
        assert_eq!(alone, page.metadata, "{text:?}");
    }
    // The same refusal as the page's: outside the plain subset.
    let bad = "---\ntitle: &anchor x\n---\nbody\n";
    assert_eq!(
        inklkunn_markup::parse_metadata(bad).err(),
        Page::parse(bad).err()
    );
    assert!(inklkunn_markup::parse_metadata(bad).is_err());
}

#[test]
fn a_page_scan_is_the_scan() {
    let text = "---\ntitle: T\ntags: [tag, other]\n---\n\n#tag here [[L]] ^b1\n\n## H\n";
    let page = Page::parse(text).expect("page");
    assert_eq!(page.scan, scan(text));
    assert_eq!(page.scan.tags.frontmatter, vec!["tag", "other"]);
    assert!(page.scan.frontmatter.is_some());
    // And so a declared tag is not reported as undeclared (MKUP-14).
    let codes: Vec<&str> = lint(&page, text, LintContext::default())
        .iter()
        .map(|f| f.code)
        .collect();
    assert!(!codes.contains(&"MKUP-14-tags"), "{codes:?}");
    assert_eq!(page.metadata.tags, vec!["tag", "other"]);
    // Tags written as one string split the same way in both places.
    let page = Page::parse("---\ntags: a b, c\n---\n").expect("page");
    assert_eq!(page.metadata.tags, vec!["a", "b", "c"]);
    assert_eq!(page.scan.tags.frontmatter, vec!["a", "b", "c"]);
}

#[test]
fn scan_anchors_are_the_rendered_ids_including_setext_and_duplicates() {
    let body = "# _em_\n\n## A <b>b</b>\n\n## [[T#Sec]]\n\n## ![[i.png]]\n\n## T <!-- c -->\n\nSetext\n------\n\n## a\n\n## a-1\n\n## a\n";
    let report = scan(body);
    let rendered = render_html(body, &Links);
    let h = &rendered.unsanitized_html;
    for heading in &report.headings {
        assert!(
            h.contains(&format!("id=\"{}\"", heading.anchor)),
            "heading `{}` anchor `{}` not in {h}",
            heading.text,
            heading.anchor
        );
    }
    // Every id is unique, the setext heading included.
    let ids: Vec<&str> = rendered.toc.iter().map(|t| t.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "{ids:?}");
    assert!(ids.contains(&"setext"), "{ids:?}");
    assert_eq!(report.headings.len() + 1, rendered.toc.len());
    // GitHub's duplicate rule: a, a-1, a → a, a-1, a-2.
    let tail: Vec<&str> = ids.iter().rev().take(3).rev().copied().collect();
    assert_eq!(tail, ["a", "a-1", "a-2"]);
    // A fragment link lands on the heading's id.
    let r = render_html(
        "## Edit Conflicts\n\n[[#Edit Conflicts]] and [[Exists#Edit Conflicts]]",
        &Links,
    );
    assert!(
        r.unsanitized_html.contains(r##"href="#edit-conflicts""##),
        "{}",
        r.unsanitized_html
    );
    assert!(
        r.unsanitized_html
            .contains(r##"href="/wiki/Exists#edit-conflicts""##)
    );
    assert!(r.unsanitized_html.contains(r#"<h2 id="edit-conflicts">"#));
}

#[test]
fn code_regions_are_commonmarks() {
    // Indented code, a multi-line code span, a longer fence closed only by an equal or longer one, a
    // tag inside code: none of them is Layer 2 syntax.
    let body = "    [[Indented]]\n\ntext `a\n[[span]]` more\n\n````\n```\n[[in-fence]]\n````\n\n`x #a` and #b\n";
    let report = scan(body);
    assert!(report.links.is_empty(), "{:?}", report.links);
    assert_eq!(report.tags.inline, vec!["b"]);
    let h = render_html(body, &Links).unsanitized_html;
    assert!(!h.contains("wikilink"), "{h}");
    assert!(h.contains("[[Indented]]"));
    assert!(h.contains("[[span]]"));
    assert!(h.contains("[[in-fence]]"));
    assert!(h.contains("<code>x #a</code>"), "{h}");
    assert!(
        h.contains(r#"<a class="tag" href="/tags/b" data-tag="b">#b</a>"#),
        "{h}"
    );
}

#[test]
fn constructs_are_replaced_at_their_own_spans_not_by_text_search() {
    // The first `#a` is inside a Markdown link (not a tag); only the second is.
    let h = render_html("[see](#a) and #a", &Links).unsanitized_html;
    assert!(h.contains(r##"<a href="#a">see</a>"##), "{h}");
    assert_eq!(h.matches("class=\"tag\"").count(), 1, "{h}");
    // An inline directive inside code stays code; the real one renders.
    let h = render_html("`:kbd[Ctrl]{}` then :kbd[Ctrl]{}", &Links).unsanitized_html;
    assert!(h.contains("<code>:kbd[Ctrl]{}</code>"), "{h}");
    assert!(h.contains("<kbd>Ctrl</kbd>"), "{h}");
    // A block id after a code span that contains a caret.
    let h = render_html("Para `x^y` ^id\n", &Links).unsanitized_html;
    assert!(h.contains("<code>x^y</code>"), "{h}");
    assert!(
        h.contains(r#"<p id="^id">Para <code>x^y</code></p>"#),
        "{h}"
    );
}

#[test]
fn containers_in_comments_and_fences_in_frontmatter_are_not_structure() {
    let h = render_html("<!--\n::: note\n-->\n\ntext", &Links).unsanitized_html;
    assert!(!h.contains("<aside"), "{h}");
    assert!(h.contains("<p>text</p>"), "{h}");
    // A fence-looking line inside the frontmatter does not hide the body from the scanner.
    let text = "---\ndescription: |\n  ```\n---\n[[Link]] #tag\n";
    let r = scan(text);
    assert_eq!(r.links.len(), 1, "{r:?}");
    assert_eq!(r.tags.inline, vec!["tag"]);
    assert!(Page::parse(text).is_ok());
    // A byte-order mark does not hide the frontmatter.
    let page = Page::parse("\u{feff}---\ntitle: T\n---\nbody\n").expect("page");
    assert_eq!(page.metadata.title.as_deref(), Some("T"));
}

#[test]
fn hrefs_with_dangerous_schemes_are_never_emitted_as_links() {
    let h = render_html(
        "[x](javascript:alert(1)) ![i](data:text/html,x) [[Evil]] [[Data]] [ok](https://e.org) [rel](./x.md) [m](mailto:a@b)",
        &Links,
    )
    .unsanitized_html;
    assert!(!h.contains("javascript:"), "{h}");
    assert!(!h.contains("data:"), "{h}");
    assert!(h.contains(r#"<span class="link-blocked">x</span>"#), "{h}");
    assert!(h.contains(r#"<span class="image-blocked">i</span>"#), "{h}");
    assert!(
        h.contains(
            r#"<span class="wikilink wikilink-blocked" data-wiki-target="Evil">Evil</span>"#
        ),
        "{h}"
    );
    assert!(
        h.contains(
            r#"<span class="wikilink wikilink-blocked" data-wiki-target="Data">Data</span>"#
        ),
        "{h}"
    );
    assert!(
        h.contains(r#"<a class="external" href="https://e.org" rel="nofollow ugc">ok</a>"#),
        "{h}"
    );
    assert!(h.contains(r#"<a href="./x.md">rel</a>"#), "{h}");
    assert!(h.contains(r#"<a href="mailto:a@b">m</a>"#), "{h}");
    // The scheme check is case-insensitive for the external marking too.
    let h = render_html("[z](HTTP://e.com)", &Links).unsanitized_html;
    assert!(h.contains(r#"rel="nofollow ugc""#), "{h}");
    // Raw HTML passes through unsanitized, and the field name says so: the web layer's sanitizer is the
    // contract (RFC 003 D-3). This pins the behaviour so a change is visible.
    let h = render_html("<script>alert(1)</script>", &Links).unsanitized_html;
    assert!(h.contains("<script>"), "{h}");
}

#[test]
fn attribute_values_are_escaped_everywhere() {
    let h = render_html(
        "[[A\"B|l<i>]] #tag :d[x<y]{k=\"v&quot;w\"}\n\n> [!NOTE] ti<tle\n> b\n\n:::note He\"ad\n:::\n",
        &Links,
    )
    .unsanitized_html;
    assert!(!h.contains("A\"B"), "{h}");
    assert!(h.contains("data-wiki-target=\"A&quot;B\""), "{h}");
    assert!(h.contains("l&lt;i&gt;"), "{h}");
    assert!(h.contains("data-attr-k=\"v&quot;w\""), "{h}");
    assert!(h.contains("x&lt;y</span>"), "{h}");
    assert!(h.contains("ti&lt;tle</p>"), "{h}");
    assert!(h.contains("He&quot;ad"), "{h}");
    assert!(!h.contains("<i>"), "{h}");
}

#[test]
fn embeds_callouts_and_multibyte_crlf_render_as_expected() {
    // An embed alone on a line does not swallow the lines after it.
    let h = render_html("![[photo.png]]\nCaption *em* a < b\n", &Links).unsanitized_html;
    assert!(h.contains("<em>em</em>"), "{h}");
    assert!(h.contains("a &lt; b"), "{h}");
    assert!(h.contains(r#"<span class="embed"><img class="wikilink-embed" src="/att/photo.png" alt="photo.png"></span>"#), "{h}");
    // A callout marker followed by inline markup is still a callout, title markup kept.
    let h = render_html("> [!NOTE] with `code` and *em*\n> body\n", &Links).unsanitized_html;
    assert!(
        h.contains(r#"<aside class="callout callout-note" data-callout="NOTE">"#),
        "{h}"
    );
    assert!(
        h.contains(r#"<p class="callout-title">with <code>code</code> and <em>em</em></p>"#),
        "{h}"
    );
    assert!(h.contains("<p>body</p>"), "{h}");
    // CRLF and multibyte text through every pass.
    let text = "---\r\ntitle: 編集\r\n---\r\n\r\n## 編集の競合 ^見出し\r\n\r\n日本語 [[編集|リンク]] #タグ ^ブロック\r\n\r\n:::note 注意\r\n中身\r\n:::\r\n";
    let r = scan(text);
    assert_eq!(r.headings.len(), 1);
    assert_eq!(r.links.len(), 1);
    assert_eq!(r.tags.inline, vec!["タグ"]);
    assert!(
        r.block_ids.is_empty(),
        "block ids are ASCII: {:?}",
        r.block_ids
    );
    let page = Page::parse(text).expect("page");
    let body = &page.body;
    let h = render_html(body, &Links).unsanitized_html;
    assert!(h.contains("data-wiki-target=\"編集\""), "{h}");
    assert!(h.contains("<p>中身</p>"), "{h}");
    assert!(h.contains(r#"<h2 id="編集の競合-見出し">"#), "{h}");
}

#[test]
fn resolution_reports_ambiguity_instead_of_hiding_it() {
    let mut ix = Index::new("");
    ix.add(PageEntry::new("A/X.md", "X"));
    ix.add(PageEntry::new("B/X.md", "X"));
    ix.add(PageEntry::new("Real.md", "Real"));
    let r = ix.resolve("X", Some("Home.md"));
    assert!(!r.resolved());
    assert_eq!(r.candidates.len(), 2);
    assert!(ix.resolve("Real", Some("Home.md")).resolved());
    assert!(
        ix.resolve("", Some("Home.md")).resolved(),
        "the page itself"
    );
    assert!(!ix.resolve("", None).resolved(), "no page to be itself");
    assert_eq!(
        ix.by_path("Real.md").map(|p| p.title.as_str()),
        Some("Real")
    );
    assert!(
        ix.by_path("Real").is_none(),
        "by_path takes the path with its extension"
    );
    ix.interwiki.insert("wp".into(), "https://w.org/".into());
    assert_eq!(
        ix.resolve("wp:A b", None).url.as_deref(),
        Some("https://w.org/A%20b"),
        "a template without {{title}} appends it"
    );
    let _ = NoLinks;
}
