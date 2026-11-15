//! Every question this test crate asks of the prikk on `PATH` (maintenance M39).
//!
//! A test reaches a prikk capability only through this module. What is public here either returns a binary
//! and prints the **guarded line** itself when it returns none (the with-prikk run in CI fails on
//! `skipping: prikk 0.57.0 or newer not found` and on `skipping: no prikk older than 0.57.0 (and at the floor) found`),
//! or asserts and returns nothing. What decides is private: no test of the crate can branch on it. The workspace check
//! `tools/floor-check/tests/guarded_skips.rs` reads this file: a probe is found by its definition here, a call of one
//! outside this module fails it, and a public function that can return none without printing fails it.
use std::path::PathBuf;
use std::time::Duration;

use inklkunn_prikk::{Admission, PrikkCli, Sandbox};

use super::World;

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
    let Some(first) = super::real_prikk() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    if inklkunn_prikk::probe_version(first.as_os_str()).is_none_or(|v| v >= (0, 57, 0)) {
        println!("skipping: no prikk older than 0.57.0 (and at the floor) found");
        return None;
    }
    Some(first)
}

impl World {
    /// Does this world's prikk have the verb `uncommit` (0.57.0 and later)? Asked of prikk without a patch id, which
    /// takes no lock.
    fn has_uncommit(&self) -> bool {
        self.recovery_witnesses()
            .has_uncommit(&self.wiki.id)
            .unwrap()
    }

    /// Asserts whether this world's prikk has the verb `uncommit` (a world made over [`prikk_057`] has it, one made
    /// over [`prikk_floor`] has not: this says so where the subject depends on it).
    pub fn assert_has_verb(&self, expected: bool) {
        assert_eq!(self.has_uncommit(), expected);
    }
}
