//! RFC 003 D-6 (planeter RFC 012 D-2/D-5), the structural half: every page and state the web crate can produce
//! is rendered through the real router and parsed with an HTML5 parser (`html5ever`, a dev-dependency
//! already in the lock through `ammonia`); structural rules run over each, in English and in Japanese. Each
//! rule has a planted failure proving it can fail. No prikk is needed.
//!
//! `STRUCTURE_REPORT=<file>` writes the per-rule table (and every violation) for the review request;
//! `STRUCTURE_DUMP=<dir>` saves every state's page as `<dir>/<language>-<id>.html`; `STRUCTURE_OUTLINES=<file>`
//! writes each page's landmark and heading outline.
//!
//! A later task that adds a page adds its states to `VISITS`: the rules then run on it in both languages, and
//! a page that cannot pass them cannot be added. Japanese is reached the way a reader reaches a preview, with
//! the cookie the language page sets.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;
mod dom;
mod rules;
mod wiki_states;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site;
use tower::ServiceExt as _;

use rules::{RULES, Violation};

// ---------------------------------------------------------------------------------------------------------
// States

/// One rendered state. `identity` says which page it is: two states with different identities must have
/// different titles; two with the same identity are the same page (and must have the same title).
struct State {
    id: String,
    identity: &'static str,
    status: u16,
    html: String,
}

/// What a request produced that is not a page: counted, never silently dropped.
#[derive(Default)]
struct Skipped(Vec<String>);

struct Out {
    states: Vec<State>,
    skipped: Skipped,
}

fn router() -> Router {
    site(
        &WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
    )
    .unwrap()
}

/// One request a state is made of.
struct Visit {
    id: &'static str,
    identity: &'static str,
    method: &'static str,
    uri: &'static str,
    headers: &'static [(&'static str, &'static str)],
    body: &'static str,
}

const SAME_SITE: &[(&str, &str)] = &[
    ("content-type", "application/x-www-form-urlencoded"),
    ("origin", "http://localhost:8080"),
    ("sec-fetch-site", "same-origin"),
];

const CROSS_SITE: &[(&str, &str)] = &[
    ("content-type", "application/x-www-form-urlencoded"),
    ("origin", "https://evil.example"),
    ("sec-fetch-site", "cross-site"),
];

const NOT_A_FORM: &[(&str, &str)] = &[("content-type", "text/plain")];

/// Every state of every page the crate has. The identities are the pages: a language page with or without a
/// place to go back to is one page; a 404 for any address is one page; the two refusals are two pages.
const VISITS: &[Visit] = &[
    Visit {
        id: "language",
        identity: "language",
        method: "GET",
        uri: "/-/lang",
        headers: &[],
        body: "",
    },
    Visit {
        id: "language-with-back",
        identity: "language",
        method: "GET",
        uri: "/-/lang?next=/handbook/Home",
        headers: &[],
        body: "",
    },
    Visit {
        id: "language-bad-back",
        identity: "language",
        method: "GET",
        uri: "/-/lang?next=//evil.example",
        headers: &[],
        body: "",
    },
    Visit {
        id: "not-found",
        identity: "not-found",
        method: "GET",
        uri: "/nothing/here",
        headers: &[],
        body: "",
    },
    Visit {
        id: "not-found-query",
        identity: "not-found",
        method: "GET",
        uri: "/a/b/c?x=1&y=2",
        headers: &[],
        body: "",
    },
    Visit {
        id: "not-found-root",
        identity: "not-found",
        method: "GET",
        uri: "/",
        headers: &[],
        body: "",
    },
    Visit {
        id: "not-found-asset",
        identity: "not-found",
        method: "GET",
        uri: "/-/assets/site-0000000000000000.css",
        headers: &[],
        body: "",
    },
    Visit {
        id: "language-refused",
        identity: "language-refused",
        method: "POST",
        uri: "/-/lang",
        headers: SAME_SITE,
        body: "lang=xx&next=%2F",
    },
    Visit {
        id: "language-refused-empty",
        identity: "language-refused",
        method: "POST",
        uri: "/-/lang",
        headers: SAME_SITE,
        body: "",
    },
    Visit {
        id: "language-refused-type",
        identity: "language-refused",
        method: "POST",
        uri: "/-/lang",
        headers: NOT_A_FORM,
        body: "lang=ja",
    },
    Visit {
        id: "form-refused",
        identity: "form-refused",
        method: "POST",
        uri: "/-/lang",
        headers: CROSS_SITE,
        body: "lang=ja&next=%2F",
    },
];

