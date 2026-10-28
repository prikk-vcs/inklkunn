#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

/// A language for a test: its own tag, direction, plural rule and number grouping, and entries keyed by the
/// real keys' identifiers.
fn language(
    tag: &'static str,
    dir: Dir,
    plural: fn(u64) -> Plural,
    group_separator: char,
    entries: Vec<Entry>,
) -> &'static LangDef {
    Box::leak(Box::new(LangDef {
        tag,
        endonym: tag,
        dir,
        plural,
        group_separator,
        review: Review::Preview,
        entries: Box::leak(entries.into_boxed_slice()),
    }))
}

fn only_other(_: u64) -> Plural {
    Plural::Other
}

fn t_in(def: &'static LangDef) -> T {
    T::new(Lang::from_def(def))
}

#[test]
fn the_shipped_tables_pass_the_audit_and_follow_one_order() {
    assert_eq!(audit(languages()), Vec::<Problem>::new());
    let ids = |def: &LangDef| def.entries.iter().map(|e| e.key).collect::<Vec<_>>();
    let keys: Vec<&str> = Key::ALL.iter().map(|k| k.id()).collect();
    assert_eq!(ids(&EN_DEF), keys, "English follows the key order");
    assert_eq!(ids(&JA_DEF), keys, "Japanese follows the English order");
}

#[test]
fn keys_are_unique_dotted_identifiers_and_their_variant_names_are_unique() {
    let mut ids: Vec<&str> = Key::ALL.iter().map(|k| k.id()).collect();
    let mut variants: Vec<&str> = Key::ALL.iter().map(|k| k.variant()).collect();
    for id in &ids {
        assert!(
            id.contains('.')
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'),
            "{id}: area.name in lower case"
        );
    }
    let (n, m) = (ids.len(), variants.len());
    ids.sort_unstable();
    ids.dedup();
    variants.sort_unstable();
    variants.dedup();
    assert_eq!((ids.len(), variants.len()), (n, m));
}

#[test]
fn english_is_the_source_japanese_is_a_preview_and_the_switcher_lists_both() {
    assert_eq!(EN_DEF.review, Review::Source);
    assert!(!EN.is_preview());
    assert_eq!(JA_DEF.review, Review::Preview);
    assert!(Lang::from_def(&JA_DEF).is_preview());
    assert!(
        !Review::Reviewed { by: "a, b, 2026" }.is_preview(),
        "a recorded sign-off ends the preview"
    );
    let tags: Vec<&str> = languages().iter().map(|d| d.tag).collect();
    assert_eq!(tags, ["en", "ja"], "English first");
    for def in languages() {
        assert!(!def.endonym.is_empty());
    }
}

#[test]
fn plural_and_placeholder_rules_follow_the_language() {
    let en = language(
        "t1",
        Dir::Ltr,
        plural_en,
        ',',
        vec![plural_msg(
            "layout.footer",
            "{count} thing",
            "{count} things",
        )],
    );
    let t = t_in(en);
    assert_eq!(t.msg(Key::LayoutFooter).n(1).text(), "1 thing");
    assert_eq!(t.msg(Key::LayoutFooter).n(0).text(), "0 things");
    assert_eq!(t.msg(Key::LayoutFooter).n(1200).text(), "1,200 things");

    // No grammatical plural: one form for every count, in the language's own number format; and a different
    // word order, because the placeholders are named.
    let q = language(
        "t2",
        Dir::Rtl,
        only_other,
        '.',
        vec![
            msg("layout.footer", "{version} / {product} ‹version›"),
            msg("layout.language", "{count} ‹languages›"),
        ],
    );
    let t = t_in(q);
    assert_eq!(
        t.msg(Key::LayoutFooter)
            .product()
            .with("version", "1")
            .text(),
        "1 / inklkunn ‹version›"
    );
    assert_eq!(t.msg(Key::LayoutLanguage).n(1).text(), "1 ‹languages›");
    assert_eq!(
        t.msg(Key::LayoutLanguage).n(1200).text(),
        "1.200 ‹languages›"
    );
    assert_eq!(Lang::from_def(q).dir(), "rtl");
}

