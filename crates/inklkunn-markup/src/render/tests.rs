#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

struct Fake;

impl RenderLinks for Fake {
    fn link(&self, link: &Link) -> LinkHref {
        match link.target.as_str() {
            "Exists" => LinkHref::Page {
                href: "/wiki/Exists".into(),
                title: Some("Exists".into()),
            },
            "wp:Wiki" => LinkHref::External("https://en.wikipedia.org/wiki/Wiki".into()),
            t if t.ends_with(".png") => LinkHref::Attachment(format!("/att/{t}")),
            _ => LinkHref::Missing {
                href: Some(format!("/new/{}", link.target)),
            },
        }
    }

    fn tag(&self, tag: &str) -> Option<String> {
        Some(format!("/tags/{tag}"))
    }
}

#[test]
fn wikilink_classes_and_targets() {
    let r = render_html(
        "[[Exists]] [[Missing|label]] [[Exists#Heading here]] [[#Local]] [[wp:Wiki]] ![[d.png|300]]",
        &Fake,
    );
    let h = &r.unsanitized_html;
    assert!(
        h.contains(
            r#"<a class="wikilink" href="/wiki/Exists" data-wiki-target="Exists">Exists</a>"#
        ),
        "{h}"
    );
    assert!(h.contains(r#"<a class="wikilink wikilink-missing" href="/new/Missing" data-wiki-target="Missing">label</a>"#), "{h}");
    assert!(h.contains(r#"href="/wiki/Exists#heading-here""#), "{h}");
    assert!(
        h.contains(r##"<a class="wikilink wikilink-self" href="#local">Local</a>"##),
        "{h}"
    );
    assert!(h.contains(r#"rel="nofollow ugc""#), "{h}");
    assert!(
        h.contains(r#"<img class="wikilink-embed" src="/att/d.png" alt="d.png" width="300">"#),
        "{h}"
    );
}

#[test]
fn code_comments_and_escapes_stay_literal() {
    let r = render_html(
        "`[[Code]]` and \\[[Esc]] <!-- [[Hidden]] -->\n\n```\n[[Fence]]\n```\n",
        &Fake,
    );
    assert!(
        !r.unsanitized_html.contains("wikilink"),
        "{}",
        r.unsanitized_html
    );
    assert!(r.unsanitized_html.contains("<code>[[Code]]</code>"));
    assert!(r.unsanitized_html.contains("[[Esc]]"));
}

#[test]
fn headings_get_ids_and_toc() {
    let r = render_html(
        "# Title with `code`\n\n## Merging\n\n## Merging\n\n### With [[Exists]]\n",
        &Fake,
    );
    assert!(
        r.unsanitized_html.contains(r#"<h2 id="merging-1">"#),
        "{}",
        r.unsanitized_html
    );
    assert_eq!(
        r.toc.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["title-with-code", "merging", "merging-1", "with-exists"]
    );
}

#[test]
fn callouts_tags_directives_and_block_ids() {
    let r = render_html(
        "> [!TIP]+ Open title\n> body\n\n> [!NOTE]\n> only\n\nPara #one. ^p1\n\n::: toc depth=2\n:::\n\n:::note Heads up\nInside.\n:::\n\nPress :kbd[Ctrl]{} now.\n",
        &Fake,
    );
    let h = &r.unsanitized_html;
    assert!(h.contains(r#"<aside class="callout callout-tip" data-callout="TIP" data-fold="+"><p class="callout-title">Open title</p>"#), "{h}");
    assert!(h.contains("<p>body</p>\n</aside>"), "{h}");
    assert!(
        h.contains(r#"<aside class="callout callout-note" data-callout="NOTE">"#),
        "{h}"
    );
    assert!(h.contains("<p>only</p>\n</aside>"), "{h}");
    assert!(
        h.contains(r#"<a class="tag" href="/tags/one" data-tag="one">#one</a>"#),
        "{h}"
    );
    // MKUP-12: the identifier is an attribute of the block it ends, with no marker beside it.
    assert!(h.contains(r#"<p id="^p1">Para "#), "{h}");
    assert!(!h.contains("block-id"), "{h}");
    assert!(
        h.contains(
            r#"<div class="directive directive-toc" data-directive="toc" data-attr-depth="2">"#
        ),
        "{h}"
    );
    assert!(
        h.contains(r#"<p class="callout-title">Heads up</p>"#),
        "{h}"
    );
    assert!(h.contains("<p>Inside.</p>"), "{h}");
    assert!(h.contains("<kbd>Ctrl</kbd>"), "{h}");
}

#[test]
fn external_markdown_links_carry_rel() {
    let r = render_html("[site](https://example.org) and [local](./x.md)", &Fake);
    assert!(
        r.unsanitized_html.contains(
            r#"<a class="external" href="https://example.org" rel="nofollow ugc">site</a>"#
        ),
        "{}",
        r.unsanitized_html
    );
    assert!(
        r.unsanitized_html.contains(r#"<a href="./x.md">local</a>"#),
        "{}",
        r.unsanitized_html
    );
}
