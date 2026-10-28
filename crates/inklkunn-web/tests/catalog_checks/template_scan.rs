//! Literal text in templates: the words between tags, in the attributes a person reads and inside expressions,
//! `|safe` and `autoescape`; with the planted cases that show the scan can fail.

use super::allow::{Allow, allowlist, none, parse_allowlist};
use super::{file_name, read, templates};

/// The attributes whose value is words a person reads (or hears).
const WORDY_ATTRIBUTES: &[&str] = &[
    "alt",
    "title",
    "placeholder",
    "aria-label",
    "aria-description",
    "aria-placeholder",
    "aria-roledescription",
    "aria-valuetext",
    "label",
    "summary",
];

/// Calls whose first string argument names a placeholder (code, not a word).
const PLACEHOLDER_CALLS: &[&str] = &["with(", "link(", "in_lang(", "n("];

fn has_letters(s: &str) -> bool {
    s.chars().any(char::is_alphabetic)
}

/// `s` without the HTML entities (`&nbsp;`, `&#39;`): their letters are not words.
fn without_entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        let (before, from) = rest.split_at(i);
        out.push_str(before);
        let end = from.find(';').filter(|e| {
            from.get(1..*e).is_some_and(|name| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '#')
            })
        });
        match end {
            Some(e) => rest = from.get(e + 1..).unwrap_or(""),
            None => {
                out.push('&');
                rest = from.get(1..).unwrap_or("");
            }
        }
    }
    out.push_str(rest);
    out
}

/// The string literals (double-quoted) of an askama expression or statement, each with the text before it.
fn quoted_in(code: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = code;
    let mut consumed = String::new();
    while let Some(i) = rest.find('"') {
        let (before, from) = rest.split_at(i);
        consumed.push_str(before);
        let body = from.get(1..).unwrap_or("");
        let Some(j) = body.find('"') else { break };
        let (lit, after) = body.split_at(j);
        out.push((consumed.clone(), lit.to_owned()));
        consumed.push('"');
        consumed.push_str(lit);
        consumed.push('"');
        rest = after.get(1..).unwrap_or("");
    }
    out
}

