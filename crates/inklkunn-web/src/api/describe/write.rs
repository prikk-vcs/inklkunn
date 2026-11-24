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

/// What a proposal that is refused as `unacceptable` says it was refused for: the `reason` words of the pages' own refusals
/// (`invalid_word`), listed in the description of each operation that can answer one (handoff G-T7: a test holds that every
/// word the server can say is described by the operations that can say it).
macro_rules! proposing_reasons {
    () => {
        " The `reason` of an `unacceptable` it answers is one of: `summary-missing`, `summary-too-long`, `summary-not-one-line`, `too-many-sources`, `bad-source`, `nothing-proposed`, `too-large`, `page-too-large`, `bad-path`, `duplicate-path`, `rename-to-self`, `path-outside-pages`, `path-not-canonical`, `path-collides`, `front-unreadable`, `bad-base`, `base-hidden`, `knowledge-curated`, `knowledge-not-shown`, `knowledge-unjudged`, `knowledge-status-unjudged`, `knowledge-retired-unexplained`, `knowledge-check-unstated`, `governance-path`, `governance-mixed`."
    };
}

/// The same for a comment, and for a decision's body, which are bounded as a comment is.
macro_rules! comment_reasons {
    () => {
        " The `reason` of an `unacceptable` it answers is one of: `comment-empty`, `comment-too-long`, `comment-nul` and `bad-anchor`."
    };
}

/// The same for a decision, whose body is a comment's and whose reason may be asked.
macro_rules! decision_reasons {
    () => {
        " The `reason` of an `unacceptable` it answers is `comment-too-long`."
    };
}

/// The same for the reason a decline or a withdrawal is given with.
macro_rules! reason_reasons {
    () => {
        " The `reason` of an `unacceptable` it answers is `reason-missing` or `reason-too-long`."
    };
}

const PROPOSALS: &str = "/{slug}/-/api/v1/proposals";

pub(crate) const CREATE_PROPOSAL: Doc = Doc {
    path: PROPOSALS,
    operation_id: "createProposal",
    tag: "proposals",
    summary: "Open a proposal: a draft, or a proposal sent for review",
    description: concat!(
        "Opens a proposal of changes to whole pages (each file is a page's full new text, or `null` to remove it; a page is named by its repository `path`, as `storage_path` of the raw endpoint gives it, or a new page by its `title`). `submit: false` keeps it a draft that only its author sees; `submit: true` sends it for review at once. The write names the view's tip it was written against in `If-Match`: the `state_id` of the `tip` the read endpoints give (a stale one is `stale-base`). The answer is the proposal, with the `etag` every later write sends back. Scope `propose`; the quotas of open proposals and of proposals per hour refuse by name (`refused-limit`, with `Retry-After` where waiting helps). A source file under `knowledge/` that this proposal moves to another `reference` and that still carries a `last_reviewed` day or an `availability` answer this proposal does not state is `unacceptable` (`knowledge-check-unstated`) when the draft is sent and at every revision: a check of the new place is stated by the proposal, never taken from the old one. A hand-written file states one with `last_reviewed.proposal` set to the proposal's own id (the `id` of the draft, which a `submit: false` call returns) beside the day it looked at the new place, which may not be earlier than the old place's day nor a day that has not come (checked when the proposal is accepted); a curation proposal states it with `checked`. **A change to the rules of the wiki is its own proposal:** under `governance/` a file may be written only at `governance/constitution.yaml` or `governance/policy.yaml` (a write of anything else there, and a rename of either file, is `unacceptable` with `reason` `governance-path`), but any path below `governance/` may be removed (`\"text\": null`: how something that stands where a governance file belongs is cleared, in the same proposal that writes the file), and a proposal that names a path under `governance/` names nothing else, no page and no rename (`unacceptable`, `governance-mixed`); the two files together are one change to the rules. The files' text is judged when the proposal is accepted.",
        proposing_reasons!()
    ),
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
    description: concat!(
        "Replaces the draft's summary, sources, pages and renames with these (its base is kept). Only the author, and only while it is a draft. Scope `propose`. A draft is checked for what is wrong with it, not for what is missing: the summary and a change are required only to submit it. The rules for `governance/` are `createProposal`'s: `governance-path`, and `governance-mixed` for a draft that names a governance file and anything else.",
        proposing_reasons!()
    ),
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
    description: concat!(
        "The draft's working copy becomes revision 1 and the proposal is held for review. Only the author. The body is `{}`. Scope `propose`. A draft that moves a source to another `reference` and leaves a `last_reviewed` day or an `availability` answer this proposal does not state is `unacceptable` (`knowledge-check-unstated`): a check of the new place is stated as `createProposal` says (`checked` in a curation; the proposal's own id in `last_reviewed.proposal` in a hand-written file). The rules for `governance/` are `createProposal`'s too (`governance-path`, `governance-mixed`).",
        proposing_reasons!()
    ),
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
    description: concat!(
        "A new revision of a proposal that is held for review or needs one: the proposal is held for review again, and decisions on earlier revisions no longer count towards acceptance (they stay recorded). Only the proposer, within the quota of revisions. Pages are as for a new proposal, held to the base the proposal already has, and a check of a source moved to another reference is stated as `createProposal` says (`knowledge-check-unstated`), and a change to the rules is its own proposal at every revision (`governance-path`, `governance-mixed`). Scope `propose`. The answer is the proposal, with the `etag` of the new revision.",
        proposing_reasons!()
    ),
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
    description: concat!(
        "A comment on the latest revision of a proposal that is held for review or needs a revision; a reply names its `parent`; a comment on a part of a page names it by `anchor` (the page's path, the part's position in the revision seen, and that revision), and the server builds the anchor from that revision's own text. Scope `propose`.",
        comment_reasons!()
    ),
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
    description: concat!(
        "A decision bound to the revision named by `If-Match` (the latest, or `stale-base`). One decision per reviewer per revision; a decision is not changed (`already-decided`). A proposer's own decision is refused (`own-proposal`); a comment on one's own proposal is not. Scope `review`.",
        decision_reasons!()
    ),
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
        Code::OwnProposal,
    ],
};

