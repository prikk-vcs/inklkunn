//! The gate of the one reader (RFC 006 D-8; INV-13; handoff G-T4, corrections H-1, J-3 and L-4): **no module but the reader reaches the
//! content of a block**. A text read from a block anywhere else would be a text read past the check of what moderation hides, so
//! each way to reach content is found in the sources, and a call outside the places that may make it fails the gate, as the
//! boundary gates fail a route registered outside `routes.rs`. The ways are six: the storage's `read` (a file's bytes), its
//! `materialize` and the service's `with_tree` and `with_block_tree` (the whole tree of a block, which an index is built from), its
//! `diff` (the hunks of a difference, which are the text of both sides), and a `Tip {` literal (a tip is read unchecked, because
//! the content of a view's tip cannot be hidden, so no module may call a block a tip). Each has its list of the places that may,
//! each with its reason, and a planted violation of its own. The scan is over the sources of every crate of the workspace but the
//! driver's (which defines the calls), with comments and strings left out; a file is test code by its `#[cfg(test)]` and not by its name, and is no product code.

use std::fs;
use std::path::{Path, PathBuf};

/// The places that may make one kind of call, **each one function of one file** (a call site, not the file: a second call in a file
/// that is allowed once, or in another function of it, fails the gate): the file, the function, how many such calls it makes
/// there, and the reason. A planted second call in an allowed function fails because the count is exact.
type May = &'static [(&'static str, &'static str, usize, &'static str)];

/// The call sites of `Storage::read`. The reader's own functions come first (each one reads one way, and says which); the others read to
/// **write** (what the acceptance and a curation proposal compare, and never serve) or hand the call on.
const READ_MAY: May = &[
    (
        "inklkunn-core/src/read/text.rs",
        "file_at",
        1,
        "the one reader of a file's text at a block: what `text_at` gives after the check of what moderation hides",
    ),
    (
        "inklkunn-core/src/read/text.rs",
        "bytes_at_tip",
        1,
        "a view's tip, which only `ReadService::tip` makes and whose content cannot be hidden",
    ),
    (
        "inklkunn-core/src/read/text.rs",
        "rules_bytes_at",
        1,
        "a governance file, which is the rules and not a revision's text",
    ),
    (
        "inklkunn-core/src/read/text.rs",
        "file_for_trace",
        1,
        "the size-bound retry of the trace's alignment, which never leaves the comparison (the same reason as the helper's)",
    ),
    (
        "inklkunn-core/src/read/text.rs",
        "bytes_for_readability",
        1,
        "the verdict of whether the wiki can read a knowledge file, which is all that leaves it",
    ),
    (
        "inklkunn-core/src/read/text.rs",
        "bytes_unchecked_at",
        1,
        "the tip check's comparison and the one audited act that shows hidden content (its callers are listed below, one by one)",
    ),
    (
        "inklkunn-core/src/read/counting.rs",
        "read",
        1,
        "the counting storage hands each call on to the storage it counts",
    ),
    (
        "inklkunn-core/src/acceptance/prepare.rs",
        "pages_at",
        1,
        "the write path: the pages at the base, which the merge compares and the signer writes; nothing is served",
    ),
    (
        "inklkunn-core/src/acceptance/prepare.rs",
        "bytes_at",
        1,
        "the write path: a page at the tip, which the merge compares and the signer writes; nothing is served",
    ),
    (
        "inklkunn-core/src/acceptance/prepare.rs",
        "governance_change",
        1,
        "the write path: the governance file at the tip, which the change to it is judged against; nothing is served",
    ),
    (
        "inklkunn-core/src/knowledge/curation.rs",
        "at_tip",
        1,
        "the write path: the standing the tip holds, which a curation proposal is stamped against; nothing is served",
    ),
];

/// The call sites of `Storage::materialize`, the extract of a block's whole tree.
const MATERIALIZE_MAY: May = &[
    (
        "inklkunn-core/src/read.rs",
        "with_tree",
        1,
        "the one extract an index is built from, of a tip, or of a base after the one reader has been asked of each file of it (`read/past.rs`)",
    ),
    (
        "inklkunn-core/src/read/counting.rs",
        "materialize",
        1,
        "the counting storage hands each call on to the storage it counts",
    ),
    (
        "inklkunn-core/src/export.rs",
        "export_bundle_inner",
        1,
        "the archive of a view's tip, whose content cannot be hidden (the tip check keeps it so); what is hidden of authors and summaries is masked by `export/suppression.rs`",
    ),
    (
        "inklkunn-core/src/import.rs",
        "wiki_state",
        1,
        "the tip of the wiki the import has just made, which it judges before it opens the wiki",
    ),
];

