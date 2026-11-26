//! The replay of a bundle (RFC 007 D-1 – D-3; handoff I-T3), with the **real signer and a real prikk**: the wiki is made with its history, one
//! revision a block, every change carrying the source's facts as typed trailers, and it appears only when it is complete. A stop after each
//! step (a panic in the observer runs no cleanup, as `SIGKILL` runs none: `creation_kill.rs` explains it) leaves what the sweep reports and
//! `discard_unfinished_creation` removes, and no reader ever saw the wiki. Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::import::bundle::{
    BundleOptions, BundlePlan, ImportBundleError, ImportStaging, ImportStep, ImportWorld,
    ImportedWiki, import_bundle, plan_bundle,
};
use inklkunn_core::message::{Message, SourceAuthorKind};
use inklkunn_core::read::ReadConfig;
use inklkunn_core::{
    CreationStep, DiscardRefusal, KeyAuthority, NoSweepAudit, Signer, SignerConfig,
    discard_unfinished_creation, sweep_unfinished_creations,
};
use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage};
use inklkunn_signer::WriteLeases;
use inklkunn_store::{
    ActStore, DisplayName, MemoryAudit, MemoryStore, NewWiki, RootLayout, Slug, WikiId, WikiStore,
};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir_in(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-replay-{}-{tag}", std::process::id()));
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

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/interchange")
        .join(rel)
}

/// Everything a replay needs, over real directories.
struct World {
    _tmp: Dir,
    _shm: Dir,
    layout: RootLayout,
    key_dir: PathBuf,
    shm: PathBuf,
    signer: Arc<Signer>,
    storage: Arc<dyn Storage>,
    staging: ImportStaging,
    leases: Arc<inklkunn_core::WikiLeases>,
    audit: Arc<MemoryAudit>,
    store: Arc<MemoryStore>,
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
    let staging = ImportStaging::new();
    let write_leases = WriteLeases::new(layout.clone());
    let signer = Arc::new(
        Signer::open_creating_store(cfg, staging.ledger())
            .expect("signer")
            .with_leases(&write_leases),
    );
    let leases = staging.leases(write_leases, "replay-test");
    let storage: Arc<dyn Storage> = Arc::new(
        PrikkCli::open(
            "prikk",
            Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(2000)),
        )
        .expect("prikk at the floor"),
    );
    World {
        key_dir,
        shm: shm.0.clone(),
        layout,
        signer,
        storage,
        staging,
        leases,
        audit: Arc::new(MemoryAudit::default()),
        store: Arc::new(MemoryStore::new()),
        _tmp: tmp,
        _shm: shm,
    }
}

fn new_wiki(slug: &str, license: &str) -> NewWiki {
    let mut new = NewWiki::new(
        Slug::new(slug).expect("slug"),
        DisplayName::new(slug).expect("name"),
    );
    new.default_license = Some(license.to_owned());
    new
}

fn plan_of(bundle: &Path, license: Option<&str>) -> BundlePlan {
    let options = BundleOptions {
        bundle_license: license.map(str::to_owned),
        ..BundleOptions::default()
    };
    let plan = plan_bundle(bundle, &ReadConfig::default(), &options).expect("plan");
    assert!(plan.is_importable(), "{}", plan.render());
    plan
}

fn run(
    w: &World,
    plan: &BundlePlan,
    slug: &str,
    license: &str,
    observe: &dyn Fn(ImportStep),
) -> Result<ImportedWiki, ImportBundleError> {
    let key = KeyAuthority::for_tests();
    let reads = w
        .staging
        .reads(w.storage.clone(), w.layout.clone(), ReadConfig::default());
    let world = ImportWorld {
        store: &*w.store,
        acts: &*w.store,
        layout: &w.layout,
        signer: &w.signer,
        storage: &w.storage,
        staging: &w.staging,
        leases: w.leases.clone(),
        audit: w.audit.clone(),
        reads: Some(reads),
        authority: &key,
        version: "0.0.0",
        clock: inklkunn_store::record::now_secs,
    };
    import_bundle(
        &world,
        new_wiki(slug, license),
        plan,
        &plan.render(),
        observe,
    )
}

/// What an unfinished creation leaves on disk: anything under the root but the shard directories, any key record, any seed directory.
fn leftovers(w: &World) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let rel = e.path().strip_prefix(root).unwrap().display().to_string();
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

