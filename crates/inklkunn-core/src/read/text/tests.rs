//! The gate of the one reader (RFC 006 D-8; INV-13; handoff G-T4, corrections H-1 and J-3): **no module but the reader reaches the
//! content of a block**. A text read from a block anywhere else would be a text read past the check of what moderation hides, so
//! each way to reach content is found in the sources, and a call outside the places that may make it fails the gate, as the
//! boundary gates fail a route registered outside `routes.rs`. The ways are six: the storage's `read` (a file's bytes), its
//! `materialize` and the service's `with_tree` and `with_block_tree` (the whole tree of a block, which an index is built from), its
//! `diff` (the hunks of a difference, which are the text of both sides), and a `Tip {` literal (a tip is read unchecked, because
//! the content of a view's tip cannot be hidden, so no module may call a block a tip). Each has its list of the places that may,
//! each with its reason, and a planted violation of its own. The scan is over the sources of every crate of the workspace but the
//! driver's (which defines the calls), with comments and strings left out; a test file or test support is no product code.

use std::fs;
use std::path::{Path, PathBuf};

/// The places that may make one kind of call, each with its reason.
type May = &'static [(&'static str, &'static str)];

/// The files that may call `Storage::read`. The reader is the first; the others read to **write** (what the acceptance and a
/// curation proposal compare, and never serve) or hand the call on.
const READ_MAY: May = &[
    (
        "inklkunn-core/src/read/text.rs",
        "the one reader of a file's text at a block",
    ),
    (
        "inklkunn-core/src/read/counting.rs",
        "the counting storage hands each call on to the storage it counts",
    ),
    (
        "inklkunn-core/src/acceptance/prepare.rs",
        "the write path: the pages at the base and at the tip, which the merge compares and the signer writes; nothing is served",
    ),
    (
        "inklkunn-core/src/knowledge/curation.rs",
        "the write path: the standing the tip holds, which a curation proposal is stamped against; nothing is served",
    ),
];

/// The files that may call `Storage::materialize`, the extract of a block's whole tree.
const MATERIALIZE_MAY: May = &[
    (
        "inklkunn-core/src/read.rs",
        "`with_tree`: the one extract an index is built from, of a tip, or of a base after the one reader has been asked of each file of it (`read/past.rs`)",
    ),
    (
        "inklkunn-core/src/read/counting.rs",
        "the counting storage hands each call on to the storage it counts",
    ),
    (
        "inklkunn-core/src/export.rs",
        "the archive of a view's tip, whose content cannot be hidden (the tip check keeps it so); what is hidden of authors and summaries is masked by `export/suppression.rs`",
    ),
    (
        "inklkunn-core/src/import.rs",
        "the tip of the wiki the import has just made, which it judges before it opens the wiki",
    ),
];

/// The files that may call `with_tree`, the service's extract of a block under its build lock.
const WITH_TREE_MAY: May = &[
    (
        "inklkunn-core/src/read.rs",
        "the title and knowledge indexes, whose bases pass the one reader file by file afterwards (`titles_without_hidden`, `knowledge_without_hidden`), and `with_block_tree`",
    ),
    (
        "inklkunn-core/src/read/graph.rs",
        "the link graph of a view's tip, whose content cannot be hidden",
    ),
];

/// The files that may call `with_block_tree`, the extract of any block for a judgement of the write path.
const WITH_BLOCK_TREE_MAY: May = &[(
    "inklkunn-core/src/knowledge/check.rs",
    "the write path: the judgement of what an acceptance would write, of the tip block it has just read; it reports rules and paths, and serves no text",
)];

/// The files that may call `Storage::diff`, whose hunks are the text of both sides.
const DIFF_MAY: May = &[
    (
        "inklkunn-core/src/read/history.rs",
        "`page_diff`: asked only after the one reader has said that neither side's text is hidden",
    ),
    (
        "inklkunn-core/src/read/compare.rs",
        "*Compare views*: the difference of two views' **tips** (`Tip` values only), whose content cannot be hidden",
    ),
    (
        "inklkunn-core/src/read/counting.rs",
        "the counting storage hands each call on to the storage it counts",
    ),
];

/// The files that may write a `Tip {` literal: the module that makes tips.
const TIP_MAY: May = &[(
    "inklkunn-core/src/read/tip.rs",
    "the one place a tip is made, by `ReadService::tip` (and `Tip::unpinned`, which names no block)",
)];

