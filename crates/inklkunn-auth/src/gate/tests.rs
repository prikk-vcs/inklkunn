#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn permits_are_bounded_and_come_back_when_dropped() {
    let gate = WorkGate::new(2, Duration::from_millis(30));
    let a = gate.acquire().expect("first");
    let b = gate.acquire().expect("second");
    assert_eq!(gate.available(), 0);
    let started = Instant::now();
    assert!(gate.acquire().is_none(), "a third caller times out");
    assert!(
        started.elapsed() >= Duration::from_millis(25),
        "after waiting its turn"
    );
    drop(a);
    assert_eq!(gate.available(), 1);
    let c = gate.acquire().expect("a permit returned");
    drop((b, c));
    assert_eq!(gate.available(), 2);
}

#[test]
fn a_waiting_caller_gets_the_permit_that_comes_free() {
    let gate = Arc::new(WorkGate::new(1, Duration::from_secs(5)));
    let held = gate.acquire().expect("held");
    let waiter = {
        let gate = Arc::clone(&gate);
        std::thread::spawn(move || gate.acquire().is_some())
    };
    std::thread::sleep(Duration::from_millis(50));
    drop(held);
    assert!(waiter.join().unwrap(), "the waiter was woken and got it");
}

#[test]
fn a_gate_always_has_at_least_one_permit() {
    let gate = WorkGate::new(0, Duration::from_millis(10));
    assert!(gate.acquire().is_some());
}

#[test]
fn many_threads_never_exceed_the_bound() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let gate = Arc::new(WorkGate::new(3, Duration::from_secs(10)));
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let (gate, running, peak) =
                (Arc::clone(&gate), Arc::clone(&running), Arc::clone(&peak));
            std::thread::spawn(move || {
                let _permit = gate.acquire().expect("a permit within ten seconds");
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                running.fetch_sub(1, Ordering::SeqCst);
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert!(
        peak.load(Ordering::SeqCst) <= 3,
        "peak {}",
        peak.load(Ordering::SeqCst)
    );
    assert_eq!(gate.available(), 3);
}
