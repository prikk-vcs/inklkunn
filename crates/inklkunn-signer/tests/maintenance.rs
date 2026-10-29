//! The keyless writable mode against a real prikk (handoff 003 T7; hold notes IS-8, IS-9): `archive export` of a
//! wiki's repository yields an archive prikk itself verifies and that holds no seed; the checkpoint refresh
//! brings back the record `extract` needs after it was lost (a restore, a cleared cache). Self-skips with a
//! visible line when prikk is not on `PATH`; runs confined (bubblewrap) as well as unconfined where bubblewrap
//! works.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use inklkunn_prikk::Sandbox;
use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_signer::Maintenance;
use inklkunn_store::{RootLayout, WikiId};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn real_prikk() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("prikk"))
            .find(|c| c.is_file())
    })
}

fn run(prikk: &Path, repo: &Path, env: &[(&str, String)], args: &[&str]) -> std::process::Output {
    let mut c = Command::new(prikk);
    c.args(args).current_dir(repo);
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().expect("prikk runs")
}

fn ok(prikk: &Path, repo: &Path, env: &[(&str, String)], args: &[&str]) -> String {
    let out = run(prikk, repo, env, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A wiki-shaped repository with `blocks` sealed blocks, at the root layout's place for `wiki`, made with prikk
/// directly; returns the seeds' text (the archive must not hold either).
fn repository(
    prikk: &Path,
    root: &RootLayout,
    keys: &Path,
    wiki: &WikiId,
    blocks: u32,
) -> Vec<String> {
    let repo = root.prepare(wiki).unwrap();
    ok(prikk, &repo, &[], &["init"]);
    std::fs::create_dir_all(keys).unwrap();
    let author = keys.join("author.seed");
    let maintainer = keys.join("maintainer.seed");
    for s in [&author, &maintainer] {
        ok(
            prikk,
            &repo,
            &[],
            &["key", "generate", "--out", s.to_str().unwrap()],
        );
    }
    let public = ok(
        prikk,
        &repo,
        &[],
        &["key", "public", "--seed-file", maintainer.to_str().unwrap()],
    );
    let public = public
        .trim()
        .strip_prefix("public key:")
        .unwrap()
        .trim()
        .to_owned();
    ok(
        prikk,
        &repo,
        &[],
        &[
            "trust",
            "maintainer",
            "add",
            "--key-id",
            "ck-maintainer",
            "--public-key",
            &public,
        ],
    );
    let author_id = std::fs::read_to_string(keys.join("author.seed.key-id")).unwrap();
    let env = [
        ("PRIKK_AUTHOR_SEED_FILE", author.display().to_string()),
        ("PRIKK_AUTHOR_KEY_ID", author_id.trim().to_owned()),
        (
            "PRIKK_MAINTAINER_SEED_FILE",
            maintainer.display().to_string(),
        ),
        ("PRIKK_MAINTAINER_KEY_ID", "ck-maintainer".to_owned()),
    ];
    std::fs::create_dir_all(repo.join("pages")).unwrap();
    for i in 0..blocks {
        std::fs::write(repo.join(format!("pages/P{i}.md")), format!("page {i}\n")).unwrap();
        ok(
            prikk,
            &repo,
            &env,
            &["commit", "-m", &format!("change {i}")],
        );
        ok(prikk, &repo, &env, &["seal", "--allow-no-audit"]);
    }
    vec![
        std::fs::read_to_string(&author).unwrap().trim().to_owned(),
        std::fs::read_to_string(&maintainer)
            .unwrap()
            .trim()
            .to_owned(),
    ]
}

fn policies() -> Vec<Sandbox> {
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        vec![Sandbox::default(), Sandbox::Unconfined]
    } else {
        println!("skipping: bubblewrap cannot confine a process here; running unconfined");
        vec![Sandbox::Unconfined]
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn an_archive_export_is_verified_by_prikk_itself_holds_no_seed_and_changes_nothing_but_prikks_caches()
 {
    let Some(prikk) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return;
    };
    for (n, sandbox) in policies().into_iter().enumerate() {
        let tmp =
            Dir(std::env::temp_dir().join(format!("inklkunn-maint-it-{}-{n}", std::process::id())));
        let _ = std::fs::remove_dir_all(&tmp.0);
        std::fs::create_dir_all(&tmp.0).unwrap();
        let root = RootLayout::new(tmp.0.join("root"));
        let wiki = WikiId::new();
        let seeds = repository(&prikk, &root, &tmp.0.join("keys"), &wiki, 5);
        let repo = root.repo_dir(&wiki);
        let before = ok(&prikk, &repo, &[], &["log", "--format", "json"]);

        let m = Maintenance::new(&prikk, sandbox.clone(), root.clone());
        let dest = tmp.0.join("out/archive.prikk");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let done = m.archive_export(&wiki, &dest).expect("the export");
        assert!(done.bytes > 0);
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), done.bytes);
        assert_eq!(
            std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // prikk, independently of inklkunn, finds the archive sound.
        let verdict = ok(
            &prikk,
            &repo,
            &[],
            &[
                "archive",
                "verify",
                dest.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(verdict.contains("\"ok\": true"), "{verdict}");
        // No seed is in the archive, and none was ever mounted.
        let bytes = std::fs::read(&dest).unwrap();
        for seed in &seeds {
            assert!(
                !contains(&bytes, seed.as_bytes()),
                "a seed is in the archive"
            );
        }
        // The history is as it was; the scratch copy is gone; a second export to the same file is refused.
        assert_eq!(before, ok(&prikk, &repo, &[], &["log", "--format", "json"]));
        assert_eq!(
            std::fs::read_dir(root.scratch_dir(&wiki)).unwrap().count(),
            0
        );
        let again = m.archive_export(&wiki, &dest).unwrap_err();
        assert!(again.to_string().contains("already exists"), "{again}");
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    }
}

#[test]
fn the_checkpoint_refresh_brings_back_the_record_extract_needs() {
    let Some(prikk) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return;
    };
    for (n, sandbox) in policies().into_iter().enumerate() {
        let tmp =
            Dir(std::env::temp_dir().join(format!("inklkunn-ckpt-it-{}-{n}", std::process::id())));
        let _ = std::fs::remove_dir_all(&tmp.0);
        std::fs::create_dir_all(&tmp.0).unwrap();
        let root = RootLayout::new(tmp.0.join("root"));
        let wiki = WikiId::new();
        // Beyond 63 blocks `extract` anchors on the checkpoint record (ledger PK-24).
        repository(&prikk, &root, &tmp.0.join("keys"), &wiki, 70);
        let repo = root.repo_dir(&wiki);
        let record = repo.join(".prikk/cache/replay-verified-blocks.v1");
        assert!(record.is_file(), "seal records the checkpoint");
        std::fs::remove_file(&record).unwrap();
        let into = tmp.0.join("x1");
        let refused = run(
            &prikk,
            &repo,
            &[],
            &[
                "extract",
                "--ref",
                "heads/main",
                into.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(
            !refused.status.success(),
            "extract anchored without the record"
        );
        assert!(
            String::from_utf8_lossy(&refused.stdout).contains("no-checkpoint-in-reach"),
            "{}",
            String::from_utf8_lossy(&refused.stdout)
        );
        // A read-only verify does not bring it back (what the driver can do); the keyless mode does.
        let m = Maintenance::new(&prikk, sandbox, root.clone());
        let v = m.refresh_checkpoint(&wiki).expect("the refresh");
        assert!(v.verdict.ok);
        assert!(record.is_file(), "the record was not re-recorded");
        let into = tmp.0.join("x2");
        ok(
            &prikk,
            &repo,
            &[],
            &[
                "extract",
                "--ref",
                "heads/main",
                into.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(into.join("pages/P69.md").is_file());
    }
}
