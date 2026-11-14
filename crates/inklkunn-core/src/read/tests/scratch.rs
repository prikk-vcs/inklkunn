//! The extracts in the scratch directory (handoff 005 corrections, sixth round, K55): an extract is live while its lock is
//! held, a sweep removes an extract only after taking that lock without waiting, and an extract that vanished under a
//! reader is an error, not an empty wiki. The sweep from another PID namespace is `inklkunn-signer/tests/
//! extract_pid_namespace.rs` (the one crate whose tests may start a process). No test needs prikk.

use std::path::{Path, PathBuf};

use inklkunn_signer::flock::{Acquired, try_exclusive};

use super::super::scratch::{Extract, sweep_leftovers};
use super::super::testing::World;
use super::*;
use crate::import::{ImportError, wiki_state};

/// A pid no process has: above the kernel's largest.
const NO_SUCH_PID: u32 = u32::MAX;

fn scratch_of(w: &World) -> PathBuf {
    let scratch = w.layout.scratch_dir(&w.wiki.id);
    std::fs::create_dir_all(&scratch).expect("scratch");
    scratch
}

fn entries(scratch: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(scratch)
        .expect("scratch")
        .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
        .collect();
    names.sort();
    names
}

/// An extract some process left: `<scratch>/<name>/pages/Junk.md`, and its lock file, free.
fn plant(scratch: &Path, name: &str) -> PathBuf {
    let dir = plant_directory(scratch, name);
    drop(try_exclusive(&scratch.join(format!("{name}.lock"))).expect("lock file"));
    dir
}

/// The directory alone (an extract of an earlier build had no lock file).
fn plant_directory(scratch: &Path, name: &str) -> PathBuf {
    let dir = scratch.join(name);
    std::fs::create_dir_all(dir.join("pages")).expect("leftover");
    std::fs::write(dir.join("pages/Junk.md"), "x").expect("junk");
    dir
}

/// The pages of exactly the block `block_id`, read from one extract of it under the wiki's build lock: the shape of
/// work these tests need (an extract that a second service can sweep around), now that the acceptance reads the title
/// index of the prepared state instead (K54 U-l).
fn entries_at_block(
    service: &ReadService,
    wiki: &WikiRecord,
    block_id: &str,
) -> Result<Vec<IndexEntry>, ReadError> {
    service.with_block_tree(wiki, block_id, |root, config| {
        entries_from_tree(root, config.index_max_files)
            .map_err(|e| ReadError::CouldNotBeReadNow(Cause::Scratch(format!("{e:?}"))))
    })?
}

/// A build of `w` that sweeps (any read does).
fn build(w: &World) {
    entries_at_block(&w.service, &w.wiki, &w.fake().block_id()).expect("entries");
}

#[test]
fn the_sweep_removes_what_no_one_holds_and_only_that() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = scratch_of(&w);
    // Left by processes that are gone: a directory with its free lock file, a lock file alone (a crash before the
    // extract, or after the directory was removed), a directory alone (the earlier build's, or a sweep that was
    // stopped between the two), and the earlier build's name.
    let gone = plant(&scratch, &format!("index-{NO_SUCH_PID}-123-0"));
    let lock_only = scratch.join(format!("index-{NO_SUCH_PID}-123-1.lock"));
    drop(try_exclusive(&lock_only).expect("lock file"));
    let no_lock = plant_directory(&scratch, &format!("index-{NO_SUCH_PID}-123-2"));
    let earlier_build = plant_directory(&scratch, &format!("index-{NO_SUCH_PID}-7"));
    // Not an extract's: no sweep touches them, whatever they hold.
    let not_an_extract = plant(&scratch, "index-nobody");
    let other = plant(&scratch, "export-1-2");
    build(&w);
    assert!(!gone.exists(), "a directory nobody holds is removed");
    assert!(!lock_only.exists(), "a lock file with no directory goes");
    assert!(!no_lock.exists(), "a directory with no lock file goes");
    assert!(!earlier_build.exists(), "and the earlier build's name");
    assert!(
        !scratch
            .join(format!("index-{NO_SUCH_PID}-123-0.lock"))
            .exists(),
        "a lock file goes with its directory"
    );
    assert!(
        not_an_extract.exists() && other.exists(),
        "a name that is no extract's is not ours to remove"
    );
}

