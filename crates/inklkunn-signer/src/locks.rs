//! **One writer per wiki** (corrections handoff C3; threat model CON-1, INT-2), shared by every act that writes
//! a wiki's repository: the [`Signer`](crate::Signer)'s acceptances and key acts, and the keyless
//! [`Maintenance`](crate::Maintenance) operations (`archive export`, the checkpoint refresh; handoff 003 T7, IS-8).
//! An acceptance is a sequence (status, switch, write files, commit, seal, read back, verify) over one worktree;
//! two at once would commit each other's files under each other's messages, and the command line beside a
//! running server is as much a second writer as a second thread. So every act that writes a wiki's repository
//! or key record takes
//!
//! 1. the in-process turn (a thread of this process waits for another thread's act on the same wiki, as it
//!    always did), and then
//! 2. an exclusive `flock` on the wiki's lock file in inklkunn's data directory
//!    ([`RootLayout::lock_file`], never inside `.prikk/`), which excludes every other process **and answers at
//!    once**: a caller that cannot have it is told the wiki is busy, naming the process that holds it, and never
//!    waits for it.
//!
//! The kernel drops the lock with its holder, so a killed process leaves nothing to clear. RFC 004's lease is
//! built on this lock ([`crate::lease`]): a lease takes the turn and the file lock for the length of an
//! acceptance, and the acts that run inside it (the signer's `accept`, the keyless export and checkpoint
//! refresh) **borrow** it instead of taking it a second time, which would find their own process busy. A borrow
//! is by scope on one thread, made only from a live lease ([`crate::lease::LeaseScope`]).

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

use inklkunn_store::{RootLayout, WikiId};

use crate::flock::{self, Acquired, FileLock};
use crate::writes::SignerError;

/// How long a caller rides out a lock that is held: the moment a sibling thread's `fork` holds a copy of a
/// descriptor just released (the child closes it at `exec`). Not a wait for the holder's work.
pub(crate) const LOCK_GRACE: Duration = Duration::from_millis(250);

/// The in-process turns, one per wiki.
#[derive(Default)]
pub(crate) struct WikiLocks {
    held: Mutex<HashSet<WikiId>>,
    turn: Condvar,
}

thread_local! {
    /// The leases the current thread is inside of: the address of the [`WikiLocks`] a lease belongs to and the
    /// wiki. Pushed by a [`crate::lease::LeaseScope`] and popped when it drops.
    static BORROWABLE: RefCell<Vec<(usize, WikiId)>> = const { RefCell::new(Vec::new()) };
}

/// Put `(locks, wiki)` among the leases this thread may borrow.
pub(crate) fn lend(locks: &WikiLocks, wiki: &WikiId) {
    BORROWABLE.with(|b| b.borrow_mut().push((address(locks), wiki.clone())));
}

/// Take it out again (the most recent one).
pub(crate) fn unlend(locks: &WikiLocks, wiki: &WikiId) {
    BORROWABLE.with(|b| {
        let mut b = b.borrow_mut();
        let key = (address(locks), wiki.clone());
        if let Some(at) = b.iter().rposition(|k| *k == key) {
            b.remove(at);
        }
    });
}

fn address(locks: &WikiLocks) -> usize {
    std::ptr::from_ref(locks) as usize
}

pub(crate) fn is_lent(locks: &WikiLocks, wiki: &WikiId) -> bool {
    let key = (address(locks), wiki.clone());
    BORROWABLE.with(|b| b.borrow().contains(&key))
}

/// A wiki's write lock, held for one act (or, when `borrowed`, the lease an enclosing scope holds: nothing is
/// released when it drops).
pub(crate) struct WriteGuard<'a> {
    pub(crate) locks: &'a WikiLocks,
    pub(crate) wiki: WikiId,
    pub(crate) file: Option<FileLock>,
    pub(crate) borrowed: bool,
}

impl Drop for WriteGuard<'_> {
    fn drop(&mut self) {
        if self.borrowed {
            return;
        }
        // The file first: the in-process turn must not be given up while the file lock is still held, or the
        // next thread would find its own process "busy".
        drop(self.file.take());
        let mut held = self
            .locks
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        held.remove(&self.wiki);
        self.locks.turn.notify_all();
        drop(held);
        // Tests widen the gap in which a wrongly ordered release would be seen (see the test that pins the order).
        #[cfg(test)]
        std::thread::sleep(Duration::from_millis(40));
    }
}

impl WikiLocks {
    pub(crate) fn enter(&self, wiki: &WikiId) {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        while held.contains(wiki) {
            held = self.turn.wait(held).unwrap_or_else(PoisonError::into_inner);
        }
        held.insert(wiki.clone());
    }

    /// Take the turn if nobody has it, without waiting.
    pub(crate) fn try_enter(&self, wiki: &WikiId) -> bool {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.insert(wiki.clone())
    }

    pub(crate) fn leave(&self, wiki: &WikiId) {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.remove(wiki);
        self.turn.notify_all();
    }

    /// Take the wiki's write lock for one act under `root`'s layout, or say who holds it.
    pub(crate) fn take<'a>(
        &'a self,
        root: &RootLayout,
        wiki: &WikiId,
    ) -> Result<WriteGuard<'a>, SignerError> {
        if is_lent(self, wiki) {
            return Ok(WriteGuard {
                locks: self,
                wiki: wiki.clone(),
                file: None,
                borrowed: true,
            });
        }
        self.enter(wiki);
        match flock::exclusive_within(&root.lock_file(wiki), LOCK_GRACE) {
            Ok(Acquired::Held(file)) => Ok(WriteGuard {
                locks: self,
                wiki: wiki.clone(),
                file: Some(file),
                borrowed: false,
            }),
            Ok(Acquired::Busy { holder }) => {
                self.leave(wiki);
                Err(SignerError::Busy {
                    wiki: wiki.to_string(),
                    holder,
                })
            }
            Err(e) => {
                self.leave(wiki);
                Err(SignerError::Unavailable(format!(
                    "the wiki's write lock could not be taken ({})",
                    e.kind()
                )))
            }
        }
    }
}
