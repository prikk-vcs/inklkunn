//! The renderer writes nothing its vocabulary does not name (handoff 003 T3; RFC 003 D-3). The sanitizer in
//! `inklkunn-core` allows exactly [`inklkunn_markup::render::vocabulary`] and the profile's raw-HTML subset
//! (empty today), so an element, an attribute or a class the renderer wrote that is not in the vocabulary
//! would reach a reader as a stripped tag. This test renders every construct and every conformance case and
//! fails first.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::Path;

use inklkunn_markup::render::vocabulary::{
    ELEMENTS, UNALLOWED_ATTRIBUTES, attribute_is_known, class_is_known,
};
use inklkunn_markup::scan::Link;
use inklkunn_markup::{
    LinkHref, Page, RenderLinks, Transclusion, TransclusionRefusal, render_html, render_page,
};

/// An opening tag found in rendered HTML.
#[derive(Debug)]
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
}

/// Every opening tag of `html`, by a scan that understands comments, quoted attribute values and entities
/// only as far as the renderer's own output needs (it escapes `<`, `>` and `"` in values).
fn tags(html: &str) -> Vec<Tag> {
    let b = html.as_bytes();
    let at = |i: usize| b.get(i).copied();
    let text = |from: usize, to: usize| html.get(from..to).unwrap_or("").to_owned();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if at(i) != Some(b'<') {
            i += 1;
            continue;
        }
        if html.get(i..).is_some_and(|rest| rest.starts_with("<!--")) {
            i = html
                .get(i..)
                .and_then(|rest| rest.find("-->"))
                .map_or(b.len(), |e| i + e + 3);
            continue;
        }
        let name_start = i + 1;
        if !at(name_start).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let mut j = name_start;
        while at(j).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'-') {
            j += 1;
        }
        let name = text(name_start, j).to_ascii_lowercase();
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
            let attr = text(s, j).to_ascii_lowercase();
            let mut value = String::new();
            if at(j) == Some(b'=') {
                j += 1;
                if let Some(q) = at(j).filter(|q| matches!(q, b'"' | b'\'')) {
                    j += 1;
                    let v = j;
                    while at(j).is_some_and(|c| c != q) {
                        j += 1;
                    }
                    value = text(v, j);
                    j += 1;
                } else {
                    let v = j;
                    while at(j).is_some_and(|c| !c.is_ascii_whitespace() && c != b'>') {
                        j += 1;
                    }
                    value = text(v, j);
                }
            }
            attrs.push((attr, value));
        }
        out.push(Tag { name, attrs });
        i = j + 1;
    }
    out
}

/// What is outside the vocabulary in `html`, as readable lines.
fn violations(html: &str) -> BTreeSet<String> {
    let mut v = BTreeSet::new();
    for t in tags(html) {
        if !ELEMENTS.contains(&t.name.as_str()) {
            v.insert(format!("element <{}>", t.name));
        }
        for (a, value) in &t.attrs {
            if UNALLOWED_ATTRIBUTES.contains(&(t.name.as_str(), a.as_str())) {
                continue;
            }
            if !attribute_is_known(&t.name, a) {
                v.insert(format!("attribute {a} on <{}>", t.name));
            }
            if a == "class" {
                for c in value.split_ascii_whitespace() {
                    if !class_is_known(c) {
                        v.insert(format!("class {c} on <{}>", t.name));
                    }
                }
            }
        }
    }
    v
}

/// A wiki that has some of everything a link can lead to.
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
        } else if t.contains("Evil") {
            LinkHref::Page {
                href: "javascript:alert(1)".into(),
                title: None,
            }
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
        let t = link.target.as_str();
        (t == "Loop" || t.starts_with("Included")).then(|| Transclusion {
            path: format!("pages/{t}.md"),
            title: t.to_owned(),
            href: format!("/w/{t}"),
            label: format!("From {t}"),
        })
    }

    fn transclusion_body(&self, page: &Transclusion) -> Option<String> {
        Some(if page.title == "Loop" {
            "Again:\n\n![[Loop]]\n".to_owned()
        } else {
            "# Inner heading\n\nInner *text* ^inner\n\n![[Included deeper]]\n".to_owned()
        })
    }

    fn transclusion_notice(&self, r: &TransclusionRefusal) -> String {
        format!("not included: {r}")
    }
}

/// Every construct the profile and Markdown have, one of each at least.
const EVERYTHING: &str = "# Title with `code`

Setext heading ^setext
======================