#[test]
fn an_extract_whose_lock_is_held_is_never_touched_whoever_holds_it_and_whatever_its_name() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = scratch_of(&w);
    // A live extract of this very process, one a process named with a pid of its own would own, and one in the
    // earlier build's shape: only the lock says they are live.
    let mine = Extract::begin(&scratch).expect("mine");
    let theirs = format!("index-{NO_SUCH_PID}-123-0");
    let theirs_dir = plant_directory(&scratch, &theirs);
    let Acquired::Held(their_lock) =
        try_exclusive(&scratch.join(format!("{theirs}.lock"))).expect("their lock")
    else {
        panic!("a new lock was busy");
    };
    // Big trees, slow to remove: they must be whole afterwards, not partly removed.
    std::fs::create_dir_all(mine.root().join("pages")).expect("pages");
    for n in 0..200 {
        std::fs::write(mine.root().join(format!("pages/P{n}.md")), "x").expect("page");
        std::fs::write(theirs_dir.join(format!("pages/P{n}.md")), "x").expect("page");
    }
    for _ in 0..3 {
        build(&w);
        sweep_leftovers(&scratch);
    }
    assert_eq!(
        std::fs::read_dir(mine.root().join("pages"))
            .expect("mine")
            .count(),
        200,
        "a live extract is whole"
    );
    assert_eq!(
        std::fs::read_dir(theirs_dir.join("pages"))
            .expect("theirs")
            .count(),
        201,
        "so is another's"
    );
    // The lock goes, and so the extract does, at the next sweep.
    drop(their_lock);
    sweep_leftovers(&scratch);
    assert!(!theirs_dir.exists(), "released, it is a leftover");
    assert!(mine.root().exists(), "mine is held still");
}

#[test]
fn an_extract_owns_a_lock_file_beside_it_from_before_the_directory_until_after_it() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = scratch_of(&w);
    let extract = Extract::begin(&scratch).expect("begin");
    let lock = scratch.join(format!("{}.lock", extract.name()));
    assert!(
        lock.is_file(),
        "the lock file is there before the directory"
    );
    assert!(!extract.root().exists(), "materialize makes the directory");
    assert!(
        matches!(try_exclusive(&lock), Ok(Acquired::Busy { .. })),
        "and it is held"
    );
    std::fs::create_dir_all(extract.root().join("pages")).expect("directory");
    let other = Extract::begin(&scratch).expect("a second");
    assert_ne!(extract.name(), other.name());
    drop(other);
    assert!(extract.root().exists(), "a second begin leaves it");
    drop(extract);
    assert!(
        entries(&scratch).is_empty(),
        "the directory and the lock file are gone: {:?}",
        entries(&scratch)
    );
}

#[test]
fn a_directory_that_will_not_go_keeps_its_lock_file_and_the_next_sweep_finishes_it() {
    use std::os::unix::fs::PermissionsExt;
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = scratch_of(&w);
    let extract = Extract::begin(&scratch).expect("begin");
    let name = extract.name().to_owned();
    let stuck = extract.root().join("sealed");
    std::fs::create_dir_all(&stuck).expect("directory");
    std::fs::write(stuck.join("file"), "x").expect("file");
    std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o555)).expect("read-only");
    if std::fs::write(stuck.join("other"), "x").is_ok() {
        // A superuser removes it anyway: there is no directory that will not go to test with.
        println!("skipping: a read-only directory is removable here");
        std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o755)).expect("mode");
        return;
    }
    drop(extract);
    assert!(
        scratch.join(&name).exists(),
        "the directory could not be removed"
    );
    assert!(
        scratch.join(format!("{name}.lock")).exists(),
        "so the lock file stays, to tell the next sweep it is a leftover and not nothing"
    );
    std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o755)).expect("mode");
    sweep_leftovers(&scratch);
    assert!(
        entries(&scratch).is_empty(),
        "the sweep finished it: {:?}",
        entries(&scratch)
    );
}