#[test]
fn numbers_group_by_threes() {
    for (n, want) in [
        (0, "0"),
        (7, "7"),
        (999, "999"),
        (1000, "1,000"),
        (12_345, "12,345"),
        (1_234_567, "1,234,567"),
    ] {
        assert_eq!(EN.number(n), want);
        assert_eq!(Lang::from_def(&JA_DEF).number(n), want);
    }
}

fn plural_en(n: u64) -> Plural {
    if n == 1 { Plural::One } else { Plural::Other }
}

#[test]
fn a_missing_key_is_a_visible_marker_never_a_crash_and_never_another_language() {
    let empty = language("t3", Dir::Ltr, only_other, ',', vec![]);
    let t = t_in(empty);
    assert_eq!(t.get(Key::LayoutSkip), "[layout.skip]");
    assert_eq!(t.msg(Key::NotFoundBody).html().as_str(), "[not_found.body]");
    assert!(
        audit(&[&EN_DEF, empty]).len() >= Key::ALL.len(),
        "the audit sees every missing key"
    );
}

#[test]
fn an_unfilled_placeholder_stays_visible() {
    let t = T::new(EN);
    assert_eq!(
        t.get(Key::LayoutFooter),
        "{product} version {version}",
        "the tests that scan rendered pages catch a placeholder nobody filled"
    );
    assert_eq!(
        t.msg(Key::LayoutFooter).product().text(),
        "inklkunn version {version}"
    );
}

#[test]
fn a_value_is_never_scanned_for_placeholders() {
    let t = T::new(EN);
    assert_eq!(
        t.msg(Key::LayoutFooter)
            .product()
            .with("version", "{product}")
            .text(),
        "inklkunn version {product}"
    );
}

// ---------------------------------------------------------------- typed slots cannot inject markup

fn tags_of(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        let (_, from) = rest.split_at(i);
        let end = from.find('>').map_or(from.len(), |j| j + 1);
        let (tag, after) = from.split_at(end);
        out.push(tag.to_owned());
        rest = after;
    }
    out
}