/// Replace comments and the contents of string and character literals with spaces, keeping every newline, so that what is left
/// is code.
fn code_of(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let blank = |out: &mut String, byte: u8| out.push(if byte == b'\n' { '\n' } else { ' ' });
    while let Some(&b) = bytes.get(i) {
        let next = bytes.get(i + 1).copied();
        match (b, next) {
            (b'/', Some(b'/')) => {
                while bytes.get(i).is_some_and(|&c| c != b'\n') {
                    out.push(' ');
                    i += 1;
                }
            }
            (b'/', Some(b'*')) => {
                let mut depth = 0;
                while let Some(&c) = bytes.get(i) {
                    if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        out.push_str("  ");
                        i += 2;
                    } else if c == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        out.push_str("  ");
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        blank(&mut out, c);
                        i += 1;
                    }
                }
            }
            // A character literal (`'"'`, `'\''`) is no string's opening; a lifetime has no closing quote and is left alone.
            (b'\'', _) => {
                let width = match (next, bytes.get(i + 2).copied()) {
                    (Some(b'\\'), _) => bytes
                        .get(i + 2..i + 12)
                        .and_then(|window| window.iter().position(|&c| c == b'\''))
                        .map(|close| close + 3),
                    (Some(_), Some(b'\'')) => Some(3),
                    _ => None,
                };
                match width {
                    Some(width) => {
                        for _ in 0..width {
                            out.push(' ');
                        }
                        i += width;
                    }
                    None => {
                        out.push('\'');
                        i += 1;
                    }
                }
            }
            // A raw string: `r"…"`, `r#"…"#`.
            (b'r', Some(b'"' | b'#'))
                if !out
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_') =>
            {
                let hashes = bytes
                    .get(i + 1..)
                    .unwrap_or_default()
                    .iter()
                    .take_while(|&&c| c == b'#')
                    .count();
                if bytes.get(i + 1 + hashes) == Some(&b'"') {
                    let close: Vec<u8> = std::iter::once(b'"')
                        .chain(std::iter::repeat_n(b'#', hashes))
                        .collect();
                    let body = i + 2 + hashes;
                    let end = bytes
                        .get(body..)
                        .and_then(|rest| rest.windows(close.len()).position(|w| w == close))
                        .map_or(bytes.len(), |at| body + at + close.len());
                    for &c in bytes.get(i..end).unwrap_or_default() {
                        blank(&mut out, c);
                    }
                    i = end;
                } else {
                    out.push('r');
                    i += 1;
                }
            }
            (b'"', _) => {
                out.push('"');
                i += 1;
                while let Some(&c) = bytes.get(i) {
                    if c == b'\\' {
                        out.push(' ');
                        blank(&mut out, bytes.get(i + 1).copied().unwrap_or(b' '));
                        i += 2;
                    } else if c == b'"' {
                        out.push('"');
                        i += 1;
                        break;
                    } else {
                        blank(&mut out, c);
                        i += 1;
                    }
                }
            }
            _ => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    out
}

/// The lines (1-based) at which a call named `name` is made with `arity` arguments (`None`: any number): a method call
/// (`.name(`), or the storage trait's name in a path (`Storage::name(`, whatever the arguments). A definition (`fn name(`), a
/// longer name, a comment and a string are not calls.
fn calls(source: &str, name: &str, arity: Option<usize>) -> Vec<usize> {
    let code = code_of(source);
    let bytes = code.as_bytes();
    let line_of = |at: usize| 1 + code.get(..at).map_or(0, |s| s.matches('\n').count());
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = code.get(from..).and_then(|s| s.find(name)) {
        let start = from + at;
        from = start + name.len();
        let before = code.get(..start).unwrap_or("").trim_end();
        let word_before = before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        let rest = code.get(start + name.len()..).unwrap_or("");
        if word_before || rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = rest.trim_start();
        if !rest.starts_with('(') {
            continue;
        }
        // The trait's own name in a path is a call whatever its arguments.
        if before.ends_with("Storage::") || before.ends_with("Storage >::") {
            found.push(line_of(start));
            continue;
        }
        // A method call: `.name(`, with `fn name(` (a definition) left out.
        if !before.ends_with('.') {
            continue;
        }
        let Some(arity) = arity else {
            found.push(line_of(start));
            continue;
        };
        let open = code.len() - rest.len();
        let (mut depth, mut args, mut any, mut end) = (0_i32, 1_usize, false, open);
        for (n, &c) in bytes.iter().enumerate().skip(open) {
            match c {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = n;
                        break;
                    }
                }
                b',' if depth == 1 => {
                    args += 1;
                    any = false;
                    continue;
                }
                _ => {}
            }
            if !c.is_ascii_whitespace() && depth >= 1 && n > open {
                any = true;
            }
        }
        // A trailing comma is no argument.
        let args = if any { args } else { args.saturating_sub(1) };
        if end > open && args == arity {
            found.push(line_of(start));
        }
    }
    found
}

