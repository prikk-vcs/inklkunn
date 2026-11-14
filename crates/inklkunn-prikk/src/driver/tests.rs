#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use crate::storage::{Stage, StageOutcome};

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
        None,
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
    let e = PrikkCli::failure(ReadVerb::Cat, &plain, None, Duration::from_secs(30), None);
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
    let e = PrikkCli::failure(
        ReadVerb::Extract,
        &run,
        None,
        Duration::from_secs(120),
        None,
    );
    assert!(
        matches!(
            &e,
            StorageError::Refused { kind: Some(k), reason: None, .. } if k == "over-max-bytes"
        ),
        "{e:?}"
    );
}

/// A finished `verify` run with the given exit code and stdout.
fn verify_run(code: i32, stdout: &str) -> Run {
    use std::os::unix::process::ExitStatusExt;
    Run {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out: false,
        output_exceeded: false,
    }
}

/// A failing `verify-report-v1` over the given stages (JSON objects, comma-separated).
fn failing(stages: &str) -> String {
    format!(
        r#"{{"schema_version":"verify-report-v1","verdict":{{"ok":false,"failed_conditions":[{{"id":"stage-failure","message":"repository verification did not complete every stage; see stage outcomes above"}}]}},"stages":[{stages}]}}"#
    )
}

const OOM: &str = r#"{"stage":"objects","status":"failed","message":"i/o error: out of memory"}"#;
const DONE: &str = r#"{"stage":"refs","status":"evaluated"}"#;
const DAMAGED: &str = r#"{"stage":"refs","status":"failed","message":"integrity error: ref pointer index has a damaged entry"}"#;

/// What `verdict_of` makes of a failing report with `stages`, with the ceiling of a production call and
/// without one: `Verdict` when it is a verdict about the repository, `Ceiling` when it is the memory limit
/// (the damage reading when no ceiling was set).
#[derive(Debug, PartialEq)]
enum Read {
    Verdict,
    Ceiling,
    Damaged,
    Other,
}

fn read(stages: &str, ceiling: Option<u64>) -> Read {
    match PrikkCli::verdict_of(&verify_run(1, &failing(stages)), ceiling) {
        Some(Ok(v)) if !v.verdict.ok => Read::Verdict,
        Some(Err(StorageError::Unavailable {
            verb: "verify",
            reason: Unavailable::CeilingExceeded { bytes },
        })) if Some(bytes) == ceiling => Read::Ceiling,
        Some(Err(StorageError::Damaged { verb: "verify", .. })) => Read::Damaged,
        _ => Read::Other,
    }
}