/// Only the tags the slots build: a link whose `href` holds no quote, and a `lang` span.
fn only_expected_tags(html: &str) {
    for tag in tags_of(html) {
        let ok = tag == "</a>"
            || tag == "</span>"
            || (tag.starts_with("<a href=\"")
                && tag.ends_with("\">")
                && tag
                    .strip_prefix("<a href=\"")
                    .and_then(|t| t.strip_suffix("\">"))
                    .is_some_and(|v| !v.contains(['"', '<', '>'])))
            || (tag.starts_with("<span lang=\"")
                && tag.ends_with("\">")
                && tag
                    .strip_prefix("<span lang=\"")
                    .and_then(|t| t.strip_suffix("\">"))
                    .is_some_and(|v| v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')));
        assert!(ok, "unexpected markup {tag:?} in {html}");
    }
}

const HOSTILE: [&str; 6] = [
    "<script>alert(1)</script>",
    "\"><img src=x onerror=alert(1)>",
    "' onmouseover='alert(1)",
    "</span><a href=\"javascript:alert(1)\">x</a>",
    "&lt;b&gt; &amp; <b>",
    "{product}{version}",
];

/// A language whose one message holds every kind of slot, so one hostile value can be tried in each.
fn slot_language() -> &'static LangDef {
    language(
        "t4",
        Dir::Ltr,
        only_other,
        ',',
        vec![msg("layout.footer", "{a} then {b} then {c}")],
    )
}

#[test]
fn a_hostile_value_in_a_text_link_or_language_slot_cannot_inject_markup_or_an_attribute() {
    let t = t_in(slot_language());
    for v in HOSTILE {
        let html = t
            .msg(Key::LayoutFooter)
            .with("a", v)
            .link("b", v, "/ok")
            .in_lang("c", "ja", v)
            .html();
        only_expected_tags(html.as_str());
        if v.contains('<') || v.contains('"') {
            assert!(!html.as_str().contains(v), "unescaped {v:?} in {html}");
        }
        // The plain-text rendering is not HTML: whoever prints it escapes it.
        assert_eq!(
            t.msg(Key::LayoutFooter)
                .with("a", v)
                .link("b", v, "/ok")
                .in_lang("c", "ja", v)
                .text(),
            format!("{v} then {v} then {v}")
        );
    }
}

#[test]
fn a_link_href_comes_from_code_and_only_a_local_path_or_an_http_url_becomes_a_link() {
    let t = t_in(slot_language());
    for (href, linked) in [
        ("/x/y?q=1&r=2", true),
        ("https://example.invalid/a?b=c", true),
        ("http://example.invalid/a", true),
        ("javascript:alert(1)", false),
        ("data:text/html,<script>", false),
        ("//evil.example/x", false),
        ("/\\evil.example", false),
        ("/ok onmouseover=alert(1)", false),
        ("", false),
        ("relative/path", false),
        ("\u{0}/x", false),
    ] {
        let html = t
            .msg(Key::LayoutFooter)
            .with("a", "")
            .link("b", "go", href)
            .with("c", "")
            .html();
        only_expected_tags(html.as_str());
        assert_eq!(
            html.as_str().contains("<a href="),
            linked,
            "{href:?}: {html}"
        );
        assert!(
            html.as_str().contains("go"),
            "the label always shows: {html}"
        );
    }
    // A quote in an allowed-looking href cannot end the attribute: it is written as an entity.
    let html = t
        .msg(Key::LayoutFooter)
        .with("a", "")
        .link("b", "go", "/x\"onmouseover=\"y")
        .with("c", "")
        .html();
    assert!(
        html.as_str()
            .contains("href=\"/x&quot;onmouseover=&quot;y\""),
        "{html}"
    );
    assert_eq!(
        html.as_str().matches('"').count(),
        2,
        "one attribute, closed once: {html}"
    );
    let html = t
        .msg(Key::LayoutFooter)
        .with("a", "")
        .link("b", "go", "/x?a=1&b=2")
        .with("c", "")
        .html();
    assert!(html.as_str().contains("href=\"/x?a=1&amp;b=2\""), "{html}");
}

#[test]
fn a_language_tag_that_is_not_a_tag_is_not_written_into_an_attribute() {
    let html = in_lang("ja", "日本語");
    assert_eq!(html.as_str(), "<span lang=\"ja\">日本語</span>");
    let too_long = "a".repeat(36);
    for bad in ["", "ja\"><b>", "ja onload=x", "ja;", too_long.as_str()] {
        let html = in_lang(bad, "<i>x</i>");
        assert_eq!(html.as_str(), "&lt;i&gt;x&lt;/i&gt;", "{bad:?}");
    }
}

#[test]
fn a_catalog_entry_cannot_introduce_a_tag_an_attribute_or_a_link_by_itself() {
    let evil = language(
        "t5",
        Dir::Ltr,
        only_other,
        ',',
        vec![
            msg(
                "layout.footer",
                "<b onclick=\"x\">{a}</b> <a href=\"/steal\">go</a>",
            ),
            msg("layout.skip", "<script>alert(1)</script>"),
        ],
    );
    let t = t_in(evil);
    let html = t.msg(Key::LayoutFooter).link("a", "dl", "/raw").html();
    // The catalog's markup is text; only the one typed link slot is an element.
    assert!(
        html.as_str().contains("&lt;b onclick=&quot;x&quot;&gt;"),
        "{html}"
    );
    assert!(
        html.as_str().contains("&lt;a href=&quot;/steal&quot;&gt;"),
        "{html}"
    );
    assert_eq!(html.as_str().matches("<a ").count(), 1, "{html}");
    assert!(!t.msg(Key::LayoutSkip).html().as_str().contains("<script"));
    // …and the audit refuses such an entry in the first place.
    let problems = audit(&[&EN_DEF, evil]);
    for key in ["layout.footer", "layout.skip"] {
        assert!(
            problems.contains(&Problem::new(Kind::Markup, "t5", key)),
            "{key}: {problems:?}"
        );
    }
}

#[test]
fn askama_prints_catalog_html_as_it_is_and_still_escapes_everything_else() {
    use askama::Template;
    #[derive(Template)]
    #[template(
        source = "<p>{{ rich }}</p><p>{{ plain }}</p><p>{{ msg }}</p>",
        ext = "html"
    )]
    struct P {
        rich: Rich,
        plain: String,
        msg: Msg,
    }
    let t = t_in(slot_language());
    let out = P {
        rich: t
            .msg(Key::LayoutFooter)
            .with("a", "<i>a</i>")
            .link("b", "<i>go</i>", "/raw?a=1&b=2")
            .with("c", "")
            .html(),
        plain: "<script>".into(),
        msg: t.msg(Key::LayoutFooter).with("a", "<b>"),
    }
    .render()
    .unwrap();
    assert!(
        out.contains("<a href=\"/raw?a=1&amp;b=2\">&lt;i&gt;go&lt;/i&gt;</a>"),
        "{out}"
    );
    assert!(
        !out.contains("&amp;lt;") && !out.contains("&amp;amp;"),
        "not escaped twice: {out}"
    );
    assert!(out.contains("<p>&#60;script&#62;</p>"), "{out}");
    assert!(
        out.contains("&#60;b&#62; then {b} then {c}"),
        "a message printed as text is escaped by askama: {out}"
    );
}

