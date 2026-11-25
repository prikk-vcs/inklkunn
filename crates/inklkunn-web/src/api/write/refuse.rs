//! What the proposal service and the acceptance refuse, as problem details (RFC 004 D-6, D-11, D-12; API-04).
//!
//! Every refusal is one fixed code with the facts a program acts on as members (a named limit, its numbers and
//! when it ends; the state the proposal is in; the revision that is the latest). **A refusal that is the store's
//! or the repository's own failure is the operator's business**: its cause goes to the log and the caller gets
//! `unavailable` with `Retry-After`. `NotFound` is the same answer for a proposal that is absent and one the
//! caller may not see (a draft is its author's alone).

use inklkunn_core::RestrictionNotice;
use inklkunn_core::acceptance::AcceptFailure;
use inklkunn_core::governance::{Fault, ReadFault, Require, Target};
use inklkunn_core::proposal::{Invalid, ProposalError, QuotaRefusal, Reset};
use inklkunn_core::protection::{Lack, Notice};
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
        Invalid::PathOutsidePages => "path-outside-pages",
        Invalid::PathNotCanonical => "path-not-canonical",
        Invalid::PathCollides => "path-collides",
        Invalid::FrontUnreadable => "front-unreadable",
        Invalid::BadBase => "bad-base",
        Invalid::ReasonMissing => "reason-missing",
        Invalid::ReasonTooLong => "reason-too-long",
        Invalid::CommentEmpty => "comment-empty",
        Invalid::CommentTooLong => "comment-too-long",
        Invalid::CommentNul => "comment-nul",
        Invalid::BadAnchor => "bad-anchor",
        Invalid::SupersedeSelf => "supersede-self",
        Invalid::KnowledgeCurated => "knowledge-curated",
        Invalid::KnowledgeNotShown => "knowledge-not-shown",
        Invalid::KnowledgeUnjudged => "knowledge-unjudged",
        Invalid::KnowledgeStatusUnjudged => "knowledge-status-unjudged",
        Invalid::KnowledgeRetiredUnexplained => "knowledge-retired-unexplained",
        Invalid::KnowledgeCheckUnstated => "knowledge-check-unstated",
        Invalid::GovernancePath => "governance-path",
        Invalid::GovernanceMixed => "governance-mixed",
    }
}

impl Request {
    /// `422 unacceptable`: the stable word, and the sentence the pages use for it in the caller's language.
    pub(crate) fn unacceptable(&self, why: Invalid) -> Problem {
        Problem::new(Code::Unacceptable)
            .named("reason", invalid_word(why))
            .said("explanation", words::invalid_text(self.t, why))
    }

    /// [`Request::refusal`] for the refusal of a proposal whose files are `files`, written on `block` (K73): what only a
    /// curation proposal may do is told as the pages tell it (`curate::sentences`), so that a removal, or the replacement
    /// of a file that cannot be read, is told to be not offered in this version and never to be something only a curation
    /// proposal may do. Any other refusal is [`Request::refusal`]'s.
    pub(crate) fn refusal_of(
        &self,
        error: ProposalError,
        files: &[inklkunn_store::FileChange],
        block: &str,
    ) -> Problem {
        if !matches!(error, ProposalError::Invalid(Invalid::KnowledgeCurated)) {
            return self.refusal(error);
        }
        let given: Vec<(String, Option<String>)> = files
            .iter()
            .filter(|f| inklkunn_core::knowledge::family_of(&f.path).is_some())
            .map(|f| (f.path.clone(), f.text.clone()))
            .collect();
        // A base that cannot be read now is the service's to say; here it only chooses the words, so it is no file.
        let was = |path: &str| {
            self.services
                .read
                .source_at(&self.wiki, block, path)
                .ok()
                .flatten()
        };
        let refused = inklkunn_core::knowledge::check::curated_by(&given, &was);
        Problem::new(Code::Unacceptable)
            .named("reason", invalid_word(Invalid::KnowledgeCurated))
            .said(
                "explanation",
                crate::knowledge::curate::sentences(self.t, &refused).join(" "),
            )
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
            ProposalError::Restricted(notice) => restricted_problem(self.t, &notice),
            ProposalError::RulesUnreadable(fault) => rules_unreadable_problem(&fault).said(
                "explanation",
                crate::proposals::governance::read_fault_text(self.t, &fault),
            ),
            ProposalError::NotFound => Problem::new(Code::NotFound),
            ProposalError::NotInThisState { state, .. }
            | ProposalError::SupersederNotEligible { state } => {
                Problem::new(Code::NotInThisState).named("state", state.as_str())
            }
            ProposalError::Invalid(why) => self.unacceptable(why),
            // The acceptance's own problem for the same rule (`governance-invalid`, with the file and the rule), said in the
            // caller's language as the pages say it: each fault once, and not the acceptance's note about a held proposal.
            ProposalError::GovernanceInvalid(faults) => governance_problem(&faults).said(
                "explanation",
                crate::proposals::governance::fault_paragraphs(self.t, &faults).join(" "),
            ),
            ProposalError::OwnedKeys(keys) => {
                self.unacceptable_with("front-owned", words::owned_keys_text(self.t, &keys))
            }
            ProposalError::Quota(q) => self.quota(&q),
            ProposalError::AlreadyDecided => Problem::new(Code::AlreadyDecided),
            ProposalError::OwnProposal => Problem::new(Code::OwnProposal),
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
            AcceptFailure::Restricted(notice) => restricted_problem(self.t, &notice),
            AcceptFailure::Refused(q) => self.quota(&q),
            // The detail is the sentence of the page that tells the same refusal (each fault once, then that nothing was
            // changed), in the caller's language: `acceptance_problem` has no language, and the code's own detail is the
            // template `{explanation}`, which every `unacceptable` problem must fill.
            AcceptFailure::GovernanceInvalid(faults) => {
                let explanation =
                    crate::proposals::governance::invalid_paragraphs(self.t, &faults).join(" ");
                acceptance_problem(AcceptFailure::GovernanceInvalid(faults), &|line| {
                    self.log(line)
                })
                .said("explanation", explanation)
            }
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
        Reset::WhenAnAppealIsDecided => ("when-an-appeal-is-decided", None),
        Reset::WhenOneOfYoursIsDecided => ("when-one-of-your-appeals-is-decided", None),
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
        Reset::WhenAProposalCloses
        | Reset::WhenAnAppealIsDecided
        | Reset::WhenOneOfYoursIsDecided
        | Reset::Never => problem,
    }
}