/// The messages of the wiki's blocks, oldest first, parsed.
fn messages(w: &World, id: &WikiId) -> Vec<Message> {
    let repo = w.layout.repo_dir(id);
    let history = w
        .storage
        .history(
            &repo,
            &inklkunn_prikk::Point::View("heads/main".to_owned()),
            None,
        )
        .expect("history");
    history
        .blocks
        .iter()
        .rev()
        .map(|b| {
            let text = &b.patch_messages.first().expect("a patch").message;
            Message::parse(text).unwrap_or_else(|e| panic!("{e}: {text:?}"))
        })
        .collect()
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn a_mediawiki_dump_comes_in_with_its_history_every_revision_a_block_and_the_pages_end_as_the_bundle_has_them()
 {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("mw");
    let bundle = fixture("mediawiki/bundle");
    let plan = plan_of(&bundle, Some("CC-BY-SA-4.0"));
    let expected_revisions = plan.schedule.len();
    assert!(expected_revisions >= 4, "{}", plan.render());
    let seen = std::cell::RefCell::new(Vec::new());
    let done = run(&w, &plan, "garden", "CC-BY-SA-4.0", &|s| {
        seen.borrow_mut().push(s);
        // **No reader can reach the wiki before it is complete**: at every step before the last, the store answers as it does for a
        // wiki that does not exist.
        if s != ImportStep::Stored {
            assert!(
                matches!(
                    w.store.by_slug(&Slug::new("garden").unwrap()),
                    Err(inklkunn_store::StoreError::NotFound)
                ),
                "{s:?}: the wiki was visible"
            );
            assert!(w.store.list().expect("list").is_empty(), "{s:?}");
        }
    })
    .expect("imported");
    assert_eq!(done.revisions, expected_revisions);
    // The steps, in order: the creation's, the staging, the first change (if any), a step a revision, the end, verified, kept, stored.
    let steps = seen.into_inner();
    assert_eq!(
        steps.first(),
        Some(&ImportStep::Creation(CreationStep::Marked))
    );
    assert_eq!(steps.last(), Some(&ImportStep::Stored));
    let revision_steps: Vec<usize> = steps
        .iter()
        .filter_map(|s| match s {
            ImportStep::Revision(n) => Some(*n),
            _ => None,
        })
        .collect();
    assert_eq!(revision_steps, (1..=expected_revisions).collect::<Vec<_>>());
    // The wiki is stored, with every act: the creation, the first change if any, each revision, the change at the end.
    let record = w
        .store
        .by_slug(&Slug::new("garden").unwrap())
        .expect("stored");
    assert_eq!(record.id, done.record.id);
    assert_eq!(record.default_license, "CC-BY-SA-4.0");
    let acts = w.store.system_acts(&record.id).expect("acts");
    let blocks = messages(&w, &record.id);
    eprintln!(
        "BLOCKS {}: {}",
        blocks.len(),
        blocks
            .iter()
            .map(|m| m.summary().as_str().to_owned())
            .collect::<Vec<_>>()
            .join(" / ")
    );
    assert_eq!(acts.len(), blocks.len(), "an act behind every block");
    assert_eq!(
        blocks.len(),
        1 + usize::from(done.first_change) + expected_revisions + done.final_changes
    );
    assert!(acts.iter().all(|a| a.end.is_some()));
    // Each replayed revision's block carries the source's facts, as data; the first block is the wiki's own.
    let replayed: Vec<&Message> = blocks.iter().filter(|m| m.source().is_some()).collect();
    assert_eq!(replayed.len(), expected_revisions);
    for m in &replayed {
        assert_eq!(m.kind(), inklkunn_core::message::Kind::Import);
        let n = m
            .notice()
            .expect("every change of the import has the notice");
        assert_eq!(n.checks(), inklkunn_core::message::ChecksState::Verified);
        assert_eq!(
            n.license(),
            inklkunn_core::message::LicenseWord::DeclaredByAdmin
        );
        assert_eq!(n.source_name(), Some("Garden Notes"));
        assert!(matches!(
            m.proposed_by(),
            inklkunn_core::message::Actor::System(_)
        ));
    }
    // The order is the plan's: by the source's time.
    let times: Vec<i64> = replayed.iter().map(|m| m.source().unwrap().at()).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
    // The final pages equal the bundle's files, byte for byte.
    for (path, _) in &plan.page_digests {
        assert_eq!(
            read_text(&done.repo.join(path)),
            read_text(&bundle.join(path)),
            "{path}"
        );
    }
    // The report is kept beside the repository, and nothing else of the import is left.
    let kept = read_text(&w.layout.import_report(&record.id));
    assert!(
        kept.contains("Garden Notes") || kept.contains("replayed"),
        "{kept}"
    );
    assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
    assert!(
        leftovers(&w).iter().all(|p| !p.ends_with(".creating")),
        "{:?}",
        leftovers(&w)
    );
}

/// The ids whose creation mark exists now.
fn marks(w: &World) -> Vec<WikiId> {
    w.layout.creation_marks().expect("marks")
}

/// Import `bundle` and stop (panic) right after `step`.
fn stop_after(w: &World, plan: &BundlePlan, step: ImportStep) {
    let r = catch_unwind(AssertUnwindSafe(|| {
        run(w, plan, "killed", "CC0-1.0", &|s| {
            if s == step {
                panic!("stopped after {s:?}");
            }
        })
    }));
    assert!(r.is_err(), "the import was not stopped after {step:?}");
}

/// The steps an import of this plan takes, in order (a throwaway world runs it to the end).
fn steps_of(plan: &BundlePlan) -> Vec<ImportStep> {
    let w = world("steps");
    let seen = std::cell::RefCell::new(Vec::new());
    run(&w, plan, "killed", "CC0-1.0", &|s| {
        seen.borrow_mut().push(s)
    })
    .expect("imported");
    seen.into_inner()
}

#[test]
fn an_import_stopped_after_any_step_leaves_nothing_a_reader_can_reach_and_what_the_sweep_keeps_a_person_discards()
 {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let plan = plan_of(&fixture("inklkunn/bundle"), None);
    let steps = steps_of(&plan);
    // The steps that matter are all there: the creation's five, the staging, the first change, a step a revision, verified, kept,
    // stored.
    for needed in [
        ImportStep::Creation(CreationStep::Marked),
        ImportStep::Creation(CreationStep::Prepared),
        ImportStep::Creation(CreationStep::RepositoryCreated),
        ImportStep::Creation(CreationStep::FilesWritten),
        ImportStep::Creation(CreationStep::Sealed),
        ImportStep::Staged,
        ImportStep::FirstChange,
        ImportStep::Revision(1),
        ImportStep::Verified,
        ImportStep::ReportKept,
        ImportStep::Stored,
    ] {
        assert!(
            steps.contains(&needed),
            "{needed:?} is not a step of {steps:?}"
        );
    }
    // Silence the panic messages of the stops; they are the test.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for step in &steps {
        let step = *step;
        let w = world(
            &format!("kill-{step:?}")
                .replace(['(', ')', ' '], "-")
                .to_lowercase(),
        );
        stop_after(&w, &plan, step);
        let before = leftovers(&w);
        let stored = step == ImportStep::Stored;
        // **No reader ever saw the wiki**: the store has it only when the last step was done.
        assert_eq!(
            w.store.list().expect("list").len(),
            usize::from(stored),
            "{step:?}"
        );
        assert_eq!(
            w.store.by_slug(&Slug::new("killed").unwrap()).is_ok(),
            stored,
            "{step:?}"
        );
        assert!(
            before.iter().any(|p| p.ends_with(".creating")),
            "{step:?}: no creation mark among {before:?}"
        );
        // The next start.
        let report = sweep_unfinished_creations(
            &*w.store,
            &*w.store,
            &w.layout,
            &*w.signer,
            &*w.storage,
            false,
            &NoSweepAudit,
        )
        .expect("sweep");
        let sealed_a_block = !matches!(
            step,
            ImportStep::Creation(
                CreationStep::Marked
                    | CreationStep::Prepared
                    | CreationStep::RepositoryCreated
                    | CreationStep::FilesWritten
            )
        );
        if stored {
            // Complete: the record and every act were stored together; only the mark was left.
            assert_eq!(report.completed.len(), 1, "{step:?}: {report:?}");
            assert!(report.left_behind.is_empty() && report.removed.is_empty());
            let record = w.store.list().expect("list").remove(0);
            let acts = w.store.system_acts(&record.id).expect("acts");
            assert!(
                acts.len() > 1 && acts.iter().all(|a| a.end.is_some()),
                "{step:?}"
            );
            assert!(
                w.storage
                    .verify(&w.layout.repo_dir(&record.id))
                    .expect("verify")
                    .verdict
                    .ok
            );
            // …and no person may discard it.
            assert_eq!(
                discard_unfinished_creation(
                    &*w.store,
                    &w.layout,
                    &*w.signer,
                    &record.id,
                    &NoSweepAudit
                ),
                Err(DiscardRefusal::NoUnfinishedCreation),
                "the sweep took the mark"
            );
            continue;
        }
        if !sealed_a_block {
            // Nothing a person could want: the sweep removes it whole.
            assert_eq!(report.removed.len(), 1, "{step:?}: {report:?}");
            assert!(
                leftovers(&w).is_empty(),
                "{step:?}: left {:?}",
                leftovers(&w)
            );
            continue;
        }
        // A sealed block is a history: the sweep reports it and removes nothing (R9), at this start and at every other.
        assert_eq!(report.left_behind.len(), 1, "{step:?}: {report:?}");
        assert!(report.removed.is_empty(), "{step:?}");
        assert!(
            report
                .left_behind
                .first()
                .is_some_and(|(_, why)| why.contains("sealed block")),
            "{step:?}: {report:?}"
        );
        assert_eq!(
            leftovers(&w),
            before,
            "{step:?}: the sweep changed what it refused"
        );
        // A person discards it, and **what the sweep would have removed is all that goes**: nothing is left.
        let id = report.left_behind[0].0.clone();
        assert_eq!(marks(&w), vec![id.clone()]);
        let cleanup =
            discard_unfinished_creation(&*w.store, &w.layout, &*w.signer, &id, &NoSweepAudit)
                .expect("discarded");
        assert_eq!(cleanup, inklkunn_core::Cleanup::Removed, "{step:?}");
        assert!(
            leftovers(&w).is_empty(),
            "{step:?}: left {:?}",
            leftovers(&w)
        );
        assert!(w.store.list().expect("list").is_empty());
        // Once more is nothing to discard.
        assert_eq!(
            discard_unfinished_creation(&*w.store, &w.layout, &*w.signer, &id, &NoSweepAudit),
            Err(DiscardRefusal::NoUnfinishedCreation)
        );
    }
    std::panic::set_hook(hook);
}
