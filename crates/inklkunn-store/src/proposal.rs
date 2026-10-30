//! Proposals as records (RFC 004 D-1, D-3; internal design DM-3; external design CT-04): the proposal, its
//! revisions, the comments (with their anchors), the reviewers' decisions, the log of every transition, and
//! the draft that is a proposal's unsent working copy. The *rules* (who may do what, which transition is
//! legal, the quotas) are `inklkunn-core`'s; this module is what is kept and the trait behind which it is
//! kept, so that the in-memory and SQLite stores are held to one set of invariants.
//!
//! **What is append-only.** Revisions, comments, decisions and the transition log are written once: the
//! trait has no operation that changes or removes them, and the SQLite store's triggers refuse an `UPDATE`,
//! a `DELETE` and an `INSERT` over a key already taken even from a connection that bypasses this crate
//! (RFC 004 D-1: "decisions and acceptances are never updated"; the acceptance records are in
//! [`crate::acceptance`]). What changes is the proposal row (its state, its summary while it is still being
//! shaped, the time it closed) and the draft, and each change to the row happens in one transaction with the
//! transition log entry that explains it, so a state is never recorded without its reason.
//!
//! **Who may see what.** A proposal that was never submitted (a *draft*, whatever became of it since) is
//! visible to its author only, and to everyone else it is **absent** (RFC 004 D-1, PR-10): every read takes
//! the viewer and answers as if the record did not exist. The records of a visible proposal are visible to
//! whoever may read the proposal; that decision is the caller's (`authorize`).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::record::{AccountId, CommentId, DecisionId, ProposalId, WikiId};

/// Where a proposal is in its life (RFC 004 D-1). *Draft*, *held for review* and *needs a revision* are the
/// sub-states of **open**; the other four end the proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProposalState {
    /// Unsent: visible to its author only (PR-10).
    Draft,
    /// Submitted, waiting for review and a decision.
    HeldForReview,
    /// An overlap or a decline-with-changes awaits the proposer.
    NeedsRevision,
    /// An authorized person took it into the history.
    Accepted,
    /// An authorized person did not take it into the history.
    Declined,
    /// Its proposer took it back.
    Withdrawn,
    /// A later proposal names it.
    Superseded,
}

impl ProposalState {
    /// Every state.
    pub const ALL: [ProposalState; 7] = [
        ProposalState::Draft,
        ProposalState::HeldForReview,
        ProposalState::NeedsRevision,
        ProposalState::Accepted,
        ProposalState::Declined,
        ProposalState::Withdrawn,
        ProposalState::Superseded,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProposalState::Draft => "draft",
            ProposalState::HeldForReview => "held-for-review",
            ProposalState::NeedsRevision => "needs-revision",
            ProposalState::Accepted => "accepted",
            ProposalState::Declined => "declined",
            ProposalState::Withdrawn => "withdrawn",
            ProposalState::Superseded => "superseded",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        ProposalState::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// Is the proposal still alive (a draft, held for review, or needing a revision)?
    pub const fn is_open(self) -> bool {
        matches!(
            self,
            ProposalState::Draft | ProposalState::HeldForReview | ProposalState::NeedsRevision
        )
    }
}

impl fmt::Display for ProposalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The acts that change a proposal or are done to it; each is one named step of the state machine in
/// `inklkunn-core` and one `op` of the transition log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProposalOp {
    /// Open a proposal as a draft.
    Create,
    /// Save the draft's working copy.
    SaveDraft,
    /// Send the draft for review (its first revision).
    Submit,
    /// Add a revision.
    Revise,
    /// Add a comment.
    Comment,
    /// A reviewer approves or asks for changes.
    Decide,
    /// An acceptor sends it back to the proposer (a decline with changes, or an overlap).
    AskForRevision,
    /// The acceptance sealed.
    Accept,
    /// An acceptor declines.
    Decline,
    /// The proposer withdraws.
    Withdraw,
    /// A later proposal names this one.
    Supersede,
}

