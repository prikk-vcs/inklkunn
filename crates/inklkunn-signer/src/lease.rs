//! The write lease's lock (RFC 004 D-4 step 1; internal design CON-1; handoff 004 T4): the per-wiki write lock of
//! the 002 corrections ([`crate::locks`]) taken **for longer than one act**, so that an acceptance holds the wiki
//! from the re-read of the tip to the sealed ids and the command line beside a running server is excluded for
//! all of it. This module is only the lock: the queue, the bounded wait, the durable holder record and the
//! words are `inklkunn-core`'s (`inklkunn_core::lease`).
//!
//! **How the acts inside a lease do not meet their own lock.** [`Signer::accept`](crate::Signer) and the keyless
//! [`Maintenance`](crate::Maintenance) operations take the wiki's write lock themselves, so that the command line
//! alone is as safe as the server. Inside a lease that would find the process busy. A lease therefore *lends*
//! itself to the calling thread for the length of a [`LeaseScope`]: an act on that thread, for that wiki, through
//! a signer or a maintenance handle opened over the same [`WriteLeases`], borrows the lease and releases
//! nothing. Nothing else borrows: another thread, another wiki, a signer opened over other leases and a thread
//! outside the scope take the lock as before and are told the wiki is busy. A scope is made only from a live
//! lease (it borrows it) and cannot leave its thread.
//!
//! No interface changes: [`Accept`](crate::Accept) is unchanged, and so is every signature the product calls.

use std::marker::PhantomData;
use std::sync::Arc;

use inklkunn_store::{RootLayout, WikiId};

use crate::flock::{self, Acquired, FileLock};
use crate::locks::{self, LOCK_GRACE, WikiLocks};

/// The in-process turns and the lock files of a root's wikis, shared by a signer, a maintenance handle and
/// the lease that wraps them. Open them all over the same `WriteLeases` (`Signer::with_leases`,
/// `Maintenance::with_leases`) and over the same root.
pub struct WriteLeases {
    root: RootLayout,
    locks: Arc<WikiLocks>,
}

impl std::fmt::Debug for WriteLeases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WriteLeases(..)")
    }
}

/// Why a lease was not given. Nothing was done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseRefusal {
    /// Another act of this process has the wiki (a lease, or an act that took the lock itself).
    ThisProcess,
    /// Another process has it (the command line, or a second server); `holder` is the process id it recorded,
    /// when it could be read.
    OtherProcess {
        /// The recorded holder.
        holder: Option<u32>,
    },
    /// The lock could not be taken at all (the lock file's directory is missing or unwritable). The text is
    /// for operators.
    Unavailable(String),
}

impl WriteLeases {
    /// The leases of the wikis under `root`.
    pub fn new(root: RootLayout) -> Arc<Self> {
        Arc::new(WriteLeases {
            root,
            locks: Arc::new(WikiLocks::default()),
        })
    }

    pub(crate) fn locks(&self) -> &Arc<WikiLocks> {
        &self.locks
    }

    /// Take the wiki's write lock for as long as the returned lease lives, **without waiting** for the one who
    /// has it (a short grace rides out a sibling thread's `fork`; a quarter of a second at most).
    pub fn try_acquire(self: &Arc<Self>, wiki: &WikiId) -> Result<WriteLease, LeaseRefusal> {
        if !self.locks.try_enter(wiki) {
            return Err(LeaseRefusal::ThisProcess);
        }
        match flock::exclusive_within(&self.root.lock_file(wiki), LOCK_GRACE) {
            Ok(Acquired::Held(file)) => Ok(WriteLease {
                leases: Arc::clone(self),
                wiki: wiki.clone(),
                file: Some(file),
            }),
            Ok(Acquired::Busy { holder }) => {
                self.locks.leave(wiki);
                Err(LeaseRefusal::OtherProcess { holder })
            }
            Err(e) => {
                self.locks.leave(wiki);
                Err(LeaseRefusal::Unavailable(format!(
                    "the wiki's write lock could not be taken ({})",
                    e.kind()
                )))
            }
        }
    }
}

/// A wiki's write lock, held until it is dropped (or the process dies: the kernel drops the file lock with its
/// holder). The file lock is released before the in-process turn is given up.
pub struct WriteLease {
    leases: Arc<WriteLeases>,
    wiki: WikiId,
    file: Option<FileLock>,
}

impl std::fmt::Debug for WriteLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WriteLease({})", self.wiki)
    }
}

impl WriteLease {
    /// The wiki.
    pub fn wiki(&self) -> &WikiId {
        &self.wiki
    }

    /// Lend the lease to this thread: until the scope drops, an act on this thread for this wiki through a
    /// signer or a maintenance handle opened over the same [`WriteLeases`] borrows it.
    pub fn scope(&self) -> LeaseScope<'_> {
        locks::lend(&self.leases.locks, &self.wiki);
        LeaseScope {
            lease: self,
            _this_thread: PhantomData,
        }
    }
}

impl Drop for WriteLease {
    fn drop(&mut self) {
        // The file first: the turn must not be given up while the file lock is still held, or the next
        // taker would find its own process busy.
        drop(self.file.take());
        self.leases.locks.leave(&self.wiki);
    }
}

/// A lease lent to the current thread. Not `Send`: the loan is the thread's.
pub struct LeaseScope<'a> {
    lease: &'a WriteLease,
    _this_thread: PhantomData<*const ()>,
}

impl Drop for LeaseScope<'_> {
    fn drop(&mut self) {
        locks::unlend(&self.lease.leases.locks, &self.lease.wiki);
    }
}

#[cfg(test)]
mod tests;
