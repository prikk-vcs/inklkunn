//! Maintenance M39 (review result Z′-a): a test reaches a prikk capability only through one module of its crate.
//!
//! A test of a 0.57.0 subject must not pass by not reaching it, and a rule that a scanner can check completely beats a
//! scanner that tries to understand every shape of an early exit (M34's looked for the keyword `return` after a
//! handful of probe names and missed eight of the reviewer's nine shapes). So the rule is about where a probe may be
//! named, not about what a test does with its answer:
//!
//! 1. **One module per test crate** (or one shared test-support module) is a file named `prikk_probes.rs`. It holds
//!    every question asked of the prikk on `PATH`: which binary, which version, what it reports, whether it has a
//!    verb.
//! 2. **What a probe module offers is public, and nothing else is.** A `pub fn` returns a binary (an `Option`) and
//!    **prints the guarded line itself** on every path that returns none, or it asserts and returns nothing. A `pub fn`
//!    that returns anything else (a `bool`, a version, a `Result`) is an answer a test could branch on, and fails.
//!    Every other function of the module is a probe, found here by its **definition**: no list is kept by hand.
//!    The free functions of `inklkunn_prikk` that the module calls by path (`probe_version`, `parse_version`) are
//!    probes of every crate in the workspace.
//! 3. **A probe is named only inside its module.** The check fails on any word-for-word use of a probe in another test
//!    source: a call, a function passed as a value, a second definition of the same name. A crate that has no module
//!    may still test its own definition of a name (the prikk driver's unit tests of `parse_version`).
//! 4. **A public `Option` function can return none only after printing.** Its body has no `?`; each `None` and each
//!    `return` has a `println!` of one of the lines CI fails on at the level of its own block before it (a line in a
//!    comment, in a string that is not printed, in another block or after the exit does not count); and its last
//!    expression is `Some(..)`, so a probe's own `Option` cannot leave through the tail.
//!
//! The with-prikk run in CI fails on `skipping: prikk 0.57.0 or newer not found` and on `skipping: no prikk older than
//! 0.57.0`. The reviewer's nine shapes (`if w.reports_block_seals() { subject(&w); }`, `if let Some(p) =
//! prikk_where(..) { .. }`, a `match` with an empty arm, `continue` in a loop, a helper using `?` whose caller's
//! `let .. else { return; }` names no probe, probe names outside the old list such as `at_least_057(`, `world_057(`,
//! `fixture_057(` and `new_057(`, and a guarded string that is never printed) are planted failures below. It lives in
//! its own test file, so the workflow rules beside it do not share its code.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use inklkunn_floor_check::workspace_root;

/// The file name of a probe module.
const PROBE_MODULE: &str = "prikk_probes.rs";

/// The lines a probe module prints before it returns none: the two the with-prikk run in CI fails on (`ci.yml`, "Test
/// with prikk"), and the one for a prikk that is not there at all (counted by `EXPECTED_SKIPS`).
const PRINTED: &[&str] = &[
    "skipping: prikk 0.57.0 or newer not found",
    "skipping: no prikk older than 0.57.0",
    "skipping: prikk not on PATH",
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
/// Every position at which `word` stands as a whole word of `code`.
fn word_positions(code: &[char], word: &str) -> Vec<usize> {
    let len = word.chars().count();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = find_from(code, from, code.len(), word) {
        from = p + 1;
        let before = p.checked_sub(1).and_then(|k| code.get(k).copied());
        if !is_ident(before) && !is_ident(code.get(p + len).copied()) {
            out.push(p);
        }
    }
    out
}

/// A function: where its `fn` stands, where its name ends, its body's `{` and the offset after its `}`.
struct Func {
    name: String,
    start: usize,
    name_end: usize,
    open: usize,
    end: usize,
}

/// Every function with a body, nested ones and methods included.
fn functions(code: &[char]) -> Vec<Func> {
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
        let start = i;
        let mut k = i + 3;
        while at(k).is_some_and(char::is_whitespace) {
            k += 1;
        }
        let name_start = k;
        while is_ident(at(k)) {
            k += 1;
        }
        let name_end = k;
        let name: String = code
            .get(name_start..name_end)
            .unwrap_or_default()
            .iter()
            .collect();
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
        if name.is_empty() {
            continue;
        }
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
        out.push(Func {
            name,
            start,
            name_end,
            open,
            end,
        });
    }
    out
}

