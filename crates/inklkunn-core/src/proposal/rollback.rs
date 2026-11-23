//! *Propose a rollback* (RFC 004 D-8; PR-8): undo is a proposal. The inverse of a change is computed at content
//! level by the read service ([`RollbackPlan`]: the pages as they were before the change); this module turns
//! it into a proposal that is **marked a rollback from its creation** and names the block it undoes, then goes
//! through review and acceptance like any other (D-2 … D-4). History is never rewritten and prikk's own
//! rollback vocabulary stays inside the driver.
//!
//! What an acceptance needs of a rollback is two facts: that the proposal *is* one (so the message says
//! `Kind: rollback`), and which block it undoes. Both are [`ProposalService::rollback_of`]; the block is also
//! the revision's base (see [`crate::read::RollbackPlan`]), which is the message's `Base:` trailer, so the history
//! names the undone change.

use inklkunn_store::{AccountId, Base, FileChange, ProposalId, WikiRecord};

use super::{
    Action, Principal, ProposalEdit, ProposalError, ProposalRecord, ProposalService, Resource,
    Result, viewer_of,
};
use crate::message::SUMMARY_MAX_CHARS;
use crate::read::{NoRollback, ReadError, ReadService, RollbackOutcome, RollbackPlan, Tip};

/// Where a rollback plan comes from. [`ReadService`] answers from the repository; a test answers from a
/// fixture.
pub trait RollbackSource: Send + Sync {
    /// The plan that undoes the change `block` of `view` (the wiki's default view for `None`).
    fn plan(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block: &str,
    ) -> std::result::Result<RollbackOutcome, ReadError>;
}

impl RollbackSource for ReadService {
    fn plan(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block: &str,
    ) -> std::result::Result<RollbackOutcome, ReadError> {
        self.rollback_plan(wiki, view, block)
    }
}

/// Why no rollback proposal was opened.
#[derive(Debug, Clone)]
pub enum RollbackError {
    /// The change cannot be turned into a rollback, and why.
    No(NoRollback),
    /// The repository could not be asked now (the text is for the operator).
    Unavailable(String),
    /// The proposal service refused.
    Proposal(ProposalError),
}

impl std::fmt::Display for RollbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RollbackError::No(why) => write!(f, "the change cannot be undone: {why:?}"),
            RollbackError::Unavailable(e) => write!(f, "the change could not be read: {e}"),
            RollbackError::Proposal(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RollbackError {}

impl From<ProposalError> for RollbackError {
    fn from(e: ProposalError) -> Self {
        RollbackError::Proposal(e)
    }
}

/// The edit a plan asks for: the plan's pages, written against the undone block, with the plan's summary.
/// Nothing is minor and nothing is machine-assisted: a person asked for it.
pub fn rollback_edit(plan: &RollbackPlan) -> ProposalEdit {
    ProposalEdit {
        summary: plan.summary.clone(),
        sources: Vec::new(),
        files: plan.files.clone(),
        renames: Vec::new(),
        minor: false,
        automated: false,
        base: Some(plan.base.clone()),
    }
}

/// The edit that restores a removed page when later changes block a rollback (RFC 006 D-10; handoff G-T6): the page written
/// again, at the path its title spells, with the text it had last (`text`, read at the removing change's parent). It is written
/// against the tip it was asked at (`tip`): a path the base does not have is a new page, so nothing of what later changes did
/// is touched. The summary is the one **given** (the caller's, in the person's language: the web's default is the catalog's
/// restoring sentence and the person may change it on the form), with line breaks and control characters made spaces and cut
/// to the summary's bound. Nothing is minor and nothing is machine-assisted: a person asked for it. `None` when the tip is not
/// a base a proposal can name.
pub fn restore_edit(summary: &str, path: &str, text: String, tip: &Tip) -> Option<ProposalEdit> {
    let mut summary: String = summary
        .chars()
        .map(|c| {
            if inklkunn_store::record::is_line_or_control(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    if summary.chars().count() > SUMMARY_MAX_CHARS {
        summary = summary.chars().take(SUMMARY_MAX_CHARS - 1).collect();
        summary.push('…');
    }
    Some(ProposalEdit {
        summary,
        sources: Vec::new(),
        files: vec![FileChange {
            path: path.to_owned(),
            text: Some(text),
        }],
        renames: Vec::new(),
        minor: false,
        automated: false,
        base: Some(Base::new(&tip.state_id, &tip.block_id)?),
    })
}

impl ProposalService {
    /// Open a **draft** rollback proposal from a plan: marked a rollback and naming the block it undoes,
    /// visible to its author only until it is submitted. Checks *propose* on every page the plan writes.
    pub fn create_rollback(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        view: Option<&str>,
        plan: &RollbackPlan,
    ) -> Result<ProposalRecord> {
        self.create_as(
            principal,
            wiki,
            view,
            rollback_edit(plan),
            super::Origin::Rollback(&plan.undoes),
        )
    }

    /// *Propose a rollback* of the change `block`: plan it from `source`, open the proposal and send it for
    /// review. The principal may propose in the wiki before anything is read from the repository.
    pub fn propose_rollback(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        view: Option<&str>,
        source: &dyn RollbackSource,
        block: &str,
    ) -> std::result::Result<ProposalRecord, RollbackError> {
        self.allow(
            principal,
            Action::Propose,
            &Resource::Wiki {
                wiki: wiki.id.clone(),
            },
        )?;
        let plan = match source.plan(wiki, view, block) {
            Ok(RollbackOutcome::Plan(plan)) => plan,
            Ok(RollbackOutcome::No(why)) => return Err(RollbackError::No(why)),
            Err(e) => return Err(RollbackError::Unavailable(e.to_string())),
        };
        let draft = self.create_rollback(principal, wiki, view, &plan)?;
        Ok(self.submit(principal, wiki, &draft.id)?)
    }

    /// The block a proposal undoes, when it is a rollback; `None` for an ordinary proposal. For whoever may
    /// read the proposal.
    pub fn rollback_of(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Option<String>> {
        let viewer: Option<&AccountId> = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        Ok(self.proposals.rollback_of(id, viewer_of(principal))?)
    }
}