/// The lines at which a `Tip` is written as a literal (`Tip { … }`, or a pattern of that shape): the word, then `{`, **not** in a
/// definition (`struct Tip {`, `impl Tip {`, `impl … for Tip {`, `enum`, `trait`) and not the opening of a function that returns one
/// (`-> Tip {`).
fn tip_literals(source: &str) -> Vec<usize> {
    let code = code_of(source);
    let line_of = |at: usize| 1 + code.get(..at).map_or(0, |s| s.matches('\n').count());
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = code.get(from..).and_then(|s| s.find("Tip")) {
        let start = from + at;
        from = start + 3;
        let before = code.get(..start).unwrap_or("");
        let word_before = before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        let rest = code.get(start + 3..).unwrap_or("");
        if word_before || rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue;
        }
        if !rest.trim_start().starts_with('{') {
            continue;
        }
        let before = before.trim_end();
        let defined = ["struct", "impl", "for", "enum", "trait", "->"]
            .iter()
            .any(|word| before.ends_with(word));
        if !defined {
            found.push(line_of(start));
        }
    }
    found
}

/// One way to reach a block's content: its name, how it is found in a source, and the places that may.
struct Way {
    name: &'static str,
    find: fn(&str) -> Vec<usize>,
    may: May,
}

fn ways() -> Vec<Way> {
    vec![
        Way {
            name: "the storage's `read` (a file's bytes)",
            find: |s| calls(s, "read", Some(4)),
            may: READ_MAY,
        },
        Way {
            name: "the storage's `materialize` (a block's whole tree)",
            find: |s| calls(s, "materialize", None),
            may: MATERIALIZE_MAY,
        },
        Way {
            name: "`with_tree` (the service's extract of a block)",
            find: |s| calls(s, "with_tree", None),
            may: WITH_TREE_MAY,
        },
        Way {
            name: "`with_block_tree` (the extract of any block)",
            find: |s| calls(s, "with_block_tree", None),
            may: WITH_BLOCK_TREE_MAY,
        },
        Way {
            name: "the storage's `diff` (the hunks of two blocks)",
            find: |s| calls(s, "diff", Some(4)),
            may: DIFF_MAY,
        },
        Way {
            name: "a `Tip {` literal (a block called a tip)",
            find: tip_literals,
            may: TIP_MAY,
        },
    ]
}

/// Every product source of the workspace's crates but the driver's: `(path relative to `crates/`, source)`.
fn product_sources() -> Vec<(String, String)> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(&crates)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    dirs.sort();
    for dir in dirs {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        // The driver defines the call; it is not a reader of a wiki's pages.
        if name == "inklkunn-prikk" {
            continue;
        }
        let mut stack = vec![dir.join("src")];
        while let Some(next) = stack.pop() {
            let Ok(entries) = fs::read_dir(&next) else {
                continue;
            };
            let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            entries.sort();
            for path in entries {
                let file = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if path.is_dir() {
                    // A directory of tests is no product code.
                    if file != "tests" {
                        stack.push(path);
                    }
                } else if file.ends_with(".rs")
                    && !matches!(file, "tests.rs" | "testing.rs" | "support.rs")
                    && let Ok(source) = fs::read_to_string(&path)
                {
                    let relative = path
                        .strip_prefix(&crates)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((relative, source));
                }
            }
        }
    }
    out
}

/// The places a source reaches content a way, not counting the places that may.
fn violations_of(way: &Way, sources: &[(String, String)]) -> Vec<String> {
    sources
        .iter()
        .filter(|(path, _)| !way.may.iter().any(|(allowed, _)| path == allowed))
        .flat_map(|(path, source)| {
            (way.find)(source)
                .into_iter()
                .map(move |line| format!("{path}:{line}"))
        })
        .collect()
}

/// The places a source calls the storage's `read`, not counting the places that may.
fn violations(sources: &[(String, String)]) -> Vec<String> {
    ways()
        .first()
        .map(|way| violations_of(way, sources))
        .unwrap_or_default()
}