/// Is the function declared `pub` (in any of its forms)? The words between the previous `;`, `{` or `}` and the `fn`.
fn is_public(code: &[char], f: &Func) -> bool {
    let mut k = f.start;
    let mut segment = String::new();
    while let Some(p) = k.checked_sub(1) {
        k = p;
        match code.get(k) {
            Some(';' | '{' | '}') | None => break,
            Some(c) => segment.insert(0, *c),
        }
    }
    segment
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|w| w == "pub")
}

/// What the function returns, as written (`None` for a function that returns nothing).
fn return_type(code: &[char], f: &Func) -> Option<String> {
    let sig = code.get(f.name_end..f.open)?;
    let mut depth = 0_i32;
    for (k, c) in sig.iter().enumerate() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            '-' if depth == 0 && sig.get(k + 1) == Some(&'>') => {
                let text: String = sig.get(k + 2..)?.iter().collect();
                let text = text.split(" where ").next().unwrap_or_default();
                return Some(text.trim().to_owned());
            }
            _ => {}
        }
    }
    None
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

/// Is there, at the level of the block opened at `block` (not in a block nested in it), a `println!` of one of the
/// `PRINTED` lines between the `{` and `at`? (Comments and unprinted strings are blank in `code`, and `text` is read
/// only where `code` holds a `println!` call.)
fn printed_before(v: &Views, block: usize, at: usize) -> bool {
    let mut from = block + 1;
    while let Some(p) = find_from(&v.code, from, at, "println!(") {
        from = p + 1;
        let level =
            v.code
                .get(block + 1..p)
                .unwrap_or_default()
                .iter()
                .fold(0_i32, |d, c| match c {
                    '{' => d + 1,
                    '}' => d - 1,
                    _ => d,
                });
        let mut q = p + "println!(".len();
        while v.code.get(q).is_some_and(|c| c.is_whitespace()) {
            q += 1;
        }
        if level == 0 && v.code.get(q) == Some(&'"') {
            let line: String = v
                .text
                .get(q + 1..(q + 80).min(v.text.len()))
                .unwrap_or_default()
                .iter()
                .collect();
            if PRINTED.iter().any(|l| line.starts_with(l)) {
                return true;
            }
        }
    }
    false
}

/// The last expression of a function body: what follows the last `;` or `}` that ends a statement of the body.
fn tail(code: &[char], f: &Func) -> String {
    let (mut braces, mut parens) = (0_i32, 0_i32);
    let mut statement_end = f.open + 1;
    let stop = f.end.saturating_sub(1);
    for k in f.open + 1..stop {
        match code.get(k) {
            Some('{') => braces += 1,
            Some('}') => {
                braces -= 1;
                if braces == 0 && parens == 0 {
                    statement_end = k + 1;
                }
            }
            Some('(' | '[') => parens += 1,
            Some(')' | ']') => parens -= 1,
            Some(';') if braces == 0 && parens == 0 => statement_end = k + 1,
            _ => {}
        }
    }
    let text: String = code
        .get(statement_end..stop)
        .unwrap_or_default()
        .iter()
        .collect();
    text.trim().to_owned()
}

/// What one probe module holds.
#[derive(Debug, Default)]
struct ModuleFacts {
    /// The probes by definition: every function of the module that is not public.
    probes: BTreeSet<String>,
    /// The free functions of `inklkunn_prikk` the module calls by path.
    product_probes: BTreeSet<String>,
    /// Public functions that return an `Option` (a binary), and public functions that assert.
    entries: usize,
    asserts: usize,
    /// Exits (`None`, `return`) of the entries that follow a printed guarded line.
    printed_exits: usize,
    problems: Vec<String>,
}

