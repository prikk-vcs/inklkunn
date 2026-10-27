#![forbid(unsafe_code)]
//! `inklkunn-floor-check` — RFC 010 D-3: the prikk floor is **one** value across the driver constant,
//! the CI workflow, the release workflow and the container recipe, **and so are the checksums** of the
//! release assets it names (recorded here, so an asset is checked against what inklkunn measured, not
//! against a file fetched beside it); and the copied wiki-commons corpus matches the commit it says it
//! came from. The check lives in `tests/floor.rs`; this module holds the extractors so each can be
//! exercised on a planted mismatch.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The workspace root: two levels above this crate's manifest.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `MIN_PRIKK_VERSION: &str = "X.Y.Z"` in the driver's source.
pub fn driver_floor(source: &str) -> Option<String> {
    source
        .lines()
        .find(|l| l.contains("MIN_PRIKK_VERSION") && l.contains('"'))
        .and_then(|l| l.split('"').nth(1).map(str::to_owned))
}

/// Every `PRIKK_VERSION: "X.Y.Z"` in a workflow (a job-level copy must agree with the top-level one).
pub fn workflow_floors(yaml: &str) -> Vec<String> {
    yaml.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("PRIKK_VERSION:"))
        .filter_map(|l| l.split('"').nth(1).map(str::to_owned))
        .collect()
}

/// `ARG PRIKK_VERSION=X.Y.Z` in the container recipe.
pub fn dockerfile_floor(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with("ARG PRIKK_VERSION="))
        .and_then(|l| l.split('=').nth(1).map(str::to_owned))
}

/// The recorded release-asset checksums, by target architecture word (`X86_64`, `AARCH64`), from
/// `PRIKK_SHA256_<ARCH>: "<hex>"` (workflows) or `ARG PRIKK_SHA256_<ARCH>=<hex>` (Dockerfile).
pub fn recorded_checksums(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        let rest = line
            .strip_prefix("ARG PRIKK_SHA256_")
            .or_else(|| line.strip_prefix("PRIKK_SHA256_"));
        let Some(rest) = rest else {
            continue;
        };
        let Some((arch, value)) = rest.split_once([':', '=']) else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_owned();
        if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            out.insert(arch.trim().to_owned(), value);
        }
    }
    out
}

/// A small, dependency-free digest of a directory tree (sorted relative paths and contents), used to pin
/// the copied corpus to the commit recorded in `CORPUS-SOURCE.md`. Not cryptographic: it detects drift.
pub fn tree_digest(dir: &Path) -> u64 {
    let mut files = Vec::new();
    collect(dir, dir, &mut files);
    files.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for rel in files {
        for b in rel.as_bytes() {
            h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
        h = (h ^ 0xff).wrapping_mul(0x0100_0000_01b3);
        if let Ok(bytes) = fs::read(dir.join(&rel)) {
            for b in bytes {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    h
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(root, &p, out);
        } else if p.file_name().is_some_and(|n| n != "CORPUS-SOURCE.md")
            && let Ok(rel) = p.strip_prefix(root)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// `digest: <hex>` recorded in `CORPUS-SOURCE.md`.
pub fn recorded_digest(source_md: &str) -> Option<u64> {
    source_md
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("digest:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
}
