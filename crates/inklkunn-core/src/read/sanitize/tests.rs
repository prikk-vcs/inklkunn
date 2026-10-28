#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

fn clean(html: &str) -> String {
    Sanitizer::profile().clean(html).as_str().to_owned()
}

/// What the five attacks of the acceptance criteria look like in HTML, each in a place of its own.
const ATTACKS: &[(&str, &str)] = &[
    ("a script", "<script>alert(1)</script>"),
    (
        "an event handler",
        "<p onclick=\"alert(2)\" onmouseover=alert(3)>x</p>",
    ),
    ("a javascript: URL", "<a href=\"javascript:alert(4)\">x</a>"),
    (
        "a data: URL",
        "<a href=\"data:text/html;base64,PHNjcmlwdD5hbGVydCg1KTwvc2NyaXB0Pg==\">x</a>",
    ),
    (
        "an iframe",
        "<iframe src=\"https://evil.example/\"></iframe>",
    ),
];

const MARKERS: &[&str] = &[
    "<script",
    "alert",
    "onclick",
    "onmouseover",
    "javascript:",
    "data:",
    "<iframe",
    "evil.example",
];

#[test]
fn a_script_an_event_handler_a_javascript_url_a_data_url_and_an_iframe_are_neutralized() {
    for (what, html) in ATTACKS {
        let out = clean(html);
        for marker in MARKERS {
            assert!(
                !out.to_lowercase().contains(marker),
                "{what}: `{marker}` survived in {out}"
            );
        }
    }
    // The text around an attack survives it.
    assert_eq!(
        clean("<p>before</p><script>alert(1)</script><p>after</p>"),
        "<p>before</p><p>after</p>"
    );
    // A handler on an element that stays is removed from it.
    assert_eq!(
        clean("<span class=\"tag\" onclick=\"alert(1)\">#t</span>"),
        "<span class=\"tag\">#t</span>"
    );
}

/// The module says these elements go *with their contents*: the text inside a frame, an object or a template
/// is a fallback for a browser that cannot show it, and a reader who saw it would be reading something no
/// author wrote for them. (A policy that kept the text would still be safe, which is why only a test that looks
/// for the text can tell the two policies apart.) The list is written out here and not read from
/// `DROPPED_WHOLE`: a test that loops over the list it checks cannot notice an entry removed from it.
#[test]
fn an_element_dropped_whole_takes_its_contents_with_it() {
    for tag in [
        "iframe",
        "math",
        "noembed",
        "noframes",
        "noscript",
        "object",
        "plaintext",
        "script",
        "style",
        "svg",
        "template",
        "textarea",
        "title",
        "xmp",
    ] {
        let out = clean(&format!("<p>keep</p><{tag}>GONE-{tag}</{tag}>"));
        assert!(!out.contains("GONE"), "<{tag}> left its contents: {out}");
        assert!(
            out.contains("<p>keep</p>"),
            "<{tag}> took more than itself: {out}"
        );
    }
}

#[test]
fn url_schemes_are_decided_after_the_browser_would_read_them() {
    for evil in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "  javascript:alert(1)",
        "\tjavascript:alert(1)",
        "java\nscript:alert(1)",
        "&#106;avascript:alert(1)",
        "&#x6A;avascript:alert(1)",
        "jav&#x09;ascript:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html,<script>alert(1)</script>",
        "data:image/svg+xml;base64,PHN2Zz4=",
        "file:///etc/passwd",
        "ftp://example.org/x",
        "blob:https://example.org/uuid",
    ] {
        let out = clean(&format!("<a href=\"{evil}\">x</a>"));
        assert!(!out.contains("href"), "{evil:?} kept an href: {out}");
        assert_eq!(out, "<a>x</a>", "{evil:?}");
    }
    for fine in [
        "https://example.org/a?b=1&c=2#d",
        "http://example.org/",
        "mailto:someone@example.org",
        "tel:+81312345678",
        "/w/Page",
        "./sibling",
        "../up",
        "#heading",
        "?query",
        "Other%20Page",
    ] {
        let out = clean(&format!("<a href=\"{fine}\">x</a>"));
        assert!(out.contains("href="), "{fine:?} lost its href: {out}");
    }
}

