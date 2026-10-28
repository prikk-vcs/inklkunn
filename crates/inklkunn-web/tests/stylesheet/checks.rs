//! The checks over a stylesheet that are not about colour: direction, focus, motion, shortcuts, what is loaded,
//! the size of targets, and the colour schemes and forced colours.

use super::css::{DARK_MEDIA, Rule, parse, strip_comments};

/// A four-value shorthand whose left and right differ is a physical declaration.
fn asymmetric_four(v: &str) -> bool {
    let w: Vec<&str> = v.split_whitespace().collect();
    matches!(w.as_slice(), [_, right, _, left] if right != left)
}

pub(crate) fn direction_findings(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    for r in parse(css) {
        for (p, v) in &r.decls {
            let physical_prop = matches!(p.as_str(), "left" | "right" | "clear" | "float")
                || p.starts_with("margin-left")
                || p.starts_with("margin-right")
                || p.starts_with("padding-left")
                || p.starts_with("padding-right")
                || p.starts_with("border-left")
                || p.starts_with("border-right")
                || (p.starts_with("border-") && (p.contains("-left-") || p.contains("-right-")))
                || matches!(p.as_str(), "scroll-margin-left" | "scroll-margin-right")
                || matches!(p.as_str(), "scroll-padding-left" | "scroll-padding-right");
            let physical_value = matches!(p.as_str(), "text-align" | "float" | "clear")
                && matches!(v.as_str(), "left" | "right");
            let shorthand = matches!(
                p.as_str(),
                "margin" | "padding" | "inset" | "border-width" | "border-style" | "border-color"
            ) && asymmetric_four(v);
            // `float: inline-start` and `clear: inline-end` are logical; only the physical names are not.
            let physical_prop = physical_prop && !matches!(p.as_str(), "float" | "clear");
            if physical_prop || physical_value || shorthand {
                out.push(format!("`{}`: {p}: {v}", r.selectors.join(", ")));
            }
        }
    }
    out
}

fn is_removal(p: &str, v: &str) -> bool {
    (p == "outline" && matches!(v.split_whitespace().next(), Some("none" | "0" | "0px")))
        || (p == "outline-style" && v == "none")
        || (p == "outline-width" && matches!(v, "0" | "0px"))
}

fn px(v: &str) -> Option<f64> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    if let Some(n) = v.strip_suffix("rem") {
        return n.trim().parse::<f64>().ok().map(|x| x * 16.0);
    }
    None
}

pub(crate) fn focus_findings(css: &str) -> Vec<String> {
    let rules = parse(css);
    let mut out = Vec::new();
    let focus: Vec<&Rule> = rules
        .iter()
        .filter(|r| r.selectors.iter().any(|s| s.contains(":focus-visible")))
        // The rules that draw the ring (another `:focus-visible` rule may style something else).
        .filter(|r| r.decls.iter().any(|(p, _)| p.starts_with("outline")))
        // The forced-colours block restates the ring in system colours; the ring itself is the rule outside it.
        .filter(|r| !r.media.iter().any(|m| m.contains("forced-colors")))
        .collect();
    for r in &rules {
        for (p, v) in &r.decls {
            if !is_removal(p, v) {
                continue;
            }
            if r.selectors.iter().all(|s| s.contains(":focus-visible")) {
                out.push(format!(
                    "`{}` removes the focus outline itself",
                    r.selectors.join(", ")
                ));
            } else {
                out.push(format!(
                    "`{}` removes the outline ({p}: {v})",
                    r.selectors.join(", ")
                ));
            }
        }
    }
    if focus.is_empty() {
        out.push("there is no :focus-visible rule".into());
    }
    // One explicit style, applied to every kind of target: a bare `:focus-visible` (or the four named).
    let covers = |name: &str| {
        focus.iter().any(|r| {
            r.selectors
                .iter()
                .any(|s| s == ":focus-visible" || s == &format!("{name}:focus-visible"))
        })
    };
    for name in ["a", "button", "input", ".skip-link"] {
        if !covers(name) {
            out.push(format!("no :focus-visible style covers `{name}`"));
        }
    }
    let mut width_ok = false;
    for r in &focus {
        let o = r.get("outline");
        let width = r
            .get("outline-width")
            .and_then(px)
            .or_else(|| o.and_then(|v| v.split_whitespace().find_map(px)));
        let solid = r.get("outline-style") == Some("solid")
            || o.is_some_and(|v| v.split_whitespace().any(|w| w == "solid"));
        if width.is_some_and(|w| w >= 2.0) && solid {
            width_ok = true;
        }
        if r.get("outline-offset").is_none() {
            out.push(format!(
                "`{}` has no outline-offset (the ring should stand off the control)",
                r.selectors.join(", ")
            ));
        }
    }
    if !focus.is_empty() && !width_ok {
        out.push("no :focus-visible rule draws a solid outline of at least 2px".into());
    }
    out
}

