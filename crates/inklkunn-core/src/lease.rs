//! The wiki's **write lease** (RFC 004 D-4 step 1; internal design CON-1, WR-3; external design OP-01, PERF-2;
//! handoff 004 T4): one act at a time writes a wiki's repository, and the act that holds the lease says so in a
//! record that outlives it.
//!
//! **Three layers, each for what it can be trusted with.**
//!
//! 1. *The queue* (here): acts of this process wait for the wiki in order, at most
//!    [`LeaseConfig::queue_bound`] (64, RFC 004 D-11) of them, and a waiter that has waited
//!    [`LeaseConfig::max_wait`] without its turn is told the wiki is **busy** with how many acts are ahead of
//!    it and an estimate of the wait ([`Busy`]; OP-01), not made to wait on. The queue is fair and in memory.
//! 2. *The lock* (`inklkunn-signer`, [`WriteLeases`]): the per-wiki `flock` of the 002 corrections, held for
//!    the whole lease. It orders this process against every other one, so the command line and a running
//!    server exclude each other, and the kernel drops it with its holder, so a killed process leaves nothing to
//!    clear. The acts that run inside the lease (the signer's `accept`, the keyless export and checkpoint
//!    refresh) borrow it through [`Lease::scope`] instead of taking it a second time.
//! 3. *The record* (the service store, [`LeaseHolder`]): written when the lease is taken and cleared when it is
//!    released. It is a **witness**, not the lock: a lease taken while a record of another holder is still
//!    there means that holder died holding it ([`Lease::replaced`]), which recovery (handoff 004 T5) reads
//!    next to the open intents.
//!
//! **Blocking, not async.** Every method here blocks the calling thread, as every service in this crate does
//! (the storage does; see [`inklkunn_prikk::Storage`]): an async caller takes the lease on a blocking thread.
//! The crate has no async runtime of its own, and the bounded wait is what keeps a blocked thread from being
//! a leak. [`WikiLeases::try_acquire`] is the form that never waits.
//!
//! **Every act that writes inside the repository takes the lease** (handoff 004 T4's binding note): an
//! acceptance, a system change, [`WikiLeases::export_archive`] and [`WikiLeases::refresh_checkpoint`], so that
//! an export can never make an acceptance meet prikk's `lock conflict` after its `commit`.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use inklkunn_prikk::storage::Verification;
use inklkunn_signer::{
    ArchiveExported, LeaseRefusal, LeaseScope, Maintenance, SignerError, WriteLease, WriteLeases,
};
use inklkunn_store::{AcceptanceId, AcceptanceStore, LeaseHolder, StoreError, WikiId};

/// How the lease queues and waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseConfig {
    /// The most acts that may wait for one wiki (RFC 004 D-11: 64). The one after them is told the wiki is
    /// busy at once.
    pub queue_bound: u64,
    /// How long an act waits for its turn before it is told the wiki is busy.
    pub max_wait: Duration,
    /// How often the act at the head of the queue asks the lock again (it may be held by another process,
    /// which cannot tell this one when it lets go).
    pub poll: Duration,
    /// What one lease is thought to take, before any has been seen: the estimate of a wait is the acts ahead
    /// times what the leases of this wiki have taken, recently.
    pub typical_hold: Duration,
}

impl LeaseConfig {
    /// The defaults, with the queue bounded as the operator's quotas say (RFC 004 D-11: per wiki, 64 waiting).
    pub fn bounded_by(quotas: &crate::proposal::QuotaConfig) -> Self {
        LeaseConfig {
            queue_bound: quotas.acceptance_queue,
            ..LeaseConfig::default()
        }
    }
}

impl Default for LeaseConfig {
    fn default() -> Self {
        LeaseConfig {
            queue_bound: 64,
            max_wait: Duration::from_secs(10),
            poll: Duration::from_millis(20),
            typical_hold: Duration::from_secs(5),
        }
    }
}

/// What a lease is held for: a short fixed word in the holder record (CON-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// A proposal's acceptance.
    Acceptance,
    /// A change the service makes for itself (an import, a system change).
    System,
    /// An archive export.
    Export,
    /// The checkpoint refresh.
    Refresh,
    /// Recovery (handoff 004 T5).
    Recovery,
}

