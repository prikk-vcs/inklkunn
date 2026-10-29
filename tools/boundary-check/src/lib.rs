#![forbid(unsafe_code)]
//! `inklkunn-boundary-check` — the gates of RFC 001 D-2, as std-only functions over the workspace's
//! own files, so that each rule can be exercised on a planted violation in a unit test and applied to
//! the real tree in `tests/gates.rs`.
//!
//! Rules:
//! - **layering** (`LAY-1`): dependencies point downward only. Upper → core → lower. Every member crate
//!   under `crates/` is in exactly one layer; a crate the lists do not know fails the gate.
//! - **key reachability** (`LAY-4`, `INV-2`): only `inklkunn-core` may depend on `inklkunn-signer`; the
//!   signer's key-store module is private and never re-exported; the bootstrap signer and the writable
//!   sandbox mounts are named by no product source outside the signer (and the sandbox's own crate).
//! - **prikk spawn** (`LAY-3`): only `inklkunn-prikk`, `inklkunn-signer` and `inklkunn-fetch` spawn a process
//!   at all (no `std::process` or `tokio::process` elsewhere), `inklkunn-prikk` names no write verb, and
//!   `inklkunn-fetch` spawns only `bwrap` (which runs `curl`) or, in tests, `curl` itself.
//! - **curl** (`LAY-3`, IQ-3, handoff 002 corrections C8): nothing but `inklkunn-fetch` names `curl`; only the
//!   binary may depend on `inklkunn-fetch`; and `inklkunn-fetch` depends on the driver alone (no TLS or HTTP
//!   crate can enter through it).
//! - **markup purity** (`LAY-6`): `inklkunn-markup` has no I/O, clock, process or randomness dependency.
//! - **no public seed** (`INV-2`, RFC 002 D-3): no public item of the signer crate mentions a seed, a
//!   secret, a key directory or the key store's types, so nothing it exports can return key material or the
//!   path of any.
//! - **route registration** (`LAY-2`, `ENF-3`): the web crate registers routes in its `routes.rs` and
//!   nowhere else, so the route table (`inklkunn_core::RouteTable`) is complete by construction.
//! - **privileged principals** (`INV-4`, RFC 002 D-1): no web product source names a system principal or
//!   builds an account principal; a handler receives the principal the auth crate produced.
//! - **unsafe is forbidden** everywhere: every member crate carries `[lints] workspace = true` and opens
//!   with `#![forbid(unsafe_code)]`.
//! - **features**: no product crate enables another crate's test or bootstrap feature.
//!
//! Manifests are read with a small TOML reader that understands dependency tables in every spelling cargo
//! accepts (`[dependencies]`, `[dependencies.name]`, `[target.'cfg'.dependencies]`, quoted keys,
//! `package = "…"` renames), and sources are scanned with comments and string literals stripped where a
//! rule is about code, not prose.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Lower crates: depend on nothing in the workspace above them. The signer depends on the driver (the sandbox,
/// the typed reports) and on the store (the validated identifiers it may put in a path or a signed message);
/// the fetcher depends on the driver's bounded runner alone.
pub const LOWER: &[&str] = &[
    "inklkunn-prikk",
    "inklkunn-signer",
    "inklkunn-store",
    "inklkunn-fetch",
];
/// Core crates: may depend on lower crates and on each other, never on upper ones.
pub const CORE: &[&str] = &["inklkunn-markup", "inklkunn-core", "inklkunn-auth"];
/// Upper crates: may depend downward; nothing depends on them.
pub const UPPER: &[&str] = &["inklkunn-web", "inklkunn"];