/// The problem an acceptance that did not put its change in the history is. `Proposal` and `Refused` are the
/// proposal service's own refusals and quotas, told by [`Request::refusal`] and [`quota_problem`] (they are
/// failures of the service, here only because the acceptance wraps them); the cause of anything that is the
/// operator's business goes to `log`.
pub(crate) fn acceptance_problem(failure: AcceptFailure, log: &dyn Fn(&str)) -> Problem {
    match failure {
        AcceptFailure::NotAllowed => Problem::new(Code::Forbidden),
        // Told by the caller that holds the language (`Request::refused_acceptance`); here, the plain fact.
        AcceptFailure::Restricted(notice) => {
            restricted_members(Problem::new(Code::Forbidden), &notice)
        }
        // The proposal replaces a file the wiki cannot read and its author no longer holds the rung that may (G-T6 correction
        // S-2): the fact, and whether a restriction is the cause; the restriction's reason is the author's alone.
        AcceptFailure::AuthorNotAllowed { restricted } => Problem::new(Code::Forbidden)
            .named("reason", "author-not-allowed")
            .member("author_restricted", json!(restricted)),
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
        // Refused before the signer was asked (RFC 006 D-4): what the caller needs to correct the file is the file's fixed
        // name, the rule's stable name, the schema's field and the line, **never what the file said**.
        AcceptFailure::GovernanceInvalid(faults) => {
            log(&format!(
                "an acceptance was refused before signing: {} governance file(s): {}",
                faults.len(),
                faults
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            governance_problem(&faults)
        }
        AcceptFailure::GovernanceRung(rung) => Problem::new(Code::Forbidden)
            .named("reason", "governance-rung")
            .named("needs", rung.as_str()),
        AcceptFailure::ApprovalsMissing { needed, have } => Problem::new(Code::Forbidden)
            .named("reason", "approvals-missing")
            .member("needed", json!(needed))
            .member("have", json!(have)),
        // The words of the proposal's summary are hidden by moderation and would be written into the change: the proposer revises
        // it first (RFC 006 D-8; H-2).
        AcceptFailure::SummaryHidden => {
            Problem::new(Code::Forbidden).named("reason", "summary-hidden")
        }
        AcceptFailure::Protected { notices, lack } => protected_problem(&notices, lack),
        AcceptFailure::RulesUnreadable(fault) => {
            log(&format!(
                "an acceptance was refused, the rules of the wiki cannot be read: {fault}"
            ));
            rules_unreadable_problem(&fault)
        }
        // Refused before the signer was asked (RFC 005 part 2 D-5): the files and rules are the operator's, in the
        // log; the caller is told the word, as for any acceptance that did not put its change in the history.
        AcceptFailure::KnowledgeInvalid(refusals) => {
            log(&format!(
                "an acceptance was refused before signing: {} knowledge file(s): {}",
                refusals.len(),
                refusals
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            Problem::new(Code::AcceptanceFailed).named("word", "knowledge-invalid")
        }
        // The signer pauses every new seal while a key retirement is unfinished (W1): a problem of its own, because the
        // person who runs the wiki is the one who can end it, and the proposal is kept.
        AcceptFailure::Failed { word, detail, .. }
            if word == inklkunn_core::acceptance::words::RETIREMENT_UNFINISHED =>
        {
            log(&format!(
                "an acceptance was refused, a key retirement is unfinished: {detail}"
            ));
            Problem::new(Code::RetirementUnfinished)
        }
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

/// `422 unacceptable`, `governance-invalid`: each refusal as the file's fixed name, the rule's stable name, the schema's field
/// and the line (where it can be said), **never what the file said**.
pub(crate) fn governance_problem(faults: &[Fault]) -> Problem {
    let refusals: Vec<_> = faults
        .iter()
        .map(|f| {
            json!({
                "file": f.path(),
                "rule": f.name(),
                "field": f.field,
                "line": f.line,
            })
        })
        .collect();
    Problem::new(Code::Unacceptable)
        .named("reason", "governance-invalid")
        .member("refusals", json!(refusals))
}

/// `403 forbidden`, `protected` (RFC 006 D-7; threat model C-3g): what the acceptance lacked (`lacks`: `role`, and `needs` the role,
/// or `approval`) and **every protection that applies, once each**, as the pages show them: the kind, what it is on, what it asks, its
/// reason and, for a temporary one, its end. The reason is the words of whoever set it, as the pages show it.
fn protected_problem(notices: &[Notice], lack: Lack) -> Problem {
    let listed: Vec<_> = notices
        .iter()
        .map(|n| {
            let (on, title) = match n.target() {
                Target::Path(title) => ("page", title),
                Target::Namespace(title) => ("namespace", title),
            };
            let (requires, role) = match n.require() {
                Require::Review => ("review", None),
                Require::Rung(role) => ("rung", Some(role.as_str())),
            };
            json!({
                "kind": if n.until().is_some() { "temporary" } else { "standing" },
                "on": on,
                "title": title,
                "requires": requires,
                "role": role,
                "reason": n.reason(),
                "until": n.until().map(crate::time::rfc3339),
            })
        })
        .collect();
    let problem = Problem::new(Code::Forbidden).named("reason", "protected");
    let problem = match lack {
        Lack::Rung(role) => problem.named("lacks", "role").named("needs", role.as_str()),
        Lack::Review => problem.named("lacks", "approval"),
    };
    problem.member("protections", json!(listed))
}

/// `403 forbidden`, `restricted` (RFC 006 D-7; threat model C-3g): the caller's account is restricted in this wiki, and without
/// the restriction it could have done this. **The reason the person was given and the end are members**, so a program reads
/// them (`restriction_reason`, and `until` in Unix seconds) and a person reads the sentence (`explanation`, in the caller's
/// language). Said to the restricted account alone: it is told only of its own restriction.
pub(crate) fn restricted_problem(t: crate::catalog::T, notice: &RestrictionNotice) -> Problem {
    restricted_members(Problem::new(Code::Forbidden), notice).named(
        "explanation",
        crate::people::sentences::restriction_paragraphs(t, notice).join(" "),
    )
}

/// The members of `restricted_problem`, without the sentence.
fn restricted_members(problem: Problem, notice: &RestrictionNotice) -> Problem {
    problem
        .named("reason", "restricted")
        .named("restriction_reason", notice.reason.as_str())
        .member("until", json!(notice.until))
}

/// `403 forbidden`, `rules-unreadable`: every write of the wiki is refused until its governance file is repaired; the file,
/// the rule and the line say which.
///
/// Where the rule is `not-a-file`, **`found`** lists what stands at or below the file's place (the repository's paths, at most
/// 50) and **`found_more`** counts the rest: what the repair, a change by an admin that removes these and writes the file,
/// removes. Said only to whoever may read the wiki, as the fault is.
pub(crate) fn rules_unreadable_problem(fault: &ReadFault) -> Problem {
    let problem = Problem::new(Code::Forbidden)
        .named("reason", "rules-unreadable")
        .named("file", fault.file().path())
        .named("rule", fault.refusal().name())
        .member("line", json!(fault.line()));
    if fault.found().is_empty() {
        return problem;
    }
    let problem = problem.member("found", json!(fault.found()));
    if fault.more() > 0 {
        problem.member("found_more", json!(fault.more()))
    } else {
        problem
    }
}

/// How long a caller is told to wait for a wiki that needs recovery: recovery is the operator's, and minutes.
const RECOVERY_RETRY: u64 = 30;

#[cfg(test)]
mod tests;