Paragraph with *emphasis*, **strong**, ~~strike~~, `code`, a [local](./x.md) link, an
[external](https://example.org \"tip\") link, <https://example.org/auto>, a footnote[^1],
a tag #topic, a block id. ^para

![alt](/att/x.png \"title\") ![alt](https://example.org/remote.png)

[[Page]] [[Page|label]] [[Page#Heading]] [[Page#^block]] [[#Here]] [[Missing]] [[Hidden]] [[wp:Wiki]]
![[diagram.png|300]] ![[manual.pdf]] [[Evil]]

![[Included one]]

![[Included two]]

![[Loop]]

![[Missing page]]

- bullet ^b1
- [ ] open task
- [x] done task
  1. nested ordered

3. third
4. fourth

> quote ^q1

> [!NOTE]
> Plain callout ^c1

> [!tip]- Folded callout
> With a **title**

:::warning Heads up
Body of the container.
:::

::: toc depth=2
:::

Press :kbd[Ctrl]{} and :ref[thing]{id=1} now.

| a | b | c |
|:--|:-:|--:|
| 1 | 2 | 3 |
^table

```rust
fn main() {}
```
^code

| left | right |
|:-----|------:|
| 1 | 2 |

    indented code

---

[^1]: The footnote.

<!-- a hidden comment -->

## Setext two
-----------
";

#[test]
fn everything_the_renderer_writes_for_every_construct_is_in_the_vocabulary() {
    let page = Page::parse(EVERYTHING).expect("parses");
    let rendered = render_page(
        Some(("pages/Everything.md", "Everything")),
        &page.body,
        &Rich,
        inklkunn_markup::scan::ScanOptions::default(),
    );
    let v = violations(&rendered.unsanitized_html);
    assert!(
        v.is_empty(),
        "outside the vocabulary: {v:?}\n{}",
        rendered.unsanitized_html
    );
    // The test is not vacuous: the constructs it names are really in the output.
    let seen: BTreeSet<String> = tags(&rendered.unsanitized_html)
        .into_iter()
        .map(|t| t.name)
        .collect();
    for expected in [
        "a",
        "aside",
        "blockquote",
        "code",
        "del",
        "div",
        "em",
        "h1",
        "h2",
        "hr",
        "img",
        "input",
        "kbd",
        "li",
        "ol",
        "p",
        "pre",
        "span",
        "strong",
        "sup",
        "table",
        "td",
        "th",
        "ul",
    ] {
        assert!(seen.contains(expected), "<{expected}> was not rendered");
    }
    let classes: BTreeSet<String> = tags(&rendered.unsanitized_html)
        .into_iter()
        .flat_map(|t| t.attrs)
        .filter(|(a, _)| a == "class")
        .flat_map(|(_, v)| {
            v.split_ascii_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    for expected in [
        "wikilink",
        "wikilink-missing",
        "wikilink-self",
        "wikilink-embed",
        "wikilink-attachment",
        "wikilink-interwiki",
        "wikilink-blocked",
        "callout",
        "callout-title",
        "directive",
        "tag",
        "external",
        "embed",
        "transclusion",
        "transclusion-source",
        "transclusion-refused",
        "footnote-reference",
        "footnote-definition",
        "language-rust",
        "callout-note",
    ] {
        assert!(
            classes.contains(expected),
            "class {expected} was not written: {classes:?}"
        );
    }
    // Everything the Markdown writer wrote for a column alignment is a known exception.
    assert!(
        rendered.unsanitized_html.contains("style=\"text-align"),
        "{}",
        rendered.unsanitized_html
    );
}

#[test]
fn every_conformance_case_renders_inside_the_vocabulary() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/markup");
    let mut cases = 0;
    for case in std::fs::read_dir(&dir).expect("corpus").flatten() {
        let input = case.path().join("input.md");
        let Ok(text) = std::fs::read_to_string(&input) else {
            continue;
        };
        cases += 1;
        let body = Page::parse(&text).map_or(text.clone(), |p| p.body);
        let html = render_html(&body, &Rich).unsanitized_html;
        let v = violations(&html);
        assert!(
            v.is_empty(),
            "{}: outside the vocabulary: {v:?}",
            input.display()
        );
    }
    assert!(cases >= 20, "the corpus was found ({cases} cases)");
}

#[test]
fn the_scanner_this_test_stands_on_reads_tags_attributes_comments_and_quotes() {
    let t = tags(
        "<p id=\"a b\" class='x y' data-k=v hidden>t<!-- <b> --><a href=\"x?a=1&amp;b=2\">l</a><br/></p>",
    );
    let names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["p", "a", "br"]);
    assert_eq!(
        t.first().map(|t| t.attrs.clone()),
        Some(vec![
            ("id".to_owned(), "a b".to_owned()),
            ("class".to_owned(), "x y".to_owned()),
            ("data-k".to_owned(), "v".to_owned()),
            ("hidden".to_owned(), String::new()),
        ])
    );
    // And the checker flags what it should.
    let v = violations(
        "<script>x</script><p onclick=\"x\" class=\"site-header wikilink\"><td style=\"a\">",
    );
    assert!(v.contains("element <script>"), "{v:?}");
    assert!(v.contains("attribute onclick on <p>"), "{v:?}");
    assert!(v.contains("class site-header on <p>"), "{v:?}");
    assert!(!v.iter().any(|l| l.contains("style")), "{v:?}");
}