#[test]
fn everything_that_can_run_or_embed_is_dropped_with_its_contents() {
    for (html, gone) in [
        ("<style>p{color:red}</style><p>x</p>", "color"),
        (
            "<noscript><img src=x onerror=alert(1)></noscript>",
            "onerror",
        ),
        (
            "<object data=\"x.swf\"><param name=a value=b></object>",
            "x.swf",
        ),
        ("<embed src=\"x.swf\">", "x.swf"),
        ("<svg onload=alert(1)><circle/></svg>", "onload"),
        (
            "<math><mi xlink:href=\"javascript:alert(1)\">x</mi></math>",
            "javascript",
        ),
        ("<template><script>alert(1)</script></template>", "alert"),
        ("<textarea><script>alert(1)</script></textarea>", "alert"),
        (
            "<title>t</title><link rel=stylesheet href=//evil.example/x.css>",
            "evil",
        ),
        (
            "<meta http-equiv=refresh content=\"0;url=https://evil.example\">",
            "evil",
        ),
        ("<base href=\"https://evil.example/\">", "evil"),
        (
            "<form action=\"https://evil.example/\"><button>go</button></form>",
            "evil",
        ),
        (
            "<video src=x onerror=alert(1)></video><audio src=x></audio>",
            "onerror",
        ),
        (
            "<iframe srcdoc=\"<script>alert(1)</script>\"></iframe>",
            "alert",
        ),
    ] {
        let out = clean(html);
        assert!(!out.contains(gone), "{html}: {out}");
        assert!(
            ![
                "<style",
                "<object",
                "<embed",
                "<svg",
                "<math",
                "<template",
                "<textarea",
                "<link",
                "<meta",
                "<base",
                "<form",
                "<video",
                "<audio",
                "<iframe"
            ]
            .iter()
            .any(|t| out.contains(t)),
            "{html}: {out}"
        );
    }
    // Comments (and what hides in them) go.
    assert_eq!(clean("a<!-- <script>x</script> -->b"), "ab");
    // Inline style is not kept, and neither are the attributes that load or navigate.
    let out = clean(
        "<p style=\"color:red\" target=\"_blank\" srcset=\"x 1x\" formaction=\"x\" name=\"n\">x</p>",
    );
    assert_eq!(out, "<p>x</p>");
}

