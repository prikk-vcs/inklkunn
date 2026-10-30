//! The words a proposal is described in (RFC 004 D-12; RFC 002 D-6): the seven states, the outcomes of an
//! acceptance, the named limits of the quotas, and what each refusal of the proposal service says. Every
//! sentence is a catalog message; this file only chooses which, so that two pages never word the same state
//! two ways.

use axum::http::StatusCode;
use inklkunn_core::proposal::{
    Invalid, MAX_COMMENT_BYTES, MAX_PAGE_BYTES, MAX_PROPOSAL_BYTES, MAX_REASON_BYTES,
    MAX_SOURCE_BYTES, MAX_SOURCES, MAX_SUMMARY_CHARS, ProposalError, QuotaLimit, QuotaRefusal,
    Reset,
};
use inklkunn_store::ProposalState;

use crate::catalog::{Key, T};
use crate::time::utc_minutes;

/// The label of a state: the one word every page uses for it.
pub(crate) fn state_label(state: ProposalState) -> Key {
    match state {
        ProposalState::Draft => Key::StateDraft,
        ProposalState::HeldForReview => Key::StateHeld,
        ProposalState::NeedsRevision => Key::StateNeedsRevision,
        ProposalState::Accepted => Key::StateAccepted,
        ProposalState::Declined => Key::StateDeclined,
        ProposalState::Withdrawn => Key::StateWithdrawn,
        ProposalState::Superseded => Key::StateSuperseded,
    }
}

/// The sentence that says what a state means for the reader.
pub(crate) fn state_note(state: ProposalState) -> Key {
    match state {
        ProposalState::Draft => Key::NoteDraft,
        ProposalState::HeldForReview => Key::NoteHeld,
        ProposalState::NeedsRevision => Key::NoteNeedsRevision,
        ProposalState::Accepted => Key::NoteAccepted,
        ProposalState::Declined => Key::NoteDeclined,
        ProposalState::Withdrawn => Key::NoteWithdrawn,
        ProposalState::Superseded => Key::NoteSuperseded,
    }
}

/// The name of a limit, as a clause ("too many open proposals").
pub(crate) fn limit_name(limit: QuotaLimit) -> Key {
    match limit {
        QuotaLimit::OpenProposals => Key::QuotaOpenProposals,
        QuotaLimit::RevisionsPerProposal => Key::QuotaRevisions,
        QuotaLimit::AcceptedBytesPerDay => Key::QuotaAcceptedBytes,
        QuotaLimit::ProposalsPerHourPerPrincipal => Key::QuotaHourPerson,
        QuotaLimit::ProposalsPerHourPerWiki => Key::QuotaHourWiki,
        QuotaLimit::AcceptanceQueue => Key::QuotaQueue,
    }
}

/// A size as a person reads it: whole mebibytes or kibibytes.
pub(crate) fn size_label(t: T, bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB && bytes.is_multiple_of(MIB) {
        t.msg(Key::SizeMib).with("amount", bytes / MIB).text()
    } else if bytes >= 1024 {
        t.msg(Key::SizeKib).with("amount", bytes / 1024).text()
    } else {
        t.msg(Key::SizeBytes).with("amount", bytes).text()
    }
}

/// What a refused quota says, as paragraphs: the outcome and the named limit, the counts, and when it ends.
pub(crate) fn quota_paragraphs(t: T, q: &QuotaRefusal) -> Vec<String> {
    let limit = t.get(limit_name(q.limit));
    let (allowed, current, asked) = match q.limit {
        QuotaLimit::AcceptedBytesPerDay => (
            size_label(t, q.allowed),
            size_label(t, q.current),
            size_label(t, q.asked),
        ),
        _ => (
            t.lang().number(q.allowed),
            t.lang().number(q.current),
            t.lang().number(q.asked),
        ),
    };
    let reset = match q.reset {
        Reset::At(at) => t.msg(Key::ResetAt).with("when", utc_minutes(at)).text(),
        Reset::WhenAProposalCloses => t.get(Key::ResetProposalCloses),
        Reset::WhenTheQueueMoves => t.get(Key::ResetQueue),
        Reset::Never => t.get(match q.limit {
            QuotaLimit::RevisionsPerProposal => Key::ResetNeverRevisions,
            _ => Key::ResetNeverTooBig,
        }),
    };
    vec![
        t.msg(Key::OutcomeRefused).with("limit", limit).text(),
        t.msg(Key::QuotaCounts)
            .with("allowed", allowed)
            .with("current", current)
            .with("asked", asked)
            .text(),
        reset,
    ]
}

