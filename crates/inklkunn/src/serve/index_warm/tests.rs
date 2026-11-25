#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The background build's worker (0.4.0 correction P-5): one task per wiki, wikis in turn, a failure said once and retried by the
//! next hide, a reader's ask heard patiently, a task ended by its bound, and a worker that ends with the service. The step is a
//! script: what one step of the core does is the core's own tests.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use inklkunn_core::read::ReadError;
use inklkunn_store::{DisplayName, NewWiki, Slug, SqliteStore, WikiStore};

use super::*;

fn wikis(n: usize) -> Vec<WikiRecord> {
    let store = SqliteStore::open_in_memory().unwrap();
    (0..n)
        .map(|i| {
            store
                .create(NewWiki::new(
                    Slug::new(&format!("wiki-{i}")).unwrap(),
                    DisplayName::new(&format!("Wiki {i}")).unwrap(),
                ))
                .unwrap()
        })
        .collect()
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Pacing with no waits: a test decides when time passes.
fn quick() -> Pacing {
    Pacing {
        floor: Duration::ZERO,
        share_percent: 0,
        yield_pause: Duration::from_millis(1),
        max_yields: 0,
        retry_after: Duration::from_secs(3600),
        reader_every: Duration::from_secs(3600),
        max_steps: 1_000,
    }
}

/// A builder that does what it is scripted to, wiki by wiki (idle when its script is empty), and remembers what it was asked.
#[derive(Default)]
struct Script {
    steps: Mutex<HashMap<String, VecDeque<Step>>>,
    /// The slug of each step taken, in order.
    order: Mutex<Vec<String>>,
    running: AtomicUsize,
    widest: AtomicUsize,
    /// A step waits here (when set) until it is sent a unit.
    gate: Mutex<Option<Receiver<()>>>,
    entered: Mutex<Option<Sender<()>>>,
    readers: AtomicU64,
    /// Each look at the readers' count says it moved (a reader is always working).
    busy_readers: AtomicBool,
    reader_looks: AtomicU64,
    /// How many looks at the readers had been made when the first step was taken.
    looks_at_first_step: Mutex<Option<u64>>,
    /// When each step was taken, on the stand-in's clock (the test's own: the worker's pauses are measured against it).
    times: Mutex<Vec<Instant>>,
}

#[derive(Clone, Copy)]
enum Step {
    Progress,
    Idle,
    Fail,
    Panic,
}

impl Script {
    fn say(&self, slug: &str, script: &[Step]) {
        self.steps
            .lock()
            .unwrap()
            .insert(slug.to_owned(), script.iter().cloned().collect());
    }

    fn taken(&self) -> Vec<String> {
        self.order.lock().unwrap().clone()
    }

    fn count(&self, slug: &str) -> usize {
        self.taken().iter().filter(|s| *s == slug).count()
    }
}

impl Build for Script {
    fn step(&self, wiki: &WikiRecord) -> BackgroundStep {
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.widest.fetch_max(now, Ordering::SeqCst);
        self.looks_at_first_step
            .lock()
            .unwrap()
            .get_or_insert(self.reader_looks.load(Ordering::SeqCst));
        let slug = wiki.slug.to_string();
        let entered = self.entered.lock().unwrap().clone();
        if let Some(entered) = entered {
            let _ = entered.send(());
        }
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.recv_timeout(Duration::from_secs(10));
        }
        self.order.lock().unwrap().push(slug.clone());
        self.times.lock().unwrap().push(Instant::now());
        let next = self
            .steps
            .lock()
            .unwrap()
            .get_mut(&slug)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Step::Idle);
        self.running.fetch_sub(1, Ordering::SeqCst);
        match next {
            Step::Progress => BackgroundStep::Progress,
            Step::Idle => BackgroundStep::Idle,
            Step::Fail => BackgroundStep::Failed(ReadError::NotCheckedYet),
            Step::Panic => panic!("a step that panics"),
        }
    }

    fn reader_advances(&self) -> u64 {
        self.reader_looks.fetch_add(1, Ordering::SeqCst);
        if self.busy_readers.load(Ordering::SeqCst) {
            self.readers.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            self.readers.load(Ordering::SeqCst)
        }
    }
}

