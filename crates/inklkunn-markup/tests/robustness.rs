//! Every entry point terminates quickly and never panics on hostile input (threat model C-4; the 2026-10-27
//! review's findings C1 and M1–M6). The pathological shapes below each took 0.3–0.9 s, or overflowed the
//! stack, before the linear passes.
//!
//! The bound is on **scaling, not wall-clock time**: each shape is built at a parameter `n` and at `2n`, and
//! the time may grow at most 1.5 × as fast as the input's byte length does (for a shape whose length doubles
//! that is 3 ×; a quadratic pass takes 4 × and more as the constant factors shrink). A shared CI runner is
//! slow and noisy, so an absolute bound would say more about the machine than about the code; a generous
//! ceiling only guards against a hang.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use inklkunn_markup::frontmatter::{YamlError, parse_yaml};
use inklkunn_markup::{LintContext, NoLinks, Page, lint, render_html, scan};
use proptest::prelude::*;

/// No single run may take longer than this, whatever the machine (a hang guard, not a performance claim).
const CEILING: Duration = Duration::from_secs(20);
/// The time may grow this many times faster than the input length.
const SLACK: f64 = 1.5;
/// Runs under this are noise; the ratio is checked only when the larger run is measurable.
const NOISE: Duration = Duration::from_millis(40);

fn time(f: &dyn Fn()) -> Duration {
    // The faster of two runs: the first may pay for page faults and allocator growth.
    let mut best = Duration::MAX;
    for _ in 0..2 {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed());
    }
    best
}

/// Build the input at `n` and `2n`, run `work` on each, and require near-linear scaling.
fn linear(label: &str, n: usize, build: impl Fn(usize) -> String, work: impl Fn(&str)) {
    let small = build(n);
    let large = build(2 * n);
    let t_small = time(&|| work(&small));
    let t_large = time(&|| work(&large));
    assert!(t_large < CEILING, "{label}: {t_large:?} for 2n");
    if t_large > NOISE {
        let time_ratio = t_large.as_secs_f64() / t_small.as_secs_f64().max(0.000_5);
        let size_ratio = large.len() as f64 / small.len().max(1) as f64;
        assert!(
            time_ratio <= SLACK * size_ratio,
            "{label}: the input grew {size_ratio:.1}× and the time {time_ratio:.1}× ({t_small:?} → {t_large:?}); a linear pass grows alike"
        );
    }
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
    let t = Instant::now();
    assert!(matches!(parse_yaml(&deep), Err(YamlError::TooDeep { .. })));
    assert!(t.elapsed() < CEILING);
    let mut blocks = String::new();
    for i in 0..1_000 {
        blocks.push_str(&" ".repeat(i));
        blocks.push_str("k:\n");
    }
    assert!(matches!(
        parse_yaml(&blocks),
        Err(YamlError::TooDeep { .. })
    ));
    let page = format!("---\n{deep}---\n[[Link]]\n");
    // The scan survives (the frontmatter error is reported, not fatal) and the page refuses cleanly.
    let r = scan(&page);
    assert_eq!(r.links.len(), 1);
    assert!(Page::parse(&page).is_err());
}

#[test]
fn many_lines_and_many_links_are_linear() {
    linear("newlines", 50_000, |n| "\n".repeat(n), everything);
    linear(
        "links",
        20_000,
        |n| "[[a]] ".repeat(n),
        |t| {
            let r = scan(t);
            assert!(r.links.len() >= 20_000);
            let _ = render_html(t, &NoLinks);
        },
    );
}

#[test]
fn unclosed_brackets_directives_and_backtick_runs_are_linear() {
    linear("unclosed [", 20_000, |n| "[".repeat(n), everything);
    linear("unclosed :a[", 10_000, |n| ":a[".repeat(n), everything);
    linear(
        "backtick runs",
        150,
        |n| {
            let mut runs = String::new();
            for k in 1..=n {
                runs.push_str(&"`".repeat(k));
                runs.push('a');
            }
            runs
        },
        everything,
    );
    linear(
        "attrs in an envelope",
        20_000,
        |n| format!("<!-- wiki:snapshot {} -->", "a".repeat(n)),
        everything,
    );
    linear(
        "attrs in an inline directive",
        20_000,
        |n| format!(":a[]{{{}}}", "a".repeat(n)),
        everything,
    );
}

#[test]
fn many_keys_headings_tags_and_block_ids_are_linear() {
    linear(
        "keys",
        20_000,
        |n| (0..n).map(|i| format!("k{i}: 1\n")).collect(),
        |t| assert!(parse_yaml(t).is_ok()),
    );
    linear(
        "headings lint",
        10_000,
        |n| {
            let mut s = String::from("---\ntitle: T\n---\n");
            for i in 0..n {
                s.push_str(&format!("## H{}\n\n", i % 7));
            }
            s
        },
        |t| {
            let page = Page::parse(t).expect("page");
            let _ = lint(&page, t, LintContext::default());
            let _ = render_html(&page.body, &NoLinks);
        },
    );
    linear("tags", 20_000, |n| "#tag ".repeat(n), everything);
    linear(
        "block ids",
        10_000,
        |n| "para ^id\n\n".repeat(n),
        everything,
    );
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