// ---------------------------------------------------------------- negotiation

/// Japanese after its sign-off: the same table, no longer a preview.
fn reviewed_japanese() -> &'static LangDef {
    Box::leak(Box::new(LangDef {
        tag: "ja",
        endonym: ja::ENDONYM,
        dir: Dir::Ltr,
        plural: only_other,
        group_separator: ',',
        review: Review::Reviewed {
            by: "the reviewers, on a date",
        },
        entries: ja::CATALOG,
    }))
}

#[test]
fn accept_language_selects_a_reviewed_language_by_q_value_and_falls_back_to_english() {
    let available = [&EN_DEF, reviewed_japanese()];
    let tag = |accept: Option<&str>, cookie: Option<&str>| {
        Lang::negotiate_in(&available, accept, cookie).tag()
    };
    for (accept, want) in [
        ("ja", "ja"),
        ("ja-JP", "ja"),
        ("JA-jp", "ja"),
        ("ja_JP", "ja"),
        ("ja-JP,en;q=0.5", "ja"),
        ("en;q=0.4,ja;q=0.9", "ja"),
        ("en,ja", "en"),
        ("ja,en", "ja"),
        ("en-GB,ja;q=0.3", "en"),
        ("ja;q=0.5, en;q=0.9", "en"),
        ("ja;q=0", "en"),
        ("ja;q=0.0, en;q=0", "en"),
        ("ja;q=abc", "en"),
        ("ja;q=NaN", "en"),
        ("ja;q=2", "en"),
        ("ja;q=-1", "en"),
        ("fr, de;q=0.5", "en"),
        ("zh-CN,zh;q=0.9", "en"),
        ("*", "en"),
        ("*;q=0.1, ja;q=0.1", "ja"),
        ("", "en"),
        (",,;;==", "en"),
        ("ja ; q=0.5 , en ; q=0.4", "ja"),
    ] {
        assert_eq!(tag(Some(accept), None), want, "{accept:?}");
    }
    assert_eq!(tag(None, None), "en");
}

#[test]
fn a_preview_is_chosen_by_the_reader_through_the_cookie_and_never_by_accept_language() {
    let tag = |accept: Option<&str>, cookie: Option<&str>| Lang::negotiate(accept, cookie).tag();
    // Japanese is a preview in this build: a browser that asks for it is served English…
    for accept in [
        "ja",
        "ja-JP",
        "ja-JP,en;q=0.5",
        "en;q=0.4,ja;q=0.9",
        "ja;q=1.0, *;q=0.1",
    ] {
        assert_eq!(tag(Some(accept), None), "en", "{accept:?}");
    }
    // …and the reader who picks it on the language page is served Japanese, whatever the browser asks for.
    assert_eq!(tag(None, Some("ja")), "ja");
    assert_eq!(tag(Some("en"), Some("ja")), "ja");
    assert_eq!(tag(Some("fr"), Some("ja-JP")), "ja");
    // Once the sign-off is recorded the same table is negotiated like any other language.
    let reviewed = Lang::negotiate_in(&[&EN_DEF, reviewed_japanese()], Some("ja"), None);
    assert_eq!(reviewed.tag(), "ja");
    // The rule is the language's review state and nothing about Japanese in particular.
    let draft = language("xa", Dir::Ltr, only_other, ',', vec![]);
    assert_eq!(
        Lang::negotiate_in(&[&EN_DEF, draft], Some("xa"), None).tag(),
        "en"
    );
    assert_eq!(
        Lang::negotiate_in(&[&EN_DEF, draft], None, Some("xa")).tag(),
        "xa"
    );
}

