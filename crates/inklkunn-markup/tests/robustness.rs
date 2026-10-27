//! Every entry point terminates quickly and never panics on hostile input (threat model C-4; the 2026-10-27
//! review's findings C1 and M1–M6). The pathological shapes below each took 0.3–0.9 s, or overflowed the
//! stack, before the linear passes; they must now be well under the bound even in a debug build.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use inklkunn_markup::frontmatter::{YamlError, parse_yaml};
use inklkunn_markup::{LintContext, NoLinks, Page, lint, render_html, scan};
use proptest::prelude::*;

/// A debug build on a slow runner: generous, and still an order of magnitude below the old timings.
const BOUND: Duration = Duration::from_millis(250);

fn bounded(label: &str, f: impl FnOnce()) {
    let t = Instant::now();
    f();
    let took = t.elapsed();
    assert!(took < BOUND, "{label} took {took:?}, more than {BOUND:?}");
}

fn everything(text: &str) {
    let _ = scan(text);
    let _ = render_html(text, &NoLinks);
    if let Ok(page) = Page::parse(text) {
        let _ = lint(&page, text, LintContext::default());
    }
}

#[test]
fn a_deeply_nested_frontmatter_value_is_refused_not_a_stack_overflow() {
    let deep = format!("a: {}{}\n", "[".repeat(20_000), "]".repeat(20_000));
    bounded("deep flow", || {
        assert!(matches!(parse_yaml(&deep), Err(YamlError::TooDeep { .. })));
    });
    let mut blocks = String::new();
    for i in 0..1_000 {
        blocks.push_str(&" ".repeat(i));
        blocks.push_str("k:\n");
    }
    bounded("deep block", || {
        assert!(matches!(
            parse_yaml(&blocks),
            Err(YamlError::TooDeep { .. })
        ));
    });
    let page = format!("---\n{deep}---\n[[Link]]\n");
    bounded("deep page", || {
        // The scan survives (the frontmatter error is reported, not fatal) and the page refuses cleanly.
        let r = scan(&page);
        assert_eq!(r.links.len(), 1);
        assert!(Page::parse(&page).is_err());
    });
}

#[test]
fn many_lines_and_many_links_are_linear() {
    let newlines = "\n".repeat(100_000);
    bounded("100k newlines", || everything(&newlines));
    let links = "[[a]] ".repeat(40_000);
    bounded("40k links", || {
        let r = scan(&links);
        assert_eq!(r.links.len(), 40_000);
    });
    bounded("40k links render", || {
        let _ = render_html(&links, &NoLinks);
    });
}

#[test]
fn unclosed_brackets_directives_and_backtick_runs_are_linear() {
    let brackets = "[".repeat(40_000);
    bounded("40k [", || everything(&brackets));
    let directives = ":a[".repeat(20_000);
    bounded("20k :a[", || everything(&directives));
    let mut runs = String::new();
    for n in 1..=300 {
        runs.push_str(&"`".repeat(n));
        runs.push('a');
    }
    bounded("backtick runs", || everything(&runs));
    let attrs = format!("<!-- wiki:snapshot {} -->", "a".repeat(40_000));
    bounded("40 KB attrs", || everything(&attrs));
    let inline = format!(":a[]{{{}}}", "a".repeat(40_000));
    bounded("40 KB inline attrs", || everything(&inline));
}

#[test]
fn many_keys_headings_tags_and_block_ids_are_linear() {
    let mut keys = String::new();
    for i in 0..40_000 {
        keys.push_str(&format!("k{i}: 1\n"));
    }
    bounded("40k keys", || {
        assert!(parse_yaml(&keys).is_ok());
    });
    let mut headings = String::from("---\ntitle: T\n---\n");
    for i in 0..20_000 {
        headings.push_str(&format!("## H{}\n\n", i % 7));
    }
    bounded("20k headings lint", || {
        let page = Page::parse(&headings).expect("page");
        let _ = lint(&page, &headings, LintContext::default());
    });
    let tags = "#tag ".repeat(40_000);
    bounded("40k tags", || everything(&tags));
    let ids = "para ^id\n\n".repeat(20_000);
    bounded("20k block ids", || everything(&ids));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn nothing_panics_on_arbitrary_text(text in "\\PC{0,400}") {
        everything(&text);
    }

    #[test]
    fn nothing_panics_on_construct_soup(
        text in proptest::collection::vec(
            prop_oneof![
                Just("[["), Just("]]"), Just("![["), Just("#"), Just("`"), Just("```\n"), Just("~~~\n"),
                Just("<!--"), Just("-->"), Just(":::"), Just(":a["), Just("]{"), Just("}"), Just("^id"),
                Just("> [!NOTE]"), Just("\n"), Just("---\n"), Just("|"), Just("\\"), Just("a"), Just(" "),
                Just("é"), Just("編"), Just("\r\n"), Just("    "), Just("# "), Just("[x](y)"),
                Just("wiki:snapshot"), Just("{title}"), Just("&"), Just("\u{feff}"),
            ],
            0..120,
        )
    ) {
        let text: String = text.concat();
        everything(&text);
    }
}
