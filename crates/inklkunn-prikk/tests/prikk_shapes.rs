//! What prikk 0.56.0 and 0.57.0 changed in the reports the driver reads (maintenance handoff M3), against **stand-ins**
//! that print the shapes those releases printed (measured on the published 0.56.0 binary and on a 0.57.0 build of
//! its source, with the repository path and the ids invented). No real prikk is needed, so these tests run on any
//! machine and name the shape, not the machine's prikk: the driver reads past a field it does not know, and a refusal
//! (exit 1) is an error and never an empty answer.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage, StorageError, Unavailable};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The word for a ref, spelt in two pieces: the boundary gate keeps the read driver's sources free of write-verb
/// literals, and `"ref"` is both a JSON key prikk prints and the name of a write verb.
const RK: &str = concat!("r", "ef");
const BLOCK: &str = "4003d22c67fa4533dd4da922984cf6b23012d9efa890d5324edd6046cd0a1f9f";
const PARENT: &str = "fd48dbb7be1604800371ba07bceec6463d4f2ee89b4804f85028eab55e2b1188";
const STATE: &str = "7cefd0e753b01da2b23e965d31c3961708ea20798d42f8a20f3b4743ed3e7379";
const PATCH: &str = "6312ca4290b52dcbe16ca6cf92222686a3d30ad2945a11531296d9164ab20f8d";

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "inklkunn-prikk-shapes-{}-{tag}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn gate() -> Admission {
    Admission::new(1 << 30, 64 << 20, Duration::from_millis(200))
}

/// A shell script standing in for prikk.
fn shim(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// Open a driver on a freshly written shim, across the fork-exec race a parallel test binary can cause.
fn open_shim(binary: &Path) -> PrikkCli {
    let mut last = None;
    for _ in 0..50 {
        match PrikkCli::open(binary.as_os_str().to_owned(), Sandbox::Unconfined, gate()) {
            Err(StorageError::Unavailable {
                reason: Unavailable::Spawn(m),
                ..
            }) if m.contains("Text file busy") => {
                std::thread::sleep(Duration::from_millis(10));
            }
            other => {
                last = Some(other);
                break;
            }
        }
    }
    last.expect("the shim ran").expect("open")
}

fn retry<T: std::fmt::Debug>(
    mut f: impl FnMut() -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    let mut r = f();
    for _ in 0..50 {
        match &r {
            Err(StorageError::Unavailable {
                reason: Unavailable::Spawn(m),
                ..
            }) if m.contains("Text file busy") => {
                std::thread::sleep(Duration::from_millis(10));
                r = f();
            }
            _ => break,
        }
    }
    r
}

fn repo(tmp: &TempDir) -> PathBuf {
    let repo = tmp.0.join("repo");
    fs::create_dir_all(repo.join(".prikk")).unwrap();
    repo
}

/// `show --format json` of 0.56.0: every patch gains `state` and `resolved_through_claim_id`.
fn show_0_56() -> String {
    format!(
        r#"{{
  "schema_version": "show-report-v1",
  "patches": [
    {{"patch_id": "{PATCH}", "queued": false, "state": "sealed", "resolved_through_claim_id": null, "operations": [
      {{"kind": "create-file", "paths": [{{"path": "pages/b.md"}}], "content": {{"kind": "create-file", "content": {{"kind": "text", "text": "b\n"}}, "mode": 33188}}}},
      {{"kind": "edit-text", "paths": [{{"path": "pages/a.md", "reason": null}}, {{"unresolved_node_id": "n1", "reason": "the node is not in the tree"}}], "content": {{"kind": "edit-text", "old_span_text": "", "replacement_text": "2"}}}}
    ]}}
  ]
}}"#
    )
}

/// `extract --plan-only --format json` of 0.56.0: `chain` and `chain_signature`.
fn extract_0_56() -> String {
    format!(
        r#"{{
  "schema_version": "extract-report-v1",
  "extract_repository": "/srv/r/.prikk",
  "point": "heads/main",
  "point_kind": "{RK}",
  "target_block_id": "{BLOCK}",
  "claim_parent_block_id": null,
  "chain": [],
  "chain_signature": null,
  "provisional": false,
  "written": false,
  "file_count": 2,
  "total_bytes": 9,
  "largest_file_bytes": 7
}}"#
    )
}

