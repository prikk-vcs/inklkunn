//! A claim's thread (RFC 005 part 2 D-11; requirement KN-7; handoff K-T10; store schema 18): the **epistemic
//! discussion** of a claim, with the records and the rules of a proposal's thread, and a label of its own.
//!
//! - **Service-authoritative and append-only** (CM-7): a comment is written once, with its author, its time and,
//!   when it answers another, its parent, which is a comment of the same claim's thread in the same wiki. Nothing is
//!   ever updated or removed (the SQLite store's triggers refuse an `UPDATE`, a `DELETE`, a replacing `INSERT` and a
//!   reply that crosses to another thread).
//! - **Keyed by the wiki and the claim's id**, not by a view: a claim is one claim in every view that reads it, and
//!   its discussion is about the claim, not about a file. The store checks the id's form and nothing else; whether
//!   the claim exists in a view is `inklkunn-core`'s question (the files are its).
//! - **A thread never changes a claim.** Nothing here reaches a repository, a claim file or a status: a decision does
//!   that, through a curation proposal (D-3). This store has no method that could.
//! - **Exported with the bundle's discussions** (XFER-10): [`ClaimThreadStore::claim_comments_in_wiki`] is every
//!   comment of every thread of a wiki, for that.

use crate::Result;
use crate::record::{AccountId, CommentId, WikiId};

/// The most comments one page of a thread holds, whatever is asked for.
pub const MAX_PAGE: usize = 200;

/// One comment in a claim's thread, written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimCommentRecord {
    /// The id.
    pub id: CommentId,
    /// The wiki.
    pub wiki: WikiId,
    /// The claim's id (a version-7 UUID in lower case with hyphens, the name of its file).
    pub claim: String,
    /// Who wrote it.
    pub author: AccountId,
    /// The comment it answers, in the same thread.
    pub parent: Option<CommentId>,
    /// The text.
    pub body: String,
    /// Unix seconds.
    pub created_at: i64,
}

/// A comment and where it stands in its thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimCommentRow {
    /// Its place in the thread, from 1: the order the comments were written in.
    pub position: u32,
    /// The place of the comment it answers, when it answers one.
    pub parent_position: Option<u32>,
    /// The comment.
    pub comment: ClaimCommentRecord,
}

/// One page of a thread, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimThreadPage {
    /// The comments of this page.
    pub rows: Vec<ClaimCommentRow>,
    /// How many comments the whole thread has.
    pub total: u64,
    /// The comment this page ends at, when more follow it: pass it as `after` for the next page.
    pub next: Option<CommentId>,
}

/// Claim threads, behind a trait (as the proposal records are): SQLite, and memory for tests.
pub trait ClaimThreadStore: Send + Sync {
    /// Add a comment to the thread of `comment.claim` in `comment.wiki`. Refused: a wiki or an account the store
    /// does not hold ([`crate::StoreError::NotFound`], [`crate::StoreError::AccountNotFound`]); a claim id that is
    /// not the spelling of one, or an empty text ([`crate::StoreError::Backend`]); a parent that is not a comment of
    /// this thread ([`crate::StoreError::NotFound`]); an id already used ([`crate::StoreError::AlreadyRecorded`]).
    fn add_claim_comment(&self, comment: &ClaimCommentRecord) -> Result<()>;

    /// At most `limit` comments (never more than [`MAX_PAGE`], and at least one) of the thread, oldest first, after
    /// the comment `after` when one is named ([`crate::StoreError::NotFound`] when that is not a comment of this
    /// thread), with how many the whole thread has. A thread nobody has written in is an empty page, never an error.
    fn claim_comments(
        &self,
        wiki: &WikiId,
        claim: &str,
        after: Option<&CommentId>,
        limit: usize,
    ) -> Result<ClaimThreadPage>;

    /// When each comment `author` made in the wiki's claim threads was made, at or after `since`, oldest first: part
    /// of the comment quota's current value, beside the proposal comments.
    fn claim_comments_since(
        &self,
        wiki: &WikiId,
        author: &AccountId,
        since: i64,
    ) -> Result<Vec<i64>>;

    /// Every comment of every claim thread of the wiki, by claim id and then in the order written: what the
    /// bundle's discussions are made from.
    fn claim_comments_in_wiki(&self, wiki: &WikiId) -> Result<Vec<ClaimCommentRow>>;
}

/// Is `claim` the spelling of a claim's id: 36 characters of lower-case hexadecimal digits and hyphens? (The same
/// check as [`crate::review::is_claim_id`]; the store does not read the claim files.)
pub(crate) fn well_formed(claim: &str) -> bool {
    crate::review::is_claim_id(claim)
}
