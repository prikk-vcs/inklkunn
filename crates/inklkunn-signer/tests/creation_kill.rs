//! A real `SIGKILL` at each prikk call of a wiki's creation (handoff 003 T7; hold note IS-13): the creation
//! mark, the repository, the keys and the first change, made the way `inklkunn-core`'s *create wiki* makes
//! them, in a child process that a **stand-in `prikk`** holds inside a chosen call; the parent kills it, and
//! what a start-up sweep then finds is removed whole. The core crate may not spawn a process (the boundary
//! gate), so the real kills live here; core's `tests/creation_kill.rs` stops the other steps.
//!
//! Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use inklkunn_prikk::Sandbox;
use inklkunn_signer::message::{Message, Summary, SystemLabel};
use inklkunn_signer::{
    AcceptanceLedger, AcceptanceToken, CreationMark, ExpectedMessage, KeyAuthority, LedgerError,
    RepositoryBootstrap, Signer, SignerConfig, ViewCreationAct, ViewCreationFacts, find_unfinished,
};
use inklkunn_store::{RootLayout, WikiId};

const SECRET: &str = "an operator secret for the creation kill tests, long enough";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir_under(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-ck-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

fn real_prikk() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("prikk"))
            .find(|c| c.is_file())
    })
}

struct NoLedger;

impl AcceptanceLedger for NoLedger {
    fn pending(&self, _: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(None)
    }
    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

/// A stand-in `prikk` that counts its calls per verb and, when `block-<verb>` names the call's index, tells the
/// parent it is there (`blocked-<verb>-<n>`) and sleeps until killed. Every other call goes to the real prikk.
fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
verb="$1"
n="$(cat "$CTL/count-$verb" 2>/dev/null || echo 0)"
echo "$((n + 1))" > "$CTL/count-$verb"
if [ -e "$CTL/block-$verb" ] && [ "$(cat "$CTL/block-$verb")" = "$n" ]; then
  : > "$CTL/blocked-$verb-$n"
  sleep 60
fi
exec "{real}" "$@"
"#,
        ctl = ctl.display(),
        real = real.display()
    );
    let path = ctl.join("prikk");
    std::fs::write(&path, script).expect("stand-in");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

fn config(
    prikk: &Path,
    root: &RootLayout,
    key_dir: &Path,
    secret: &Path,
    shm: &Path,
) -> SignerConfig {
    let mut cfg = SignerConfig::new(
        prikk,
        Sandbox::Unconfined,
        root.clone(),
        key_dir,
        secret,
        shm,
    );
    cfg.cheap_derivation = true;
    cfg
}

/// The child: mark, directories, repository and keys, a file, the first change. Blocks in the call the control
/// files name until the parent kills it.
#[test]
fn child_creates_a_wiki_and_blocks() {
    let Some(spec) = std::env::var_os("INKLKUNN_CK_CHILD") else {
        return; // an ordinary run of the suite: not the child
    };
    let spec: HashMap<String, String> = serde_json::from_str(&spec.to_string_lossy()).unwrap();
    let get = |k: &str| spec.get(k).unwrap().clone();
    let root = RootLayout::new(get("root"));
    let signer = Signer::open_creating_store(
        config(
            Path::new(&get("prikk")),
            &root,
            Path::new(&get("key_dir")),
            Path::new(&get("secret")),
            Path::new(&get("shm")),
        ),
        Arc::new(NoLedger),
    )
    .expect("signer");
    let id = WikiId::parse(&get("wiki")).unwrap();
    let _mark = CreationMark::begin(&root, &id).expect("mark");
    let repo = root.prepare(&id).expect("directories");
    let authority = KeyAuthority::for_tests();
    signer
        .create_repository(&repo, &authority)
        .expect("repository");
    std::fs::create_dir_all(repo.join("pages")).unwrap();
    std::fs::write(repo.join("pages/Help.md"), "---\ntitle: Help\n---\n\nHi.\n").unwrap();
    let first = Message::system(
        Summary::from_text("Create the wiki").unwrap(),
        SystemLabel::inklkunn(),
    );
    signer
        .commit_and_seal(&repo, &first.to_text())
        .expect("first change");
}

