//! RFC 003 D-6 (planeter RFC 012 D-2, D-3(c), D-5), the presentation half: checks over the one stylesheet,
//! `static/site.css`, as tests.
//!
//! The stylesheet is small and ours, so it is read with a few lines of hand-written CSS reading (no CSS crate):
//! rules, `@media` blocks, custom properties. Every threshold below is written here, independently of the
//! stylesheet, and every check is shown able to fail on a planted stylesheet (the `GOOD` one at the end, with
//! one thing broken at a time).
//!
//! What is checked: the contrast of every foreground/background pair the rules use, in both schemes, computed
//! from the tokens (text 4.5:1; the borders that are the only identification of a control, and the focus ring,
//! 3:1); every colour written once, as a token, and every token given a value in both schemes; logical
//! properties only; an explicit visible focus style that is never removed; no motion outside a
//! `prefers-reduced-motion` guard; a 24 CSS px minimum on every standalone target; forced colours and
//! `color-scheme` present; and nothing loaded from anywhere (no `@import`, no font, no `url(`).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod checks;
mod css;

use checks::{
    direction_findings, focus_findings, loads_findings, motion_findings, scheme_findings,
    shortcut_findings, target_findings,
};
use css::{Theme, contrast_findings, contrast_rows, hex, token_findings};

const CSS: &str = include_str!("../../static/site.css");

// ---------------------------------------------------------------------------------------------------------
// The tests on the real stylesheet

