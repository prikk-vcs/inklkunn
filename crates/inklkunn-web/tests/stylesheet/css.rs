//! A small reader of the stylesheet (no CSS crate: rules, `@media` blocks, custom properties) and the colour
//! arithmetic of WCAG: the tokens of a scheme, the contrast of every pair a rule uses, and the discipline of
//! the tokens themselves.

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub(crate) selectors: Vec<String>,
    pub(crate) decls: Vec<(String, String)>,
    /// The `@media` conditions this rule sits in, outermost first, lower-cased, spaces removed.
    pub(crate) media: Vec<String>,
}

impl Rule {
    pub(crate) fn get(&self, prop: &str) -> Option<&str> {
        self.decls
            .iter()
            .rev()
            .find(|(p, _)| p == prop)
            .map(|(_, v)| v.as_str())
    }
}

pub(crate) fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        let (before, from) = rest.split_at(i);
        out.push_str(before);
        match from.find("*/") {
            Some(j) => rest = from.get(j + 2..).unwrap_or(""),
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn parse_block(css: &str, media: &[String], out: &mut Vec<Rule>) {
    let mut rest = css.trim();
    while !rest.is_empty() {
        let Some(open) = rest.find('{') else { break };
        let (prelude, from_open) = rest.split_at(open);
        let prelude = prelude.trim();
        let mut depth = 0_usize;
        let mut close = None;
        for (i, c) in from_open.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.expect("an unclosed block in the stylesheet");
        let body = from_open.get(1..close).unwrap_or("");
        if let Some(cond) = prelude.strip_prefix("@media") {
            let mut m = media.to_vec();
            m.push(
                cond.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase(),
            );
            parse_block(body, &m, out);
        } else if prelude.starts_with('@') {
            // An at-rule that is not a media query (`@font-face`, `@keyframes` …) is a rule of its own; its
            // name is its selector, so the checks below can see that it is there.
            out.push(Rule {
                selectors: vec![prelude.to_owned()],
                decls: declarations(body),
                media: media.to_vec(),
            });
        } else {
            out.push(Rule {
                selectors: prelude.split(',').map(|s| s.trim().to_owned()).collect(),
                decls: declarations(body),
                media: media.to_vec(),
            });
        }
        rest = from_open.get(close + 1..).unwrap_or("").trim();
    }
}

fn declarations(body: &str) -> Vec<(String, String)> {
    body.split(';')
        .filter_map(|d| {
            let (p, v) = d.split_once(':')?;
            Some((p.trim().to_lowercase(), v.trim().to_owned()))
        })
        .collect()
}

pub(crate) fn parse(css: &str) -> Vec<Rule> {
    let mut out = Vec::new();
    parse_block(&strip_comments(css), &[], &mut out);
    out
}

// ---------------------------------------------------------------------------------------------------------
// Colours

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Theme {
    Light,
    Dark,
}

pub(crate) const DARK_MEDIA: &str = "(prefers-color-scheme:dark)";

/// The custom properties of a theme: `:root` outside any media block, over which the dark block is laid.
pub(crate) fn tokens(rules: &[Rule], theme: Theme) -> BTreeMap<String, String> {
    let mut t = BTreeMap::new();
    for r in rules {
        let root = r.selectors.iter().any(|s| s == ":root");
        let plain = r.media.is_empty();
        let dark = r.media == [DARK_MEDIA];
        if root && (plain || (dark && theme == Theme::Dark)) {
            for (p, v) in &r.decls {
                if p.starts_with("--") {
                    t.insert(p.clone(), v.clone());
                }
            }
        }
    }
    t
}

pub(crate) type Rgb = (u8, u8, u8);

pub(crate) fn parse_hex(v: &str) -> Result<Rgb, String> {
    let bad = || format!("cannot read the colour {v:?}");
    let hex = v.strip_prefix('#').ok_or_else(bad)?;
    let digits: Vec<u32> = hex
        .chars()
        .map(|c| c.to_digit(16))
        .collect::<Option<_>>()
        .ok_or_else(bad)?;
    let byte = |hi: u32, lo: u32| u8::try_from(hi * 16 + lo).map_err(|_| bad());
    match digits.as_slice() {
        [r, g, b] => Ok((byte(*r, *r)?, byte(*g, *g)?, byte(*b, *b)?)),
        [r1, r2, g1, g2, b1, b2] => Ok((byte(*r1, *r2)?, byte(*g1, *g2)?, byte(*b1, *b2)?)),
        _ => Err(bad()),
    }
}

pub(crate) fn resolve(value: &str, t: &BTreeMap<String, String>) -> Result<Rgb, String> {
    let v = value.trim();
    if let Some(name) = v.strip_prefix("var(").and_then(|x| x.strip_suffix(')')) {
        let name = name.trim();
        let inner = t
            .get(name)
            .ok_or_else(|| format!("{name} is not defined"))?;
        return resolve(inner, t);
    }
    parse_hex(v)
}

pub(crate) fn luminance((r, g, b): Rgb) -> f64 {
    let f = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
}

pub(crate) fn ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The colour a declaration value names: a `background`/`border` shorthand holds it among other words.
pub(crate) fn colour_word(value: &str) -> Option<&str> {
    value
        .split_whitespace()
        .find(|w| w.starts_with("var(--") || w.starts_with('#'))
}

/// Elements whose borders are the only visual identification of a control (WCAG 1.4.11): they need 3:1.
pub(crate) const CONTROL_BORDERS: &[&str] =
    &["input", "button", "select", "textarea", ".skip-link"];
const TEXT_MIN: f64 = 4.5;
const NON_TEXT_MIN: f64 = 3.0;

/// Is this selector one of a control (`button`, `button[aria-current]`, `input:hover` …)?
fn is_control_selector(s: &str) -> bool {
    CONTROL_BORDERS
        .iter()
        .any(|c| s == *c || s.starts_with(&format!("{c}[")) || s.starts_with(&format!("{c}:")))
        || s.contains(":focus-visible")
}

/// One measured pair: what it is, where it is used, its ratio and the threshold it must meet.
#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub(crate) theme: Theme,
    pub(crate) what: &'static str,
    pub(crate) selector: String,
    pub(crate) fg: Rgb,
    pub(crate) bg: Rgb,
    pub(crate) ratio: f64,
    pub(crate) min: f64,
}

