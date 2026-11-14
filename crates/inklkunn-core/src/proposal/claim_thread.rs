//! A claim's thread (RFC 005 part 2 D-11; requirement KN-7; handoff 005 K-T10): the **epistemic discussion** of a
//! claim, with the proposal thread's records and rules.
//!
//! - **Who may comment is who may comment on proposals**: a signed-in account (`Comment`, the same action, the same
//!   role and scope), at the same rate (the hourly limit counts a person's comments on proposals and in claim threads
//!   together, halved for a new account) and the same size. An anonymous visitor and the system principal may not.
//! - **Who may read is who may read the wiki** (`Read`), and a thread nobody has written in is an empty thread, so
//!   a claim that has no thread and a claim id that names nothing are told apart by no read here.
//! - **A thread never changes a claim.** This module writes one table of the service's store and reads the knowledge
//!   index to see that the claim exists; it holds no handle to a repository, a claim file or a proposal, so there is
//!   nothing in it that could change a status, a link or a source. A decision does that, through a curation proposal
//!   (D-3). A comment is the discussion; it is never a vote and counts for nothing.
//! - **Append-only**: a comment is written once ([`inklkunn_store::ClaimThreadStore`]); there is no edit and no removal
//!   here (moderation is RFC 006's).

use std::sync::{Arc, PoisonError};

use inklkunn_store::{
    ClaimCommentRecord, ClaimThreadPage, ClaimThreadStore, CommentId, StoreError, WikiRecord,
};

use super::{Invalid, MAX_COMMENT_BYTES, ProposalError, ProposalService, Result, account_of};
use crate::authorize::{Action, Principal, Resource};
use crate::knowledge::ClaimId;
use crate::read::{ReadError, ReadService};

/// A comment to add to a claim's thread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewClaimComment {
    /// The text.
    pub body: String,
    /// The comment it replies to, in the same thread.
    pub parent: Option<CommentId>,
}

/// The thread store and the read service that sees whether a claim exists, held by a service that keeps threads.
pub(super) struct Threads {
    pub(super) store: Arc<dyn ClaimThreadStore>,
    pub(super) read: Arc<ReadService>,
}

impl ProposalService {
    /// The same, keeping claim threads in `threads` and reading `read`'s knowledge index to see that the claim a
    /// comment is about is there (RFC 005 part 2 D-11). The comment rate counts the threads' comments beside the
    /// proposals'.
    #[must_use]
    pub fn with_claim_threads(
        mut self,
        threads: Arc<dyn ClaimThreadStore>,
        read: Arc<ReadService>,
    ) -> Self {
        self.quotas = self.quotas.with_claim_threads(threads.clone());
        self.claim_threads = Some(Threads {
            store: threads,
            read,
        });
        self
    }

    fn threads(&self) -> Result<&Threads> {
        self.claim_threads.as_ref().ok_or_else(|| {
            ProposalError::Store(StoreError::Backend(
                "this service keeps no claim threads".to_owned(),
            ))
        })
    }

    /// What a claim's discussion is, to the authorizer: a discussion of the wiki named by the claim. The claim's id
    /// is an identifier, not content, so it may stand in the audit trail.
    fn thread_resource(wiki: &WikiRecord, claim: &ClaimId) -> Resource {
        Resource::Discussion {
            wiki: wiki.id.clone(),
            id: format!("claim:{claim}"),
        }
    }

    /// May `principal` comment in the thread of `claim`? What a page asks to decide whether to offer the form; the
    /// service asks again when the comment is made.
    pub fn may_comment_on_claim(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        claim: &ClaimId,
    ) -> bool {
        matches!(principal, Principal::Account(_))
            && self.claim_threads.is_some()
            && self.authorizes(
                principal,
                Action::Comment,
                &Self::thread_resource(wiki, claim),
            )
    }

    /// Comment in the thread of `claim`, as `principal`, in `view` (the wiki's default view when `None`): the claim
    /// must be there in that view. Authorizes `Comment` before anything is read; the text is trimmed, may not be empty
    /// and is at most [`MAX_COMMENT_BYTES`]; the hourly comment limit counts this one. A reply names a comment of this
    /// thread. **Nothing but the thread changes.**
    pub fn comment_on_claim(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        view: Option<&str>,
        claim: &ClaimId,
        new: NewClaimComment,
    ) -> Result<ClaimCommentRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Comment,
            &Self::thread_resource(wiki, claim),
        )?;
        let threads = self.threads()?;
        let body = new.body.trim();
        if body.is_empty() {
            return Err(Invalid::CommentEmpty.into());
        }
        if body.len() > MAX_COMMENT_BYTES {
            return Err(Invalid::CommentTooLong.into());
        }
        let index = threads
            .read
            .knowledge_index(wiki, view)
            .map_err(|e| match e {
                ReadError::NoSuchView(v) => ProposalError::NoSuchView(v),
                other => ProposalError::BaseUnavailable(other.to_string()),
            })?;
        if index.claim(claim.as_str()).is_none() {
            return Err(ProposalError::NotFound);
        }
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.quotas.check_comment_rate(&wiki.id, who)?;
        let comment = ClaimCommentRecord {
            id: CommentId::new(),
            wiki: wiki.id.clone(),
            claim: claim.as_str().to_owned(),
            author: who.clone(),
            parent: new.parent,
            body: body.to_owned(),
            created_at: (self.clock)(),
        };
        // A reply to a comment that is not in this thread is `NotFound`, as a reply to a proposal's comment that is not
        // in the proposal is.
        threads.store.add_claim_comment(&comment)?;
        Ok(comment)
    }

    /// One page of `claim`'s thread, oldest first, after the comment `after` when one is named, at most `limit`
    /// comments. Authorizes `Read`; a claim nobody has discussed has an empty thread, whether or not the claim
    /// exists. A comment to continue from that is not in this thread is [`ProposalError::NotFound`].
    pub fn claim_thread(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        claim: &ClaimId,
        after: Option<&CommentId>,
        limit: usize,
    ) -> Result<ClaimThreadPage> {
        self.allow(principal, Action::Read, &Self::thread_resource(wiki, claim))?;
        let threads = self.threads()?;
        Ok(threads
            .store
            .claim_comments(&wiki.id, claim.as_str(), after, limit)?)
    }

    /// Every comment of every claim thread of the wiki, by claim and in the order written: what the bundle's
    /// discussions are made from. An export is an administrator's act and authorized as one by its caller; this reads
    /// the records and asks nobody.
    pub fn all_claim_comments(
        &self,
        wiki: &WikiRecord,
    ) -> Result<Vec<inklkunn_store::ClaimCommentRow>> {
        Ok(self.threads()?.store.claim_comments_in_wiki(&wiki.id)?)
    }
}