/// What is wrong with the words in a template `src` (its file name is `name`): literal text between tags,
/// literal words in the attributes a person reads, a literal inside an expression that is not a placeholder
/// name, `|safe`, `autoescape` and a branch on a literal.
pub(crate) fn template_findings(name: &str, src: &str, allow: &Allow) -> Vec<String> {
    let mut found = Vec::new();
    // First the askama tags, each replaced by a marker in the HTML that is left.
    let mut html = String::new();
    let mut rest = src;
    loop {
        let next = ["{{", "{%", "{#"]
            .iter()
            .filter_map(|open| rest.find(open).map(|i| (i, *open)))
            .min();
        let Some((i, open)) = next else { break };
        let (before, from) = rest.split_at(i);
        html.push_str(before);
        html.push('\u{1}');
        let close = match open {
            "{{" => "}}",
            "{%" => "%}",
            _ => "#}",
        };
        let Some(j) = from.find(close) else {
            found.push(format!("{name}: an askama tag is not closed"));
            rest = "";
            break;
        };
        let inner = from.get(2..j).unwrap_or("");
        let squashed: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
        match open {
            "{{" => {
                if let Some(k) = squashed.find("|safe") {
                    let expr = squashed.get(..k).unwrap_or("").to_owned();
                    if !allow.safe_ok(name, &expr) {
                        found.push(format!(
                            "{name}: `{expr}|safe` is not allowlisted (never use |safe on catalog output)"
                        ));
                    }
                }
                for (before_lit, lit) in quoted_in(inner) {
                    let is_name = PLACEHOLDER_CALLS
                        .iter()
                        .any(|c| before_lit.trim_end().ends_with(c));
                    if !is_name && has_letters(&lit) && !allow.literal_ok(name, &lit) {
                        found.push(format!("{name}: a literal {lit:?} inside an expression"));
                    }
                }
            }
            "{%" => {
                if squashed.starts_with("autoescape") || squashed.contains("filtersafe") {
                    found.push(format!(
                        "{name}: `{{% {} %}}` switches escaping off",
                        inner.trim()
                    ));
                }
                let names_a_file = {
                    let t = inner.trim_start();
                    t.starts_with("extends") || t.starts_with("include") || t.starts_with("import")
                };
                if !names_a_file {
                    for (_, lit) in quoted_in(inner) {
                        if has_letters(&lit) && !allow.literal_ok(name, &lit) {
                            found.push(format!(
                                "{name}: a literal {lit:?} in a statement (a page does not branch on words)"
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
        rest = from.get(j + 2..).unwrap_or("");
    }
    html.push_str(rest);

    // Then the HTML: every tag's wordy attributes, and the text between tags.
    let mut nodes = Vec::new();
    let mut text = String::new();
    let mut rest = html.as_str();
    loop {
        let Some(open) = rest.find('<') else {
            text.push_str(rest);
            break;
        };
        let (before, from) = rest.split_at(open);
        text.push_str(before);
        nodes.push(std::mem::take(&mut text));
        let Some(close) = from.find('>') else { break };
        let tag = from.get(1..close).unwrap_or("");
        let is_hidden_input = tag.starts_with("input") && tag.contains("type=\"hidden\"");
        let is_button_input = tag.starts_with("input")
            && ["submit", "button", "reset"]
                .iter()
                .any(|t| tag.contains(&format!("type=\"{t}\"")));
        let mut attrs = WORDY_ATTRIBUTES.to_vec();
        if is_button_input || (tag.starts_with("input") && !is_hidden_input) {
            attrs.push("value");
        }
        for attr in attrs {
            let needle = format!(" {attr}=\"");
            for (at, _) in tag.match_indices(&needle) {
                let after = tag.get(at + needle.len()..).unwrap_or("");
                let value = after.split('"').next().unwrap_or("");
                if has_letters(&without_entities(value)) && !value.contains('\u{1}') {
                    found.push(format!("{name}: {attr}=\"{value}\" is literal text"));
                }
            }
        }
        rest = from.get(close + 1..).unwrap_or("");
    }
    nodes.push(text);
    for node in nodes {
        for piece in node.split('\u{1}') {
            let piece = without_entities(piece);
            let t = piece.trim();
            if has_letters(t) && !allow.literal_ok(name, t) {
                found.push(format!("{name}: literal text {t:?}"));
            }
        }
    }
    found
}

pub(crate) fn template_files_findings(allow: &Allow) -> Vec<String> {
    let files = templates();
    assert!(files.len() >= 3, "the templates were not found");
    files
        .iter()
        .flat_map(|p| template_findings(&file_name(p), &read(p), allow))
        .collect()
}

#[test]
fn no_template_holds_user_visible_literal_text_and_none_prints_catalog_output_with_safe() {
    let allow = allowlist();
    let found = template_files_findings(&allow);
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_template_scanner_finds_every_kind_of_literal_and_spares_what_is_not_a_word() {
    let a = none();
    for (what, bad, want) in [
        (
            "a sentence",
            "<h1>Hello there</h1>",
            "literal text \"Hello there\"",
        ),
        (
            "a literal in a paragraph",
            "<p>{{ page.get(Key::A) }} Oops</p>",
            "literal text \"Oops\"",
        ),
        (
            "a placeholder attribute",
            "<input placeholder=\"owner/name\">",
            "placeholder=\"owner/name\"",
        ),
        (
            "an alt text",
            "<img alt=\"A cat\" src=\"/c\">",
            "alt=\"A cat\"",
        ),
        (
            "an aria-label",
            "<nav aria-label=\"Main\">",
            "aria-label=\"Main\"",
        ),
        (
            "a title attribute",
            "<a href=\"/x\" title=\"More\">{{ x }}</a>",
            "title=\"More\"",
        ),
        (
            "a submit value",
            "<input type=\"submit\" value=\"Save\">",
            "value=\"Save\"",
        ),
        (
            "a text input value",
            "<input name=\"q\" value=\"Search\">",
            "value=\"Search\"",
        ),
        (
            "a literal in an expression",
            "<p>{{ \"Oops\" }}</p>",
            "a literal \"Oops\" inside an expression",
        ),
        (
            "a literal that is not a placeholder name",
            "<p>{{ t.msg(Key::A).with(\"n\", \"Oops\") }}</p>",
            "a literal \"Oops\" inside an expression",
        ),
        ("|safe", "{{ t.msg(Key::Y).html()|safe }}", "|safe"),
        ("| safe with spaces", "{{ x | safe }}", "|safe"),
        (
            "autoescape off",
            "{% autoescape none %}{{ x }}{% endautoescape %}",
            "switches escaping off",
        ),
        (
            "a branch on a word",
            "{% if lang == \"ja\" %}x{% endif %}",
            "a literal \"ja\" in a statement",
        ),
        (
            "text in a non-ASCII script",
            "<p>こんにちは</p>",
            "literal text",
        ),
        (
            "text after an entity",
            "<p>&nbsp;Hello</p>",
            "literal text \"Hello\"",
        ),
    ] {
        let found = template_findings("x.html", bad, &a);
        assert!(
            found.iter().any(|l| l.contains(want)),
            "{what}: {bad:?} → {found:?}"
        );
    }
    for (what, ok) in [
        (
            "catalog calls and separators",
            "<p>{{ a }} · <a href=\"/x\">{{ t.get(Key::X) }}</a> / ({{ y }})</p>",
        ),
        (
            "placeholder names",
            "<p>{{ t.msg(Key::X).with(\"version\", v).link(\"a\", l, h).in_lang(\"n\", t, e).n(3) }}</p>",
        ),
        (
            "a hidden input",
            "<input type=\"hidden\" name=\"lang\" value=\"{{ tag }}\">",
        ),
        (
            "an attribute made of a value",
            "<nav aria-label=\"{{ t.get(Key::L) }}\">{{ y }}</nav>",
        ),
        (
            "the names of templates",
            "{% extends \"base.html\" %}{% include \"part.html\" %}",
        ),
        ("an entity", "<p>{{ a }}&nbsp;{{ b }}&#39;</p>"),
        (
            "class names and ids",
            "<main id=\"main\" class=\"skip-link\"><a href=\"#main\">{{ x }}</a></main>",
        ),
        ("a comment", "{# Hello there #}<p>{{ x }}</p>"),
        (
            "a script-free page",
            "<!doctype html><html lang=\"{{ l }}\" dir=\"{{ d }}\"><head><meta charset=\"utf-8\"></head></html>",
        ),
    ] {
        assert_eq!(
            template_findings("y.html", ok, &a),
            Vec::<String>::new(),
            "{what}"
        );
    }
    // An allowlisted expression may be printed with |safe; any other may not.
    let allowed = parse_allowlist(
        "safe :: z.html :: page.sanitized() :: sanitised by ammonia, not catalog output\n",
    );
    assert!(template_findings("z.html", "{{ page.sanitized()|safe }}", &allowed).is_empty());
    assert!(!template_findings("z.html", "{{ page.other()|safe }}", &allowed).is_empty());
    assert!(!template_findings("w.html", "{{ page.sanitized()|safe }}", &allowed).is_empty());
    assert!(allowed.unused().is_empty());
    // An allowlisted word is allowed in its template only.
    let allowed = parse_allowlist("exact :: z.html :: Hello :: a name\n");
    assert!(template_findings("z.html", "<p>Hello</p>", &allowed).is_empty());
    assert!(!template_findings("w.html", "<p>Hello</p>", &allowed).is_empty());
}
