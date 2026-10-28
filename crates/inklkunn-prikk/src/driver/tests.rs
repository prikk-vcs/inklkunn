use super::*;

#[test]
fn versions_parse_and_compare_to_the_floor() {
    assert_eq!(parse_version("prikk 0.54.1"), Some((0, 54, 1)));
    assert_eq!(parse_version("prikk 1.2.3-rc1\n"), Some((1, 2, 3)));
    assert_eq!(parse_version("something else"), None);
    assert!(parse_version("prikk 0.53.9").is_some_and(|v| v < FLOOR));
    assert!(parse_version("prikk 0.54.0").is_some_and(|v| v >= FLOOR));
}

#[test]
fn a_report_with_another_schema_is_refused_before_its_shape_is_read() {
    let doctored = br#"{"schema_version": "branch-list-v9", "branches": []}"#;
    let r: Result<Views> = parse_report("branch", doctored, schema::BRANCH_LIST);
    assert!(
        matches!(r, Err(StorageError::UnsupportedSchema { found, pinned, .. }) if found == "branch-list-v9" && pinned == "branch-list-v1")
    );
    let not_json: Result<Views> = parse_report("branch", b"error", schema::BRANCH_LIST);
    assert!(matches!(not_json, Err(StorageError::Damaged { .. })));
}

#[test]
fn argument_shapes_are_checked() {
    assert!(is_ref_name("heads/main"));
    assert!(is_ref_name("tags/v1.0"));
    assert!(!is_ref_name("main"));
    assert!(!is_ref_name("heads/"));
    assert!(!is_ref_name("heads/../x"));
    assert!(!is_ref_name("heads/-x"));
    assert!(is_hex_id(&"a".repeat(64)));
    assert!(!is_hex_id("deadbeef"));
    assert!(!is_hex_id(&"A".repeat(64)));
    assert!(is_repo_path("pages/Home.md"));
    assert!(is_repo_path("pages/Guides/Getting Started.md"));
    for bad in [
        "",
        "/etc/passwd",
        "../x",
        "a/./b",
        "a//b",
        "-x",
        "a\nb",
        &"x".repeat(1025),
    ] {
        assert!(!is_repo_path(bad), "{bad:?}");
    }
}

fn failed_run(code: i32, stdout: &str, stderr: &str) -> Run {
    use std::os::unix::process::ExitStatusExt;
    Run {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
        output_exceeded: false,
    }
}

#[test]
fn a_refusal_report_carries_its_id_and_its_reason() {
    // The report prikk 0.54.1 prints when `extract` meets a history beyond 63 blocks whose checkpoint was
    // never recorded (ledger PK-24), copied from a measured run.
    let report = r#"{"schema_version": "refusal-v1", "command": "extract", "refusal": "no-checkpoint-in-reach", "reason": "not-recorded", "message": "precondition not met: no admitted snapshot within 63 blocks of this point"}"#;
    let run = failed_run(
        1,
        report,
        "error: precondition not met: no admitted snapshot within 63 blocks of this point",
    );
    let e = PrikkCli::failure(
        ReadVerb::Extract,
        &run,
        Some(1 << 30),
        Duration::from_secs(120),
    );
    match e {
        StorageError::Refused {
            verb: "extract",
            kind: Some(kind),
            reason: Some(reason),
            message,
        } => {
            assert_eq!(kind, "no-checkpoint-in-reach");
            assert_eq!(reason, "not-recorded");
            assert!(message.starts_with("precondition not met"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_refusal_without_a_report_or_without_a_reason_has_none() {
    // A refusal that prints no `refusal-v1` has stderr only.
    let plain = failed_run(1, "", "error: path pages/Nope.md not found");
    let e = PrikkCli::failure(ReadVerb::Cat, &plain, None, Duration::from_secs(30));
    assert!(
        matches!(
            &e,
            StorageError::Refused { kind: None, reason: None, message, .. } if message == "path pages/Nope.md not found"
        ),
        "{e:?}"
    );
    // A refusal id that carries `reason: null` (every id but `no-checkpoint-in-reach`).
    let report = r#"{"schema_version": "refusal-v1", "command": "extract", "refusal": "over-max-bytes", "reason": null, "message": "too big"}"#;
    let run = failed_run(1, report, "error: too big");
    let e = PrikkCli::failure(ReadVerb::Extract, &run, None, Duration::from_secs(120));
    assert!(
        matches!(
            &e,
            StorageError::Refused { kind: Some(k), reason: None, .. } if k == "over-max-bytes"
        ),
        "{e:?}"
    );
}