/// A warmer over a script, the lines the operator heard kept.
struct Rig {
    warmer: IndexWarmer,
    script: Arc<Script>,
    heard: Arc<Mutex<Vec<String>>>,
}

fn rig(pacing: Pacing) -> Rig {
    let script = Arc::new(Script::default());
    let heard: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    let warmer = IndexWarmer::start(
        pacing,
        Arc::new(move |line: &str| sink.lock().unwrap().push(line.to_owned())),
    );
    let weak: Weak<Script> = Arc::downgrade(&script);
    let builder: Weak<dyn Build> = weak;
    warmer.bind(builder);
    Rig {
        warmer,
        script,
        heard,
    }
}

impl Rig {
    fn heard(&self) -> Vec<String> {
        self.heard.lock().unwrap().clone()
    }

    /// Wait until no wiki is in flight and the worker waits for the next ask: whatever a task had to do is done.
    fn quiet(&self) {
        until("the worker to be done", || {
            let shared = self.warmer.shared.lock().unwrap();
            shared.queued.is_empty() && shared.waiting
        });
    }
}

#[test]
fn two_asks_for_one_wiki_are_one_task_that_builds_until_there_is_nothing_left() {
    let r = rig(quick());
    let wiki = wikis(1).remove(0);
    let slug = wiki.slug.to_string();
    r.script
        .say(&slug, &[Step::Progress, Step::Progress, Step::Progress]);
    // The first step is held; the second ask arrives while the wiki is in flight and is the same task.
    let (entered_tx, entered_rx) = channel();
    let (go_tx, go_rx) = channel();
    *r.script.entered.lock().unwrap() = Some(entered_tx);
    *r.script.gate.lock().unwrap() = Some(go_rx);
    r.warmer.ask(&wiki, AskedFor::Hide);
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.warmer.ask(&wiki, AskedFor::Reader);
    go_tx.send(()).unwrap();
    r.quiet();
    // Three steps of progress, one that found nothing left, and the one look after the wiki left the set: five, by one builder.
    assert_eq!(r.script.count(&slug), 5, "{:?}", r.script.taken());
    assert_eq!(
        r.script.widest.load(Ordering::SeqCst),
        1,
        "never two at once"
    );
    // Asked again after it ended, it is a task again.
    r.warmer.ask(&wiki, AskedFor::Hide);
    until("the new task", || r.script.count(&slug) >= 7);
    r.quiet();
    assert_eq!(r.script.count(&slug), 7);
    assert_eq!(r.heard(), Vec::<String>::new());
}

#[test]
fn wikis_are_built_in_turn_one_step_each_and_never_two_at_once() {
    let r = rig(quick());
    let [first, second]: [WikiRecord; 2] = wikis(2).try_into().unwrap();
    let (a, b) = (first.slug.to_string(), second.slug.to_string());
    r.script.say(&a, &[Step::Progress; 4]);
    r.script.say(&b, &[Step::Progress; 4]);
    // The first step is held while the second wiki is asked for, so that both are in the worker's hands together.
    let (entered_tx, entered_rx) = channel();
    let (go_tx, go_rx) = channel();
    *r.script.entered.lock().unwrap() = Some(entered_tx);
    *r.script.gate.lock().unwrap() = Some(go_rx);
    r.warmer.ask(&first, AskedFor::Hide);
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    r.warmer.ask(&second, AskedFor::Hide);
    go_tx.send(()).unwrap();
    r.quiet();
    let order = r.script.taken();
    assert_eq!(
        order.iter().take(6).cloned().collect::<Vec<_>>(),
        vec![&a, &b, &a, &b, &a, &b]
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
        "in turn: {order:?}"
    );
    assert_eq!(r.script.count(&a), 6);
    assert_eq!(r.script.count(&b), 6);
    assert_eq!(r.script.widest.load(Ordering::SeqCst), 1);
}