pub(crate) const ACCEPT: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/acceptances",
    operation_id: "acceptProposal",
    tag: "review",
    summary: "Accept a revision into the history",
    description: "Takes the revision named by `If-Match` into the wiki's history, signed by the service in the acceptor's name, under the wiki's write lease. A retry of an acceptance that finished answers `already-signed` and writes nothing. When the view has moved on and nothing overlaps, the change is re-authored and accepted (`reauthored`); an overlap stops with the proposal needing a revision (`overlap`). The wiki being busy is `busy` with how many acts are ahead, and `Retry-After`. Scope `accept`, and the account must hold the role to accept in this wiki; a token that carries less is `forbidden` whatever the account is. **The rules of the wiki** (its `governance/` files, read from the default view's tip) are applied before the signer is asked, and nothing is written when one refuses: a change that names a governance file and anything else is `unacceptable` with `reason` `governance-mixed`; a governance file that is not sound is `unacceptable` with `reason` `governance-invalid` and `refusals`, a list of `{file, rule, field, line}` (the file's fixed name, the rule's stable name, the schema's field, and the line or `null`; never what the file said); the constitution can ask a role above the acceptor's to change the rules, which is `forbidden` with `reason` `governance-rung` and `needs`, the role; the policy and the constitution can ask for approvals, which is `forbidden` with `reason` `approvals-missing`, `needed` the approvals they ask for and `have` how many people other than the proposer approved the latest revision, hold the review role now and have an account that is not disabled; a protection of a page the change touches (the policy's, or one the role that moderates put in place for a time; `protect` is web-only) asks what the acceptance lacks, which is `forbidden` with `reason` `protected`, `lacks` (`role`, with `needs` the role, or `approval`: one approval of the latest revision from someone other than the proposer, whatever the policy's approvals say) and `protections`, every protection that applies, each once, as a list of `{kind, on, title, requires, role, reason, until}` (the stricter decides; the reason is the words of whoever set it); the summary of the latest revision can be hidden by moderation, and it would be written into the change, so the acceptance is `forbidden` with `reason` `summary-hidden` until the proposer revises it (a hidden author does not refuse: the name is masked wherever the change is shown); a proposal that replaces or takes away a knowledge file the wiki cannot read (not UTF-8, over what a knowledge file may be, or not in the format) is the work of someone who may moderate, which is asked again of the proposal's author under the write lease, and is `forbidden` with `reason` `author-not-allowed` and `author_restricted` (true when a restriction holds the author below that role; its reason is theirs alone) when they no longer may, nothing being signed (when the acceptor is the author, the refusal is the acceptor's own: `forbidden`, or `restricted`); and a governance file that cannot be read makes the wiki refuse every write (`forbidden`, `reason` `rules-unreadable`, with `file`, `rule` and `line`, and, where the rule is `not-a-file`, `found`, the paths at or below the file's place, at most 50, and `found_more`, how many more) until an admin repairs it with a change that touches the two governance files and nothing else, which for `not-a-file` removes what `found` lists and writes the file.",

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
        Code::Unacceptable,
        Code::NotInThisState,
        Code::RefusedLimit,
        Code::Busy,
        Code::NeedsRecovery,
        Code::Overlap,
        Code::NothingToChange,
        Code::AcceptanceInProgress,
        Code::AcceptanceFailed,
        Code::RetirementUnfinished,
        Code::NotAvailable,
        Code::SigningNotConfigured,
    ],
};

pub(crate) const DECLINE: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/{id}/declines",
    operation_id: "declineProposal",
    tag: "review",
    summary: "Decline a proposal, with a reason",
    description: concat!(
        "Ends a proposal that is held for review or needs a revision; the reason is required and the proposer is told. Scope `accept`. While a governance file of the wiki cannot be read, only a proposal that changes the two governance files and nothing else can be declined; any other is `forbidden` with `reason` `rules-unreadable` (and `file`, `rule` and `line`, as `acceptProposal` says).",
        reason_reasons!()
    ),
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
    description: concat!(
        "Ends the proposer's own proposal, a draft included; the reason is required. Only the proposer. Scope `propose`.",
        reason_reasons!()
    ),
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
