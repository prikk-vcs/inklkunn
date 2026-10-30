//! The proposal state machine (RFC 004 D-1, D-8): **every transition named, every other one refused.** A
//! pure table: no I/O, no clock. The service consults [`apply`] before it does anything, and the store records
//! the result with the step that explains it, so a state outside the RFC's seven can neither be reached nor
//! stored (the property tests walk every sequence of operations to a depth and hold the service to this
//! table).
//!
//! - *create* makes a **draft**; *save draft* keeps it a draft; *submit* makes it **held for review**.
//! - *revise* keeps it held, or brings a proposal that **needs a revision** back to held for review.
//! - *ask for revision* sends a held proposal back to its proposer (a decline with changes, or an overlap).
//! - *accept* ends a held proposal as **accepted**; *decline* ends a held one, or one that needs a revision, as
//!   **declined**; *withdraw* ends a draft, a held one or one that needs a revision as **withdrawn**;
//!   *supersede* ends a held one or one that needs a revision as **superseded**.
//!
//! *Comment* and *decide* are not changes of state, but they are only legal in some states, so they are rows
//! of the same table: a decision is possible while the proposal is held for review, a comment while it is held
//! or needs a revision, and neither once it has ended. The four end states have no way out.

use inklkunn_store::{ProposalOp, ProposalState};

use ProposalOp::{
    Accept, AskForRevision, Comment, Create, Decide, Decline, Revise, SaveDraft, Submit, Supersede,
    Withdraw,
};
use ProposalState::{
    Accepted, Declined, Draft, HeldForReview, NeedsRevision, Superseded, Withdrawn,
};

/// Every legal step: the act, the state it may be done in (`None` for the creation), and the state it leaves
/// the proposal in. **This is the whole machine.**
pub const TRANSITIONS: &[(ProposalOp, Option<ProposalState>, ProposalState)] = &[
    (Create, None, Draft),
    (SaveDraft, Some(Draft), Draft),
    (Submit, Some(Draft), HeldForReview),
    (Revise, Some(HeldForReview), HeldForReview),
    (Revise, Some(NeedsRevision), HeldForReview),
    (Comment, Some(HeldForReview), HeldForReview),
    (Comment, Some(NeedsRevision), NeedsRevision),
    (Decide, Some(HeldForReview), HeldForReview),
    (AskForRevision, Some(HeldForReview), NeedsRevision),
    (Accept, Some(HeldForReview), Accepted),
    (Decline, Some(HeldForReview), Declined),
    (Decline, Some(NeedsRevision), Declined),
    (Withdraw, Some(Draft), Withdrawn),
    (Withdraw, Some(HeldForReview), Withdrawn),
    (Withdraw, Some(NeedsRevision), Withdrawn),
    (Supersede, Some(HeldForReview), Superseded),
    (Supersede, Some(NeedsRevision), Superseded),
];

/// An act that the state it was asked in does not allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Illegal {
    /// The act.
    pub op: ProposalOp,
    /// The state it was asked in; `None` for a creation (which has no state to be asked in, so it is never
    /// illegal) and for the other acts on a proposal that does not exist (which are refused before this).
    pub state: Option<ProposalState>,
}

/// The state `op` leaves a proposal in when done in `state`, or why it may not be done.
pub fn apply(state: Option<ProposalState>, op: ProposalOp) -> Result<ProposalState, Illegal> {
    TRANSITIONS
        .iter()
        .find(|(o, from, _)| *o == op && *from == state)
        .map(|(_, _, to)| *to)
        .ok_or(Illegal { op, state })
}

/// The acts that may be done to a proposal in `state`, in the table's order.
pub fn legal_in(state: ProposalState) -> Vec<ProposalOp> {
    TRANSITIONS
        .iter()
        .filter(|(_, from, _)| *from == Some(state))
        .map(|(op, _, _)| *op)
        .collect()
}

/// Does this act change the proposal's state or its revisions, and so appear in the proposal's log? Saving a
/// draft, commenting and deciding have records of their own (the working copy, the comment, the decision).
pub const fn is_logged(op: ProposalOp) -> bool {
    !matches!(op, SaveDraft | Comment | Decide)
}
