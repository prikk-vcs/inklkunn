//! The gate of the one reader (RFC 006 D-8; INV-13; handoff G-T4, correction H-1): **no module but the reader asks the storage for
//! the bytes of a file at a block**. A text read from a block anywhere else would be a text read past the check of what
//! moderation hides, so the call is found in the sources and a call outside the places that may make it fails the gate, as the
//! boundary gates fail a route registered outside `routes.rs`. The scan is over the sources of every crate of the workspace but
//! the driver's (which defines the call), with comments and strings left out; a test file or test support is no product code.

use std::fs;
use std::path::{Path, PathBuf};

/// The files that may call `Storage::read`, with the reason. The reader is the first; the others read to **write** (what the
/// acceptance and a curation proposal compare, and never serve) or hand the call on.
const MAY_CALL: &[(&str, &str)] = &[
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

/// The lines (1-based) at which the storage's `read` is called: a method call named `read` with four arguments (the repository,
/// the point, the path and the bound), or the trait's name in a path (`Storage::read`).
fn read_calls(source: &str) -> Vec<usize> {
    let code = code_of(source);
    let bytes = code.as_bytes();
    let line_of = |at: usize| 1 + code.get(..at).map_or(0, |s| s.matches('\n').count());
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = code.get(from..).and_then(|s| s.find("read")) {
        let start = from + at;
        from = start + 4;
        let before = code.get(..start).unwrap_or("").trim_end();
        let word_before = before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        let rest = code.get(start + 4..).unwrap_or("");
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
        // A method call: `.read(`, with `fn read(` (a definition) left out.
        if !before.ends_with('.') {
            continue;
        }
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
        if end > open && args == 4 {
            found.push(line_of(start));
        }
    }
    found
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

/// The places a source calls the storage's `read`, not counting the places that may.
fn violations(sources: &[(String, String)]) -> Vec<String> {
    sources
        .iter()
        .filter(|(path, _)| !MAY_CALL.iter().any(|(allowed, _)| path == allowed))
        .flat_map(|(path, source)| {
            read_calls(source)
                .into_iter()
                .map(move |line| format!("{path}:{line}"))
        })
        .collect()
}

#[test]
fn no_module_but_the_reader_asks_the_storage_for_a_file_at_a_block() {
    let sources = product_sources();
    assert!(
        sources.len() > 100,
        "the scan reads the workspace's sources: {} found",
        sources.len()
    );
    let found = violations(&sources);
    assert!(
        found.is_empty(),
        "these call the storage's `read` outside the reader (`read/text.rs`): they would serve a text past the check of what moderation hides ({found:?}); read through `ReadService::text_at`"
    );
}

#[test]
fn the_places_that_may_call_it_do_call_it() {
    // A list of places that may call it names places that do: a stale entry would let a later call in unseen.
    let sources = product_sources();
    for (allowed, why) in MAY_CALL {
        let source = sources
            .iter()
            .find(|(path, _)| path == allowed)
            .map(|(_, source)| source.as_str());
        let Some(source) = source else {
            panic!("`{allowed}` is not a source of the workspace ({why})");
        };
        assert!(
            !read_calls(source).is_empty(),
            "`{allowed}` does not call the storage's `read`: take it off the list ({why})"
        );
    }
}

#[test]
fn the_scan_finds_a_planted_call_and_not_what_is_not_one() {
    let planted = |source: &str| {
        vec![(
            "inklkunn-core/src/elsewhere.rs".to_owned(),
            source.to_owned(),
        )]
    };
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
    let reader = vec![(
        "inklkunn-core/src/read/text.rs".to_owned(),
        "fn f() { let b = storage.read(repo, &point, path, max); }".to_owned(),
    )];
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
