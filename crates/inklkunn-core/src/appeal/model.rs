//! What a surface is given about appeals: the handle of a *contest this* link, a person's standing toward an action, the rows of
//! the pages, and the mark the moderation log shows. **An appeal's text and its appellant are in [`AppealRow`] only, which only
//! the appellant and the rung that decides ever read**; [`AppealMark`] is all the log gets.

use inklkunn_store::{
    AccountId, AppealId, AppealRecord, AppealState, AppealedAction, AppealedKind, ProposalId, Role,
};

/// What a *contest this* link names: the kind of action and its record's id, **as a surface was given them** (a decline's id is
/// its position in the proposal log, in decimal). The service decides whether the person asking may contest it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContestRef {
    /// What kind of action.
    pub kind: AppealedKind,
    /// The record's id.
    pub id: String,
}

impl ContestRef {
    /// The action as an appeal records it.
    pub fn action(&self) -> AppealedAction {
        AppealedAction {
            kind: self.kind,
            id: self.id.clone(),
        }
    }
}

/// Where a person stands toward an action: what a surface draws beside it. **Only the person the action affects is ever offered
/// anything**; everyone else, and every action that is not in force or not there, is [`Offer::Nothing`] (one answer, so that the
/// link says nothing of the records).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// No link: the action is not about this person, or it is not in force, or it is not there.
    Nothing,
    /// *Contest this*: the action affects this person and they have no appeal open against it.
    Contest,
    /// They have appealed it, and it waits for a decision: no second link, the one open appeal is named.
    Filed(AppealId),
}

/// How an appeal was decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// The action stands.
    Upheld,
    /// The action was reversed in the same step.
    Overturned,
}

/// What the moderation log shows of an appeal against a hide: **that it was appealed, and how it ended**. No text, no name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppealMark {
    /// Appealed, and waiting.
    Appealed,
    /// Appealed, and the hiding stands.
    Upheld,
    /// Appealed, and the hiding was reversed.
    Overturned,
}

impl AppealMark {
    /// What the log shows of the appeals made against one action: `None` when there are none. **An overturn wins over everything
    /// else (the action was reversed), an open appeal over an upheld one** (it is still being asked).
    pub fn of(appeals: &[AppealRecord]) -> Option<AppealMark> {
        appeals.iter().fold(None, |mark, appeal| {
            let this = match appeal.state {
                AppealState::Open => AppealMark::Appealed,
                AppealState::Upheld(_) => AppealMark::Upheld,
                AppealState::Overturned(..) => AppealMark::Overturned,
            };
            Some(match (mark, this) {
                (Some(AppealMark::Overturned), _) | (_, AppealMark::Overturned) => {
                    AppealMark::Overturned
                }
                (Some(AppealMark::Appealed), _) | (_, AppealMark::Appealed) => AppealMark::Appealed,
                _ => AppealMark::Upheld,
            })
        })
    }
}

/// A decision on an appeal, as the pages show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRow {
    /// Upheld or overturned.
    pub outcome: Outcome,
    /// When: Unix seconds.
    pub at: i64,
    /// Who decided, by name.
    pub by: String,
    /// Why.
    pub reason: String,
    /// The rung the decider held when they decided (criterion 12); `None` when the decision was recorded without it (*not recorded*).
    pub rung: Option<Role>,
    /// The block that set the rules the rung was held under; `None` is the default rules when `rung` is set.
    pub policy_block: Option<String>,
}

/// One appeal as the appellant and the rung that decides see it: **never anyone else**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealRow {
    /// The appeal.
    pub id: AppealId,
    /// When it was filed: Unix seconds.
    pub at: i64,
    /// What it is against.
    pub action: AppealedAction,
    /// For an appeal against a decline: the proposal, so that the page can link to it.
    pub proposal: Option<ProposalId>,
    /// Who appeals.
    pub appellant: AccountId,
    /// The appellant's name.
    pub appellant_name: String,
    /// Who took the action, by name, when an account did (the operator's grants have no one).
    pub action_by: Option<String>,
    /// What the appellant said.
    pub text: String,
    /// How it ended; `None` while it is open.
    pub decision: Option<DecisionRow>,
}

impl AppealRow {
    /// Is it waiting for a decision?
    pub fn is_open(&self) -> bool {
        self.decision.is_none()
    }
}

/// The state of an appeal record as the plain facts the pages and the log read.
pub(super) fn outcome_of(record: &AppealRecord) -> Option<Outcome> {
    match record.state {
        AppealState::Open => None,
        AppealState::Upheld(_) => Some(Outcome::Upheld),
        AppealState::Overturned(..) => Some(Outcome::Overturned),
    }
}
