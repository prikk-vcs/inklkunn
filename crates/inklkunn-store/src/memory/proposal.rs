//! The in-memory proposal records: the reference the SQLite store must agree with.

use std::collections::HashMap;
use std::sync::{MutexGuard, PoisonError};

use crate::proposal::{
    CommentRecord, DecisionRecord, ProposalChange, ProposalFilter, ProposalOp, ProposalRecord,
    ProposalState, ProposalStore, ProposalTransition, RevisionContent, RevisionRecord,
};
use crate::record::{AccountId, ProposalId, WikiId};
use crate::{Result, StoreError};

use super::MemoryStore;

/// Everything proposal-shaped, behind one lock so that a check and the change that follows it are one step.
#[derive(Debug, Default)]
pub(super) struct Proposals {
    proposals: Vec<ProposalRecord>,
    drafts: HashMap<ProposalId, RevisionContent>,
    revisions: Vec<RevisionRecord>,
    transitions: Vec<ProposalTransition>,
    comments: Vec<CommentRecord>,
    decisions: Vec<DecisionRecord>,
}

impl Proposals {
    fn find(&self, id: &ProposalId) -> Option<&ProposalRecord> {
        self.proposals.iter().find(|p| p.id == *id)
    }

    fn latest_revision(&self, id: &ProposalId) -> u32 {
        self.revisions
            .iter()
            .filter(|r| r.proposal == *id)
            .map(|r| r.seq)
            .max()
            .unwrap_or(0)
    }

    /// The proposal, if `viewer` may see it.
    fn visible(&self, id: &ProposalId, viewer: Option<&AccountId>) -> Option<&ProposalRecord> {
        self.find(id).filter(|p| can_see(p, viewer))
    }
}

/// A never-submitted proposal is its author's alone.
fn can_see(p: &ProposalRecord, viewer: Option<&AccountId>) -> bool {
    p.submitted_at.is_some() || viewer == Some(&p.author)
}

impl MemoryStore {
    fn proposals_lock(&self) -> MutexGuard<'_, Proposals> {
        self.proposals
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn wiki_exists(&self, wiki: &WikiId) -> bool {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|r| r.id == *wiki)
    }
}

