//! Maintenance M34: no test of a 0.57.0 subject passes by not reaching it.
//!
//! A test that asks which prikk it runs over (a capability probe: `probe_version`, `prikk_where`,
//! `real_prikk_at_least_057`, `reports_block_seals`, ...) and then returns early must print one of the two **guarded
//! lines** first: the with-prikk run in CI fails on `skipping: prikk 0.57.0 or newer not found` and on `skipping: no
//! prikk older than 0.57.0`, so a test of a 0.57.0 subject cannot pass silently where it did not run. Review result Z-a
//! found one that did (`if !w.prikk_reports_block_seals() { return; }`).
//!
//! The check reads the Rust sources of the workspace, without a parser: comments, strings and character literals are
//! blanked, each function is found by its braces, and in a function that holds a probe every `return` after the first
//! probe must have a guarded line between the `{` that opens the block it is in and itself. It is heuristic and
//! strict: a `return` it cannot place is reported, and the fix is to print the line (or to move the probe out of the
//! function that returns). It lives in its own test file, a function of its own, so the workflow rules beside it do
//! not share its code.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use inklkunn_floor_check::workspace_root;

/// What asks which prikk a test runs over. (A substring: `prikk_reports_block_seals(` is a probe too.)
const PROBES: &[&str] = &[
    "probe_version(",
    "parse_version(",
    "real_prikk_at_least_057(",
    "prikk_where(",
    "reports_block_seals(",
];

/// The lines the with-prikk run in CI fails on (`ci.yml`, "Test with prikk").
const GUARDED: &[&str] = &[
    "skipping: prikk 0.57.0 or newer not found",
    "skipping: no prikk older than 0.57.0",
];

/// The source twice, same length and same offsets: with comments blanked and strings kept (to find the guarded
/// lines), and with comments, strings and character literals blanked (to find code, braces and keywords).
struct Views {
    text: Vec<char>,
    code: Vec<char>,
}

fn blank(c: char) -> char {
    if c == '\n' { '\n' } else { ' ' }
}

