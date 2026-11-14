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

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// What a CommonMark and GFM reader other than inklkunn's renderer (`pulldown-cmark` itself, every extension on) makes of
/// `markdown`: the one thing a text must be, a fenced block with the info string `text`, and the text of that block.
/// Anything else (a paragraph, a link, a second block) fails the test.
fn text_block_of(markdown: &str) -> String {
    let events: Vec<Event> = Parser::new_ext(markdown, Options::all()).collect();
    let (first, rest) = events
        .split_first()
        .unwrap_or_else(|| panic!("no events\n{markdown}"));
    assert!(
        matches!(first, Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if &**info == "text"),
        "not one fenced text block: {events:?}\n{markdown}"
    );
    let (last, middle) = rest
        .split_last()
        .unwrap_or_else(|| panic!("no end\n{markdown}"));
    assert!(
        matches!(last, Event::End(TagEnd::CodeBlock)),
        "the block does not end the document: {events:?}\n{markdown}"
    );
    let mut shown = String::new();
    for event in middle {
        match event {
            Event::Text(t) => shown.push_str(t),
            other => panic!("{other:?} inside the block: {events:?}\n{markdown}"),
        }
    }
    // The block's content is lines, each ended by a line feed; the text is the content less the last one.
    shown
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("content not ended by a line feed: {shown:?}"))
        .to_owned()
}

/// The text with its line endings as LF, the one change the bundle makes.
fn lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[test]
fn a_text_in_the_bundle_is_a_fenced_text_block_that_a_commonmark_reader_shows_as_it_is() {
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
        // The reviewer's cases (V-c): an extended autolink, a mail address, a linkifier after unescaping, a lone carriage
        // return, a line of only no-break spaces, math and a comment marker.
        "www.evil.example/x and mail@evil.example and https://evil.example and https\\://evil.example",
        "x\r1. item\r2. item",
        "x\r\ny\r\n\r\nz",
        "\u{a0}\n\u{a0}\u{a0}\nafter",
        "$x^2$ and $$y$$ and %% a comment %% and ==marked== and ^sup^ and ~sub~ and ::: fenced div",
        "---\ntitle: front matter\n---\nbody\n***\n___\n[ref]: https://evil.example/x \"t\"\n\n[ref]",
        "term\n: a definition\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [ ] a task\n- [x] done",
        "<!-- comment --> <details><summary>x</summary></details>\n\n<div>block</div>",
        "~~~\ntilde fence\n~~~\n\n    indented",
        "",
        "\n",
        "\n\nleading and trailing blank lines\n\n",
        " leading space",
    ];
    for case in cases {
        let md = markdown_text(case);
        assert_eq!(text_block_of(&md), lf(case), "--- markdown\n{md}");
        // Nothing else was changed: the text stands in the block as it is.
        assert!(
            md.contains(&lf(case)),
            "the text was altered\n--- text\n{case}\n--- markdown\n{md}"
        );
    }
}

#[test]
fn the_fence_is_one_more_backtick_than_the_longest_run_in_the_text_and_never_fewer_than_three() {
    assert!(markdown_text("plain").starts_with("```text\n"));
    assert!(markdown_text("a ` b `` c").starts_with("```text\n"));
    assert!(markdown_text("a ``` b").starts_with("````text\n"));
    assert!(markdown_text("```\nnot a closing fence\n```").starts_with("````text\n"));
    assert!(markdown_text("``````").starts_with("```````text\n"));
    // A run is of backticks next to each other: two lines of two are not a run of four.
    assert!(markdown_text("``\n``").starts_with("```text\n"));
    assert!(markdown_text("``\r``").starts_with("```text\n"));
    assert!(markdown_text("``\r\n``").starts_with("```text\n"));
    for text in [
        "```",
        "````",
        "```\n<script>\n```\n",
        "````\n```\n`````\n``````",
        "``` text\n```text\n ```\n  ````\n~~~",
        "a ``` b ```` c ````` d",
        "`````````````````````",
    ] {
        let md = markdown_text(text);
        assert_eq!(text_block_of(&md), lf(text), "--- markdown\n{md}");
    }
}

#[test]
fn a_texts_line_endings_are_lf_and_nothing_else_about_it_changes() {
    assert_eq!(markdown_text("a\r\nb\rc\nd"), "```text\na\nb\nc\nd\n```");
    assert_eq!(
        markdown_text("  é &amp; \u{a0}  \n\ttab"),
        "```text\n  é &amp; \u{a0}  \n\ttab\n```"
    );
    assert_eq!(markdown_text(""), "```text\n\n```");
}

#[test]
fn every_short_text_of_awkward_characters_round_trips_through_its_fence() {
    // Every text up to five characters from an alphabet of what ends a line, opens a construct or is a fence.
    let alphabet = ['`', '\n', '\r', ' ', '~', '#', '>', '-', '\u{a0}'];
    let mut level = vec![String::new()];
    let mut all = level.clone();
    for _ in 0..5 {
        level = level
            .iter()
            .flat_map(|t| {
                alphabet.iter().map(move |c| {
                    let mut next = t.clone();
                    next.push(*c);
                    next
                })
            })
            .collect();
        all.extend(level.iter().cloned());
    }
    assert!(all.len() > 60_000, "{}", all.len());
    for text in all {
        let md = markdown_text(&text);
        assert_eq!(text_block_of(&md), lf(&text), "--- markdown\n{md:?}");
    }
}

#[test]
fn a_text_in_the_bundle_has_no_construct_inklkunns_scanner_finds_and_renders_as_code() {
    let md = markdown_text(
        "[[Link]] and ![[image.png]] and #tag and ^id\n:name[x]{k=v}\n> [!NOTE] x\n[a](https://x.example)\n\
         ::: warn\nbody\n:::\n```\nnot a closing fence",
    );
    let page = inklkunn_markup::scan(&md);
    assert!(page.links.is_empty(), "{md}\n{:?}", page.links);
    assert!(page.tags.inline.is_empty(), "{md}");
    assert!(page.block_ids.is_empty(), "{md}");
    assert!(page.directives.is_empty(), "{md}");
    assert!(page.callouts.is_empty(), "{md}");
    let html = inklkunn_markup::render_html(&md, &inklkunn_markup::NoLinks).unsanitized_html;
    assert!(
        html.starts_with("<pre><code class=\"language-text\">"),
        "{html}"
    );
    assert!(
        !html.contains("<a ") && !html.contains("<img") && !html.contains("<p>"),
        "{html}"
    );
}
