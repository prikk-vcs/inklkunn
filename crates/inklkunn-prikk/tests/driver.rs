//! The driver against a real repository at the floor (handoff 001 T3). Every test self-skips with a
//! visible line when prikk is not on `PATH`; the confined tests also skip when bubblewrap cannot run here.
//! The fixture is `tests/fixtures/wiki-repo` (see its README), copied to a fresh temporary directory per
//! test so nothing in the crate is ever written.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use inklkunn_prikk::sandbox::{BwrapConfig, probe_bwrap};
use inklkunn_prikk::{
    Admission, ExtractBounds, MIN_PRIKK_VERSION, Point, PrikkCli, Sandbox, Storage, StorageError,
    Unavailable,
};

const SKIP_PRIKK: &str = "skipping: prikk not on PATH";
const SKIP_BWRAP: &str = "skipping: bubblewrap cannot confine a process here";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

fn bwrap_ok() -> bool {
    probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok()
}

/// A fresh directory under the system temp dir; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("inklkunn-prikk-{}-{tag}-{n}", std::process::id()));
        fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for entry in fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// The fixture copied into `tmp/repo`, plus an empty `tmp/scratch`.
fn fixture(tmp: &TempDir) -> (PathBuf, PathBuf) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wiki-repo");
    let repo = tmp.0.join("repo");
    copy_dir(&src, &repo);
    let scratch = tmp.0.join("scratch");
    fs::create_dir_all(&scratch).expect("scratch");
    (repo, scratch)
}

fn gate() -> Admission {
    Admission::new(1 << 30, 64 << 20, Duration::from_millis(200))
}

fn driver(sandbox: Sandbox) -> PrikkCli {
    PrikkCli::open("prikk", sandbox, gate()).expect("a prikk at the floor")
}

fn main_view() -> Point {
    Point::View("heads/main".into())
}

fn no_bounds() -> ExtractBounds {
    ExtractBounds {
        max_bytes: None,
        max_files: None,
    }
}

/// Both policies when bubblewrap works, else the unconfined one with a visible note.
fn policies() -> Vec<Sandbox> {
    if bwrap_ok() {
        println!("policies: confined (bubblewrap) and unconfined");
        vec![Sandbox::default(), Sandbox::Unconfined]
    } else {
        println!("{SKIP_BWRAP}");
        vec![Sandbox::Unconfined]
    }
}

#[test]
fn open_checks_the_floor() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    let d = driver(Sandbox::Unconfined);
    assert!(d.version().starts_with("prikk 0."), "{}", d.version());
    assert!(MIN_PRIKK_VERSION.starts_with("0.54"));
}

#[test]
fn views_tree_stat_and_read_return_typed_values() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    for sandbox in policies() {
        let tmp = TempDir::new("read");
        let (repo, _) = fixture(&tmp);
        let d = driver(sandbox);
        let views = d.views(&repo).expect("views");
        assert_eq!(views.schema_version, "branch-list-v1");
        assert!(
            views
                .views
                .iter()
                .any(|v| v.name == "heads/main" && !v.closed),
            "{views:?}"
        );

        let main = main_view();
        let tree = d.tree(&repo, &main, None).expect("tree");
        let paths: Vec<&str> = tree.entries.iter().map(|e| e.path.as_str()).collect();
        for expected in [
            "pages/Home.md",
            "pages/Glossary.md",
            "pages/Guides/Getting Started.md",
            "wiki-bundle.yaml",
        ] {
            assert!(paths.contains(&expected), "{paths:?}");
        }
        let pages = d.tree(&repo, &main, Some("pages")).expect("tree pages");
        assert!(
            pages.entries.iter().all(|e| e.path.starts_with("pages/")),
            "{pages:?}"
        );

        let meta = d.stat(&repo, &main, "pages/Home.md").expect("stat");
        assert_eq!(meta.path, "pages/Home.md");
        assert!(meta.size > 0);
        let bytes = d
            .read(&repo, &main, "pages/Home.md", 1 << 20)
            .expect("read");
        assert_eq!(bytes.len() as u64, meta.size);
        assert!(bytes.starts_with(b"---\ntitle: Home\n"));
        assert!(
            String::from_utf8_lossy(&bytes).contains("whole wiki"),
            "the tip has block 2's edit"
        );
    }
}