/// A reader whose browser asks for nothing in particular; and a reader who chose the Japanese preview on the
/// language page (the cookie it sets). A preview is never served for a browser's setting alone.
const ENGLISH: &[(&str, &str)] = &[];
const JAPANESE: &[(&str, &str)] = &[("cookie", "inklkunn_lang=ja")];

/// How many of the states above are pages.
const EXPECTED_PAGES: usize = VISITS.len() + wiki_states::SCENARIOS.len();

async fn build_states(extra: &[(&str, &str)]) -> Out {
    let router = router();
    let mut out = Out {
        states: Vec::new(),
        skipped: Skipped::default(),
    };
    for visit in VISITS {
        let mut b = Request::builder()
            .method(visit.method)
            .uri(visit.uri)
            .header(header::HOST, "localhost:8080");
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        for (k, v) in visit.headers {
            b = b.header(*k, *v);
        }
        let res = router
            .clone()
            .oneshot(b.body(Body::from(visit.body)).unwrap())
            .await
            .unwrap();
        let status = res.status().as_u16();
        let is_html = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|c| c.starts_with("text/html"));
        let bytes = axum::body::to_bytes(res.into_body(), 16 << 20)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes).into_owned();
        if is_html && html.contains("<html") {
            out.states.push(State {
                id: visit.id.to_owned(),
                identity: visit.identity,
                status,
                html,
            });
        } else {
            out.skipped.0.push(format!(
                "{}: status {status}, {} bytes, not an HTML document",
                visit.id,
                bytes.len()
            ));
        }
    }
    wiki_states::build(extra, &mut out).await;
    out
}

// ---------------------------------------------------------------------------------------------------------
// Running the rules

struct Report {
    /// rule → (state id → details)
    by_rule: BTreeMap<&'static str, BTreeMap<String, Vec<String>>>,
    checked: usize,
    /// (state id, status, title)
    listing: Vec<(String, u16, String)>,
}

/// Titles: a page has one, and two states that are different pages do not share it.
fn title_violations(states: &[&State], titles: &[Option<String>]) -> Vec<(String, Violation)> {
    let mut out = Vec::new();
    let mut by_title: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut by_identity: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (s, t) in states.iter().zip(titles) {
        if let Some(t) = t {
            by_title.entry(t.clone()).or_default().insert(s.identity);
            by_identity.entry(s.identity).or_default().insert(t.clone());
        }
    }
    for (title, identities) in &by_title {
        if identities.len() > 1 {
            let ids: Vec<&str> = states
                .iter()
                .filter(|s| identities.contains(s.identity))
                .map(|s| s.id.as_str())
                .collect();
            out.push((
                ids.join(", "),
                Violation {
                    rule: "title",
                    detail: format!(
                        "{title:?} is the title of different pages ({})",
                        identities.iter().copied().collect::<Vec<_>>().join(" | ")
                    ),
                },
            ));
        }
    }
    for (identity, titles) in &by_identity {
        if titles.len() > 1 {
            out.push((
                (*identity).to_owned(),
                Violation {
                    rule: "title",
                    detail: format!("the same page has different titles: {titles:?}"),
                },
            ));
        }
    }
    out
}

