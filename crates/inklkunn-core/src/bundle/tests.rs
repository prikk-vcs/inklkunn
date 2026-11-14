use super::*;
use inklkunn_store::{DisplayName, NewWiki, Slug, WikiId};

fn record_named(name: &str) -> WikiRecord {
    let slug = Slug::new("demo").unwrap_or_else(|e| panic!("{e}"));
    let name = DisplayName::new(name).unwrap_or_else(|e| panic!("{e}"));
    WikiRecord::from_new(WikiId::new(), NewWiki::new(slug, name), 0)
        .unwrap_or_else(|e| panic!("{e}"))
}

fn record() -> WikiRecord {
    record_named("Demo: a wiki")
}

#[test]
fn the_manifest_is_plain_yaml_the_markup_crate_can_read() {
    let m = manifest(&record(), "0.0.0");
    let v = inklkunn_markup::frontmatter::parse_yaml(&m).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        v.get("format").and_then(|f| f.as_str()),
        Some("portable-wiki-bundle")
    );
    assert_eq!(
        v.get("source")
            .and_then(|s| s.get("name"))
            .and_then(|n| n.as_str()),
        Some("Demo: a wiki")
    );
    assert_eq!(
        v.get("fidelity").and_then(|f| f.get("level")),
        Some(&inklkunn_markup::frontmatter::Value::Int(3))
    );
    // Values a person chose are quoted whenever YAML could read them otherwise.
    let mut r = record();
    r.default_license = "LicenseRef-x OR (MIT AND no)".to_owned();
    let m = manifest(&r, "0.0.0");
    let v = inklkunn_markup::frontmatter::parse_yaml(&m).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        v.get("license").and_then(|l| l.as_str()),
        Some("LicenseRef-x OR (MIT AND no)")
    );
    assert_eq!(yaml_quote("yes"), "\"yes\"");
    assert_eq!(yaml_quote("1.0"), "\"1.0\"");
    assert_eq!(yaml_quote("CC-BY-SA-4.0"), "CC-BY-SA-4.0");
}

#[test]
fn the_help_page_parses_scans_and_keeps_a_name_as_text() {
    let page = inklkunn_markup::Page::parse(&help_page(
        &record(),
        "019a0000-0000-7000-8000-000000000001",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(page.metadata.title.as_deref(), Some("Help"));
    assert!(page.scan.links.iter().any(|l| l.target == "Help"));
    assert!(
        page.scan.headings.iter().all(|h| h.level >= 2),
        "body headings begin at level 2 (MKUP-8)"
    );
    // A name with markup characters is text in the page, not structure.
    let tricky = record_named("Links [[Evil]] and #tags *here*");
    let page =
        inklkunn_markup::Page::parse(&help_page(&tricky, "019a0000-0000-7000-8000-000000000002"))
            .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        !page.scan.links.iter().any(|l| l.target == "Evil"),
        "{:?}",
        page.scan.links
    );
    assert!(page.scan.tags.inline.is_empty(), "{:?}", page.scan.tags);
    assert_eq!(page.scan.headings.len(), 5);
}

#[test]
fn the_ignore_file_reserves_the_service_prefix() {
    assert_eq!(prikkignore(), ".inklkunn/\n");
}

/// The text a rendered page shows: the tags taken out, the entities put back. Only `<p>` and `<br />` may be among
/// them (an escaped body is paragraphs and line breaks and nothing that links, embeds or styles).
fn shown(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(rest.get(..open).unwrap_or_default());
        let after = rest.get(open..).unwrap_or_default();
        let close = after.find('>').unwrap_or(after.len() - 1);
        let tag = after.get(1..close).unwrap_or_default();
        assert!(
            matches!(tag, "p" | "/p" | "br /" | "br"),
            "the escaped body rendered `<{tag}>`: {html}"
        );
        out.push(' ');
        rest = after.get(close + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_text_escaped_to_markdown_shows_exactly_the_words_it_was_with_no_link_image_or_html() {
    let cases = [
        "Where was this measured?",
        "See [the survey](https://evil.example/x) and [[Some Page]].",
        "![tracking pixel](https://evil.example/pixel.png) ![[a.png]]",
        "<script>alert(1)</script> <img src=x onerror=y> <b>bold</b> &amp; &lt;i&gt; &#60;",
        "line one\nline two\n\nafter a gap\n\n\n\nand another",
        "# not a heading\n> not a quote\n- not a list\n+ nor this\n* nor this\n1. not numbered\n2) nor this",
        "    four spaces, not code\n\ttabbed\ntrailing spaces   \n   ",
        "`code` **strong** _em_ ~~gone~~ | a | table |\n---|---\n===",
        "autolink <https://evil.example/> and https://evil.example/path and www.evil.example and a@b.example",
        "tags #later and a block id ^b1 and a directive :note[text]{k=v} and ::: warn\nbody\n:::",
        "> [!NOTE] a callout\n[^1]: a footnote\n\\ a backslash at the end \\",
        "ends with a hard break \\\nand a name: value",
        "日本語の文章。全角の＊や［ ］や「」もそのまま。",
    ];
    for case in cases {
        let md = markdown_text(case);
        let html = inklkunn_markup::render_html(&md, &inklkunn_markup::NoLinks).unsanitized_html;
        assert_eq!(
            words(&shown(&html)),
            words(case),
            "the text must read as it was\n--- text\n{case}\n--- markdown\n{md}\n--- html\n{html}"
        );
    }
}

#[test]
fn an_escaped_text_has_no_construct_the_scanner_finds() {
    let md = markdown_text(
        "[[Link]] and ![[image.png]] and #tag and ^id\n:name[x]{k=v}\n> [!NOTE] x\n[a](https://x.example)",
    );
    let page = inklkunn_markup::scan(&md);
    assert!(page.links.is_empty(), "{md}\n{:?}", page.links);
    assert!(page.tags.inline.is_empty(), "{md}");
    assert!(page.block_ids.is_empty(), "{md}");
    assert!(page.directives.is_empty(), "{md}");
    assert!(page.callouts.is_empty(), "{md}");
}
