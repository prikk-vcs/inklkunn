//! Every question these test crates ask of the prikk on `PATH` (maintenance M39).
//!
//! `acceptance_kill.rs`, `format_marker.rs` and `recover.rs` mount this file; a test reaches a prikk capability only
//! through it. What is public here returns a binary and prints the **guarded line** itself when it returns none (the
//! with-prikk run in CI fails on every skip line the guard names, `WITH_PRIKK_GUARDED` in `tools/floor-check`, among them
//! `skipping: prikk 0.57.0 or newer not found`, `skipping: no prikk older than 0.57.0 (and at the floor) found` and
//! `skipping: prikk not on PATH`). What decides is private: no test can branch on it. The workspace check
//! `tools/floor-check/tests/guarded_skips.rs` reads this file: a probe is found by its definition here, a call of one
//! outside this module fails it, and a public function that can return none without printing fails it.
// Each test crate uses some of these.
#![allow(dead_code)]

use std::path::PathBuf;

type Version = (u64, u64, u64);

/// The first `prikk` on `PATH` (in any directory) whose version satisfies `wanted`.
fn prikk_where(wanted: impl Fn(Version) -> bool) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("prikk"))
        .filter(|c| c.is_file())
        .find(|c| inklkunn_prikk::probe_version(c.as_os_str()).is_some_and(&wanted))
}

/// The first `prikk` on `PATH`, in whatever version.
fn first_prikk_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("prikk"))
        .find(|c| c.is_file())
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

/// The first prikk 0.57.0 or newer on `PATH`, wherever it stands (maintenance M28). With none it prints the line CI
/// fails on.
pub fn prikk_057() -> Option<PathBuf> {
    let Some(found) = prikk_where(|v| v >= (0, 57, 0)) else {
        println!("skipping: prikk 0.57.0 or newer not found");
        return None;
    };
    Some(found)
}

/// The first prikk on `PATH`, in any directory, that is older than 0.57.0 and at the floor (0.54.0): for a test that
/// needs both sides on one machine. With none it prints the line CI fails on.
pub fn prikk_older_than_057() -> Option<PathBuf> {
    let Some(found) = prikk_where(|v| ((0, 54, 0)..(0, 57, 0)).contains(&v)) else {
        println!("skipping: no prikk older than 0.57.0 (and at the floor) found");
        return None;
    };
    Some(found)
}

/// Asserts what `prikk status --format json` (the binary `prikk`, run in `repo`) shows in the state a `seal` leaves that
/// published its block `tip` and stopped before it drained the queue (maintenance M51, ledger PK-44): up to 0.57.0 it
/// exits 1 with an integrity error; from 0.58.0 it exits 0, lists the one queued patch and says `queue.already_sealed`:
/// `tip`. Called wherever a test used to say that `status` refuses.
pub fn assert_status_json_shows_a_pending_cleanup(
    prikk: &std::path::Path,
    repo: &std::path::Path,
    tip: &str,
) {
    let version =
        inklkunn_prikk::probe_version(prikk.as_os_str()).expect("prikk's version is read");
    let out = std::process::Command::new(prikk)
        .args(["status", "--format", "json"])
        .current_dir(repo)
        .output()
        .expect("prikk runs");
    if version < (0, 58, 0) {
        assert!(
            !out.status.success(),
            "up to prikk 0.57.0 `status --format json` refuses in this state"
        );
    } else {
        assert!(out.status.success(), "{out:?}");
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a JSON report");
        assert_eq!(
            json.pointer("/queue/already_sealed/block_id")
                .and_then(serde_json::Value::as_str),
            Some(tip)
        );
        assert_eq!(
            json.pointer("/queue/count")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
    }
}

/// The **first** prikk on `PATH`, when it is older than 0.57.0 (CI's is the floor): the tests of the answer *no verb*.
/// A first prikk that has the verb prints the line CI fails on (the floor must be first), and the subject runs on the
/// 0.57.0 side through [`prikk_057`] instead.
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
