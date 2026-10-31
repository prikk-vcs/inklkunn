//! The write API's endpoints, as the document describes them (RFC 004 D-13; handoff 004 T7; external design
//! API-03 … API-05). Beside the read endpoints' `Doc`s and held to the route table the same way: a path in one
//! and not the other fails the build of the document.
//!
//! The bounds on a body are the form's, three ways over: the **decoded** texts are bounded by the proposal
//! service (a page 1 MiB, a proposal 2 MiB), and the **bytes of the body** are bounded here, before a byte is
//! parsed, at the most those texts can take when written as JSON (a control character is six bytes, `\u0001`).

use inklkunn_core::Action;
use inklkunn_core::proposal::{MAX_COMMENT_BYTES, MAX_PROPOSAL_BYTES, MAX_REASON_BYTES};

use super::{Answer, Base, Doc, Write};
use crate::api::problem::Code;

/// The most bytes of JSON for a text of `n` bytes: every byte of it escaped as `\u00XX`.
const fn json_of(n: usize) -> usize {
    6 * n
}

/// The most bytes of a proposal's body: its texts as JSON, and a little for the other members.
pub(crate) const PROPOSAL_BODY_LIMIT: usize = json_of(MAX_PROPOSAL_BYTES as usize) + 256 * 1024;

/// The most bytes of a comment's, a decision's or a reason's body.
pub(crate) const SMALL_BODY_LIMIT: usize = json_of(if MAX_COMMENT_BYTES > MAX_REASON_BYTES {
    MAX_COMMENT_BYTES
} else {
    MAX_REASON_BYTES
}) + 16 * 1024;

/// The most bytes of a body that carries no member (`{}`): a little room for white space.
pub(crate) const EMPTY_BODY_LIMIT: usize = 1024;

const PROPOSALS: &str = "/{slug}/-/api/v1/proposals";

pub(crate) const CREATE_PROPOSAL: Doc = Doc {
    path: PROPOSALS,
    operation_id: "createProposal",
    tag: "proposals",
    summary: "Open a proposal: a draft, or a proposal sent for review",
    description: "Opens a proposal of changes to whole pages (each file is a page's full new text, or `null` to remove it; a page is named by its repository `path`, as `storage_path` of the raw endpoint gives it, or a new page by its `title`). `submit: false` keeps it a draft that only its author sees; `submit: true` sends it for review at once. The write names the view's tip it was written against in `If-Match`: the `state_id` of the `tip` the read endpoints give (a stale one is `stale-base`). The answer is the proposal, with the `etag` every later write sends back. Scope `propose`; the quotas of open proposals and of proposals per hour refuse by name (`refused-limit`, with `Retry-After` where waiting helps).",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Propose,
        request: "NewProposal",
        base: Base::View,
        success: 201,
        limit: PROPOSAL_BODY_LIMIT,
    }),
    problems: &[
        Code::Unacceptable,
        Code::NoSuchView,
        Code::RefusedLimit,
        Code::NotInThisState,
    ],
};

pub(crate) const GET_PROPOSAL: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}",
    operation_id: "getProposal",
    tag: "proposals",
    summary: "Get a proposal's state and the base to write against",
    description: "The proposal's state, its summary and sources, its latest revision and the `etag` of it: what a later write names in `If-Match`. A draft is its author's alone (`not-found` for anyone else, as for a proposal that does not exist). The `ETag` header is the same value; it names the revision, not these bytes, so a conditional `GET` is always answered in full.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: None,
    problems: &[Code::InvalidParameter],
};

pub(crate) const SAVE_DRAFT: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/draft",
    operation_id: "saveDraft",
    tag: "proposals",
    summary: "Save a draft's working copy",
    description: "Replaces the draft's summary, sources, pages and renames with these (its base is kept). Only the author, and only while it is a draft. Scope `propose`. A draft is checked for what is wrong with it, not for what is missing: the summary and a change are required only to submit it.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Propose,
        request: "ProposalEdit",
        base: Base::Proposal,
        success: 200,
        limit: PROPOSAL_BODY_LIMIT,
    }),
    problems: &[Code::Unacceptable, Code::NotInThisState, Code::RefusedLimit],
};

pub(crate) const SUBMIT_DRAFT: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/submission",
    operation_id: "submitDraft",
    tag: "proposals",
    summary: "Send a draft for review",
    description: "The draft's working copy becomes revision 1 and the proposal is held for review. Only the author. The body is `{}`. Scope `propose`.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Propose,
        request: "Empty",
        base: Base::Proposal,
        success: 200,
        limit: EMPTY_BODY_LIMIT,
    }),
    problems: &[Code::Unacceptable, Code::NotInThisState],
};

