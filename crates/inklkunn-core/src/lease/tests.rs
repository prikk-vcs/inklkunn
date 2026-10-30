#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The write lease: the queue, the bounded wait, the holder record, the other process.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_signer::Maintenance;
use inklkunn_store::{
    AcceptanceStore, DisplayName, LeaseHolder, MemoryStore, NewWiki, RootLayout, Slug, WikiRecord,
    WikiStore,
};

use super::*;
use crate::read::testing::TempDir;

struct World {
    _dir: TempDir,
    store: Arc<MemoryStore>,
    wiki: WikiRecord,
    layout: RootLayout,
    leases: Arc<WikiLeases>,
}

fn world(tag: &str, config: LeaseConfig) -> World {
    let dir = TempDir::new(tag);
    let store = Arc::new(MemoryStore::new());
    let wiki = store
        .create(NewWiki::new(
            Slug::new("lease-test").unwrap(),
            DisplayName::new("Lease test").unwrap(),
        ))
        .unwrap();
    let layout = RootLayout::new(dir.0.join("root"));
    layout.prepare(&wiki.id).unwrap();
    let leases = WikiLeases::new(
        inklkunn_signer::WriteLeases::new(layout.clone()),
        store.clone(),
        "this-process",
        config,
    );
    World {
        _dir: dir,
        store,
        wiki,
        layout,
        leases,
    }
}

fn quick(wait_ms: u64) -> LeaseConfig {
    LeaseConfig {
        max_wait: Duration::from_millis(wait_ms),
        poll: Duration::from_millis(5),
        ..LeaseConfig::default()
    }
}

#[test]
fn the_holder_is_recorded_when_the_lease_is_taken_and_cleared_when_it_is_released() {
    let w = world("record", quick(100));
    assert_eq!(w.store.lease_holder(&w.wiki.id).unwrap(), None);
    let lease = w.leases.acquire(&w.wiki.id, Purpose::Export, None).unwrap();
    let held = w.store.lease_holder(&w.wiki.id).unwrap().unwrap();
    assert_eq!(
        (
            held.holder.as_str(),
            held.purpose.as_str(),
            held.acceptance.as_ref()
        ),
        ("this-process", "export", None)
    );
    assert!(w.leases.is_held(&w.wiki.id));
    // It says which acceptance once there is a record of one.
    let acceptance = AcceptanceId::new();
    assert!(
        lease.attach(&acceptance).is_err(),
        "an acceptance that is not in the store cannot be named"
    );
    drop(lease);
    assert_eq!(w.store.lease_holder(&w.wiki.id).unwrap(), None);
    assert!(!w.leases.is_held(&w.wiki.id));
}

#[test]
fn a_record_of_another_holder_found_at_the_take_is_a_holder_that_died_holding_the_lease() {
    let w = world("died", quick(100));
    w.store
        .record_lease_holder(&LeaseHolder {
            wiki: w.wiki.id.clone(),
            holder: "inklkunn-4242".to_owned(),
            purpose: "acceptance".to_owned(),
            acceptance: None,
            since: 5,
        })
        .unwrap();
    let lease = w
        .leases
        .acquire(&w.wiki.id, Purpose::Acceptance, None)
        .unwrap();
    assert_eq!(
        lease.replaced().map(|h| h.holder.as_str()),
        Some("inklkunn-4242")
    );
    drop(lease);
    // Released cleanly: the next lease finds nothing.
    let next = w
        .leases
        .acquire(&w.wiki.id, Purpose::Acceptance, None)
        .unwrap();
    assert_eq!(next.replaced(), None);
}

