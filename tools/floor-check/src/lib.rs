#![forbid(unsafe_code)]
//! `inklkunn-floor-check` — RFC 010 D-3: the prikk floor is **one** value across the driver constant,
//! the CI workflow, the release workflow and the container recipe; and the copied wiki-commons corpus
//! matches the commit it says it came from. The check lives in `tests/floor.rs`; this module holds the
//! extractors so each can be exercised on a planted mismatch.

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

/// `PRIKK_VERSION: "X.Y.Z"` in a workflow.
pub fn workflow_floor(yaml: &str) -> Option<String> {
    yaml.lines()
        .map(str::trim)
        .find(|l| l.starts_with("PRIKK_VERSION:"))
        .and_then(|l| l.split('"').nth(1).map(str::to_owned))
}

/// `ARG PRIKK_VERSION=X.Y.Z` in the container recipe.
pub fn dockerfile_floor(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with("ARG PRIKK_VERSION="))
        .and_then(|l| l.split('=').nth(1).map(str::to_owned))
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