fn run(states: &[&State]) -> Report {
    let mut by_rule: BTreeMap<&'static str, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut titles = Vec::new();
    let mut listing = Vec::new();
    for s in states {
        let dom = dom::parse(&s.html);
        let title = rules::page_title(&dom);
        listing.push((s.id.clone(), s.status, title.clone().unwrap_or_default()));
        titles.push(title);
        for v in rules::check(&dom) {
            by_rule
                .entry(v.rule)
                .or_default()
                .entry(s.id.clone())
                .or_default()
                .push(v.detail);
        }
    }
    for (ids, v) in title_violations(states, &titles) {
        by_rule
            .entry(v.rule)
            .or_default()
            .entry(ids)
            .or_default()
            .push(v.detail);
    }
    Report {
        by_rule,
        checked: states.len(),
        listing,
    }
}

fn print(label: &str, rep: &Report, skipped: &Skipped) -> String {
    let mut s = format!(
        "== {label}: {} states checked, {} not pages\n",
        rep.checked,
        skipped.0.len()
    );
    for rule in RULES {
        let found = rep.by_rule.get(rule);
        let states = found.map_or(0, |m| m.len());
        let items: usize = found.map_or(0, |m| m.values().map(Vec::len).sum());
        s.push_str(&format!(
            "{rule:<28} {states:>3} states  {items:>4} findings\n"
        ));
    }
    s.push_str("-- not pages:\n");
    for x in &skipped.0 {
        s.push_str(&format!("   {x}\n"));
    }
    s.push_str("-- states (id, status, title):\n");
    for (id, status, title) in &rep.listing {
        s.push_str(&format!("   {id:<26} {status}  {title}\n"));
    }
    s.push_str("-- findings:\n");
    for (rule, m) in &rep.by_rule {
        for (id, details) in m {
            let mut seen = BTreeSet::new();
            for d in details {
                if seen.insert(d) {
                    s.push_str(&format!("   [{rule}] {id}: {d}\n"));
                }
            }
        }
    }
    s
}

fn append_to(env: &str, text: &str) {
    use std::io::Write;
    if let Ok(path) = std::env::var(env) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(f, "{text}").unwrap();
    }
}

fn finish(label: &str, language: &str, out: &Out) {
    let all: Vec<&State> = out.states.iter().collect();
    let rep = run(&all);
    let text = print(label, &rep, &out.skipped);
    eprintln!("{text}");
    if let Ok(dir) = std::env::var("STRUCTURE_DUMP") {
        std::fs::create_dir_all(&dir).unwrap();
        for s in &out.states {
            std::fs::write(
                Path::new(&dir).join(format!("{language}-{}.html", s.id)),
                &s.html,
            )
            .unwrap();
        }
    }
    for s in &out.states {
        append_to(
            "STRUCTURE_OUTLINES",
            &format!(
                "### {language}-{} ({})\n{}",
                s.id,
                s.status,
                rules::outline(&dom::parse(&s.html))
            ),
        );
    }
    append_to("STRUCTURE_REPORT", &text);
    assert!(
        rep.by_rule.is_empty(),
        "{label}: structural findings (the table is above)"
    );
}

#[tokio::test]
async fn every_state_passes_every_rule_in_english() {
    let out = build_states(ENGLISH).await;
    assert_eq!(
        out.skipped.0,
        Vec::<String>::new(),
        "a state stopped being a page"
    );
    assert_eq!(out.states.len(), EXPECTED_PAGES);
    finish("English", "en", &out);
    for s in &out.states {
        assert!(
            s.html.contains("<html lang=\"en\" dir=\"ltr\">"),
            "{} is not rendered in English",
            s.id
        );
    }
}

/// The Japanese preview renders every state with the same structure: `lang="ja"`, one `h1`, landmarks, the
/// switcher in the footer, the title pattern.
#[tokio::test]
async fn every_state_passes_every_rule_in_japanese() {
    let out = build_states(JAPANESE).await;
    assert_eq!(
        out.skipped.0,
        Vec::<String>::new(),
        "a state stopped being a page"
    );
    assert_eq!(out.states.len(), EXPECTED_PAGES);
    finish("Japanese (preview)", "ja", &out);
    for s in &out.states {
        assert!(
            s.html.contains("<html lang=\"ja\" dir=\"ltr\">"),
            "{} is not rendered in Japanese",
            s.id
        );
    }
}

