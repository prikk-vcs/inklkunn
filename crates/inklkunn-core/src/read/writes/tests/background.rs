//! The writes index built in the background and bounded by time (0.4.0 readiness correction P-5; P-3's L-9 half): a step builds the
//! index toward the tip with no reader, stops at the tip and when the hide is lifted, goes on after a restart, fails without
//! building anything wrong and retries; a request's own part ends at its time budget (a clock that ticks by itself) and at the
//! backstop; a reader that meets an index dropped under it gives no answer; and reads are never wrong while a builder runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use inklkunn_store::{DerivedStore, WikiRecord};

use super::*;

/// Blocks: 1 (the pages as first written), 2 ..= `n + 1` (an edit of Bar each), `n + 2` (Foo's secret, which is hidden).
pub(super) fn long(n: u64, adjust: impl FnOnce(ReadService) -> ReadService) -> Rig {
    let r = rig_with(
        &[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))],
        adjust,
    );
    for i in 0..n {
        r.w.fake()
            .set("pages/Bar.md", &bar(&format!("revision {i}")));
    }
    r.w.fake().set("pages/Foo.md", SECRET);
    r.hide(n + 2);
    r
}

fn head(r: &Rig) -> Option<u64> {
    r.w.store
        .writes_state(&r.w.wiki.id, &r.w.wiki.default_view)
        .unwrap()
        .map(|s| s.head_number)
}

/// Steps until the build says it is idle; the shows of each step.
fn steps_to_idle(r: &Rig) -> Vec<u64> {
    let mut shows = Vec::new();
    for _ in 0..100 {
        let (step, calls) = r.w.counted(|| r.service.background_step(&r.w.wiki));
        match step {
            BackgroundStep::Idle => return shows,
            BackgroundStep::Progress => shows.push(calls.change),
            BackgroundStep::Failed(e) => panic!("a step failed: {e}"),
        }
    }
    panic!("the build did not end");
}

#[test]
fn the_background_builds_the_index_to_the_tip_one_bounded_step_at_a_time_with_no_reader() {
    let r = long(40, |s| s);
    // Forty-two blocks, sixteen a step: three steps, each within its bound, and then it is idle.
    let steps = steps_to_idle(&r);
    assert_eq!(steps, vec![16, 16, 10], "the shows of each step");
    assert_eq!(head(&r), Some(42), "the index reaches the tip");
    // No reader has built anything, and reads of old blocks need no `show` and answer.
    assert_eq!(r.service.reader_advances(), 0);
    let (_, calls) = r.w.counted(|| {
        assert!(r.is_hidden(42, "pages/Foo.md"));
        assert!(r.shows(20, "pages/Bar.md"));
        assert!(r.shows(1, "pages/Foo.md"));
    });
    assert_eq!(calls.change, 0, "the reads built nothing: {calls:?}");
    assert_eq!(r.service.reader_advances(), 0);
}

#[test]
fn a_step_is_idle_at_the_tip_and_when_nothing_is_hidden_or_the_hide_is_lifted_and_builds_nothing_then()
 {
    // Nothing hidden: nothing to build, and not a `show` is asked.
    let none = rig(&[("pages/Foo.md", &foo("x"))]);
    for i in 0..20 {
        none.w.fake().set("pages/Foo.md", &foo(&format!("v{i}")));
    }
    let (step, calls) = none
        .w
        .counted(|| none.service.background_step(&none.w.wiki));
    assert!(matches!(step, BackgroundStep::Idle), "{step:?}");
    assert_eq!(calls.change, 0, "{calls:?}");
    assert_eq!(head(&none), None);

    // A hide in force: a step builds; the hide is lifted: the next is idle, and builds nothing more (what was built stays).
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    for i in 0..40 {
        r.w.fake()
            .set("pages/Bar.md", &bar(&format!("revision {i}")));
    }
    let hide = r.hide(2);
    assert!(matches!(
        r.service.background_step(&r.w.wiki),
        BackgroundStep::Progress
    ));
    assert_eq!(head(&r), Some(16));
    r.unhide(&hide, 2);
    let (step, calls) = r.w.counted(|| r.service.background_step(&r.w.wiki));
    assert!(matches!(step, BackgroundStep::Idle), "{step:?}");
    assert_eq!(calls.change, 0, "{calls:?}");
    assert_eq!(head(&r), Some(16), "what was built is kept");
    // The same hide in force again: the build goes on from there, and at the tip it is idle (asking again costs no `show`).
    r.hide(2);
    let rest = steps_to_idle(&r);
    assert_eq!(rest, vec![16, 9], "from block 17 to the tip, 41");
    assert_eq!(head(&r), Some(41));
    let (step, calls) = r.w.counted(|| r.service.background_step(&r.w.wiki));
    assert!(matches!(step, BackgroundStep::Idle));
    assert_eq!(calls.change, 0);
}

