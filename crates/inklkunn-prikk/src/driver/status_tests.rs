//! The `status` report: read as pinned, and refused when it is anything else (handoff 004 T5).
#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

/// `prikk status --format json` as prikk 0.55.0 printed it for a queue of one patch with every operation kind a
/// change makes (copied from a measured run).
const STATUS_QUEUED: &str = r#"{
  "schema_version": "status-report-v1",
  "repository": "/srv/r/.prikk",
  "active_wal_records": 1,
  "trailing_partial_wal_bytes": 0,
  "heads_main_ref_state": "e1d1f9176a20d72406ef621cc38bdbea0de1148c568f4a0d91920504bfc0ca46",
  "current_branch": "heads/main",
  "provisional_worktree": null,
  "interrupted_materialization": null,
  "queue": {
    "count": 1,
    "target_ref": "heads/main",
    "target_ref_status": null,
    "threshold_status": "none",
    "warn_threshold": 800,
    "hard_limit": 1000,
    "patches": [
      {"patch_id": "dec019ac91d020eec40818a47871714dc9c6de4130ba18a55d8cb02c170152c7", "message": "c3", "operations": [
        {"kind": "delete-node", "paths": [{"path": "pages/A.md"}]},
        {"kind": "create-file", "paths": [{"path": "pages/D.md"}]},
        {"kind": "edit-text", "paths": [{"path": "pages/C.md"}]},
        {"kind": "rename-path", "paths": [{"path": "pages/B.md"},{"path": "pages/C.md"}], "author_key_id": "author"}
      ]}
    ]
  }
}"#;

/// The same, after the active WAL was cut short: nothing queued, and bytes that are not a record.
const STATUS_TORN: &str = r#"{"schema_version": "status-report-v1", "repository": "/srv/r/.prikk", "active_wal_records": 0, "trailing_partial_wal_bytes": 900, "heads_main_ref_state": null, "current_branch": "heads/main", "provisional_worktree": null, "interrupted_materialization": null, "queue": {"count": 0, "target_ref": null, "target_ref_status": null, "threshold_status": null, "warn_threshold": null, "hard_limit": null, "patches": []}}"#;

fn parse(text: &str) -> Result<RepoStatus> {
    parse_report::<RepoStatus>("status", text.as_bytes(), schema::STATUS_REPORT)
        .and_then(RepoStatus::checked)
}

#[test]
fn a_status_report_is_read_with_its_queue_and_its_torn_bytes() {
    let queued = parse(STATUS_QUEUED).unwrap();
    assert_eq!(queued.current_branch.as_deref(), Some("heads/main"));
    assert_eq!(queued.queue.count, 1);
    assert_eq!(queued.queue.target_ref.as_deref(), Some("heads/main"));
    let patch = queued.queue.patches.first().unwrap();
    assert_eq!(patch.message, "c3");
    assert_eq!(
        patch
            .operations
            .iter()
            .map(|o| o.kind.as_str())
            .collect::<Vec<_>>(),
        ["delete-node", "create-file", "edit-text", "rename-path"],
        "the operations are kept in prikk's order"
    );
    let rename = patch.operations.last().unwrap();
    assert_eq!(
        rename
            .paths
            .iter()
            .map(|p| p.path.as_deref())
            .collect::<Vec<_>>(),
        [Some("pages/B.md"), Some("pages/C.md")]
    );
    assert_eq!(queued.trailing_partial_wal_bytes, 0);
    assert!(queued.provisional_worktree.is_none() && queued.interrupted_materialization.is_none());

    // A torn write: an empty queue and bytes that are not a record. Not the same thing as "nothing happened".
    let torn = parse(STATUS_TORN).unwrap();
    assert_eq!(
        (torn.queue.count, torn.trailing_partial_wal_bytes),
        (0, 900)
    );
}

#[test]
fn a_status_report_of_a_shape_the_driver_does_not_know_is_refused_not_guessed() {
    // Another schema name is a schema refusal, before the shape is read.
    let doctored = STATUS_TORN.replace("status-report-v1", "status-report-v2");
    assert!(
        matches!(parse(&doctored), Err(StorageError::UnsupportedSchema { found, .. }) if found == "status-report-v2")
    );
    // The right name with a field gone, or of the wrong type, is damaged output.
    for broken in [
        STATUS_TORN.replace("\"queue\"", "\"queued\""),
        STATUS_TORN.replace("\"count\": 0", "\"count\": \"none\""),
        STATUS_TORN.replace(
            "\"trailing_partial_wal_bytes\": 900",
            "\"trailing_partial_wal_bytes\": -1",
        ),
        STATUS_TORN.replace("\"patches\": []", "\"patches\": {}"),
        STATUS_QUEUED.replace("\"message\": \"c3\", ", ""),
        "{}".to_owned(),
        "not json".to_owned(),
    ] {
        assert!(
            matches!(parse(&broken), Err(StorageError::Damaged { .. })),
            "{broken}"
        );
    }
    // A report that disagrees with itself is not read: the count is the list.
    let lying = STATUS_QUEUED.replace("\"count\": 1", "\"count\": 2");
    assert!(
        matches!(parse(&lying), Err(StorageError::Damaged { detail, .. }) if detail.contains("count"))
    );
}