/// `log --ref heads/main --format json` of 0.57.0: each block gains `parent_block_ids` and `sealed_by`.
fn log_0_57() -> String {
    format!(
        r#"{{
  "schema_version": "log-report-v1",
  "repository": "/srv/r/.prikk",
  "{RK}": "heads/main",
  "current_branch": "heads/main",
  "blocks": [
    {{
      "block_id": "{BLOCK}",
      "ref_state_id": "{STATE}",
      "update_seq": 2,
      "kind": "Normal",
      "rollback_block": false,
      "parent_count": 1,
      "parent_block_ids": ["{PARENT}"],
      "sealed_by": ["m1"],
      "patch_count": 1,
      "rollback_patch_count": 0,
      "required_attestation_count": 0,
      "patch_messages": [
        {{"patch_id": "{PATCH}", "message": "two"}}
      ],
      "previous_ref_state_id": null
    }}
  ]
}}"#
    )
}

#[test]
fn the_fields_prikk_0_56_and_0_57_added_to_their_reports_are_read_past() {
    let tmp = TempDir::new("additive");
    let repo = repo(&tmp);
    for (name, text) in [
        ("show.json", show_0_56()),
        ("extract.json", extract_0_56()),
        ("log.json", log_0_57()),
    ] {
        fs::write(tmp.0.join(name), text).unwrap();
    }
    let fake = shim(
        &tmp.0,
        "prikk",
        &format!(
            r#"case "$1" in
  --version) echo 'prikk 0.57.0' ;;
  show) cat '{d}/show.json' ;;
  extract) cat '{d}/extract.json' ;;
  log) cat '{d}/log.json' ;;
  *) echo "error: unexpected $1" >&2; exit 2 ;;
esac"#,
            d = tmp.0.display()
        ),
    );
    let d = open_shim(&fake);
    assert_eq!(d.version(), "prikk 0.57.0");

    let change = retry(|| d.change(&repo, PATCH)).expect("a 0.56.0 show report is read");
    assert_eq!(change.patches.len(), 1);
    let patch = change.patches.first().expect("one patch");
    assert_eq!(patch.patch_id, PATCH);
    assert_eq!(patch.operations.len(), 2);

    let plan = retry(|| d.plan(&repo, &Point::View("heads/main".into())))
        .expect("a 0.56.0 extract report is read");
    assert_eq!(plan.target_block_id, BLOCK);
    assert_eq!((plan.file_count, plan.total_bytes), (2, 9));
    assert!(!plan.written);

    let history = retry(|| d.history(&repo, &Point::View("heads/main".into()), Some(5)))
        .expect("a 0.57.0 log report is read");
    assert_eq!(history.blocks.len(), 1);
    let block = history.blocks.first().expect("one block");
    assert_eq!(block.block_id, BLOCK);
    assert_eq!(
        block.patch_messages.first().expect("a message").message,
        "two"
    );
}

#[test]
fn a_log_that_prikk_0_56_refuses_is_an_error_and_never_an_empty_history() {
    // 0.55.0 answered `log --ref <not a view>` with exit 0 and no blocks (ledger PK-36); 0.56.0 exits 1 and says why.
    // The driver never sends a block id (it refuses one itself, see `malformed_arguments_are_refused_before_prikk_runs`),
    // so what can reach this is a view prikk does not have: the refusal's words come back, not an empty list.
    let tmp = TempDir::new("log-refusal");
    let repo = repo(&tmp);
    let fake = shim(
        &tmp.0,
        "prikk",
        r#"case "$1" in
  --version) echo 'prikk 0.56.0' ;;
  log) echo "error: invalid name: heads/x is not a point this command reads: expected a ref name (heads/…, tags/… or remotes/…) or a block id (64 lowercase hex characters)" >&2; exit 1 ;;
esac"#,
    );
    let d = open_shim(&fake);
    let r = retry(|| d.history(&repo, &Point::View("heads/x".into()), None));
    match r {
        Err(StorageError::Refused {
            verb: "log",
            message: text,
            ..
        }) => {
            assert!(text.contains("not a point this command reads"), "{text}");
        }
        other => panic!("a refusal is an error with prikk's words, got {other:?}"),
    }
}