impl ProposalOp {
    /// Every operation.
    pub const ALL: [ProposalOp; 11] = [
        ProposalOp::Create,
        ProposalOp::SaveDraft,
        ProposalOp::Submit,
        ProposalOp::Revise,
        ProposalOp::Comment,
        ProposalOp::Decide,
        ProposalOp::AskForRevision,
        ProposalOp::Accept,
        ProposalOp::Decline,
        ProposalOp::Withdraw,
        ProposalOp::Supersede,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProposalOp::Create => "create",
            ProposalOp::SaveDraft => "save-draft",
            ProposalOp::Submit => "submit",
            ProposalOp::Revise => "revise",
            ProposalOp::Comment => "comment",
            ProposalOp::Decide => "decide",
            ProposalOp::AskForRevision => "ask-for-revision",
            ProposalOp::Accept => "accept",
            ProposalOp::Decline => "decline",
            ProposalOp::Withdraw => "withdraw",
            ProposalOp::Supersede => "supersede",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        ProposalOp::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

impl fmt::Display for ProposalOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The view's state when a revision was made: prikk's RefState id and the block that state belongs to (64
/// lower-case hex each), the pair a read of the tip is pinned to. The merge's *base* side is `cat` at
/// `block_id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Base {
    /// The view's RefState id.
    pub state_id: String,
    /// The block that state belongs to.
    pub block_id: String,
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Base {
    /// A base, or `None` unless both ids are 64 lower-case hex characters.
    pub fn new(state_id: &str, block_id: &str) -> Option<Self> {
        (is_hex64(state_id) && is_hex64(block_id)).then(|| Base {
            state_id: state_id.to_owned(),
            block_id: block_id.to_owned(),
        })
    }
}

/// One page of a revision: its path and its whole new text, or its removal. A revision carries **full page
/// text**, never a diff (RFC 004 D-1); the text is data and is never interpreted by the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// The repository path of the page.
    pub path: String,
    /// The page's new text; `None` removes the page.
    pub text: Option<String>,
}

/// A rename a revision declares (PR-15): the page at `from` becomes the page at `to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rename {
    /// The path the page has now.
    pub from: String,
    /// The path it will have.
    pub to: String,
}

/// What a revision (or the draft that becomes one) says: the base it was written against, the pages, the
/// declared renames, and the two flags that filter feeds (RFC 004 D-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionContent {
    /// The view's state when the text was written.
    pub base: Base,
    /// The pages, in the order the proposer gave them.
    pub files: Vec<FileChange>,
    /// The declared renames.
    pub renames: Vec<Rename>,
    /// A minor change.
    pub minor: bool,
    /// Machine-assisted content, attributed to the human who accepts it (C-13a).
    pub automated: bool,
}

impl RevisionContent {
    /// The bytes the content holds: every path and every text, as the quota and the bounds count them.
    pub fn bytes(&self) -> u64 {
        let files: usize = self
            .files
            .iter()
            .map(|f| f.path.len() + f.text.as_ref().map_or(0, String::len))
            .sum();
        let renames: usize = self.renames.iter().map(|r| r.from.len() + r.to.len()).sum();
        u64::try_from(files + renames).unwrap_or(u64::MAX)
    }
}

/// A revision as stored: written once, numbered from 1 within its proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRecord {
    /// The proposal.
    pub proposal: ProposalId,
    /// 1 for the first; each is the previous plus one.
    pub seq: u32,
    /// What it says.
    pub content: RevisionContent,
    /// Unix seconds.
    pub created_at: i64,
}

/// A proposal as stored (RFC 004 D-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalRecord {
    /// The id; the proposal's address is made from it.
    pub id: ProposalId,
    /// The wiki.
    pub wiki: WikiId,
    /// The view (branch) the proposal is addressed at, as prikk names it (`heads/main`).
    pub view: String,
    /// Where it is in its life.
    pub state: ProposalState,
    /// Who proposes.
    pub author: AccountId,
    /// "What and why" (the CT-03 first line): it may change while the proposal is a draft or is revised.
    pub summary: String,
    /// Where the change comes from (links, citations), as the proposer wrote them.
    pub sources: Vec<String>,
    /// The proposal that superseded this one, once one does.
    pub superseded_by: Option<ProposalId>,
    /// When it was opened (Unix seconds).
    pub created_at: i64,
    /// When it last changed.
    pub updated_at: i64,
    /// When it was first submitted; `None` for a proposal that never was, which only its author can see.
    pub submitted_at: Option<i64>,
    /// When it reached an end state (accepted, declined, withdrawn or superseded).
    pub closed_at: Option<i64>,
}

/// One step in a proposal's life, written once with the change of state it explains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalTransition {
    /// The proposal.
    pub proposal: ProposalId,
    /// The act.
    pub op: ProposalOp,
    /// The state before; `None` only for the creation.
    pub from: Option<ProposalState>,
    /// The state after.
    pub to: ProposalState,
    /// Who did it.
    pub actor: AccountId,
    /// The reason, for the acts that require one (decline, withdraw, ask for revision).
    pub reason: Option<String>,
    /// Unix seconds.
    pub at: i64,
}

