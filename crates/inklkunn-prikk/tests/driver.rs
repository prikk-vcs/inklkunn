//! The driver against a real repository at the floor (handoff 001 T3). Every test self-skips with a
//! visible line when prikk is not on `PATH`; the confined tests also skip when bubblewrap cannot run here.
//! The fixture is `tests/fixtures/wiki-repo` (see its README), copied to a fresh temporary directory per
//! test so nothing in the crate is ever written.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use inklkunn_prikk::sandbox::probe_bwrap;
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

        let main = Point::View("heads/main".into());
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
        let history = d.history(&repo, "heads/main", None).expect("history");
        assert_eq!(history.blocks.len(), 2, "{history:?}");
        let newest = history.blocks.first().expect("tip");
        let root = history.blocks.get(1).expect("root");
        assert!(newest.update_seq > root.update_seq, "newest first");
        assert_eq!(root.kind, "Root");
        let message = &newest.patch_messages.first().expect("message").message;
        assert!(message.starts_with("Proposal: 019a0000"), "{message}");

        let one = d.history(&repo, "heads/main", Some(1)).expect("limited");
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
        let main = Point::View("heads/main".into());
        let plan = d.plan(&repo, &main).expect("plan");
        assert!(!plan.written);
        assert_eq!(plan.file_count, 6, "{plan:?}");

        let into = scratch.join("out");
        let done = d
            .materialize(
                &repo,
                &main,
                &into,
                ExtractBounds {
                    max_bytes: None,
                    max_files: None,
                },
            )
            .expect("materialize");
        assert!(done.written);
        assert!(into.join("pages/Home.md").is_file());
        assert!(into.join("attachments/diagram.svg").is_file());

        // The bound refuses before the first byte.
        let bounded = d.materialize(
            &repo,
            &main,
            &scratch.join("bounded"),
            ExtractBounds {
                max_bytes: Some(10),
                max_files: None,
            },
        );
        assert!(
            matches!(bounded, Err(StorageError::Refused { .. })),
            "{bounded:?}"
        );
        assert!(!scratch.join("bounded").exists());

        // An existing target is refused by the driver itself.
        let exists = d.materialize(
            &repo,
            &main,
            &into,
            ExtractBounds {
                max_bytes: None,
                max_files: None,
            },
        );
        assert!(matches!(exists, Err(StorageError::Refused { .. })));

        // Confined: a directory outside the mounts does not exist inside the sandbox, so nothing is written there.
        if confined {
            let elsewhere = TempDir::new("elsewhere");
            let outside = elsewhere.0.join("out");
            // The driver mounts `into.parent()` as the scratch; make the parent a sibling the sandbox never sees
            // by pointing the call at a nested path whose parent exists on the host but is not mounted.
            let nested_parent = elsewhere.0.join("deeper");
            fs::create_dir_all(&nested_parent).unwrap();
            let r = d.materialize(
                &repo,
                &main,
                &nested_parent.join("x"),
                ExtractBounds {
                    max_bytes: None,
                    max_files: None,
                },
            );
            // `nested_parent` *is* mounted (it is the scratch), so this succeeds; the real check is that the
            // repository itself stayed read-only:
            assert!(r.is_ok(), "{r:?}");
            let into_repo = d.materialize(
                &repo,
                &main,
                &repo.join("inside"),
                ExtractBounds {
                    max_bytes: None,
                    max_files: None,
                },
            );
            assert!(
                into_repo.is_err(),
                "writing into the repository must fail: {into_repo:?}"
            );
            assert!(!repo.join("inside").exists());
            drop(outside);
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
    let main = Point::View("heads/main".into());
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
        matches!(
            not_a_repo,
            Err(StorageError::Refused { verb: "branch", .. })
        ),
        "{not_a_repo:?}"
    );
    let too_big = d.read(&repo, &main, "pages/Home.md", 4);
    assert!(
        matches!(too_big, Err(StorageError::Refused { verb: "cat", .. })),
        "{too_big:?}"
    );
}

/// A shell script standing in for prikk.
fn shim(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join("prikk");
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[test]
fn an_older_prikk_is_refused_at_open() {
    let tmp = TempDir::new("old");
    let old = shim(&tmp.0, "echo 'prikk 0.53.9'");
    let r = PrikkCli::open(old.as_os_str().to_owned(), Sandbox::Unconfined, gate());
    assert!(
        matches!(r, Err(StorageError::UnsupportedVersion { ref found, required }) if found == "prikk 0.53.9" && required == MIN_PRIKK_VERSION),
        "{r:?}"
    );
    let garbage = shim(&tmp.0, "echo 'not prikk'");
    assert!(matches!(
        PrikkCli::open(garbage.as_os_str().to_owned(), Sandbox::Unconfined, gate()),
        Err(StorageError::UnsupportedVersion { .. })
    ));
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
    let d = PrikkCli::open(fake.as_os_str().to_owned(), Sandbox::Unconfined, gate()).expect("open");
    let r = d.views(&repo);
    assert!(
        matches!(&r, Err(StorageError::UnsupportedSchema { found, .. }) if found == "branch-list-v9"),
        "{r:?}"
    );
    // The right schema name with the wrong shape is damaged output, not a schema refusal.
    let t = d.tree(&repo, &Point::View("heads/main".into()), None);
    assert!(matches!(t, Err(StorageError::Damaged { .. })), "{t:?}");
}

#[test]
fn a_too_small_ceiling_is_unavailable_never_a_verdict() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    let tmp = TempDir::new("ceiling");
    let (repo, _) = fixture(&tmp);
    // 64 KiB of data: not even the dynamic loader gets through. Unconfined, so the ceiling hits prikk itself.
    let d = driver(Sandbox::Unconfined).with_fixed_ceiling(64 * 1024);
    let r = d.verify(&repo);
    assert!(
        matches!(r, Err(StorageError::Unavailable { verb: "verify", reason: Unavailable::CeilingExceeded { bytes } }) if bytes == 64 * 1024),
        "{r:?}"
    );
}

#[test]
fn admission_refusals_are_typed() {
    if !prikk_on_path() {
        println!("{SKIP_PRIKK}");
        return;
    }
    let tmp = TempDir::new("admission");
    let (repo, _) = fixture(&tmp);
    // A budget below any cost: every read is too large.
    let tiny = PrikkCli::open(
        "prikk",
        Sandbox::Unconfined,
        Admission::new(1 << 20, 1 << 20, Duration::from_millis(10)),
    )
    .expect("open");
    let r = tiny.views(&repo);
    assert!(
        matches!(r, Err(StorageError::TooLarge { verb: "branch", .. })),
        "{r:?}"
    );
}