impl Purpose {
    /// The word in the record.
    pub const fn as_str(self) -> &'static str {
        match self {
            Purpose::Acceptance => "acceptance",
            Purpose::System => "system",
            Purpose::Export => "export",
            Purpose::Refresh => "refresh",
            Purpose::Recovery => "recovery",
        }
    }
}

/// Why the wiki could not be had now (OP-01: *the wiki is busy*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusyWhy {
    /// The wait ran out before the turn came.
    Waited,
    /// So many acts already wait that this one was not queued.
    QueueFull {
        /// The bound that was reached.
        bound: u64,
        /// How many acts were waiting (the holder not counted).
        waiting: u64,
    },
    /// Another process holds the wiki (the command line, or a second server); `holder` is the process id it
    /// recorded, when it could be read. This process cannot be told when it lets go.
    OtherProcess {
        /// The recorded holder.
        holder: Option<u32>,
    },
}

/// *The wiki is busy*, with what a person needs to decide whether to wait: how many acts stand between them
/// and the wiki, and about how long that takes. Nothing was done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy {
    /// The acts ahead: the one that holds the wiki (when one does) and the ones waiting before this one.
    pub depth: u64,
    /// About how long they take, from what leases of this wiki have taken lately (never zero when
    /// `depth` is not).
    pub estimate: Duration,
    /// Why.
    pub why: BusyWhy,
}

/// Why a lease was not given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseError {
    /// The wiki is busy now.
    Busy(Busy),
    /// The lease could not be taken or recorded at all (the lock file's directory, the store). The text is for
    /// operators.
    Unavailable(String),
}

impl std::fmt::Display for LeaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeaseError::Busy(b) => write!(
                f,
                "the wiki is busy ({} ahead, about {} s)",
                b.depth,
                b.estimate.as_secs()
            ),
            LeaseError::Unavailable(why) => write!(f, "the write lease could not be taken: {why}"),
        }
    }
}

impl std::error::Error for LeaseError {}

/// One wiki's queue, in memory.
#[derive(Debug, Default)]
struct Line {
    /// The tickets of the acts waiting, in the order they came.
    waiting: VecDeque<u64>,
    next_ticket: u64,
    /// An act of this process holds the lease.
    held: bool,
    /// What the leases of this wiki have taken, lately (an exponential average).
    average_hold: Option<Duration>,
}

impl Line {
    /// The acts ahead of the ticket (all of them, for a ticket not in the line).
    fn ahead_of(&self, ticket: Option<u64>) -> u64 {
        let before = match ticket {
            Some(t) => self.waiting.iter().take_while(|w| **w != t).count(),
            None => self.waiting.len(),
        };
        u64::try_from(before).unwrap_or(u64::MAX) + u64::from(self.held)
    }
}

/// The write leases of a process's wikis. Cheap to share (`Arc`); every method takes `&self`.
pub struct WikiLeases {
    signer: Arc<WriteLeases>,
    store: Arc<dyn AcceptanceStore>,
    /// The name this process gives itself in the holder record: an instance, never a person.
    holder: String,
    config: LeaseConfig,
    lines: Mutex<HashMap<WikiId, Line>>,
    turn: Condvar,
}

impl std::fmt::Debug for WikiLeases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WikiLeases")
    }
}

impl WikiLeases {
    /// The leases of the wikis `signer` locks, recorded in `store`. `holder` names this process in the holder
    /// record (the binary gives each process its own: `inklkunn-<pid>`, say; two processes with one name could
    /// clear each other's record).
    pub fn new(
        signer: Arc<WriteLeases>,
        store: Arc<dyn AcceptanceStore>,
        holder: impl Into<String>,
        config: LeaseConfig,
    ) -> Arc<Self> {
        Arc::new(WikiLeases {
            signer,
            store,
            holder: holder.into(),
            config,
            lines: Mutex::new(HashMap::new()),
            turn: Condvar::new(),
        })
    }

    /// The signer's leases this one is built on: a signer and a maintenance handle opened over them
    /// (`Signer::with_leases`, `Maintenance::with_leases`) run inside a lease's [`Lease::scope`].
    pub fn signer_leases(&self) -> &Arc<WriteLeases> {
        &self.signer
    }