/// A change of a proposal's state, with everything that must change with it, in one step. The proposal must
/// be in `transition.from`; otherwise nothing changes and the answer is [`crate::StoreError::StateChanged`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalChange {
    /// The step: the proposal, the act, the states, the actor, the reason.
    pub transition: ProposalTransition,
    /// A new summary and sources, when the act changes them (a revision).
    pub text: Option<(String, Vec<String>)>,
    /// A revision to append; its number must be the proposal's latest plus one.
    pub revision: Option<RevisionRecord>,
    /// The proposal that supersedes this one (the [`ProposalOp::Supersede`] step).
    pub superseded_by: Option<ProposalId>,
}

/// A quote selector (COLL-5): the exact text a comment is about, with the text around it, so that the anchor
/// can be found again in a later revision when the block has no `^id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    /// The exact quoted text.
    pub exact: String,
    /// The text just before it.
    pub prefix: String,
    /// The text just after it.
    pub suffix: String,
}

/// What a comment is anchored to (RFC 004 D-3): a block of a page, by the block's `^id` when it has one,
/// else by a quote selector and the block's ordinal. Re-anchoring on a later revision is derived at read
/// time; the anchor stored is the one the comment was made with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// The page's path.
    pub page: String,
    /// The block's `^id`, when it has one.
    pub block_id: Option<String>,
    /// The quote selector, when the block has no `^id` (or in addition to it).
    pub quote: Option<Quote>,
    /// The block's position among the page's top-level blocks (0 for the first).
    pub ordinal: u32,
}

/// A comment, written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentRecord {
    /// The id.
    pub id: CommentId,
    /// The proposal.
    pub proposal: ProposalId,
    /// The revision that was the latest when the comment was made.
    pub revision: u32,
    /// Who wrote it.
    pub author: AccountId,
    /// The comment it replies to, in the same proposal.
    pub parent: Option<CommentId>,
    /// The text.
    pub body: String,
    /// The block it is about, when it is about one.
    pub anchor: Option<Anchor>,
    /// Unix seconds.
    pub created_at: i64,
}

/// What a reviewer decided about one revision (RFC 004 D-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DecisionKind {
    /// The revision is good to take.
    Approve,
    /// The revision needs changes first.
    RequestChanges,
}

impl DecisionKind {
    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            DecisionKind::Approve => "approve",
            DecisionKind::RequestChanges => "request-changes",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        [DecisionKind::Approve, DecisionKind::RequestChanges]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

/// A reviewer's decision, written once, bound to the revision that was reviewed. There is at most one per
/// reviewer per revision; a later revision supersedes it for the purpose of acceptance and it stays recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRecord {
    /// The id.
    pub id: DecisionId,
    /// The proposal.
    pub proposal: ProposalId,
    /// The revision that was reviewed.
    pub revision: u32,
    /// Who decided.
    pub reviewer: AccountId,
    /// Which way.
    pub kind: DecisionKind,
    /// What the reviewer said with it (may be empty).
    pub body: String,
    /// Unix seconds.
    pub created_at: i64,
}

/// The flags of a proposal's latest revision (RFC 004 D-10): what a feed of accepted changes filters on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RevisionFlags {
    /// A minor change.
    pub minor: bool,
    /// Machine-assisted content.
    pub automated: bool,
}

/// Which proposals a listing wants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProposalFilter {
    /// Only these states; `None` for every state.
    pub states: Option<Vec<ProposalState>>,
    /// Only this author's proposals.
    pub author: Option<AccountId>,
}

/// The proposal records. Every implementation passes the same invariant tests (`tests/proposals.rs`).
///
/// Every operation that writes is **atomic**: it lands whole or changes nothing. The errors are the plain
/// facts of [`crate::StoreError`]: [`NotFound`](crate::StoreError::NotFound) (the wiki or the proposal is not
/// there), [`AccountNotFound`](crate::StoreError::AccountNotFound) (an actor is not an account),
/// [`IdTaken`](crate::StoreError::IdTaken) (a fresh id collided),
/// [`StateChanged`](crate::StoreError::StateChanged) (the record is no longer as the change assumed) and
/// [`AlreadyRecorded`](crate::StoreError::AlreadyRecorded) (a write-once record exists under that key).
pub trait ProposalStore: Send + Sync {
    /// Open a proposal as a draft: the record (in [`ProposalState::Draft`], not submitted), its working copy,
    /// and the creation step of its log, together.
    fn create_proposal(
        &self,
        proposal: &ProposalRecord,
        draft: &RevisionContent,
        created: &ProposalTransition,
    ) -> Result<()>;

