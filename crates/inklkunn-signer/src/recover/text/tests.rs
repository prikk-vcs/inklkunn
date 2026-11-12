//! The readers of `prikk doctor` and `prikk unlock`: output captured from prikk 0.55.0, and every way the shape can be
//! wrong. Recovery clears locks and discards a commit on what these say, so they refuse what they do not know.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

/// The `i`-th of a list that must have it (tests may not index).
fn nth<T>(items: &[T], i: usize) -> &T {
    items.get(i).expect("an element")
}

const DOCTOR_OK: &str = "\
doctor repository: /srv/r/.prikk
verified repository: /srv/r/.prikk
active sessions: 1 total, 1 covered by sealed-history verification (default only, by construction -- see `prikk doctor` for the rest)
verification stages: 17
stage objects: evaluated
object items: 352 scanned, 0 failed
interrupted appends: 0
trailing partial WAL bytes: 0
info [PRIKK-DOCTOR-VERIFY-OK]: repository structural verification scan completed
  recommendation: review the remaining diagnostics before deciding whether action is required
issue summary: errors=0, warnings=0, info=1
";

/// The report of a queued commit whose WAL was cut short (measured on prikk 0.55.0): an error, and beside it a
/// warning that asks for nothing, and two lines of the form `error: …` that are not diagnostics.
const DOCTOR_DAMAGED: &str = "\
doctor repository: /srv/r/.prikk
verified repository: /srv/r/.prikk
active WAL metadata: stale local metadata for empty WAL (heads/main)
warning: active WAL ref metadata exists but the active WAL is empty
acknowledged commits: sequence 1 is damaged
error: a queued commit you were told had succeeded (sequence 1) is damaged; it was already acknowledged, so it cannot be removed as a crash leftover -- run `prikk doctor --discard-damaged-commits` to remove it instead
warning [PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS]: active WAL is empty but stale ref metadata remains for heads/main
  recommendation: no repair is required; the next guarded active-WAL append will replace stale metadata
error [PRIKK-DOCTOR-COMMIT-WITNESS-ACKNOWLEDGED-DAMAGE]: a queued commit you were told had succeeded (sequence 1) is damaged
  recommendation: run `prikk doctor --discard-damaged-commits` to remove it; it was already acknowledged, so it cannot be removed as a crash leftover
error: doctor found repository health errors
issue summary: errors=1, warnings=1, info=0
";

const UNLOCK_TWO: &str = "\
/srv/r/.prikk/active/default/active.lock
  kind: active
  recorded pid: 408657
  liveness: does not appear to be running (not proof it is safe to clear)
/srv/r/.prikk/containers/objects.lock
  kind: container:object-store
  recorded pid: 2
  liveness: appears running -- do not clear

liveness is advisory only: a positive result is reliable evidence the process is still running, but a negative or unknown result is NOT proof it is safe to clear -- PID reuse and container namespace isolation can both make a genuinely running process appear absent. Clear a lock only if you have independently confirmed the process that created it is gone.
";

#[test]
fn a_clean_doctor_report_is_read() {
    let r = parse_doctor_output(DOCTOR_OK).unwrap();
    assert_eq!((r.errors, r.warnings, r.info), (0, 0, 1));
    assert!(r.is_clean_for_recovery());
    assert!(!r.has_acknowledged_damage());
    assert!(r.others().is_empty());
    assert_eq!(r.diagnostics.len(), 1);
    assert_eq!(nth(&r.diagnostics, 0).code, "PRIKK-DOCTOR-VERIFY-OK");
    assert_eq!(nth(&r.diagnostics, 0).severity, Severity::Info);
}