    /// The limits and times in force.
    pub fn config(&self) -> &LeaseConfig {
        &self.config
    }

    fn lines(&self) -> MutexGuard<'_, HashMap<WikiId, Line>> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many acts of this process wait for the wiki now (the holder not counted): the visible queue
    /// (RFC 004 D-11).
    pub fn waiting(&self, wiki: &WikiId) -> u64 {
        self.lines()
            .get(wiki)
            .map_or(0, |l| u64::try_from(l.waiting.len()).unwrap_or(u64::MAX))
    }

    /// Whether an act of this process holds the wiki now.
    pub fn is_held(&self, wiki: &WikiId) -> bool {
        self.lines().get(wiki).is_some_and(|l| l.held)
    }

    /// The durable record of who holds the wiki, if there is one (a witness: see the module documentation).
    pub fn recorded_holder(&self, wiki: &WikiId) -> Result<Option<LeaseHolder>, StoreError> {
        self.store.lease_holder(wiki)
    }

    fn estimate(&self, line: &Line, depth: u64) -> Duration {
        let each = line.average_hold.unwrap_or(self.config.typical_hold);
        each.saturating_mul(u32::try_from(depth).unwrap_or(u32::MAX))
    }

    fn busy(&self, line: &Line, ticket: Option<u64>, why: BusyWhy) -> LeaseError {
        let depth = line.ahead_of(ticket);
        LeaseError::Busy(Busy {
            depth,
            estimate: self.estimate(line, depth),
            why,
        })
    }

    /// Take the wiki's lease for `purpose`, waiting in turn for at most [`LeaseConfig::max_wait`].
    /// `acceptance` is the acceptance the lease is held for, when it already has a record (it may be
    /// attached later: [`Lease::attach`]).
    pub fn acquire(
        self: &Arc<Self>,
        wiki: &WikiId,
        purpose: Purpose,
        acceptance: Option<&AcceptanceId>,
    ) -> Result<Lease, LeaseError> {
        let deadline = Instant::now() + self.config.max_wait;
        let ticket = {
            let mut lines = self.lines();
            let line = lines.entry(wiki.clone()).or_default();
            let waiting = u64::try_from(line.waiting.len()).unwrap_or(u64::MAX);
            if waiting >= self.config.queue_bound {
                return Err(self.busy(
                    line,
                    None,
                    BusyWhy::QueueFull {
                        bound: self.config.queue_bound,
                        waiting,
                    },
                ));
            }
            let ticket = line.next_ticket;
            line.next_ticket += 1;
            line.waiting.push_back(ticket);
            ticket
        };
        let mut other_process: Option<Option<u32>> = None;
        loop {
            let mut lines = self.lines();
            let head = lines
                .get(wiki)
                .is_some_and(|l| !l.held && l.waiting.front() == Some(&ticket));
            if head {
                // Only the head asks the lock, and not with the queue's mutex held: a lock held by another
                // process is waited on for a moment (the signer's grace).
                drop(lines);
                match self.signer.try_acquire(wiki) {
                    Ok(signer) => return self.took(wiki, ticket, signer, purpose, acceptance),
                    Err(LeaseRefusal::Unavailable(why)) => {
                        self.leave(wiki, ticket);
                        return Err(LeaseError::Unavailable(why));
                    }
                    Err(LeaseRefusal::OtherProcess { holder }) => other_process = Some(holder),
                    // An act of this process that took the lock without the queue (a key act).
                    Err(LeaseRefusal::ThisProcess) => other_process = None,
                }
                lines = self.lines();
            }
            let now = Instant::now();
            if now >= deadline {
                let error = lines.get(wiki).map(|line| {
                    let why = match other_process {
                        Some(holder) => BusyWhy::OtherProcess { holder },
                        None => BusyWhy::Waited,
                    };
                    self.busy(line, Some(ticket), why)
                });
                drop(lines);
                self.leave(wiki, ticket);
                return Err(error.unwrap_or_else(|| {
                    LeaseError::Unavailable("the wiki's queue was lost".to_owned())
                }));
            }
            let wait = (deadline - now).min(if head {
                self.config.poll
            } else {
                // Not at the head: woken when the line moves; the timeout only bounds a missed wake-up.
                self.config.poll.saturating_mul(10)
            });
            let (guard, _) = self
                .turn
                .wait_timeout(lines, wait)
                .unwrap_or_else(PoisonError::into_inner);
            drop(guard);
        }
    }