#[test]
fn the_renderers_own_constructs_pass_untouched() {
    let html = "<h2 id=\"intro\">Intro</h2>\n<p id=\"^p1\">See <a class=\"wikilink\" href=\"/w/Page\" data-wiki-target=\"Page\">Page</a>, \
                <a class=\"wikilink wikilink-missing\" href=\"/w/-/create?title=X\" data-wiki-target=\"X\">X</a> and \
                <a class=\"external\" href=\"https://example.org\" rel=\"nofollow ugc\">out</a> <span class=\"tag\" data-tag=\"t\">#t</span> \
                <kbd>Ctrl</kbd>.</p>\n<aside class=\"callout callout-tip\" data-callout=\"TIP\" data-fold=\"+\"><p class=\"callout-title\">Title</p>\n</aside>\n\
                <div class=\"directive directive-toc\" data-directive=\"toc\" data-attr-depth=\"2\">\n</div>\n\
                <pre><code class=\"language-rust\">fn main() {}\n</code></pre>\n\
                <ol start=\"3\"><li><input disabled=\"\" type=\"checkbox\" checked=\"\">done</li></ol>\n\
                <p><img class=\"wikilink-embed\" src=\"/att/d.png\" alt=\"d\" width=\"300\"></p>\n\
                <div class=\"transclusion\" data-transclusion=\"B\" data-transclusion-depth=\"1\">\n\
                <p class=\"transclusion-source\"><a class=\"wikilink\" href=\"/w/B\">From B</a></p>\n</div>\n\
                <p class=\"transclusion-refused\" data-transclusion-refusal=\"cycle\" role=\"note\">A → B → A</p>\n\
                <table id=\"^t\"><thead><tr><th>a</th></tr></thead><tbody>\n<tr><td>1</td></tr>\n</tbody></table>";
    let out = clean(html);
    // The sanitizer re-serializes (a void tag has no slash, an empty attribute is written `=\"\"`), so the
    // comparison is of what is there: every tag, attribute name and value of the input is in the output.
    for needle in [
        "<h2 id=\"intro\">",
        "<p id=\"^p1\">",
        "class=\"wikilink\" href=\"/w/Page\" data-wiki-target=\"Page\"",
        "class=\"wikilink wikilink-missing\"",
        "class=\"external\" href=\"https://example.org\" rel=\"nofollow ugc\"",
        "<span class=\"tag\" data-tag=\"t\">",
        "<kbd>Ctrl</kbd>",
        "<aside class=\"callout callout-tip\" data-callout=\"TIP\" data-fold=\"+\">",
        "<p class=\"callout-title\">",
        "class=\"directive directive-toc\" data-directive=\"toc\" data-attr-depth=\"2\"",
        "<code class=\"language-rust\">",
        "<ol start=\"3\">",
        "checked=\"\"",
        "<img class=\"wikilink-embed\" src=\"/att/d.png\" alt=\"d\" width=\"300\">",
        "<div class=\"transclusion\" data-transclusion=\"B\" data-transclusion-depth=\"1\">",
        "<p class=\"transclusion-source\">",
        "role=\"note\"",
        "A → B → A",
        "<table id=\"^t\">",
    ] {
        assert!(out.contains(needle), "lost `{needle}` in {out}");
    }
}

#[test]
fn classes_ids_roles_and_relations_are_the_renderers_vocabulary_and_nothing_else() {
    // Classes the renderer does not write are dropped, the ones it does are kept, in order.
    assert_eq!(
        clean("<p class=\"site-header verified wikilink callout-tip language-c++ callout-\">x</p>"),
        "<p class=\"wikilink callout-tip language-c++\">x</p>"
    );
    assert_eq!(clean("<p class=\"site-header badge\">x</p>"), "<p>x</p>");
    // An id is one token.
    assert_eq!(clean("<p id=\"a b\">x</p>"), "<p>x</p>");
    assert_eq!(clean("<p id=\"\">x</p>"), "<p>x</p>");
    assert_eq!(clean("<p id=\"^ok-1\">x</p>"), "<p id=\"^ok-1\">x</p>");
    assert_eq!(
        clean(&format!("<p id=\"{}\">x</p>", "a".repeat(201))),
        "<p>x</p>"
    );
    // A role is `note`: no page makes a region a button, a dialog or a status.
    assert_eq!(clean("<p role=\"button\">x</p>"), "<p>x</p>");
    assert_eq!(clean("<p role=\"note\">x</p>"), "<p role=\"note\">x</p>");
    // A link's relation keeps the tokens that cost a reader nothing.
    assert_eq!(
        clean("<a href=\"/x\" rel=\"opener me nofollow ugc\">x</a>"),
        "<a href=\"/x\" rel=\"nofollow ugc\">x</a>"
    );
    assert_eq!(
        clean("<a href=\"/x\" rel=\"opener\">x</a>"),
        "<a href=\"/x\">x</a>"
    );
    // Direction and language are plain.
    assert_eq!(
        clean("<p dir=\"rtl\" lang=\"ja\">x</p>"),
        "<p dir=\"rtl\" lang=\"ja\">x</p>"
    );
    assert_eq!(
        clean("<p dir=\"expression(1)\" lang=\"j a\">x</p>"),
        "<p>x</p>"
    );
    // Numbers are numbers.
    assert_eq!(
        clean("<ol start=\"3\"><li>x</li></ol>"),
        "<ol start=\"3\"><li>x</li></ol>"
    );
    assert_eq!(
        clean("<ol start=\"x\"><li>x</li></ol>"),
        "<ol><li>x</li></ol>"
    );
    assert_eq!(
        clean("<img src=\"/a.png\" width=\"1e9\">"),
        "<img src=\"/a.png\">"
    );
    // Data and accessibility attributes are kept, scripts' attributes are not.
    assert_eq!(
        clean("<span data-x=\"1\" aria-label=\"l\" onfocus=\"x\" ondata=\"y\">x</span>"),
        "<span data-x=\"1\" aria-label=\"l\">x</span>"
    );
}