pub(crate) fn motion_findings(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    for r in parse(css) {
        for (p, v) in &r.decls {
            let moving = p.starts_with("animation")
                || p.starts_with("transition")
                || (p == "scroll-behavior" && v != "auto");
            let guarded = r
                .media
                .iter()
                .any(|m| m == "(prefers-reduced-motion:no-preference)");
            if moving && !guarded {
                out.push(format!(
                    "`{}`: {p}: {v} without a prefers-reduced-motion guard",
                    r.selectors.join(", ")
                ));
            }
        }
        if r.selectors.iter().any(|s| s.starts_with("@keyframes")) && r.media.is_empty() {
            // Keyframes alone move nothing, but an unguarded one invites an unguarded `animation`.
            out.push(format!(
                "`{}` is outside a motion guard",
                r.selectors.join(", ")
            ));
        }
    }
    out
}

pub(crate) fn shortcut_findings(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    for r in parse(css) {
        for (p, v) in &r.decls {
            if v.contains("!important") {
                out.push(format!("`{}`: {p} uses !important", r.selectors.join(", ")));
            }
            if (p == "overflow" || p == "overflow-x")
                && v == "hidden"
                && !r.selectors.iter().any(|s| s == ".visually-hidden")
            {
                out.push(format!(
                    "`{}`: {p}: hidden hides overflow instead of fixing it",
                    r.selectors.join(", ")
                ));
            }
        }
    }
    out
}

/// Nothing is loaded from anywhere: one stylesheet, no web font, no image, no other origin (INC-5; the page
/// policy allows `style-src 'self'` and `img-src 'self'` and nothing else).
pub(crate) fn loads_findings(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    let code = strip_comments(css);
    for needle in ["@import", "@font-face", "url(", "src:"] {
        if code.contains(needle) {
            out.push(format!("the stylesheet uses `{needle}`"));
        }
    }
    out
}

/// Standalone targets: the selectors that style them. Links in running text are exempt and are not listed.
/// A page that adds a kind of standalone target adds its selector here, and the stylesheet gives it a size.
const TARGETS: &[&str] = &[
    "header a",
    "footer a",
    "nav a",
    "th a",
    "td a",
    "main > p > a",
    ".skip-link",
    "button",
    "input",
    "select",
    "textarea",
    "summary",
];
const TARGET_MIN: f64 = 24.0;

pub(crate) fn target_findings(css: &str) -> Vec<String> {
    let rules = parse(css);
    let mut out = Vec::new();
    for want in TARGETS {
        let hits: Vec<&Rule> = rules
            .iter()
            .filter(|r| r.media.is_empty() && r.selectors.iter().any(|s| s == want))
            .collect();
        let biggest = |props: &[&str]| {
            hits.iter()
                .flat_map(|r| props.iter().filter_map(|p| r.get(p).and_then(px)))
                .fold(0.0_f64, f64::max)
        };
        if biggest(&["min-height", "min-block-size"]) < TARGET_MIN {
            out.push(format!(
                "`{want}` has no minimum block size of {TARGET_MIN}px"
            ));
        }
        if biggest(&["min-width", "min-inline-size"]) < TARGET_MIN {
            out.push(format!(
                "`{want}` has no minimum inline size of {TARGET_MIN}px"
            ));
        }
        // A link is `inline` by default, and a minimum size does nothing to an inline box.
        let is_link = want.ends_with(" a") || want.ends_with("> a") || *want == ".skip-link";
        if is_link {
            let display = hits
                .iter()
                .filter_map(|r| r.get("display"))
                .next_back()
                .unwrap_or("inline");
            let positioned = hits.iter().any(|r| r.get("position") == Some("absolute"));
            if !positioned
                && !matches!(
                    display,
                    "inline-block" | "inline-flex" | "block" | "flex" | "grid" | "inline-grid"
                )
            {
                out.push(format!(
                    "`{want}` is `display: {display}`: a minimum size has no effect on it"
                ));
            }
        }
    }
    out
}

pub(crate) fn scheme_findings(css: &str) -> Vec<String> {
    let rules = parse(css);
    let mut out = Vec::new();
    let root_scheme = rules
        .iter()
        .filter(|r| r.media.is_empty() && r.selectors.iter().any(|s| s == ":root"))
        .any(|r| {
            r.get("color-scheme")
                .is_some_and(|v| v.split_whitespace().collect::<Vec<_>>() == ["light", "dark"])
        });
    if !root_scheme {
        out.push("`:root` does not declare `color-scheme: light dark` (form controls stay light in the dark scheme)".into());
    }
    let dark = rules.iter().any(|r| r.media == [DARK_MEDIA]);
    if !dark {
        out.push("there is no `@media (prefers-color-scheme: dark)` block".into());
    }
    let forced: Vec<&Rule> = rules
        .iter()
        .filter(|r| r.media.iter().any(|m| m == "(forced-colors:active)"))
        .collect();
    if forced.is_empty() {
        out.push("there is no `@media (forced-colors: active)` block".into());
    }
    let has = |sel: &str, prop: &str| {
        forced
            .iter()
            .any(|r| r.selectors.iter().any(|s| s.contains(sel)) && r.get(prop).is_some())
    };
    if !has(".skip-link", "border") && !has(".skip-link", "border-color") {
        out.push("forced colours: `.skip-link` keeps no border".into());
    }
    if !has(":focus-visible", "outline") && !has(":focus-visible", "outline-color") {
        out.push("forced colours: the focus ring is not restated".into());
    }
    if !has("button", "border") && !has("button", "border-color") {
        out.push("forced colours: `button` keeps no border".into());
    }
    out
}
