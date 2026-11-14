#![forbid(unsafe_code)]
//! `inklkunn-floor-check` — RFC 010 D-3: the prikk floor is **one** value across the driver constant,
//! the CI workflow, the release workflow and the container recipe, **and so are the checksums** of the
//! release assets it names (recorded here, so an asset is checked against what inklkunn measured, not
//! against a file fetched beside it); and the copied wiki-commons corpus matches the commit it says it
//! came from. The check lives in `tests/floor.rs`; this module holds the extractors so each can be
//! exercised on a planted mismatch.
//!
//! **A second prikk, test-only (maintenance M25).** The workflows also install one newer release
//! (`PRIKK_TEST_VERSION`, with its own checksum) so that the tests of what it added run in CI. It is **not** a
//! floor: nothing but the tests reads it, it is never in the image, it is not the driver's
//! `MIN_PRIKK_VERSION`, and it goes on `PATH` after the floor so the floor stays the prikk every other test
//! uses. Its name never starts with `PRIKK_VERSION` or `PRIKK_SHA256_`, so the floor rules above do not read it;
//! [`test_prikk`] and [`test_prikk_shape_violations`] hold it instead.

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

fn quoted_value(yaml: &str, key: &str) -> Option<String> {
    yaml.lines()
        .map(str::trim)
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split('"').nth(1).map(str::to_owned))
}

/// The test-only prikk a workflow records: `PRIKK_TEST_VERSION` and `PRIKK_TEST_SHA256_X86_64` (maintenance M25).
pub fn test_prikk(yaml: &str) -> Option<(String, String)> {
    Some((
        quoted_value(yaml, "PRIKK_TEST_VERSION:")?,
        quoted_value(yaml, "PRIKK_TEST_SHA256_X86_64:")?,
    ))
}

/// What is wrong with the way a workflow uses its test-only prikk (M25); empty when it is installed and
/// recorded, comes after the floor on `PATH`, is removed with the floor in the shimmed run, and the with-prikk
/// run fails when a test that needs it skipped.
pub fn test_prikk_shape_violations(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    match test_prikk(yaml) {
        None => {
            out.push("no PRIKK_TEST_VERSION / PRIKK_TEST_SHA256_X86_64 recorded".to_owned());
            return out;
        }
        Some((_, sum)) => {
            if sum.len() != 64 || !sum.bytes().all(|b| b.is_ascii_hexdigit()) {
                out.push(format!(
                    "the test-only checksum is not 64 hex digits: {sum}"
                ));
            }
        }
    }
    // GitHub prepends each `$GITHUB_PATH` entry, so what is installed first is found last: the test-only
    // prikk's step must come before the floor's.
    match (
        yaml.find("releases/download/${PRIKK_TEST_VERSION}/"),
        yaml.find("releases/download/${PRIKK_VERSION}/"),
    ) {
        (Some(test), Some(floor)) if test < floor => {}
        (Some(_), Some(_)) => {
            out.push(
                "the test-only prikk is installed after the floor, so it would come first on PATH"
                    .to_owned(),
            );
        }
        _ => {
            out.push("the test-only prikk or the floor is not downloaded from a release".to_owned())
        }
    }
    if !yaml.contains("sha256sum -c") || !yaml.contains("${PRIKK_TEST_SHA256_X86_64}") {
        out.push("the test-only asset is not checked against its recorded checksum".to_owned());
    }
    // The with-prikk run: output is shown (`--nocapture`, or the skip lines are never printed), kept, and a
    // skip line of either 0.57.0 test fails the job.
    let lines: Vec<&str> = yaml.lines().collect();
    match lines.iter().position(|l| l.contains("tee with-prikk.log")) {
        None => out
            .push("the with-prikk run does not keep its output (`tee with-prikk.log`)".to_owned()),
        Some(i) => {
            if !lines.get(i).is_some_and(|l| l.contains("--nocapture")) {
                out.push("the with-prikk run is not `--nocapture`: a passing test's skip line is never printed".to_owned());
            }
        }
    }
    let guard = lines.iter().position(|l| {
        l.contains("grep")
            && l.contains("with-prikk.log")
            && l.contains("prikk 0\\.57\\.0 or newer not found")
            && l.contains("no prikk older than 0\\.57\\.0")
            && l.contains("PID namespaces unavailable")
    });
    match guard {
        None => out.push(
            "no grep of with-prikk.log for both 0.57.0 skip lines and the PID-namespace one"
                .to_owned(),
        ),
        Some(i) => {
            let fails = lines
                .iter()
                .skip(i + 1)
                .take(3)
                .any(|l| l.trim() == "exit 1");
            if !fails {
                out.push(
                    "a 0.57.0 skip line found in with-prikk.log does not fail the run".to_owned(),
                );
            }
        }
    }
    // The shimmed run removes both directories.
    let shim = lines.iter().find(|l| l.contains("SYSTEM_PATH=\""));
    if !shim.is_some_and(|l| {
        l.contains("\"$HOME/.local/bin\"") && l.contains("\"$HOME/.local/prikk-test/bin\"")
    }) {
        out.push("the shimmed run does not remove both prikk directories from PATH".to_owned());
    }
    out
}

/// The line that lifts the runner's restriction on unprivileged user namespaces (Ubuntu 24.04's AppArmor).
const LIFT_USERNS: &str = "sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0";

/// What is wrong with how a workflow makes user namespaces available to its tests (maintenance M37): the
/// PID-namespace test of the extract locks and the bubblewrap tests need them, and the runner forbids them
/// until the restriction is lifted. Empty when a `run:` line lifts it before the first `cargo test`.
pub fn user_namespace_violations(yaml: &str) -> Vec<String> {
    let lines: Vec<&str> = yaml.lines().map(str::trim).collect();
    let lifts = lines
        .iter()
        .position(|l| l.strip_prefix("run:").unwrap_or(l).trim() == LIFT_USERNS);
    let first_test = lines.iter().position(|l| l.contains("cargo test"));
    match (lifts, first_test) {
        (None, _) => vec![format!("no line `{LIFT_USERNS}`")],
        (Some(_), None) => vec!["the workflow runs no `cargo test`".to_owned()],
        (Some(lift), Some(test)) if lift < test => Vec::new(),
        (Some(_), Some(_)) => vec![
            "the restriction on user namespaces is lifted after the first `cargo test`".to_owned(),
        ],
    }
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