fn analyse_module(label: &str, src: &str) -> ModuleFacts {
    let v = views(src);
    let funcs = functions(&v.code);
    let mut facts = ModuleFacts::default();
    // The free functions of the product's driver that the module asks by path.
    for p in word_positions(&v.code, "inklkunn_prikk") {
        let mut k = p + "inklkunn_prikk".len();
        if v.code.get(k) != Some(&':') || v.code.get(k + 1) != Some(&':') {
            continue;
        }
        k += 2;
        let name_start = k;
        while is_ident(v.code.get(k).copied()) {
            k += 1;
        }
        while v.code.get(k).is_some_and(|c| c.is_whitespace()) {
            k += 1;
        }
        let name: String = v
            .code
            .get(name_start..k)
            .unwrap_or_default()
            .iter()
            .collect::<String>()
            .trim()
            .to_owned();
        if v.code.get(k) == Some(&'(') && name.chars().next().is_some_and(char::is_lowercase) {
            facts.product_probes.insert(name);
        }
    }
    for f in &funcs {
        if !is_public(&v.code, f) {
            facts.probes.insert(f.name.clone());
        }
    }
    for f in funcs.iter().filter(|f| is_public(&v.code, f)) {
        let line = line_of(&v.code, f.start);
        let name = &f.name;
        let ret = return_type(&v.code, f);
        let Some(ret) = ret else {
            facts.asserts += 1;
            continue;
        };
        if !ret.starts_with("Option<") {
            facts.problems.push(format!(
                "{label}:{line}: `pub fn {name}` returns `{ret}`, an answer a test could branch on (a public function of a \
                 probe module returns an `Option` after printing the guarded line, or returns nothing)"
            ));
            continue;
        }
        facts.entries += 1;
        let body = f.open..f.end;
        if body.clone().any(|k| v.code.get(k) == Some(&'?')) {
            facts.problems.push(format!(
                "{label}:{line}: `{name}` uses `?`, which can return none without printing"
            ));
        }
        for word in ["None", "return"] {
            for p in word_positions(&v.code, word)
                .into_iter()
                .filter(|p| body.contains(p))
            {
                let block = enclosing_open(&v.code, p, f.open);
                if printed_before(&v, block, p) {
                    facts.printed_exits += 1;
                } else {
                    facts.problems.push(format!(
                        "{label}:{}: `{name}` has a `{word}` with no printed guarded line before it in its block",
                        line_of(&v.code, p)
                    ));
                }
            }
        }
        let last = tail(&v.code, f);
        if !last.starts_with("Some(") {
            let shown: String = last.chars().take(40).collect();
            facts.problems.push(format!(
                "{label}:{line}: `{name}` ends in `{shown}`; the last expression of a function that returns an `Option` \
                 must be `Some(..)`"
            ));
        }
    }
    facts
}

/// One source of the workspace: its path from the root (with `/`) and its text.
struct Source {
    path: String,
    text: String,
}

/// A source that holds tests: `tests.rs`, anything under a `tests` directory, or a probe module. (The product's own
/// code asks which prikk it has too, and returns for its own reasons.)
fn is_test_source(path: &str) -> bool {
    let p = Path::new(path);
    p.file_name()
        .is_some_and(|n| n == "tests.rs" || n == PROBE_MODULE)
        || p.components().any(|c| c.as_os_str() == "tests")
}

fn is_probe_module(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .is_some_and(|n| n == PROBE_MODULE)
}

/// The crate or tool a path belongs to: its first two components under `crates/` and `tools/`.
fn package_of(path: &str) -> String {
    path.split('/').take(2).collect::<Vec<_>>().join("/")
}

/// What the check found over a set of sources.
#[derive(Debug, Default)]
struct Report {
    violations: Vec<String>,
    modules: usize,
    entries: usize,
    asserts: usize,
    printed_exits: usize,
    probes: usize,
    product_probes: BTreeSet<String>,
    test_sources: usize,
}