pub(crate) fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Every pair the stylesheet uses, in both themes, and what could not be read.
pub(crate) fn contrast_rows(css: &str) -> (Vec<Row>, Vec<String>) {
    let rules = parse(css);
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    for theme in [Theme::Light, Theme::Dark] {
        let t = tokens(&rules, theme);
        // The page's colours come from `body`.
        let body = rules
            .iter()
            .find(|r| r.media.is_empty() && r.selectors.iter().any(|s| s == "body"));
        let page = |prop: &str| -> String {
            body.and_then(|r| {
                r.get(prop).or_else(|| {
                    (prop == "background-color")
                        .then(|| r.get("background"))
                        .flatten()
                })
            })
            .unwrap_or("")
            .to_owned()
        };
        let (body_fg, body_bg) = (page("color"), page("background-color"));
        let (Ok(fg0), Ok(bg0)) = (resolve(&body_fg, &t), resolve(&body_bg, &t)) else {
            problems.push(format!(
                "{theme:?}: the body's colour or background cannot be read ({body_fg:?}, {body_bg:?})"
            ));
            continue;
        };
        rows.push(Row {
            theme,
            what: "text",
            selector: "body".into(),
            fg: fg0,
            bg: bg0,
            ratio: ratio(fg0, bg0),
            min: TEXT_MIN,
        });
        for r in &rules {
            // Forced colours replace the author's colours with the system's: nothing to compute.
            if r.media.iter().any(|m| m.contains("forced-colors")) {
                continue;
            }
            let sel = r.selectors.join(", ");
            let fg_decl = r.get("color");
            let bg_decl = r.get("background").or_else(|| r.get("background-color"));
            // The body's own pair is the first row of the theme; it is not measured twice.
            let is_body = r.media.is_empty() && r.selectors == ["body"];
            if (fg_decl.is_some() || bg_decl.is_some()) && !is_body {
                let bg = match bg_decl.and_then(colour_word) {
                    Some(w) => resolve(w, &t),
                    None => Ok(bg0),
                };
                let fg = match fg_decl {
                    Some(v) => resolve(v, &t),
                    None => Ok(fg0),
                };
                match (fg, bg) {
                    (Ok(fg), Ok(bg)) => rows.push(Row {
                        theme,
                        what: "text",
                        selector: sel.clone(),
                        fg,
                        bg,
                        ratio: ratio(fg, bg),
                        min: TEXT_MIN,
                    }),
                    (Err(e), _) | (_, Err(e)) => problems.push(format!("{theme:?}: `{sel}`: {e}")),
                }
            }
            // Borders and outlines of controls and of the focus ring.
            for (prop, what) in [
                ("border", "control border"),
                ("border-color", "control border"),
                ("outline", "focus ring"),
                ("outline-color", "focus ring"),
            ] {
                let Some(v) = r.get(prop) else { continue };
                let Some(w) = colour_word(v) else { continue };
                if !r.selectors.iter().any(|s| is_control_selector(s)) {
                    continue;
                }
                let bg = match r.get("background").and_then(colour_word) {
                    Some(b) => resolve(b, &t).unwrap_or(bg0),
                    None => bg0,
                };
                match resolve(w, &t) {
                    Ok(c) => rows.push(Row {
                        theme,
                        what,
                        selector: sel.clone(),
                        fg: c,
                        bg,
                        ratio: ratio(c, bg),
                        min: NON_TEXT_MIN,
                    }),
                    Err(e) => problems.push(format!("{theme:?}: `{sel}`: {e}")),
                }
            }
        }
        // The controls must say their border colour: a browser-default border cannot be computed here.
        for need in CONTROL_BORDERS {
            let has = rules.iter().any(|r| {
                r.media.is_empty()
                    && r.selectors.iter().any(|s| s == need)
                    && (r.get("border").and_then(colour_word).is_some()
                        || r.get("border-color").is_some())
            });
            if !has {
                problems.push(format!(
                    "{theme:?}: `{need}` has no explicit border colour (the browser's default cannot be checked)"
                ));
            }
        }
    }
    (rows, problems)
}

