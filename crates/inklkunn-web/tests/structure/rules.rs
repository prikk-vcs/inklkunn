//! The structural rules of RFC 003 D-6 (planeter RFC 012 D-2, D-5) over a parsed page. Every rule returns what
//! it found, in words a reviewer can act on; every rule has a planted failure in `main.rs` proving it can fail.

use std::collections::BTreeMap;

use super::dom::{Data, Dom, Handle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub detail: String,
}

fn v(rule: &'static str, detail: impl Into<String>) -> Violation {
    Violation {
        rule,
        detail: detail.into(),
    }
}

/// Every rule, by name. A rule without a planted failure in `main.rs` fails the build.
pub const RULES: &[&str] = &[
    "parse-errors",
    "lang-dir",
    "one-h1",
    "heading-order",
    "skip-link",
    "landmarks",
    "nav-names",
    "form-labels",
    "button-text",
    "tables",
    "images",
    "no-icon-glyphs",
    "aria-hidden-focusable",
    "link-text",
    "title",
    "form-in-phrasing",
    "no-inline-script-or-style",
    "unique-ids",
    "id-references",
];

/// The product, as every title ends with it.
const PRODUCT: &str = "inklkunn";

/// Elements whose content model is phrasing only: a `<form>` below one of them is invalid HTML.
const PHRASING_ONLY: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "button", "cite", "code", "data", "dfn", "em", "h1", "h2",
    "h3", "h4", "h5", "h6", "i", "kbd", "label", "mark", "p", "pre", "q", "s", "samp", "small",
    "span", "strong", "sub", "sup", "time", "u", "var", "legend", "summary",
];

fn all(dom: &Dom) -> Vec<Handle> {
    dom.document.descendants()
}

fn elements(dom: &Dom, tag: &str) -> Vec<Handle> {
    all(dom)
        .into_iter()
        .filter(|n| n.tag() == Some(tag))
        .collect()
}

/// `Some(1..=6)` for `h1` … `h6`.
fn heading_level(tag: &str) -> Option<u8> {
    let rest = tag.strip_prefix('h')?;
    match rest.parse::<u8>() {
        Ok(n @ 1..=6) if rest.len() == 1 => Some(n),
        _ => None,
    }
}

fn is_focusable(n: &Handle) -> bool {
    match n.tag() {
        Some("a") => n.has_attr("href"),
        Some("input") => {
            n.attr("type").map(|t| t.to_ascii_lowercase()) != Some("hidden".into())
                && !n.has_attr("disabled")
        }
        Some("button") | Some("select") | Some("textarea") => !n.has_attr("disabled"),
        Some("summary") => true,
        Some(_) => n
            .attr("tabindex")
            .and_then(|t| t.trim().parse::<i32>().ok())
            .is_some_and(|t| t >= 0),
        None => false,
    }
}

fn title_of(dom: &Dom) -> Vec<String> {
    elements(dom, "title").iter().map(|t| t.text()).collect()
}

/// The words a `<title>` carries (for the suite-level uniqueness check).
pub fn page_title(dom: &Dom) -> Option<String> {
    let t = title_of(dom);
    match t.as_slice() {
        [only] if !only.is_empty() => Some(only.clone()),
        _ => None,
    }
}

fn is_icon_glyph(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x2300..=0x23FF | 0x2190..=0x21FF | 0x25A0..=0x25FF)
}

fn load_vague() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/structure/vague-link-text.txt");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_lowercase)
        .collect()
}