/// Every entry on disk that belongs to the wiki being created (all but an empty shard directory), the key
/// records and the seed directories.
fn leftovers(root: &RootLayout, key_dir: &Path, shm: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let is_shard = e.path().parent() == Some(root) && e.path().is_dir();
            if is_shard {
                walk(&e.path(), root, out);
            } else {
                out.push(e.path().strip_prefix(root).unwrap().display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(root.root(), root.root(), &mut out);
    for e in std::fs::read_dir(key_dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".key.json") {
            out.push(format!("keys/{name}"));
        }
    }
    for e in std::fs::read_dir(shm).into_iter().flatten().flatten() {
        out.push(format!("seeds/{}", e.file_name().to_string_lossy()));
    }
    out.sort();
    out
}

/// The kill points: the prikk verb and the index of the call (`key` is called to generate the author key, the
/// maintainer key and to derive the public key; `log` first to look for a history, then to read the block back).
const POINTS: [(&str, u32); 9] = [
    ("init", 0),
    ("key", 0),
    ("key", 1),
    ("key", 2),
    ("trust", 0),
    ("log", 0),
    ("commit", 0),
    ("seal", 0),
    ("log", 1),
];

#[test]
fn a_sigkill_inside_any_prikk_call_of_a_creation_leaves_nothing_the_sweep_cannot_remove_whole() {
    if std::env::var_os("INKLKUNN_CK_CHILD").is_some() {
        return; // the child runs `child_creates_a_wiki_and_blocks`, not this test
    }
    let Some(real) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return;
    };
    for (verb, n) in POINTS {
        let tag = format!("{verb}{n}");
        let tmp = dir_under(&std::env::temp_dir(), &tag);
        let shm = dir_under(Path::new("/dev/shm"), &tag);
        let ctl = tmp.0.join("ctl");
        std::fs::create_dir_all(&ctl).unwrap();
        let prikk = write_standin(&ctl, &real);
        let root = RootLayout::new(tmp.0.join("root"));
        let key_dir = tmp.0.join("keys");
        let secret = tmp.0.join("secret");
        std::fs::write(&secret, format!("{SECRET}\n")).unwrap();
        // The parent's signer makes the key store, so the child does not race it.
        let parent = Signer::open_creating_store(
            config(&prikk, &root, &key_dir, &secret, &shm.0),
            Arc::new(NoLedger),
        )
        .expect("parent signer");
        let wiki = WikiId::new();
        std::fs::write(ctl.join(format!("block-{verb}")), n.to_string()).unwrap();
        let spec = serde_json::json!({
            "prikk": prikk, "root": root.root(), "key_dir": key_dir, "secret": secret,
            "shm": shm.0, "wiki": wiki.as_str(),
        });
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "child_creates_a_wiki_and_blocks",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("INKLKUNN_CK_CHILD", spec.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("child");
        let marker = ctl.join(format!("blocked-{verb}-{n}"));
        let deadline = Instant::now() + Duration::from_secs(60);
        while !marker.exists() {
            assert!(
                Instant::now() < deadline,
                "{verb}#{n}: the child never reached the call"
            );
            if let Ok(Some(status)) = child.try_wait() {
                panic!("{verb}#{n}: the child ended early ({status}) without reaching the call");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // While the child lives, its creation is live: a sweep must leave it alone.
        let found = find_unfinished(&root).unwrap();
        assert!(
            found.dead.is_empty(),
            "{verb}#{n}: a live creation was claimed"
        );
        assert_eq!(found.live, vec![wiki.clone()], "{verb}#{n}");
        drop(found);
        child.kill().expect("SIGKILL");
        let _ = child.wait();

        // What the kill left: the mark at least, and whatever the steps before it made.
        let left = leftovers(&root, &key_dir, &shm.0);
        assert!(
            left.iter().any(|p| p.ends_with(".creating")),
            "{verb}#{n}: no mark among {left:?}"
        );

        // The next start: the sweep claims the dead creation and removes it whole, in the order core's does.
        let found = find_unfinished(&root).unwrap();
        assert!(found.live.is_empty(), "{verb}#{n}: {:?}", found.live);
        assert_eq!(found.dead.len(), 1, "{verb}#{n}");
        for mark in found.dead {
            assert_eq!(mark.id(), &wiki);
            parent
                .discard_unfinished(&root.repo_dir(&wiki))
                .expect("key record");
            root.discard_unfinished(&wiki).expect("directories");
            mark.finish().expect("mark");
        }
        // The killed call's seeds on the tmpfs go at the next signer start.
        let restarted = Signer::open(
            config(&real, &root, &key_dir, &secret, &shm.0),
            Arc::new(NoLedger),
        )
        .expect("restart");
        let _ = restarted.leftover_call_directories_removed();
        let after = leftovers(&root, &key_dir, &shm.0);
        assert!(after.is_empty(), "{verb}#{n}: left behind {after:?}");
        assert!(find_unfinished(&root).unwrap().dead.is_empty());
    }
}
