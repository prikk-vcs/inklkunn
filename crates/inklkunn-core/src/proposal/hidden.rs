//! What the proposal service hides (RFC 006 D-8; INV-13): **every proposal, revision and comment a surface is given passes
//! here**. The summary of a proposal whose latest revision's summary is hidden, the pages of a revision whose content is
//! hidden, and the body of a comment that is hidden are not given; a surface asks which, and why, through the *marks* below,
//! and draws the tombstone. **A hidden author is not a field that can be blanked** (the author's identity is what the
//! service's own rules read: *whose proposal is this*), so a surface that names an author asks [`ProposalService::hidden_of`]
//! or [`ProposalService::comment_marks`] first, and the tests of every surface hold it to that.
//!
//! The raw reads ([`ProposalService::get_raw`], [`ProposalService::revisions_raw`]) are `pub(crate)`: the acceptance builds
//! the change's message and its files from them, and nothing outside this crate can.

use std::collections::HashMap;
use std::sync::Arc;

use inklkunn_store::{
    ClaimThreadPage, CommentId, CommentRecord, ProposalId, ProposalRecord, StoreError, WikiRecord,
};

use super::{ProposalError, ProposalService, Result};
use crate::authorize::Principal;
use crate::tombstone::{Hidden, ProposalHidden, Tombstones, Unreadable};

/// A window of a proposal's comments, as a surface may have them: a hidden comment's body is already gone, and `marks` says
/// which comments have something hidden and why ([`ProposalService::comment_marks`]'s answer, for these comments only).
#[derive(Debug, Clone)]
pub struct CommentsPage {
    /// The comments, oldest first.
    pub comments: Vec<CommentRecord>,
    /// What is hidden of each comment of the window that has something hidden.
    pub marks: HashMap<CommentId, Hidden>,
}

pub(super) fn unreadable(e: Unreadable) -> ProposalError {
    ProposalError::Store(StoreError::Backend(e.0))
}

impl ProposalService {
    /// The same, hiding what moderation hides (RFC 006 D-8): a hidden summary, the pages of a hidden revision, the body of a
    /// hidden comment or claim comment. **A server that serves wikis gives this**; without it nothing is hidden.
    #[must_use]
    pub fn with_tombstones(mut self, tombstones: Arc<Tombstones>) -> Self {
        self.tombstones = Some(tombstones);
        self
    }

    /// What is hidden of revision `seq` of a proposal: nothing when the service has no tombstones.
    pub(super) fn revision_hidden(
        &self,
        wiki: &WikiRecord,
        id: &ProposalId,
        seq: u32,
    ) -> Result<Hidden> {
        match &self.tombstones {
            None => Ok(Hidden::none()),
            Some(tombstones) => tombstones
                .proposal_revision(&wiki.id, id, seq)
                .map_err(unreadable),
        }
    }

    /// The record as a surface may have it: its summary and sources taken out when the latest revision's summary is hidden.
    pub(super) fn shown(
        &self,
        wiki: &WikiRecord,
        viewer: Option<&inklkunn_store::AccountId>,
        mut record: ProposalRecord,
    ) -> Result<ProposalRecord> {
        if self.tombstones.is_none() {
            return Ok(record);
        }
        let latest = self
            .proposals
            .revisions(&record.id, viewer)?
            .last()
            .map_or(0, |r| r.seq);
        if self.hidden_in(wiki, &record.id, latest)?.summary.is_some() {
            record.summary.clear();
            record.sources.clear();
        }
        Ok(record)
    }

    fn hidden_in(&self, wiki: &WikiRecord, id: &ProposalId, latest: u32) -> Result<ProposalHidden> {
        match &self.tombstones {
            None => Ok(ProposalHidden::default()),
            Some(tombstones) => tombstones
                .proposal(&wiki.id, id, latest)
                .map_err(unreadable),
        }
    }

    /// What is hidden of the proposal **wherever it is named**: its summary, and its author (whose name no surface may print
    /// when it is).
    pub fn hidden_of(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<ProposalHidden> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        let latest = self
            .proposals
            .revisions(id, viewer)?
            .last()
            .map_or(0, |r| r.seq);
        self.hidden_in(wiki, id, latest)
    }

    /// **Is the summary of the proposal's latest revision hidden?** What the acceptance asks before it builds the message of the
    /// change from the record: hidden words are never written into a new block (RFC 006 D-8, H-2). Raw, for the acceptance alone.
    pub(crate) fn latest_summary_hidden(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<bool> {
        let latest = self
            .revisions_raw(principal, wiki, id)?
            .last()
            .map_or(0, |r| r.seq);
        Ok(self.hidden_in(wiki, id, latest)?.summary.is_some())
    }

    /// What is hidden of each revision, oldest first, for the proposal's page to draw the tombstone where a revision is.
    pub fn revision_marks(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Vec<(u32, Hidden)>> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        let mut out = Vec::new();
        for revision in self.proposals.revisions(id, viewer)? {
            out.push((revision.seq, self.revision_hidden(wiki, id, revision.seq)?));
        }
        Ok(out)
    }

    /// What is hidden of each comment of the proposal that has something hidden: the body, or the author. A comment that is
    /// not in the map is not hidden in any part.
    pub fn comment_marks(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<HashMap<CommentId, Hidden>> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        let Some(tombstones) = &self.tombstones else {
            return Ok(HashMap::new());
        };
        let mut out = HashMap::new();
        for comment in self.proposals.comments(id, viewer)? {
            let hidden = tombstones
                .proposal_comment(&wiki.id, &comment.id)
                .map_err(unreadable)?;
            if hidden.is_hidden() {
                out.insert(comment.id, hidden);
            }
        }
        Ok(out)
    }

    /// A window of the proposal's comments, oldest first, **as [`ProposalService::comments`] gives them with the marks of
    /// [`ProposalService::comment_marks`]**, for the comments of the window alone: at most `limit` comments after the comment
    /// `after` (from the first, for `None`). `Ok(None)` when `after` is no comment of this proposal. The cost (the reads, and
    /// what moderation is asked) is the window's, never the thread's.
    pub fn comments_page(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        after: Option<&CommentId>,
        limit: usize,
    ) -> Result<Option<CommentsPage>> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        let Some(mut comments) = self.proposals.comments_after(id, viewer, after, limit)? else {
            return Ok(None);
        };
        let mut marks = HashMap::new();
        if let Some(tombstones) = &self.tombstones {
            for comment in &mut comments {
                let hidden = tombstones
                    .proposal_comment(&wiki.id, &comment.id)
                    .map_err(unreadable)?;
                if hidden.content().is_some() {
                    comment.body.clear();
                }
                if hidden.is_hidden() {
                    marks.insert(comment.id.clone(), hidden);
                }
            }
        }
        Ok(Some(CommentsPage { comments, marks }))
    }

    /// What is hidden of each comment of a page of a claim's thread (the body, or the author), for the comments that have
    /// something hidden.
    pub fn claim_comment_marks(
        &self,
        wiki: &WikiRecord,
        page: &ClaimThreadPage,
    ) -> Result<HashMap<CommentId, Hidden>> {
        let Some(tombstones) = &self.tombstones else {
            return Ok(HashMap::new());
        };
        let mut out = HashMap::new();
        for row in &page.rows {
            let hidden = tombstones
                .claim_comment(&wiki.id, &row.comment.id)
                .map_err(unreadable)?;
            if hidden.is_hidden() {
                out.insert(row.comment.id.clone(), hidden);
            }
        }
        Ok(out)
    }
}
