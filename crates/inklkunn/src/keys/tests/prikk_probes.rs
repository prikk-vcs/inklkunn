//! Every question the command-line tests ask of the prikk on `PATH` (maintenance M39).
//!
//! A test reaches a prikk capability only through this module. What is public here returns a binary and prints the
//! **guarded line** itself when it returns none (the with-prikk run in CI fails on
//! `skipping: prikk 0.57.0 or newer not found` and on `skipping: no prikk older than 0.57.0 (and at the floor) found`),
//! or asserts and returns nothing. What decides is private: no test can branch on it. The workspace check
//! `tools/floor-check/tests/guarded_skips.rs` reads this file: a probe is found by its definition here, a call of one
//! outside this module fails it, and a public function that can return none without printing fails it.
use std::path::{Path, PathBuf};

/// Every `prikk` on `PATH`, in order.
fn prikks_on_path() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("prikk"))
                .filter(|c| c.is_file())
                .collect()
        })
        .unwrap_or_default()
}

fn at_least_057(binary: &Path) -> bool {
    inklkunn_prikk::probe_version(binary.as_os_str()).is_some_and(|v| v >= (0, 57, 0))
}

/// Does the prikk whose `--version` text this is report who sealed each block as recorded (`verify --format json`'s
/// `block_seals`, 0.57.0 and later, ledger PK-39)? Then the late-seal check sees a key that was removed from trust;
/// before, it did not.
fn reports_block_seals(version_text: &str) -> bool {
    inklkunn_prikk::parse_version(version_text).is_some_and(|v| v >= (0, 57, 0))
}

/// The first `prikk` on `PATH` (CI's is the floor), in whatever version.
pub fn prikk_first() -> Option<PathBuf> {
    let Some(first) = prikks_on_path().into_iter().next() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    Some(first)
}

/// The first `prikk` on `PATH`, when it is older than 0.57.0: the tests of what prikk did before it reported
/// `block_seals`. A first prikk that reports them prints the guarded line (CI fails on it: the floor must be first),
/// and the subject runs on the 0.57.0 side through [`prikk_057`] (maintenance M34).
pub fn prikk_floor() -> Option<PathBuf> {
    let Some(first) = prikks_on_path().into_iter().next() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    if at_least_057(&first) {
        println!("skipping: no prikk older than 0.57.0 (and at the floor) found");
        return None;
    }
    Some(first)
}

/// The first `prikk` on `PATH` that is 0.57.0 or newer, wherever it is: the tests of what 0.57.0 reports (ledger
/// PK-39). With none it prints the line CI fails on.
pub fn prikk_057() -> Option<PathBuf> {
    let Some(new) = prikks_on_path().into_iter().find(|c| at_least_057(c)) else {
        println!("skipping: prikk 0.57.0 or newer not found");
        return None;
    };
    Some(new)
}

/// Asserts whether the prikk whose `--version` text this is reports `block_seals`. The worlds assert it as they are
/// made, so a world built over the wrong prikk fails there rather than testing the other side.
pub fn assert_reports_block_seals(version_text: &str, expected: bool) {
    assert_eq!(reports_block_seals(version_text), expected);
}
