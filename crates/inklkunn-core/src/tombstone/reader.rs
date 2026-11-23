//! Reading what is hidden (RFC 006 D-8; INV-13): **the one place a surface asks**. A read service, the proposal service and
//! the claim threads each hold a [`Tombstones`] and ask it about the items they are about to show; none of them keeps an
//! opinion of its own about what is hidden.
//!
//! **An accepted proposal's revision and the page's revision are one content.** The record of a hide lives on the page
//! revision (the block) once the proposal is sealed ([`Tombstones::item_of_revision`] says where), and a question about the
//! proposal's revision is answered with the block's marks too, so one act covers every place the content is shown. A hide
//! made before the acceptance, on a revision that was never the one accepted, stays with the proposal.
//!
//! **A store that cannot be read is an error, never "nothing hidden".** A surface that cannot tell what is hidden shows
//! nothing of the item.

use std::sync::Arc;

use inklkunn_store::{
    AcceptanceOutcome, AcceptanceStore, CommentId, GovernanceStore, HiddenItem, HiddenItemKind,
    ProposalId, StoreError, WikiId,
};

use super::model::{Hidden, ProposalHidden};

/// Why what is hidden could not be read now. The text is for the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable(pub String);

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "what is hidden could not be read: {}", self.0)
    }
}

impl std::error::Error for Unreadable {}

impl From<StoreError> for Unreadable {
    fn from(e: StoreError) -> Self {
        Unreadable(e.to_string())
    }
}

/// The item a page revision is: the change, by its block id.
pub fn page_item(block: &str) -> HiddenItem {
    HiddenItem {
        kind: HiddenItemKind::PageRevision,
        id: block.to_owned(),
        reference: None,
    }
}

/// The item a proposal's revision is: the proposal and the revision's number.
pub fn revision_item(proposal: &ProposalId, seq: u32) -> HiddenItem {
    HiddenItem {
        kind: HiddenItemKind::ProposalRevision,
        id: proposal.to_string(),
        reference: Some(seq.to_string()),
    }
}

/// The item a comment on a proposal is.
pub fn comment_item(comment: &CommentId) -> HiddenItem {
    HiddenItem {
        kind: HiddenItemKind::ProposalComment,
        id: comment.to_string(),
        reference: None,
    }
}

/// The item a comment in a claim's thread is.
pub fn claim_comment_item(comment: &CommentId) -> HiddenItem {
    HiddenItem {
        kind: HiddenItemKind::ClaimComment,
        id: comment.to_string(),
        reference: None,
    }
}

/// What is hidden, read from the moderation records.
#[derive(Clone)]
pub struct Tombstones {
    governance: Arc<dyn GovernanceStore>,
    acceptances: Arc<dyn AcceptanceStore>,
}

impl std::fmt::Debug for Tombstones {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tombstones")
    }
}

impl Tombstones {
    /// Over the moderation records, and the acceptance records that say which block a proposal's revision became.
    pub fn new(
        governance: Arc<dyn GovernanceStore>,
        acceptances: Arc<dyn AcceptanceStore>,
    ) -> Self {
        Tombstones {
            governance,
            acceptances,
        }
    }

    fn of_item(&self, wiki: &WikiId, item: &HiddenItem) -> Result<Hidden, Unreadable> {
        let events = self.governance.tombstone_events_of(wiki, item)?;
        Ok(Hidden::of_events(&events))
    }

    /// The block the proposal's revision `seq` was sealed as, when it was accepted.
    pub fn accepted_block(
        &self,
        proposal: &ProposalId,
        seq: u32,
    ) -> Result<Option<String>, Unreadable> {
        Ok(self
            .acceptances
            .acceptances_of(proposal, seq)?
            .into_iter()
            .find_map(|entry| match entry.result?.outcome {
                AcceptanceOutcome::Sealed { block_id, .. } => Some(block_id),
                AcceptanceOutcome::Failed { .. } => None,
            }))
    }

    /// **Where a hide of this proposal revision is recorded**: on the page revision when the revision was accepted (one
    /// content, one record), else on the proposal's revision.
    pub fn item_of_revision(
        &self,
        proposal: &ProposalId,
        seq: u32,
    ) -> Result<HiddenItem, Unreadable> {
        Ok(match self.accepted_block(proposal, seq)? {
            Some(block) => page_item(&block),
            None => revision_item(proposal, seq),
        })
    }

    /// What is hidden of the change `block` (a page revision): its content, its summary, its author.
    pub fn page_revision(&self, wiki: &WikiId, block: &str) -> Result<Hidden, Unreadable> {
        self.of_item(wiki, &page_item(block))
    }

    /// What is hidden of revision `seq` of the proposal: what is recorded on it, and, when it was accepted, what is recorded
    /// on the page revision it became.
    pub fn proposal_revision(
        &self,
        wiki: &WikiId,
        proposal: &ProposalId,
        seq: u32,
    ) -> Result<Hidden, Unreadable> {
        let own = self.of_item(wiki, &revision_item(proposal, seq))?;
        match self.accepted_block(proposal, seq)? {
            Some(block) => Ok(own.with(self.page_revision(wiki, &block)?)),
            None => Ok(own),
        }
    }

    /// What is hidden of a comment on a proposal.
    pub fn proposal_comment(&self, wiki: &WikiId, id: &CommentId) -> Result<Hidden, Unreadable> {
        self.of_item(wiki, &comment_item(id))
    }

    /// What is hidden of a comment in a claim's thread.
    pub fn claim_comment(&self, wiki: &WikiId, id: &CommentId) -> Result<Hidden, Unreadable> {
        self.of_item(wiki, &claim_comment_item(id))
    }

    /// What is hidden of the proposal **wherever it is named**: its author when the author of any of its `latest` revisions is
    /// hidden (a proposal has one author, so one hidden author hides the name everywhere), and its summary when the latest
    /// revision's summary is hidden (the store keeps the summary of the latest revision only).
    pub fn proposal(
        &self,
        wiki: &WikiId,
        proposal: &ProposalId,
        latest: u32,
    ) -> Result<ProposalHidden, Unreadable> {
        let mut out = ProposalHidden::default();
        for seq in 1..=latest {
            let hidden = self.proposal_revision(wiki, proposal, seq)?;
            if let Some(mark) = hidden.author() {
                out.author = Some(mark.clone());
            }
            if seq == latest
                && let Some(mark) = hidden.summary()
            {
                out.summary = Some(mark.clone());
            }
        }
        Ok(out)
    }
}
