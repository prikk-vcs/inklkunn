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
//!
//! **The release the upgrade test starts from (0.3.0 readiness, R-3).** The workflows also install one earlier
//! *inklkunn* release (`UPGRADE_FROM_VERSION`, with the checksum of its release asset, `UPGRADE_FROM_SHA256_X86_64`):
//! the upgrade test has that binary make a data directory and this tree open it. It is a pinned release asset like
//! the test-only prikk (recorded checksum, checked before it is unpacked), it is on `PATH` for that one test, and
//! the test prints a guarded skip line when it is not found. [`upgrade_from`] and [`upgrade_from_shape_violations`]
//! hold it.

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

/// The parts of the skip lines the with-prikk run's `grep` must name, each as written in the workflow's pattern (a
/// regular expression, so the dots are escaped): the two 0.57.0 lines (M25), the PID-namespace one,
/// `prikk not on PATH` (prikk is always on `PATH` in that run, so the line can only be a silent skip; M41) and the
/// confined tests' lines (`bubblewrap cannot confine` carries bubblewrap's own reason; `the confined policy`; `bwrap or
/// curl not available` is the real vector's; M43), and the upgrade test's (`inklkunn 0.2.0 not found`: the release the
/// workflow installs for it is not on `PATH`; R-3).
pub const WITH_PRIKK_GUARDED: [&str; 8] = [
    "prikk 0\\.57\\.0 or newer not found",
    "no prikk older than 0\\.57\\.0",
    "PID namespaces unavailable",
    "prikk not on PATH",
    "bubblewrap cannot confine",
    "the confined policy",
    "bwrap or curl not available",
    "inklkunn 0\\.2\\.0 not found",
];

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
            // One red must not hide the rest of the workspace (CI run 44065172543 stopped after one crate's failure).
            if !lines.get(i).is_some_and(|l| l.contains("--no-fail-fast")) {
                out.push("the with-prikk run is not `--no-fail-fast`: the first failing test binary stops the tests of every crate after it".to_owned());
            }
        }
    }
    // The shimmed run counts its self-skips, so one red in it must not hide the crates after it either.
    if let Some(l) = lines.iter().find(|l| l.contains("tee shimmed.log"))
        && !l.contains("--no-fail-fast")
    {
        out.push("the shimmed run is not `--no-fail-fast`: the first failing test binary stops the tests of every crate after it, and the self-skips are then counted over a part of the workspace".to_owned());
    }
    let guard = lines.iter().position(|l| {
        l.contains("grep")
            && l.contains("with-prikk.log")
            && WITH_PRIKK_GUARDED
                .iter()
                .all(|fragment| l.contains(fragment))
    });
    match guard {
        None => out.push(
            "no grep of with-prikk.log for every skip line the with-prikk run must not print (both 0.57.0 lines, the PID-namespace one, `prikk not on PATH`, the bubblewrap ones, and the upgrade test's)"
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

/// The earlier inklkunn release a workflow installs for the upgrade test: `UPGRADE_FROM_VERSION` and
/// `UPGRADE_FROM_SHA256_X86_64` (0.3.0 readiness, R-3).
pub fn upgrade_from(yaml: &str) -> Option<(String, String)> {
    Some((
        quoted_value(yaml, "UPGRADE_FROM_VERSION:")?,
        quoted_value(yaml, "UPGRADE_FROM_SHA256_X86_64:")?,
    ))
}

/// What is wrong with the way a workflow installs the release the upgrade test starts from (R-3); empty when its
/// version and checksum are recorded, the asset is downloaded from this project's own release of that version and
/// checked against the recorded checksum before it is unpacked, and it is put on `PATH`, where the test looks.
pub fn upgrade_from_shape_violations(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    match upgrade_from(yaml) {
        None => {
            out.push("no UPGRADE_FROM_VERSION / UPGRADE_FROM_SHA256_X86_64 recorded".to_owned());
            return out;
        }
        Some((_, sum)) => {
            if sum.len() != 64 || !sum.bytes().all(|b| b.is_ascii_hexdigit()) {
                out.push(format!(
                    "the upgrade-from checksum is not 64 hex digits: {sum}"
                ));
            }
        }
    }
    let lines: Vec<&str> = yaml.lines().collect();
    let download = lines.iter().position(|l| {
        l.contains("curl")
            && l.contains(
                "github.com/prikk-vcs/inklkunn/releases/download/${UPGRADE_FROM_VERSION}/",
            )
    });
    let Some(download) = download else {
        out.push(
            "the upgrade-from asset is not downloaded from this project's release of that version"
                .to_owned(),
        );
        return out;
    };
    // The check comes after the download and before the unpacking.
    let check = lines
        .iter()
        .position(|l| l.contains("sha256sum -c") && l.contains("${UPGRADE_FROM_SHA256_X86_64}"));
    let unpack = lines
        .iter()
        .skip(download)
        .position(|l| l.contains("tar -xzf"))
        .map(|i| i + download);
    match (check, unpack) {
        (Some(c), Some(u)) if download < c && c < u => {}
        _ => out.push(
            "the upgrade-from asset is not checked against its recorded checksum between its download and its unpacking"
                .to_owned(),
        ),
    }
    if !lines
        .iter()
        .any(|l| l.contains(">> \"$GITHUB_PATH\"") && l.contains("inklkunn-old/bin"))
    {
        out.push("the upgrade-from binary's directory is not added to PATH".to_owned());
    }
    out
}

/// The line that lifts the runner's restriction on unprivileged user namespaces (Ubuntu 24.04's AppArmor).
const LIFT_USERNS: &str = "sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0";

/// One step of a job: its command lines (comments left out) and the `if:` expressions in force on it, the
/// job's first and then its own, whitespace-normalized and without `${{ }}`.
struct JobStep {
    lines: Vec<String>,
    guards: Vec<String>,
    /// Is the step `continue-on-error` (any value but `false`)? Its failure then stops nothing.
    optional: bool,
}

/// Is `v` the indicator of a block scalar (`>`, `>-`, `|`, `|+`, `>2`), which is not part of the text?
fn is_block_indicator(v: &str) -> bool {
    let v = v.split(" #").next().unwrap_or("").trim();
    v.starts_with(['>', '|'])
        && v.chars()
            .skip(1)
            .all(|c| matches!(c, '+' | '-' | '0'..='9'))
}

/// The `if:` on `lines[at]`, if it is one: its **whole** value (a folded `if: >-` or a plain value continued on
/// the lines below it, comment lines left out), normalized and without `${{ }}`, and the number of lines after
/// `at` that belong to it. The first line alone would make two folded guards equal (maintenance M42).
fn guard_at(lines: &[&str], at: usize) -> Option<(String, usize)> {
    let line = lines.get(at)?;
    let t = line.trim_start();
    let (key_indent, rest) = match t.strip_prefix("- if:") {
        Some(rest) => (indent_of(line) + 2, rest),
        None => (indent_of(line), t.strip_prefix("if:")?),
    };
    let mut value = rest.trim().to_owned();
    if is_block_indicator(&value) {
        value.clear();
    }
    let mut used = 0;
    for l in lines.iter().skip(at + 1) {
        let lt = l.trim();
        if !lt.is_empty() && indent_of(l) <= key_indent {
            break;
        }
        used += 1;
        if !lt.is_empty() && !lt.starts_with('#') {
            value.push(' ');
            value.push_str(lt);
        }
    }
    let v = value.trim();
    let v = v
        .strip_prefix("${{")
        .and_then(|r| r.strip_suffix("}}"))
        .unwrap_or(v)
        .trim();
    Some((v.split_whitespace().collect::<Vec<_>>().join(" "), used))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The jobs of a workflow as `(name, steps)`, read line by line: a job is a key at indent 2 under `jobs:`, its
/// `if:` a key at indent 4, its steps the `- ` items under its `steps:`. Reading what the canonical form that
/// `pin-check` enforces allows is enough here; that gate refuses what this cannot read.
fn jobs_with_steps(yaml: &str) -> Vec<(String, Vec<JobStep>)> {
    let lines: Vec<&str> = yaml.lines().collect();
    let mut jobs: Vec<(String, Option<String>, Vec<JobStep>)> = Vec::new();
    let mut in_jobs = false;
    let mut in_steps = false;
    let mut step_indent: Option<usize> = None;
    let mut at = 0;
    while let Some(&line) = lines.get(at) {
        let here = at;
        at += 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 {
            in_jobs = t == "jobs:";
            in_steps = false;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if indent == 2 && t.ends_with(':') && !t.starts_with('-') {
            jobs.push((t.trim_end_matches(':').to_owned(), None, Vec::new()));
            in_steps = false;
            step_indent = None;
            continue;
        }
        let Some((_, job_guard, steps)) = jobs.last_mut() else {
            continue;
        };
        if indent == 4 {
            in_steps = t == "steps:";
            step_indent = None;
            if let Some((g, used)) = guard_at(&lines, here) {
                *job_guard = Some(g);
                at += used;
            }
            continue;
        }
        if !in_steps {
            continue;
        }
        let si = *step_indent.get_or_insert(indent);
        if indent == si && t.starts_with("- ") {
            steps.push(JobStep {
                lines: Vec::new(),
                guards: Vec::new(),
                optional: false,
            });
        }
        if let Some(cur) = steps.last_mut() {
            cur.lines.push(t.to_owned());
            if indent == si || indent == si + 2 {
                if let Some((g, used)) = guard_at(&lines, here) {
                    cur.guards.push(g);
                    at += used;
                } else if let Some(v) = t
                    .strip_prefix("- ")
                    .unwrap_or(t)
                    .strip_prefix("continue-on-error:")
                {
                    cur.optional = v.split(" #").next().unwrap_or("").trim() != "false";
                }
            }
        }
    }
    // The job's `if:` may stand before or after its steps: it holds on every step either way.
    jobs.into_iter()
        .map(|(name, job_guard, mut steps)| {
            if let Some(g) = job_guard {
                for s in &mut steps {
                    s.guards.insert(0, g.clone());
                }
            }
            (name, steps)
        })
        .collect()
}

fn lifts(step_line: &str) -> bool {
    let l = step_line.strip_prefix("- ").unwrap_or(step_line);
    l.strip_prefix("run:").unwrap_or(l).trim() == LIFT_USERNS
}

/// Does this command line run the tests: `cargo test`, or `cargo +<toolchain> test`?
fn runs_tests(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    words.windows(2).any(|w| w == ["cargo", "test"])
        || words.windows(3).any(|w| {
            w.first() == Some(&"cargo")
                && w.get(1).is_some_and(|p| p.starts_with('+'))
                && w.get(2) == Some(&"test")
        })
}

/// What is wrong with how a workflow makes user namespaces available to its tests (maintenance M37, M40, M42): the
/// PID-namespace test of the extract locks and the bubblewrap tests need them, and the runner forbids them
/// until the restriction is lifted. Checked **per job**: every step of a job that runs `cargo test` (or
/// `cargo +<toolchain> test`) needs a step of the same job before it (or an earlier line of the same step) that
/// lifts the restriction, a lift that is not `continue-on-error`, and every `if:` in force on the lift (its job's
/// and its own, each read whole, folded lines included) must hold on the test step too, so a lift guarded by a
/// condition the test does not share is no lift. Empty when every job that runs `cargo test` does so.
pub fn user_namespace_violations(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut any_test = false;
    for (job, steps) in jobs_with_steps(yaml) {
        for (i, step) in steps.iter().enumerate() {
            let Some(test_line) = step.lines.iter().position(|l| runs_tests(l)) else {
                continue;
            };
            any_test = true;
            // Every lift before the test: (its guards, is it optional).
            let mut lifts_before: Vec<(&Vec<String>, bool)> = steps
                .iter()
                .take(i)
                .filter(|s| s.lines.iter().any(|l| lifts(l)))
                .map(|s| (&s.guards, s.optional))
                .collect();
            if step.lines.iter().take(test_line).any(|l| lifts(l)) {
                lifts_before.push((&step.guards, step.optional));
            }
            if lifts_before.is_empty() {
                let after = steps
                    .iter()
                    .skip(i)
                    .any(|s| s.lines.iter().any(|l| lifts(l)));
                out.push(if after {
                    format!("job `{job}`: the restriction on user namespaces is lifted after a `cargo test` step")
                } else {
                    format!("job `{job}` runs `cargo test` and has no step `{LIFT_USERNS}`")
                });
                continue;
            }
            lifts_before.retain(|(_, optional)| !optional);
            if lifts_before.is_empty() {
                out.push(format!(
                    "job `{job}`: every lift of the restriction before a `cargo test` step is `continue-on-error`, so a failed lift stops nothing"
                ));
            } else if !lifts_before
                .iter()
                .any(|(g, _)| g.iter().all(|e| step.guards.contains(e)))
            {
                out.push(format!(
                    "job `{job}`: every lift of the restriction before a `cargo test` step is guarded by an `if:` that the test step does not share"
                ));
            }
        }
    }
    if !any_test {
        out.push("the workflow runs no `cargo test`".to_owned());
    }
    out
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