/// Findings of the contrast test over a stylesheet, both themes.
pub(crate) fn contrast_findings(css: &str) -> Vec<String> {
    let (rows, mut out) = contrast_rows(css);
    for r in rows.iter().filter(|r| r.ratio < r.min) {
        out.push(format!(
            "{:?}: the {} of `{}` is {:.2}:1 (< {})",
            r.theme, r.what, r.selector, r.ratio, r.min
        ));
    }
    out.sort();
    out.dedup();
    out
}

/// Design tokens: every colour is written once, as a custom property, and each token has a value in both
/// schemes, so a scheme never borrows the other's by accident.
pub(crate) fn token_findings(css: &str) -> Vec<String> {
    let rules = parse(css);
    let mut out = Vec::new();
    for r in &rules {
        let is_root = r.selectors.iter().all(|s| s == ":root");
        for (p, v) in &r.decls {
            if is_root && p.starts_with("--") {
                continue;
            }
            let literal = v.contains('#')
                || v.contains("rgb(")
                || v.contains("rgba(")
                || v.contains("hsl(")
                || v.contains("hsla(");
            if literal {
                out.push(format!(
                    "`{}`: {p}: {v} writes a colour outside the tokens",
                    r.selectors.join(", ")
                ));
            }
        }
    }
    let light = tokens(&rules, Theme::Light);
    let dark_only: BTreeMap<String, String> = rules
        .iter()
        .filter(|r| r.media == [DARK_MEDIA] && r.selectors.iter().any(|s| s == ":root"))
        .flat_map(|r| r.decls.iter().cloned())
        .collect();
    let is_colour = |v: &str| v.starts_with('#') || v.contains('(');
    for (name, value) in &light {
        if is_colour(value) && !dark_only.contains_key(name) {
            out.push(format!("{name} has no value in the dark scheme"));
        }
    }
    for name in dark_only.keys() {
        if !light.contains_key(name) {
            out.push(format!("{name} has no value in the light scheme"));
        }
    }
    if light.is_empty() {
        out.push("there are no tokens in :root".into());
    }
    out
}

#[test]
fn the_reader_reads_nested_blocks_and_comments() {
    let rules = parse(
        "/* a { color: red } */ a,b{color:#fff;margin:0} @media (a: b) { @media (c) { x{y:z} } }",
    );
    assert_eq!(rules.len(), 2);
    let first = rules.first().unwrap();
    assert_eq!(first.selectors, ["a", "b"]);
    assert_eq!(first.get("color"), Some("#fff"));
    assert_eq!(rules.get(1).unwrap().media, ["(a:b)", "(c)"]);
    assert_eq!(parse_hex("#fff"), Ok((255, 255, 255)));
    assert_eq!(parse_hex("#1a2b3c"), Ok((0x1a, 0x2b, 0x3c)));
    assert!(parse_hex("#12").is_err() && parse_hex("red").is_err() && parse_hex("#ggg").is_err());
    assert!((ratio((0, 0, 0), (255, 255, 255)) - 21.0).abs() < 1e-9);
}