/// What went wrong with what a person sent, in one sentence.
pub(crate) fn invalid_text(t: T, why: Invalid) -> String {
    match why {
        Invalid::SummaryMissing => t.get(Key::InvalidSummaryMissing),
        Invalid::SummaryTooLong => t
            .msg(Key::InvalidSummaryTooLong)
            .with("limit", MAX_SUMMARY_CHARS)
            .text(),
        Invalid::SummaryNotOneLine => t.get(Key::InvalidSummaryNotOneLine),
        Invalid::TooManySources => t
            .msg(Key::InvalidTooManySources)
            .with("limit", MAX_SOURCES)
            .text(),
        Invalid::BadSource => t
            .msg(Key::InvalidBadSource)
            .with("limit", MAX_SOURCE_BYTES)
            .text(),
        Invalid::NothingProposed => t.get(Key::InvalidNothingProposed),
        Invalid::TooLarge => t
            .msg(Key::InvalidTooLarge)
            .with("limit", size_label(t, MAX_PROPOSAL_BYTES))
            .text(),
        Invalid::PageTooLarge => t
            .msg(Key::InvalidPageTooLarge)
            .with("limit", size_label(t, MAX_PAGE_BYTES as u64))
            .text(),
        Invalid::BadPath
        | Invalid::DuplicatePath
        | Invalid::RenameToSelf
        | Invalid::BadBase
        | Invalid::BadAnchor
        | Invalid::SupersedeSelf => t.get(Key::InvalidIncomplete),
        Invalid::ReasonMissing => t.get(Key::InvalidReasonMissing),
        Invalid::ReasonTooLong => t
            .msg(Key::InvalidReasonTooLong)
            .with("limit", size_label(t, MAX_REASON_BYTES as u64))
            .text(),
        Invalid::CommentEmpty => t.get(Key::InvalidCommentEmpty),
        Invalid::CommentTooLong => t
            .msg(Key::InvalidCommentTooLong)
            .with("limit", size_label(t, MAX_COMMENT_BYTES as u64))
            .text(),
    }
}

/// What a refusal of the service comes to as a page: its status, heading and paragraphs. `NotFound` is not
/// here: it is the generic not-found page, the same for a proposal that is absent and one that may not be seen.
pub(crate) struct Refusal {
    pub(crate) status: StatusCode,
    pub(crate) heading: Key,
    pub(crate) paragraphs: Vec<String>,
    /// Waiting helps (the store or the repository could not be asked now).
    pub(crate) retry: bool,
}

/// The page a refusal of the service is, or `None` for the refusals that are the generic not-found page.
pub(crate) fn refusal(t: T, error: &ProposalError) -> Option<Refusal> {
    let page = |status, heading, paragraphs| {
        Some(Refusal {
            status,
            heading,
            paragraphs,
            retry: false,
        })
    };
    match error {
        ProposalError::NotFound => None,
        ProposalError::NotAllowed => page(
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            vec![t.get(Key::RefusedNotAllowedBody)],
        ),
        ProposalError::NotInThisState { state, .. } => page(
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![
                t.msg(Key::RefusedNotInThisState)
                    .with("state", t.get(state_label(*state)))
                    .text(),
            ],
        ),
        ProposalError::Invalid(why) => page(
            StatusCode::UNPROCESSABLE_ENTITY,
            Key::RefusedInvalidHeading,
            vec![invalid_text(t, *why)],
        ),
        ProposalError::Quota(q) => page(
            StatusCode::TOO_MANY_REQUESTS,
            Key::OutcomeRefusedHeading,
            quota_paragraphs(t, q),
        ),
        ProposalError::AlreadyDecided => page(
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![t.get(Key::RefusedAlreadyDecided)],
        ),
        ProposalError::StaleRevision { latest } => page(
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![
                t.msg(Key::RefusedStaleRevision)
                    .with("latest", latest)
                    .text(),
            ],
        ),
        ProposalError::SupersederNotEligible { state } => page(
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![
                t.msg(Key::RefusedSuperseder)
                    .with("state", t.get(state_label(*state)))
                    .text(),
            ],
        ),
        ProposalError::Changed => page(
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![t.get(Key::RefusedChanged)],
        ),
        ProposalError::NoSuchView(view) => page(
            StatusCode::NOT_FOUND,
            Key::NoViewHeading,
            vec![t.msg(Key::NoViewBody).with("view", view).text()],
        ),
        ProposalError::BaseUnavailable(_) | ProposalError::Store(_) => Some(Refusal {
            status: StatusCode::SERVICE_UNAVAILABLE,
            heading: Key::UnavailableHeading,
            paragraphs: vec![t.get(Key::UnavailableBody)],
            retry: true,
        }),
    }
}
