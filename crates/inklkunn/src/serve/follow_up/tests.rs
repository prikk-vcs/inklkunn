#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

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
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_flood_of_asks_is_bounded_asked_without_waiting_and_run_one_at_a_time() {
    let (started_tx, started_rx) = channel::<()>();
    let (go_tx, go_rx) = channel::<()>();
    let go_rx = Mutex::new(go_rx);
    let running = Arc::new(AtomicUsize::new(0));
    let widest = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let (r, w, d) = (running.clone(), widest.clone(), done.clone());
    let follow_up = FollowUp::with_runner(move |_job: &Job| {
        let now = r.fetch_add(1, Ordering::SeqCst) + 1;
        w.fetch_max(now, Ordering::SeqCst);
        let _ = started_tx.send(());
        let _ = go_rx.lock().unwrap().recv();
        r.fetch_sub(1, Ordering::SeqCst);
        d.fetch_add(1, Ordering::SeqCst);
    });
    let records = wikis(QUEUE_BOUND * 4);
    // The first job is taken by the worker and held there.
    follow_up.warm_title_index(records.first().unwrap());
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // Many more asks while it is held: none of them waits, and no more than the bound is kept.
    let began = Instant::now();
    for record in records.iter().skip(1) {
        follow_up.warm_title_index(record);
        follow_up.enqueue_verification(record);
    }
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "asking never waits for the work"
    );
    for _ in 0..(records.len() * 2) {
        let _ = go_tx.send(());
    }
    until("the queue to drain", || {
        follow_up.waiting.lock().unwrap().is_empty()
    });
    std::thread::sleep(Duration::from_millis(200));
    let ran = done.load(Ordering::SeqCst);
    assert!(
        ran <= 1 + QUEUE_BOUND,
        "at most the held job and a full queue ran: {ran}"
    );
    assert!(ran > 1, "the queued jobs ran: {ran}");
    assert_eq!(
        widest.load(Ordering::SeqCst),
        1,
        "one worker, never two jobs at once"
    );
}

#[test]
fn the_same_job_for_the_same_wiki_is_asked_once_while_it_waits_and_again_after_it_was_taken() {
    let (started_tx, started_rx) = channel::<()>();
    let (go_tx, go_rx) = channel::<()>();
    let go_rx = Mutex::new(go_rx);
    let done = Arc::new(AtomicUsize::new(0));
    let d = done.clone();
    let follow_up = FollowUp::with_runner(move |_job: &Job| {
        let _ = started_tx.send(());
        let _ = go_rx.lock().unwrap().recv();
        d.fetch_add(1, Ordering::SeqCst);
    });
    let records = wikis(2);
    let (a, b) = (records.first().unwrap(), records.get(1).unwrap());
    // `a` is taken and held; `b` waits once however often it is asked; `a` asked again while it runs is a new job.
    follow_up.warm_title_index(a);
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    for _ in 0..10 {
        follow_up.warm_title_index(b);
    }
    follow_up.warm_title_index(a);
    // Plenty of room for every job that might run: only the three that should are there.
    for _ in 0..30 {
        let _ = go_tx.send(());
    }
    until("three jobs", || done.load(Ordering::SeqCst) >= 3);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(done.load(Ordering::SeqCst), 3, "a, b once, a again");
}

#[test]
fn both_kinds_reach_the_runner_with_the_wiki() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let follow_up = FollowUp::with_runner(move |job: &Job| {
        s.lock()
            .unwrap()
            .push((job.kind, job.wiki.slug.to_string()));
    });
    let records = wikis(1);
    let record = records.first().unwrap();
    follow_up.warm_title_index(record);
    follow_up.enqueue_verification(record);
    until("both", || seen.lock().unwrap().len() == 2);
    let seen = seen.lock().unwrap().clone();
    assert!(seen.contains(&(Kind::WarmTitleIndex, "wiki-0".to_owned())));
    assert!(seen.contains(&(Kind::Verify, "wiki-0".to_owned())));
}

type Heard = Arc<Mutex<Vec<String>>>;
type Log = Arc<dyn Fn(&str) + Send + Sync>;

fn lines() -> (Heard, Log) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    (
        heard,
        Arc::new(move |line: &str| sink.lock().unwrap().push(line.to_owned())),
    )
}

#[test]
fn a_job_that_panics_is_a_line_and_the_next_job_still_runs() {
    // A4: the worker used to end with the panicking job, and every later ask failed silently.
    let (heard, log) = lines();
    let done = Arc::new(AtomicUsize::new(0));
    let d = done.clone();
    let follow_up = FollowUp::with_runner_logging(
        move |job: &Job| {
            if job.kind == Kind::WarmTitleIndex {
                panic!("a job that goes wrong");
            }
            d.fetch_add(1, Ordering::SeqCst);
        },
        log,
    );
    let records = wikis(2);
    let (a, b) = (records.first().unwrap(), records.get(1).unwrap());
    follow_up.warm_title_index(a);
    until("the panic to be heard", || {
        !heard.lock().unwrap().is_empty()
    });
    // The worker is alive: jobs asked for after the panic run.
    follow_up.enqueue_verification(a);
    follow_up.enqueue_verification(b);
    until("the later jobs", || done.load(Ordering::SeqCst) >= 2);
    let heard = heard.lock().unwrap().clone();
    assert!(
        heard
            .iter()
            .any(|l| l.contains("panicked") && l.contains("wiki-0")),
        "{heard:?}"
    );
    assert!(
        heard.iter().all(|l| !l.contains("worker has stopped")),
        "{heard:?}"
    );
}

#[test]
fn a_closed_queue_is_said_once_not_at_every_ask() {
    let (heard, log) = lines();
    let follow_up = FollowUp::with_dead_worker(log);
    let records = wikis(1);
    let record = records.first().unwrap();
    assert!(heard.lock().unwrap().is_empty(), "nothing yet");
    for _ in 0..5 {
        follow_up.warm_title_index(record);
        follow_up.enqueue_verification(record);
    }
    let heard = heard.lock().unwrap().clone();
    assert_eq!(heard.len(), 1, "once: {heard:?}");
    assert!(
        heard
            .iter()
            .all(|l| l.contains("follow-up worker has stopped")),
        "{heard:?}"
    );
    assert!(
        follow_up.waiting.lock().unwrap().is_empty(),
        "a job that could not be queued is not left waiting"
    );
}
