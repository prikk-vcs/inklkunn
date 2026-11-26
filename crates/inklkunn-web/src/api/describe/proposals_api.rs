//! The reads of the proposals, as the document describes them (RFC 007 D-9; handoff 007 I-T5): a wiki's proposals, and a
//! proposal's comments. The write API's own `GET` of one proposal (`getProposal`) is beside the writes.
//!
//! **They show what the same person's pages show** (`/{slug}/-/proposals`, and the proposal's page): a wiki the caller may not read is
//! `not-found`, as the pages say it; **no draft is ever listed**, not even the caller's own, which the proposal's page and
//! `getProposal` keep for its author; and what moderation hides is told in its place, as `getProposal` tells it (RFC 006 D-8).
//! Each is on the same address as the write beside it (`createProposal`, `commentOnProposal`): the address names the collection,
//! the method says what is done to it.

use super::{Answer, CURSOR, Doc, Kind, LIMIT, Param};
use crate::api::problem::Code;

/// The states a list can be narrowed to: every state but a draft, which a list never holds.
pub(crate) const LISTED_STATES: &[&str] = &[
    "held-for-review",
    "needs-revision",
    "accepted",
    "declined",
    "withdrawn",
    "superseded",
];

const STATE: Param = Param {
    name: "state",
    kind: Kind::Choice(LISTED_STATES),
    required: false,
    missing: Code::InvalidParameter,
    description: "Only proposals in this state. Without it, every state a list holds: `held-for-review` and `needs-revision` are the open ones, the others are decided or taken back. A draft is no state of a list: it is never listed.",
};

const PROPOSALS_PARAMS: &[Param] = &[STATE, LIMIT, CURSOR];

const COMMENTS_PARAMS: &[Param] = &[LIMIT, CURSOR];

pub(crate) const LIST_PROPOSALS: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals",
    operation_id: "listProposals",
    tag: "proposals",
    summary: "List the wiki's proposals",
    description: "The proposals that were sent for review, **newest sent first**, one page of the list at a time (follow `next_cursor`, or the `Link` header's `rel=\"next\"`, until it is `null`), each as `getProposal` tells it: its state, summary and sources, its latest `revision`, its `etag`, and whether it is `yours`. **A draft is never listed**, not even the caller's own: a draft is its author's working copy, and `getProposal` and the proposal's page give it to its author alone, so this list holds the same proposals for everyone who may read the wiki. A proposal moves in the list by when it was first sent, never by what happens to it after, so a walk visits each proposal once whatever is decided meanwhile; a cursor names the place and not a proposal, so it stays good when a proposal changes state. **What moderation hides is hidden here as it is on the proposal's page:** a hidden summary is empty (and its sources with it) and the proposal carries `hidden`, `tombstone` and `tombstones` (which parts, why, by which role, when); a hidden author is not named anywhere (this list names no author). The same proposals are the feed `/{slug}/-/proposals.atom`, which says when each was opened and decided.",
    query: PROPOSALS_PARAMS,
    answer: Answer::Schema("ProposalList"),
    write: None,
    problems: &[Code::InvalidParameter, Code::InvalidCursor],
};

pub(crate) const LIST_PROPOSAL_COMMENTS: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/comments",
    operation_id: "listProposalComments",
    tag: "review",
    summary: "List a proposal's comments",
    description: "The comments on the proposal, **in the order they were made**, one page of the list at a time (follow `next_cursor`, or the `Link` header's `rel=\"next\"`, until it is `null`), each as `commentOnProposal` tells it. **A comment that moderation hides is its tombstone:** its `body` is empty and it carries `hidden` (which parts: `content`, `author`), `tombstone` and `tombstones` (why, by which role, when), as the proposal's page draws it; no comment's author is named in this list, hidden or not. The proposal is read as the proposal's page reads it: a proposal that does not exist, one in a wiki the caller may not read and a draft that is not the caller's own are one `not-found`, byte for byte. A cursor names the last comment of the page, and comments are never removed, so it stays good as comments are added.",
    query: COMMENTS_PARAMS,
    answer: Answer::Schema("ProposalComments"),
    write: None,
    problems: &[Code::InvalidParameter, Code::InvalidCursor],
};