pub fn check(dom: &Dom) -> Vec<Violation> {
    let mut out = Vec::new();
    let nodes = all(dom);
    let html = elements(dom, "html");

    // the parser's own complaints: an unexpected token, a stray end tag, a bad entity …
    let mut errors: Vec<String> = dom.errors.borrow().clone();
    errors.sort();
    errors.dedup();
    for e in errors.iter().take(5) {
        out.push(v("parse-errors", format!("the HTML parser reports: {e}")));
    }

    // lang / dir
    match html.first() {
        Some(h) => {
            if h.attr("lang").is_none_or(|l| l.trim().is_empty()) {
                out.push(v("lang-dir", "<html> has no lang"));
            }
            if !matches!(h.attr("dir").as_deref(), Some("ltr" | "rtl")) {
                out.push(v("lang-dir", "<html> has no dir of ltr or rtl"));
            }
        }
        None => out.push(v(
            "lang-dir",
            "not an HTML document: there is no <html> element",
        )),
    }

    // headings
    let headings: Vec<(Handle, u8)> = nodes
        .iter()
        .filter_map(|n| Some((n.clone(), heading_level(n.tag()?)?)))
        .collect();
    let h1s = headings.iter().filter(|(_, level)| *level == 1).count();
    if h1s != 1 {
        out.push(v(
            "one-h1",
            format!("{h1s} <h1> in the page (want exactly 1)"),
        ));
    }
    let mut prev: Option<u8> = None;
    for (h, level) in &headings {
        match prev {
            None if *level != 1 => out.push(v(
                "heading-order",
                format!("the first heading is <h{level}>, not <h1>"),
            )),
            Some(p) if *level > p + 1 => out.push(v(
                "heading-order",
                format!("<h{p}> is followed by <h{level}> ({:?})", h.text()),
            )),
            _ => {}
        }
        prev = Some(*level);
    }

    // skip link
    let first = nodes.iter().find(|n| is_focusable(n));
    let main_ids: Vec<String> = elements(dom, "main")
        .iter()
        .filter_map(|m| m.attr("id"))
        .collect();
    match first {
        Some(f) if f.tag() == Some("a") && f.attr("href").is_some_and(|h| h.starts_with('#')) => {
            let target = f
                .attr("href")
                .and_then(|h| h.strip_prefix('#').map(str::to_owned))
                .unwrap_or_default();
            if !main_ids.contains(&target) {
                out.push(v(
                    "skip-link",
                    format!("the first link points to #{target}, which is not the <main> id"),
                ));
            }
            if f.text().is_empty() {
                out.push(v("skip-link", "the skip link has no text"));
            }
        }
        Some(f) => out.push(v(
            "skip-link",
            format!(
                "the first focusable element is {}, not a skip link",
                f.describe()
            ),
        )),
        None => out.push(v("skip-link", "no focusable element at all")),
    }

    // landmarks
    let body = elements(dom, "body").into_iter().next();
    let body_children: Vec<Handle> = body
        .as_ref()
        .map(|b| b.element_children())
        .unwrap_or_default();
    if !body_children.iter().any(|c| c.tag() == Some("header")) {
        out.push(v("landmarks", "no <header> (banner) directly in <body>"));
    }
    if !body_children.iter().any(|c| c.tag() == Some("footer")) {
        out.push(v(
            "landmarks",
            "no <footer> (contentinfo) directly in <body>",
        ));
    }
    let mains = elements(dom, "main");
    if mains.len() != 1 {
        out.push(v(
            "landmarks",
            format!("{} <main> elements (want exactly 1)", mains.len()),
        ));
    }
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for nav in elements(dom, "nav") {
        match nav
            .attr("aria-label")
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
        {
            Some(l) => *seen.entry(l).or_default() += 1,
            None => out.push(v(
                "nav-names",
                format!("{} has no accessible name", nav.describe()),
            )),
        }
    }
    for (name, n) in seen {
        if n > 1 {
            out.push(v(
                "nav-names",
                format!("{n} <nav> elements share the name {name:?}"),
            ));
        }
    }

    // form controls and buttons
    let labels = elements(dom, "label");
    for c in nodes
        .iter()
        .filter(|n| matches!(n.tag(), Some("input" | "select" | "textarea")))
    {
        if c.tag() == Some("input")
            && matches!(
                c.attr("type").map(|t| t.to_ascii_lowercase()).as_deref(),
                Some("hidden" | "submit" | "button" | "reset" | "image")
            )
        {
            continue;
        }
        let by_for = c.attr("id").is_some_and(|id| {
            labels
                .iter()
                .any(|l| l.attr("for").as_deref() == Some(&id) && !l.text().is_empty())
        });
        let by_wrap = c
            .ancestors()
            .iter()
            .any(|a| a.tag() == Some("label") && !a.text().is_empty());
        if !(by_for || by_wrap) {
            let why = if c.has_attr("placeholder") {
                " (a placeholder is not a label)"
            } else if c.has_attr("aria-label") {
                " (an aria-label is not a visible label)"
            } else {
                ""
            };
            out.push(v(
                "form-labels",
                format!("{} has no visible <label>{why}", c.describe()),
            ));
        }
    }
    for b in elements(dom, "button") {
        if b.text().is_empty() && b.attr("aria-label").is_none_or(|l| l.trim().is_empty()) {
            out.push(v("button-text", format!("{} has no text", b.describe())));
        }
    }

    // tables
    for t in elements(dom, "table") {
        let caption = t
            .element_children()
            .into_iter()
            .find(|c| c.tag() == Some("caption"));
        if caption.is_none_or(|c| c.text().is_empty()) {
            out.push(v("tables", "a <table> has no <caption>"));
        }
        let ths: Vec<Handle> = t
            .descendants()
            .into_iter()
            .filter(|n| n.tag() == Some("th"))
            .collect();
        if ths.is_empty() {
            out.push(v("tables", "a <table> has no <th> header cell"));
        }
        for th in ths {
            if !matches!(
                th.attr("scope").as_deref(),
                Some("col" | "row" | "colgroup" | "rowgroup")
            ) {
                out.push(v("tables", format!("{} has no scope", th.describe())));
            }
        }
    }

    // images and icons
    for img in elements(dom, "img") {
        if !img.has_attr("alt") && img.attr("aria-hidden").as_deref() != Some("true") {
            out.push(v("images", format!("{} has no alt text", img.describe())));
        }
    }
    for n in &nodes {
        if let Data::Text(t) = &n.data
            && !n.ancestors().iter().any(|a| a.tag() == Some("pre"))
            && let Some(g) = t.borrow().chars().find(|c| is_icon_glyph(*c))
        {
            out.push(v(
                "no-icon-glyphs",
                format!(
                    "the glyph {g:?} is read aloud as text: {:?}",
                    t.borrow().chars().take(40).collect::<String>()
                ),
            ));
        }
    }

    // aria-hidden
    for n in nodes
        .iter()
        .filter(|n| n.attr("aria-hidden").as_deref() == Some("true"))
    {
        if n.descendants().iter().any(is_focusable) {
            out.push(v(
                "aria-hidden-focusable",
                format!(
                    "{} is aria-hidden but is or holds a focusable element",
                    n.describe()
                ),
            ));
        }
    }

    // links
    let vague = load_vague();
    for a in elements(dom, "a")
        .into_iter()
        .filter(|a| a.has_attr("href"))
    {
        let mut text = a.text();
        if text.is_empty() {
            text = a
                .descendants()
                .iter()
                .filter(|n| n.tag() == Some("img"))
                .filter_map(|i| i.attr("alt"))
                .collect::<Vec<_>>()
                .join(" ");
        }
        if text.is_empty()
            && let Some(l) = a.attr("aria-label")
        {
            text = l;
        }
        let bare_id =
            !text.is_empty() && text.chars().all(|c| c.is_ascii_hexdigit()) && text.len() >= 6;
        if text.is_empty() {
            out.push(v("link-text", format!("{} has no text", a.describe())));
        } else if bare_id {
            out.push(v(
                "link-text",
                format!("the link text {text:?} is a bare identifier"),
            ));
        } else if vague.contains(&text.to_lowercase()) {
            out.push(v(
                "link-text",
                format!("the link text {text:?} says nothing out of context"),
            ));
        }
    }

    // title
    match page_title(dom) {
        None => out.push(v("title", "the page has no single, non-empty <title>")),
        Some(t) => {
            // One pattern, `{page} · {product}`, and an identifier is never longer than 12 characters.
            if !t.ends_with(&format!(" · {PRODUCT}")) {
                out.push(v(
                    "title",
                    format!("the title {t:?} does not end with the product (\" · {PRODUCT}\")"),
                ));
            }
            let mut run = 0;
            for c in t.chars().chain(std::iter::once(' ')) {
                if c.is_ascii_hexdigit() && !c.is_ascii_uppercase() {
                    run += 1;
                    continue;
                }
                if run > 12 {
                    out.push(v(
                        "title",
                        format!("the title {t:?} holds an identifier longer than 12 characters"),
                    ));
                    break;
                }
                run = 0;
            }
        }
    }

    // validity and script
    for f in elements(dom, "form") {
        if let Some(p) = f
            .ancestors()
            .iter()
            .find(|a| a.tag().is_some_and(|t| PHRASING_ONLY.contains(&t)))
        {
            out.push(v(
                "form-in-phrasing",
                format!(
                    "a <form> inside {}, whose content model is phrasing only",
                    p.describe()
                ),
            ));
        }
    }
    for n in &nodes {
        if matches!(n.tag(), Some("script" | "style")) {
            out.push(v(
                "no-inline-script-or-style",
                format!("{} element", n.describe()),
            ));
        }
        for a in n.attr_names() {
            if a == "style" || a.starts_with("on") {
                out.push(v(
                    "no-inline-script-or-style",
                    format!("{} carries the attribute {a}", n.describe()),
                ));
            }
        }
    }

    // ids: unique, and every reference to one finds it
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for n in &nodes {
        if let Some(id) = n.attr("id") {
            *ids.entry(id).or_default() += 1;
        }
    }
    for (id, n) in &ids {
        if *n > 1 {
            out.push(v("unique-ids", format!("the id {id:?} is used {n} times")));
        }
    }
    for n in &nodes {
        let mut references: Vec<(&str, String)> = Vec::new();
        if n.tag() == Some("label")
            && let Some(target) = n.attr("for")
        {
            references.push(("for", target));
        }
        for name in ["aria-describedby", "aria-labelledby", "aria-controls"] {
            for target in n.attr(name).unwrap_or_default().split_whitespace() {
                references.push((name, target.to_owned()));
            }
        }
        for (name, target) in references {
            if !ids.contains_key(&target) {
                out.push(v(
                    "id-references",
                    format!(
                        "{} names {name}={target:?}, which is no element",
                        n.describe()
                    ),
                ));
            }
        }
    }
    out
}

