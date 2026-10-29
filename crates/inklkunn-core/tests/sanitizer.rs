//! The sanitizer and the renderer agree (handoff 003 T3; RFC 003 D-3): everything `inklkunn-markup` writes for
//! every construct and for every conformance case passes the policy untouched (so no reader sees a tag the
//! renderer wrote stripped), and the one attack surface the renderer opens, the author's own HTML, is closed
//! by that same policy. Needs no prikk.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use inklkunn_core::Sanitizer;
use inklkunn_markup::scan::{Link, ScanOptions};
use inklkunn_markup::{LinkHref, Page, RenderLinks, Transclusion, render_html, render_page};

/// `(element, attribute names)` of every opening tag, in order; comments are skipped. A scan for the output of
/// an HTML serializer (values quoted and escaped), not a parser.
fn signature(html: &str) -> Vec<(String, Vec<(String, String)>)> {
    let b = html.as_bytes();
    let at = |i: usize| b.get(i).copied();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if at(i) != Some(b'<') {
            i += 1;
            continue;
        }
        if html.get(i..).is_some_and(|r| r.starts_with("<!--")) {
            i = html
                .get(i..)
                .and_then(|r| r.find("-->"))
                .map_or(b.len(), |e| i + e + 3);
            continue;
        }
        let start = i + 1;
        if !at(start).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let mut j = start;
        while at(j).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'-') {
            j += 1;
        }
        let name = html.get(start..j).unwrap_or("").to_ascii_lowercase();
        let mut attrs = Vec::new();
        loop {
            while at(j).is_some_and(|c| c.is_ascii_whitespace() || c == b'/') {
                j += 1;
            }
            if j >= b.len() || at(j) == Some(b'>') {
                break;
            }
            let s = j;
            while at(j)
                .is_some_and(|c| !c.is_ascii_whitespace() && !matches!(c, b'=' | b'>' | b'/'))
            {
                j += 1;
            }
            let attr = html.get(s..j).unwrap_or("").to_ascii_lowercase();
            let mut value = String::new();
            if at(j) == Some(b'=') {
                j += 1;
                if let Some(q) = at(j).filter(|q| matches!(q, b'"' | b'\'')) {
                    j += 1;
                    let v = j;
                    while at(j).is_some_and(|c| c != q) {
                        j += 1;
                    }
                    value = html.get(v..j).unwrap_or("").to_owned();
                    j += 1;
                } else {
                    let v = j;
                    while at(j).is_some_and(|c| !c.is_ascii_whitespace() && c != b'>') {
                        j += 1;
                    }
                    value = html.get(v..j).unwrap_or("").to_owned();
                }
            }
            attrs.push((attr, value));
        }
        attrs.sort();
        out.push((name, attrs));
        i = j + 1;
    }
    out
}

/// A wiki with some of everything a link can lead to.
struct Rich;

impl RenderLinks for Rich {
    fn link(&self, link: &Link) -> LinkHref {
        let t = link.target.as_str();
        if t.ends_with(".png") || t.ends_with(".pdf") {
            LinkHref::Attachment(format!("/att/{t}"))
        } else if t.starts_with("wp:") {
            LinkHref::External("https://en.wikipedia.org/wiki/Wiki".into())
        } else if t.contains("Missing") {
            LinkHref::Missing {
                href: Some(format!("/new/{t}")),
            }
        } else if t.contains("Hidden") {
            LinkHref::Missing { href: None }
        } else {
            LinkHref::Page {
                href: format!("/w/{t}"),
                title: Some(t.to_owned()),
            }
        }
    }

    fn tag(&self, tag: &str) -> Option<String> {
        Some(format!("/tags/{tag}"))
    }

    fn transclusion(&self, link: &Link) -> Option<Transclusion> {
        link.target.starts_with("Included").then(|| Transclusion {
            path: format!("pages/{}.md", link.target),
            title: link.target.clone(),
            href: format!("/w/{}", link.target),
            label: format!("From {}", link.target),
        })
    }

    fn transclusion_body(&self, _: &Transclusion) -> Option<String> {
        Some("# Inner\n\nInner *text* ^inner\n\n![[Included deeper]]\n".to_owned())
    }
}

/// The rendered HTML as the sanitizer should leave it: the one attribute the Markdown writer emits that no
/// policy keeps (a column's alignment as an inline style) is not expected back.
fn expected(rendered: &str) -> Vec<(String, Vec<(String, String)>)> {
    signature(rendered)
        .into_iter()
        .map(|(name, attrs)| {
            let attrs = attrs
                .into_iter()
                .filter(|(a, _)| !(matches!(name.as_str(), "td" | "th") && a == "style"))
                // The task-list checkbox's attributes have their values forced, never their names.
                .collect();
            (name, attrs)
        })
        .collect()
}

const EVERYTHING: &str = "# Title with `code`\n\n\
Setext heading ^setext\n======================\n\n\
Paragraph with *emphasis*, **strong**, ~~strike~~, `code`, a [local](./x.md) link, an \
[external](https://example.org \"tip\") link, <https://example.org/auto>, a footnote[^1], a tag #topic, a block id. ^para\n\n\
[[Page]] [[Page|label]] [[Page#Heading]] [[Page#^block]] [[#Here]] [[Missing]] [[Hidden]] [[wp:Wiki]] ![[diagram.png|300]] ![[manual.pdf]]\n\n\
![[Included one]]\n\n- bullet ^b1\n- [ ] open task\n- [x] done task\n  1. nested ordered\n\n3. third\n\n\
> quote ^q1\n\n> [!NOTE]\n> Plain callout ^c1\n\n> [!tip]- Folded callout\n> With a **title**\n\n\
:::warning Heads up\nBody of the container.\n:::\n\n::: toc depth=2\n:::\n\n\
Press :kbd[Ctrl]{} and :ref[thing]{id=1} now.\n\n\
| a | b |\n|:--|--:|\n| 1 | 2 |\n\n| c |\n|---|\n| 3 |\n^table\n\n\
```rust\nfn main() {}\n```\n^code\n\n    indented code\n\n---\n\n[^1]: The footnote.\n";