#[test]
fn a_failed_step_ends_the_task_and_is_said_once_a_hide_retries_at_once_and_a_reader_is_not_heard_for_a_while()
 {
    let r = rig(quick());
    let wiki = wikis(1).remove(0);
    let slug = wiki.slug.to_string();
    r.script.say(&slug, &[Step::Fail]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    assert_eq!(r.script.count(&slug), 1);
    assert_eq!(r.heard().len(), 1, "{:?}", r.heard());
    assert!(
        r.heard().iter().all(|l| l.contains(&slug)),
        "{:?}",
        r.heard()
    );
    // A reader that meets *not checked yet* asks: the failure is recent, so it is not heard (prikk is not asked at every request).
    r.warmer.ask(&wiki, AskedFor::Reader);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(r.script.count(&slug), 1, "a reader's ask is not heard yet");
    // A hide is: it retries at once, fails again, and the operator is not told twice.
    r.script.say(&slug, &[Step::Fail]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    until("the retry", || r.script.count(&slug) >= 2);
    r.quiet();
    assert_eq!(r.heard().len(), 1, "said once: {:?}", r.heard());
    // And a retry that works builds on; then a failure is news again.
    r.script.say(&slug, &[Step::Progress, Step::Idle]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    r.script.say(&slug, &[Step::Fail]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    assert_eq!(r.heard().len(), 2, "{:?}", r.heard());
}

#[test]
fn a_reader_is_heard_once_the_pause_after_a_failure_or_a_finished_task_has_passed() {
    let r = rig(Pacing {
        retry_after: Duration::from_millis(400),
        reader_every: Duration::from_millis(400),
        ..quick()
    });
    let wiki = wikis(1).remove(0);
    let slug = wiki.slug.to_string();
    // A task that ended with nothing left: a reader's ask within the pause is not heard, after it is.
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    let done = r.script.count(&slug);
    r.warmer.ask(&wiki, AskedFor::Reader);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(r.script.count(&slug), done, "not heard within the pause");
    std::thread::sleep(Duration::from_millis(500));
    r.warmer.ask(&wiki, AskedFor::Reader);
    until("the reader's ask", || r.script.count(&slug) > done);
}

#[test]
fn a_task_ends_at_its_bound_and_the_next_ask_goes_on() {
    let r = rig(Pacing {
        max_steps: 3,
        ..quick()
    });
    let wiki = wikis(1).remove(0);
    let slug = wiki.slug.to_string();
    r.script.say(&slug, &[Step::Progress; 10]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    assert_eq!(r.script.count(&slug), 3, "the task ended at its bound");
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    assert_eq!(r.script.count(&slug), 6, "the next ask went on");
}

#[test]
fn a_step_that_panics_is_a_failure_and_the_worker_goes_on_with_the_next_wiki() {
    let r = rig(quick());
    let [first, second]: [WikiRecord; 2] = wikis(2).try_into().unwrap();
    let (a, b) = (first.slug.to_string(), second.slug.to_string());
    r.script.say(&a, &[Step::Panic]);
    r.script.say(&b, &[Step::Progress]);
    r.warmer.ask(&first, AskedFor::Hide);
    r.quiet();
    assert_eq!(r.heard().len(), 1, "{:?}", r.heard());
    r.warmer.ask(&second, AskedFor::Hide);
    r.quiet();
    assert_eq!(r.script.count(&b), 3, "the worker lived on and built");
}

#[test]
fn the_worker_yields_to_readers_that_are_building_and_never_for_ever() {
    let r = rig(Pacing {
        max_yields: 2,
        ..quick()
    });
    r.script.busy_readers.store(true, Ordering::SeqCst);
    let wiki = wikis(1).remove(0);
    let slug = wiki.slug.to_string();
    r.script.say(&slug, &[Step::Progress]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    // Readers were busy at every look: two waits, and then the step is taken (the third look).
    assert_eq!(
        *r.script.looks_at_first_step.lock().unwrap(),
        Some(3),
        "two yields, then the step"
    );
    assert!(r.script.count(&slug) >= 2);
}

#[test]
fn the_worker_ends_when_the_warmer_is_dropped_and_an_ask_after_that_is_nothing() {
    let r = rig(quick());
    let wiki = wikis(1).remove(0);
    let shared = r.warmer.shared.clone();
    // The warmer's, the worker's and this one: the worker's goes when it ends.
    assert_eq!(Arc::strong_count(&shared), 3);
    let Rig { warmer, .. } = r;
    drop(warmer);
    until("the worker to end", || Arc::strong_count(&shared) == 1);
    // An ask of a warmer whose worker is gone leaves nothing in flight.
    let (queue, jobs) = channel::<WikiRecord>();
    drop(jobs);
    let orphan = IndexWarmer {
        queue: Some(queue),
        shared: shared.clone(),
        builder: Arc::new(OnceLock::new()),
    };
    orphan.ask(&wiki, AskedFor::Hide);
    assert!(shared.lock().unwrap().queued.is_empty());
}

#[test]
fn an_ask_before_the_service_is_bound_builds_nothing_and_the_next_ask_after_it_does() {
    let heard: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let warmer = IndexWarmer::start(quick(), Arc::new(|_: &str| {}));
    let _ = &heard;
    let script = Arc::new(Script::default());
    let wiki = wikis(1).remove(0);
    let slug = wiki.slug.to_string();
    warmer.ask(&wiki, AskedFor::Hide);
    until("the early ask to be dropped", || {
        warmer.shared.lock().unwrap().queued.is_empty()
    });
    assert_eq!(script.count(&slug), 0);
    let weak: Weak<Script> = Arc::downgrade(&script);
    let builder: Weak<dyn Build> = weak;
    warmer.bind(builder);
    script.say(&slug, &[Step::Progress]);
    warmer.ask(&wiki, AskedFor::Hide);
    until("the build", || script.count(&slug) >= 3);
}

/// Asks for a new wiki every `every` for at most `how_long` (a busy server: readers in many wikis).
fn traffic(warmer: &IndexWarmer, wikis: &[WikiRecord], every: Duration, how_long: Duration) {
    let began = Instant::now();
    for wiki in wikis {
        if began.elapsed() >= how_long {
            break;
        }
        warmer.ask(wiki, AskedFor::Hide);
        std::thread::sleep(every);
    }
}

#[test]
fn a_job_every_few_milliseconds_does_not_cut_the_pause_between_steps_short() {
    let floor = Duration::from_millis(120);
    let r = rig(Pacing { floor, ..quick() });
    let all = wikis(80);
    let (first, others) = all.split_first().unwrap();
    // Every wiki has more to do (so the worker always has a next turn to pause for: a look after a task is over is not paced, it is
    // the same turn), and a new wiki is asked for every 10 ms.
    for wiki in &all {
        r.script.say(&wiki.slug.to_string(), &[Step::Progress; 50]);
    }
    r.warmer.ask(first, AskedFor::Hide);
    traffic(
        &r.warmer,
        others,
        Duration::from_millis(10),
        Duration::from_millis(700),
    );
    let times = r.script.times.lock().unwrap().clone();
    assert!(times.len() >= 3, "the worker built: {}", times.len());
    for pair in times.windows(2) {
        let [a, b] = pair else { continue };
        assert!(
            b.duration_since(*a) >= floor,
            "two steps {:?} apart, a pause is {floor:?}",
            b.duration_since(*a)
        );
    }
    // Over 700 ms of traffic and a 120 ms pause: a handful of steps, not one for each of the 70 asks.
    assert!(times.len() <= 10, "{} steps", times.len());
}

#[test]
fn a_job_during_the_yield_pause_is_queued_and_taken_after_it_not_instead_of_it() {
    let yield_pause = Duration::from_millis(300);
    let r = rig(Pacing {
        yield_pause,
        max_yields: 1,
        ..quick()
    });
    r.script.busy_readers.store(true, Ordering::SeqCst);
    let all = wikis(30);
    let (first, others) = all.split_first().unwrap();
    let asked = Instant::now();
    r.warmer.ask(first, AskedFor::Hide);
    traffic(
        &r.warmer,
        others,
        Duration::from_millis(10),
        Duration::from_millis(250),
    );
    until("the first step", || {
        !r.script.times.lock().unwrap().is_empty()
    });
    let first_step = r.script.times.lock().unwrap().first().copied().unwrap();
    assert!(
        first_step.duration_since(asked) >= yield_pause,
        "the first step was taken {:?} after the ask; the readers' pause is {yield_pause:?}",
        first_step.duration_since(asked)
    );
    // What was asked for meanwhile was queued, and is built after.
    r.quiet();
    assert!(r.script.count(&others.first().unwrap().slug.to_string()) >= 1);
}

#[test]
fn a_second_look_that_fails_is_said_once_and_takes_the_retry_pause() {
    // Readers are heard at once after a finished task, and not for an hour after a failure: only the failure's pause can hold one.
    let r = rig(Pacing {
        reader_every: Duration::ZERO,
        retry_after: Duration::from_secs(3600),
        ..quick()
    });
    let wiki = wikis(1).remove(0);
    let slug = wiki.slug.to_string();
    // The step finds nothing left to do, and the look after it fails.
    r.script.say(&slug, &[Step::Idle, Step::Fail]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    r.quiet();
    assert_eq!(r.script.count(&slug), 2, "the step and the second look");
    assert_eq!(r.heard().len(), 1, "said: {:?}", r.heard());
    assert!(
        r.heard().iter().all(|l| l.contains(&slug)),
        "{:?}",
        r.heard()
    );
    r.warmer.ask(&wiki, AskedFor::Reader);
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(
        r.script.count(&slug),
        2,
        "a reader's ask is not heard within the retry pause"
    );
    // Said once: the same failure again is not news; a hide retries at once.
    r.script.say(&slug, &[Step::Idle, Step::Fail]);
    r.warmer.ask(&wiki, AskedFor::Hide);
    until("the retry", || r.script.count(&slug) >= 4);
    r.quiet();
    assert_eq!(r.heard().len(), 1, "said once: {:?}", r.heard());
}

#[test]
fn the_start_up_resume_says_a_store_error_in_a_line_and_goes_on() {
    let all = wikis(3);
    let [s0, s1, _]: [String; 3] = all
        .iter()
        .map(|w| w.slug.to_string())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    // The wiki list fails: one line, nothing else.
    let notes = resume_notes(Err::<Vec<WikiRecord>, _>("the store\nsaid no"), |_| {
        Ok::<bool, String>(true)
    });
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes.iter().all(|l| !l.contains('\n')), "{notes:?}");
    assert!(
        notes.iter().all(|l| l.contains("the store said no")),
        "{notes:?}"
    );
    // One wiki's hide cannot be looked at: a line that names it; the others are resumed or not as they are.
    let notes = resume_notes(Ok::<_, String>(all), |wiki| {
        let slug = wiki.slug.to_string();
        if slug == s0 {
            Ok(true)
        } else if slug == s1 {
            Err("it\nfailed".to_owned())
        } else {
            Ok(false)
        }
    });
    assert_eq!(notes.len(), 2, "{notes:?}");
    let [resumed, unreadable]: [String; 2] = notes.try_into().unwrap();
    assert!(
        resumed.contains(&s0) && resumed.contains("in force"),
        "{resumed:?}"
    );
    assert!(
        unreadable.contains(&s1) && unreadable.contains("it failed") && !unreadable.contains('\n'),
        "{unreadable:?}"
    );
    // Nothing in force and nothing wrong: no line.
    assert!(resume_notes(Ok::<_, String>(wikis(2)), |_| Ok::<bool, String>(false)).is_empty());
}