#[test]
fn an_acknowledged_damage_is_read_and_its_companion_warning_does_not_hide_other_trouble() {
    let r = parse_doctor_output(DOCTOR_DAMAGED).unwrap();
    assert_eq!((r.errors, r.warnings, r.info), (1, 1, 0));
    assert!(!r.is_clean_for_recovery());
    assert!(r.has_acknowledged_damage());
    assert!(
        r.others().is_empty(),
        "the damage and the debris beside it are the discard's: {:?}",
        r.others()
    );
    let damage = r.diagnostics.iter().find(|d| d.is_acknowledged_damage());
    assert_eq!(
        damage.map(|d| d.named_flags()),
        Some(vec!["--discard-damaged-commits".to_owned()])
    );
    // The same debris warning with no damage beside it is not waved through.
    let alone = "doctor repository: /x\nwarning [PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS]: x\nissue summary: errors=0, warnings=1, info=0\n";
    let alone = parse_doctor_output(alone).unwrap();
    assert_eq!(alone.others().len(), 1);
    // Another error beside the damage is still in the way.
    let more = DOCTOR_DAMAGED.replace(
        "issue summary: errors=1, warnings=1, info=0",
        "error [PRIKK-DOCTOR-OBJECT-MISSING]: an object is missing\nissue summary: errors=2, warnings=1, info=0",
    );
    let more = parse_doctor_output(&more).unwrap();
    assert!(more.has_acknowledged_damage());
    assert_eq!(more.others().len(), 1);
    assert_eq!(nth(&more.others(), 0).code, "PRIKK-DOCTOR-OBJECT-MISSING");
    // A lost acknowledged commit is the other thing the discard answers; a warning of the damage's code is not.
    let lost = "doctor repository: /x\nerror [PRIKK-DOCTOR-COMMIT-WITNESS-ACKNOWLEDGED-LOSS]: gone\nissue summary: errors=1, warnings=0, info=0\n";
    assert!(parse_doctor_output(lost).unwrap().has_acknowledged_damage());
    let weak = "doctor repository: /x\nwarning [PRIKK-DOCTOR-COMMIT-WITNESS-ACKNOWLEDGED-DAMAGE]: x\nissue summary: errors=0, warnings=1, info=0\n";
    assert!(!parse_doctor_output(weak).unwrap().has_acknowledged_damage());
}

#[test]
fn an_unreached_object_warning_is_a_note_that_does_not_block_and_nothing_else_is() {
    let with = |code: &str, severity: &str, rec: &str| {
        let counts = match severity {
            "warning" => "errors=0, warnings=1, info=0",
            _ => "errors=1, warnings=0, info=0",
        };
        parse_doctor_output(&format!(
            "doctor repository: /x\n{severity} [{code}]: 3 objects nothing refers to\n{rec}issue summary: {counts}\n"
        ))
        .unwrap()
    };
    let note = with(
        UNREACHED_OBJECT_CODE,
        "warning",
        "  recommendation: the bytes stay; no command removes them yet\n",
    );
    assert!(nth(&note.diagnostics, 0).is_unreached_note());
    assert_eq!(note.unreached_notes(), 1);
    assert!(note.is_clean_for_recovery());
    assert!(note.others().is_empty());
    // An error of that code is damage; a warning that names a repair is not a note; another code is not.
    assert!(!with(UNREACHED_OBJECT_CODE, "error", "").is_clean_for_recovery());
    assert!(
        !with(
            UNREACHED_OBJECT_CODE,
            "warning",
            "  recommendation: run `prikk doctor --repair-index`\n"
        )
        .is_clean_for_recovery()
    );
    assert!(!with("PRIKK-DOCTOR-SOMETHING-NEW", "warning", "").is_clean_for_recovery());
}

