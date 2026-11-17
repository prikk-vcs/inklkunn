//! Every question this test crate asks of the prikk on `PATH` (maintenance M39).
//!
//! A test reaches a prikk capability only through this module. What is public here either returns a binary
//! and prints the **guarded line** itself when it returns none (the with-prikk run in CI fails on
//! every skip line the guard names, `WITH_PRIKK_GUARDED` in `tools/floor-check`, among them
//! `skipping: prikk 0.57.0 or newer not found`, `skipping: no prikk older than 0.57.0 (and at the floor) found` and
//! `skipping: prikk not on PATH`), or asserts and returns nothing. What decides is private: no test of the crate can branch on it. The workspace check
//! `tools/floor-check/tests/guarded_skips.rs` reads this file: a probe is found by its definition here, a call of one
//! outside this module fails it, and a public function that can return none without printing fails it.
// Each test file of the crate mounts this module and uses some of it.
#![allow(dead_code)]

use std::path::PathBuf;
use std::time::Duration;

use inklkunn_core::Witnesses;
use inklkunn_prikk::{Admission, PrikkCli, Sandbox};
use inklkunn_store::WikiId;

/// The first `prikk` on `PATH` (the first directory that has one), in whatever version.
fn first_prikk_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("prikk"))
        .find(|c| c.is_file())
}

/// The first `prikk` on `PATH` (any directory, not only the first) that reports 0.57.0 or newer.
fn real_prikk_at_least_057() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("prikk"))
        .filter(|c| c.is_file())
        .find(|c| {
            // The driver asks the binary for its version (the core crate may not spawn a process itself).
            PrikkCli::open(
                c,
                Sandbox::Unconfined,
                Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
            )
            .ok()
            .and_then(|p| inklkunn_prikk::parse_version(p.version()))
            .is_some_and(|v| v >= (0, 57, 0))
        })
}

/// The first `prikk` on `PATH`, in whatever version: the tests of what holds on any prikk. With none it prints the line
/// the shimmed run counts (`EXPECTED_SKIPS`).
pub fn prikk_first() -> Option<PathBuf> {
    let Some(first) = first_prikk_on_path() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    Some(first)
}

/// The first prikk 0.57.0 or newer on `PATH`, wherever it stands (the host's, when the gate runs with 0.55.0 first): for
/// the tests that need `uncommit` and what 0.57.0 does with a held lock. With none it prints the line CI fails on.
pub fn prikk_057() -> Option<PathBuf> {
    let Some(found) = real_prikk_at_least_057() else {
        println!("skipping: prikk 0.57.0 or newer not found");
        return None;
    };
    Some(found)
}

/// The **first** `prikk` on `PATH`, when it is older than 0.57.0 (CI's is the floor): the tests of the answer *no
/// verb*. A first prikk that has `uncommit` prints the guarded line (CI fails on it: the floor must be first), and the
/// subject runs on the 0.57.0 side through [`prikk_057`] instead (maintenance M28).
pub fn prikk_floor() -> Option<PathBuf> {
    let Some(first) = first_prikk_on_path() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    if inklkunn_prikk::probe_version(first.as_os_str()).is_none_or(|v| v >= (0, 57, 0)) {
        println!("skipping: no prikk older than 0.57.0 (and at the floor) found");
        return None;
    }
    Some(first)
}

/// Does the prikk behind `witnesses` have the verb `uncommit` (0.57.0 and later)? Asked of prikk without a patch id,
/// which takes no lock.
fn has_uncommit(witnesses: &Witnesses, wiki: &WikiId) -> bool {
    witnesses.has_uncommit(wiki).unwrap()
}

/// Asserts whether the prikk behind `witnesses` has the verb `uncommit` (a world made over [`prikk_057`] has it, one
/// made over [`prikk_floor`] has not: this says so where the subject depends on it).
pub fn assert_has_verb(witnesses: &Witnesses, wiki: &WikiId, expected: bool) {
    assert_eq!(has_uncommit(witnesses, wiki), expected);
}
