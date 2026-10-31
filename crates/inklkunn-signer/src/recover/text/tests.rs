//! The readers of `prikk doctor` and `prikk unlock`: output captured from prikk 0.55.0, and every way the shape can be
//! wrong. Recovery clears locks and discards a commit on what these say, so they refuse what they do not know.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

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
    assert_eq!(r.diagnostics[0].code, "PRIKK-DOCTOR-VERIFY-OK");
    assert_eq!(r.diagnostics[0].severity, Severity::Info);
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
    assert_eq!(more.others()[0].code, "PRIKK-DOCTOR-OBJECT-MISSING");
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
    assert!(note.diagnostics[0].is_unreached_note());
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
    assert_eq!(locks[0].path, "/srv/r/.prikk/active/default/active.lock");
    assert_eq!(locks[0].kind, "active");
    assert_eq!(locks[0].recorded_pid, 408_657);
    assert!(
        locks[0]
            .liveness
            .starts_with("does not appear to be running")
    );
    // The second says a process "appears running": read, kept, and nothing here changes with it (ledger PL-8).
    assert_eq!(locks[1].kind, "container:object-store");
    assert_eq!(locks[1].recorded_pid, 2);
    assert!(locks[1].liveness.starts_with("appears running"));
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
