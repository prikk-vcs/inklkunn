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

use std::path::{Path, PathBuf};
use std::time::Duration;

use inklkunn_core::Witnesses;
use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage};
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

/// The `(major, minor, patch)` of the prikk behind `storage`; a test that asserts per version asks it through a probe
/// below and nowhere else (it is private: no test branches on it, a probe asserts).
fn version_of(storage: &PrikkCli) -> (u64, u64, u64) {
    inklkunn_prikk::parse_version(storage.version()).expect("prikk's version is read")
}

/// Asserts what `prikk status --format json` shows in the state a `seal` leaves that published its block and stopped
/// before it drained the queue, whose tip block is `tip` (maintenance M51, ledger PK-44): up to 0.57.0 it **refuses**
/// with an integrity error; from 0.58.0 it exits 0, lists the queue (the one patch) and says `queue.already_sealed`:
/// that block. Called wherever a test used to say that `status` refuses.
pub fn assert_status_shows_a_pending_cleanup(storage: &PrikkCli, repo: &Path, tip: &str) {
    let status = storage.status(repo);
    if version_of(storage) < (0, 58, 0) {
        let refused =
            status.expect_err("up to prikk 0.57.0 `status --format json` refuses in this state");
        assert!(refused.to_string().contains("integrity error"), "{refused}");
    } else {
        let status = status.expect("from prikk 0.58.0 `status --format json` reads this state");
        assert_eq!(status.queue.count, 1);
        assert_eq!(
            status.queue.already_sealed.map(|sealed| sealed.block_id),
            Some(tip.to_owned()),
            "the queue is already sealed in the tip"
        );
    }
}

/// Asserts how prikk words the finding `PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS` that `doctor` printed, on this prikk
/// (maintenance M52, ledger PK-46): a **warning** with "no repair is required; the next guarded active-WAL append will
/// replace stale metadata" up to 0.57.0, and from 0.58.0 **info** with "nothing to do: the next `prikk commit`
/// replaces it". The id is the same and recovery reads it alike (`Diagnostic::is_metadata_debris`).
pub fn assert_debris_is_worded_as_this_prikk_words_it(
    storage: &PrikkCli,
    debris: &inklkunn_signer::recover::text::Diagnostic,
) {
    use inklkunn_signer::recover::text::Severity;
    assert!(
        debris
            .message
            .contains("active WAL is empty but stale ref metadata remains"),
        "{debris:?}"
    );
    if version_of(storage) < (0, 58, 0) {
        assert_eq!(debris.severity, Severity::Warning);
        assert_eq!(
            debris.recommendation.as_deref(),
            Some(
                "no repair is required; the next guarded active-WAL append will replace stale metadata"
            )
        );
    } else {
        assert_eq!(debris.severity, Severity::Info);
        assert_eq!(
            debris.recommendation.as_deref(),
            Some("nothing to do: the next `prikk commit` replaces it")
        );
    }
}