#[test]
fn history_change_and_diff() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    for sandbox in policies() {
        let tmp = TempDir::new("history");
        let (repo, _) = fixture(&tmp);
        let d = driver(sandbox);
        let main = main_view();
        let history = d.history(&repo, &main, None).expect("history");
        assert_eq!(history.blocks.len(), 2, "{history:?}");
        let newest = history.blocks.first().expect("tip");
        let root = history.blocks.get(1).expect("root");
        assert!(newest.update_seq > root.update_seq, "newest first");
        assert_eq!(root.kind, "Root");
        let message = &newest.patch_messages.first().expect("message").message;
        assert!(message.starts_with("Proposal: 019a0000"), "{message}");

        let one = d.history(&repo, &main, Some(1)).expect("limited");
        assert_eq!(one.blocks.len(), 1);

        let change = d.change(&repo, &newest.block_id).expect("change");
        let ops: Vec<&str> = change
            .patches
            .iter()
            .flat_map(|p| p.operations.iter())
            .map(|o| o.kind.as_str())
            .collect();
        assert!(!ops.is_empty(), "{change:?}");

        let diff = d
            .diff(
                &repo,
                &Point::Block(root.block_id.clone()),
                &Point::Block(newest.block_id.clone()),
                &[],
            )
            .expect("diff");
        let home = diff
            .entries
            .iter()
            .find(|e| e.path == "pages/Home.md")
            .expect("Home changed");
        assert_eq!(home.status, "modified");
        assert!(
            home.hunks.iter().any(|h| h.contains("whole wiki")),
            "{home:?}"
        );
        assert!(
            diff.entries
                .iter()
                .any(|e| e.path == "pages/Glossary.md" && e.status == "added"),
            "{diff:?}"
        );

        let only = d
            .diff(
                &repo,
                &Point::Block(root.block_id.clone()),
                &Point::Block(newest.block_id.clone()),
                &["pages/Glossary.md"],
            )
            .expect("filtered diff");
        assert!(
            only.entries.iter().all(|e| e.path == "pages/Glossary.md"),
            "{only:?}"
        );
    }
}

#[test]
fn plan_and_materialize_write_only_into_the_scratch_mount() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    for sandbox in policies() {
        let confined = sandbox.is_confined();
        let tmp = TempDir::new("extract");
        let (repo, scratch) = fixture(&tmp);
        let d = driver(sandbox);
        let main = main_view();
        let plan = d.plan(&repo, &main).expect("plan");
        assert!(!plan.written);
        assert_eq!(plan.file_count, 6, "{plan:?}");

        let done = d
            .materialize(&repo, &main, &scratch, "out", no_bounds())
            .expect("materialize");
        assert!(done.written);
        let into = scratch.join("out");
        assert!(into.join("pages/Home.md").is_file());
        assert!(into.join("attachments/diagram.svg").is_file());

        // The bound refuses before the first byte, with prikk's words and code.
        let bounded = d.materialize(
            &repo,
            &main,
            &scratch,
            "bounded",
            ExtractBounds {
                max_bytes: Some(10),
                max_files: None,
            },
        );
        assert!(
            matches!(
                bounded,
                Err(StorageError::Refused {
                    verb: "extract",
                    ..
                })
            ),
            "{bounded:?}"
        );
        assert!(!scratch.join("bounded").exists());

        // An existing target, a bad name, a missing scratch: refused by the driver before anything runs.
        let exists = d.materialize(&repo, &main, &scratch, "out", no_bounds());
        assert!(
            matches!(
                exists,
                Err(StorageError::InvalidArgument {
                    verb: "extract",
                    ..
                })
            ),
            "{exists:?}"
        );
        for bad in ["", "../x", "a/b", ".hidden", "-x"] {
            let r = d.materialize(&repo, &main, &scratch, bad, no_bounds());
            assert!(
                matches!(r, Err(StorageError::InvalidArgument { .. })),
                "{bad}: {r:?}"
            );
        }
        let r = d.materialize(&repo, &main, &tmp.0.join("nope"), "x", no_bounds());
        assert!(
            matches!(r, Err(StorageError::InvalidArgument { .. })),
            "{r:?}"
        );

        // The scratch may not be the repository or anything inside it, nor a symlink into it.
        let inside = d.materialize(&repo, &main, &repo, "inside", no_bounds());
        assert!(
            matches!(
                inside,
                Err(StorageError::Unavailable {
                    reason: Unavailable::Spawn(_),
                    ..
                })
            ),
            "{inside:?}"
        );
        assert!(!repo.join("inside").exists());
        let link = tmp.0.join("link-into-repo");
        std::os::unix::fs::symlink(repo.join("pages"), &link).unwrap();
        let through = d.materialize(&repo, &main, &link, "x", no_bounds());
        assert!(
            matches!(
                through,
                Err(StorageError::Unavailable {
                    reason: Unavailable::Spawn(_),
                    ..
                })
            ),
            "{through:?}"
        );
        assert!(!repo.join("pages/x").exists());

        if confined {
            // The repository is read-only inside the sandbox: a second scratch beside it is the only place
            // bytes can land, and they do.
            let other = tmp.0.join("other");
            fs::create_dir_all(&other).unwrap();
            let r = d.materialize(&repo, &main, &other, "x", no_bounds());
            assert!(r.is_ok(), "{r:?}");
            assert!(other.join("x/pages/Home.md").is_file());
        }
    }
}