#[test]
fn every_colour_pair_the_stylesheet_uses_has_enough_contrast_in_both_schemes() {
    let f = contrast_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

/// `STYLESHEET_REPORT=<file>` writes the table of every pair (for the review request's contrast table).
#[test]
fn the_contrast_table_can_be_written() {
    let (rows, problems) = contrast_rows(CSS);
    assert!(problems.is_empty(), "{problems:?}");
    assert!(rows.len() >= 16, "only {} pairs were found", rows.len());
    for theme in [Theme::Light, Theme::Dark] {
        assert!(
            rows.iter()
                .any(|r| r.theme == theme && r.what == "focus ring"),
            "{theme:?}: the focus ring was not measured"
        );
        assert!(
            rows.iter()
                .any(|r| r.theme == theme && r.what == "control border"),
            "{theme:?}: no control border was measured"
        );
    }
    if let Ok(path) = std::env::var("STYLESHEET_REPORT") {
        let mut t = String::from(
            "| scheme | pair | where used | foreground | background | ratio | threshold |\n|---|---|---|---|---|---|---|\n",
        );
        for r in &rows {
            t.push_str(&format!(
                "| {:?} | {} | `{}` | {} | {} | {:.2}:1 | {}:1 |\n",
                r.theme,
                r.what,
                r.selector,
                hex(r.fg),
                hex(r.bg),
                r.ratio,
                r.min
            ));
        }
        std::fs::write(path, t).unwrap();
    }
}

#[test]
fn every_colour_is_a_token_and_every_token_has_a_value_in_both_schemes() {
    let f = token_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn the_stylesheet_uses_logical_properties_only() {
    let f = direction_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn focus_has_one_explicit_visible_style_and_is_never_removed() {
    let f = focus_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn nothing_moves_without_a_reduced_motion_guard() {
    let f = motion_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn no_shortcut_hides_overflow_or_wins_a_specificity_fight() {
    let f = shortcut_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn nothing_is_loaded_from_anywhere() {
    let f = loads_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn every_standalone_target_is_at_least_24_css_px() {
    let f = target_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn the_page_declares_both_colour_schemes_and_restates_itself_for_forced_colours() {
    let f = scheme_findings(CSS);
    assert!(f.is_empty(), "{f:#?}");
}

// ---------------------------------------------------------------------------------------------------------
// Each check can fail

/// A stylesheet that passes every check; each planted failure below changes one thing in it.
const GOOD: &str = r#"
:root{color-scheme:light dark;--bg:#fff;--fg:#1a1a1a;--muted:#666;--control:#767676;--accent:#2a5db0;--line:#ddd}
@media (prefers-color-scheme:dark){:root{--bg:#111;--fg:#eee;--muted:#aaa;--control:#8c8c8c;--accent:#7aa7ff;--line:#333}}
body{margin:0;color:var(--fg);background:var(--bg)}
header a,footer a,nav a,th a,td a,main > p > a,.skip-link{display:inline-block;min-block-size:24px;min-inline-size:24px}
button,input,select,textarea,summary{min-block-size:24px;min-inline-size:24px}
input,button,select,textarea{border:1px solid var(--control);background:var(--bg);color:var(--fg)}
.skip-link{border:1px solid var(--control);background:var(--bg)}
.muted{color:var(--muted)}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
header .who{margin-inline-start:auto;padding:0 1rem 0 1rem}
td,th{text-align:start;border-block-end:1px solid var(--line)}
@media (prefers-reduced-motion:no-preference){a{transition:color .1s}}
@media (forced-colors:active){.skip-link{border:1px solid CanvasText}:focus-visible{outline:2px solid Highlight}button{border:1px solid ButtonBorder}}
"#;

fn replaced(from: &str, to: &str) -> String {
    assert!(GOOD.contains(from), "the plant does not apply: {from:?}");
    GOOD.replacen(from, to, 1)
}

#[test]
fn the_good_stylesheet_passes_every_check() {
    assert_eq!(contrast_findings(GOOD), Vec::<String>::new());
    assert_eq!(token_findings(GOOD), Vec::<String>::new());
    assert_eq!(direction_findings(GOOD), Vec::<String>::new());
    assert_eq!(focus_findings(GOOD), Vec::<String>::new());
    assert_eq!(motion_findings(GOOD), Vec::<String>::new());
    assert_eq!(shortcut_findings(GOOD), Vec::<String>::new());
    assert_eq!(loads_findings(GOOD), Vec::<String>::new());
    assert_eq!(target_findings(GOOD), Vec::<String>::new());
    assert_eq!(scheme_findings(GOOD), Vec::<String>::new());
}

#[test]
fn contrast_fails_on_a_weak_text_pair_in_either_scheme_and_on_a_weak_control_border_or_ring() {
    // light text pair
    assert!(!contrast_findings(&replaced("--muted:#666", "--muted:#999")).is_empty());
    // dark text pair only
    let f = contrast_findings(&replaced("--muted:#aaa", "--muted:#444"));
    assert!(
        f.iter().all(|x| x.starts_with("Dark")) && !f.is_empty(),
        "{f:?}"
    );
    // a control's border in the dark scheme only
    let f = contrast_findings(&replaced("--control:#8c8c8c", "--control:#222"));
    assert!(
        f.iter()
            .any(|x| x.starts_with("Dark") && x.contains("input")),
        "{f:?}"
    );
    // the focus ring
    let f = contrast_findings(&replaced(
        "outline:2px solid var(--accent)",
        "outline:2px solid var(--line)",
    ));
    assert!(f.iter().any(|x| x.contains(":focus-visible")), "{f:?}");
    // a control with no border colour of its own
    let f = contrast_findings(&replaced(
        "input,button,select,textarea{border:1px solid var(--control);",
        "input,button,select,textarea{",
    ));
    assert!(
        f.iter().any(|x| x.contains("no explicit border colour")),
        "{f:?}"
    );
    // the skip link's border is held to the same 3:1
    let f = contrast_findings(&replaced(
        ".skip-link{border:1px solid var(--control)",
        ".skip-link{border:1px solid var(--line)",
    ));
    assert!(f.iter().any(|x| x.contains(".skip-link")), "{f:?}");
    // an undefined token is a finding, not a pass
    assert!(!contrast_findings(&replaced("color:var(--muted)", "color:var(--nope)")).is_empty());
    // decorative lines may be faint: the good sheet draws `--line` (1.4:1) under its table cells and passes
    assert_eq!(contrast_findings(GOOD), Vec::<String>::new());
}

#[test]
fn a_colour_outside_the_tokens_or_a_token_in_one_scheme_only_is_found() {
    assert!(!token_findings(&replaced("color:var(--muted)", "color:#666666")).is_empty());
    assert!(
        !token_findings(&replaced(
            "background:var(--bg)}\n.muted",
            "background:rgb(1,2,3)}\n.muted"
        ))
        .is_empty()
    );
    let f = token_findings(&replaced("--line:#333}}", "}}"));
    assert!(
        f.iter()
            .any(|x| x.contains("--line has no value in the dark scheme")),
        "{f:?}"
    );
    let f = token_findings(&replaced("--line:#ddd}", "}"));
    assert!(
        f.iter()
            .any(|x| x.contains("--line has no value in the light scheme")),
        "{f:?}"
    );
    assert!(
        !token_findings("body{margin:0}").is_empty(),
        "no tokens at all"
    );
    // A custom property that is not a colour needs no value in the dark scheme.
    assert_eq!(
        token_findings(&replaced(
            ":root{color-scheme",
            ":root{--gap:1rem;color-scheme"
        )),
        Vec::<String>::new()
    );
}

#[test]
fn direction_fails_on_every_physical_declaration() {
    for (from, to) in [
        ("margin-inline-start:auto", "margin-left:auto"),
        ("margin-inline-start:auto", "margin-right:1rem"),
        ("margin-inline-start:auto", "padding-left:1rem"),
        ("margin-inline-start:auto", "padding-right:1rem"),
        ("margin-inline-start:auto", "border-left:1px solid #000"),
        ("margin-inline-start:auto", "border-right-color:#000"),
        ("margin-inline-start:auto", "border-top-left-radius:4px"),
        ("margin-inline-start:auto", "left:0"),
        ("margin-inline-start:auto", "right:0"),
        ("margin-inline-start:auto", "float:left"),
        ("margin-inline-start:auto", "clear:right"),
        ("margin-inline-start:auto", "scroll-margin-left:1rem"),
        ("text-align:start", "text-align:left"),
        ("text-align:start", "text-align:right"),
        // a four-value shorthand names the sides in physical order
        ("padding:0 1rem 0 1rem", "padding:0 1rem 0 0"),
        ("margin-inline-start:auto", "margin:0 0 0 1rem"),
        ("margin-inline-start:auto", "inset:0 0 0 2rem"),
    ] {
        assert!(!direction_findings(&replaced(from, to)).is_empty(), "{to}");
    }
    assert!(
        direction_findings(&replaced(
            "margin-inline-start:auto",
            "inset-inline-start:0;margin-inline-end:1rem;float:inline-start;margin:0 1rem;padding:1px 2px 3px 2px"
        ))
        .is_empty()
    );
}

#[test]
fn focus_fails_when_the_outline_is_removed_or_the_ring_is_thin_or_missing() {
    assert!(
        !focus_findings(&replaced(
            "input,button,select,textarea{",
            "a{outline:none}input,button,select,textarea{"
        ))
        .is_empty()
    );
    assert!(
        !focus_findings(&replaced(
            "input,button,select,textarea{",
            "a{outline:0}input,button,select,textarea{"
        ))
        .is_empty()
    );
    assert!(
        !focus_findings(&replaced(
            "input,button,select,textarea{",
            "a{outline-style:none}input,button,select,textarea{"
        ))
        .is_empty()
    );
    assert!(
        !focus_findings(&replaced(
            ":focus-visible{outline:2px solid var(--accent);outline-offset:2px}",
            ""
        ))
        .is_empty()
    );
    assert!(
        !focus_findings(&replaced(
            "outline:2px solid var(--accent)",
            "outline:1px solid var(--accent)"
        ))
        .is_empty()
    );
    assert!(
        !focus_findings(&replaced(
            "outline:2px solid var(--accent);outline-offset:2px",
            "outline:2px solid var(--accent)"
        ))
        .is_empty()
    );
    assert!(
        !focus_findings(&replaced(
            ":focus-visible{outline:2px",
            "a:focus-visible{outline:2px"
        ))
        .is_empty(),
        "a style for links only leaves the rest"
    );
}

#[test]
fn motion_fails_without_the_guard() {
    assert!(
        !motion_findings(&replaced(
            "@media (prefers-reduced-motion:no-preference){a{transition:color .1s}}",
            "a{transition:color .1s}"
        ))
        .is_empty()
    );
    assert!(!motion_findings(&replaced("body{", "html{scroll-behavior:smooth}body{")).is_empty());
    assert!(!motion_findings(&replaced("body{", "a{animation:spin 1s infinite}body{")).is_empty());
    assert!(
        !motion_findings(&replaced(
            "body{",
            "@keyframes spin{from{opacity:0}to{opacity:1}}body{"
        ))
        .is_empty()
    );
}

#[test]
fn the_shortcuts_are_found() {
    assert!(
        !shortcut_findings(&replaced(
            "td,th{text-align:start",
            "td,th{text-align:start !important"
        ))
        .is_empty()
    );
    assert!(!shortcut_findings(&replaced("td,th{", "main{overflow:hidden}td,th{")).is_empty());
    assert!(
        shortcut_findings(&replaced(
            "td,th{",
            ".visually-hidden{overflow:hidden}td,th{"
        ))
        .is_empty()
    );
}

#[test]
fn loading_anything_is_found() {
    for plant in [
        "@import url(\"https://example.invalid/a.css\");",
        "@font-face{font-family:x;src:local(x)}",
        "a{background:url(/-/x.png)}",
        "a{background-image:url(\"data:image/png;base64,AAAA\")}",
    ] {
        assert!(
            !loads_findings(&format!("{plant}{GOOD}")).is_empty(),
            "{plant}"
        );
    }
    // A comment that names one is only a comment.
    assert!(loads_findings(&format!("/* no @import, no url( */{GOOD}")).is_empty());
}

#[test]
fn target_size_fails_on_a_missing_minimum_or_an_inline_link() {
    assert!(
        !target_findings(&replaced(
            "header a,footer a,nav a,th a,td a,main > p > a,.skip-link{display:inline-block;min-block-size:24px;min-inline-size:24px}",
            "header a,footer a,nav a,th a,td a,main > p > a,.skip-link{display:inline-block;min-inline-size:24px}"
        ))
        .is_empty()
    );
    assert!(
        !target_findings(&replaced(
            "display:inline-block;min-block-size:24px",
            "display:inline-block;min-block-size:20px"
        ))
        .is_empty()
    );
    assert!(
        !target_findings(&replaced(
            "display:inline-block;min-block-size:24px",
            "display:inline;min-block-size:24px"
        ))
        .is_empty()
    );
    assert!(
        !target_findings(&replaced(
            "button,input,select,textarea,summary{min-block-size:24px;min-inline-size:24px}",
            "button,input,select,textarea,summary{min-inline-size:24px}"
        ))
        .is_empty()
    );
    assert!(
        !target_findings(&replaced("th a,", "")).is_empty(),
        "a target selector that no rule styles"
    );
    // The physical spellings are accepted too.
    assert!(
        target_findings(&replaced(
            "button,input,select,textarea,summary{min-block-size:24px;min-inline-size:24px}",
            "button,input,select,textarea,summary{min-height:24px;min-width:24px}"
        ))
        .is_empty()
    );
}

#[test]
fn colour_scheme_and_forced_colours_fail_when_absent() {
    assert!(!scheme_findings(&replaced("color-scheme:light dark;", "")).is_empty());
    assert!(
        !scheme_findings(&replaced("color-scheme:light dark;", "color-scheme:light;")).is_empty()
    );
    assert!(!scheme_findings(&replaced("@media (prefers-color-scheme:dark){:root{--bg:#111;--fg:#eee;--muted:#aaa;--control:#8c8c8c;--accent:#7aa7ff;--line:#333}}", "")).is_empty());
    assert!(!scheme_findings(&replaced("@media (forced-colors:active){.skip-link{border:1px solid CanvasText}:focus-visible{outline:2px solid Highlight}button{border:1px solid ButtonBorder}}", "")).is_empty());
    assert!(!scheme_findings(&replaced(".skip-link{border:1px solid CanvasText}", "")).is_empty());
    assert!(!scheme_findings(&replaced("button{border:1px solid ButtonBorder}", "")).is_empty());
    assert!(
        !scheme_findings(&replaced(":focus-visible{outline:2px solid Highlight}", "")).is_empty()
    );
}