#[test]
fn a_restart_goes_on_where_the_index_stopped_and_does_not_build_again_what_was_built() {
    let r = long(40, |s| s);
    assert!(matches!(
        r.service.background_step(&r.w.wiki),
        BackgroundStep::Progress
    ));
    assert_eq!(head(&r), Some(16));
    // A restart: a new service over the same repository and store (the index is derived and kept in the store, nothing is held).
    let again = service_of(&r.w);
    let mut shows = 0;
    for _ in 0..10 {
        let (step, calls) = r.w.counted(|| again.background_step(&r.w.wiki));
        match step {
            BackgroundStep::Idle => break,
            BackgroundStep::Progress => shows += calls.change,
            BackgroundStep::Failed(e) => panic!("{e}"),
        }
    }
    assert_eq!(
        shows, 26,
        "only blocks 17 to 42: the first sixteen were not asked again"
    );
    assert_eq!(head(&r), Some(42));
}

#[test]
fn a_step_that_fails_builds_nothing_wrong_and_the_next_one_retries() {
    let r = long(40, |s| s);
    assert!(matches!(
        r.service.background_step(&r.w.wiki),
        BackgroundStep::Progress
    ));
    r.w.fake().fail_change(Some(StorageError::Refused {
        verb: "show",
        message: "the repository cannot be read now".to_owned(),
        kind: None,
        reason: None,
    }));
    let step = r.service.background_step(&r.w.wiki);
    assert!(
        matches!(
            step,
            BackgroundStep::Failed(ReadError::CouldNotBeReadNow(_))
        ),
        "{step:?}"
    );
    assert_eq!(head(&r), Some(16), "a failed step added nothing");
    // A reader meanwhile is not given a text of a block the index has not reached.
    assert!(
        r.is_unchecked(30, "pages/Bar.md")
            || matches!(
                r.service
                    .text_at(&r.w.wiki, None, &block_id(30), "pages/Bar.md"),
                Err(ReadError::CouldNotBeReadNow(_))
            )
    );
    r.w.fake().fail_change(None);
    let rest = steps_to_idle(&r);
    assert_eq!(rest, vec![16, 10]);
    assert_eq!(head(&r), Some(42));
    assert!(r.is_hidden(42, "pages/Foo.md"));
}

#[test]
fn the_default_view_is_built_first_and_then_the_other_open_views() {
    let r = long(20, |s| s);
    // A second view with a block of its own (the double's block of the second view is number 1,000,100).
    r.w.fake()
        .add_view_with("heads/v1", &[("pages/Foo.md".to_owned(), foo("a view"))]);
    r.w.fake()
        .show(1_000_100, shows(vec![op("edit-text", &["pages/Foo.md"])]));
    let steps = steps_to_idle(&r);
    // 22 blocks of the default view: a step of 16 and one of 6; then the other view's one block.
    assert_eq!(
        steps,
        vec![16, 6, 1],
        "the default view first, then the other"
    );
    assert_eq!(head(&r), Some(22));
    assert!(
        r.w.store
            .writes_state(&r.w.wiki.id, "heads/v1")
            .unwrap()
            .is_some(),
        "the other view is built too"
    );
}

