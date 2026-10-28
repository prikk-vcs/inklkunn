//! The counting harness (handoff 003 T3; RFC 003 D-2): how many prikk calls a request issued, so that a test
//! can say "none on a hit, at most two on a miss" and the next change that adds a call fails it.
//!
//! [`CountingStorage`] wraps any [`Storage`] and counts every call **as it is made**, whether it succeeds or
//! not (a call that was refused or timed out still cost a process). A test takes [`Calls`] before and after
//! the request and subtracts. The counters are process-wide for the wrapped storage, not per request: they
//! are for tests and for a metrics endpoint, and a test that counts runs one request at a time.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use inklkunn_prikk::{
    Change, Diff, ExtractBounds, FileMeta, History, Materialized, Point, Result, Storage, Tree,
    Verification, Views,
};

/// How many calls of each kind were made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Calls {
    /// `views` (prikk `branch list`).
    pub views: u64,
    /// `tree`.
    pub tree: u64,
    /// `stat` (prikk `cat --format json`).
    pub stat: u64,
    /// `read` (prikk `cat`).
    pub read: u64,
    /// `history` (prikk `log`).
    pub history: u64,
    /// `change` (prikk `show`).
    pub change: u64,
    /// `diff`.
    pub diff: u64,
    /// `plan` (prikk `extract --plan-only`).
    pub plan: u64,
    /// `materialize` (prikk `extract`).
    pub materialize: u64,
    /// `verify`.
    pub verify: u64,
}

impl Calls {
    /// All calls, of every kind.
    pub fn total(&self) -> u64 {
        self.views
            + self.tree
            + self.stat
            + self.read
            + self.history
            + self.change
            + self.diff
            + self.plan
            + self.materialize
            + self.verify
    }

    /// The calls made since `earlier` was taken.
    #[must_use]
    pub fn since(&self, earlier: &Calls) -> Calls {
        Calls {
            views: self.views.saturating_sub(earlier.views),
            tree: self.tree.saturating_sub(earlier.tree),
            stat: self.stat.saturating_sub(earlier.stat),
            read: self.read.saturating_sub(earlier.read),
            history: self.history.saturating_sub(earlier.history),
            change: self.change.saturating_sub(earlier.change),
            diff: self.diff.saturating_sub(earlier.diff),
            plan: self.plan.saturating_sub(earlier.plan),
            materialize: self.materialize.saturating_sub(earlier.materialize),
            verify: self.verify.saturating_sub(earlier.verify),
        }
    }
}

/// A [`Storage`] that counts the calls made through it.
#[derive(Debug, Default)]
pub struct CountingStorage<S> {
    inner: S,
    views: AtomicU64,
    tree: AtomicU64,
    stat: AtomicU64,
    read: AtomicU64,
    history: AtomicU64,
    change: AtomicU64,
    diff: AtomicU64,
    plan: AtomicU64,
    materialize: AtomicU64,
    verify: AtomicU64,
}

impl<S> CountingStorage<S> {
    /// Count the calls made to `inner`.
    pub fn new(inner: S) -> Self {
        CountingStorage {
            inner,
            views: AtomicU64::new(0),
            tree: AtomicU64::new(0),
            stat: AtomicU64::new(0),
            read: AtomicU64::new(0),
            history: AtomicU64::new(0),
            change: AtomicU64::new(0),
            diff: AtomicU64::new(0),
            plan: AtomicU64::new(0),
            materialize: AtomicU64::new(0),
            verify: AtomicU64::new(0),
        }
    }

    /// The storage counted.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// The calls made so far.
    pub fn calls(&self) -> Calls {
        let n = |c: &AtomicU64| c.load(Ordering::Relaxed);
        Calls {
            views: n(&self.views),
            tree: n(&self.tree),
            stat: n(&self.stat),
            read: n(&self.read),
            history: n(&self.history),
            change: n(&self.change),
            diff: n(&self.diff),
            plan: n(&self.plan),
            materialize: n(&self.materialize),
            verify: n(&self.verify),
        }
    }
}

fn count(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

impl<S: Storage> Storage for CountingStorage<S> {
    fn views(&self, repo: &Path) -> Result<Views> {
        count(&self.views);
        self.inner.views(repo)
    }

    fn tree(&self, repo: &Path, point: &Point, prefix: Option<&str>) -> Result<Tree> {
        count(&self.tree);
        self.inner.tree(repo, point, prefix)
    }

    fn stat(&self, repo: &Path, point: &Point, path: &str) -> Result<FileMeta> {
        count(&self.stat);
        self.inner.stat(repo, point, path)
    }

    fn read(&self, repo: &Path, point: &Point, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
        count(&self.read);
        self.inner.read(repo, point, path, max_bytes)
    }

    fn history(&self, repo: &Path, view: &Point, limit: Option<u64>) -> Result<History> {
        count(&self.history);
        self.inner.history(repo, view, limit)
    }

    fn change(&self, repo: &Path, id: &str) -> Result<Change> {
        count(&self.change);
        self.inner.change(repo, id)
    }

    fn diff(&self, repo: &Path, from: &Point, to: &Point, paths: &[&str]) -> Result<Diff> {
        count(&self.diff);
        self.inner.diff(repo, from, to, paths)
    }

    fn plan(&self, repo: &Path, point: &Point) -> Result<Materialized> {
        count(&self.plan);
        self.inner.plan(repo, point)
    }

    fn materialize(
        &self,
        repo: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        bounds: ExtractBounds,
    ) -> Result<Materialized> {
        count(&self.materialize);
        self.inner.materialize(repo, point, scratch, name, bounds)
    }

    fn verify(&self, repo: &Path) -> Result<Verification> {
        count(&self.verify);
        self.inner.verify(repo)
    }
}

#[cfg(test)]
mod tests;