/// The only crate that may depend on the signer (RFC 001 D-2 b).
pub const SIGNER_CONSUMER: &str = "inklkunn-core";
/// The crate that runs `curl` (IQ-3): the third crate allowed to spawn a process, and the only one that may
/// name `curl`.
pub const FETCH_CRATE: &str = "inklkunn-fetch";
/// The only crate that may depend on [`FETCH_CRATE`]: the binary, whose wiring joins the egress guard, the
/// fetcher and the authentication seam.
pub const FETCH_CONSUMER: &str = "inklkunn";
/// What [`FETCH_CRATE`] may depend on in a non-dev table: the driver's bounded runner and nothing else, so no
/// TLS or HTTP crate (and no core crate) can enter through it.
pub const FETCH_ALLOWED_DEPS: &[&str] = &["inklkunn-prikk"];
/// The arguments [`FETCH_CRATE`] may give `Command::new`: the sandbox (`bwrap`, which runs `curl`) or `curl`
/// itself (the tests-only unconfined mode), as a name or a literal.
pub const FETCH_SPAWN_ALLOWED_ARGS: &[&str] = &[
    "bwrap",
    "self.bwrap",
    "BWRAP",
    "curl",
    "self.curl",
    "CURL",
    "\"bwrap\"",
    "\"curl\"",
];
/// The signer's private key-store module name, which no other crate may mention.
pub const KEY_STORE_MODULE: &str = "key_store";
/// Names only the signer's product sources may use: the writable mounts and the bootstrap signer.
pub const SIGNER_ONLY_NAMES: &[&str] = &[
    "Mounts::write",
    "Mounts::dir_writable",
    "BootstrapSigner",
    "run_mounts",
];
/// Cargo features no product crate may enable in a non-dev dependency.
pub const TEST_ONLY_FEATURES: &[&str] = &["test-knobs", "bootstrap", "test-authority"];

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

/// What the pure markup crate may not reach for in its sources (matched on normalized `use` paths and on
/// inline paths).
pub const IMPURE_PATHS: &[&str] = &[
    "std::fs",
    "std::net",
    "std::process",
    "std::time",
    "std::env",
    "std::io::stdin",
    "std::io::Read",
    "std::io::Write",
    "std::thread",
    "rand",
    "tokio",
];

/// What means a process is spawned: the `Command` type of `std::process` or `tokio::process`, in any
/// spelling (`std::process::id()` is not a spawn and stays allowed).
pub const SPAWN_PATHS: &[&str] = &["process::Command", "Command::new(", "tokio::process"];

/// Dependencies the pure markup crate may carry.
pub const MARKUP_ALLOWED_DEPS: &[&str] = &[
    "pulldown-cmark",
    "serde",
    "serde_json",
    "unicode-normalization",
];

/// What registers a route with axum (or merges a router in), in any spelling: the table
/// (`inklkunn_core::RouteTable`) is complete only if none of these appears outside [`ROUTE_REGISTRY_FILE`].
pub const ROUTE_REGISTRATION_TOKENS: &[&str] = &[
    ".route(",
    ".route_service(",
    ".nest(",
    ".nest_service(",
    ".merge(",
    ".fallback(",
    ".fallback_service(",
    "Router::new",
    // The same calls in function-call spelling (`Router::route(app, …)`).
    "Router::route",
    "Router::route_service",
    "Router::nest",
    "Router::nest_service",
    "Router::merge",
    "Router::fallback",
    "Router::fallback_service",
];
/// The one web source file that may register routes.
pub const ROUTE_REGISTRY_FILE: &str = "routes.rs";
/// Names a web handler must never use: it receives a principal, it does not make one.
pub const PRIVILEGED_PRINCIPAL_NAMES: &[&str] = &[
    "Principal::System",
    "SystemPrincipal",
    "IdentityAuthority",
    "AccountPrincipal",
    "Principal::Account",
    "Principal::session",
    "Principal::token",
    // The ways to name a variant or a constructor without writing `Principal::Account` in full: a rename on
    // import, a glob or a group import, and the qualified-type spelling.
    "Principal as",
    "Principal::*",
    "Principal::{",
    "<Principal>",
];

/// Names of the signer's crate that the web and auth crates must not use: the signer is reached through
/// `inklkunn-core`'s services (RFC 004), never opened or driven from a surface. (`Accept` is not listed: it
/// is also the name of an authorization action.)
pub const SIGNER_NAMES_FORBIDDEN_IN_SURFACES: &[&str] = &[
    "Signer",
    "SignerConfig",
    "OldKey",
    "OldKeyOutcome",
    "Rotation",
    "RepositoryBootstrap",
    "KeyAuthority",
    "AcceptanceLedger",
    "inklkunn_signer",
];

/// Words and types that must not appear in a public item of the signer crate (matched on the item's line,
/// case-insensitively for `seed`, exactly for the type names). `pub(crate)` and `pub(super)` items are not
/// public and are not examined.
pub const SIGNER_PUBLIC_FORBIDDEN_TYPES: &[&str] = &[
    "KeyDir",
    "SeedDir",
    "SeedBase",
    "Seeds",
    "Secret",
    "Zeroizing",
    "MasterKey",
    "KeyStore",
];