fn is_ident(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

fn views(src: &str) -> Views {
    let chars: Vec<char> = src.chars().collect();
    let at = |k: usize| chars.get(k).copied();
    let (mut text, mut code) = (Vec::new(), Vec::new());
    let mut i = 0;
    while let Some(c) = at(i) {
        // A line comment.
        if c == '/' && at(i + 1) == Some('/') {
            while let Some(d) = at(i) {
                if d == '\n' {
                    break;
                }
                text.push(' ');
                code.push(' ');
                i += 1;
            }
            continue;
        }
        // A block comment (they nest).
        if c == '/' && at(i + 1) == Some('*') {
            let mut depth = 0;
            while let Some(d) = at(i) {
                if d == '/' && at(i + 1) == Some('*') {
                    depth += 1;
                    for _ in 0..2 {
                        text.push(' ');
                        code.push(' ');
                    }
                    i += 2;
                } else if d == '*' && at(i + 1) == Some('/') {
                    depth -= 1;
                    for _ in 0..2 {
                        text.push(' ');
                        code.push(' ');
                    }
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    text.push(blank(d));
                    code.push(blank(d));
                    i += 1;
                }
            }
            continue;
        }
        // A raw string: r"..", r#".."#, br".." .
        if c == 'r' && !is_ident(i.checked_sub(1).and_then(at).filter(|p| *p != 'b')) {
            let mut hashes = 0;
            while at(i + 1 + hashes) == Some('#') {
                hashes += 1;
            }
            if at(i + 1 + hashes) == Some('"') {
                for k in 0..(hashes + 2) {
                    let d = at(i + k).unwrap_or(' ');
                    text.push(d);
                    code.push(d);
                }
                i += hashes + 2;
                while let Some(d) = at(i) {
                    let closes = d == '"' && (0..hashes).all(|h| at(i + 1 + h) == Some('#'));
                    if closes {
                        for k in 0..(hashes + 1) {
                            let e = at(i + k).unwrap_or(' ');
                            text.push(e);
                            code.push(e);
                        }
                        i += hashes + 1;
                        break;
                    }
                    text.push(d);
                    code.push(blank(d));
                    i += 1;
                }
                continue;
            }
        }
        // A string.
        if c == '"' {
            text.push(c);
            code.push(c);
            i += 1;
            while let Some(d) = at(i) {
                if d == '\\' {
                    let e = at(i + 1).unwrap_or(' ');
                    text.push(d);
                    text.push(e);
                    code.push(' ');
                    code.push(blank(e));
                    i += 2;
                    continue;
                }
                if d == '"' {
                    text.push(d);
                    code.push(d);
                    i += 1;
                    break;
                }
                text.push(d);
                code.push(blank(d));
                i += 1;
            }
            continue;
        }
        // A character literal (a lifetime is not one).
        if c == '\'' {
            let len = if at(i + 1) == Some('\\') {
                // '\n', '\'', '\u{..}': up to the closing quote after the escaped character.
                let mut k = i + 3;
                while at(k).is_some_and(|d| d != '\'') {
                    k += 1;
                }
                Some(k + 1 - i)
            } else if at(i + 2) == Some('\'') && at(i + 1) != Some('\'') {
                Some(3)
            } else {
                None
            };
            if let Some(len) = len {
                for k in 0..len {
                    let d = at(i + k).unwrap_or(' ');
                    text.push(d);
                    code.push(if k == 0 || k + 1 == len { d } else { ' ' });
                }
                i += len;
                continue;
            }
        }
        text.push(c);
        code.push(c);
        i += 1;
    }
    Views { text, code }
}

fn find_from(hay: &[char], from: usize, to: usize, needle: &str) -> Option<usize> {
    let needle: Vec<char> = needle.chars().collect();
    let window = hay.get(from..to.min(hay.len()))?;
    window
        .windows(needle.len())
        .position(|w| w == needle.as_slice())
        .map(|p| p + from)
}

fn line_of(chars: &[char], offset: usize) -> usize {
    1 + chars.iter().take(offset).filter(|c| **c == '\n').count()
}

/// The body of every function: (name, offset of its `{`, offset after its `}`).
fn functions(code: &[char]) -> Vec<(String, usize, usize)> {
    let at = |k: usize| code.get(k).copied();
    let mut out = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let is_fn = at(i) == Some('f')
            && at(i + 1) == Some('n')
            && at(i + 2).is_some_and(char::is_whitespace)
            && !is_ident(i.checked_sub(1).and_then(at));
        if !is_fn {
            i += 1;
            continue;
        }
        let mut k = i + 3;
        while at(k).is_some_and(char::is_whitespace) {
            k += 1;
        }
        let name_start = k;
        while is_ident(at(k)) {
            k += 1;
        }
        let name: String = code.get(name_start..k).unwrap_or_default().iter().collect();
        // The signature ends at `{` (a body) or `;` (a declaration), outside parentheses and brackets.
        let mut depth = 0_i32;
        let open = loop {
            match at(k) {
                None => break None,
                Some('(' | '[') => depth += 1,
                Some(')' | ']') => depth -= 1,
                Some('{') if depth == 0 => break Some(k),
                Some(';') if depth == 0 => break None,
                _ => {}
            }
            k += 1;
        };
        i += 3;
        let Some(open) = open else {
            continue;
        };
        let mut braces = 0_i32;
        let mut end = open;
        while let Some(c) = at(end) {
            match c {
                '{' => braces += 1,
                '}' => {
                    braces -= 1;
                    if braces == 0 {
                        end += 1;
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        out.push((name, open, end));
    }
    out
}

/// The `{` of the innermost block that holds `at`, searching back no further than `floor`.
fn enclosing_open(code: &[char], at: usize, floor: usize) -> usize {
    let mut depth = 0_i32;
    for k in (floor..at).rev() {
        match code.get(k) {
            Some('}') => depth += 1,
            Some('{') if depth == 0 => return k,
            Some('{') => depth -= 1,
            _ => {}
        }
    }
    floor
}

/// What the check found in one source: the `return`s after a probe with no guarded line before them in their block
/// (as `line: in fn name`), and how many probe functions and guarded returns it saw (so a vacuous pass is visible).
#[derive(Debug, Default, PartialEq, Eq)]
struct Findings {
    unguarded: Vec<String>,
    probe_functions: usize,
    guarded_returns: usize,
}

fn check_source(src: &str) -> Findings {
    let v = views(src);
    let mut out = Findings::default();
    for (name, open, end) in functions(&v.code) {
        let first_probe = PROBES
            .iter()
            .filter_map(|p| find_from(&v.code, open, end, p))
            .min();
        let Some(first_probe) = first_probe else {
            continue;
        };
        out.probe_functions += 1;
        let mut from = first_probe;
        while let Some(r) = find_from(&v.code, from, end, "return") {
            from = r + 6;
            let word = !is_ident(r.checked_sub(1).and_then(|p| v.code.get(p).copied()))
                && !is_ident(v.code.get(r + 6).copied());
            if !word {
                continue;
            }
            let block = enclosing_open(&v.code, r, open);
            let guarded = GUARDED
                .iter()
                .any(|g| find_from(&v.text, block, r, g).is_some());
            if guarded {
                out.guarded_returns += 1;
            } else {
                out.unguarded
                    .push(format!("{}: in fn {name}", line_of(&v.code, r)));
            }
        }
    }
    out
}

/// A source that holds tests: `tests.rs`, or anything under a `tests` directory. (The product's own code asks which
/// prikk it has too, and returns for its own reasons.)
fn is_test_source(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == "tests.rs")
        || path.components().any(|c| c.as_os_str() == "tests")
}

/// Every test source of the workspace's crates and tools except this crate's (which holds planted sources) and the
/// build output.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if p.is_dir() {
            if !matches!(name, "target" | ".git" | "floor-check") {
                sources(&p, out);
            }
        } else if p.extension().is_some_and(|e| e == "rs") && is_test_source(&p) {
            out.push(p);
        }
    }
}