#[test]
fn what_the_renderer_writes_for_every_construct_passes_the_policy_untouched() {
    let sanitizer = Sanitizer::profile();
    let page = Page::parse(EVERYTHING).expect("parses");
    let rendered = render_page(
        Some(("pages/Everything.md", "Everything")),
        &page.body,
        &Rich,
        ScanOptions::default(),
    )
    .unsanitized_html;
    let clean = sanitizer.clean(&rendered);
    assert_eq!(
        signature(clean.as_str()),
        expected(&rendered),
        "an element or an attribute of the renderer's was stripped:\n{rendered}\n---\n{clean}"
    );
    // The text is untouched too: nothing the page said was lost with a tag.
    let words = |h: &str| -> Vec<String> {
        h.split(['<', '>'])
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .flat_map(|(_, t)| t.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
            .collect()
    };
    let decode = |s: &str| {
        s.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#x27;", "'")
    };
    assert_eq!(
        words(clean.as_str())
            .iter()
            .map(|w| decode(w))
            .collect::<Vec<_>>(),
        words(&rendered)
            .iter()
            .map(|w| decode(w))
            .collect::<Vec<_>>()
    );
}

#[test]
fn what_the_renderer_writes_for_every_conformance_case_passes_the_policy_untouched() {
    let sanitizer = Sanitizer::profile();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../inklkunn-markup/tests/corpus/markup");
    let mut cases = 0;
    for case in std::fs::read_dir(&dir)
        .expect("the markup crate's corpus")
        .flatten()
    {
        let input = case.path().join("input.md");
        let Ok(text) = std::fs::read_to_string(&input) else {
            continue;
        };
        cases += 1;
        let body = Page::parse(&text).map_or(text.clone(), |p| p.body);
        let rendered = render_html(&body, &Rich).unsanitized_html;
        let clean = sanitizer.clean(&rendered);
        assert_eq!(
            signature(clean.as_str()),
            expected(&rendered),
            "{}:\n{rendered}\n---\n{clean}",
            input.display()
        );
    }
    assert!(cases >= 20, "the corpus was found ({cases} cases)");
}

#[test]
fn the_authors_own_html_is_held_to_the_same_policy_as_everything_else() {
    // The sanitizer is the second line: even if a page's raw HTML were handed on (a wiki that declares a
    // subset), the policy decides. (By default the renderer escapes it first; the next test.)
    let sanitizer = Sanitizer::profile();
    let page = "Text <script>alert(1)</script> and <span class=\"tag\" onclick=\"alert(2)\">kept</span>.\n\n\
                <div class=\"site-header\" style=\"position:fixed\">Verified by prikk</div>\n\n\
                <a href=\"javascript:alert(3)\">js</a> <a href=\"https://example.org\">out</a>\n\n\
                <img src=\"https://tracker.example/p.gif\" alt=\"pixel\"> <iframe src=\"https://evil.example\"></iframe>\n";
    let rendered = inklkunn_markup::render_page_with_html(
        None,
        page,
        &Rich,
        inklkunn_markup::ScanOptions::default(),
        inklkunn_markup::AuthorHtml::PassToSanitizer,
    )
    .unsanitized_html;
    let clean = sanitizer.clean(&rendered);
    let out = clean.as_str();
    for gone in [
        "<script",
        "alert",
        "onclick",
        "site-header",
        "position",
        "javascript:",
        "tracker.example",
        "<iframe",
        "evil.example",
    ] {
        assert!(!out.contains(gone), "`{gone}` survived in {out}");
    }
    // What is the profile's stays, and the words of the author do too.
    for kept in [
        "<span class=\"tag\">kept</span>",
        "Verified by prikk",
        "href=\"https://example.org\"",
        "alt=\"pixel\"",
    ] {
        assert!(out.contains(kept), "`{kept}` lost from {out}");
    }
}

#[test]
fn by_default_the_authors_html_never_reaches_the_sanitizer_as_markup() {
    // The profile declares no raw-HTML subset, so everything the author spelled out is text a reader sees,
    // and nothing of it can be mistaken for what the renderer wrote (review finding IS-14).
    let page = "Text <script>alert(1)</script> and <span class=\"tag\" onclick=\"alert(2)\">kept</span>.\n\n\
                <div class=\"site-header\" style=\"position:fixed\">Verified by prikk</div>\n\n\
                <a class=\"wikilink\" href=\"https://evil.example/\">Home</a>\n";
    let rendered = render_html(page, &Rich).unsanitized_html;
    let clean = Sanitizer::profile().clean(&rendered);
    let out = clean.as_str();
    for element in ["<script", "<span", "<div", "<a ", "<a>"] {
        assert!(!out.contains(element), "`{element}` in {out}");
    }
    // The words are there, as text.
    for text in ["&lt;script&gt;", "Verified by prikk", "&lt;a class="] {
        assert!(out.contains(text), "`{text}` lost from {out}");
    }
}