    /// Take the lease only if it is free this instant: `Busy` at once otherwise (and never queued behind
    /// others).
    pub fn try_acquire(
        self: &Arc<Self>,
        wiki: &WikiId,
        purpose: Purpose,
        acceptance: Option<&AcceptanceId>,
    ) -> Result<Lease, LeaseError> {
        let ticket = {
            let mut lines = self.lines();
            let line = lines.entry(wiki.clone()).or_default();
            if line.held || !line.waiting.is_empty() {
                return Err(self.busy(line, None, BusyWhy::Waited));
            }
            // A ticket of its own, so that a concurrent `acquire` does not take the head from under it.
            let ticket = line.next_ticket;
            line.next_ticket += 1;
            line.waiting.push_back(ticket);
            ticket
        };
        match self.signer.try_acquire(wiki) {
            Ok(signer) => self.took(wiki, ticket, signer, purpose, acceptance),
            Err(refusal) => {
                self.leave(wiki, ticket);
                Err(match refusal {
                    LeaseRefusal::Unavailable(why) => LeaseError::Unavailable(why),
                    LeaseRefusal::OtherProcess { holder } => {
                        self.busy_now(wiki, BusyWhy::OtherProcess { holder })
                    }
                    LeaseRefusal::ThisProcess => self.busy_now(wiki, BusyWhy::Waited),
                })
            }
        }
    }

    fn busy_now(&self, wiki: &WikiId, why: BusyWhy) -> LeaseError {
        let lines = self.lines();
        match lines.get(wiki) {
            Some(line) => self.busy(line, None, why),
            None => self.busy(&Line::default(), None, why),
        }
    }

    /// The ticket's turn has come and the lock is held: record it, and become the holder.
    fn took(
        self: &Arc<Self>,
        wiki: &WikiId,
        ticket: u64,
        signer: WriteLease,
        purpose: Purpose,
        acceptance: Option<&AcceptanceId>,
    ) -> Result<Lease, LeaseError> {
        let record = LeaseHolder {
            wiki: wiki.clone(),
            holder: self.holder.clone(),
            purpose: purpose.as_str().to_owned(),
            acceptance: acceptance.cloned(),
            since: inklkunn_store::record::now_secs(),
        };
        let replaced = match self.store.record_lease_holder(&record) {
            Ok(previous) => previous,
            Err(e) => {
                // Nothing was started: give the lock back before saying so.
                drop(signer);
                self.leave(wiki, ticket);
                return Err(LeaseError::Unavailable(format!(
                    "the lease holder could not be recorded: {e}"
                )));
            }
        };
        {
            let mut lines = self.lines();
            let line = lines.entry(wiki.clone()).or_default();
            line.waiting.retain(|t| *t != ticket);
            line.held = true;
        }
        self.turn.notify_all();
        Ok(Lease {
            owner: Arc::clone(self),
            wiki: wiki.clone(),
            // A record of this very holder is this process's own (a lease it did not release is a bug, but a
            // record of itself says nothing about another's death).
            replaced: replaced.filter(|p| p.holder != self.holder),
            signer,
            _done: Done {
                owner: Arc::clone(self),
                wiki: wiki.clone(),
                taken: Instant::now(),
            },
        })
    }

    /// Give up a place in the line.
    fn leave(&self, wiki: &WikiId, ticket: u64) {
        if let Some(line) = self.lines().get_mut(wiki) {
            line.waiting.retain(|t| *t != ticket);
        }
        self.turn.notify_all();
    }

    /// Write an archive of the wiki to `destination` under the wiki's lease (handoff 004 T4's binding note:
    /// the export holds the lease, so it can never make an acceptance meet prikk's `lock conflict` after its
    /// `commit`). `maintenance` must be opened over this lease's [`WikiLeases::signer_leases`].
    pub fn export_archive(
        self: &Arc<Self>,
        maintenance: &Maintenance,
        wiki: &WikiId,
        destination: &Path,
    ) -> Result<ArchiveExported, Leased<SignerError>> {
        let lease = self
            .acquire(wiki, Purpose::Export, None)
            .map_err(Leased::NotLeased)?;
        let _scope = lease.scope();
        maintenance
            .archive_export(wiki, destination)
            .map_err(Leased::Failed)
    }

