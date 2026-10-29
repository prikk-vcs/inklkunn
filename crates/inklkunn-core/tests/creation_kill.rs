//! A creation that is stopped at any step leaves either a complete wiki or nothing that survives the next
//! start's sweep (handoff 003 T7; hold note IS-13), with the **real signer** (a sealed key record in a real key
//! store) and a real prikk. Self-skips with a visible line when prikk is not on `PATH`.
//!
//! "Stopped" is a panic raised by the observer after each step: it runs no cleanup, as `SIGKILL` runs none
//! (the cleanup in `create_wiki` is explicit code on its error paths, never a destructor). A real `SIGKILL`
//! inside each prikk call of the signer's part of the creation is `inklkunn-signer`'s `tests/creation_kill.rs`,
//! which may spawn a process; this crate may not (the boundary gate).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::{
    AcceptanceLedger, AcceptanceToken, CreationStep, ExpectedMessage, KeyAuthority, LedgerError,
    RepositoryBootstrap, Signer, SignerConfig, ViewCreationAct, ViewCreationFacts, create_wiki,
    create_wiki_observed, sweep_unfinished_creations,
};
use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage};
use inklkunn_store::{DisplayName, MemoryStore, NewWiki, RootLayout, Slug, WikiId, WikiStore};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir_in(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-kill-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
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

struct World {
    _tmp: Dir,
    _shm: Dir,
    layout: RootLayout,
    key_dir: PathBuf,
    shm: PathBuf,
    secret: PathBuf,
    signer: Signer,
    storage: PrikkCli,
    store: MemoryStore,
}

fn world(tag: &str) -> World {
    let tmp = dir_in(&std::env::temp_dir(), tag);
    let shm = dir_in(Path::new("/dev/shm"), tag);
    let secret = tmp.0.join("secret");
    std::fs::write(&secret, "an operator secret long enough for the tests").expect("secret");
    let layout = RootLayout::new(tmp.0.join("root"));
    let key_dir = tmp.0.join("keys");
    let mut cfg = SignerConfig::new(
        "prikk",
        Sandbox::Unconfined,
        layout.clone(),
        &key_dir,
        &secret,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let signer = Signer::open_creating_store(cfg, Arc::new(NoLedger)).expect("signer");
    let storage = PrikkCli::open(
        "prikk",
        Sandbox::Unconfined,
        Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
    )
    .expect("prikk at the floor");
    World {
        key_dir,
        shm: shm.0.clone(),
        secret,
        layout,
        signer,
        storage,
        store: MemoryStore::new(),
        _tmp: tmp,
        _shm: shm,
    }
}

fn new_wiki(slug: &str) -> NewWiki {
    NewWiki::new(
        Slug::new(slug).expect("slug"),
        DisplayName::new(slug).expect("name"),
    )
}

/// Everything on disk that belongs to an unfinished creation: any entry under the root but the (empty) shard
/// directories, any key record, any seed directory.
fn leftovers(w: &World) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let rel = e.path().strip_prefix(root).unwrap().display().to_string();
            // A shard directory is just a shard: only what is inside it counts.
            let is_shard = e.path().parent() == Some(root) && e.path().is_dir();
            if is_shard {
                walk(&e.path(), root, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(w.layout.root(), w.layout.root(), &mut out);
    for entry in std::fs::read_dir(&w.key_dir)
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".key.json") {
            out.push(format!("keys/{name}"));
        }
    }
    for entry in std::fs::read_dir(&w.shm).into_iter().flatten().flatten() {
        out.push(format!("seeds/{}", entry.file_name().to_string_lossy()));
    }
    out.sort();
    out
}

/// Create a wiki and stop (panic) right after `step`; return what that left, or `None` if prikk is missing.
fn stop_after(w: &World, step: CreationStep) {
    let key = KeyAuthority::for_tests();
    let r = catch_unwind(AssertUnwindSafe(|| {
        create_wiki_observed(
            &w.store,
            &w.layout,
            &w.signer,
            &w.storage,
            new_wiki("killed"),
            "0.0.0",
            &key,
            &|s| {
                if s == step {
                    panic!("stopped after {s:?}");
                }
            },
        )
    }));
    assert!(r.is_err(), "the creation was not stopped after {step:?}");
}

#[test]
fn a_creation_stopped_after_any_step_leaves_a_complete_wiki_or_nothing_that_survives_the_sweep() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // Silence the panic messages of the stops; they are the test.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut seen = Vec::new();
    for step in CreationStep::ALL {
        let w = world(&format!("step-{step:?}").to_lowercase());
        stop_after(&w, step);
        let stored = w.store.list().expect("list").len();
        let before = leftovers(&w);
        // The mark is the reference: while the creation is unfinished, a mark exists.
        assert!(
            before.iter().any(|p| p.ends_with(".creating")),
            "{step:?}: no creation mark among {before:?}"
        );
        assert_eq!(
            stored,
            usize::from(step == CreationStep::Stored),
            "{step:?}"
        );
        // The next start.
        let report = sweep_unfinished_creations(&w.store, &w.layout, &w.signer).expect("sweep");
        assert!(report.left_behind.is_empty(), "{step:?}: {report:?}");
        if step == CreationStep::Stored {
            // Complete: the record was stored; only the mark is removed, and the wiki is whole.
            assert_eq!(report.completed.len(), 1, "{step:?}: {report:?}");
            assert!(report.removed.is_empty(), "{step:?}");
            let record = w.store.list().expect("list").remove(0);
            let repo = w.layout.repo_dir(&record.id);
            assert!(w.storage.verify(&repo).expect("verify").verdict.ok);
            let after = leftovers(&w);
            assert!(
                after.iter().all(|p| !p.ends_with(".creating")),
                "{step:?}: {after:?}"
            );
            assert!(
                after.iter().any(|p| p.ends_with(&format!("{}", record.id))),
                "the wiki's repository was removed: {after:?}"
            );
        } else {
            // Nothing: no record, no directory, no lock file, no mark, no key record.
            assert_eq!(report.removed.len(), 1, "{step:?}: {report:?}");
            assert!(report.completed.is_empty(), "{step:?}");
            let after = leftovers(&w);
            assert!(after.is_empty(), "{step:?}: left {after:?}");
            assert!(w.store.list().expect("list").is_empty());
        }
        // A second start finds nothing more.
        let again = sweep_unfinished_creations(&w.store, &w.layout, &w.signer).expect("sweep");
        assert!(again.is_empty(), "{step:?}: {again:?}");
        seen.push(step);
    }
    std::panic::set_hook(hook);
    assert_eq!(seen, CreationStep::ALL.to_vec());
}