#[test]
fn no_module_but_the_reader_reaches_the_content_of_a_block() {
    let sources = product_sources();
    assert!(
        sources.len() > 100,
        "the scan reads the workspace's sources: {} found",
        sources.len()
    );
    let found: Vec<String> = ways()
        .iter()
        .flat_map(|way| {
            violations_of(way, &sources)
                .into_iter()
                .map(move |at| format!("{at} by {}", way.name))
        })
        .collect();
    assert!(
        found.is_empty(),
        "these reach the content of a block outside the places that may ({found:#?}): they would serve a text past the check of what moderation hides; read through `ReadService::text_at` and the types of `read/tip.rs`, or list the place with its reason"
    );
}

#[test]
fn the_places_that_may_do_it_do_it() {
    // A list of places that may names places that do: a stale entry would let a later call in unseen.
    let sources = product_sources();
    for way in ways() {
        for (allowed, why) in way.may {
            let source = sources
                .iter()
                .find(|(path, _)| path == allowed)
                .map(|(_, source)| source.as_str());
            let Some(source) = source else {
                panic!("`{allowed}` is not a source of the workspace ({why})");
            };
            assert!(
                !(way.find)(source).is_empty(),
                "`{allowed}` does not reach content by {}: take it off the list ({why})",
                way.name
            );
        }
    }
}

/// A source of `path` that is one line of code, as a gate sees the workspace.
fn at(path: &str, source: &str) -> Vec<(String, String)> {
    vec![(path.to_owned(), source.to_owned())]
}

#[test]
fn the_scan_finds_a_planted_call_and_not_what_is_not_one() {
    let planted = |source: &str| at("inklkunn-core/src/elsewhere.rs", source);
    // The plain call, the call down a chain, and the call by the trait's name.
    for call in [
        "fn f() { let b = storage.read(repo, &point, path, max); }",
        "fn f() { let b = self\n    .storage\n    .read(\n        &repo,\n        &point,\n        path,\n        self.max,\n    ); }",
        "fn f() { let b = Storage::read(&s, repo, &point, path, max); }",
        "fn f() { let b = self.0.read(repo, point, path, 1,); }",
    ] {
        assert_eq!(violations(&planted(call)).len(), 1, "{call}");
    }
    // The same call in the reader's own module is the reader.
    let reader = at(
        "inklkunn-core/src/read/text.rs",
        "fn f() { let b = storage.read(repo, &point, path, max); }",
    );
    assert!(violations(&reader).is_empty());
    // What is not the storage's call: a stream's, a builder's, a definition, a comment, a string, other arities.
    for fine in [
        "fn f() { file.read(&mut buf); }",
        "fn f() { options.read(true); }",
        "fn f() { trail.read(after, PAGE); }",
        "fn read(&self, repo: &Path, point: &Point, path: &str, max: u64) -> R { todo() }",
        "// storage.read(repo, point, path, max)\nfn f() {}",
        "fn f() { let s = \"storage.read(repo, point, path, max)\"; }",
        "/* storage.read(repo, point, path, max) */ fn f() {}",
        "fn f() { let b = already_read(repo, point, path, max); }",
        "fn f() { let b = reader.read(repo, point, path, max, more); }",
        "fn f() { let q = '\"'; let r = r#\"storage.read(a, b, c, d)\"#; }",
    ] {
        assert!(violations(&planted(fine)).is_empty(), "{fine}");
    }
    // A quote character and a lifetime do not hide a call that follows.
    let after_a_quote =
        "fn f<'a>(x: &'a str) { let q = '\"'; storage.read(repo, &point, path, max); }";
    assert_eq!(violations(&planted(after_a_quote)).len(), 1);
}

/// The way whose name begins with `begins`.
fn way(begins: &str) -> Way {
    ways()
        .into_iter()
        .find(|w| w.name.starts_with(begins))
        .unwrap_or_else(|| panic!("there is no way `{begins}`"))
}

