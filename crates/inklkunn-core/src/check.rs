//! The verification check (RFC 003 D-5; RFC 002 D-6; threat model INV-6): ask prikk to verify a wiki's
//! repository, say what came of it in the terms a page chooses its words by, and record it as the wiki's last
//! verification (the badge reads the record, never the page cache).
//!
//! **A check ends in one of three ways, and only the first two are about the repository.** *Verified*: prikk
//! ran every stage and found nothing. *Failed*: prikk ran and found a problem; the stages that did not come out
//! well are named, as data, because a stage name is prikk's and the same everywhere. *Could not be checked
//! now*: prikk did not run to completion (busy, over its memory ceiling, out of time, not installed): this is
//! **not a verdict** and says nothing about the repository, and the driver never turns a memory limit into a
//! failed verification (handoff follow-on IS-7).
//!
//! **A check that could not run never replaces a verdict** (handoff 003, review R1; INT-6). The wiki's record
//! keeps the last verdict (*Verified* or *Failed*, with its time) and, apart from it, the last attempt that
//! could not run; a later verdict clears the attempt, an attempt never touches the verdict. Otherwise a visitor
//! who keeps the heavy pool busy could turn *Verification failed* into *Could not be checked now*, and an
//! acceptance (INT-6) would read the weaker state.
//!
//! **One check per wiki at a time, and a few at a time overall.** A check is claimed before it runs: a second
//! request for a wiki whose check is running is told so ([`Declined::Running`]), more checks than the cap are
//! told the server is busy ([`Declined::Crowded`]), and a check is refused if the last one that ran is under
//! [`MIN_RECHECK_SECS`] old ([`Declined::Recent`]; a recorded time in the future counts as recent). The
//! recency is read **inside** the claim, from the record as it stands, so two requests cannot both pass it.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_prikk::{Storage, StorageError, Unavailable};
use inklkunn_store::{
    Attempt, NotRun, RootLayout, Verification, VerificationState, WikiRecord, WikiStore,
    record::now_secs,
};

/// A check that ran (or could not run) less than this long ago is not repeated.
pub const MIN_RECHECK_SECS: i64 = 60;

/// How many checks may run at once, over all wikis. `verify` is the longest read there is and its cost grows
/// with the repository, which is known only when it asks the gate for its permit; a count below what the heavy
/// pool can hold keeps a visitor who asks every wiki at once from filling the pool.
pub const CONCURRENT_CHECKS: usize = 2;

/// A stage that did not come out well, as prikk names it. Data, not words: a page shows the name as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageFinding {
    /// prikk's name for the stage (`objects`, `refs`, …).
    pub stage: String,
    /// `failed` or `not_evaluated` (never `evaluated`).
    pub status: String,
}

/// Why a check could not run to completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotChecked {
    /// The server was busy with other work; try again soon.
    Busy,
    /// The check was stopped for want of memory or time; the repository may be larger than the server is set
    /// up to check.
    Limit,
    /// The check could not be started or its answer could not be read: the operator's to look at.
    Unavailable,
}

/// Why a request for a check was turned away **before anything ran**. Nothing is recorded for these: no attempt
/// was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declined {
    /// A check of this wiki is running now.
    Running,
    /// The last check that ran is under [`MIN_RECHECK_SECS`] old.
    Recent,
    /// The server is already running as many checks as it allows.
    Crowded,
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// Every stage ran and found nothing wrong.
    Verified,
    /// A stage found a problem.
    Failed {
        /// The stages that did not come out well, in prikk's order.
        stages: Vec<StageFinding>,
    },
    /// The check did not run to completion. Not a verdict, and the last verdict stands.
    CouldNotBeChecked(NotChecked),
    /// The request was turned away before a check began.
    Declined(Declined),
}

/// A finished check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// What it found.
    pub outcome: CheckOutcome,
    /// When (Unix seconds).
    pub at: i64,
    /// Whether the wiki's record now says so. `false` when the store failed; the page then says what was
    /// found now and the badge keeps the record it had. Meaningful only when [`Checked::ran`].
    pub recorded: bool,
    /// What went wrong, for the operator's log only; never shown to a reader.
    pub operator: Option<String>,
}

impl Checked {
    /// Did a check start? `false` for a request that was [`Declined`]: nothing ran and nothing was recorded.
    pub fn ran(&self) -> bool {
        !matches!(self.outcome, CheckOutcome::Declined(_))
    }
}

/// Runs the check.
pub struct Verifier {
    storage: Arc<dyn Storage>,
    wikis: Arc<dyn WikiStore>,
    layout: RootLayout,
    clock: fn() -> i64,
    cap: usize,
    running: Mutex<BTreeSet<String>>,
}

/// A claim on one wiki's check, given back when it is dropped (also when the check panics).
struct Claim<'a> {
    running: &'a Mutex<BTreeSet<String>>,
    id: String,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Verifier")
    }
}