    /// [`ProposalStore::create_proposal`] for a **rollback** (RFC 004 D-8): the same records, and with them,
    /// in the same step, the note that the proposal undoes the change `undoes` (a block id, 64 lower-case hex, which the
    /// caller has checked: the SQLite store refuses anything else as [`crate::StoreError::Backend`]). A proposal is a rollback from its creation or never.
    fn create_rollback_proposal(
        &self,
        proposal: &ProposalRecord,
        draft: &RevisionContent,
        created: &ProposalTransition,
        undoes: &str,
    ) -> Result<()>;

    /// The block a proposal undoes, when it is a rollback; `None` for every other proposal, for one that does
    /// not exist, and for a never-submitted one the viewer may not see.
    fn rollback_of(&self, id: &ProposalId, viewer: Option<&AccountId>) -> Result<Option<String>>;

    /// The flags of each given proposal's latest revision, for the **submitted** proposals among them (a
    /// draft has no revision, so it is absent from the answer). One step for a whole list, so that a feed of
    /// changes asks once.
    fn revision_flags(&self, ids: &[ProposalId]) -> Result<Vec<(ProposalId, RevisionFlags)>>;

    /// The proposal, as `viewer` may see it: a never-submitted proposal is `None` to anyone but its author.
    fn proposal(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Option<ProposalRecord>>;

    /// The wiki's proposals in the order they were opened, as `viewer` may see them: never-submitted ones
    /// only for their author.
    fn proposals(
        &self,
        wiki: &WikiId,
        filter: &ProposalFilter,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<ProposalRecord>>;

    /// The draft's working copy; `None` once it was submitted, or for anyone but the author.
    fn draft(&self, id: &ProposalId, viewer: Option<&AccountId>)
    -> Result<Option<RevisionContent>>;

    /// Replace the draft's working copy and the proposal's summary and sources, while the proposal is a
    /// draft ([`crate::StoreError::StateChanged`] otherwise).
    fn save_draft(
        &self,
        id: &ProposalId,
        summary: &str,
        sources: &[String],
        draft: &RevisionContent,
        at: i64,
    ) -> Result<()>;

    /// Change a proposal's state with its log entry (and, when the act has them, its new text, its new
    /// revision and its superseder), atomically. A submission sets the time of submission and removes the
    /// working copy; a step into an end state sets the time it closed. Returns the proposal as it now is.
    fn change_proposal(&self, change: &ProposalChange) -> Result<ProposalRecord>;

    /// The revisions, oldest first (empty for a proposal the viewer may not see).
    fn revisions(&self, id: &ProposalId, viewer: Option<&AccountId>)
    -> Result<Vec<RevisionRecord>>;

    /// The log of the proposal's steps, oldest first.
    fn transitions(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<ProposalTransition>>;

    /// Add a comment, if the proposal is in one of `states` and the comment's revision is its latest
    /// ([`crate::StoreError::StateChanged`] otherwise) and its parent, if any, is a comment of the same
    /// proposal ([`crate::StoreError::NotFound`] otherwise).
    fn add_comment(&self, comment: &CommentRecord, states: &[ProposalState]) -> Result<()>;

    /// The comments, oldest first.
    fn comments(&self, id: &ProposalId, viewer: Option<&AccountId>) -> Result<Vec<CommentRecord>>;

    /// Add a decision, if the proposal is held for review and the decision's revision is its latest
    /// ([`crate::StoreError::StateChanged`] otherwise). A second decision by the same reviewer on the same
    /// revision is [`crate::StoreError::AlreadyRecorded`] and changes nothing.
    fn add_decision(&self, decision: &DecisionRecord) -> Result<()>;

    /// The proposal's decisions, oldest first.
    fn decisions(&self, id: &ProposalId, viewer: Option<&AccountId>)
    -> Result<Vec<DecisionRecord>>;

    /// Every decision on the wiki's submitted proposals, oldest first: what the participation metrics read.
    fn wiki_decisions(&self, wiki: &WikiId) -> Result<Vec<DecisionRecord>>;

    /// How many open proposals (drafts included) `author` has in the wiki: a quota's current value.
    fn open_count(&self, wiki: &WikiId, author: &AccountId) -> Result<u64>;

    /// When each proposal opened (drafts included) at or after `since`, oldest first, for one author or, with
    /// `None`, for the whole wiki: a rate quota's current value and the time it will reset.
    fn opened_since(
        &self,
        wiki: &WikiId,
        author: Option<&AccountId>,
        since: i64,
    ) -> Result<Vec<i64>>;
}