#[test]
fn no_test_returns_early_after_a_prikk_capability_probe_without_printing_a_guarded_line() {
    let root = workspace_root();
    let mut files = Vec::new();
    for top in ["crates", "tools"] {
        sources(&root.join(top), &mut files);
    }
    let (mut probe_functions, mut guarded_returns) = (0, 0);
    let mut unguarded = Vec::new();
    for f in &files {
        let src = fs::read_to_string(f).expect("a source file");
        let found = check_source(&src);
        probe_functions += found.probe_functions;
        guarded_returns += found.guarded_returns;
        let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
        unguarded.extend(found.unguarded.into_iter().map(|u| format!("{rel}:{u}")));
    }
    // Non-vacuity: the check saw the probes and the guarded returns that exist (the floor/0.57.0 helpers of the
    // signer, the core recovery tests and the command-line tests alone give more than this).
    assert!(
        files.len() > 200,
        "only {} sources were read: the walk is broken",
        files.len()
    );
    assert!(
        probe_functions >= 25,
        "only {probe_functions} functions with a probe were found: the probes are not recognised"
    );
    assert!(
        guarded_returns >= 10,
        "only {guarded_returns} guarded returns were found: the guarded lines are not recognised"
    );
    assert!(
        unguarded.is_empty(),
        "a return after a prikk-capability probe prints no guarded line (a test of a 0.57.0 subject must not pass by \
         not reaching it; print `skipping: prikk 0.57.0 or newer not found` or `skipping: no prikk older than \
         0.57.0`):\n{}",
        unguarded.join("\n")
    );
}

#[test]
fn the_guarded_line_check_catches_planted_failures() {
    let guarded_new = "println!(\"skipping: prikk 0.57.0 or newer not found\");";
    let guarded_old =
        "println!(\"skipping: no prikk older than 0.57.0 (and at the floor) found\");";
    // The shapes that are right.
    let right = [
        // A world constructor: the probe, the line, the return.
        format!(
            "fn new_057() -> Option<u8> {{\n let Some(r) = real_prikk_at_least_057() else {{\n {guarded_new}\n return None;\n }};\n Some(r)\n}}\n"
        ),
        format!(
            "fn floor() -> Option<u8> {{\n if probe_version(x).is_none_or(|v| v >= (0, 57, 0)) {{\n {guarded_old}\n return None;\n }}\n Some(1)\n}}\n"
        ),
        // A return before the probe, and a function with a probe and no return.
        "fn a() {\n if nothing() { return; }\n let _ = probe_version(x);\n}\n".to_owned(),
        // A `return` word inside a name, a string and a comment is not a return.
        "fn b() {\n let _ = probe_version(x);\n let returned = \"return\"; // return\n let _ = returned;\n}\n"
            .to_owned(),
    ];
    for src in &right {
        assert_eq!(check_source(src).unguarded, Vec::<String>::new(), "{src}");
    }
    assert_eq!(check_source(right.first().unwrap()).guarded_returns, 1);
    // The shapes that are wrong, the first being review result Z-a's.
    let wrong = [
        (
            "a silent return after a block_seals probe (Z-a)",
            "fn a_block_prikk_cannot_read() {\n let w = world();\n if !w.prikk_reports_block_seals() {\n return;\n }\n garbage();\n}\n"
                .to_owned(),
        ),
        (
            "the same on one line",
            "fn t() {\n if !w.reports_block_seals() { return; }\n}\n".to_owned(),
        ),
        (
            "a let-else that returns without a line",
            "fn t() {\n let Some(p) = prikk_where(|v| v >= (0, 57, 0)) else {\n return;\n };\n}\n"
                .to_owned(),
        ),
        (
            "a line that is not a guarded one",
            "fn t() {\n let Some(p) = prikk_where(|v| v >= (0, 57, 0)) else {\n println!(\"skipping: prikk not on PATH\");\n return;\n };\n}\n"
                .to_owned(),
        ),
        (
            "a guarded line in another block than the return's",
            format!(
                "fn t() {{\n if other() {{\n {guarded_new}\n }}\n if !w.reports_block_seals() {{\n return;\n }}\n}}\n"
            ),
        ),
        (
            "a guarded line only in a comment",
            "fn t() {\n if !w.reports_block_seals() {\n // println!(\"skipping: prikk 0.57.0 or newer not found\");\n return;\n }\n}\n"
                .to_owned(),
        ),
        (
            "a match arm that returns",
            "fn t() {\n match probe_version(x) {\n None => return,\n Some(_) => {}\n }\n}\n".to_owned(),
        ),
        (
            "a return after the guarded block",
            format!(
                "fn t() {{\n if probe_version(x).is_none() {{\n {guarded_new}\n }}\n if other() {{\n return;\n }}\n}}\n"
            ),
        ),
    ];
    for (what, src) in &wrong {
        assert_eq!(
            check_source(src).unguarded.len(),
            1,
            "{what}: not reported\n{src}"
        );
    }
}
