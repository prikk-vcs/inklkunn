//! The full listing of views shows a closed one (handoff 005 K12; review finding K-i): a closed branch keeps its name,
//! in any case, so the name check of a new view must see it. The driver's `all_views` (`branch list --all`) is run
//! against a repository made from nothing with the real prikk, a branch closed with it, and prikk's own refusal to
//! make the name again is pinned beside it. Self-skips with a visible line when prikk is not on `PATH`. (The driver's
//! own tests may not name a write verb, so this lives where the writes do.)
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage, Views};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

/// A fresh directory under the system temp dir; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("inklkunn-closed-{}-{tag}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A repository made from nothing with the real prikk (a trusted maintainer key, one sealed block), and a command
/// runner for it: `prikk <args>` in the repository with the maintainer and author seeds in the environment.
struct Made {
    _tmp: TempDir,
    repo: PathBuf,
    seed: PathBuf,
}

impl Made {
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new("prikk")
            .args(args)
            .current_dir(&self.repo)
            .env("PRIKK_MAINTAINER_SEED_FILE", &self.seed)
            .env("PRIKK_AUTHOR_SEED_FILE", &self.seed)
            .output()
            .expect("prikk runs")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "prikk {args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

fn made(tag: &str) -> Made {
    let tmp = TempDir::new(tag);
    let repo = tmp.0.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let seed = tmp.0.join("maintainer.seed");
    let made = Made {
        _tmp: tmp,
        repo,
        seed,
    };
    made.ok(&["init", "."]);
    made.ok(&["key", "generate", "--out", made.seed.to_str().unwrap()]);
    let out = made.ok(&["key", "public", "--seed-file", made.seed.to_str().unwrap()]);
    let public = out
        .lines()
        .find_map(|l| l.strip_prefix("public key: "))
        .expect("the public key");
    let key_id = std::fs::read_to_string(format!("{}.key-id", made.seed.display())).unwrap();
    made.ok(&[
        "trust",
        "maintainer",
        "add",
        "--key-id",
        key_id.trim(),
        "--public-key",
        public.trim(),
    ]);
    std::fs::write(made.repo.join("a.txt"), "hello\n").unwrap();
    made.ok(&["commit", "--ref", "heads/main", "-m", "first"]);
    made.ok(&["seal", "--allow-no-audit", "--ref", "heads/main"]);
    made
}

fn names(views: Views) -> Vec<(String, bool)> {
    views
        .views
        .into_iter()
        .map(|v| (v.name, v.closed))
        .collect()
}

#[test]
fn the_full_listing_of_views_shows_a_closed_one_and_the_default_listing_does_not() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let m = made("listing");
    m.ok(&["branch", "create", "heads/a_", "--from", "heads/main"]);
    m.ok(&["branch", "create", "heads/open", "--from", "heads/main"]);
    m.ok(&["branch", "close", "heads/a_"]);
    let d = PrikkCli::open(
        "prikk",
        Sandbox::Unconfined,
        Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
    )
    .expect("a prikk at the floor");
    let repo: &Path = &m.repo;
    // The open views only, as ever.
    assert_eq!(
        names(d.views(repo).unwrap()),
        [
            ("heads/main".to_owned(), false),
            ("heads/open".to_owned(), false)
        ]
    );
    // Every view, the closed one marked.
    assert_eq!(
        names(d.all_views(repo).unwrap()),
        [
            ("heads/a_".to_owned(), true),
            ("heads/main".to_owned(), false),
            ("heads/open".to_owned(), false)
        ]
    );
    // And prikk keeps the closed one's name, in any case: the reason the listing must show it.
    let again = m.run(&["branch", "create", "heads/a_", "--from", "heads/main"]);
    assert!(!again.status.success());
    let other_case = m.run(&["branch", "create", "heads/A_", "--from", "heads/main"]);
    assert!(!other_case.status.success());
    assert!(
        String::from_utf8_lossy(&other_case.stderr).contains("case-insensitive"),
        "{}",
        String::from_utf8_lossy(&other_case.stderr)
    );
}