fn check(sources: &[Source]) -> Report {
    let mut report = Report::default();
    // 1. The probe modules: by package, the probes they define; everywhere, the product's functions they call.
    let mut probes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut with_module: BTreeSet<String> = BTreeSet::new();
    for s in sources.iter().filter(|s| is_probe_module(&s.path)) {
        let facts = analyse_module(&s.path, &s.text);
        report.modules += 1;
        report.entries += facts.entries;
        report.asserts += facts.asserts;
        report.printed_exits += facts.printed_exits;
        report.probes += facts.probes.len();
        report.violations.extend(facts.problems);
        report.product_probes.extend(facts.product_probes);
        let package = package_of(&s.path);
        with_module.insert(package.clone());
        probes.entry(package).or_default().extend(facts.probes);
    }
    // 2. A crate without a module may test its own definition of a product probe (the driver's unit tests of
    // `parse_version`): the names its non-test sources define.
    let mut defined: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for s in sources
        .iter()
        .filter(|s| !is_test_source(&s.path) && !with_module.contains(&package_of(&s.path)))
    {
        let v = views(&s.text);
        defined
            .entry(package_of(&s.path))
            .or_default()
            .extend(functions(&v.code).into_iter().map(|f| f.name));
    }
    // 3. No other test source names a probe.
    for s in sources
        .iter()
        .filter(|s| is_test_source(&s.path) && !is_probe_module(&s.path))
    {
        report.test_sources += 1;
        let package = package_of(&s.path);
        let own = defined.get(&package);
        let mut names: BTreeSet<&String> = probes.get(&package).into_iter().flatten().collect();
        names.extend(
            report
                .product_probes
                .iter()
                .filter(|n| !own.is_some_and(|d| d.contains(*n))),
        );
        let v = views(&s.text);
        let mut found = Vec::new();
        for name in names {
            for p in word_positions(&v.code, name) {
                found.push(format!(
                    "{}:{}: `{name}` is a prikk-capability probe and is named only in the crate's `{PROBE_MODULE}`",
                    s.path,
                    line_of(&v.code, p)
                ));
            }
        }
        report.violations.extend(found);
    }
    report.violations.sort();
    report
}

/// Every Rust source of the workspace's crates and tools except this crate's (which holds planted sources) and the
/// build output.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if p.is_dir() {
            if !matches!(name, "target" | ".git" | "floor-check") {
                walk(&p, out);
            }
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

fn workspace_sources() -> Vec<Source> {
    let root = workspace_root();
    let mut files = Vec::new();
    for top in ["crates", "tools"] {
        walk(&root.join(top), &mut files);
    }
    files
        .iter()
        .map(|f| Source {
            path: f
                .strip_prefix(&root)
                .unwrap_or(f)
                .to_string_lossy()
                .replace('\\', "/"),
            text: fs::read_to_string(f).expect("a source file"),
        })
        .collect()
}

#[test]
fn a_prikk_capability_is_reached_only_through_the_probe_module_of_each_test_crate() {
    let report = check(&workspace_sources());
    // Non-vacuity: the check found the modules, what they offer and what they print (the core recovery tests, the
    // signer's unit and integration tests and the command-line tests each have one), and read the tests.
    assert!(
        report.test_sources > 200,
        "only {} test sources were read: the walk is broken",
        report.test_sources
    );
    assert!(
        report.modules >= 4,
        "only {} probe modules were found",
        report.modules
    );
    assert!(
        report.entries >= 9,
        "only {} public probe functions that return a binary were found",
        report.entries
    );
    assert!(
        report.asserts >= 3,
        "only {} asserting probe functions were found",
        report.asserts
    );
    assert!(
        report.printed_exits >= 12,
        "only {} exits after a printed guarded line were found",
        report.printed_exits
    );
    assert!(
        report.probes >= 10,
        "only {} probes were found by their definition",
        report.probes
    );
    assert!(
        report.product_probes.len() >= 2,
        "the version probes the modules call by path were not found: {:?}",
        report.product_probes
    );
    assert!(
        report.violations.is_empty(),
        "a test reaches a prikk capability outside its crate's `{PROBE_MODULE}`, or a function of a probe module can \
         return none without printing the guarded line (`skipping: prikk 0.57.0 or newer not found`, `skipping: no \
         prikk older than 0.57.0`):\n{}",
        report.violations.join("\n")
    );
}

/// A probe module that is right in every shape the check accepts.
const RIGHT_MODULE: &str = r#"
use std::path::{Path, PathBuf};

type Version = (u64, u64, u64);

fn prikk_where(wanted: impl Fn(Version) -> bool) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("prikk"))
        .find(|c| inklkunn_prikk::probe_version(c.as_os_str()).is_some_and(&wanted))
}

fn at_least_057(binary: &Path) -> bool {
    inklkunn_prikk::probe_version(binary.as_os_str()).is_some_and(|v| v >= (0, 57, 0))
}