/// The statuses are the ones the pages promise, so a page that answers the wrong status is not hidden by the
/// rules passing on it.
#[tokio::test]
async fn the_states_answer_their_statuses() {
    let out = build_states(ENGLISH).await;
    let status: BTreeMap<&str, u16> = out
        .states
        .iter()
        .map(|s| (s.id.as_str(), s.status))
        .collect();
    for (id, want) in [
        ("language", 200),
        ("language-with-back", 200),
        ("language-bad-back", 200),
        ("not-found", 404),
        ("not-found-query", 404),
        ("not-found-root", 404),
        ("not-found-asset", 404),
        ("language-refused", 422),
        ("language-refused-empty", 422),
        ("language-refused-type", 415),
        ("form-refused", 403),
    ] {
        assert_eq!(status.get(id), Some(&want), "{id}");
    }
}

/// The two runs together: no two different pages share a title, and the same page has one title.
#[tokio::test]
async fn titles_are_distinct_for_distinct_pages_and_one_per_page() {
    let en = build_states(ENGLISH).await;
    let ja = build_states(JAPANESE).await;
    for out in [&en, &ja] {
        let all: Vec<&State> = out.states.iter().collect();
        let rep = run(&all);
        assert!(
            !rep.by_rule.contains_key("title"),
            "{:#?}",
            rep.by_rule.get("title")
        );
        let distinct: BTreeSet<&str> = rep.listing.iter().map(|(_, _, t)| t.as_str()).collect();
        let identities: BTreeSet<&str> = out.states.iter().map(|s| s.identity).collect();
        assert_eq!(distinct.len(), identities.len(), "{:?}", rep.listing);
    }
}

// ---------------------------------------------------------------------------------------------------------
// Each rule can fail

/// A page that passes every rule; each planted failure below breaks exactly one thing in it.
const GOOD: &str = r##"<!doctype html><html lang="en" dir="ltr"><head><meta charset="utf-8"><title>Page · inklkunn</title></head><body>
<a class="skip" href="#main">Skip to the main content</a>
<header><p>inklkunn</p></header>
<main id="main"><h1>Heading</h1><h2>Sub</h2>
<form method="get" action="/search"><label for="q">Address</label><input id="q" name="q" aria-describedby="hint"><p id="hint">A hint</p><button type="submit">Open</button></form>
<table><caption>Files</caption><thead><tr><th scope="col">Name</th></tr></thead><tbody><tr><th scope="row">a</th></tr></tbody></table>
<p><a href="/a">Download main.rs</a> <img src="/i" alt="A picture"></p>
</main><footer><nav aria-label="Language"><ul><li>English</li></ul></nav><p>inklkunn 0.0.0</p></footer></body></html>"##;

fn rules_found(html: &str) -> BTreeSet<&'static str> {
    rules::check(&dom::parse(html))
        .into_iter()
        .map(|v| v.rule)
        .collect()
}