#[test]
fn a_doctor_report_of_an_unknown_shape_is_refused() {
    for bad in [
        "",
        "something else entirely\n",
        // the diagnostics do not add up to the summary
        "doctor repository: /x\ninfo [A]: a\nissue summary: errors=0, warnings=0, info=2\n",
        "doctor repository: /x\nissue summary: errors=0, warnings=0, info=1\n",
        // no summary
        "doctor repository: /x\ninfo [A]: a\n",
        // a summary that names something else, or a count that is not a number
        "doctor repository: /x\nissue summary: errors=0, warnings=0, notes=1\n",
        "doctor repository: /x\nissue summary: errors=x, warnings=0, info=0\n",
        // a recommendation with nothing before it
        "doctor repository: /x\n  recommendation: orphan\nissue summary: errors=0, warnings=0, info=0\n",
        // a diagnostic without its code's closing bracket
        "doctor repository: /x\nerror [A: broken\nissue summary: errors=1, warnings=0, info=0\n",
    ] {
        assert!(parse_doctor_output(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_unlock_listing_is_read_with_its_locks_and_its_advisory_kept_but_not_used() {
    let locks = parse_unlock_output(UNLOCK_TWO).unwrap();
    assert_eq!(locks.len(), 2);
    assert_eq!(
        nth(&locks, 0).path,
        "/srv/r/.prikk/active/default/active.lock"
    );
    assert_eq!(nth(&locks, 0).kind, "active");
    assert_eq!(nth(&locks, 0).recorded_pid, 408_657);
    assert!(
        nth(&locks, 0)
            .liveness
            .starts_with("does not appear to be running")
    );
    // The second says a process "appears running": read, kept, and nothing here changes with it (ledger PL-8).
    assert_eq!(nth(&locks, 1).kind, "container:object-store");
    assert_eq!(nth(&locks, 1).recorded_pid, 2);
    assert!(nth(&locks, 1).liveness.starts_with("appears running"));
    assert!(
        parse_unlock_output("no locks currently held\n")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_unlock_listing_of_an_unknown_shape_is_refused() {
    for bad in [
        "",
        "   \n",
        "garbage\n",
        "/a/b.lock\n",
        "/a/b.lock\n  kind: active\n",
        "/a/b.lock\n  kind: active\n  recorded pid: nope\n  liveness: x\n",
        "/a/b.lock\n  pid: 1\n  kind: active\n  liveness: x\n",
        "/a/b.lock\n  kind: active\n  recorded pid: 1\n  liveness: x\nno locks currently held\n",
        "/path with space.lock\n  kind: active\n  recorded pid: 1\n  liveness: x\n",
        "no locks currently held\nsomething new a later prikk prints\n",
        "/a/b.lock\n  kind: active\n  recorded pid: 1\n  liveness: x\n\nliveness is advisory only: x\nand then more\n",
    ] {
        assert!(parse_unlock_output(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_acceptance_lock_kinds_are_what_commit_and_seal_take() {
    for kind in [
        "active",
        "ref",
        "container:object-store",
        "container:ref-pointer-index",
        "container:ref-log",
    ] {
        assert!(ACCEPTANCE_LOCK_KINDS.contains(&kind), "{kind}");
    }
    // The trust policy's lock, the received index's, and one nobody has seen are not an acceptance's.
    for kind in [
        "container:trust-policy",
        "container:received-index",
        "container:something-new",
        "",
    ] {
        assert!(!ACCEPTANCE_LOCK_KINDS.contains(&kind), "{kind}");
    }
}

#[test]
fn what_a_discard_said_is_read_only_when_it_said_it_was_done() {
    let said = "\
doctor repository: /srv/r/.prikk
acknowledged commit at sequence 1 (patch dec019ac91d020eec40818a47871714dc9c6de4130ba18a55d8cb02c170152c7)
900 bytes saved to recovery/log, run 806a9dc5a7eb3cf3 before truncation
this content may still be in your working tree
damaged commit discarded
";
    let d = parse_discard_output(said).unwrap();
    assert_eq!(
        d.patch_id.as_deref(),
        Some("dec019ac91d020eec40818a47871714dc9c6de4130ba18a55d8cb02c170152c7")
    );
    assert_eq!(d.recovery_run.as_deref(), Some("806a9dc5a7eb3cf3"));
    assert!(d.worktree_may_hold_content);
    // A plan, or anything else, is not a discard.
    let plan = "\
doctor repository: /srv/r/.prikk
acknowledged commit at sequence 1 (patch dec0)
900 bytes saved to recovery/log (planned, nothing written) before truncation
plan only -- nothing written
";
    assert_eq!(parse_discard_output(plan), None);
    assert_eq!(parse_discard_output(""), None);
    // The details are a courtesy: a line in another shape loses the detail and not the fact.
    let bare = parse_discard_output("damaged commit discarded\n").unwrap();
    assert_eq!(bare, Discarded::default());
}

/// The report of a repository after a key retirement (shape measured on prikk 0.55.0): one error for each block, ref
/// state and ref update that lost its trusted signature, then the summary.
fn untrusted_report(findings: &[(&str, char)]) -> String {
    let mut out =
        String::from("doctor repository: /srv/r/.prikk\nverified repository: /srv/r/.prikk\n");
    for (kind, c) in findings {
        out.push_str(&format!(
            "error [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: {kind} {} has no trusted MAINTAINER signature\n  recommendation: configure trusted MAINTAINER keys and re-run verification; doctor will not auto-trust keys or repair signatures\n",
            c.to_string().repeat(64)
        ));
    }
    out.push_str(&format!(
        "issue summary: errors={}, warnings=0, info=0\n",
        findings.len()
    ));
    out
}

#[test]
fn only_the_publication_findings_an_excuse_names_are_taken_out_of_the_report() {
    // P13. The subject of a finding is its kind and id; the report loses those the excuse accepts, and no other.
    let report = parse_doctor_output(&untrusted_report(&[
        ("block", 'a'),
        ("ref-state", '1'),
        ("ref-update", '8'),
        ("block", 'b'),
    ]))
    .unwrap();
    assert_eq!(
        nth(&report.diagnostics, 0).publication_subject(),
        Some(("block", "a".repeat(64).as_str()))
    );
    let (rest, removed) = report.without_excused_publication_findings(|kind, id| {
        (kind == "block" && id == "a".repeat(64)) || (kind == "ref-state" && id == "1".repeat(64))
    });
    assert_eq!(removed, 2);
    assert_eq!((rest.errors, rest.diagnostics.len()), (2, 2));
    assert_eq!(
        nth(&rest.diagnostics, 0).publication_subject(),
        Some(("ref-update", "8".repeat(64).as_str()))
    );
    // Nothing accepted, nothing taken out; the same id under another kind is another finding.
    assert_eq!(
        report.without_excused_publication_findings(|_, _| false).1,
        0
    );
    let (_, removed) = report.without_excused_publication_findings(|kind, id| {
        kind == "ref-state" && id == "a".repeat(64)
    });
    assert_eq!(removed, 0);
    // A finding of another code, or of a shape this build does not know, is never taken out, whatever is accepted.
    let odd = parse_doctor_output(
        "doctor repository: /x\nerror [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: tag aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa has no trusted MAINTAINER signature\nerror [PRIKK-DOCTOR-COMMIT-WITNESS-ACKNOWLEDGED-DAMAGE]: block aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa has no trusted MAINTAINER signature\nissue summary: errors=2, warnings=0, info=0\n",
    )
    .unwrap();
    assert!(
        odd.diagnostics
            .iter()
            .all(|d| d.publication_subject().is_none())
    );
    assert_eq!(odd.without_excused_publication_findings(|_, _| true).1, 0);
}

/// The report of a `seal` killed after it published its block and before it drained the queue (measured on prikk
/// 0.55.0, handoff 004 P15): one error, with prikk's own recommendation.
const DOCTOR_CLEANUP_PENDING: &str = "\
doctor repository: /srv/r/.prikk
verified repository: /srv/r/.prikk
ref publication issues: 1
ref-publication [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: pointer and log agree but matching active publication state remains
trailing partial WAL bytes: 0
error [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: pointer and log agree but matching active publication state remains
  recommendation: run signer-backed `prikk seal --allow-no-audit` for the affected ref; doctor does not sign or append
issue summary: errors=1, warnings=0, info=0
";

#[test]
fn a_pending_cleanup_is_read_and_only_exactly_that_one_error_is_it() {
    let r = parse_doctor_output(DOCTOR_CLEANUP_PENDING).unwrap();
    assert_eq!((r.errors, r.warnings, r.info), (1, 0, 0));
    assert!(nth(&r.diagnostics, 0).is_cleanup_pending());
    assert!(r.is_only_cleanup_pending());
    // Not clean for recovery, and not the discard's: the plan decides what to make of it.
    assert!(!r.is_clean_for_recovery());
    assert!(!r.has_acknowledged_damage());
    // Information and unreached-object notes beside it do not change what it is.
    let notes = DOCTOR_CLEANUP_PENDING.replace(
        "issue summary: errors=1, warnings=0, info=0",
        "info [PRIKK-DOCTOR-VERIFY-OK]: done\nwarning [PRIKK-DOCTOR-UNREACHED-OBJECT]: 3 objects nothing refers to\n  recommendation: the bytes stay; no command removes them yet\nissue summary: errors=1, warnings=1, info=1",
    );
    assert!(
        parse_doctor_output(&notes)
            .unwrap()
            .is_only_cleanup_pending()
    );
    // Another error or warning beside it, a second one of its code, or the same code as a warning: not this.
    let beside = |extra: &str, counts: &str| {
        parse_doctor_output(&DOCTOR_CLEANUP_PENDING.replace(
            "issue summary: errors=1, warnings=0, info=0",
            &format!("{extra}\nissue summary: {counts}"),
        ))
        .unwrap()
    };
    assert!(
        !beside(
            "error [PRIKK-DOCTOR-OBJECT-MISSING]: an object is missing",
            "errors=2, warnings=0, info=0"
        )
        .is_only_cleanup_pending()
    );
    assert!(
        !beside(
            "warning [PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS]: stale",
            "errors=1, warnings=1, info=0"
        )
        .is_only_cleanup_pending()
    );
    assert!(
        !beside(
            "error [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: again",
            "errors=2, warnings=0, info=0"
        )
        .is_only_cleanup_pending()
    );
    let weak = "doctor repository: /x\nwarning [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: x\nissue summary: errors=0, warnings=1, info=0\n";
    assert!(!parse_doctor_output(weak).unwrap().is_only_cleanup_pending());
    // And a clean report, or one for another error, is not it.
    assert!(
        !parse_doctor_output(DOCTOR_OK)
            .unwrap()
            .is_only_cleanup_pending()
    );
    assert!(
        !parse_doctor_output(DOCTOR_DAMAGED)
            .unwrap()
            .is_only_cleanup_pending()
    );
}

/// The report of a `branch create` killed after it published the ref's pointer and before it appended the ref log
/// (measured on prikk 0.55.0): one error, whose recommendation is `ref complete`, not `seal` (handoff 005 K4).
const DOCTOR_LEAD: &str = "\
doctor repository: /srv/r/.prikk
verified repository: /srv/r/.prikk
ref publication issues: 1
trailing partial ref log bytes: 0
error [PRIKK-VERIFY-REF-POINTER-LEADS-LOG]: authoritative pointer leads committed ref log by one transition; run `prikk ref complete heads/draft`
  recommendation: run `prikk ref complete heads/draft`; doctor does not sign or append
error: doctor found repository health errors
issue summary: errors=1, warnings=0, info=0
";

#[test]
fn an_interrupted_publication_is_read_and_only_exactly_that_one_error_of_one_ref_is_it() {
    let r = parse_doctor_output(DOCTOR_LEAD).unwrap();
    assert_eq!((r.errors, r.warnings, r.info), (1, 0, 0));
    assert_eq!(nth(&r.diagnostics, 0).lead_ref(), Some("heads/draft"));
    assert_eq!(r.only_ref_lead(), Some("heads/draft"));
    // It is not a pending cleanup (the `seal` row), and not the discard's.
    assert!(!r.is_only_cleanup_pending());
    assert!(!nth(&r.diagnostics, 0).is_cleanup_pending());
    assert!(!r.has_acknowledged_damage());
    // Information and unreached-object notes beside it do not change what it is.
    let notes = DOCTOR_LEAD.replace(
        "issue summary: errors=1, warnings=0, info=0",
        "info [PRIKK-DOCTOR-VERIFY-OK]: done\nwarning [PRIKK-DOCTOR-UNREACHED-OBJECT]: 3 objects nothing refers to\n  recommendation: the bytes stay; no command removes them yet\nissue summary: errors=1, warnings=1, info=1",
    );
    assert_eq!(
        parse_doctor_output(&notes).unwrap().only_ref_lead(),
        Some("heads/draft")
    );
    // Another error or warning beside it, or a second lead, is not exactly it.
    let beside = |extra: &str, counts: &str| {
        parse_doctor_output(&DOCTOR_LEAD.replace(
            "issue summary: errors=1, warnings=0, info=0",
            &format!("{extra}\nissue summary: {counts}"),
        ))
        .unwrap()
    };
    assert_eq!(
        beside(
            "error [PRIKK-DOCTOR-OBJECT-MISSING]: an object is missing",
            "errors=2, warnings=0, info=0"
        )
        .only_ref_lead(),
        None
    );
    assert_eq!(
        beside(
            "warning [PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS]: stale",
            "errors=1, warnings=1, info=0"
        )
        .only_ref_lead(),
        None
    );
    let second = "error [PRIKK-VERIFY-REF-POINTER-LEADS-LOG]: authoritative pointer leads committed ref log by one transition; run `prikk ref complete heads/other`\n  recommendation: run `prikk ref complete heads/other`; doctor does not sign or append";
    assert_eq!(
        beside(second, "errors=2, warnings=0, info=0").only_ref_lead(),
        None
    );
    // The same code as a warning, with another recommendation, or with a message that names another ref: not it.
    let weak = DOCTOR_LEAD
        .replace(
            "error [PRIKK-VERIFY-REF-POINTER",
            "warning [PRIKK-VERIFY-REF-POINTER",
        )
        .replace("errors=1, warnings=0", "errors=0, warnings=1");
    let weak = parse_doctor_output(&weak).unwrap();
    assert_eq!(nth(&weak.diagnostics, 0).lead_ref(), None);
    assert_eq!(weak.only_ref_lead(), None);
    let reworded = DOCTOR_LEAD.replace(
        "run `prikk ref complete heads/draft`; doctor does not sign or append",
        "run signer-backed `prikk seal --allow-no-audit` for the affected ref",
    );
    assert_eq!(
        parse_doctor_output(&reworded).unwrap().only_ref_lead(),
        None
    );
    let elsewhere = DOCTOR_LEAD.replace(
        "one transition; run `prikk ref complete heads/draft`",
        "one transition; run `prikk ref complete heads/other`",
    );
    assert_eq!(
        parse_doctor_output(&elsewhere).unwrap().only_ref_lead(),
        None
    );
    let odd = DOCTOR_LEAD.replace("heads/draft", "heads/x y");
    assert_eq!(parse_doctor_output(&odd).unwrap().only_ref_lead(), None);
    let none = DOCTOR_LEAD.replace("complete heads/draft`", "complete `");
    assert_eq!(parse_doctor_output(&none).unwrap().only_ref_lead(), None);
    let other_code = DOCTOR_LEAD.replace(
        "PRIKK-VERIFY-REF-POINTER-LEADS-LOG",
        "PRIKK-VERIFY-REF-DIVERGENCE",
    );
    let other_code = parse_doctor_output(&other_code).unwrap();
    assert_eq!(nth(&other_code.diagnostics, 0).lead_ref(), None);
    assert_eq!(other_code.only_ref_lead(), None);
    // And a clean report, or one for a pending cleanup, has none.
    assert_eq!(
        parse_doctor_output(DOCTOR_OK).unwrap().only_ref_lead(),
        None
    );
    assert_eq!(
        parse_doctor_output(DOCTOR_CLEANUP_PENDING)
            .unwrap()
            .only_ref_lead(),
        None
    );
}

/// What `prikk ref complete heads/draft --plan-only` printed on prikk 0.55.0 for the state above.
const COMPLETION_PLAN: &str = "\
ref: heads/draft
leading RefState: a64ea6edd4260601ff2f85055bc97b9c2c02a02d7e9bf4c746ed199888428d25
original signer: ed25519-d03c81318ad222b9
target: 69ed41554bd1003a73f321d29df1e2c2b1ea0746c60b758cb51f590ecf337f82 (Branch)
log tip: (none -- first publication for this ref)
sequence: 1
completing key: ed25519-d03c81318ad222b9
partial tail to remove: 0 byte(s)
";

#[test]
fn what_ref_complete_would_write_is_read_strictly() {
    let p = parse_completion_plan(COMPLETION_PLAN).unwrap();
    assert_eq!(p.ref_name, "heads/draft");
    assert_eq!(
        p.target,
        "69ed41554bd1003a73f321d29df1e2c2b1ea0746c60b758cb51f590ecf337f82"
    );
    assert_eq!(p.kind, "Branch");
    assert_eq!(p.log_tip, None);
    assert_eq!(p.sequence, 1);
    assert_eq!(p.partial_tail_bytes, 0);
    // A log tip that is an id is an id; a partial tail is a number.
    let later = COMPLETION_PLAN
        .replace("(none -- first publication for this ref)", &"c".repeat(64))
        .replace("sequence: 1", "sequence: 4")
        .replace("0 byte(s)", "17 byte(s)");
    let later = parse_completion_plan(&later).unwrap();
    assert_eq!(later.log_tip, Some("c".repeat(64)));
    assert_eq!((later.sequence, later.partial_tail_bytes), (4, 17));
    // Every shape that is not exactly this is refused, and nothing is concluded from it.
    let target_line =
        "target: 69ed41554bd1003a73f321d29df1e2c2b1ea0746c60b758cb51f590ecf337f82 (Branch)";
    for (what, text) in [
        ("empty", String::new()),
        (
            "a line missing",
            COMPLETION_PLAN.replace("sequence: 1\n", ""),
        ),
        (
            "lines in another order",
            COMPLETION_PLAN.replace(
                "sequence: 1\ncompleting key: ed25519-d03c81318ad222b9\n",
                "completing key: ed25519-d03c81318ad222b9\nsequence: 1\n",
            ),
        ),
        (
            "a line added",
            format!("{COMPLETION_PLAN}note: something\n"),
        ),
        (
            "a leading id that is short",
            COMPLETION_PLAN.replace("a64ea6edd4260601", "a64ea6ed"),
        ),
        (
            "a target in capitals",
            COMPLETION_PLAN.replace(target_line, &target_line.to_uppercase()),
        ),
        (
            "a target with no kind",
            COMPLETION_PLAN.replace(" (Branch)", ""),
        ),
        (
            "a log tip that is neither",
            COMPLETION_PLAN.replace("(none -- first publication for this ref)", "unknown"),
        ),
        (
            "a sequence that is not a number",
            COMPLETION_PLAN.replace("sequence: 1", "sequence: first"),
        ),
        (
            "a partial tail that is not a count",
            COMPLETION_PLAN.replace("0 byte(s)", "none"),
        ),
        (
            "an error",
            "error: heads/draft has no pointer; nothing to complete\n".to_owned(),
        ),
    ] {
        assert!(parse_completion_plan(&text).is_err(), "{what}");
    }
}