// ------------------------------------------------------------------------------------ the time budget of a request

#[test]
fn a_request_builds_at_least_one_block_whatever_the_clock_says_and_goes_on_with_each_request() {
    // A budget of nothing and a clock that is a second further at every look: the time is up at the first look after a `show`.
    let r = long(10, |s| {
        s.with_writes_budget(Duration::ZERO, ticking(Duration::from_secs(1)))
    });
    for expected_head in [1, 2, 3] {
        let (got, calls) = r.w.counted(|| {
            r.service
                .text_at(&r.w.wiki, None, &block_id(3), "pages/Bar.md")
        });
        if expected_head < 3 {
            assert!(matches!(got, Err(ReadError::NotCheckedYet)), "{got:?}");
        }
        assert_eq!(calls.change, 1, "one block a request: {calls:?}");
        assert_eq!(head(&r), Some(expected_head));
    }
    assert!(
        r.shows(3, "pages/Bar.md"),
        "the third request reached block 3"
    );
    // Each request that built something is a reader that is working, which the background yields to (a read that built nothing is
    // not one).
    assert_eq!(r.service.reader_advances(), 3);
}

#[test]
fn a_request_stops_at_the_backstop_when_the_clock_does_not_move() {
    let r = long(600, |s| {
        s.with_writes_budget(Duration::from_secs(1), Arc::new(|| Duration::ZERO))
    });
    let (got, calls) = r.w.counted(|| {
        r.service
            .text_at(&r.w.wiki, None, &block_id(602), "pages/Foo.md")
    });
    assert!(matches!(got, Err(ReadError::NotCheckedYet)), "{got:?}");
    assert_eq!(
        calls.change,
        u64::try_from(MAX_INDEXED_SHOWS).unwrap(),
        "the backstop: {calls:?}"
    );
    assert_eq!(head(&r), Some(u64::try_from(MAX_INDEXED_SHOWS).unwrap()));
}

#[test]
fn the_time_of_a_request_is_shared_by_every_view_it_asks() {
    // One look is 100 ms and the budget 450 ms: the request builds the default view for four looks and has nothing left for others.
    let r = long(30, |s| {
        s.with_writes_budget(
            Duration::from_millis(450),
            ticking(Duration::from_millis(100)),
        )
    });
    let (_, calls) = r.w.counted(|| {
        r.service
            .text_at(&r.w.wiki, None, &block_id(32), "pages/Foo.md")
    });
    // The first block costs no look; each of the next is one: blocks 1 to 5 are built (looks at 100 .. 400 ms; the fifth look is
    // 500 ms, which is past the budget at block 6).
    assert_eq!(calls.change, 5, "{calls:?}");
}

// ------------------------------------------------------------------------------------ who asks the background

/// A background that only remembers who asked.
#[derive(Default)]
struct Asked(Mutex<Vec<(String, AskedFor)>>);

impl WritesBackground for Asked {
    fn ask(&self, wiki: &WikiRecord, why: AskedFor) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((wiki.slug.to_string(), why));
    }
}

impl Asked {
    fn taken(&self) -> Vec<AskedFor> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
            .into_iter()
            .map(|(_, why)| why)
            .collect()
    }
}

#[test]
fn a_hide_asks_the_background_and_builds_nothing_itself_and_without_one_it_builds_now() {
    let asked = Arc::new(Asked::default());
    let r = long(40, |s| s.with_writes_background(asked.clone()));
    let (_, calls) = r.w.counted(|| r.service.warm_writes_after_hide(&r.w.wiki));
    assert_eq!(asked.taken(), vec![AskedFor::Hide]);
    assert_eq!(calls.change, 0, "nothing waits for the build: {calls:?}");
    assert_eq!(head(&r), None);

    let without = long(40, |s| s);
    without.service.warm_writes_after_hide(&without.w.wiki);
    assert_eq!(
        head(&without),
        Some(42),
        "no background: the hide builds it, now"
    );
}