#[test]
fn verify_reports_a_verdict() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    for sandbox in policies() {
        let tmp = TempDir::new("verify");
        let (repo, _) = fixture(&tmp);
        let d = driver(sandbox);
        let v = d.verify(&repo).expect("verify runs");
        assert!(v.verdict.ok, "{v:?}");
        assert!(v.stages.iter().any(|s| s.stage == "objects"));
    }
}

#[test]
fn refusals_carry_prikks_text_and_a_missing_repository_is_refused() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    let tmp = TempDir::new("refuse");
    let (repo, _) = fixture(&tmp);
    let d = driver(Sandbox::Unconfined);
    let main = main_view();
    let missing = d.stat(&repo, &main, "pages/Nope.md");
    assert!(
        matches!(&missing, Err(StorageError::Refused { verb: "cat", message, .. }) if message.contains("pages/Nope.md")),
        "{missing:?}"
    );
    let no_view = d.tree(&repo, &Point::View("heads/nope".into()), None);
    assert!(
        matches!(no_view, Err(StorageError::Refused { verb: "tree", .. })),
        "{no_view:?}"
    );
    let not_a_repo = d.views(&tmp.0);
    assert!(
        matches!(&not_a_repo, Err(StorageError::Refused { verb: "branch", kind: Some(k), message, .. }) if k == "driver:no-repository" && !message.contains(&*tmp.0.to_string_lossy())),
        "{not_a_repo:?}"
    );
    let too_big = d.read(&repo, &main, "pages/Home.md", 4);
    assert!(
        matches!(too_big, Err(StorageError::Refused { verb: "cat", .. })),
        "{too_big:?}"
    );
}

#[test]
fn malformed_arguments_are_refused_before_prikk_runs() {
    // No prikk is needed: the shim counts its invocations, and must see none.
    let tmp = TempDir::new("args");
    let marker = tmp.0.join("called");
    let fake = shim(
        &tmp.0,
        &format!(
            "case \"$1\" in --version) echo 'prikk 0.54.0' ;; *) touch '{}'; exit 2 ;; esac",
            marker.display()
        ),
    );
    let d = open_shim(&fake).expect("open");
    let repo = tmp.0.join("repo");
    fs::create_dir_all(repo.join(".prikk")).unwrap();
    let cases: Vec<Result<(), StorageError>> = vec![
        d.tree(&repo, &Point::View("main".into()), None).map(|_| ()),
        d.tree(&repo, &main_view(), Some("../x")).map(|_| ()),
        d.stat(&repo, &main_view(), "/etc/passwd").map(|_| ()),
        d.read(&repo, &Point::Block("deadbeef".into()), "pages/x", 10)
            .map(|_| ()),
        d.history(&repo, &Point::View("heads/".into()), None)
            .map(|_| ()),
        // prikk's `log --ref` takes a view; a block id there is an empty list with exit 0 (ledger PK-36), so
        // the driver refuses it before prikk can give an answer that looks like "no history".
        d.history(&repo, &Point::Block("a".repeat(64)), None)
            .map(|_| ()),
        d.change(&repo, "--help").map(|_| ()),
        d.diff(&repo, &main_view(), &Point::View("-x".into()), &[])
            .map(|_| ()),
        d.diff(&repo, &main_view(), &main_view(), &["a/./b"])
            .map(|_| ()),
        d.plan(&repo, &Point::Block("X".repeat(64))).map(|_| ()),
    ];
    for c in &cases {
        assert!(
            matches!(c, Err(StorageError::InvalidArgument { .. })),
            "{c:?}"
        );
    }
    assert!(!marker.exists(), "prikk must not have been called");
}