#[test]
fn waiters_are_served_in_the_order_they_came() {
    let w = world(
        "fifo",
        LeaseConfig {
            max_wait: Duration::from_secs(10),
            poll: Duration::from_millis(5),
            ..LeaseConfig::default()
        },
    );
    let first = w.leases.acquire(&w.wiki.id, Purpose::System, None).unwrap();
    let served = Arc::new(Mutex::new(Vec::new()));
    let mut threads = Vec::new();
    for n in 0..4_u32 {
        let (leases, wiki, served) = (w.leases.clone(), w.wiki.id.clone(), served.clone());
        threads.push(std::thread::spawn(move || {
            let lease = leases.acquire(&wiki, Purpose::System, None).unwrap();
            served.lock().unwrap().push(n);
            std::thread::sleep(Duration::from_millis(10));
            drop(lease);
        }));
        // The next one comes only once this one stands in the line.
        let start = Instant::now();
        while w.leases.waiting(&w.wiki.id) < u64::from(n) + 1 {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    drop(first);
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(*served.lock().unwrap(), vec![0, 1, 2, 3]);
    assert_eq!(w.leases.waiting(&w.wiki.id), 0);
}

#[test]
fn a_waiter_hears_how_many_are_ahead_and_about_how_long_and_the_estimate_follows_what_leases_took()
{
    let w = world("estimate", quick(120));
    // A lease that took a while teaches the estimate.
    let lease = w.leases.acquire(&w.wiki.id, Purpose::System, None).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    drop(lease);
    let holder = w.leases.acquire(&w.wiki.id, Purpose::System, None).unwrap();
    let LeaseError::Busy(busy) = w
        .leases
        .acquire(&w.wiki.id, Purpose::System, None)
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!((busy.depth, busy.why.clone()), (1, BusyWhy::Waited));
    assert!(
        busy.estimate >= Duration::from_millis(150),
        "about what a lease took lately, once ahead: {:?}",
        busy.estimate
    );
    // Before any lease was seen, the typical hold is the estimate.
    let fresh = world("estimate2", quick(50));
    let _held = fresh
        .leases
        .acquire(&fresh.wiki.id, Purpose::System, None)
        .unwrap();
    let LeaseError::Busy(busy) = fresh
        .leases
        .acquire(&fresh.wiki.id, Purpose::System, None)
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(busy.estimate, fresh.leases.config().typical_hold);
    drop(holder);
}

#[test]
fn the_queue_is_bounded_and_the_one_past_the_bound_is_not_queued() {
    let w = world(
        "bound",
        LeaseConfig {
            queue_bound: 1,
            max_wait: Duration::from_secs(10),
            poll: Duration::from_millis(5),
            ..LeaseConfig::default()
        },
    );
    let held = w.leases.acquire(&w.wiki.id, Purpose::System, None).unwrap();
    let (leases, wiki) = (w.leases.clone(), w.wiki.id.clone());
    let waiter = std::thread::spawn(move || leases.acquire(&wiki, Purpose::System, None).map(drop));
    let start = Instant::now();
    while w.leases.waiting(&w.wiki.id) < 1 {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
    let LeaseError::Busy(busy) = w
        .leases
        .acquire(&w.wiki.id, Purpose::System, None)
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(
        busy.why,
        BusyWhy::QueueFull {
            bound: 1,
            waiting: 1
        }
    );
    assert_eq!(busy.depth, 2, "the holder and the one waiting");
    drop(held);
    assert!(waiter.join().unwrap().is_ok());
}

#[test]
fn a_lock_held_by_another_process_is_told_as_such_with_the_process_that_holds_it() {
    let w = world("other", quick(100));
    // Another open file description on the same lock file is what another process is.
    let theirs = inklkunn_signer::WriteLeases::new(w.layout.clone());
    let cli = theirs.try_acquire(&w.wiki.id).unwrap();
    let LeaseError::Busy(busy) = w
        .leases
        .acquire(&w.wiki.id, Purpose::Acceptance, None)
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(
        busy.why,
        BusyWhy::OtherProcess {
            holder: Some(std::process::id())
        }
    );
    assert_eq!(w.leases.waiting(&w.wiki.id), 0, "it left the line");
    assert!(
        w.store.lease_holder(&w.wiki.id).unwrap().is_none(),
        "it never recorded itself as the holder"
    );
    drop(cli);
    w.leases
        .acquire(&w.wiki.id, Purpose::Acceptance, None)
        .expect("free once the other process lets go");
}

#[test]
fn try_acquire_never_waits_and_never_jumps_the_queue() {
    let w = world("try", quick(5_000));
    let held = w
        .leases
        .try_acquire(&w.wiki.id, Purpose::Recovery, None)
        .unwrap();
    let started = Instant::now();
    assert!(matches!(
        w.leases.try_acquire(&w.wiki.id, Purpose::Recovery, None),
        Err(LeaseError::Busy(_))
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(held);
    w.leases
        .try_acquire(&w.wiki.id, Purpose::Recovery, None)
        .unwrap();
}

#[test]
fn the_export_and_the_checkpoint_refresh_take_the_lease_and_are_told_busy_like_any_other_act() {
    let w = world("maintenance", quick(80));
    let maintenance = Maintenance::new("prikk", Sandbox::Unconfined, w.layout.clone())
        .with_leases(w.leases.signer_leases());
    let held = w
        .leases
        .acquire(&w.wiki.id, Purpose::Acceptance, None)
        .unwrap();
    let dest = w.layout.scratch_dir(&w.wiki.id).join("out.prikk");
    let err = w
        .leases
        .export_archive(&maintenance, &w.wiki.id, &dest)
        .unwrap_err();
    assert!(
        matches!(err, Leased::NotLeased(LeaseError::Busy(_))),
        "{err}"
    );
    let err = w
        .leases
        .refresh_checkpoint(&maintenance, &w.wiki.id)
        .unwrap_err();
    assert!(
        matches!(err, Leased::NotLeased(LeaseError::Busy(_))),
        "{err}"
    );
    drop(held);
    // With the lease free the act is attempted (this wiki has no repository, so the act itself says so): it
    // took the lease, ran inside it, and gave it back.
    let err = w
        .leases
        .refresh_checkpoint(&maintenance, &w.wiki.id)
        .unwrap_err();
    assert!(matches!(err, Leased::Failed(_)), "{err}");
    assert!(!w.leases.is_held(&w.wiki.id));
    assert_eq!(w.store.lease_holder(&w.wiki.id).unwrap(), None);
}