#[test]
fn the_good_document_passes_every_rule() {
    let found = rules::check(&dom::parse(GOOD));
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn every_rule_has_a_planted_failure_and_each_plant_trips_its_rule() {
    // (rule, what to replace, with what)
    let plants: &[(&str, &str, &str)] = &[
        ("parse-errors", "<p>inklkunn</p>", "<p>inklkunn</div></p>"),
        (
            "parse-errors",
            "<h1>Heading</h1>",
            r#"<h1 class="a" class="b">Heading</h1>"#,
        ),
        ("lang-dir", r#" lang="en" dir="ltr""#, ""),
        ("lang-dir", r#" dir="ltr""#, ""),
        ("lang-dir", r#" dir="ltr""#, r#" dir="sideways""#),
        ("one-h1", "<h1>Heading</h1>", "<h1>Heading</h1><h1>Two</h1>"),
        ("one-h1", "<h1>Heading</h1><h2>Sub</h2>", "<h2>Sub</h2>"),
        ("heading-order", "<h2>Sub</h2>", "<h3>Sub</h3>"),
        (
            "skip-link",
            r##"<a class="skip" href="#main">Skip to the main content</a>"##,
            "",
        ),
        ("skip-link", r##"href="#main""##, r##"href="#elsewhere""##),
        ("skip-link", ">Skip to the main content</a>", "></a>"),
        (
            "skip-link",
            r##"<a class="skip" href="#main">Skip to the main content</a>"##,
            r##"<a href="/other">Other</a><a class="skip" href="#main">Skip to the main content</a>"##,
        ),
        ("landmarks", "</main><footer>", "</main><div>"),
        (
            "landmarks",
            "<header><p>inklkunn</p></header>",
            "<div><p>inklkunn</p></div>",
        ),
        (
            "landmarks",
            "</main><footer>",
            "</main><main>x</main><footer>",
        ),
        ("landmarks", r#"<main id="main">"#, r#"<div id="main">"#),
        ("nav-names", r#"<nav aria-label="Language">"#, "<nav>"),
        (
            "nav-names",
            "</nav><p>inklkunn 0.0.0</p>",
            r#"</nav><nav aria-label="Language"><ul><li>Deutsch</li></ul></nav><p>inklkunn 0.0.0</p>"#,
        ),
        ("form-labels", r#"<label for="q">Address</label>"#, ""),
        (
            "form-labels",
            r#"<label for="q">Address</label><input id="q" name="q" aria-describedby="hint">"#,
            r#"<input name="q" placeholder="Address">"#,
        ),
        (
            "form-labels",
            r#"<label for="q">Address</label><input id="q" name="q" aria-describedby="hint">"#,
            r#"<input name="q" aria-label="Address">"#,
        ),
        ("button-text", ">Open</button>", "></button>"),
        ("tables", "<caption>Files</caption>", ""),
        ("tables", r#"<th scope="col">"#, "<th>"),
        (
            "tables",
            r#"<thead><tr><th scope="col">Name</th></tr></thead><tbody><tr><th scope="row">a</th></tr></tbody>"#,
            "<tr><td>a</td></tr>",
        ),
        ("images", r#" alt="A picture""#, ""),
        ("no-icon-glyphs", "A hint</p>", "📁 A hint</p>"),
        (
            "aria-hidden-focusable",
            "<p><a href=\"/a\">",
            r#"<p><a aria-hidden="true" href="/z">z</a> <a href="/a">"#,
        ),
        ("link-text", "Download main.rs", "click here"),
        ("link-text", "Download main.rs", "0123456789ab"),
        ("link-text", "Download main.rs", ""),
        ("title", "<title>Page · inklkunn</title>", ""),
        ("title", "<title>Page · inklkunn</title>", "<title></title>"),
        (
            "title",
            "<title>Page · inklkunn</title>",
            "<title>Page</title>",
        ),
        (
            "title",
            "<title>Page · inklkunn</title>",
            "<title>change 39fd67ec44546355daa76edb6405d475724d2fc685eb97a5ce928f0c003a1b36 · inklkunn</title>",
        ),
        (
            "title",
            "<title>Page · inklkunn</title>",
            "<title>A</title><title>B · inklkunn</title>",
        ),
        (
            "form-in-phrasing",
            "</main>",
            "<span><form></form></span></main>",
        ),
        (
            "no-inline-script-or-style",
            "</main>",
            "<script>x()</script></main>",
        ),
        (
            "no-inline-script-or-style",
            "</main>",
            "<style>p{}</style></main>",
        ),
        (
            "no-inline-script-or-style",
            r#"<p id="hint">"#,
            r#"<p id="hint" style="color:red">"#,
        ),
        (
            "no-inline-script-or-style",
            r#"<p id="hint">"#,
            r#"<p id="hint" onclick="x()">"#,
        ),
        ("unique-ids", r#"<p id="hint">"#, r#"<p id="q">"#),
        (
            "id-references",
            r#"aria-describedby="hint""#,
            r#"aria-describedby="nope""#,
        ),
        (
            "id-references",
            r#"<label for="q">"#,
            r#"<label for="zzz">"#,
        ),
    ];
    for (rule, from, to) in plants {
        assert!(
            GOOD.contains(from),
            "the plant for {rule} does not apply: {from:?}"
        );
        let bad = GOOD.replacen(from, to, 1);
        let found = rules_found(&bad);
        assert!(
            found.contains(rule),
            "planted failure for {rule} ({from:?} -> {to:?}) was not found; found {found:?}"
        );
    }
    let planted: BTreeSet<&str> = plants.iter().map(|p| p.0).collect();
    for rule in RULES {
        assert!(planted.contains(rule), "rule {rule} has no planted failure");
    }
    assert_eq!(
        RULES.iter().copied().collect::<BTreeSet<_>>().len(),
        RULES.len(),
        "a rule is listed twice"
    );
}

/// The rules read real output, not only the planted document: break a real page and the rule finds it.
#[tokio::test]
async fn the_rules_find_a_regression_in_a_real_page() {
    let out = build_states(ENGLISH).await;
    let page = out
        .states
        .iter()
        .find(|s| s.id == "language")
        .expect("the language page")
        .html
        .clone();
    assert!(
        rules::check(&dom::parse(&page)).is_empty(),
        "the real page passes"
    );
    for (rule, from, to) in [
        (
            "skip-link",
            "<a class=\"skip-link\" href=\"#main\">Skip to the main content</a>",
            "",
        ),
        (
            "one-h1",
            "<h1>Language</h1>",
            "<h1>Language</h1><h1>Again</h1>",
        ),
        ("lang-dir", " lang=\"en\" dir=\"ltr\"", ""),
        (
            "title",
            "<title>Language · inklkunn</title>",
            "<title>Language</title>",
        ),
        ("landmarks", "<main id=\"main\">", "<div id=\"main\">"),
        (
            "button-text",
            "<button type=\"submit\" aria-current=\"true\"><span lang=\"en\">English</span></button>",
            "<button type=\"submit\"></button>",
        ),
        (
            "no-inline-script-or-style",
            "<footer>",
            "<script>x()</script><footer>",
        ),
    ] {
        assert!(page.contains(from), "the real page has no {from:?}");
        let found = rules_found(&page.replacen(from, to, 1));
        assert!(
            found.contains(rule),
            "{rule}: not found in a broken real page ({found:?})"
        );
    }
}

#[test]
fn the_vague_link_list_matches_whole_link_text_only() {
    // "Download main.rs" is not vague; "Download" alone is.
    let bad = GOOD.replacen("Download main.rs", "Download", 1);
    assert!(rules_found(&bad).contains("link-text"));
    assert!(rules_found(GOOD).is_empty());
}

#[test]
fn titles_shared_by_different_pages_are_found_and_the_same_page_may_repeat() {
    let mk = |id: &str, identity: &'static str| State {
        id: id.into(),
        identity,
        status: 200,
        html: String::new(),
    };
    let owned = [mk("a", "page-a"), mk("b", "page-b"), mk("a2", "page-a")];
    let states: Vec<&State> = owned.iter().collect();
    let same = vec![
        Some("T".to_owned()),
        Some("T".to_owned()),
        Some("T".to_owned()),
    ];
    assert_eq!(
        title_violations(&states, &same).len(),
        1,
        "two different pages with one title"
    );
    let ok = vec![
        Some("A".to_owned()),
        Some("B".to_owned()),
        Some("A".to_owned()),
    ];
    assert!(
        title_violations(&states, &ok).is_empty(),
        "the same page, the same title"
    );
    let drift = vec![
        Some("A".to_owned()),
        Some("B".to_owned()),
        Some("A2".to_owned()),
    ];
    assert_eq!(
        title_violations(&states, &drift).len(),
        1,
        "one page, two titles"
    );
}

#[test]
fn a_state_that_is_not_a_page_is_counted_and_not_dropped() {
    let skipped = Skipped(vec!["x: status 200, 2 bytes, not an HTML document".into()]);
    let rep = run(&[]);
    let text = print("test", &rep, &skipped);
    assert!(
        text.contains("1 not pages") && text.contains("x: status 200"),
        "{text}"
    );
}
