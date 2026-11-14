//! A sweep of the scratch directory from another PID namespace (handoff 005 corrections, sixth round, K55; the review's
//! X-a, proved there the same way; and, in the seventh round, K65, for the export, whose extraction and partial bundle are
//! held the same way: the review's A-a).
//!
//! The image runs `inklkunn` as PID 1 of its container, and so does the natural `docker run --rm -v <same volume> ...
//! wiki import-folder`: two processes that share a scratch directory and see each other's pids as different processes
//! (or as none). An extract is live while its `flock` is held, and a lock holds across PID namespaces, so the sweep of one
//! leaves the other's extract whole and removes a dead one. This test starts the sweep as PID 1 of a fresh namespace
//! (`unshare -Urpf --mount-proc`), which only a crate that may start a process can, and so lives here. Where such a
//! namespace cannot be made (a host that forbids unprivileged user namespaces) it prints `skipping: PID namespaces
//! unavailable` and returns.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use inklkunn_core::export::export_bundle;
use inklkunn_core::read::scratch::{Extract, sweep_leftovers, sweep_partials};
use inklkunn_prikk::storage::{
    Change, Diff, FileMeta, History, HistoryBlock, Materialized, Tree, Verification, Views,
};
use inklkunn_prikk::{ExtractBounds, Point, Storage, StorageError};
use inklkunn_signer::flock::{Acquired, try_exclusive};
use inklkunn_store::{DisplayName, NewWiki, RootLayout, Slug, WikiId, WikiRecord};

/// Set in the child: the scratch directory it sweeps.
const CHILD_SWEEPS: &str = "INKLKUNN_TEST_SWEEP_FROM_NAMESPACE";

/// Set in the child when it also sweeps the partial bundles of [`DEST`] in this directory.
const CHILD_SWEEPS_PARTIALS: &str = "INKLKUNN_TEST_SWEEP_PARTIALS_FROM_NAMESPACE";

/// The name of the export's destination in the test of the export.
const DEST: &str = "bundle";

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
    if let Some(parent) = std::env::var_os(CHILD_SWEEPS_PARTIALS) {
        sweep_partials(Path::new(&parent), DEST);
    }
}

fn unshare(args: &[&str]) -> Command {
    let mut command = Command::new("unshare");
    command.args(["-Urpf", "--mount-proc"]).args(args);
    command
}

/// Can a PID namespace be made here? A host that cannot (no `unshare`, user namespaces forbidden) is one this cannot be
/// shown on.
fn namespaces_available() -> bool {
    let yes = unshare(&["true"])
        .output()
        .is_ok_and(|out| out.status.success());
    if !yes {
        println!("skipping: PID namespaces unavailable");
    }
    yes
}

/// Run the sweeper as PID 1 of a fresh PID namespace over `scratch` (and, when given, the partial bundles of [`DEST`] in
/// `parent`).
fn sweep_from_another_namespace(scratch: &Path, parent: Option<&Path>) {
    let mut command = unshare(&[
        std::env::current_exe()
            .expect("this test")
            .to_str()
            .unwrap(),
        "--exact",
        "child_sweeps_from_its_own_pid_namespace",
        "--test-threads=1",
    ]);
    command.env(CHILD_SWEEPS, scratch);
    if let Some(parent) = parent {
        command.env(CHILD_SWEEPS_PARTIALS, parent);
    }
    let out = command.output().expect("the sweeper starts");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && said.contains("1 passed"),
        "the sweeper did not run: {said} {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_sweep_from_another_pid_namespace_leaves_a_live_extract_and_removes_a_dead_one() {
    if !namespaces_available() {
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

    sweep_from_another_namespace(&scratch, None);

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

const TIP: &str = "b10c000000000000000000000000000000000000000000000000000000000001";

/// The pages the stand-in repository holds.
const PAGES: usize = 40;

/// A repository that holds `PAGES` pages and one block, and that has a sweep from another PID namespace run at the two
/// moments an export is exposed: right after its extraction is made, and while its bundle is being built beside the
/// destination. It answers only what an export asks, and reports what it extracted truthfully.
struct Stand<'a> {
    files: BTreeMap<String, String>,
    scratch: &'a Path,
    parent: &'a Path,
    sweeps: AtomicUsize,
}

fn unsupported<T>() -> Result<T, StorageError> {
    Err(StorageError::Refused {
        verb: "stand-in",
        message: "the stand-in answers only what an export asks".to_owned(),
        kind: None,
        reason: None,
    })
}

/// The directories in `dir` whose names start with `prefix`.
fn directories(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .expect("listing")
        .map(|e| e.expect("entry").path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix))
        })
        .collect()
}

