//! The replay of a bundle (RFC 007 D-1 – D-3; handoff I-T3), with the **real signer and a real prikk**: the wiki is made with its history, one
//! revision a block, every change carrying the source's facts as typed trailers, and it appears only when it is complete. A stop after each
//! step (a panic in the observer runs no cleanup, as `SIGKILL` runs none: `creation_kill.rs` explains it) leaves what the sweep reports and
//! `discard_unfinished_creation` removes, and no reader ever saw the wiki. Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/replay.rs"]
mod replay;

use std::panic::{AssertUnwindSafe, catch_unwind};

use inklkunn_core::import::bundle::{BundlePlan, ImportStep};
use inklkunn_core::message::Message;
use inklkunn_core::{
    CreationStep, DiscardRefusal, NoSweepAudit, discard_unfinished_creation,
    sweep_unfinished_creations,
};
use inklkunn_store::{ActStore, Slug, WikiId, WikiStore};
use replay::{World, fixture, leftovers, messages, plan_of, prikk_on_path, read_text, run, world};

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