#[test]
fn a_creation_that_is_still_running_is_not_swept_and_goes_on_to_finish() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("live");
    let key = KeyAuthority::for_tests();
    let seen = std::cell::RefCell::new(Vec::new());
    let created = create_wiki_observed(
        &w.store,
        &w.layout,
        &w.signer,
        &w.storage,
        new_wiki("live"),
        "0.0.0",
        &key,
        &|step| {
            // Another process starting now (a server beside the command line) sweeps at every step.
            let report = sweep_unfinished_creations(&w.store, &w.layout, &w.signer).expect("sweep");
            assert!(
                report.removed.is_empty(),
                "{step:?}: a live creation was removed"
            );
            assert!(report.left_behind.is_empty(), "{step:?}: {report:?}");
            seen.borrow_mut().push((step, report.live.len()));
        },
    )
    .expect("the creation finished");
    // At every step but the last the sweep saw it as live; after the record is stored the mark is still held
    // until the end of the call.
    let seen = seen.into_inner();
    assert_eq!(seen.len(), CreationStep::ALL.len());
    assert!(seen.iter().all(|(_, live)| *live == 1), "{seen:?}");
    assert!(w.storage.verify(&created.repo).expect("verify").verdict.ok);
    assert!(leftovers(&w).iter().all(|p| !p.ends_with(".creating")));
}

/// The real signer, except that it cannot remove the key record of an unfinished creation.
struct CannotDiscard<'a>(&'a Signer);

impl RepositoryBootstrap for CannotDiscard<'_> {
    fn create_repository(
        &self,
        repo: &Path,
        authority: &KeyAuthority,
    ) -> Result<inklkunn_signer::KeyIds, inklkunn_signer::SignerError> {
        self.0.create_repository(repo, authority)
    }
    fn commit_and_seal(
        &self,
        repo: &Path,
        message: &str,
    ) -> Result<inklkunn_signer::SealedIds, inklkunn_signer::SignerError> {
        self.0.commit_and_seal(repo, message)
    }
    fn discard_unfinished(&self, _: &Path) -> Result<(), inklkunn_signer::SignerError> {
        Err(inklkunn_signer::SignerError::Unavailable(
            "the key store is not writable".to_owned(),
        ))
    }
}

#[test]
fn a_sweep_that_cannot_remove_everything_keeps_the_mark_and_the_next_start_finishes_the_job() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let w = world("stuck");
    stop_after(&w, CreationStep::Sealed);
    std::panic::set_hook(hook);
    let stuck = CannotDiscard(&w.signer);
    let report = sweep_unfinished_creations(&w.store, &w.layout, &stuck).expect("sweep");
    assert_eq!(report.left_behind.len(), 1, "{report:?}");
    assert!(report.removed.is_empty());
    assert!(
        report
            .left_behind
            .first()
            .is_some_and(|(_, why)| why.contains("key record")),
        "the operator is told what is left: {report:?}"
    );
    let still = leftovers(&w);
    assert!(
        still.iter().any(|p| p.ends_with(".creating")),
        "the mark must stay while something is left: {still:?}"
    );
    assert!(still.iter().any(|p| p.starts_with("keys/")), "{still:?}");
    // The next start, with a working key store, finishes it.
    let report = sweep_unfinished_creations(&w.store, &w.layout, &w.signer).expect("sweep");
    assert_eq!(report.removed.len(), 1, "{report:?}");
    assert!(leftovers(&w).is_empty(), "{:?}", leftovers(&w));
}

#[test]
fn a_refused_creation_makes_no_mark_and_a_finished_one_leaves_none() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("clean");
    let key = KeyAuthority::for_tests();
    let first = create_wiki(
        &w.store,
        &w.layout,
        &w.signer,
        &w.storage,
        new_wiki("one"),
        "0.0.0",
        &key,
    )
    .expect("created");
    assert!(leftovers(&w).iter().all(|p| !p.ends_with(".creating")));
    let again = create_wiki(
        &w.store,
        &w.layout,
        &w.signer,
        &w.storage,
        new_wiki("one"),
        "0.0.0",
        &key,
    );
    assert!(again.is_err(), "the slug is taken");
    let after = leftovers(&w);
    assert!(
        after.iter().all(|p| !p.ends_with(".creating")),
        "a refused request made a mark: {after:?}"
    );
    assert!(w.store.get(&first.record.id).is_ok());
    assert!(w.secret.is_file());
    let _: WikiId = first.record.id;
}