/// The workspace root: two levels above this crate's manifest.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

// ---- manifests ----

/// One dependency edge as a manifest declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    /// The crate depended on (the `package =` name when renamed).
    pub package: String,
    /// `dependencies`, `dev-dependencies` or `build-dependencies`.
    pub kind: String,
    /// Features enabled on it.
    pub features: Vec<String>,
}

fn unquote(s: &str) -> &str {
    let t = s.trim();
    t.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| t.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .unwrap_or(t)
}

fn features_in(value: &str) -> Vec<String> {
    let Some(start) = value.find("features") else {
        return Vec::new();
    };
    let rest = &value[start..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find(']') else {
        return Vec::new();
    };
    rest[open + 1..open + close]
        .split(',')
        .map(unquote)
        .filter(|f| !f.is_empty())
        .map(str::to_owned)
        .collect()
}

fn package_in(value: &str) -> Option<String> {
    let start = value.find("package")?;
    let rest = &value[start + "package".len()..];
    let rest = rest.trim_start().strip_prefix('=')?;
    let rest = rest.trim_start();
    let q = rest.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let end = rest[1..].find(q)?;
    Some(rest[1..1 + end].to_owned())
}

/// Every dependency a manifest declares, in every table spelling.
pub fn dependencies(manifest: &str) -> Vec<Dep> {
    let mut out = Vec::new();
    // The current table: Some(kind, Some(name)) for `[dependencies.name]`, Some(kind, None) for a table of
    // deps, None otherwise.
    let mut table: Option<(String, Option<String>)> = None;
    let mut inline_value = String::new();
    let mut pending: Option<(String, String)> = None; // (kind, key) of a multi-line value
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some((kind, key)) = pending.take() {
            inline_value.push_str(line);
            if line.contains('}') || line.contains(']') && !line.contains('[') {
                out.push(dep_of(&kind, &key, &inline_value));
                inline_value.clear();
            } else {
                pending = Some((kind, key));
            }
            continue;
        }
        if line.starts_with('[') {
            let header = line.trim_matches(['[', ']']).trim();
            table = table_kind(header);
            if let Some((kind, Some(name))) = &table {
                // `[dependencies.name]`: the body lines follow; record the edge now, features later.
                out.push(Dep {
                    package: name.clone(),
                    kind: kind.clone(),
                    features: Vec::new(),
                });
            }
            continue;
        }
        let Some((kind, name)) = &table else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = unquote(key.trim());
        let value = value.trim();
        match name {
            Some(n) => {
                // Body of `[dependencies.name]`: `features = […]` or `package = "…"`.
                if let Some(last) = out
                    .last_mut()
                    .filter(|d| d.package == *n || d.kind == *kind)
                {
                    if key == "features" {
                        last.features = features_in(&format!("features = {value}"));
                    } else if key == "package" {
                        last.package = unquote(value).to_owned();
                    }
                }
            }
            None => {
                let key = key.split('.').next().unwrap_or(key);
                if value.starts_with('{') && !value.contains('}') {
                    pending = Some((kind.clone(), key.to_owned()));
                    inline_value = value.to_owned();
                } else {
                    out.push(dep_of(kind, key, value));
                }
            }
        }
    }
    out
}

fn dep_of(kind: &str, key: &str, value: &str) -> Dep {
    Dep {
        package: package_in(value).unwrap_or_else(|| key.to_owned()),
        kind: kind.to_owned(),
        features: features_in(value),
    }
}

fn table_kind(header: &str) -> Option<(String, Option<String>)> {
    // `dependencies`, `dev-dependencies`, `build-dependencies`, `target.'cfg(..)'.dependencies`, and the
    // dotted `dependencies.name` forms of each.
    let parts: Vec<&str> = split_header(header);
    let idx = parts.iter().position(|p| {
        matches!(
            *p,
            "dependencies" | "dev-dependencies" | "build-dependencies"
        )
    })?;
    let kind = parts.get(idx)?.to_string();
    let name = parts.get(idx + 1).map(|n| unquote(n).to_owned());
    Some((kind, name))
}