impl ProposalStore for MemoryStore {
    fn create_proposal(
        &self,
        proposal: &ProposalRecord,
        draft: &RevisionContent,
        created: &ProposalTransition,
    ) -> Result<()> {
        // The SQLite store's foreign keys, checked in the same order: the wiki, then the accounts.
        if !self.wiki_exists(&proposal.wiki) {
            return Err(StoreError::NotFound);
        }
        if !self.has_account(&proposal.author) || !self.has_account(&created.actor) {
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.proposals_lock();
        if d.find(&proposal.id).is_some() {
            return Err(StoreError::IdTaken(proposal.id.to_string()));
        }
        d.proposals.push(proposal.clone());
        d.drafts.insert(proposal.id.clone(), draft.clone());
        d.transitions.push(created.clone());
        Ok(())
    }

    fn proposal(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Option<ProposalRecord>> {
        Ok(self.proposals_lock().visible(id, viewer).cloned())
    }

    fn proposals(
        &self,
        wiki: &WikiId,
        filter: &ProposalFilter,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<ProposalRecord>> {
        let d = self.proposals_lock();
        Ok(d.proposals
            .iter()
            .filter(|p| p.wiki == *wiki && can_see(p, viewer))
            .filter(|p| filter.states.as_ref().is_none_or(|s| s.contains(&p.state)))
            .filter(|p| filter.author.as_ref().is_none_or(|a| *a == p.author))
            .cloned()
            .collect())
    }

    fn draft(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Option<RevisionContent>> {
        let d = self.proposals_lock();
        // The working copy is the author's alone, whatever became of the proposal.
        let Some(p) = d.find(id).filter(|p| viewer == Some(&p.author)) else {
            return Ok(None);
        };
        Ok(d.drafts.get(&p.id).cloned())
    }

    fn save_draft(
        &self,
        id: &ProposalId,
        summary: &str,
        sources: &[String],
        draft: &RevisionContent,
        at: i64,
    ) -> Result<()> {
        let mut d = self.proposals_lock();
        let Some(p) = d.proposals.iter_mut().find(|p| p.id == *id) else {
            return Err(StoreError::NotFound);
        };
        if p.state != ProposalState::Draft {
            return Err(StoreError::StateChanged);
        }
        p.summary = summary.to_owned();
        p.sources = sources.to_vec();
        p.updated_at = at;
        d.drafts.insert(id.clone(), draft.clone());
        Ok(())
    }

    fn change_proposal(&self, change: &ProposalChange) -> Result<ProposalRecord> {
        let t = &change.transition;
        // The proposal is looked for first, then the actor, as the SQLite store answers.
        if !self.proposal_exists(&t.proposal) {
            return Err(StoreError::NotFound);
        }
        if !self.has_account(&t.actor) {
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.proposals_lock();
        let Some(current) = d.find(&t.proposal) else {
            return Err(StoreError::NotFound);
        };
        if Some(current.state) != t.from {
            return Err(StoreError::StateChanged);
        }
        if let Some(r) = &change.revision
            && (r.proposal != t.proposal || r.seq != d.latest_revision(&t.proposal) + 1)
        {
            return Err(StoreError::StateChanged);
        }
        if let Some(by) = &change.superseded_by
            && d.find(by).is_none()
        {
            return Err(StoreError::NotFound);
        }
        if let Some(r) = &change.revision {
            d.revisions.push(r.clone());
        }
        if t.op == ProposalOp::Submit {
            d.drafts.remove(&t.proposal);
        }
        d.transitions.push(t.clone());
        let Some(p) = d.proposals.iter_mut().find(|p| p.id == t.proposal) else {
            return Err(StoreError::NotFound);
        };
        if let Some((summary, sources)) = &change.text {
            p.summary.clone_from(summary);
            p.sources.clone_from(sources);
        }
        p.state = t.to;
        p.updated_at = t.at;
        if t.op == ProposalOp::Submit {
            p.submitted_at.get_or_insert(t.at);
        }
        if !t.to.is_open() {
            p.closed_at = Some(t.at);
        }
        if change.superseded_by.is_some() {
            p.superseded_by.clone_from(&change.superseded_by);
        }
        Ok(p.clone())
    }

    fn revisions(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<RevisionRecord>> {
        let d = self.proposals_lock();
        if d.visible(id, viewer).is_none() {
            return Ok(Vec::new());
        }
        let mut out: Vec<RevisionRecord> = d
            .revisions
            .iter()
            .filter(|r| r.proposal == *id)
            .cloned()
            .collect();
        out.sort_by_key(|r| r.seq);
        Ok(out)
    }

    fn transitions(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<ProposalTransition>> {
        let d = self.proposals_lock();
        if d.visible(id, viewer).is_none() {
            return Ok(Vec::new());
        }
        Ok(d.transitions
            .iter()
            .filter(|t| t.proposal == *id)
            .cloned()
            .collect())
    }

    fn add_comment(&self, comment: &CommentRecord, states: &[ProposalState]) -> Result<()> {
        if !self.has_account(&comment.author) {
            // The proposal is checked first, as the SQLite store does.
            if self.proposals_lock().find(&comment.proposal).is_none() {
                return Err(StoreError::NotFound);
            }
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.proposals_lock();
        let Some(p) = d.find(&comment.proposal) else {
            return Err(StoreError::NotFound);
        };
        if !states.contains(&p.state) || comment.revision != d.latest_revision(&comment.proposal) {
            return Err(StoreError::StateChanged);
        }
        if let Some(parent) = &comment.parent
            && !d
                .comments
                .iter()
                .any(|c| c.id == *parent && c.proposal == comment.proposal)
        {
            return Err(StoreError::NotFound);
        }
        if d.comments.iter().any(|c| c.id == comment.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.comments.push(comment.clone());
        Ok(())
    }

    fn comments(&self, id: &ProposalId, viewer: Option<&AccountId>) -> Result<Vec<CommentRecord>> {
        let d = self.proposals_lock();
        if d.visible(id, viewer).is_none() {
            return Ok(Vec::new());
        }
        Ok(d.comments
            .iter()
            .filter(|c| c.proposal == *id)
            .cloned()
            .collect())
    }

    fn add_decision(&self, decision: &DecisionRecord) -> Result<()> {
        if !self.has_account(&decision.reviewer) {
            if self.proposals_lock().find(&decision.proposal).is_none() {
                return Err(StoreError::NotFound);
            }
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.proposals_lock();
        let Some(p) = d.find(&decision.proposal) else {
            return Err(StoreError::NotFound);
        };
        if p.state != ProposalState::HeldForReview
            || decision.revision != d.latest_revision(&decision.proposal)
        {
            return Err(StoreError::StateChanged);
        }
        if d.decisions.iter().any(|x| {
            x.id == decision.id
                || (x.proposal == decision.proposal
                    && x.revision == decision.revision
                    && x.reviewer == decision.reviewer)
        }) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.decisions.push(decision.clone());
        Ok(())
    }

    fn decisions(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<DecisionRecord>> {
        let d = self.proposals_lock();
        if d.visible(id, viewer).is_none() {
            return Ok(Vec::new());
        }
        Ok(d.decisions
            .iter()
            .filter(|x| x.proposal == *id)
            .cloned()
            .collect())
    }

    fn wiki_decisions(&self, wiki: &WikiId) -> Result<Vec<DecisionRecord>> {
        let d = self.proposals_lock();
        Ok(d.decisions
            .iter()
            .filter(|x| {
                d.find(&x.proposal)
                    .is_some_and(|p| p.wiki == *wiki && p.submitted_at.is_some())
            })
            .cloned()
            .collect())
    }

    fn open_count(&self, wiki: &WikiId, author: &AccountId) -> Result<u64> {
        let d = self.proposals_lock();
        let n = d
            .proposals
            .iter()
            .filter(|p| p.wiki == *wiki && p.author == *author && p.state.is_open())
            .count();
        Ok(u64::try_from(n).unwrap_or(u64::MAX))
    }

    fn opened_since(
        &self,
        wiki: &WikiId,
        author: Option<&AccountId>,
        since: i64,
    ) -> Result<Vec<i64>> {
        let d = self.proposals_lock();
        let mut times: Vec<i64> = d
            .proposals
            .iter()
            .filter(|p| p.wiki == *wiki && p.created_at >= since)
            .filter(|p| author.is_none_or(|a| p.author == *a))
            .map(|p| p.created_at)
            .collect();
        times.sort_unstable();
        Ok(times)
    }
}

impl MemoryStore {
    /// Is there a proposal with this id (whoever may see it)?
    pub(super) fn proposal_exists(&self, id: &ProposalId) -> bool {
        self.proposals_lock().find(id).is_some()
    }
}

impl MemoryStore {
    /// Is there a revision with this number in this proposal?
    pub(super) fn revision_exists(&self, id: &ProposalId, seq: u32) -> bool {
        self.proposals_lock()
            .revisions
            .iter()
            .any(|r| r.proposal == *id && r.seq == seq)
    }
}