#[test]
fn a_remote_image_is_never_loaded_on_a_readers_behalf() {
    for remote in [
        "https://tracker.example/pixel.gif",
        "http://tracker.example/pixel.gif",
        "//tracker.example/pixel.gif",
        "\\\\tracker.example\\pixel.gif",
        "data:image/gif;base64,R0lGODlhAQABAAAAACw=",
        "javascript:alert(1)",
    ] {
        let out = clean(&format!("<img src=\"{remote}\" alt=\"alt text\">"));
        assert!(!out.contains("src="), "{remote}: {out}");
        assert!(
            out.contains("alt=\"alt text\""),
            "the alt text stays: {out}"
        );
    }
    for local in [
        "/att/diagram.svg",
        "attachments/a.png",
        "./a.png",
        "../a.png",
        "a.png?v=1",
    ] {
        let out = clean(&format!("<img src=\"{local}\" alt=\"a\">"));
        assert!(out.contains("src="), "{local}: {out}");
    }
}

#[test]
fn the_one_form_control_is_a_disabled_checkbox() {
    for html in [
        "<input type=\"text\" value=\"secret\" name=\"n\" form=\"f\" autofocus>",
        "<input type=\"password\">",
        "<input>",
        "<input type=\"checkbox\">",
        "<input type=\"submit\" formaction=\"https://evil.example/\">",
    ] {
        let out = clean(html);
        assert!(
            out == "<input type=\"checkbox\" disabled=\"\">"
                || out == "<input disabled=\"\" type=\"checkbox\">",
            "{html}: {out}"
        );
    }
    let checked = clean("<input type=\"checkbox\" disabled checked>");
    assert!(checked.contains("checked=\"\""), "{checked}");
    assert!(checked.contains("type=\"checkbox\"") && checked.contains("disabled=\"\""));
    for (html, tag) in [
        ("<button>x</button>", "<button"),
        ("<select><option>x</option></select>", "<select"),
        ("<label for=a>x</label>", "<label"),
    ] {
        let out = clean(html);
        assert!(!out.contains(tag), "{html}: {out}");
    }
}

#[test]
fn nothing_outside_the_vocabulary_survives_and_the_raw_html_subset_is_empty() {
    let none: &[&str] = &[];
    assert_eq!(HTML_SUBSET, none, "the manifest says html_subset: []");
    // Elements Markdown cannot make and the renderer does not write: unwrapped, their text kept.
    for tag in [
        "b", "i", "u", "font", "center", "marquee", "blink", "details", "summary", "figure", "nav",
        "section", "article", "header", "footer", "dl", "dt", "dd", "sub", "abbr", "mark", "ins",
        "time", "wbr",
    ] {
        let out = clean(&format!("<{tag}>x</{tag}>"));
        assert_eq!(out, "x", "<{tag}> is not in the profile's vocabulary");
    }
    // The ones that are, stay (the table's own parts only inside a table, where an HTML parser keeps them).
    let in_table = ["tbody", "thead", "tr", "td", "th"];
    for tag in ELEMENTS
        .iter()
        .filter(|t| !matches!(**t, "br" | "hr" | "img" | "input") && !in_table.contains(*t))
    {
        let out = clean(&format!("<{tag}>x</{tag}>"));
        assert!(
            out.contains(&format!("<{tag}")),
            "<{tag}> is the renderer's: {out}"
        );
    }
    let table = clean(
        "<table><thead><tr><th>h</th></tr></thead><tbody><tr><td>c</td></tr></tbody></table>",
    );
    for tag in in_table {
        assert!(table.contains(&format!("<{tag}")), "<{tag}> in {table}");
    }
}