fn split_header(header: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote: Option<char> = None;
    for (i, c) in header.char_indices() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c2) if c2 == q => quote = None,
            (None, '.') => {
                parts.push(&header[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&header[start..]);
    parts
}

/// Does a manifest declare a (non-dev) dependency on crate `dep`?
pub fn declares_dep_on(manifest: &str, dep: &str) -> bool {
    dependencies(manifest)
        .iter()
        .any(|d| d.package == dep && d.kind != "dev-dependencies")
}

/// The `name = "…"` of a manifest's `[package]`.
pub fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == "name"
        {
            return Some(unquote(v).to_owned());
        }
    }
    None
}

/// Read every member crate's manifest under `crates/`: package name → manifest text. A directory without
/// a readable manifest is skipped; an unknown package name is reported by [`unlayered_crates`].
pub fn read_manifests(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(root.join("crates")) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path().join("Cargo.toml");
        if let Ok(text) = fs::read_to_string(&path) {
            let name =
                package_name(&text).unwrap_or_else(|| e.file_name().to_string_lossy().into_owned());
            out.insert(name, text);
        }
    }
    out
}

/// Member crates that are in no layer, and layer names that have no crate.
pub fn unlayered_crates(manifests: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>) {
    let all: Vec<&str> = LOWER.iter().chain(CORE).chain(UPPER).copied().collect();
    let unknown = manifests
        .keys()
        .filter(|n| !all.contains(&n.as_str()))
        .cloned()
        .collect();
    let missing = all
        .iter()
        .filter(|n| !manifests.contains_key(**n))
        .map(|n| (*n).to_owned())
        .collect();
    (unknown, missing)
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
/// `inklkunn-signer` (dev-dependencies excepted: tests may use the bootstrap signer).
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

/// Test-only features enabled by a non-dev dependency: `(crate, dependency, feature)`.
pub fn test_feature_leaks(manifests: &BTreeMap<String, String>) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (name, text) in manifests {
        for d in dependencies(text) {
            if d.kind == "dev-dependencies" {
                continue;
            }
            for f in &d.features {
                if TEST_ONLY_FEATURES.contains(&f.as_str()) {
                    out.push((name.clone(), d.package.clone(), f.clone()));
                }
            }
        }
    }
    out
}

/// The key-reachability rule, part (b): the signer's `lib.rs` must declare the key-store module private
/// (`mod key_store;`, never `pub mod key_store`) and never re-export from it (`pub use key_store::`).
pub fn key_store_exposed(signer_lib: &str) -> bool {
    strip_comments(signer_lib).lines().any(|l| {
        let t = l.trim_start();
        (t.starts_with("pub mod ") && t.contains(KEY_STORE_MODULE))
            || (t.starts_with("pub use ") && t.contains(&format!("{KEY_STORE_MODULE}::")))
            || (t.starts_with("pub(crate) use ") && t.contains(&format!("{KEY_STORE_MODULE}::")))
    })
}

/// Every `.rs` file under `dir` (recursively), excluding only the workspace's own `target/` at `root`.
pub fn rust_sources_under(root: &Path, dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let target = root.join("target");
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p == target {
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

/// Every `.rs` file under `dir`, with the workspace root taken as two levels above this crate.
pub fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    rust_sources_under(&workspace_root(), dir)
}

/// Is this source a product source (under `src/`) rather than a test, example or benchmark?
pub fn is_product_source(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "src")
        && !path.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some("tests" | "examples" | "benches")
            )
        })
}

/// `source` with `//` line comments and `/* */` block comments removed (string literals kept).
pub fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let b = source.as_bytes();
    let at = |i: usize| b.get(i).copied();
    let mut i = 0;
    let mut in_str = false;
    let mut block = 0usize;
    while let Some(c) = at(i) {
        let next = at(i + 1);
        if block > 0 {
            if c == b'*' && next == Some(b'/') {
                block -= 1;
                i += 2;
            } else if c == b'/' && next == Some(b'*') {
                block += 1;
                i += 2;
            } else {
                if c == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            continue;
        }
        if in_str {
            if c == b'\\' {
                push_bytes(&mut out, b, i, 2);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            push_bytes(&mut out, b, i, 1);
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push('"');
            i += 1;
        } else if c == b'/' && next == Some(b'/') {
            while at(i).is_some_and(|c| c != b'\n') {
                i += 1;
            }
        } else if c == b'/' && next == Some(b'*') {
            block = 1;
            i += 2;
        } else {
            push_bytes(&mut out, b, i, 1);
            i += 1;
        }
    }
    out
}

fn push_bytes(out: &mut String, b: &[u8], i: usize, n: usize) {
    if let Some(slice) = b.get(i..(i + n).min(b.len())) {
        out.push_str(&String::from_utf8_lossy(slice));
    }
}

/// `source` with string literal contents blanked (`"..."` → `""`), after comments are stripped.
pub fn strip_strings(source: &str) -> String {
    let s = strip_comments(source);
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                in_str = false;
                out.push('"');
            }
            continue;
        }
        if c == '"' {
            in_str = true;
        }
        out.push(c);
    }
    out
}