/// Open a driver on a freshly written shim, retrying the one failure a parallel test binary can cause: a
/// sibling test forks while this file is still open for writing, so its child briefly holds the descriptor
/// and the kernel answers `ETXTBSY` to our exec (the child never execs this file; the race is in the window
/// between its fork and its own exec).
fn open_shim(binary: &Path) -> Result<PrikkCli, StorageError> {
    let mut last = Err(StorageError::Unavailable {
        verb: "--version",
        reason: Unavailable::Busy,
    });
    for _ in 0..50 {
        last = PrikkCli::open(binary.as_os_str().to_owned(), Sandbox::Unconfined, gate());
        match &last {
            Err(StorageError::Unavailable {
                reason: Unavailable::Spawn(m),
                ..
            }) if m.contains("Text file busy") => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => return last,
        }
    }
    last
}

/// A shell script standing in for prikk.
fn shim(dir: &Path, body: &str) -> PathBuf {
    shim_named(dir, "prikk", body)
}

fn shim_named(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// Retry a read against a fresh shim across the fork–exec race (see `open_shim`).
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

#[test]
fn a_size_hint_stands_in_for_the_walk_and_never_vouches_for_a_repository() {
    let tmp = TempDir::new("hints");
    let repo = tmp.0.join("repo");
    fs::create_dir_all(repo.join(".prikk")).unwrap();
    fs::write(repo.join(".prikk/blob"), vec![0u8; 1024]).unwrap();
    let fake = shim(
        &tmp.0,
        r#"case "$1" in
  --version) echo 'prikk 0.54.0' ;;
  branch) echo '{"schema_version": "branch-list-v1", "branches": []}' ;;
  *) echo '{}' ;;
esac"#,
    );
    let hints = inklkunn_prikk::SizeHints::new();
    let d = open_shim(&fake)
        .expect("open")
        .with_size_hints(hints.clone());

    // No hint: the walk finds 1 KiB, a `tree` costs the floor and is admitted (the shim then answers
    // something that is not a tree, which is damaged output and not a refusal to admit).
    let walked = retry(|| d.tree(&repo, &main_view(), None));
    assert!(
        matches!(walked, Err(StorageError::Damaged { .. })),
        "{walked:?}"
    );
    // A hint of 10 GiB is believed instead of the walk: the same read would cost 21 GiB, which no pool of
    // this gate can hold, and it is answered before anything runs.
    hints.set(&repo, 10 << 30);
    let hinted = d.tree(&repo, &main_view(), None);
    assert!(
        matches!(hinted, Err(StorageError::TooLarge { verb: "tree", .. })),
        "{hinted:?}"
    );
    // A verb whose cost does not follow the store needs no figure, so the hint does not touch it.
    let flat = retry(|| d.views(&repo));
    assert!(flat.is_ok(), "{flat:?}");
    // Forgotten: the walk is the truth again.
    hints.forget(&repo);
    let again = retry(|| d.tree(&repo, &main_view(), None));
    assert!(
        matches!(again, Err(StorageError::Damaged { .. })),
        "{again:?}"
    );
    // A hint is a figure and not a proof: a directory that is not a repository is still refused as such.
    let elsewhere = tmp.0.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    hints.set(&elsewhere, 1);
    for r in [
        d.tree(&elsewhere, &main_view(), None).map(|_| ()),
        d.views(&elsewhere).map(|_| ()),
    ] {
        assert!(
            matches!(&r, Err(StorageError::Refused { kind: Some(k), .. }) if k == "driver:no-repository"),
            "{r:?}"
        );
    }
}

#[test]
fn a_verb_whose_cost_is_flat_does_not_walk_the_store() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new("flat");
    let repo = tmp.0.join("repo");
    let locked = repo.join(".prikk/locked");
    fs::create_dir_all(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    // A directory the walk cannot enter: running as a user for whom the mode does not bind (root) shows nothing.
    let binds = fs::read_dir(&locked).is_err();
    let restore = || fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !binds {
        restore();
        println!("skipping: running as a user for whom the mode does not bind");
        return;
    }
    let fake = shim(
        &tmp.0,
        r#"case "$1" in
  --version) echo 'prikk 0.54.0' ;;
  branch) echo '{"schema_version": "branch-list-v1", "branches": []}' ;;
  *) echo '{}' ;;