/// The call sites of `with_tree`, the service's extract of a block under its build lock.
const WITH_TREE_MAY: May = &[
    (
        "inklkunn-core/src/read.rs",
        "knowledge_of",
        1,
        "the knowledge index, whose base passes the one reader file by file afterwards (`knowledge_without_hidden`)",
    ),
    (
        "inklkunn-core/src/read.rs",
        "build_entries",
        1,
        "the title index, whose base passes the one reader file by file afterwards (`titles_without_hidden`)",
    ),
    (
        "inklkunn-core/src/read.rs",
        "with_block_tree",
        1,
        "the extract of any block, whose callers are listed below",
    ),
    (
        "inklkunn-core/src/read/graph.rs",
        "link_graph",
        1,
        "the link graph of a view's tip, whose content cannot be hidden",
    ),
];

/// The call sites of `with_block_tree`, the extract of any block for a judgement of the write path.
const WITH_BLOCK_TREE_MAY: May = &[(
    "inklkunn-core/src/knowledge/check.rs",
    "check_change_report",
    1,
    "the write path: the judgement of what an acceptance would write, of the tip block it has just read; it reports rules and paths, and serves no text",
)];

/// The call sites of `Storage::diff`, whose hunks are the text of both sides.
const DIFF_MAY: May = &[
    (
        "inklkunn-core/src/read/history.rs",
        "page_diff",
        1,
        "asked only after the one reader has said that neither side's text is hidden",
    ),
    (
        "inklkunn-core/src/read/compare.rs",
        "compare_views",
        1,
        "*Compare views*: the difference of two views' **tips** (`Tip` values only), whose content cannot be hidden",
    ),
    (
        "inklkunn-core/src/read/compare.rs",
        "compare_page",
        1,
        "*Compare views*, one page: the difference of two views' **tips** (`Tip` values only), whose content cannot be hidden",
    ),
    (
        "inklkunn-core/src/read/counting.rs",
        "diff",
        1,
        "the counting storage hands each call on to the storage it counts",
    ),
];

/// The call sites that may write a `Tip {` literal: the module that makes tips.
const TIP_MAY: May = &[
    (
        "inklkunn-core/src/read/tip.rs",
        "tip",
        2,
        "the one place a tip is made, by `ReadService::tip`: the held pair and the pair the log gave",
    ),
    (
        "inklkunn-core/src/read/tip.rs",
        "unpinned",
        1,
        "a tip that names no block (the empty block id), for a page served stale",
    ),
    (
        "inklkunn-core/src/read/tip.rs",
        "for_tests",
        1,
        "the tests' constructor, which exists in test builds only",
    ),
];

/// The callers of `bytes_unchecked_at`, **one by one** (L-4): a file at a block as the storage has it, hidden or not.
const UNCHECKED_MAY: May = &[
    (
        "inklkunn-core/src/read/tipcheck.rs",
        "tip_check",
        2,
        "the comparison of a change's files with the views' tips, which is a boolean; nothing read leaves the function",
    ),
    (
        "inklkunn-core/src/read/tipcheck.rs",
        "surviving_lines",
        2,
        "the count of the lines a change added that a tip's page still has, which is a number; no line leaves the function",
    ),
    (
        "inklkunn-core/src/read/tipcheck.rs",
        "reveal_change",
        1,
        "the one audited act that shows hidden content, called after its audit event is written and for the `moderate` rung",
    ),
    (
        "inklkunn-core/src/read/holders.rs",
        "added_lines",
        2,
        "the lines a change added, its own text at a path and its parent's: the change is the one a hide is judged for, and only a count of lines leaves the function",
    ),
    (
        "inklkunn-core/src/read/text.rs",
        "file_for_trace",
        1,
        "the trace's alignment of one version with the next, which says which change wrote a paragraph and never what it said",
    ),
    (
        "inklkunn-core/src/read/text.rs",
        "merge_text",
        1,
        "the verdict of whether a rollback would merge, which is a boolean",
    ),
];

/// The callers of `file_for_trace`, one by one.
const TRACE_MAY: May = &[(
    "inklkunn-core/src/read/why.rs",
    "past_text",
    1,
    "*Why is this here?* aligns one version of a page with the next and says which change wrote a paragraph, never what it said",
)];

