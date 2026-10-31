//! What the proposal service and the acceptance refuse, as problem details (RFC 004 D-6, D-11, D-12; API-04).
//!
//! Every refusal is one fixed code with the facts a program acts on as members (a named limit, its numbers and
//! when it ends; the state the proposal is in; the revision that is the latest). **A refusal that is the store's
//! or the repository's own failure is the operator's business**: its cause goes to the log and the caller gets
//! `unavailable` with `Retry-After`. `NotFound` is the same answer for a proposal that is absent and one the
//! caller may not see (a draft is its author's alone).

use inklkunn_core::acceptance::AcceptFailure;
use inklkunn_core::proposal::{Invalid, ProposalError, QuotaRefusal, Reset};
use serde_json::json;

use super::Request;
use super::base::{ProposalTag, stale};
use crate::api::problem::{Code, Problem};
use crate::proposals::words;

/// How long a caller is told to wait for a queue that moves: a lease is given in turn and takes seconds.
const QUEUE_RETRY: u64 = 5;

/// The stable word of what was wrong with what was sent.
pub(crate) const fn invalid_word(why: Invalid) -> &'static str {
    match why {
        Invalid::SummaryMissing => "summary-missing",
        Invalid::SummaryTooLong => "summary-too-long",
        Invalid::SummaryNotOneLine => "summary-not-one-line",
        Invalid::TooManySources => "too-many-sources",
        Invalid::BadSource => "bad-source",
        Invalid::NothingProposed => "nothing-proposed",
        Invalid::TooLarge => "too-large",
        Invalid::PageTooLarge => "page-too-large",
        Invalid::BadPath => "bad-path",
        Invalid::DuplicatePath => "duplicate-path",
        Invalid::RenameToSelf => "rename-to-self",
        Invalid::BadBase => "bad-base",
        Invalid::ReasonMissing => "reason-missing",
        Invalid::ReasonTooLong => "reason-too-long",
        Invalid::CommentEmpty => "comment-empty",
        Invalid::CommentTooLong => "comment-too-long",
        Invalid::BadAnchor => "bad-anchor",
        Invalid::SupersedeSelf => "supersede-self",
    }
}

impl Request {
    /// `422 unacceptable`: the stable word, and the sentence the pages use for it in the caller's language.
    pub(crate) fn unacceptable(&self, why: Invalid) -> Problem {
        Problem::new(Code::Unacceptable)
            .named("reason", invalid_word(why))
            .said("explanation", words::invalid_text(self.t, why))
    }

    /// A refusal of a page the API checks before the service does (the same words, a word of its own).
    pub(crate) fn unacceptable_with(&self, reason: &'static str, explanation: String) -> Problem {
        Problem::new(Code::Unacceptable)
            .named("reason", reason)
            .said("explanation", explanation)
    }

    /// What the proposal service refused, as the problem it is.
    pub(crate) fn refusal(&self, error: ProposalError) -> Problem {
        match error {
            ProposalError::NotAllowed => Problem::new(Code::Forbidden),
            ProposalError::NotFound => Problem::new(Code::NotFound),
            ProposalError::NotInThisState { state, .. }
            | ProposalError::SupersederNotEligible { state } => {
                Problem::new(Code::NotInThisState).named("state", state.as_str())
            }
            ProposalError::Invalid(why) => self.unacceptable(why),
            ProposalError::Quota(q) => self.quota(&q),
            ProposalError::AlreadyDecided => Problem::new(Code::AlreadyDecided),
            ProposalError::StaleRevision { latest } => self.stale_at(latest),
            // Another act got there first: say what the proposal is now, so the caller can read and send again.
            ProposalError::Changed => match self.id.as_ref().map(|id| self.current_tag(id)) {
                Some(Ok(current)) => stale(&current),
                _ => Problem::new(Code::Failed),
            },
            ProposalError::NoSuchView(view) => Problem::new(Code::NoSuchView).named("view", view),
            ProposalError::BaseUnavailable(cause) => {
                self.log(&format!("a proposal's base could not be read now: {cause}"));
                Problem::new(Code::Unavailable)
            }
            ProposalError::Store(cause) => {
                self.log(&format!("the proposal records could not be used: {cause}"));
                Problem::new(Code::Unavailable)
            }
        }
    }

    /// `409 stale-base` for the proposal of this request, whose latest revision is `latest`.
    pub(crate) fn stale_at(&self, latest: u32) -> Problem {
        match &self.id {
            Some(id) => stale(&ProposalTag {
                id: id.clone(),
                revision: latest,
            }),
            None => Problem::new(Code::Failed),
        }
    }