#[test]
fn a_failing_report_is_a_verdict_only_when_every_stage_ran_to_an_outcome() {
    let blocked = |by: &str| {
        format!(
            r#"{DAMAGED},{{"stage":"ref-update-schema-trust","status":"not_evaluated","blocked_by":"{by}"}}"#
        )
    };
    let verdicts = [
        // Genuine damage as measured on 0.54.1 / 0.55.0: failed with findings, the rest blocked behind one.
        ("damage with its blocked stages", blocked("refs")),
        ("a finding beside evaluated stages", format!("{DONE},{DAMAGED}")),
        // A message may name a thing the repository's contributors chose: never a substring match.
        (
            "a planted name",
            r#"{"stage":"objects","status":"failed","message":"integrity error: page \"out of memory\" has a damaged entry"}"#.to_owned(),
        ),
        (
            "the words after the prefix, not whole",
            r#"{"stage":"objects","status":"failed","message":"i/o error: out of memory while reading page X"}"#.to_owned(),
        ),
        (
            "the words behind another prefix",
            r#"{"stage":"objects","status":"failed","message":"integrity error: i/o error: out of memory"}"#.to_owned(),
        ),
        (
            "the words without the prefix",
            r#"{"stage":"objects","status":"failed","message":"out of memory"}"#.to_owned(),
        ),
        (
            "a failure that gives no message is not a resource failure",
            r#"{"stage":"objects","status":"failed"}"#.to_owned(),
        ),
    ];
    let partials = [
        // prikk walks every stage, so an empty list is not a report it prints (review V2).
        ("no stages", String::new()),
        ("the measured out-of-memory shape", format!("{OOM},{DONE}")),
        (
            "raw ENOMEM, whole",
            r#"{"stage":"objects","status":"failed","message":"i/o error: Cannot allocate memory (os error 12)"}"#.to_owned(),
        ),
        (
            "out of memory with padding after the prefix",
            r#"{"stage":"objects","status":"failed","message":"i/o error:   out of memory  "}"#.to_owned(),
        ),
        ("blocked by a stage missing from the report", blocked("no-such-stage").replace(DAMAGED, DONE)),
        ("blocked by a stage the report does not list, beside damage", blocked("nowhere")),
        (
            "blocked by a not_evaluated stage",
            format!(
                r#"{DAMAGED},{{"stage":"a","status":"not_evaluated","blocked_by":"refs"}},{{"stage":"b","status":"not_evaluated","blocked_by":"a"}}"#
            ),
        ),
        (
            "blocked by a stage that ran out of memory",
            format!(
                r#"{OOM},{{"stage":"c","status":"not_evaluated","blocked_by":"objects"}}"#
            ),
        ),
        (
            "not_evaluated with no blocker named",
            format!(r#"{DAMAGED},{{"stage":"c","status":"not_evaluated"}}"#),
        ),
        ("a halted stage", format!(r#"{DONE},{{"stage":"c","status":"halted"}}"#)),
        (
            "a status the driver does not know",
            format!(r#"{DAMAGED},{{"stage":"c","status":"skipped"}}"#),
        ),
        (
            "an out-of-memory stage beside a finding",
            format!("{OOM},{DAMAGED}"),
        ),
    ];
    for (what, stages) in &verdicts {
        for ceiling in [Some(4 << 20), None] {
            assert_eq!(
                read(stages, ceiling),
                Read::Verdict,
                "{what} (ceiling {ceiling:?})"
            );
        }
    }
    for (what, stages) in &partials {
        assert_eq!(
            read(stages, Some(4 << 20)),
            Read::Ceiling,
            "{what}, with a ceiling"
        );
        assert_eq!(read(stages, None), Read::Damaged, "{what}, without one");
    }
}

#[test]
fn only_an_exit_one_failing_report_is_considered_and_everything_else_is_read_as_a_failure() {
    let report = failing(DAMAGED);
    let good = r#"{"schema_version":"verify-report-v1","verdict":{"ok":true},"stages":[]}"#;
    // Exit 0: the report, whatever it says.
    assert!(
        matches!(PrikkCli::verdict_of(&verify_run(0, good), Some(1)), Some(Ok(v)) if v.verdict.ok)
    );
    // A death by signal (bwrap's 128 + n) or an abort is never a verdict, even with a report on stdout.
    for code in [2, 127, 134, 139] {
        assert!(
            PrikkCli::verdict_of(&verify_run(code, &report), Some(1)).is_none(),
            "exit {code}"
        );
    }
    // Exit 1 with a good verdict, with output that is not a report, and with a doctored schema.
    assert!(PrikkCli::verdict_of(&verify_run(1, good), Some(1)).is_none());
    assert!(PrikkCli::verdict_of(&verify_run(1, "error: nope"), Some(1)).is_none());
    assert!(
        PrikkCli::verdict_of(
            &verify_run(1, &report.replace("verify-report-v1", "verify-report-v9")),
            Some(1)
        )
        .is_none()
    );
    // A kill or an output bound outranks the report.
    for (timed_out, output_exceeded) in [(true, false), (false, true)] {
        let mut run = verify_run(1, &report);
        run.timed_out = timed_out;
        run.output_exceeded = output_exceeded;
        assert!(PrikkCli::verdict_of(&run, Some(1)).is_none());
    }
}

#[test]
fn a_stage_is_modelled_as_prikk_reports_it_and_an_unknown_status_is_never_a_good_one() {
    let stage = |status: &str| Stage {
        stage: "s".to_owned(),
        status: status.to_owned(),
        message: Some("m".to_owned()),
        blocked_by: Some("b".to_owned()),
    };
    assert_eq!(stage("evaluated").outcome(), StageOutcome::Evaluated);
    assert_eq!(
        stage("failed").outcome(),
        StageOutcome::Failed { message: Some("m") }
    );
    assert_eq!(
        stage("not_evaluated").outcome(),
        StageOutcome::NotEvaluated {
            blocked_by: Some("b")
        }
    );
    assert_eq!(stage("halted").outcome(), StageOutcome::Halted);
    for other in ["", "Evaluated", "skipped", "passed", "ok"] {
        assert_eq!(stage(other).outcome(), StageOutcome::Unknown(other));
    }
    let v: Verification = serde_json::from_str(&failing(&format!("{OOM},{DONE}"))).unwrap();
    assert_eq!(v.stage_summary(), "objects failed");
    let v: Verification = serde_json::from_str(&failing(&format!(
        r#"{DAMAGED},{{"stage":"x","status":"not_evaluated","blocked_by":"refs"}}"#
    )))
    .unwrap();
    assert_eq!(
        v.stage_summary(),
        "refs failed, x not_evaluated (behind refs)"
    );
}

/// A `verify-report-v1` with the given verdict (`ok` and the ids of its failed conditions) over the given stages.
fn report_with(ok: bool, conditions: &[&str], stages: &str) -> Verification {
    report_with_failed_objects(ok, conditions, stages, Some("[]"))
}

/// [`report_with`] with the report's `failed_objects` as given (`None`: the field is not in the report).
fn report_with_failed_objects(
    ok: bool,
    conditions: &[&str],
    stages: &str,
    failed_objects: Option<&str>,
) -> Verification {
    let failed_objects =
        failed_objects.map_or(String::new(), |f| format!(r#","failed_objects":{f}"#));
    let conditions: Vec<String> = conditions
        .iter()
        .map(|id| format!(r#"{{"id":"{id}","message":"x"}}"#))
        .collect();
    let text = format!(
        r#"{{"schema_version":"verify-report-v1","verdict":{{"ok":{ok},"failed_conditions":[{}]}},"stages":[{stages}]{failed_objects}}}"#,
        conditions.join(",")
    );
    parse_report("verify", text.as_bytes(), "verify-report-v1").expect("a report")
}

#[test]
fn the_list_of_block_seals_is_whole_only_where_the_report_shows_nothing_left_out() {
    // Measured on the published 0.57.0 (m3-measurements, section 8): a repository whose verification fails on
    // publication trust alone (a retired key) lists every block; one with a block or ref container that cannot be
    // read lists fewer, or none, while every stage is `evaluated`; a failed stage and the stages behind it also
    // leave it short. The stricter reading is asked of the list only: the verdict's `is_partial` is not it.
    // Every stage evaluated, the `objects` stage among them.
    const ALL: &str =
        r#"{"stage":"objects","status":"evaluated"},{"stage":"refs","status":"evaluated"}"#;
    const NOT_RUN: &str = r#"{"stage":"b","status":"not_evaluated","blocked_by":"refs"}"#;
    let whole = [
        ("a repository that verifies", report_with(true, &[], ALL)),
        (
            "a retired key (publication trust alone)",
            report_with(false, &["publication-trust"], ALL),
        ),
    ];
    for (what, report) in &whole {
        assert!(report.block_seals_are_whole(), "{what}");
    }
    let short = [
        (
            "a container that cannot be read, every stage evaluated",
            report_with(false, &["item-failure"], ALL),
        ),
        (
            "publication trust beside an unreadable container",
            report_with(false, &["publication-trust", "item-failure"], ALL),
        ),
        (
            "a blocked ref publication",
            report_with(false, &["blocking-ref-publication"], ALL),
        ),
        (
            "a failed stage",
            report_with(false, &["stage-failure"], DAMAGED),
        ),
        (
            "a stage not evaluated behind a failed one (a finding: not partial for the verdict)",
            report_with(false, &["stage-failure"], &format!("{DAMAGED},{NOT_RUN}")),
        ),
        (
            "a failed stage the verdict does not name (a report that contradicts itself)",
            report_with(false, &["publication-trust"], DAMAGED),
        ),
        (
            "a stage not evaluated beside a retired key",
            report_with(false, &["publication-trust"], &format!("{ALL},{NOT_RUN}")),
        ),
        (
            "a condition nobody knows",
            report_with(false, &["a-newer-condition"], ALL),
        ),
        (
            "a failing verdict that names no condition",
            report_with(false, &[], ALL),
        ),
        ("no stages", report_with(true, &[], "")),
        // PK-39 (prikk's reply of 2026-11-14): a damaged Block is missing from the list and named in `failed_objects`.
        (
            "a damaged Block named in failed_objects, every stage evaluated, the verdict failing on publication trust alone",
            report_with_failed_objects(
                false,
                &["publication-trust"],
                ALL,
                Some(r#"[{"object_type":"block","class":"corrupt"}]"#),
            ),
        ),
        (
            "a damaged Block named in failed_objects beside a verdict that holds (a report that contradicts itself)",
            report_with_failed_objects(
                true,
                &[],
                ALL,
                Some(r#"[{"object_type":"block","class":"corrupt"}]"#),
            ),
        ),
        (
            "a report without failed_objects",
            report_with_failed_objects(true, &[], ALL, None),
        ),
        (
            "failed_objects that is not a list",
            report_with_failed_objects(true, &[], ALL, Some("null")),
        ),
        (
            "every stage evaluated but no objects stage among them",
            report_with(true, &[], r#"{"stage":"refs","status":"evaluated"}"#),
        ),
    ];
    for (what, report) in &short {
        assert!(!report.block_seals_are_whole(), "{what}");
    }
    // The verdict's own reading is not changed: the blocked stage behind a finding is not partial.
    assert!(!report_with(false, &["stage-failure"], &format!("{DAMAGED},{NOT_RUN}")).is_partial());
}

/// `prikk --version` as the driver reads it.
const V055: Option<(u64, u64, u64)> = Some((0, 55, 0));
const V056: Option<(u64, u64, u64)> = Some((0, 56, 0));
const V057: Option<(u64, u64, u64)> = Some((0, 57, 0));

fn format_failure(stderr: &str, prikk: Option<(u64, u64, u64)>) -> StorageError {
    PrikkCli::failure(
        ReadVerb::Tree,
        &failed_run(1, "", stderr),
        None,
        Duration::from_secs(120),
        prikk,
    )
}

#[test]
fn prikks_unsupported_format_sentence_is_a_mismatch_only_where_the_running_prikk_is_known_not_to_read_it()
 {
    // Measured: `error: unsupported format version: 8` on stderr, exit 1, no report, from prikk 0.55.0 and 0.56.0 on a
    // repository made by 0.57.0 (format 8), for every command that opens the repository: an operator's mismatch.
    for prikk in [V055, V056] {
        let e = format_failure("error: unsupported format version: 8\n", prikk);
        assert!(
            matches!(
                e,
                StorageError::UnsupportedFormat {
                    verb: "tree",
                    version: 8
                }
            ),
            "{prikk:?}: {e:?}"
        );
        let said = e.to_string();
        assert!(said.contains("format 8"), "{said}");
        assert!(
            said.contains("0.57.0"),
            "the first prikk that reads it: {said}"
        );
        assert!(!said.contains("damaged"), "{said}");
    }
    // The same words from a prikk that reads format 8 (measured on 0.57.0 with `.prikk/FORMAT` holding `8` without its
    // newline), from one whose version is not known, from a newer one: a damaged or unknown marker, never a mismatch.
    for (what, prikk) in [
        ("0.57.0", V057),
        ("0.58.0", Some((0, 58, 0))),
        ("a version nobody read", None),
    ] {
        let e = format_failure("error: unsupported format version: 8\n", prikk);
        assert!(
            matches!(
                e,
                StorageError::FormatMarker {
                    verb: "tree",
                    version: 8
                }
            ),
            "{what}: {e:?}"
        );
        let said = e.to_string();
        assert!(said.contains(".prikk/FORMAT"), "{said}");
        assert!(
            !said.contains("nothing in the repository is damaged"),
            "{said}"
        );
    }
    // A number the table knows nothing about is a marker this build cannot judge, on any prikk, and the sentence names
    // the number prikk gave, never a fixed 8.
    for (what, prikk) in [("0.57.0", V057), ("0.55.0", V055)] {
        let e = format_failure("error: unsupported format version: 9\n", prikk);
        assert!(
            matches!(
                e,
                StorageError::FormatMarker {
                    verb: "tree",
                    version: 9
                }
            ),
            "{what}: {e:?}"
        );
        let said = e.to_string();
        assert!(said.contains("format 9"), "{said}");
        assert!(!said.contains('8'), "{said}");
    }
    // Whatever else it says is a refusal or damage as before: a name the repository's contributors chose, a longer
    // sentence, a number that is not one (or has a leading zero), the usage exit, a report that names a refusal.
    let others = [
        (
            1,
            "",
            "error: page \"unsupported format version: 8\" not found",
        ),
        (1, "", "error: unsupported format version: 8 and more"),
        (1, "", "error: not an unsupported format version: 8"),
        (1, "", "error: unsupported format version: eight"),
        (1, "", "error: unsupported format version: "),
        (1, "", "error: unsupported format version: 08"),
        (1, "", "error: unsupported format version: 00"),
        (
            1,
            "",
            "error: unsupported format version: 99999999999999999999",
        ),
        (1, "", "error: unsupported format version: -1"),
        (2, "", "error: unsupported format version: 8"),
        (
            1,
            r#"{"schema_version":"refusal-v1","refusal":"precondition-not-met","reason":null,"message":"unsupported format version: 8"}"#,
            "error: unsupported format version: 8",
        ),
    ];
    for prikk in [V055, V057] {
        for (code, stdout, stderr) in others {
            let e = PrikkCli::failure(
                ReadVerb::Tree,
                &failed_run(code, stdout, stderr),
                None,
                Duration::from_secs(120),
                prikk,
            );
            assert!(
                !matches!(
                    e,
                    StorageError::UnsupportedFormat { .. } | StorageError::FormatMarker { .. }
                ),
                "{stderr}: {e:?}"
            );
        }
    }
    // And the parser itself: with and without the prefix, padded, 9 digits at most.
    assert_eq!(
        crate::unsupported_format_version("unsupported format version: 9"),
        Some(9)
    );
    assert_eq!(
        crate::unsupported_format_version("  error:   unsupported format version: 123456789\n"),
        Some(123_456_789)
    );
    assert_eq!(
        crate::unsupported_format_version("unsupported format version: 1234567890"),
        None
    );
    // `0` is a number; `08` and `007` are not what prikk writes.
    assert_eq!(
        crate::unsupported_format_version("unsupported format version: 0"),
        Some(0)
    );
    for padded in ["08", "007", "00"] {
        assert_eq!(
            crate::unsupported_format_version(&format!("unsupported format version: {padded}")),
            None,
            "{padded}"
        );
    }
}