fn real_prikk_at_least_057() -> Option<PathBuf> {
    prikk_where(|v| v >= (0, 57, 0))
}

impl World {
    fn reports_block_seals(&self) -> bool {
        inklkunn_prikk::parse_version(self.version()).is_some_and(|v| v >= (0, 57, 0))
    }

    fn has_uncommit(&self) -> bool {
        self.witnesses().has_uncommit(&self.id).unwrap()
    }

    pub fn assert_side(&self, reports: bool) {
        assert_eq!(self.reports_block_seals(), reports);
    }
}

fn world_057() {}
fn fixture_057() {}
fn new_057() {}

/// The first prikk 0.57.0 or newer.
pub fn prikk_057() -> Option<PathBuf> {
    let Some(found) = real_prikk_at_least_057() else {
        println!("skipping: prikk 0.57.0 or newer not found");
        return None;
    };
    Some(found)
}

pub fn prikk_floor() -> Option<PathBuf> {
    let Some(first) = prikk_where(|_| true) else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    if at_least_057(&first) {
        println!("skipping: no prikk older than 0.57.0 (and at the floor) found");
        return None;
    }
    Some(first)
}
"#;

const MODULE_PATH: &str = "crates/p/tests/support/prikk_probes.rs";

/// A test source that goes through the module, as a test should.
const RIGHT_TEST: &str = r#"
#[path = "support/prikk_probes.rs"]
mod prikk_probes;

// prikk_where, at_least_057 and world_057 are named in this comment, and in the string below, only.
fn fixture_with(tag: &str) -> Option<Fixture> {
    let real = prikk_probes::prikk_057()?;
    fixture_over(tag, real)
}

#[test]
fn a_test() {
    let Some(f) = fixture_with("x") else {
        return;
    };
    let _ = f.go("prikk_where at_least_057");
    w.assert_side(true);
}
"#;

fn run(files: &[(&str, &str)]) -> Report {
    let sources: Vec<Source> = files
        .iter()
        .map(|(path, text)| Source {
            path: (*path).to_owned(),
            text: (*text).to_owned(),
        })
        .collect();
    check(&sources)
}

#[test]
fn the_shapes_that_are_right_pass() {
    let report = run(&[
        (MODULE_PATH, RIGHT_MODULE),
        ("crates/p/tests/a.rs", RIGHT_TEST),
    ]);
    assert_eq!(report.violations, Vec::<String>::new());
    assert_eq!(report.modules, 1);
    assert_eq!(report.entries, 2);
    assert_eq!(report.asserts, 1);
    // Each exit of an entry is a `return` and a `None`, both after a printed line.
    assert_eq!(report.printed_exits, 6);
    // The probes are what the module defines and does not make public, whatever their names.
    let facts = analyse_module(MODULE_PATH, RIGHT_MODULE);
    assert_eq!(
        facts.probes,
        [
            "prikk_where",
            "at_least_057",
            "real_prikk_at_least_057",
            "reports_block_seals",
            "has_uncommit",
            "world_057",
            "fixture_057",
            "new_057",
        ]
        .map(str::to_owned)
        .into()
    );
    assert_eq!(
        report.product_probes,
        BTreeSet::from(["parse_version".to_owned(), "probe_version".to_owned()])
    );
    // A crate with no module tests its own definition of a product probe (the driver's unit tests of `parse_version`)...
    let report = run(&[
        (MODULE_PATH, RIGHT_MODULE),
        (
            "crates/d/src/driver.rs",
            "pub fn parse_version(text: &str) -> Option<u8> { None }\n",
        ),
        (
            "crates/d/src/driver/tests.rs",
            "#[test]\nfn parses() { assert_eq!(parse_version(\"prikk 0.54.1\"), None); }\n",
        ),
    ]);
    assert_eq!(report.violations, Vec::<String>::new());
}

/// Run `test` (a test source of the crate that has the right module) and report what the check says of it.
fn violations_of_test(test: &str) -> Vec<String> {
    run(&[(MODULE_PATH, RIGHT_MODULE), ("crates/p/tests/a.rs", test)]).violations
}