pub(crate) const REVISE: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/revisions",
    operation_id: "reviseProposal",
    tag: "proposals",
    summary: "Add a revision to a proposal",
    description: "A new revision of a proposal that is held for review or needs one: the proposal is held for review again, and decisions on earlier revisions no longer count towards acceptance (they stay recorded). Only the proposer, within the quota of revisions. Pages are as for a new proposal, held to the base the proposal already has. Scope `propose`. The answer is the proposal, with the `etag` of the new revision.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Propose,
        request: "ProposalEdit",
        base: Base::Proposal,
        success: 201,
        limit: PROPOSAL_BODY_LIMIT,
    }),
    problems: &[Code::Unacceptable, Code::NotInThisState, Code::RefusedLimit],
};

pub(crate) const COMMENT: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/comments",
    operation_id: "commentOnProposal",
    tag: "review",
    summary: "Comment on a proposal",
    description: "A comment on the latest revision of a proposal that is held for review or needs a revision; a reply names its `parent`; a comment on a part of a page names it by `anchor` (the page's path, the part's position in the revision seen, and that revision), and the server builds the anchor from that revision's own text. Scope `propose`.",
    query: &[],
    answer: Answer::Schema("Comment"),
    write: Some(Write {
        action: Action::Comment,
        request: "NewComment",
        base: Base::Proposal,
        success: 201,
        limit: SMALL_BODY_LIMIT,
    }),
    problems: &[Code::Unacceptable, Code::NotInThisState],
};

pub(crate) const DECIDE: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/decisions",
    operation_id: "decideOnProposal",
    tag: "review",
    summary: "Approve a revision, or ask for changes",
    description: "A decision bound to the revision named by `If-Match` (the latest, or `stale-base`). One decision per reviewer per revision; a decision is not changed (`already-decided`). Scope `review`.",
    query: &[],
    answer: Answer::Schema("Decision"),
    write: Some(Write {
        action: Action::Review,
        request: "NewDecision",
        base: Base::Proposal,
        success: 201,
        limit: SMALL_BODY_LIMIT,
    }),
    problems: &[
        Code::Unacceptable,
        Code::NotInThisState,
        Code::AlreadyDecided,
    ],
};

pub(crate) const ACCEPT: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/acceptances",
    operation_id: "acceptProposal",
    tag: "review",
    summary: "Accept a revision into the history",
    description: "Takes the revision named by `If-Match` into the wiki's history, signed by the service in the acceptor's name, under the wiki's write lease. A retry of an acceptance that finished answers `already-signed` and writes nothing. When the view has moved on and nothing overlaps, the change is re-authored and accepted (`reauthored`); an overlap stops with the proposal needing a revision (`overlap`). The wiki being busy is `busy` with how many acts are ahead, and `Retry-After`. Scope `accept`, and the account must hold the role to accept in this wiki; a token that carries less is `forbidden` whatever the account is.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Accept,
        request: "Empty",
        base: Base::Proposal,
        success: 200,
        limit: EMPTY_BODY_LIMIT,
    }),
    problems: &[
        Code::NotInThisState,
        Code::RefusedLimit,
        Code::Busy,
        Code::NeedsRecovery,
        Code::Overlap,
        Code::NothingToChange,
        Code::AcceptanceInProgress,
        Code::AcceptanceFailed,
        Code::NotAvailable,
    ],
};

pub(crate) const DECLINE: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/declines",
    operation_id: "declineProposal",
    tag: "review",
    summary: "Decline a proposal, with a reason",
    description: "Ends a proposal that is held for review or needs a revision; the reason is required and the proposer is told. Scope `accept`.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Accept,
        request: "Reason",
        base: Base::Proposal,
        success: 200,
        limit: SMALL_BODY_LIMIT,
    }),
    problems: &[Code::Unacceptable, Code::NotInThisState],
};

pub(crate) const WITHDRAW: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/withdrawals",
    operation_id: "withdrawProposal",
    tag: "proposals",
    summary: "Take a proposal back, with a reason",
    description: "Ends the proposer's own proposal, a draft included; the reason is required. Only the proposer. Scope `propose`.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Propose,
        request: "Reason",
        base: Base::Proposal,
        success: 200,
        limit: SMALL_BODY_LIMIT,
    }),
    problems: &[Code::Unacceptable, Code::NotInThisState],
};
