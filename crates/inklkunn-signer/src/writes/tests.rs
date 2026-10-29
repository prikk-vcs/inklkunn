#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn a_path_into_a_seed_directory_never_reaches_an_error() {
    let said = "error: author signing is required: no seed at /dev/shm/inklkunn/inklkunn-seeds-0123456789abcdef0123456789abcdef/author.seed. Create one with `prikk key generate --out /dev/shm/inklkunn/inklkunn-seeds-ffffffffffffffffffffffffffffffff/x.seed` first";
    let clean = scrub(said);
    assert!(
        !clean.contains("inklkunn-seeds-") && !clean.contains("/dev/shm"),
        "{clean}"
    );
    assert!(clean.contains("no seed at <seed directory>."), "{clean}");
    assert!(
        clean.contains("--out <seed directory>` first"),
        "the rest of the sentence stays: {clean}"
    );
    // Nothing to scrub, nothing changes; and a quoted path ends at the quote.
    assert_eq!(
        scrub("error: precondition not met"),
        "error: precondition not met"
    );
    assert_eq!(
        scrub("cannot read '/a/inklkunn-seeds-ab/x' now"),
        "cannot read '<seed directory>' now"
    );
    assert_eq!(scrub("inklkunn-seeds-ab"), "<seed directory>");
    assert_eq!(scrub(""), "");
}

#[test]
fn a_refused_write_carries_prikks_words_without_the_seed_path() {
    // Exercise `outcome` itself with a failed run whose stderr names a seed file.
    let run = Run {
        status: std::process::Command::new("false")
            .status()
            .expect("false runs"),
        stdout: Vec::new(),
        stderr: b"error: no seed at /run/inklkunn-seeds-00ff/author.seed\n".to_vec(),
        timed_out: false,
        output_exceeded: false,
    };
    let e = outcome("commit", run, 1 << 30).unwrap_err();
    let shown = e.to_string();
    assert!(
        !shown.contains("inklkunn-seeds-") && !shown.contains("/run/"),
        "{shown}"
    );
    assert!(shown.contains("<seed directory>"), "{shown}");
}

fn run_of(wait_status: i32, stderr: &str) -> Run {
    use std::os::unix::process::ExitStatusExt;
    Run {
        status: std::process::ExitStatus::from_raw(wait_status),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
        output_exceeded: false,
    }
}

#[test]
fn a_call_that_died_of_its_memory_ceiling_is_the_ceiling_and_never_a_refusal() {
    // The shapes the driver already knows (PK-9), through the one classifier: an allocator abort, a segfault,
    // a loader failure, bubblewrap running out of memory, a signal reported as 128 + n.
    for (what, run) in [
        (
            "an allocator abort",
            run_of(6, "memory allocation of 629145648 bytes failed"),
        ),
        ("a segfault", run_of(11, "")),
        ("a signal under bwrap", run_of(139 << 8, "")),
        (
            "a loader failure",
            run_of(
                127 << 8,
                "error while loading shared libraries: cannot allocate TLS",
            ),
        ),
        ("bwrap out of memory", run_of(1 << 8, "Out of memory")),
    ] {
        match outcome("verify", run, 1 << 30) {
            Err(SignerError::CeilingExceeded { verb, bytes }) => {
                assert_eq!((verb.as_str(), bytes), ("verify", 1 << 30), "{what}");
            }
            other => panic!("{what}: {other:?}"),
        }
    }
    // A refusal is still a refusal, and the same text is not mistaken for a ceiling when the run succeeded.
    assert!(matches!(
        outcome(
            "commit",
            run_of(1 << 8, "error: precondition not met: x"),
            1 << 30
        ),
        Err(SignerError::Refused(_))
    ));
    assert!(
        outcome(
            "commit",
            run_of(0, "memory allocation of 8 bytes failed"),
            1 << 30
        )
        .is_ok()
    );
    // Its words tell an operator it is neither refused nor failed.
    let shown = SignerError::CeilingExceeded {
        verb: "seal".into(),
        bytes: 1,
    }
    .to_string();
    assert!(
        shown.contains("memory ceiling") && shown.contains("not refused"),
        "{shown}"
    );
}

#[test]
fn only_the_whole_resource_message_of_a_failed_stage_is_a_ceiling_hit() {
    let report = |status: &str, message: &str| {
        serde_json::json!({
            "schema_version": "verify-report-v1",
            "verdict": {"ok": false, "failed_conditions": []},
            "stages": [
                {"stage": "objects", "status": status, "message": message},
                {"stage": "refs", "status": "evaluated"}
            ]
        })
        .to_string()
        .into_bytes()
    };
    // The measured shape, and Rust's text for a raw ENOMEM.
    assert!(resource_failure_in(&report(
        "failed",
        "i/o error: out of memory"
    )));
    assert!(resource_failure_in(&report(
        "failed",
        " i/o error: Cannot allocate memory (os error 12) "
    )));
    // Damage that merely mentions the words, a different status, no message, a different prefix, no report.
    for (what, bytes) in [
        (
            "a page named like the failure",
            report(
                "failed",
                "integrity error: pages/out of memory.md is damaged",
            ),
        ),
        (
            "the words inside another i/o error",
            report("failed", "i/o error: out of memory for pages/x.md"),
        ),
        (
            "a stage that was not run",
            report("not_evaluated", "i/o error: out of memory"),
        ),
        (
            "a different prefix",
            report("failed", "integrity error: out of memory"),
        ),
        ("not json", b"error: out of memory".to_vec()),
        ("an empty report", b"{}".to_vec()),
    ] {
        assert!(!resource_failure_in(&bytes), "{what}");
    }
}