/// Lines (1-based) of `source` whose code mentions the key-store module (comments and strings excluded,
/// word boundaries respected).
pub fn key_store_mentions(source: &str) -> Vec<usize> {
    word_mentions(&strip_strings(source), KEY_STORE_MODULE)
}

fn word_mentions(code: &str, word: &str) -> Vec<usize> {
    code.lines()
        .enumerate()
        .filter(|(_, l)| {
            let mut from = 0;
            while let Some(i) = l[from..].find(word) {
                let start = from + i;
                let end = start + word.len();
                let before = l[..start].chars().next_back();
                let after = l[end..].chars().next();
                let boundary =
                    |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
                if boundary(before) && boundary(after) {
                    return true;
                }
                from = end;
            }
            false
        })
        .map(|(i, _)| i + 1)
        .collect()
}

/// Lines of `source` whose code spawns a process: `std::process::Command` or `tokio::process` named in
/// any spelling, including inside a `use` group (comments and strings excluded).
pub fn spawn_lines(source: &str) -> Vec<usize> {
    let code = strip_strings(source);
    code.lines()
        .enumerate()
        .filter(|(_, l)| {
            SPAWN_PATHS.iter().any(|p| l.contains(p))
                || expand_use(l).iter().any(|path| {
                    path == "std::process::Command"
                        || path == "std::process"
                        || path.starts_with("tokio::process")
                })
        })
        .map(|(i, _)| i + 1)
        .collect()
}

/// The argument of every `Command::new(…)` in `source` (comments excluded, strings kept), with the line it
/// starts on: `Command::new(&self.bwrap)` gives `(n, "self.bwrap")`. A leading `&` is dropped and whitespace
/// trimmed. Parentheses are balanced, so `Command::new(f(x))` yields `f(x)`.
pub fn command_new_arguments(source: &str) -> Vec<(usize, String)> {
    let code = strip_comments(source);
    let mut out = Vec::new();
    let needle = "Command::new";
    let mut from = 0;
    while let Some(i) = code[from..].find(needle) {
        let start = from + i + needle.len();
        from = start;
        let rest = code[start..].trim_start();
        let Some(inner) = rest.strip_prefix('(') else {
            continue;
        };
        let mut depth = 1usize;
        let mut end = inner.len();
        for (k, c) in inner.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = k;
                        break;
                    }
                }
                _ => {}
            }
        }
        let arg = inner[..end]
            .trim()
            .trim_end_matches(',')
            .trim_start_matches('&')
            .trim()
            .to_owned();
        let line = code[..start].matches('\n').count() + 1;
        out.push((line, arg));
    }
    out
}

/// What the fetch crate's `source` spawns that it may not: any `Command::new` whose argument is not in
/// [`FETCH_SPAWN_ALLOWED_ARGS`], and any use of `tokio::process`. Returns `(line, what)`. The crate may spawn
/// `bwrap` (which runs `curl`) and, in its tests-only unconfined mode, `curl`; a spawn of `prikk`, a shell or
/// anything computed fails.
pub fn fetch_spawn_violations(source: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = command_new_arguments(source)
        .into_iter()
        .filter(|(_, arg)| !FETCH_SPAWN_ALLOWED_ARGS.contains(&arg.as_str()))
        .collect();
    let code = strip_strings(source);
    for (i, line) in code.lines().enumerate() {
        if line.contains("tokio::process") {
            out.push((i + 1, "tokio::process".to_owned()));
        }
    }
    out.sort();
    out
}

