//! The extracts in the scratch directory (handoff 005 corrections, fifth round, K51): one process's sweep removes only
//! what a process that is gone left, and an extract that vanished under a reader is an error, not an empty wiki. No test
//! needs prikk.

use std::path::{Path, PathBuf};

use super::super::testing::World;
use super::*;
use crate::lease::start_time_of;

/// A pid no process has: above the kernel's largest.
const NO_SUCH_PID: u32 = u32::MAX;

fn scratch_of(w: &World) -> PathBuf {
    let scratch = w.layout.scratch_dir(&w.wiki.id);
    std::fs::create_dir_all(&scratch).expect("scratch");
    scratch
}

/// An extract some process left: `<scratch>/<name>/pages/Junk.md`.
fn plant(scratch: &Path, name: &str) -> PathBuf {
    let dir = scratch.join(name);
    std::fs::create_dir_all(dir.join("pages")).expect("leftover");
    std::fs::write(dir.join("pages/Junk.md"), "x").expect("junk");
    dir
}

/// A build of `w` that sweeps (any read does).
fn build(w: &World) {
    w.service
        .page_entries_at_block(&w.wiki, &w.fake().block_id())
        .expect("entries");
}

#[test]
fn the_sweep_removes_what_a_process_that_is_gone_left_and_only_that() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = scratch_of(&w);
    let gone = plant(&scratch, &format!("index-{NO_SUCH_PID}-123-0"));
    let earlier_build = plant(&scratch, &format!("index-{NO_SUCH_PID}-7"));
    let mine = plant(
        &scratch,
        &format!(
            "index-{}-{}-9999999",
            std::process::id(),
            start_time_of("self").expect("start")
        ),
    );
    let not_an_extract = plant(&scratch, "index-nobody");
    let other = plant(&scratch, "export-1-2");
    build(&w);
    assert!(!gone.exists(), "a gone process's extract is removed");
    assert!(
        !earlier_build.exists(),
        "and the earlier build's name, by its pid alone"
    );
    assert!(mine.exists(), "a live process's extract, this one, stays");
    assert!(
        not_an_extract.exists() && other.exists(),
        "a name that is no extract's is not ours to remove"
    );
}

#[test]
fn another_live_processs_extract_stays_and_a_reused_pid_does_not_keep_one() {
    // The init process is another process, and there as long as the machine is.
    let start = start_time_of("1").expect("the init process's start");
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = scratch_of(&w);
    let theirs = plant(&scratch, &format!("index-1-{start}-0"));
    // The same pid with another start time is another process that was given the number since.
    let reused = plant(&scratch, &format!("index-1-{}-1", start + 1));
    build(&w);
    assert!(theirs.exists(), "a live process's extract is not swept");
    assert!(
        !reused.exists(),
        "a pid another process now has keeps nothing"
    );
}

#[test]
fn two_services_on_one_layout_never_judge_each_other_on_an_empty_tree() {
    let w = World::pages(&[("Home", &[], "x\n"), ("Guide", &[], "y\n")]);
    let other = World::service_over(&w.counting, &w.store, &w.layout, ReadConfig::default());
    let block = w.fake().block_id();
    let titles = |entries: &[IndexEntry]| {
        let mut t: Vec<String> = entries.iter().map(|e| e.title.clone()).collect();
        t.sort();
        t
    };
    let want = ["Guide".to_owned(), "Home".to_owned()];
    // One service extracts while the other builds (and sweeps, and extracts) in the middle of its work.
    let seen = w
        .service
        .with_block_tree(&w.wiki, &block, |root, config| {
            let before = entries_from_tree(root, config.index_max_files).expect("mine, before");
            let theirs = other
                .page_entries_at_block(&w.wiki, &block)
                .expect("the other's extract is whole");
            assert_eq!(titles(&theirs), want, "the other is not judged on nothing");
            assert!(root.join("pages").is_dir(), "mine is still there");
            let after = entries_from_tree(root, config.index_max_files).expect("mine, after");
            (before, after)
        })
        .expect("mine is whole");
    assert_eq!(titles(&seen.0), want);
    assert_eq!(titles(&seen.1), want);
    // And the other way about.
    let seen = other
        .with_block_tree(&w.wiki, &block, |root, config| {
            let mine = w
                .service
                .page_entries_at_block(&w.wiki, &block)
                .expect("the first's extract is whole");
            assert_eq!(titles(&mine), want);
            entries_from_tree(root, config.index_max_files).expect("the other's, after")
        })
        .expect("the other's is whole");
    assert_eq!(titles(&seen), want);
    let scratch = scratch_of(&w);
    assert_eq!(
        std::fs::read_dir(scratch).expect("scratch").count(),
        0,
        "each removed its own"
    );
}

#[test]
fn an_extract_that_vanished_while_it_was_read_is_could_not_be_read_now_and_never_an_empty_tree() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let block = w.fake().block_id();
    // The work reads the tree as another process's sweep might have left it: gone.
    let outcome = w.service.with_block_tree(&w.wiki, &block, |root, config| {
        std::fs::remove_dir_all(root).expect("remove the extract");
        entries_from_tree(root, config.index_max_files)
    });
    // What the work found (no `pages/`, which reads as no pages) is no answer: the call says the wiki could not be
    // read now.
    assert!(
        matches!(
            outcome,
            Err(ReadError::CouldNotBeReadNow(Cause::Scratch(_)))
        ),
        "{outcome:?}"
    );
}