#[test]
fn a_planted_extract_of_a_whole_tree_is_found() {
    // `materialize`, `with_tree` and `with_block_tree` each have a planted violation of their own, and each is quiet where it may be.
    for (begins, call, allowed) in [
        (
            "the storage's `materialize`",
            "fn f() { self.storage.materialize(&repo, &point, &scratch, name, bounds); }",
            "inklkunn-core/src/export.rs",
        ),
        (
            "the storage's `materialize`",
            "fn f() { Storage::materialize(&s, &repo, &point, &scratch, name, bounds); }",
            "inklkunn-core/src/import.rs",
        ),
        (
            "`with_tree`",
            "fn f() { self.with_tree(wiki, &at, |root| Ok(1)); }",
            "inklkunn-core/src/read/graph.rs",
        ),
        (
            "`with_block_tree`",
            "fn f() { reads.with_block_tree(wiki, &at, |root, c| 1); }",
            "inklkunn-core/src/knowledge/check.rs",
        ),
    ] {
        let way = way(begins);
        assert_eq!(
            violations_of(&way, &at("inklkunn-core/src/elsewhere.rs", call)).len(),
            1,
            "{call}"
        );
        assert!(
            violations_of(&way, &at(allowed, call)).is_empty(),
            "{call} in {allowed}"
        );
    }
    // What is not a call of them: a definition, a comment, a string, another name that contains one.
    for (begins, fine) in [
        (
            "the storage's `materialize`",
            "fn materialize(&self, repo: &Path) -> R { todo() }",
        ),
        ("`with_tree`", "fn with_tree<R>(&self, wiki: &W) -> R { x }"),
        ("`with_tree`", "// self.with_tree(wiki, &at, f)\nfn f() {}"),
        ("`with_tree`", "fn f() { self.with_tree_of(wiki); }"),
        (
            "`with_block_tree`",
            "fn f() { let s = \"x.with_block_tree(w, a, f)\"; }",
        ),
    ] {
        assert!(
            violations_of(&way(begins), &at("inklkunn-core/src/elsewhere.rs", fine)).is_empty(),
            "{fine}"
        );
    }
}

#[test]
fn a_planted_diff_is_found() {
    let way = way("the storage's `diff`");
    for call in [
        "fn f() { self.storage.diff(&repo, &from, &to, &[path]); }",
        "fn f() { Storage::diff(&s, &repo, &from, &to, &[]); }",
    ] {
        assert_eq!(
            violations_of(&way, &at("inklkunn-core/src/elsewhere.rs", call)).len(),
            1,
            "{call}"
        );
        assert!(violations_of(&way, &at("inklkunn-core/src/read/history.rs", call)).is_empty());
    }
    // Another `diff` is not the storage's: the arity differs, or it is a definition.
    for fine in [
        "fn f() { words.diff(a, b); }",
        "fn diff(&self, repo: &Path, from: &Point, to: &Point, paths: &[&str]) -> R { todo() }",
        "fn f() { let d = word_diff(a, b, c, d); }",
    ] {
        assert!(
            violations_of(&way, &at("inklkunn-core/src/elsewhere.rs", fine)).is_empty(),
            "{fine}"
        );
    }
}

#[test]
fn a_planted_tip_literal_is_found_and_a_definition_is_not() {
    let way = way("a `Tip {` literal");
    for literal in [
        "fn f() -> R { let tip = Tip { view: v, state_id: s, block_id: b }; tip }",
        "fn f() -> R { Tip {\n        view: v,\n    } }",
        "fn f(t: &Tip) { let Tip { block_id, .. } = t; }",
    ] {
        assert_eq!(
            violations_of(&way, &at("inklkunn-core/src/elsewhere.rs", literal)).len(),
            1,
            "{literal}"
        );
        assert!(violations_of(&way, &at("inklkunn-core/src/read/tip.rs", literal)).is_empty());
    }
    for fine in [
        "pub struct Tip { view: String }",
        "impl Tip { fn f(&self) {} }",
        "impl Place for Tip { fn f(&self) {} }",
        "fn tip(&self) -> Tip { make() }",
        "fn f() -> Result<Tip, E> { make() }",
        "struct RecordedTip { view: String }",
        "fn f() { let x = RecordedTip { view: v }; let y = TipCheck { a: 1 }; }",
        "// Tip { view }\nfn f() {}",
        "fn f() { let s = \"Tip { view }\"; }",
    ] {
        assert!(
            violations_of(&way, &at("inklkunn-core/src/elsewhere.rs", fine)).is_empty(),
            "{fine}"
        );
    }
}

#[test]
fn every_place_that_may_has_a_reason_and_no_list_names_a_place_twice() {
    for way in ways() {
        let mut seen: Vec<&str> = Vec::new();
        for (place, why) in way.may {
            assert!(
                why.split_whitespace().count() >= 4,
                "`{place}` has no reason for {}",
                way.name
            );
            assert!(
                !seen.contains(place),
                "`{place}` is listed twice for {}",
                way.name
            );
            seen.push(place);
        }
    }
}