/// Lines (1-based) of `source` that name `curl` as a word, in code or in a string literal, in any case
/// (`Curl` the type, `"curl"` the program, `curl` the variable; `CurlFetcher` and `curly` are other words).
/// Comments are excluded: prose may say what the fetcher is.
pub fn curl_mentions(source: &str) -> Vec<usize> {
    word_mentions(&strip_comments(source).to_ascii_lowercase(), "curl")
}

/// Every crate other than [`FETCH_CONSUMER`] and the fetch crate itself that depends on [`FETCH_CRATE`] in a
/// non-dev table.
pub fn fetch_dependents(manifests: &BTreeMap<String, String>) -> Vec<String> {
    manifests
        .iter()
        .filter(|(name, text)| {
            name.as_str() != FETCH_CONSUMER
                && name.as_str() != FETCH_CRATE
                && declares_dep_on(text, FETCH_CRATE)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The non-dev dependencies of the fetch crate's `manifest` that are not in [`FETCH_ALLOWED_DEPS`].
pub fn fetch_disallowed_deps(manifest: &str) -> Vec<String> {
    runtime_dependencies(manifest)
        .into_iter()
        .filter(|d| !FETCH_ALLOWED_DEPS.contains(&d.as_str()))
        .collect()
}

/// String literals in `source` that are prikk write verbs (the read driver may not name them). Comments
/// are stripped first; a literal inside an attribute (`#[serde(rename = "ref")]` names a report key) is
/// not an argument vector and is skipped.
pub fn write_verb_literals(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in strip_comments(source).lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("#[") || t.starts_with("#![") {
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

/// Lines of `source` whose code reaches for an impure path (the markup crate may not). `use` groups are
/// expanded (`use std::{fs, env};` names `std::fs` and `std::env`).
pub fn impure_lines(source: &str) -> Vec<(usize, String)> {
    let code = strip_strings(source);
    let mut out = Vec::new();
    for (i, line) in code.lines().enumerate() {
        for path in expand_use(line) {
            for p in IMPURE_PATHS {
                if path.starts_with(p) || path == *p {
                    out.push((i + 1, (*p).to_owned()));
                }
            }
        }
        if !line.trim_start().starts_with("use ") {
            for p in IMPURE_PATHS {
                if line.contains(p) {
                    out.push((i + 1, (*p).to_owned()));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The full paths a `use` line names: `use std::{fs, time::{Instant, SystemTime}};` → `std::fs`,
/// `std::time::Instant`, `std::time::SystemTime`.
pub fn expand_use(line: &str) -> Vec<String> {
    let t = line.trim();
    let Some(rest) = t
        .strip_prefix("pub use ")
        .or_else(|| t.strip_prefix("pub(crate) use "))
        .or_else(|| t.strip_prefix("use "))
    else {
        return Vec::new();
    };
    let rest = rest.trim_end_matches(';').trim();
    let mut out = Vec::new();
    expand(rest, "", &mut out);
    out
}

fn expand(spec: &str, prefix: &str, out: &mut Vec<String>) {
    let spec = spec.trim();
    if let Some(open) = spec.find('{') {
        let head = spec[..open].trim().trim_end_matches("::");
        let inner = spec[open + 1..].trim_end_matches('}');
        let prefix = if head.is_empty() {
            prefix.to_owned()
        } else {
            join(prefix, head)
        };
        for part in split_top(inner) {
            expand(part, &prefix, out);
        }
    } else {
        let name = spec.split(" as ").next().unwrap_or(spec).trim();
        if !name.is_empty() {
            out.push(join(prefix, name));
        }
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else if name == "self" {
        prefix.to_owned()
    } else {
        format!("{prefix}::{name}")
    }
}

fn split_top(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// Third-party dependencies named by a manifest's non-dev dependency tables, in every spelling.
pub fn runtime_dependencies(manifest: &str) -> Vec<String> {
    dependencies(manifest)
        .into_iter()
        .filter(|d| d.kind != "dev-dependencies")
        .map(|d| d.package)
        .collect()
}

/// The lexical tokens of Rust-ish `code` with the line each starts on: identifiers and numbers whole,
/// every other non-space character alone. Whitespace only separates; it is never a token.
fn lex(code: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut line = 1;
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            line += 1;
        } else if c.is_whitespace() {
            continue;
        } else if c.is_alphanumeric() || c == '_' {
            let mut word = String::from(c);
            while let Some(&n) = chars.peek() {
                if n.is_alphanumeric() || n == '_' {
                    word.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push((word, line));
        } else {
            out.push((c.to_string(), line));
        }
    }
    out
}

/// Where `tokens` appear in `source`'s code (comments and strings excluded), **whatever whitespace splits
/// them**: `.route (` and `.route\n(` are the same call, `impl X` and `dyn X` still begin with the word `X`,
/// and `SubRouter::new` is not `Router::new` (words are matched whole). Returns `(line, token)` of each
/// match's first token.
fn token_hits(source: &str, tokens: &[&str]) -> Vec<(usize, String)> {
    let code = lex(&strip_strings(source));
    let mut out = Vec::new();
    for token in tokens {
        let pattern: Vec<String> = lex(token).into_iter().map(|(t, _)| t).collect();
        if pattern.is_empty() || pattern.len() > code.len() {
            continue;
        }
        for window_start in 0..=(code.len() - pattern.len()) {
            let matches = pattern.iter().enumerate().all(|(k, want)| {
                code.get(window_start + k)
                    .is_some_and(|(have, _)| have == want)
            });
            if matches {
                let line = code.get(window_start).map_or(1, |(_, l)| *l);
                out.push((line, (*token).to_owned()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Lines of the signer's `source` that declare a **public** item (`pub`, not `pub(crate)` or `pub(super)`)
/// whose line mentions a seed (any case) or one of [`SIGNER_PUBLIC_FORBIDDEN_TYPES`]. Comments and strings
/// are excluded. A public signature that names a seed, a secret or a key directory could hand one out.
pub fn public_seed_exposures(source: &str) -> Vec<(usize, String)> {
    let code = strip_strings(source);
    let mut out = Vec::new();
    for (i, line) in code.lines().enumerate() {
        let t = line.trim_start();
        let is_public = t.starts_with("pub ") && !t.starts_with("pub(");
        if !is_public {
            continue;
        }
        if t.to_ascii_lowercase().contains("seed") {
            out.push((i + 1, "seed".to_owned()));
        }
        for name in SIGNER_PUBLIC_FORBIDDEN_TYPES {
            if !word_mentions(t, name).is_empty() {
                out.push((i + 1, (*name).to_owned()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Lines of `source` that register a route or merge a router ([`ROUTE_REGISTRATION_TOKENS`]).
pub fn route_registrations(source: &str) -> Vec<(usize, String)> {
    token_hits(source, ROUTE_REGISTRATION_TOKENS)
}

/// Lines of `source` that name a privileged principal ([`PRIVILEGED_PRINCIPAL_NAMES`]).
pub fn privileged_principal_mentions(source: &str) -> Vec<(usize, String)> {
    token_hits(source, PRIVILEGED_PRINCIPAL_NAMES)
}

/// Lines of `source` that name one of [`SIGNER_NAMES_FORBIDDEN_IN_SURFACES`].
pub fn signer_mentions_in_a_surface(source: &str) -> Vec<(usize, String)> {
    token_hits(source, SIGNER_NAMES_FORBIDDEN_IN_SURFACES)
}

/// Lines of `source` whose code names one of [`SIGNER_ONLY_NAMES`].
pub fn signer_only_mentions(source: &str) -> Vec<(usize, String)> {
    let code = strip_strings(source);
    let mut out = Vec::new();
    for (i, line) in code.lines().enumerate() {
        for name in SIGNER_ONLY_NAMES {
            if line.contains(name) {
                out.push((i + 1, (*name).to_owned()));
            }
        }
    }
    out
}

/// Does a crate root (`lib.rs` or `main.rs`) begin with `#![forbid(unsafe_code)]` (comments and blank
/// lines skipped)?
pub fn forbids_unsafe(crate_root: &str) -> bool {
    strip_comments(crate_root)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .is_some_and(|l| l.replace(' ', "") == "#![forbid(unsafe_code)]")
}

/// Does a manifest carry `[lints] workspace = true`?
pub fn uses_workspace_lints(manifest: &str) -> bool {
    let mut in_lints = false;
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_lints = line == "[lints]";
            continue;
        }
        if in_lints && line.replace(' ', "") == "workspace=true" {
            return true;
        }
    }
    false
}