#[test]
fn the_reviewers_shapes_are_planted_failures_of_the_rule() {
    let wrong = [
        // Z′-a, shape 1: a test branches on the answer and skips its subject without a word.
        (
            "an `if` on reports_block_seals",
            "fn t() {\n if w.reports_block_seals() { subject(&w); }\n}\n",
            "reports_block_seals",
        ),
        (
            "an `if let` on prikk_where",
            "fn t() {\n if let Some(p) = prikk_where(|v| v >= (0, 57, 0)) { subject(p); }\n}\n",
            "prikk_where",
        ),
        (
            "a `match` with an empty arm",
            "fn t() {\n match prikk_where(|v| v >= (0, 57, 0)) {\n Some(p) => subject(p),\n None => {}\n }\n}\n",
            "prikk_where",
        ),
        (
            "`continue` in a loop",
            "fn t() {\n for tag in tags {\n let Some(p) = prikk_where(|v| v >= (0, 57, 0)) else {\n continue;\n };\n subject(p, tag);\n }\n}\n",
            "prikk_where",
        ),
        (
            "a helper using `?` whose caller's let-else names no probe",
            "fn helper() -> Option<Fixture> {\n let p = prikk_where(|v| v >= (0, 57, 0))?;\n Some(fixture(p))\n}\n#[test]\nfn t() {\n let Some(f) = helper() else {\n return;\n };\n subject(f);\n}\n",
            "prikk_where",
        ),
        // Probe names outside the old list, found by their definition in the module.
        (
            "at_least_057",
            "fn t() {\n let new = found.into_iter().find(|c| at_least_057(c));\n}\n",
            "at_least_057",
        ),
        (
            "world_057",
            "fn t() {\n let Some(w) = world_057(\"x\") else {\n return;\n };\n}\n",
            "world_057",
        ),
        (
            "fixture_057",
            "fn t() {\n let Some(f) = fixture_057(\"x\") else {\n return;\n };\n}\n",
            "fixture_057",
        ),
        (
            "new_057",
            "fn t() {\n let Some(w) = World::new_057(\"x\", lease) else {\n return;\n };\n}\n",
            "new_057",
        ),
        (
            "real_prikk_at_least_057",
            "fn t() {\n let _ = real_prikk_at_least_057();\n}\n",
            "real_prikk_at_least_057",
        ),
        (
            "has_uncommit",
            "fn t() {\n assert!(witnesses.has_uncommit(&id).unwrap());\n}\n",
            "has_uncommit",
        ),
        (
            "a probe passed as a value",
            "fn t() {\n let new = found.into_iter().find(at_least_057);\n}\n",
            "at_least_057",
        ),
        (
            "a second definition of a probe name",
            "fn prikk_where(wanted: impl Fn(u8) -> bool) -> Option<u8> {\n None\n}\n",
            "prikk_where",
        ),
        // The product's version probes, asked of the binary directly.
        (
            "probe_version by path",
            "fn t() {\n let v = inklkunn_prikk::probe_version(binary.as_os_str());\n}\n",
            "probe_version",
        ),
        (
            "parse_version by path",
            "fn t() {\n let v = inklkunn_prikk::parse_version(w.storage.version());\n}\n",
            "parse_version",
        ),
    ];
    for (what, test, name) in wrong {
        let found = violations_of_test(test);
        assert!(
            found.iter().any(|v| v.contains(&format!("`{name}`"))),
            "{what}: not reported ({found:?})\n{test}"
        );
    }
    // A crate with no module is held to the product's version probes, unless it tests its own definition.
    let report = run(&[
        (MODULE_PATH, RIGHT_MODULE),
        (
            "crates/q/tests/b.rs",
            "fn t() {\n let _ = inklkunn_prikk::probe_version(b);\n let _ = parse_version(\"x\");\n}\n",
        ),
        ("crates/q/src/other.rs", "pub fn something_else() {}\n"),
    ]);
    assert_eq!(report.violations.len(), 2, "{:?}", report.violations);
    // The probes of one crate are not the names of another (no module, no probe by definition).
    let report = run(&[
        (MODULE_PATH, RIGHT_MODULE),
        (
            "crates/q/tests/b.rs",
            "fn at_least_057(b: &Path) -> bool {\n true\n}\n",
        ),
    ]);
    assert_eq!(report.violations, Vec::<String>::new());
}