impl Verifier {
    /// A verifier over `storage` for the wikis under `layout`, recording into `wikis`.
    pub fn new(storage: Arc<dyn Storage>, wikis: Arc<dyn WikiStore>, layout: RootLayout) -> Self {
        Verifier {
            storage,
            wikis,
            layout,
            clock: now_secs,
            cap: CONCURRENT_CHECKS,
            running: Mutex::new(BTreeSet::new()),
        }
    }

    /// The same, allowing `cap` checks at once (at least one).
    #[must_use]
    pub fn with_concurrency(mut self, cap: usize) -> Self {
        self.cap = cap.max(1);
        self
    }

    /// The same, reading the time from `clock` (tests pin it).
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    fn claim(&self, wiki: &WikiRecord) -> Result<Claim<'_>, Declined> {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let id = wiki.id.as_str().to_owned();
        if running.contains(&id) {
            return Err(Declined::Running);
        }
        if running.len() >= self.cap {
            return Err(Declined::Crowded);
        }
        running.insert(id.clone());
        Ok(Claim {
            running: &self.running,
            id,
        })
    }

    /// Verify the wiki's repository now and record the outcome. **Blocks** the calling thread (prikk's
    /// `verify` is the longest read there is); an async caller runs it on a blocking thread. A request that
    /// cannot be served now comes back [`CheckOutcome::Declined`] with nothing recorded.
    pub fn check(&self, wiki: &WikiRecord) -> Checked {
        let declined = |why| Checked {
            outcome: CheckOutcome::Declined(why),
            at: (self.clock)(),
            recorded: false,
            operator: None,
        };
        let claim = match self.claim(wiki) {
            Ok(claim) => claim,
            Err(why) => return declined(why),
        };
        // The record as it stands now, read inside the claim: a check that finished a moment ago is in it.
        let current = self.wikis.get(&wiki.id).unwrap_or_else(|_| wiki.clone());
        if is_recent(&current, (self.clock)()) {
            return declined(Declined::Recent);
        }
        let result = self.storage.verify(&self.layout.repo_dir(&wiki.id));
        let at = (self.clock)();
        let (outcome, operator) = classify(result);
        let recorded = match &outcome {
            CheckOutcome::Verified => self.verdict(wiki, VerificationState::Verified, at),
            CheckOutcome::Failed { .. } => self.verdict(wiki, VerificationState::Failed, at),
            CheckOutcome::CouldNotBeChecked(why) => self
                .wikis
                .record_attempt(
                    &wiki.id,
                    Attempt {
                        at,
                        why: match why {
                            NotChecked::Busy => NotRun::Busy,
                            NotChecked::Limit => NotRun::Limit,
                            NotChecked::Unavailable => NotRun::Unavailable,
                        },
                    },
                )
                .is_ok(),
            CheckOutcome::Declined(_) => false,
        };
        drop(claim);
        Checked {
            outcome,
            at,
            recorded,
            operator,
        }
    }

    fn verdict(&self, wiki: &WikiRecord, state: VerificationState, at: i64) -> bool {
        self.wikis
            .set_verification(
                &wiki.id,
                Verification {
                    state,
                    at: Some(at),
                },
            )
            .is_ok()
    }
}

/// Did a check run, or fail to, less than [`MIN_RECHECK_SECS`] ago? An attempt that was only turned away by
/// the gate (*busy*) cost nothing and does not count; a time in the future (a clock set back) counts as recent.
fn is_recent(wiki: &WikiRecord, now: i64) -> bool {
    let attempt = wiki
        .last_attempt
        .filter(|a| a.why != NotRun::Busy)
        .map(|a| a.at);
    wiki.verification
        .at
        .into_iter()
        .chain(attempt)
        .any(|at| now - at < MIN_RECHECK_SECS)
}

fn classify(
    result: Result<inklkunn_prikk::Verification, StorageError>,
) -> (CheckOutcome, Option<String>) {
    match result {
        Ok(report) if report.verdict.ok => (CheckOutcome::Verified, None),
        Ok(report) => (
            CheckOutcome::Failed {
                stages: report
                    .stages
                    .iter()
                    .filter(|s| s.status != "evaluated")
                    .map(|s| StageFinding {
                        stage: s.stage.clone(),
                        status: s.status.clone(),
                    })
                    .collect(),
            },
            None,
        ),
        Err(e) => {
            let why = match &e {
                StorageError::Unavailable {
                    reason: Unavailable::Busy,
                    ..
                } => NotChecked::Busy,
                StorageError::Unavailable {
                    reason:
                        Unavailable::CeilingExceeded { .. }
                        | Unavailable::TimedOut { .. }
                        | Unavailable::OutputExceeded,
                    ..
                }
                | StorageError::TooLarge { .. } => NotChecked::Limit,
                _ => NotChecked::Unavailable,
            };
            (CheckOutcome::CouldNotBeChecked(why), Some(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests;