#[test]
fn a_start_asks_only_where_a_content_hide_is_in_force_and_a_reader_that_finds_the_index_short_asks()
{
    let asked = Arc::new(Asked::default());
    let r = long(40, |s| s.with_writes_background(asked.clone()));
    assert!(r.service.resume_writes_background(&r.w.wiki).unwrap());
    assert_eq!(asked.taken(), vec![AskedFor::Hide]);
    // A reader that cannot be answered because the index has not reached its block asks (the answer is *not checked yet*).
    r.w.fake().fail_change(Some(StorageError::Refused {
        verb: "show",
        message: "down".to_owned(),
        kind: None,
        reason: None,
    }));
    let got = r
        .service
        .text_at(&r.w.wiki, None, &block_id(30), "pages/Bar.md");
    assert!(got.is_err(), "{got:?}");
    r.w.fake().fail_change(None);
    // With a budget of nothing it is *not checked yet*, and asks.
    let tiny = long(40, |s| {
        s.with_writes_background(asked.clone())
            .with_writes_budget(Duration::ZERO, ticking(Duration::from_secs(1)))
    });
    asked.taken();
    assert!(tiny.is_unchecked(30, "pages/Bar.md"));
    assert_eq!(asked.taken(), vec![AskedFor::Reader]);

    // Nothing hidden: nothing is asked at a start, and a read asks nothing.
    let quiet = Arc::new(Asked::default());
    let none = rig_with(&[("pages/Foo.md", &foo("x"))], |s| {
        s.with_writes_background(quiet.clone())
    });
    assert!(!none.service.resume_writes_background(&none.w.wiki).unwrap());
    assert!(none.shows(1, "pages/Foo.md"));
    assert_eq!(quiet.taken(), vec![]);
}

// ------------------------------------------------------------------------------------ the generation (P-3, L-9)

/// The service `service_of(&r.w)` makes over the rig's repository and store, built in a cell so that a hook can use it.
fn another(r: &Rig) -> Arc<ReadService> {
    Arc::new(service_of(&r.w))
}

#[test]
fn a_reader_whose_index_was_dropped_and_built_again_while_it_placed_its_block_gives_no_answer() {
    let r = long(40, |s| s);
    // Some of the index is built, so that dropping it moves its generation.
    assert!(matches!(
        r.service.background_step(&r.w.wiki),
        BackgroundStep::Progress
    ));
    let generation = || {
        r.w.store
            .writes_generation(&r.w.wiki.id, &r.w.wiki.default_view)
            .unwrap()
    };
    let before = generation();
    // While the reader builds toward its block, another process (here: another service) drops the index and builds it whole.
    let other = another(&r);
    let armed = Arc::new(AtomicBool::new(true));
    {
        let (other, armed, wiki) = (other.clone(), armed.clone(), r.w.wiki.clone());
        r.w.fake().on_change(move || {
            if armed.swap(false, Ordering::SeqCst) {
                other.clear_derived(&wiki).unwrap();
                while !other.warm_writes(&wiki).unwrap() {}
            }
        });
    }
    let got = r
        .service
        .text_at(&r.w.wiki, None, &block_id(42), "pages/Foo.md");
    assert!(!armed.load(Ordering::SeqCst), "the drop happened");
    assert!(generation() > before, "the drop moved the generation");
    // The index is whole and right now, and the reader began before the drop: it gives no answer, and the next one does.
    assert!(matches!(got, Err(ReadError::NotCheckedYet)), "{got:?}");
    assert!(r.is_hidden(42, "pages/Foo.md"));
}