esac"#,
    );
    let d = open_shim(&fake).expect("open");
    // `branch` (like `log`) costs the floor whatever the store holds, so it does not measure it.
    let views = retry(|| d.views(&repo));
    // A verb that follows the store measures it, and meets the directory.
    let tree = d.tree(&repo, &main_view(), None);
    restore();
    assert!(views.is_ok(), "{views:?}");
    assert!(
        matches!(tree, Err(StorageError::Damaged { verb: "tree", .. })),
        "{tree:?}"
    );
}

#[test]
fn an_older_prikk_is_refused_at_open() {
    let tmp = TempDir::new("old");
    let old = shim(&tmp.0, "echo 'prikk 0.53.9'");
    let r = open_shim(&old);
    assert!(
        matches!(r, Err(StorageError::UnsupportedVersion { ref found, required }) if found == "prikk 0.53.9" && required == MIN_PRIKK_VERSION),
        "{r:?}"
    );
    let garbage = shim(&tmp.0, "echo 'not prikk'");
    assert!(matches!(
        open_shim(&garbage),
        Err(StorageError::UnsupportedVersion { .. })
    ));
    // A probe that fails or hangs is unavailable, never "older than the floor".
    let failing = shim(&tmp.0, "exit 1");
    assert!(
        matches!(
            open_shim(&failing),
            Err(StorageError::Unavailable {
                verb: "--version",
                ..
            })
        ),
        "a failing probe"
    );
}

#[test]
fn a_doctored_schema_version_is_refused() {
    let tmp = TempDir::new("schema");
    let repo = tmp.0.join("repo");
    fs::create_dir_all(repo.join(".prikk")).unwrap();
    let fake = shim(
        &tmp.0,
        r#"case "$1" in
  --version) echo 'prikk 0.54.0' ;;
  branch) echo '{"schema_version": "branch-list-v9", "branches": []}' ;;
  tree) echo '{"schema_version": "tree-listing-v1"}' ;;
esac"#,
    );
    let d = open_shim(&fake).expect("open");
    let r = retry(|| d.views(&repo));
    assert!(
        matches!(&r, Err(StorageError::UnsupportedSchema { found, .. }) if found == "branch-list-v9"),
        "{r:?}"
    );
    // The right schema name with the wrong shape is damaged output, not a schema refusal.
    let t = retry(|| d.tree(&repo, &main_view(), None));
    assert!(matches!(t, Err(StorageError::Damaged { .. })), "{t:?}");
}

#[test]
fn sandbox_tool_failures_and_deadlines_and_output_bounds_are_typed() {
    let tmp = TempDir::new("tools");
    let repo = tmp.0.join("repo");
    fs::create_dir_all(repo.join(".prikk")).unwrap();
    // A prikk that answers the probe and otherwise sleeps or floods.
    let fake = shim(
        &tmp.0,
        r#"case "$1" in
  --version) echo 'prikk 0.54.0' ;;
  branch) sleep 5 ;;
  tree) yes '{"schema_version": "tree-listing-v1"}' | head -c 300000 ;;
  log) echo "error: something prikk said" >&2; exit 1 ;;
  show) exit 139 ;;