#[test]
fn a_link_under_an_extracts_name_is_not_followed_and_not_removed() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = scratch_of(&w);
    let elsewhere = plant_directory(&scratch, "elsewhere");
    let link = scratch.join(format!("index-{NO_SUCH_PID}-1-1"));
    std::os::unix::fs::symlink(&elsewhere, &link).expect("link");
    sweep_leftovers(&scratch);
    assert!(
        elsewhere.join("pages/Junk.md").exists(),
        "what it points to is untouched"
    );
    assert!(
        std::fs::symlink_metadata(&link).is_ok(),
        "and the link is no extract: it stays"
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
            let theirs =
                entries_at_block(&other, &w.wiki, &block).expect("the other's extract is whole");
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
            let mine = entries_at_block(&w.service, &w.wiki, &block)
                .expect("the first's extract is whole");
            assert_eq!(titles(&mine), want);
            entries_from_tree(root, config.index_max_files).expect("the other's, after")
        })
        .expect("the other's is whole");
    assert_eq!(titles(&seen), want);
    let scratch = scratch_of(&w);
    assert!(
        entries(&scratch).is_empty(),
        "each removed its own, and its lock file: {:?}",
        entries(&scratch)
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

#[test]
fn an_imports_extract_is_beside_a_servers_and_neither_removes_the_other() {
    let w = World::pages(&[("Home", &[], "x\n"), ("Guide", &[], "y\n")]);
    w.fake().list_views();
    let block = w.fake().block_id();
    let config = ReadConfig::default();
    let scratch = scratch_of(&w);
    // The import (`wiki_state`) while the server's extract is live: the server's is whole afterwards, and what a
    // crashed import left goes.
    let crashed = plant(&scratch, &format!("index-{NO_SUCH_PID}-5-0"));
    let state = w
        .service
        .with_block_tree(&w.wiki, &block, |root, _| {
            let state = wiki_state(&*w.counting, &w.layout, &w.wiki, &config).expect("import");
            assert!(root.join("pages").is_dir(), "the server's extract is whole");
            state
        })
        .expect("the server's read");
    assert_eq!(state.tip, block, "the plan is pinned to the tip");
    assert!(!crashed.exists(), "the import swept a crashed one");
    // And the server's build while the import's extract is live: the import's `materialize` is where its extract
    // is claimed, so claim one by hand and build around it.
    let import = Extract::begin(&scratch).expect("an import's extract");
    std::fs::create_dir_all(import.root().join("pages")).expect("pages");
    std::fs::write(import.root().join("pages/Home.md"), "x").expect("page");
    build(&w);
    assert!(
        import.root().join("pages/Home.md").exists(),
        "the server's build left the import's extract whole"
    );
    drop(import);
    assert!(entries(&scratch).is_empty(), "{:?}", entries(&scratch));
}

#[test]
fn an_imports_extract_is_bounded_by_the_operators_limits_and_never_left_behind() {
    let w = World::pages(&[("Home", &[], "x\n"), ("Guide", &[], "y\n")]);
    w.fake().list_views();
    let scratch = scratch_of(&w);
    // Room enough is enough; fewer files or bytes than the wiki has is refused by prikk's extract, as it is for a read.
    let at = |index_max_files: u64, index_max_bytes: u64| {
        wiki_state(
            &*w.counting,
            &w.layout,
            &w.wiki,
            &ReadConfig {
                index_max_files,
                index_max_bytes,
                ..ReadConfig::default()
            },
        )
    };
    assert!(at(100, 1 << 20).is_ok(), "the wiki fits");
    assert!(
        matches!(at(1, 1 << 20), Err(ImportError::Storage(_))),
        "two files are over the operator's cap of one"
    );
    assert!(
        matches!(at(100, 1), Err(ImportError::Storage(_))),
        "and over the operator's bytes"
    );
    assert!(
        entries(&scratch).is_empty(),
        "a refused extract leaves nothing: {:?}",
        entries(&scratch)
    );
}

#[test]
fn a_sweep_running_all_the_while_never_takes_an_import_extract_from_under_it() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let w = World::pages(&[("Home", &[], "x\n"), ("Guide", &[], "y\n")]);
    w.fake().list_views();
    let scratch = scratch_of(&w);
    let config = ReadConfig::default();
    let whole = wiki_state(&*w.counting, &w.layout, &w.wiki, &config).expect("the first");
    // `materialize` takes a moment, so a sweep meets the extract before, during and after its files are written.
    w.fake().slow_materialize(Duration::from_millis(3));
    let done = AtomicBool::new(false);
    // The sweeper stops when the rounds are over or when one of them fails: a failed assertion must end the test, not
    // leave the scope waiting for a thread that never stops.
    struct Stops<'a>(&'a AtomicBool);
    impl Drop for Stops<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    std::thread::scope(|scope| {
        let sweeping = scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                sweep_leftovers(&scratch);
            }
        });
        let stops = Stops(&done);
        for round in 0..25 {
            let state = wiki_state(&*w.counting, &w.layout, &w.wiki, &config)
                .unwrap_or_else(|e| panic!("round {round}: {e}"));
            assert_eq!(state, whole, "round {round}: both pages are read");
        }
        drop(stops);
        sweeping.join().expect("the sweeper");
    });
    assert!(entries(&scratch).is_empty(), "{:?}", entries(&scratch));
}