/// The callers of `rules_bytes_at`, one by one.
const RULES_BYTES_MAY: May = &[(
    "inklkunn-core/src/read/rules.rs",
    "read_governance_file",
    1,
    "a governance file at a block, which is the rules and not a revision's text (governance files are never hidden)",
)];

/// The callers of `bytes_for_readability`, one by one.
const READABILITY_MAY: May = &[(
    "inklkunn-core/src/read/preview.rs",
    "knowledge_readability_at",
    1,
    "the verdict of whether the wiki can read a knowledge file at a view's tip, which is all that leaves it",
)];

/// The callers of `bytes_at_tip`, one by one: a view's tip, which only `ReadService::tip` makes.
const TIP_BYTES_MAY: May = &[
    (
        "inklkunn-core/src/read.rs",
        "read_text",
        1,
        "a page's source at a `Tip` value, which only `ReadService::tip` makes and whose content cannot be hidden",
    ),
    (
        "inklkunn-core/src/read/raw.rs",
        "raw",
        1,
        "the stored file of a page at a view's `Tip` value, whose content cannot be hidden",
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

/// The lines (1-based) at which a call named `name` is made with `arity` arguments (`None`: any number): a method call
/// (`.name(`), a call by a path (`<dyn Storage>::name(`, `<S as Storage>::name(`, `S::name(`, which has the receiver for one argument
/// more), or the storage trait's name in a path (`Storage::name(`, whatever the arguments). A definition (`fn name(`), a longer name,
/// a comment and a string are not calls.
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
        // A call by a path whatever the type is called (`<dyn Storage>::read(`, `<S as Storage>::read(`, `S::read(`) is the same call
        // with the receiver as its first argument, so it has one argument more (P-3). A *type's* path says so: `>` closes a qualified
        // one and a type's name begins with a capital, while `cleanup::read(` is a module's function and no trait's call. A definition
        // has `fn` before its name, not `::`.
        let by_path = before.strip_suffix("::").is_some_and(|path| {
            let last: String = path
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            path.ends_with('>') || last.chars().last().is_some_and(char::is_uppercase)
        });
        // A method call: `.name(`, with `fn name(` (a definition) left out.
        if !before.ends_with('.') && !by_path {
            continue;
        }
        let Some(arity) = arity else {
            found.push(line_of(start));
            continue;
        };
        let arity_wanted = |args: usize| {
            if by_path {
                args == arity + 1
            } else {
                args == arity
            }
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
        if end > open && arity_wanted(args) {
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

/// The lines at which a helper that reads past the check is called: a method call or a call by path (`Self::name(`,
/// `ReadService::name(`, any other). A definition, a longer name, a comment and a string are not calls.
fn helper_calls(source: &str, name: &str) -> Vec<usize> {
    calls(source, name, None)
}

/// The name of the function whose body holds `line` (1-based): the last `fn name` of the code at or before it. (A function written
/// inside another one, before a call of the outer one, would name the inner one: the gate then fails and says which, never passes.)
fn enclosing_fn(source: &str, line: usize) -> String {
    let code = code_of(source);
    let mut name = String::new();
    for text in code.lines().take(line) {
        let mut from = 0;
        while let Some(at) = text.get(from..).and_then(|s| s.find("fn ")) {
            let start = from + at;
            from = start + 3;
            let word_before = text
                .get(..start)
                .unwrap_or("")
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if word_before {
                continue;
            }
            let ident: String = text
                .get(from..)
                .unwrap_or("")
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !ident.is_empty() {
                name = ident;
            }
        }
    }
    name
}

/// The calls a way finds in one source, by function: `(function, lines)`, in the order the functions are met.
fn sites_of(way: &Way, source: &str) -> Vec<(String, Vec<usize>)> {
    let mut sites: Vec<(String, Vec<usize>)> = Vec::new();
    for line in (way.find)(source) {
        let function = enclosing_fn(source, line);
        match sites.iter_mut().find(|(name, _)| *name == function) {
            Some((_, lines)) => lines.push(line),
            None => sites.push((function, vec![line])),
        }
    }
    sites
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
        Way {
            name: "`bytes_unchecked_at` (a file at a block, past the check)",
            find: |s| helper_calls(s, "bytes_unchecked_at"),
            may: UNCHECKED_MAY,
        },
        Way {
            name: "`file_for_trace` (a file at a block, for the trace to align)",
            find: |s| helper_calls(s, "file_for_trace"),
            may: TRACE_MAY,
        },
        Way {
            name: "`bytes_at_tip` (a file at a view's tip)",
            find: |s| helper_calls(s, "bytes_at_tip"),
            may: TIP_BYTES_MAY,
        },
        Way {
            name: "`rules_bytes_at` (a governance file)",
            find: |s| helper_calls(s, "rules_bytes_at"),
            may: RULES_BYTES_MAY,
        },
        Way {
            name: "`bytes_for_readability` (a knowledge file, for a verdict)",
            find: |s| helper_calls(s, "bytes_for_readability"),
            may: READABILITY_MAY,
        },
    ]
}

/// Every source of the workspace's crates but the driver's, test code included: `(path relative to `crates/`, source)`.
fn all_sources() -> Vec<(String, String)> {
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
                    stack.push(path);
                } else if file.ends_with(".rs")
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

/// What follows the attribute `#[cfg(test)]` at `at` (the end of it) in `code`, when it is a module: `(name, end of the declaration,
/// is it inline)`. Other attributes between the two and the module's visibility are passed over; anything but a module (a `use`, a
/// function, an `impl`) is `None` and stays product code, since it is in the product file.
fn module_after_cfg_test(code: &str, at: usize) -> Option<(String, usize, bool)> {
    let bytes = code.as_bytes();
    let mut i = at;
    loop {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if !code.get(i..)?.starts_with("#[") {
            break;
        }
        let mut depth = 0_i32;
        while let Some(&c) = bytes.get(i) {
            i += 1;
            match c {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let mut rest = code.get(i..)?;
    if let Some(after) = rest.strip_prefix("pub") {
        let after = after.trim_start();
        rest = match after.strip_prefix('(') {
            Some(inner) => inner.split_once(')')?.1,
            None => after,
        };
    }
    let rest = rest.trim_start().strip_prefix("mod ")?.trim_start();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let mut j = code.len() - rest.len() + name.len();
    while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
        j += 1;
    }
    match bytes.get(j) {
        Some(b';') => Some((name, j + 1, false)),
        Some(b'{') => {
            let mut depth = 0_i32;
            let mut end = bytes.len();
            for (n, &c) in bytes.iter().enumerate().skip(j) {
                match c {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = n + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            Some((name, end, true))
        }
        _ => None,
    }
}

/// The directory a module declared in the file `path` keeps its own modules in: `a/b/c.rs` declares `a/b/c/x.rs`; `a/b/mod.rs`,
/// `lib.rs` and `main.rs` declare `a/b/x.rs`.
fn module_dir_of(path: &str) -> String {
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    match file {
        "mod.rs" | "lib.rs" | "main.rs" => dir.to_owned(),
        _ => format!("{dir}/{}", file.trim_end_matches(".rs")),
    }
}

/// The product code of a workspace's sources (P-3): **a file is test code when the source says so, never by its name**. A file is
/// test code when a file that declares it does so under `#[cfg(test)]` (`#[cfg(test)] mod tests;`, under whatever name), when it is
/// under the directory of such a module, or when it says `#![cfg(test)]` at its head; a `#[cfg(test)] mod name { … }` body is blanked
/// where it stands. What is left is returned as code (comments and strings left out, the lines as they were). A production file that
/// is *named* `tests.rs` or `*_tests.rs` and is not under `#[cfg(test)]` is scanned like any other.
fn product_of(all: &[(String, String)]) -> Vec<(String, String)> {
    let mut test_files: Vec<String> = Vec::new();
    let mut test_dirs: Vec<String> = Vec::new();
    let mut product: Vec<(String, String)> = Vec::new();
    for (path, source) in all {
        let mut code = code_of(source);
        let mut blank: Vec<(usize, usize)> = Vec::new();
        for (at, found) in code.match_indices("#[cfg(test)]") {
            let Some((name, end, inline)) = module_after_cfg_test(&code, at + found.len()) else {
                continue;
            };
            if inline {
                blank.push((at, end));
            } else {
                let dir = module_dir_of(path);
                test_files.push(format!("{dir}/{name}.rs"));
                test_files.push(format!("{dir}/{name}/mod.rs"));
                test_dirs.push(format!("{dir}/{name}/"));
            }
        }
        for (from, to) in blank {
            let blanked: String = code
                .get(from..to)
                .unwrap_or_default()
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' })
                .collect();
            code.replace_range(from..to, &blanked);
        }
        if code.trim_start().starts_with("#![cfg(test)]") {
            test_files.push(path.clone());
            test_dirs.push(format!("{}/", module_dir_of(path)));
            continue;
        }
        product.push((path.clone(), code));
    }
    product.retain(|(path, _)| {
        !test_files.contains(path) && !test_dirs.iter().any(|dir| path.starts_with(dir.as_str()))
    });
    product
}

/// Every product source of the workspace's crates but the driver's: `(path relative to `crates/`, code)`.
fn product_sources() -> Vec<(String, String)> {
    product_of(&all_sources())
}

/// The calls of a way that no site allows, **per call site** (L-4): a call in a function that is not listed for the file, and the
/// calls of a listed function past the number it is listed for. Each is `path:line (function)`.
fn violations_of(way: &Way, sources: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, source) in sources {
        for (function, lines) in sites_of(way, source) {
            let allowed = way
                .may
                .iter()
                .find(|(place, name, _, _)| path == place && *name == function)
                .map_or(0, |(_, _, calls, _)| *calls);
            out.extend(
                lines
                    .into_iter()
                    .skip(allowed)
                    .map(|line| format!("{path}:{line} ({function})")),
            );
        }
    }
    out
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
    // A list of places that may names call sites that exist, with the number of calls each makes: a stale entry (a function that no
    // longer makes the call, or makes fewer) would let a later call in unseen.
    let sources = product_sources();
    for way in ways() {
        for (allowed, function, count, why) in way.may {
            let source = sources
                .iter()
                .find(|(path, _)| path == allowed)
                .map(|(_, source)| source.as_str());
            let Some(source) = source else {
                panic!("`{allowed}` is not a source of the workspace ({why})");
            };
            let made = sites_of(&way, source)
                .into_iter()
                .find(|(name, _)| name == function)
                .map_or(0, |(_, lines)| lines.len());
            assert_eq!(
                made, *count,
                "`{function}` in `{allowed}` makes {made} call(s) by {} and the list says {count}: correct the list ({why})",
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
    // The same call in the reader's own function is the reader.
    let reader = at(
        "inklkunn-core/src/read/text.rs",
        "fn file_at() { let b = storage.read(repo, &point, path, max); }",
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
            "fn export_bundle_inner() { self.storage.materialize(&repo, &point, &scratch, name, bounds); }",
            "inklkunn-core/src/export.rs",
        ),
        (
            "the storage's `materialize`",
            "fn wiki_state() { Storage::materialize(&s, &repo, &point, &scratch, name, bounds); }",
            "inklkunn-core/src/import.rs",
        ),
        (
            "`with_tree`",
            "fn link_graph() { self.with_tree(wiki, &at, |root| Ok(1)); }",
            "inklkunn-core/src/read/graph.rs",
        ),
        (
            "`with_block_tree`",
            "fn check_change_report() { reads.with_block_tree(wiki, &at, |root, c| 1); }",
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
fn a_planted_call_by_a_path_is_found_whatever_the_type_is_called() {
    // P-3: the trait's call written as a path (the receiver is its first argument) is the same call.
    let elsewhere = "inklkunn-core/src/elsewhere.rs";
    for (begins, call, allowed, function) in [
        (
            "the storage's `read`",
            "fn f() { <dyn Storage>::read(&*s, repo, &point, path, max); }",
            "inklkunn-core/src/read/text.rs",
            "file_at",
        ),
        (
            "the storage's `read`",
            "fn f() { <S as Storage>::read(&s, repo, &point, path, max); }",
            "inklkunn-core/src/read/text.rs",
            "file_at",
        ),
        (
            "the storage's `read`",
            "fn f<S: Storage>() { S::read(&s, repo, &point, path, max); }",
            "inklkunn-core/src/read/text.rs",
            "file_at",
        ),
        (
            "the storage's `read`",
            "fn f() { Store::read(\n    &s,\n    repo,\n    &point,\n    path,\n    max,\n); }",
            "inklkunn-core/src/read/text.rs",
            "file_at",
        ),
        (
            "the storage's `diff`",
            "fn f() { <dyn Storage>::diff(&*s, &repo, &from, &to, &[path]); }",
            "inklkunn-core/src/read/history.rs",
            "page_diff",
        ),
        (
            "the storage's `diff`",
            "fn f<S: Storage>() { S::diff(&s, &repo, &from, &to, &[]); }",
            "inklkunn-core/src/read/history.rs",
            "page_diff",
        ),
        (
            "the storage's `materialize`",
            "fn f() { <S as Storage>::materialize(&s, &repo, &point, &scratch, name, bounds); }",
            "inklkunn-core/src/export.rs",
            "export_bundle_inner",
        ),
        (
            "the storage's `materialize`",
            "fn f() { S::materialize(&s, &repo, &point, &scratch, name, bounds); }",
            "inklkunn-core/src/import.rs",
            "wiki_state",
        ),
        (
            "`with_tree`",
            "fn f() { ReadService::with_tree(&service, wiki, &at, |root| Ok(1)); }",
            "inklkunn-core/src/read/graph.rs",
            "link_graph",
        ),
        (
            "`with_block_tree`",
            "fn f() { Self::with_block_tree(&reads, wiki, &at, |root, c| 1); }",
            "inklkunn-core/src/knowledge/check.rs",
            "check_change_report",
        ),
    ] {
        let way = way(begins);
        assert_eq!(violations_of(&way, &at(elsewhere, call)).len(), 1, "{call}");
        // The same call in the function that may is the listed one.
        let listed = call.replacen("fn f", &format!("fn {function}"), 1);
        assert!(
            violations_of(&way, &at(allowed, &listed)).is_empty(),
            "{listed} in {allowed}"
        );
    }
    // What is no call of the storage's: a module's function, a type's own function with the arguments of the storage's, another name.
    for (begins, fine) in [
        (
            "the storage's `read`",
            "fn f() { cleanup::read(&*p.storage, &repo, &status, &doctor, &acts); }",
        ),
        (
            "the storage's `read`",
            "fn f() { Rules::read(block, policy, constitution, limits); }",
        ),
        (
            "the storage's `read`",
            "fn f() { thread::read(services, headers, &wiki, &claim, params); }",
        ),
        ("the storage's `read`", "fn f() { fs::read(path); }"),
        (
            "the storage's `read`",
            "fn f() { Reader::reads(&s, repo, &point, path, max); }",
        ),
        (
            "the storage's `diff`",
            "fn f() { Words::diff(a, b, c, d); }",
        ),
        ("`with_tree`", "fn f() { module::with_tree(wiki, &at, f); }"),
    ] {
        assert!(
            violations_of(&way(begins), &at("inklkunn-core/src/elsewhere.rs", fine)).is_empty(),
            "{fine}"
        );
    }
    // A call by a path is counted like any other in a place that may: the second one fails.
    let text = "inklkunn-core/src/read/text.rs";
    let two = "fn file_at() { storage.read(repo, &point, path, max);\n    <S as Storage>::read(&s, repo, &point, path, max); }";
    assert_eq!(
        violations_of(&way("the storage's `read`"), &at(text, two)),
        vec![format!("{text}:2 (file_at)")]
    );
}

fn file(path: &str, source: &str) -> (String, String) {
    (path.to_owned(), source.to_owned())
}

/// The paths of the product code of some sources.
fn product_paths(all: &[(String, String)]) -> Vec<String> {
    product_of(all).into_iter().map(|(path, _)| path).collect()
}

#[test]
fn a_file_is_test_code_by_its_cfg_test_and_never_by_its_name() {
    // P-3: `tests.rs`, `testing.rs`, `support.rs` and `*_tests.rs` were skipped by their names. A **production** file with one of them
    // is scanned, and a test file under any name is not.
    let call = "fn f() { storage.read(repo, &point, path, max); }";
    let production = |path: &str| {
        let sources = vec![
            file("inklkunn-core/src/read.rs", "mod helpers;"),
            file(path, call),
        ];
        violations(&product_of(&sources))
    };
    for name in [
        "inklkunn-core/src/read/tests.rs",
        "inklkunn-core/src/read/testing.rs",
        "inklkunn-core/src/read/support.rs",
        "inklkunn-core/src/read/page_tests.rs",
        "inklkunn-core/src/read/tests/deep.rs",
    ] {
        assert_eq!(
            production(name),
            vec![format!("{name}:1 (f)")],
            "{name} is production code: nothing declares it under `#[cfg(test)]`"
        );
    }
    // A test file by its declaration, whatever it is called, and every module under it.
    let sources = vec![
        file(
            "inklkunn-core/src/read.rs",
            "mod page;\n#[cfg(test)]\nmod checks;\n#[cfg(test)]\n#[allow(dead_code)]\npub(crate) mod fixtures;\n",
        ),
        file("inklkunn-core/src/read/page.rs", "fn page() {}"),
        file(
            "inklkunn-core/src/read/checks.rs",
            &format!("mod deeper;\n{call}"),
        ),
        file("inklkunn-core/src/read/checks/deeper.rs", call),
        file("inklkunn-core/src/read/fixtures/mod.rs", call),
        file("inklkunn-core/src/read/fixtures/more.rs", call),
    ];
    assert_eq!(
        product_paths(&sources),
        [
            "inklkunn-core/src/read.rs",
            "inklkunn-core/src/read/page.rs"
        ]
    );
    // The same for a file that declares its tests beside itself, and for the crate's root.
    let sources = vec![
        file(
            "inklkunn-core/src/read/text.rs",
            "#[cfg(test)]\nmod tests;\nfn file_at() {}",
        ),
        file("inklkunn-core/src/read/text/tests.rs", call),
        file("inklkunn-core/src/lib.rs", "#[cfg(test)]\nmod suite;"),
        file("inklkunn-core/src/suite.rs", call),
    ];
    assert_eq!(
        product_paths(&sources),
        ["inklkunn-core/src/read/text.rs", "inklkunn-core/src/lib.rs"]
    );
    // A file that says `#![cfg(test)]` at its head is test code, and so are its modules.
    let sources = vec![
        file(
            "inklkunn-core/src/odd.rs",
            &format!("#![cfg(test)]\n{call}"),
        ),
        file("inklkunn-core/src/odd/inner.rs", call),
    ];
    assert!(product_paths(&sources).is_empty());
    // A `#[cfg(test)] mod name { … }` body is test code where it stands; what follows it is not.
    let inline = format!("#[cfg(test)]\nmod scratch {{\n    {call}\n}}\n{call}\n");
    assert_eq!(
        violations(&product_of(&at("inklkunn-core/src/elsewhere.rs", &inline))),
        vec!["inklkunn-core/src/elsewhere.rs:5 (f)".to_owned()]
    );
    // Only a module is a test module: a function under `#[cfg(test)]` is product code that tests use, and is scanned.
    let function = format!("#[cfg(test)]\n{call}\n");
    assert_eq!(
        violations(&product_of(&at(
            "inklkunn-core/src/elsewhere.rs",
            &function
        )))
        .len(),
        1
    );
    // `#[cfg(any(test, feature = "x"))]` is not test code: the product build has it with the feature.
    let either = "#[cfg(any(test, feature = \"x\"))]\nmod helpers;";
    let sources = vec![
        file("inklkunn-core/src/lib.rs", either),
        file("inklkunn-core/src/helpers.rs", call),
    ];
    assert_eq!(violations(&product_of(&sources)).len(), 1);
}

#[test]
fn a_planted_diff_is_found() {
    let way = way("the storage's `diff`");
    for call in [
        "fn page_diff() { self.storage.diff(&repo, &from, &to, &[path]); }",
        "fn page_diff() { Storage::diff(&s, &repo, &from, &to, &[]); }",
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
        "fn tip() -> R { let tip = Tip { view: v, state_id: s, block_id: b }; tip }",
        "fn tip() -> R { Tip {\n        view: v,\n    } }",
        "fn tip(t: &Tip) { let Tip { block_id, .. } = t; }",
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
fn a_second_call_in_a_place_that_may_and_a_call_in_another_function_of_it_fail_the_gate() {
    // L-4: the lists are of call sites, not of files. `page_diff` of `history.rs` may call `diff` once.
    let diff = way("the storage's `diff`");
    let history = "inklkunn-core/src/read/history.rs";
    let one = "fn page_diff() { self.storage.diff(&repo, &from, &to, &[path]); }";
    assert!(violations_of(&diff, &at(history, one)).is_empty());
    // A second call in the same function.
    let two = "fn page_diff() { self.storage.diff(&repo, &from, &to, &[path]);\n    self.storage.diff(&repo, &a, &b, &[path]); }";
    assert_eq!(
        violations_of(&diff, &at(history, two)),
        vec![format!("{history}:2 (page_diff)")]
    );
    // A call in another function of the same file.
    let other = format!("{one}\nfn sneaky() {{ self.storage.diff(&repo, &from, &to, &[path]); }}");
    assert_eq!(
        violations_of(&diff, &at(history, &other)),
        vec![format!("{history}:2 (sneaky)")]
    );
    // The same for the helpers that read past the check: `added_lines` of `holders.rs` is listed for two calls.
    let unchecked = way("`bytes_unchecked_at`");
    let holders = "inklkunn-core/src/read/holders.rs";
    let listed =
        "fn added_lines() { self.bytes_unchecked_at(a, b, c); Self::bytes_unchecked_at(x, y, z); }";
    assert!(violations_of(&unchecked, &at(holders, listed)).is_empty());
    let third = "fn added_lines() { self.bytes_unchecked_at(a, b, c); Self::bytes_unchecked_at(x, y, z);\n    self\n        .bytes_unchecked_at(p, q, r); }";
    assert_eq!(
        violations_of(&unchecked, &at(holders, third)),
        vec![format!("{holders}:3 (added_lines)")]
    );
    let elsewhere = "fn later_holders() { self.bytes_unchecked_at(a, b, c); }";
    assert_eq!(violations_of(&unchecked, &at(holders, elsewhere)).len(), 1);
    assert_eq!(
        violations_of(&unchecked, &at("inklkunn-core/src/read/past.rs", listed)).len(),
        2
    );
    // Not a call: the definition, a comment, a string, a longer name.
    for fine in [
        "fn bytes_unchecked_at(&self) -> R { todo() }",
        "// self.bytes_unchecked_at(a, b, c)\nfn f() {}",
        "fn f() { let s = \"self.bytes_unchecked_at(a, b, c)\"; }",
        "fn f() { self.bytes_unchecked_at_tip(a, b, c); }",
    ] {
        assert!(
            violations_of(&unchecked, &at("inklkunn-core/src/elsewhere.rs", fine)).is_empty(),
            "{fine}"
        );
    }
    // Each of the other helpers has its planted call.
    for (begins, call) in [
        (
            "`file_for_trace`",
            "fn f() { self.file_for_trace(w, b, p); }",
        ),
        (
            "`rules_bytes_at`",
            "fn f() { self.rules_bytes_at(w, p, f, m); }",
        ),
        (
            "`bytes_for_readability`",
            "fn f() { self.bytes_for_readability(r, p, f, b); }",
        ),
        ("`bytes_at_tip`", "fn f() { self.bytes_at_tip(w, t, p); }"),
    ] {
        assert_eq!(
            violations_of(&way(begins), &at("inklkunn-core/src/elsewhere.rs", call)).len(),
            1,
            "{call}"
        );
    }
}

#[test]
fn the_rules_are_read_at_a_governance_point_and_through_no_public_function() {
    // L-4: `rules_at` read any block it was given, unchecked, and was public. It is not public, and it takes the governance reader's
    // own point, which says what it is (no revision's text) and which a surface cannot name.
    let sources = product_sources();
    let Some(rules) = sources
        .iter()
        .find(|(path, _)| path == "inklkunn-core/src/read/rules.rs")
        .map(|(_, source)| code_of(source))
    else {
        panic!("the rules' module is not a source of the workspace");
    };
    let Some(signature) = rules
        .find("fn rules_at")
        .and_then(|at| rules.get(..at).zip(rules.get(at..)))
    else {
        panic!("`rules_at` is not a function of the rules' module");
    };
    assert!(
        signature.0.trim_end().ends_with("pub(crate)"),
        "`rules_at` is `pub(crate)` and nothing wider"
    );
    let parameters: String = signature.1.chars().take_while(|c| *c != '{').collect();
    assert!(
        parameters.contains("&GovernancePoint") && !parameters.contains("BlockPoint"),
        "`rules_at` takes a governance point: {parameters}"
    );
    // No other module of the workspace but the crate's own may name the function.
    for (path, source) in sources
        .iter()
        .filter(|(path, _)| !path.starts_with("inklkunn-core/"))
    {
        assert!(
            !code_of(source).contains("rules_at("),
            "{path} calls `rules_at`, which the core keeps to itself"
        );
    }
}

#[test]
fn every_call_site_that_may_has_a_reason_and_no_list_names_a_site_twice() {
    for way in ways() {
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for (place, function, count, why) in way.may {
            assert!(
                why.split_whitespace().count() >= 4,
                "`{function}` in `{place}` has no reason for {}",
                way.name
            );
            assert!(
                *count >= 1,
                "`{function}` in `{place}` is listed for no call by {}",
                way.name
            );
            assert!(
                !seen.contains(&(place, function)),
                "`{function}` in `{place}` is listed twice for {}",
                way.name
            );
            seen.push((place, function));
        }
    }
}