    /// `429 refused-limit`: the limit by name, the numbers, and when it ends; `Retry-After` where waiting helps.
    pub(crate) fn quota(&self, q: &QuotaRefusal) -> Problem {
        quota_problem(q, (self.services.clock)())
    }

    /// What the acceptance refused, as the problem it is. Nothing a person did not ask for is told: the detail
    /// of a failure is the operator's, in the log.
    pub(crate) fn refused_acceptance(&self, failure: AcceptFailure) -> Problem {
        match failure {
            AcceptFailure::Proposal(error) => self.refusal(error),
            AcceptFailure::Refused(q) => self.quota(&q),
            other => acceptance_problem(other, &|line| self.log(line)),
        }
    }
}

/// `429 refused-limit` for a refusal at `now` (Unix seconds).
pub(crate) fn quota_problem(q: &QuotaRefusal, now: i64) -> Problem {
    let (word, at) = match q.reset {
        Reset::At(at) => ("at", Some(at)),
        Reset::WhenAProposalCloses => ("when-a-proposal-closes", None),
        Reset::WhenTheQueueMoves => ("when-the-queue-moves", None),
        Reset::Never => ("never", None),
    };
    let problem = Problem::new(Code::RefusedLimit)
        .named("limit", q.limit.name())
        .member("allowed", json!(q.allowed))
        .member("current", json!(q.current))
        .member("asked", json!(q.asked))
        .member("reset", json!(word))
        .member("reset_at", json!(at));
    match q.reset {
        Reset::At(at) => problem.retry_after(u64::try_from(at.saturating_sub(now)).unwrap_or(0)),
        Reset::WhenTheQueueMoves => problem.retry_after(QUEUE_RETRY),
        Reset::WhenAProposalCloses | Reset::Never => problem,
    }
}

/// The problem an acceptance that did not put its change in the history is. `Proposal` and `Refused` are the
/// proposal service's own refusals and quotas, told by [`Request::refusal`] and [`quota_problem`] (they are
/// failures of the service, here only because the acceptance wraps them); the cause of anything that is the
/// operator's business goes to `log`.
pub(crate) fn acceptance_problem(failure: AcceptFailure, log: &dyn Fn(&str)) -> Problem {
    match failure {
        AcceptFailure::NotAllowed => Problem::new(Code::Forbidden),
        AcceptFailure::NotFound => Problem::new(Code::NotFound),
        AcceptFailure::Busy(busy) => Problem::new(Code::Busy)
            .member("waiting", json!(busy.depth))
            .said("waiting", busy.depth)
            .retry_after(busy.estimate.as_secs().max(1)),
        AcceptFailure::NeedsRecovery(flag) => {
            log(&format!(
                "an acceptance was refused: the wiki needs recovery ({})",
                flag.reason.as_str()
            ));
            Problem::new(Code::NeedsRecovery).retry_after(RECOVERY_RETRY)
        }
        AcceptFailure::InProgress { .. } => Problem::new(Code::AcceptanceInProgress),
        AcceptFailure::Overlap { overlaps } => Problem::new(Code::Overlap)
            .member("overlaps", json!(overlaps))
            .said("overlaps", overlaps),
        AcceptFailure::NothingToChange => Problem::new(Code::NothingToChange),
        AcceptFailure::Failed { word, detail, .. } => {
            log(&format!("an acceptance failed ({word}): {detail}"));
            Problem::new(Code::AcceptanceFailed).named("word", word)
        }
        AcceptFailure::Incomplete {
            reason,
            flagged,
            detail,
            ..
        } => {
            log(&format!(
                "an acceptance is incomplete ({}; flagged: {flagged}): {detail}",
                reason.as_str()
            ));
            Problem::new(Code::NeedsRecovery).retry_after(RECOVERY_RETRY)
        }
        AcceptFailure::Unavailable(cause) => {
            log(&format!("an acceptance could not be made now: {cause}"));
            Problem::new(Code::Unavailable)
        }
        AcceptFailure::Store(cause) => {
            log(&format!(
                "the acceptance records could not be used: {cause}"
            ));
            Problem::new(Code::Unavailable)
        }
        // Told by the caller that holds the service's own words; here, the plain fact.
        AcceptFailure::Proposal(_) | AcceptFailure::Refused(_) => Problem::new(Code::Failed),
    }
}

/// How long a caller is told to wait for a wiki that needs recovery: recovery is the operator's, and minutes.
const RECOVERY_RETRY: u64 = 30;

#[cfg(test)]
mod tests;