impl Storage for Stand<'_> {
    fn views(&self, _: &Path) -> Result<Views, StorageError> {
        unsupported()
    }
    fn all_views(&self, _: &Path) -> Result<Views, StorageError> {
        unsupported()
    }
    fn tree(&self, _: &Path, point: &Point, _: Option<&str>) -> Result<Tree, StorageError> {
        Ok(Tree {
            schema_version: "tree-listing-v1".to_owned(),
            point: point.to_string(),
            target_block_id: TIP.to_owned(),
            prefix: None,
            entries: Vec::new(),
        })
    }
    fn stat(&self, _: &Path, _: &Point, _: &str) -> Result<FileMeta, StorageError> {
        unsupported()
    }
    fn read(&self, _: &Path, _: &Point, _: &str, _: u64) -> Result<Vec<u8>, StorageError> {
        unsupported()
    }
    fn history(&self, _: &Path, view: &Point, _: Option<u64>) -> Result<History, StorageError> {
        // The export is building its bundle: the pages are copied, the partial bundle is made, the extraction is still
        // there. A sweep from a namespace that sees none of this export's processes runs now.
        sweep_from_another_namespace(self.scratch, Some(self.parent));
        self.sweeps.fetch_add(1, Ordering::Relaxed);
        let whole = |dirs: Vec<PathBuf>, what: &str| {
            let [dir] = dirs.as_slice() else {
                panic!("{what}: {dirs:?}");
            };
            assert_eq!(
                std::fs::read_dir(dir.join("pages")).map_or(0, Iterator::count),
                PAGES,
                "{what} is whole after the sweep"
            );
        };
        whole(
            directories(self.parent, &format!(".{DEST}.partial-")),
            "the partial bundle",
        );
        whole(directories(self.scratch, "index-"), "the extraction");
        Ok(History {
            schema_version: "log-report-v1".to_owned(),
            view: view.to_string(),
            blocks: vec![HistoryBlock {
                block_id: TIP.to_owned(),
                ref_state_id: "a".repeat(64),
                update_seq: 1,
                kind: "Root".to_owned(),
                rollback_block: false,
                patch_count: 0,
                patch_messages: Vec::new(),
                previous_ref_state_id: None,
            }],
        })
    }
    fn change(&self, _: &Path, _: &str) -> Result<Change, StorageError> {
        Ok(Change {
            schema_version: "show-report-v1".to_owned(),
            patches: Vec::new(),
        })
    }
    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> Result<Diff, StorageError> {
        unsupported()
    }
    fn plan(&self, _: &Path, _: &Point) -> Result<Materialized, StorageError> {
        unsupported()
    }
    fn materialize(
        &self,
        _: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        _: ExtractBounds,
    ) -> Result<Materialized, StorageError> {
        let target = scratch.join(name);
        for (path, text) in &self.files {
            let at = target.join(path);
            std::fs::create_dir_all(at.parent().expect("a parent")).expect("dirs");
            std::fs::write(at, text).expect("a file");
        }
        // The extraction is made and the export has not read it yet: the sweep runs here too.
        sweep_from_another_namespace(self.scratch, Some(self.parent));
        self.sweeps.fetch_add(1, Ordering::Relaxed);
        Ok(Materialized {
            schema_version: "extract-report-v1".to_owned(),
            point: point.to_string(),
            point_kind: "block".to_owned(),
            target_block_id: TIP.to_owned(),
            claim_parent_block_id: None,
            provisional: false,
            written: true,
            file_count: self.files.len() as u64,
            total_bytes: self.files.values().map(|t| t.len() as u64).sum(),
            largest_file_bytes: 0,
        })
    }
    fn verify(&self, _: &Path) -> Result<Verification, StorageError> {
        unsupported()
    }
}

/// K65 (the review's A-a): the export's extraction and its partial bundle are held by locks, so a sweep from a PID namespace
/// that cannot see the export's process (`docker exec ... wiki export` beside `docker run ... wiki export`, which is PID 1)
/// leaves both whole, and the bundle has every page. The old rule read `/proc/<pid>` and removed a live export's tree, and
/// the export then wrote a bundle with files missing, checksums that matched what it held, and success.
#[test]
fn an_export_beside_a_sweep_from_another_pid_namespace_keeps_its_directories_and_bundles_every_file()
 {
    if !namespaces_available() {
        return;
    }
    let tmp =
        Tmp(std::env::temp_dir().join(format!("inklkunn-export-namespace-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&tmp.0);
    let out = tmp.0.join("out");
    std::fs::create_dir_all(&out).expect("out");
    let layout = RootLayout::new(tmp.0.join("root"));
    let record = WikiRecord::from_new(
        WikiId::new(),
        NewWiki::new(Slug::new("exp").unwrap(), DisplayName::new("Exp").unwrap()),
        1,
    )
    .unwrap();
    let scratch = layout.scratch_dir(&record.id);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let files: BTreeMap<String, String> = (0..PAGES)
        .map(|n| {
            (
                format!("pages/P{n}.md"),
                format!("---\ntitle: P{n}\nid: p{n}\n---\n\nText {n}.\n"),
            )
        })
        .collect();
    let stand = Stand {
        files: files.clone(),
        scratch: &scratch,
        parent: &out,
        sweeps: AtomicUsize::new(0),
    };
    let dest = out.join(DEST);
    let done = export_bundle(&stand, &layout, &record, &dest, "0.0.0", 1)
        .unwrap_or_else(|e| panic!("the export: {e}"));
    assert_eq!(stand.sweeps.load(Ordering::Relaxed), 2, "both moments");
    assert_eq!(done.pages, PAGES);
    for (path, text) in &files {
        assert_eq!(
            std::fs::read_to_string(dest.join(path)).unwrap_or_default(),
            *text,
            "{path} is in the bundle as it was extracted"
        );
    }
    let sums = std::fs::read_to_string(dest.join("SHA256SUMS")).expect("checksums");
    for path in files.keys() {
        assert!(sums.contains(path), "{path} is listed in the checksums");
    }
    // Nothing of the export is left: its directories and their lock files are gone with it.
    let left: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .chain(std::fs::read_dir(&scratch).unwrap())
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec![DEST.to_owned()], "{left:?}");
}