/// Landmarks, headings, forms and tables of a page, indented by nesting: what a reader of the page's outline
/// (a screen reader's landmark and heading lists) would find. For the review request's samples.
pub fn outline(dom: &Dom) -> String {
    let mut out = String::new();
    for n in all(dom) {
        let Some(tag) = n.tag() else { continue };
        let depth = n
            .ancestors()
            .iter()
            .filter(|a| {
                matches!(
                    a.tag(),
                    Some("header" | "nav" | "main" | "footer" | "section" | "form" | "table")
                )
            })
            .count();
        let pad = "  ".repeat(depth);
        let text: String = n.text().chars().take(60).collect();
        let line = match tag {
            "header" => "banner (header)".to_owned(),
            "nav" => format!("navigation {:?}", n.attr("aria-label").unwrap_or_default()),
            "main" => format!("main id={:?}", n.attr("id").unwrap_or_default()),
            "footer" => format!("contentinfo (footer) {text:?}"),
            "form" => format!("form action={:?}", n.attr("action").unwrap_or_default()),
            "table" => format!(
                "table caption={:?}",
                n.element_children()
                    .into_iter()
                    .find(|c| c.tag() == Some("caption"))
                    .map(|c| c.text())
                    .unwrap_or_default()
            ),
            "a" if n.attr("href").is_some_and(|h| h.starts_with('#')) => format!(
                "skip link {text:?} -> {}",
                n.attr("href").unwrap_or_default()
            ),
            t if heading_level(t).is_some() => format!("{t} {text:?}"),
            "label" => format!("label {text:?}"),
            "input" | "select" | "textarea" => format!(
                "{tag} name={:?} labelled={}",
                n.attr("name").unwrap_or_default(),
                n.attr("id").is_some()
            ),
            _ => continue,
        };
        out.push_str(&format!("{pad}{line}\n"));
    }
    out
}
