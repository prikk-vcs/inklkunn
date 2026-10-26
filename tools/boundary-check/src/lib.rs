#![forbid(unsafe_code)]
//! `inklkunn-boundary-check` — the gates of RFC 001 D-2, as std-only functions over the workspace's
//! own files, so that each rule can be exercised on a planted violation in a unit test and applied to
//! the real tree in `tests/gates.rs`.
//!
//! Rules:
//! - **layering** (`LAY-1`): dependencies point downward only. Upper → core → lower.
//! - **key reachability** (`LAY-4`, `INV-2`): only `inklkunn-core` may depend on `inklkunn-signer`, and
//!   the signer's key-store module is private to it.
//! - **prikk spawn** (`LAY-3`): only `inklkunn-prikk` and `inklkunn-signer` spawn a `prikk` process, and
//!   `inklkunn-prikk` names no write verb.
//! - **markup purity** (`LAY-6`): `inklkunn-markup` has no I/O, clock, process or randomness dependency.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Lower crates: depend on nothing in the workspace except, for the signer, the driver.
pub const LOWER: &[&str] = &["inklkunn-prikk", "inklkunn-signer", "inklkunn-store"];
/// Core crates: may depend on lower crates and on each other, never on upper ones.
pub const CORE: &[&str] = &["inklkunn-markup", "inklkunn-core", "inklkunn-auth"];
/// Upper crates: may depend downward; nothing depends on them.
pub const UPPER: &[&str] = &["inklkunn-web", "inklkunn"];

/// The only crate that may depend on the signer (RFC 001 D-2 b).
pub const SIGNER_CONSUMER: &str = "inklkunn-core";
/// The signer's private key-store module name, which no other crate may mention.
pub const KEY_STORE_MODULE: &str = "key_store";

/// prikk verbs that write; `inklkunn-prikk` (the read driver) may not name any of them as a string.
pub const PRIKK_WRITE_VERBS: &[&str] = &[
    "init",
    "setup",
    "commit",
    "seal",
    "mv",
    "create",
    "close",
    "switch",
    "trust",
    "key",
    "rollback-draft",
    "merge",
    "sync",
    "bundle",
    "archive",
    "doctor",
    "unlock",
    "compact",
    "format",
    "received",
    "config",
    "ref",
];

/// What the pure markup crate may not reach for in its sources.
pub const IMPURE_PATHS: &[&str] = &[
    "std::fs",
    "std::net",
    "std::process",
    "std::time::SystemTime",
    "std::time::Instant",
    "std::env",
    "std::io::stdin",
    "rand",
    "tokio",
];

/// The workspace root: two levels above this crate's manifest.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Does a `Cargo.toml` text declare a dependency on crate `dep`? A dependency line is `dep = …`,
/// `dep.workspace = true` or `dep = { … }`, in any dependency table; the trailing check avoids
/// matching a longer name.
pub fn declares_dep_on(manifest: &str, dep: &str) -> bool {
    manifest.lines().any(|line| {
        let t = line.trim_start();
        if t.starts_with('#') {
            return false;
        }
        t.starts_with(dep)
            && matches!(
                t.get(dep.len()..).and_then(|r| r.chars().next()),
                Some(' ' | '=' | '.')
            )
    })
}

/// Read every member crate's manifest: name → manifest text.
pub fn read_manifests(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for name in LOWER.iter().chain(CORE).chain(UPPER) {
        let path = root.join("crates").join(name).join("Cargo.toml");
        if let Ok(text) = fs::read_to_string(&path) {
            out.insert((*name).to_owned(), text);
        }
    }
    out
}

/// The layering rule over a set of manifests. Returns the violations as `(crate, depends on)`.
pub fn layering_violations(manifests: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut v = Vec::new();
    for (name, text) in manifests {
        let is_lower = LOWER.contains(&name.as_str());
        let is_core = CORE.contains(&name.as_str());
        if is_lower || is_core {
            for upper in UPPER {
                if declares_dep_on(text, upper) {
                    v.push((name.clone(), (*upper).to_owned()));
                }
            }
        }
        if is_lower {
            for core in CORE {
                if declares_dep_on(text, core) {
                    v.push((name.clone(), (*core).to_owned()));
                }
            }
        }
    }
    v
}

/// The key-reachability rule, part (a): every crate other than [`SIGNER_CONSUMER`] that depends on
/// `inklkunn-signer`.
pub fn signer_dependents(manifests: &BTreeMap<String, String>) -> Vec<String> {
    manifests
        .iter()
        .filter(|(name, text)| {
            name.as_str() != SIGNER_CONSUMER
                && name.as_str() != "inklkunn-signer"
                && declares_dep_on(text, "inklkunn-signer")
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The key-reachability rule, part (b): the signer's `lib.rs` must declare the key-store module private
/// (`mod key_store;`, never `pub mod key_store`), and no other crate's source may mention it.
pub fn key_store_exposed(signer_lib: &str) -> bool {
    signer_lib.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("pub mod ") && t.contains(KEY_STORE_MODULE)
    })
}

/// Every `.rs` file under `dir` (recursively), excluding `target/`.
pub fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Lines (1-based) of `source` that mention the key-store module.
pub fn key_store_mentions(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains(KEY_STORE_MODULE))
        .map(|(i, _)| i + 1)
        .collect()
}

/// Lines of `source` that spawn a process naming prikk: a `Command::new(` whose argument text contains
/// `prikk`, or a `Command::new(` at all when `any_spawn` is set (crates that may not spawn anything).
pub fn prikk_spawn_lines(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("Command::new(") && l.to_ascii_lowercase().contains("prikk"))
        .map(|(i, _)| i + 1)
        .collect()
}

/// String literals in `source` that are prikk write verbs (the read driver may not name them).
pub fn write_verb_literals(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        for verb in PRIKK_WRITE_VERBS {
            let lit = format!("\"{verb}\"");
            if line.contains(&lit) {
                out.push((i + 1, (*verb).to_owned()));
            }
        }
    }
    out
}

/// Lines of `source` that reach for an impure path (the markup crate may not).
pub fn impure_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        for p in IMPURE_PATHS {
            if line.contains(p) {
                out.push((i + 1, (*p).to_owned()));
            }
        }
    }
    out
}

/// Third-party dependencies named by a manifest's `[dependencies]` table (not dev-dependencies).
pub fn runtime_dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_table = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_table = t == "[dependencies]";
            continue;
        }
        if in_table
            && !t.is_empty()
            && !t.starts_with('#')
            && let Some(name) = t.split(['=', '.', ' ']).next()
        {
            deps.push(name.to_owned());
        }
    }
    deps
}

/// Dependencies the pure markup crate may carry.
pub const MARKUP_ALLOWED_DEPS: &[&str] = &[
    "pulldown-cmark",
    "serde",
    "serde_json",
    "unicode-normalization",
];