#[test]
fn a_public_function_that_can_return_none_without_printing_is_a_planted_failure() {
    let wrong = [
        (
            "a guarded string that is never printed",
            "pub fn e() -> Option<PathBuf> {\n let _why = \"skipping: prikk 0.57.0 or newer not found\";\n return None;\n}\n",
            "`return`",
        ),
        (
            "a guarded line in a comment only",
            "pub fn e() -> Option<PathBuf> {\n // println!(\"skipping: prikk 0.57.0 or newer not found\");\n return None;\n}\n",
            "`return`",
        ),
        (
            "a line that is not a guarded one",
            "pub fn e() -> Option<PathBuf> {\n println!(\"skipping: no /dev/shm here\");\n return None;\n}\n",
            "`return`",
        ),
        (
            "a guarded line printed in a block that is not the exit's",
            "pub fn e() -> Option<PathBuf> {\n if other() {\n println!(\"skipping: prikk 0.57.0 or newer not found\");\n }\n return None;\n}\n",
            "`return`",
        ),
        (
            "a guarded line printed after the exit",
            "pub fn e() -> Option<PathBuf> {\n if other() {\n return None;\n println!(\"skipping: prikk 0.57.0 or newer not found\");\n }\n Some(p)\n}\n",
            "`return`",
        ),
        (
            "`?`",
            "pub fn e() -> Option<PathBuf> {\n let p = prikk_where(|v| v >= (0, 57, 0))?;\n Some(p)\n}\n",
            "uses `?`",
        ),
        (
            "a probe's own option as the last expression",
            "pub fn e() -> Option<PathBuf> {\n prikk_where(|v| v >= (0, 57, 0))\n}\n",
            "must be `Some(..)`",
        ),
        (
            "an option held in a variable",
            "pub fn e() -> Option<PathBuf> {\n let p = prikk_where(|v| v >= (0, 57, 0));\n p\n}\n",
            "must be `Some(..)`",
        ),
        (
            "a match whose arm is none",
            "pub fn e() -> Option<PathBuf> {\n match prikk_where(|v| v >= (0, 57, 0)) {\n Some(p) => Some(p),\n None => None,\n }\n}\n",
            "`None`",
        ),
        (
            "a none that is only a value",
            "pub fn e() -> Option<PathBuf> {\n let r: Option<PathBuf> = None;\n Some(r.unwrap_or_default())\n}\n",
            "`None`",
        ),
        (
            "a public bool",
            "pub fn reports(&self) -> bool {\n true\n}\n",
            "returns `bool`",
        ),
        (
            "a public version",
            "pub fn version(binary: &Path) -> Option<(u64, u64, u64)> {\n println!(\"skipping: prikk not on PATH\");\n Some((0, 57, 0))\n}\npub fn other() -> (u64, u64, u64) {\n (0, 57, 0)\n}\n",
            "returns `(u64, u64, u64)`",
        ),
        (
            "a public result",
            "pub fn world() -> Result<World, String> {\n Ok(w)\n}\n",
            "returns `Result<World, String>`",
        ),
    ];
    for (what, module, expected) in wrong {
        let report = run(&[("crates/p/tests/support/prikk_probes.rs", module)]);
        assert!(
            report.violations.iter().any(|v| v.contains(expected)),
            "{what}: not reported ({:?})\n{module}",
            report.violations
        );
    }
    // The shapes that are right: every exit after its own printed line, in its own block.
    let right = [
        "pub fn e() -> Option<PathBuf> {\n let Some(p) = raw() else {\n println!(\"skipping: prikk 0.57.0 or newer not found\");\n return None;\n };\n Some(p)\n}\n",
        "pub fn e() -> Option<PathBuf> {\n if bad() {\n println!(\"skipping: no prikk older than 0.57.0 (and at the floor) found\");\n return None;\n }\n Some(p)\n}\n",
        "pub fn e() -> Option<PathBuf> {\n let first = raw();\n if first.is_none() {\n println!(\"skipping: prikk not on PATH\");\n return None;\n }\n Some(first.unwrap())\n}\n",
        "pub fn assert_it(w: &World) {\n assert!(raw(w));\n}\n",
    ];
    for module in right {
        let report = run(&[("crates/p/tests/support/prikk_probes.rs", module)]);
        assert_eq!(report.violations, Vec::<String>::new(), "{module}");
    }
}