#[test]
fn what_is_dropped_whole_is_never_also_allowed() {
    // ammonia refuses to clean (it panics) when a tag is both allowed and dropped with its contents; the
    // vocabulary and the raw subset must stay clear of the drop list, and say so before a page is cleaned.
    for tag in DROPPED_WHOLE {
        assert!(!ELEMENTS.contains(tag), "{tag}");
        assert!(!HTML_SUBSET.contains(tag), "{tag}");
    }
}

#[test]
fn the_same_input_is_the_same_bytes_from_every_instance_of_the_sanitizer() {
    // ENF-5 compares a page rebuilt after a restart with the one cached before it, and every process has
    // hash maps of its own. The renderer's checkboxes carry their attributes already, so the policy keeps
    // them in the renderer's order whichever instance cleans them.
    let html = "<ul><li><input disabled=\"\" type=\"checkbox\">open</li>\
                <li><input disabled=\"\" type=\"checkbox\" checked=\"\">done</li></ul>";
    let first = Sanitizer::profile().clean(html);
    assert_eq!(first.as_str(), html);
    for _ in 0..50 {
        assert_eq!(Sanitizer::profile().clean(html), first);
    }
}

#[test]
fn a_second_pass_changes_nothing() {
    let sanitizer = Sanitizer::profile();
    let mut inputs: Vec<String> = ATTACKS.iter().map(|(_, h)| (*h).to_owned()).collect();
    inputs.push("<p class=\"wikilink x\" id=\"^a\" data-k=\"v\">t <a href=\"/x\" rel=\"nofollow\">l</a><img src=\"https://x/y.png\" alt=\"a\"></p>".to_owned());
    inputs.push(
        "<ul><li><input type=text>x</li></ul><table><tr><td style=\"a\">1</td></tr></table>"
            .to_owned(),
    );
    inputs.push("<<script>script>alert(1)<</script>/script>".to_owned());
    for input in inputs {
        let once = sanitizer.clean(&input);
        let twice = sanitizer.clean(once.as_str());
        assert_eq!(once, twice, "{input}");
    }
}

#[test]
fn plain_text_becomes_inert_html() {
    let s = Sanitizer::profile();
    let hostile = "<script>alert(1)</script> & <img src=x onerror=alert(2)> \"quoted\" 'single'";
    let out = s.text(hostile);
    assert!(!out.as_str().contains('<'), "{out}");
    assert!(!out.as_str().contains("<script"));
    // Whatever a parser makes of it, it is text: cleaning it again leaves no markup in it either.
    let again = s.clean(out.as_str());
    assert!(!again.as_str().contains('<'), "{again}");
    assert!(again.as_str().contains("&lt;script&gt;"), "{again}");
    // Text in any script survives as itself; only what means something to a parser is written as an entity.
    let japanese = s.text("日本語 & more");
    assert!(
        japanese.as_str().starts_with("日本語") && japanese.as_str().contains("&amp;"),
        "{japanese}"
    );
}

#[test]
fn safe_html_prints_as_its_text() {
    let s = Sanitizer::profile();
    let h = s.clean("<p>x</p>");
    assert_eq!(h.to_string(), "<p>x</p>");
    assert_eq!(h.as_str(), "<p>x</p>");
}