esac"#,
    );
    let d = open_shim(&fake).expect("open");
    let slow = retry(|| {
        d.clone()
            .with_fixed_deadline(Duration::from_millis(300))
            .views(&repo)
    });
    assert!(
        matches!(
            slow,
            Err(StorageError::Unavailable {
                verb: "branch",
                reason: Unavailable::TimedOut { .. }
            })
        ),
        "{slow:?}"
    );
    let flood = retry(|| {
        d.clone()
            .with_output_cap(10_000)
            .tree(&repo, &main_view(), None)
    });
    assert!(
        matches!(
            flood,
            Err(StorageError::Unavailable {
                verb: "tree",
                reason: Unavailable::OutputExceeded
            })
        ),
        "{flood:?}"
    );
    let refused = retry(|| d.history(&repo, &main_view(), None));
    assert!(
        matches!(&refused, Err(StorageError::Refused { verb: "log", message, kind: None, reason: None }) if message == "something prikk said"),
        "{refused:?}"
    );
    // Without a ceiling in play (the knob is off, so a costed one applies; the shim ignores it), a 139 is
    // read as the ceiling, because the ceiling is always set in production. Say so in the detail otherwise.
    let seg = retry(|| d.change(&repo, &"a".repeat(64)));
    assert!(
        matches!(
            seg,
            Err(StorageError::Unavailable {
                reason: Unavailable::CeilingExceeded { .. },
                ..
            })
        ),
        "{seg:?}"
    );

    // bwrap's own failure (the binary gone, a path missing) is a prerequisite problem, not prikk's refusal.
    let failing_bwrap = shim_named(
        &tmp.0,
        "bwrap",
        "case \"$*\" in *true*) exit 0 ;; *) echo \"bwrap: Can't find source path /nope: No such file or directory\" >&2; exit 1 ;; esac",
    );
    let prlimit = shim_named(
        &tmp.0,
        "prlimit",
        "case \"$1\" in --version) exit 0 ;; *) shift; shift; exec \"$@\" ;; esac",
    );
    let sandbox = Sandbox::Bwrap(BwrapConfig {
        bwrap: failing_bwrap.into_os_string(),
        prlimit: prlimit.into_os_string(),
    });
    let d = (0..50)
        .find_map(
            |_| match PrikkCli::open(fake.as_os_str().to_owned(), sandbox.clone(), gate()) {
                Err(StorageError::Unavailable {
                    reason: Unavailable::Spawn(m),
                    ..
                }) if m.contains("Text file busy") => {
                    std::thread::sleep(Duration::from_millis(10));
                    None
                }
                other => Some(other),
            },
        )
        .expect("opened")
        .expect("a confined driver over the shims");
    let r = retry(|| d.views(&repo));
    assert!(
        matches!(&r, Err(StorageError::Unavailable { verb: "branch", reason: Unavailable::Prerequisite(m) }) if m.starts_with("bwrap:")),
        "{r:?}"
    );
}

#[test]
fn a_too_small_ceiling_is_unavailable_never_a_verdict() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    let tmp = TempDir::new("ceiling");
    let (repo, _) = fixture(&tmp);
    // Unconfined, 64 KiB of data: not even the dynamic loader gets through; the ceiling hits prikk itself.
    let d = driver(Sandbox::Unconfined).with_fixed_ceiling(64 * 1024);
    let r = d.verify(&repo);
    assert!(
        matches!(r, Err(StorageError::Unavailable { verb: "verify", reason: Unavailable::CeilingExceeded { bytes } }) if bytes == 64 * 1024),
        "{r:?}"
    );
    // Confined: the death shapes change with the ceiling (measured on bubblewrap 0.13.1: bwrap's own loader
    // fails at 64 KiB with exit 127; prikk faults inside the namespace at 80–96 KiB, which bwrap reports as
    // exit 139; bwrap runs short while setting up at 112–192 KiB and says "Out of memory" with exit 1; the
    // tiny fixture verifies from about 224 KiB). Whatever the shape, a ceiling hit is `CeilingExceeded`
    // and never a refusal, a damaged repository or a verdict.
    if bwrap_ok() {
        let mut hits = 0;
        for kib in [64u64, 96, 128, 192, 4096] {
            let d = driver(Sandbox::default()).with_fixed_ceiling(kib << 10);
            match d.verify(&repo) {
                Err(StorageError::Unavailable {
                    verb: "verify",
                    reason: Unavailable::CeilingExceeded { bytes },
                }) => {
                    assert_eq!(bytes, kib << 10);
                    hits += 1;
                }
                Ok(v) => assert!(v.verdict.ok, "{kib} KiB: {v:?}"),
                other => panic!("{kib} KiB: a ceiling hit must be typed as one, got {other:?}"),
            }
        }
        assert!(hits >= 1, "no ceiling fired in the confined policy");
    } else {
        println!("{SKIP_BWRAP}");
    }
}

#[test]
fn admission_refusals_are_typed() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    let tmp = TempDir::new("admission");
    let (repo, _) = fixture(&tmp);
    // A budget below any ceiling: every read is too large, and the answer names the pool.
    let tiny = PrikkCli::open(
        "prikk",
        Sandbox::Unconfined,
        Admission::new(1 << 20, 1 << 20, Duration::from_millis(10)),
    )
    .expect("open");
    let r = tiny.views(&repo);
    assert!(
        matches!(r, Err(StorageError::TooLarge { verb: "branch", needed, pool }) if needed > pool),
        "{r:?}"
    );
}
