//! A sweep of the scratch directory from another PID namespace (handoff 005 corrections, sixth round, K55; the review's
//! X-a, proved there the same way).
//!
//! The image runs `inklkunn` as PID 1 of its container, and so does the natural `docker run --rm -v <same volume> ...
//! wiki import-folder`: two processes that share a scratch directory and see each other's pids as different processes
//! (or as none). An extract is live while its `flock` is held, and a lock holds across PID namespaces, so the sweep of one
//! leaves the other's extract whole and removes a dead one. This test starts the sweep as PID 1 of a fresh namespace
//! (`unshare -Urpf --mount-proc`), which only a crate that may start a process can, and so lives here. Where such a
//! namespace cannot be made (a host that forbids unprivileged user namespaces) it prints `skipping: PID namespaces
//! unavailable` and returns.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use inklkunn_core::read::scratch::{Extract, sweep_leftovers};
use inklkunn_signer::flock::{Acquired, try_exclusive};

/// Set in the child: the scratch directory it sweeps.
const CHILD_SWEEPS: &str = "INKLKUNN_TEST_SWEEP_FROM_NAMESPACE";

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pages_in(dir: &Path) {
    std::fs::create_dir_all(dir.join("pages")).expect("pages");
    for n in 0..50 {
        std::fs::write(dir.join(format!("pages/P{n}.md")), "x").expect("page");
    }
}

fn count(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("pages")).map_or(0, Iterator::count)
}

/// The sweeper: runs as PID 1 of a PID namespace of its own, started by the test below. Run
/// by hand or by the harness without the variable, it does nothing.
#[test]
fn child_sweeps_from_its_own_pid_namespace() {
    let Some(scratch) = std::env::var_os(CHILD_SWEEPS) else {
        return;
    };
    assert_eq!(
        std::process::id(),
        1,
        "the sweep was meant to run as PID 1 of a namespace of its own"
    );
    sweep_leftovers(Path::new(&scratch));
}

#[test]
fn a_sweep_from_another_pid_namespace_leaves_a_live_extract_and_removes_a_dead_one() {
    let unshare = |args: &[&str]| {
        let mut command = Command::new("unshare");
        command.args(["-Urpf", "--mount-proc"]).args(args);
        command
    };
    // A namespace that cannot be made here (no `unshare`, user namespaces forbidden) is a host this cannot be shown on.
    if !unshare(&["true"])
        .output()
        .is_ok_and(|out| out.status.success())
    {
        println!("skipping: PID namespaces unavailable");
        return;
    }
    let tmp = Tmp(
        std::env::temp_dir().join(format!("inklkunn-extract-namespace-{}", std::process::id()))
    );
    let _ = std::fs::remove_dir_all(&tmp.0);
    let scratch = tmp.0.join("scratch");
    std::fs::create_dir_all(&scratch).expect("scratch");

    // A server's live extract: its lock is held here, in this namespace, by a process the other namespace cannot see (this
    // pid is not a pid there, or is another process's). The old rule read `/proc` and removed it.
    let live = Extract::begin(&scratch).expect("a live extract");
    pages_in(&live.root());
    // The other server's, and a dead one: a crashed process' directory and its lock file, nobody holding the lock. Its
    // name says it belonged to PID 1, which is what the sweeper itself is, so a rule that looks the pid up would call
    // it live.
    let dead = scratch.join("index-1-1-0");
    pages_in(&dead);
    drop(try_exclusive(&scratch.join("index-1-1-0.lock")).expect("their lock file"));

    let out = unshare(&[
        std::env::current_exe()
            .expect("this test")
            .to_str()
            .unwrap(),
        "--exact",
        "child_sweeps_from_its_own_pid_namespace",
        "--test-threads=1",
    ])
    .env(CHILD_SWEEPS, &scratch)
    .output()
    .expect("the sweeper starts");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && said.contains("1 passed"),
        "the sweeper did not run: {said} {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(count(&live.root()), 50, "a live extract is whole");
    assert!(
        matches!(
            try_exclusive(&scratch.join(format!("{}.lock", live.name()))),
            Ok(Acquired::Busy { .. })
        ),
        "and still held"
    );
    assert!(!dead.exists(), "a dead one, with a free lock, is removed");
    assert!(
        !scratch.join("index-1-1-0.lock").exists(),
        "and its lock file after it"
    );
}