#[test]
fn only_the_first_entries_of_an_accept_language_header_are_read() {
    let available = [&EN_DEF, reviewed_japanese()];
    let mut header = "xx,".repeat(MAX_ACCEPTED);
    header.push_str("ja");
    assert_eq!(
        Lang::negotiate_in(&available, Some(&header), None).tag(),
        "en"
    );
    let mut header = "xx,".repeat(MAX_ACCEPTED - 1);
    header.push_str("ja");
    assert_eq!(
        Lang::negotiate_in(&available, Some(&header), None).tag(),
        "ja"
    );
}

#[test]
fn the_cookie_beats_accept_language_and_a_value_that_names_nothing_is_ignored() {
    let available = [&EN_DEF, reviewed_japanese()];
    let tag = |accept: Option<&str>, cookie: Option<&str>| {
        Lang::negotiate_in(&available, accept, cookie).tag()
    };
    assert_eq!(tag(Some("en"), Some("ja")), "ja");
    assert_eq!(tag(Some("ja"), Some("en")), "en");
    assert_eq!(tag(Some("ja"), Some("zz")), "ja");
    assert_eq!(tag(None, Some("<script>")), "en");
    assert_eq!(tag(Some("ja"), Some("")), "ja");
}

#[test]
fn negotiation_among_other_languages_uses_only_those() {
    let reviewed = |tag| {
        let def: &'static LangDef = Box::leak(Box::new(LangDef {
            tag,
            endonym: tag,
            dir: Dir::Ltr,
            plural: only_other,
            group_separator: ',',
            review: Review::Reviewed { by: "a, b" },
            entries: &[],
        }));
        def
    };
    let (a, b) = (reviewed("xa"), reviewed("xb"));
    let among = |accept: &str, cookie: Option<&str>| {
        Lang::negotiate_in(&[a, b], Some(accept), cookie).tag()
    };
    assert_eq!(among("xb", None), "xb");
    assert_eq!(among("xb;q=0.2, xa;q=0.8", None), "xa");
    assert_eq!(among("xa", Some("xb")), "xb");
    // English is the fallback of negotiation whether or not the build offers it.
    assert_eq!(among("fr", None), "en");
}

#[test]
fn from_tag_takes_the_primary_subtag() {
    assert_eq!(Lang::from_tag("ja").map(Lang::tag), Some("ja"));
    assert_eq!(Lang::from_tag(" ja-JP ").map(Lang::tag), Some("ja"));
    assert_eq!(Lang::from_tag("EN").map(Lang::tag), Some("en"));
    assert_eq!(Lang::from_tag("jp"), None);
    assert_eq!(Lang::from_tag(""), None);
    assert_eq!(Lang::from_tag("ja;evil=1"), None);
}

// ---------------------------------------------------------------- the helpers of the audit

#[test]
fn placeholders_are_listed_sorted_and_once() {
    assert_eq!(placeholders("{b} and {a} and {b}"), ["a", "b"]);
    assert_eq!(placeholders("no placeholders"), Vec::<&str>::new());
    assert_eq!(placeholders("open {brace"), Vec::<&str>::new());
}

#[test]
fn braces_are_exactly_the_placeholders() {
    for ok in ["", "plain", "{a}", "x {a_1} y {b}", "{count} items"] {
        assert!(braces_ok(ok), "{ok:?}");
    }
    for bad in [
        "{", "}", "{a", "a}", "{}", "{ a }", "{A}", "{1a}", "{{a}}", "{a{b}}", "{a-b}", "}{",
    ] {
        assert!(!braces_ok(bad), "{bad:?}");
    }
}
