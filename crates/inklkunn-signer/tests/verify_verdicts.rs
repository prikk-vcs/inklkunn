//! `verify` under a memory ceiling, against a real prikk (RFC 001 follow-on, issue IS-7; ledger PK-22).
//!
//! Under `RLIMIT_DATA` prikk's `verify` sometimes does not abort: its `objects` stage fails with `i/o error:
//! out of memory`, every other stage finishes, and prikk prints a **complete** failing `verify-report-v1`
//! with exit 1. Only a history of some length shows it (a handful of blocks only ever abort), so the
//! repository is *generated here*, through the bootstrap signer (the read driver's crate may name no write
//! verb), and every ceiling of a sweep is run twice: raw, through the driver's own sandbox vector, to see
//! which ceilings produce the shape, and through the driver, whose answer must never be a failed verdict
//! for a repository that verifies unconfined. Self-skips with a visible line when prikk is not on `PATH`;
//! the confined policy runs when bubblewrap works.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::{
    Admission, CallLimits, Mounts, PrikkCli, Sandbox, Storage, StorageError, Unavailable,
};
use inklkunn_signer::{BootstrapSigner, KeyAuthority, RepositoryBootstrap};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

/// The measured shape's repository: 40 pages written in the first block, then 69 more blocks that each
/// rewrite one page. Pages of a few hundred bytes show nothing of the shape above 1 MiB; pages of about
/// 21 KiB make a store of about 3 MiB, and on prikk 0.55.0 the shape sits between 1.7 and 2.5 MiB (ledger
/// PK-22), so a 256 KiB sweep meets it three times.
fn generate(root: &Path) -> PathBuf {
    let keys = root.join("keys");
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    let signer = BootstrapSigner::new("prikk", Sandbox::Unconfined, &keys).expect("signer");
    signer
        .create_repository(&repo, &KeyAuthority::for_tests())
        .expect("repository");
    std::fs::create_dir_all(repo.join("pages")).expect("pages");
    let page = |i: usize, rev: usize| {
        format!(
            "# P{i}\n\n{}\nrev {rev}\n",
            format!("body {i} ").repeat(3000)
        )
    };
    for i in 0..40 {
        std::fs::write(repo.join(format!("pages/P{i}.md")), page(i, 0)).expect("page");
    }
    signer
        .commit_and_seal(&repo, "Seed 40 pages")
        .expect("seed");
    for rev in 1..70 {
        let i = rev % 40;
        std::fs::write(repo.join(format!("pages/P{i}.md")), page(i, rev)).expect("page");
        signer
            .commit_and_seal(&repo, &format!("Revise P{i}"))
            .expect("revision");
    }
    repo
}

/// One raw `verify` at `bytes`, through the driver's own sandbox vector: is its stdout a complete failing
/// report whose `objects` stage failed for lack of memory (the shape)?
fn raw_is_the_shape(sandbox: &Sandbox, repo: &Path, bytes: u64) -> bool {
    let run = sandbox
        .run_mounts(
            std::ffi::OsStr::new("prikk"),
            &Mounts::read(repo, None),
            &["verify", "--format", "json"],
            CallLimits {
                data_ceiling: Some(bytes),
                max_output: 8 << 20,
            },
            Some(Duration::from_secs(120)),
        )
        .expect("prikk runs");
    if run.status.code() != Some(1) {
        return false;
    }
    let Ok(report) = serde_json::from_slice::<serde_json::Value>(&run.stdout) else {
        return false;
    };
    report.pointer("/verdict/ok") == Some(&serde_json::Value::Bool(false))
        && report
            .pointer("/stages")
            .and_then(|s| s.as_array())
            .is_some_and(|stages| {
                stages.iter().any(|s| {
                    s.get("status").and_then(|v| v.as_str()) == Some("failed")
                        && s.get("message").and_then(|v| v.as_str())
                            == Some("i/o error: out of memory")
                })
            })
}

#[test]
fn no_ceiling_in_a_sweep_turns_a_memory_limit_into_a_failed_verification() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let tmp =
        TempDir(std::env::temp_dir().join(format!("inklkunn-verdicts-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&tmp.0);
    std::fs::create_dir_all(&tmp.0).expect("temp");
    let repo = generate(&tmp.0);

    let mut policies = vec![("unconfined", Sandbox::Unconfined)];
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        policies.push(("confined", Sandbox::default()));
    } else {
        println!("skipping the confined policy: bubblewrap cannot confine a process here");
    }
    for (name, sandbox) in policies {
        let unconfined = PrikkCli::open(
            "prikk",
            sandbox.clone(),
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
        )
        .expect("prikk at the floor");
        // The repository verifies when nothing limits it: whatever a ceiling does, it is not the repository.
        let whole = unconfined.verify(&repo).expect("verifies unconfined");
        assert!(whole.verdict.ok, "{name}: {whole:?}");

        let (mut shape, mut hit, mut verified) = (Vec::new(), 0, 0);
        // 1 MiB to 16 MiB in 256 KiB steps.
        for kib in (1024u64..=16 * 1024).step_by(256) {
            let bytes = kib << 10;
            if raw_is_the_shape(&sandbox, &repo, bytes) {
                shape.push(kib);
            }
            let d = PrikkCli::open(
                "prikk",
                sandbox.clone(),
                Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
            )
            .expect("prikk at the floor")
            .with_fixed_ceiling(bytes);
            match d.verify(&repo) {
                Ok(v) => {
                    assert!(
                        v.verdict.ok,
                        "{name}: a ceiling of {kib} KiB was shown as a failed verification: {v:?}"
                    );
                    verified += 1;
                }
                Err(StorageError::Unavailable {
                    verb: "verify",
                    reason: Unavailable::CeilingExceeded { bytes: b },
                }) => {
                    assert_eq!(b, bytes);
                    hit += 1;
                }
                other => {
                    panic!("{name}: {kib} KiB: neither a verification nor a ceiling hit: {other:?}")
                }
            }
        }
        println!(
            "{name}: ceilings that produced the shape (KiB): {shape:?}; ceiling hits {hit}, verified {verified}"
        );
        // The sweep must be able to see the defect: if a new prikk no longer prints the shape in this range,
        // the repository above needs re-measuring (ledger PK-22), not the test dropping the check.
        assert!(
            !shape.is_empty(),
            "{name}: no ceiling from 1 to 16 MiB produced the out-of-memory report; re-measure the generated repository"
        );
        assert!(hit >= shape.len(), "{name}: every shape is a ceiling hit");
    }
}