    /// Re-record prikk's checkpoint under the wiki's lease (handoff 004 T4's binding note). As
    /// [`WikiLeases::export_archive`].
    pub fn refresh_checkpoint(
        self: &Arc<Self>,
        maintenance: &Maintenance,
        wiki: &WikiId,
    ) -> Result<Verification, Leased<SignerError>> {
        let lease = self
            .acquire(wiki, Purpose::Refresh, None)
            .map_err(Leased::NotLeased)?;
        let _scope = lease.scope();
        maintenance.refresh_checkpoint(wiki).map_err(Leased::Failed)
    }
}

/// An act done under a lease that was not done: the lease was not given (nothing was done), or the act failed
/// inside it.
#[derive(Debug)]
pub enum Leased<E> {
    /// The lease was not given.
    NotLeased(LeaseError),
    /// The act ran and failed.
    Failed(E),
}

impl<E: std::fmt::Display> std::fmt::Display for Leased<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Leased::NotLeased(e) => e.fmt(f),
            Leased::Failed(e) => e.fmt(f),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for Leased<E> {}

/// A wiki's write lease, held until it is dropped: the record is cleared first, then the lock is given back,
/// then the line is told (the fields drop in the order they are declared).
pub struct Lease {
    owner: Arc<WikiLeases>,
    wiki: WikiId,
    replaced: Option<LeaseHolder>,
    // The lock goes before the line is told that the wiki is free, or the next in line would find this
    // process busy.
    signer: WriteLease,
    _done: Done,
}

/// What a lease does last: tell the line.
struct Done {
    owner: Arc<WikiLeases>,
    wiki: WikiId,
    taken: Instant,
}

impl Drop for Done {
    fn drop(&mut self) {
        let held_for = self.taken.elapsed();
        {
            let mut lines = self.owner.lines();
            let line = lines.entry(self.wiki.clone()).or_default();
            line.held = false;
            line.average_hold = Some(match line.average_hold {
                // An exponential average: a quarter of the new figure, three quarters of what was thought.
                Some(avg) => (avg.saturating_mul(3) + held_for) / 4,
                None => held_for,
            });
        }
        self.owner.turn.notify_all();
    }
}

impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lease({})", self.wiki)
    }
}

impl Lease {
    /// The wiki.
    pub fn wiki(&self) -> &WikiId {
        &self.wiki
    }

    /// The record of another holder that was still there when this lease was taken: **that holder died
    /// holding the lease** (a holder that releases clears its record, and the lock is free, or this lease
    /// could not have been taken). What it was doing is not known; recovery reads the open intents.
    pub fn replaced(&self) -> Option<&LeaseHolder> {
        self.replaced.as_ref()
    }

    /// Lend the lease to this thread: until the scope drops, the signer's `accept` and the maintenance
    /// operations (opened over [`WikiLeases::signer_leases`]) borrow it instead of taking the lock again.
    pub fn scope(&self) -> LeaseScope<'_> {
        self.signer.scope()
    }

    /// Record that the lease is now held for `acceptance` (its record exists from here on).
    pub fn attach(&self, acceptance: &AcceptanceId) -> Result<(), StoreError> {
        self.owner
            .store
            .record_lease_holder(&LeaseHolder {
                wiki: self.wiki.clone(),
                holder: self.owner.holder.clone(),
                purpose: Purpose::Acceptance.as_str().to_owned(),
                acceptance: Some(acceptance.clone()),
                since: inklkunn_store::record::now_secs(),
            })
            .map(|_| ())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        // The record first, while the lock still keeps another process from writing its own: a failure to clear
        // it leaves a witness behind that the next lease reports as a holder that died, which is the safe way
        // to be wrong.
        let _ = self
            .owner
            .store
            .release_lease_holder(&self.wiki, &self.owner.holder);
    }
}

#[cfg(test)]
mod tests;
