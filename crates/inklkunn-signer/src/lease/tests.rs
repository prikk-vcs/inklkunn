#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The write lease's lock: taken without waiting, shared between the lease and the acts that run inside it
//! through a scope on one thread, and borrowed by nothing else.

use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::flock::{self, Acquired};
use crate::locks::is_lent;

struct Dir(std::path::PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn leases(tag: &str) -> (Dir, RootLayout, Arc<WriteLeases>) {
    let dir = std::env::temp_dir().join(format!("inklkunn-lease-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let root = RootLayout::new(dir.join("wikis"));
    (Dir(dir), root.clone(), WriteLeases::new(root))
}

#[test]
fn a_lease_is_given_to_one_taker_at_a_time_and_again_when_it_ends() {
    let (_dir, root, leases) = leases("one");
    let wiki = WikiId::new();
    root.prepare(&wiki).unwrap();
    let first = leases.try_acquire(&wiki).expect("the first lease");
    assert_eq!(first.wiki(), &wiki);
    assert_eq!(
        leases.try_acquire(&wiki).unwrap_err(),
        LeaseRefusal::ThisProcess,
        "the second taker in this process is refused at once"
    );
    // Another wiki is another lease.
    let other = WikiId::new();
    root.prepare(&other).unwrap();
    let elsewhere = leases.try_acquire(&other).expect("another wiki is free");
    drop(elsewhere);
    drop(first);
    leases.try_acquire(&wiki).expect("free again once it ended");
}

#[test]
fn a_lock_held_by_another_open_file_description_is_the_other_process() {
    let (_dir, root, leases) = leases("other");
    let wiki = WikiId::new();
    root.prepare(&wiki).unwrap();
    // The command line beside a running server holds the file lock: an open file description of its own.
    let Acquired::Held(theirs) = flock::try_exclusive(&root.lock_file(&wiki)).unwrap() else {
        panic!("the lock was free");
    };
    match leases.try_acquire(&wiki) {
        Err(LeaseRefusal::OtherProcess { holder }) => {
            assert_eq!(holder, Some(std::process::id()), "the holder it recorded");
        }
        other => panic!("{other:?}"),
    }
    // And the in-process turn was given back: once the other lets go, the lease is free.
    drop(theirs);
    leases
        .try_acquire(&wiki)
        .expect("the refusal left the turn free");
}

#[test]
fn a_missing_lock_directory_is_unavailable_and_leaves_the_turn_free() {
    let (_dir, root, leases) = leases("nodir");
    let wiki = WikiId::new();
    // `prepare` was not called: the lock file's directory is not there.
    assert!(matches!(
        leases.try_acquire(&wiki),
        Err(LeaseRefusal::Unavailable(_))
    ));
    root.prepare(&wiki).unwrap();
    leases
        .try_acquire(&wiki)
        .expect("the failed attempt did not keep the turn");
}

#[test]
fn inside_a_scope_the_acts_of_that_wiki_on_that_thread_borrow_the_lease_and_release_nothing() {
    let (_dir, root, leases) = leases("scope");
    let wiki = WikiId::new();
    root.prepare(&wiki).unwrap();
    let lease = leases.try_acquire(&wiki).unwrap();
    {
        let _scope = lease.scope();
        assert!(is_lent(leases.locks(), &wiki));
        let guard = leases.locks().take(&root, &wiki).expect("borrowed");
        assert!(guard.borrowed, "no second lock was taken");
        drop(guard);
        // Dropping the borrowing guard gave nothing up: the lease is still held.
        assert_eq!(
            leases.try_acquire(&wiki).unwrap_err(),
            LeaseRefusal::ThisProcess
        );
    }
    assert!(
        !is_lent(leases.locks(), &wiki),
        "the loan ended with the scope"
    );
    drop(lease);
    let again = leases.try_acquire(&wiki).expect("released by the lease");
    drop(again);
}

#[test]
fn a_loan_is_for_that_wiki_that_thread_and_those_locks_only() {
    let (_dir, root, leases) = leases("narrow");
    let (wiki, other) = (WikiId::new(), WikiId::new());
    root.prepare(&wiki).unwrap();
    root.prepare(&other).unwrap();
    let lease = leases.try_acquire(&wiki).unwrap();
    let scope = lease.scope();
    // Another wiki on the same thread takes its own lock.
    let guard = leases.locks().take(&root, &other).expect("another wiki");
    assert!(
        !guard.borrowed,
        "a lease for one wiki lends nothing for another"
    );
    assert!(matches!(
        leases.try_acquire(&other),
        Err(LeaseRefusal::ThisProcess)
    ));
    drop(guard);
    // Other locks (a signer opened over leases of its own) borrow nothing.
    let stranger = WriteLeases::new(root.clone());
    assert!(!is_lent(stranger.locks(), &wiki));
    // Another thread waits for the lease.
    let (tx, rx) = mpsc::channel();
    let locks = Arc::clone(leases.locks());
    let (r, w) = (root.clone(), wiki.clone());
    let waiter = std::thread::spawn(move || {
        let guard = locks.take(&r, &w);
        tx.send(guard.map(|g| g.borrowed)).unwrap();
    });
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "a thread outside the scope did not get the wiki while the lease was held"
    );
    drop(scope);
    drop(lease);
    let got = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("it got its turn");
    assert_eq!(got.ok(), Some(false), "and took a lock of its own");
    waiter.join().unwrap();
}
