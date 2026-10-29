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

use std::sync::Arc;

use inklkunn_prikk::{Storage, StorageError, Unavailable};
use inklkunn_store::{
    RootLayout, Verification, VerificationState, WikiRecord, WikiStore, record::now_secs,
};

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
    /// The check did not run to completion. Not a verdict.
    CouldNotBeChecked(NotChecked),
}

/// A finished check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// What it found.
    pub outcome: CheckOutcome,
    /// When (Unix seconds).
    pub at: i64,
    /// Whether the wiki's record now says so. `false` when the store failed; the page then says what was
    /// found now and the badge keeps the record it had.
    pub recorded: bool,
    /// What went wrong, for the operator's log only; never shown to a reader.
    pub operator: Option<String>,
}

/// Runs the check.
pub struct Verifier {
    storage: Arc<dyn Storage>,
    wikis: Arc<dyn WikiStore>,
    layout: RootLayout,
    clock: fn() -> i64,
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
        }
    }

    /// The same, reading the time from `clock` (tests pin it).
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    /// Verify the wiki's repository now and record the outcome. **Blocks** the calling thread (prikk's
    /// `verify` is the longest read there is); an async caller runs it on a blocking thread.
    pub fn check(&self, wiki: &WikiRecord) -> Checked {
        let result = self.storage.verify(&self.layout.repo_dir(&wiki.id));
        let at = (self.clock)();
        let (outcome, operator) = classify(result);
        let state = match &outcome {
            CheckOutcome::Verified => VerificationState::Verified,
            CheckOutcome::Failed { .. } => VerificationState::Failed,
            CheckOutcome::CouldNotBeChecked(_) => VerificationState::CouldNotCheck,
        };
        let recorded = self
            .wikis
            .set_verification(
                &wiki.id,
                Verification {
                    state,
                    at: Some(at),
                },
            )
            .is_ok();
        Checked {
            outcome,
            at,
            recorded,
            operator,
        }
    }
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