#[test]
fn a_build_that_finds_its_index_dropped_under_it_appends_nothing() {
    let r = long(40, |s| s);
    assert!(matches!(
        r.service.background_step(&r.w.wiki),
        BackgroundStep::Progress
    ));
    assert_eq!(head(&r), Some(16));
    // The index is dropped (another process's refresh) during the next step's first `show`.
    let other = another(&r);
    let armed = Arc::new(AtomicBool::new(true));
    {
        let (other, armed, wiki) = (other.clone(), armed.clone(), r.w.wiki.clone());
        r.w.fake().on_change(move || {
            if armed.swap(false, Ordering::SeqCst) {
                other.clear_derived(&wiki).unwrap();
            }
        });
    }
    let step = r.service.background_step(&r.w.wiki);
    assert!(!armed.load(Ordering::SeqCst), "the drop happened");
    // What the step built is not appended to a dropped index: blocks 17 to 32 without 1 to 16 would be an index that says it
    // begins at the view's first block and does not hold it. The step made no progress, and the next one builds from the start.
    assert_eq!(head(&r), None, "{step:?}");
    assert!(
        r.w.store
            .indexed_block(&r.w.wiki.id, &r.w.wiki.default_view, &block_id(20))
            .unwrap()
            .is_none()
    );
    let steps = steps_to_idle(&r);
    // Blocks 1 to 32 were fetched before the drop and are held, so they cost no `show` again: 10 are left.
    assert_eq!(steps, vec![10]);
    assert_eq!(head(&r), Some(42));
}

// ------------------------------------------------------------------------------------ readers while it builds

#[test]
fn reads_are_never_wrong_while_the_background_builds_a_text_hidden_by_a_change_not_yet_indexed_is_never_served()
 {
    // A request builds three blocks of its own (a clock of 100 ms a look, a budget of 300 ms) and each `show` takes a moment, so that
    // the readers meet the index at every stage of its build.
    let r = long(300, |s| {
        s.with_writes_budget(
            Duration::from_millis(300),
            ticking(Duration::from_millis(100)),
        )
    });
    r.w.fake()
        .on_change(|| std::thread::sleep(Duration::from_micros(200)));
    let done = AtomicBool::new(false);
    let served = AtomicUsize::new(0);
    let checked = AtomicUsize::new(0);
    let unchecked = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            loop {
                match r.service.background_step(&r.w.wiki) {
                    BackgroundStep::Progress => {}
                    BackgroundStep::Idle => break,
                    BackgroundStep::Failed(e) => panic!("{e}"),
                }
            }
            done.store(true, Ordering::SeqCst);
        });
        // Meanwhile readers ask for Foo (whose secret at the last block is hidden) at the blocks from the secret on and for Bar at
        // old blocks: the secret is never given, and Bar's text is given or not yet checked, never a tombstone.
        while !done.load(Ordering::SeqCst) {
            for _ in 0..4 {
                match r
                    .service
                    .text_at(&r.w.wiki, None, &block_id(302), "pages/Foo.md")
                {
                    Ok(BlockText::Hidden(_)) => {
                        checked.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(ReadError::NotCheckedYet) => {
                        unchecked.fetch_add(1, Ordering::SeqCst);
                    }
                    other => panic!("the hidden text was not withheld: {other:?}"),
                }
                match r
                    .service
                    .text_at(&r.w.wiki, None, &block_id(150), "pages/Bar.md")
                {
                    Ok(BlockText::Text(_)) => {
                        served.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(ReadError::NotCheckedYet) => {}
                    other => panic!("an unhidden text was refused: {other:?}"),
                }
            }
        }
    });
    // At the end the index is whole and both answer.
    assert!(r.is_hidden(302, "pages/Foo.md"));
    assert!(r.shows(150, "pages/Bar.md"));
    assert!(
        unchecked.load(Ordering::SeqCst) > 0,
        "the readers met an index that was not built yet"
    );
    let _ = (
        served.load(Ordering::SeqCst),
        checked.load(Ordering::SeqCst),
    );
}
