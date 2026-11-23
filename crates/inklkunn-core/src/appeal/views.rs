//! What the pages read of appeals: the *appeals* page of the rung that decides them, the appellant's own appeals and what they may
//! contest, the standing a *contest this* link is drawn from, and the mark the moderation log shows.
//!
//! **Who reads what.** An appeal's text and its appellant are read by the appellant and by the rung that decides, and by no one
//! else; to anyone else the *appeals* page is *not allowed* and the appellant's page shows only their own. A wiki the person may not
//! read answers as an absent one, as every page of the wiki does.

use inklkunn_store::{
    AccountId, AppealId, AppealRecord, AppealState, AppealedAction, AppealedKind, HiddenItem,
    HiddenParts, ProposalFilter, ProposalId, ProposalOp, ProposalState, ProtectionKind,
    ReasonCategory, Role, TombstoneAct, TombstoneEventId, WikiRecord,
};

use super::find::{Found, Who, position_of};
use super::model::{AppealMark, AppealRow, ContestRef, DecisionRow, Offer, Outcome, outcome_of};
use super::service::{AppealError, AppealService, DeciderRefusal, unavailable};
use crate::authorize::{Principal, RestrictionNotice};

/// How many appeals a page lists: the store's page.
const PAGE: usize = inklkunn_store::governance::MAX_PAGE;

/// How many of the wiki's latest moderation records are looked through for what a person may contest.
const LOG_SCAN: usize = inklkunn_store::governance::MAX_PAGE;

/// One open appeal in the queue of the rung that decides, with whether the person looking may decide it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queued {
    /// The appeal.
    pub row: AppealRow,
    /// Why the person looking may not decide it, when they may not (they are the appellant, or took the action).
    pub blocked: Option<DeciderRefusal>,
}

/// The *appeals* page of the rung that decides: **open** appeals oldest first, **decided** ones newest first, each with its text, its
/// appellant, the action and the decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealsView {
    /// Waiting for a decision, oldest first.
    pub open: Vec<Queued>,
    /// Decided, newest first.
    pub decided: Vec<AppealRow>,
}

/// What a person may contest, as the appellant's page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContestWhat {
    /// A decline of the person's proposal.
    Decline {
        /// The proposal.
        proposal: ProposalId,
    },
    /// A hiding of what the person wrote.
    Tombstone {
        /// What was hidden, by identifier.
        item: HiddenItem,
        /// Which parts.
        parts: HiddenParts,
        /// Why, in the closed set.
        category: Option<ReasonCategory>,
        /// When.
        at: i64,
        /// The rung that hid it.
        rung: Role,
    },
    /// A restriction in force on the person.
    Restriction {
        /// When it ends.
        until: i64,
        /// Why.
        reason: String,
    },
    /// A temporary protection that holds back a proposal of the person's.
    Protection {
        /// A page or a namespace.
        kind: ProtectionKind,
        /// By title.
        target: String,
        /// When it ends.
        until: i64,
        /// Why.
        reason: String,
        /// The person's open proposals it holds back.
        proposals: Vec<ProposalId>,
    },
    /// A decision that lowered the person's role.
    Grant {
        /// The role before, if any.
        before: Option<Role>,
        /// The role after, if any.
        after: Option<Role>,
        /// When.
        at: i64,
    },
}

/// One thing a person may contest: its handle, what it is, and the open appeal of theirs against it, if they have one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contestable {
    /// The handle a link names.
    pub contest: ContestRef,
    /// What it is.
    pub what: ContestWhat,
    /// Their open appeal against it.
    pub filed: Option<AppealId>,
}

/// A person's own appeals and what they may contest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MineView {
    /// Their appeals, newest first.
    pub appeals: Vec<AppealRow>,
    /// What they may contest now.
    pub contestable: Vec<Contestable>,
}

impl AppealService {
    /// May the person decide appeals here? **Nothing is recorded**: a page asks, the act is [`AppealService::decide`].
    pub fn may_decide(&self, principal: &Principal, wiki: &WikiRecord) -> bool {
        self.authorizer.may_decide_appeals(principal, &wiki.id)
    }

    /// The row of one appeal record, with names. `None` for an appeal whose records cannot be read now.
    fn row_of(&self, wiki: &WikiRecord, record: &AppealRecord) -> Result<AppealRow, AppealError> {
        let proposal = match record.action.kind {
            AppealedKind::Decline => position_of(&record.action.id)
                .map(|p| self.proposals.transition_at(&wiki.id, p))
                .transpose()
                .map_err(unavailable)?
                .flatten()
                .map(|(proposal, _)| proposal),
            _ => None,
        };
        let decision = match &record.state {
            AppealState::Open => None,
            AppealState::Upheld(d) | AppealState::Overturned(d, _) => Some(DecisionRow {
                outcome: outcome_of(record).unwrap_or(Outcome::Upheld),
                at: d.at,
                by: self.name_of(&d.by),
                reason: d.reason.clone(),
            }),
        };
        Ok(AppealRow {
            id: record.id.clone(),
            at: record.at,
            action: record.action.clone(),
            proposal,
            appellant: record.appellant.clone(),
            appellant_name: self.name_of(&record.appellant),
            action_by: record.action_actor.as_ref().map(|a| self.name_of(a)),
            text: record.text.clone(),
            decision,
        })
    }

    /// **The *appeals* page**: for the rung that decides, and for no one else. Each open appeal says whether the person looking
    /// may decide it.
    pub fn appeals(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
    ) -> Result<AppealsView, AppealError> {
        let acting = self.begin(principal, wiki)?;
        if !self.may_decide(principal, wiki) {
            return Err(AppealError::NotAllowed);
        }
        let records = self
            .governance
            .recent_appeals(&wiki.id, PAGE)
            .map_err(unavailable)?;
        let mut open = Vec::new();
        let mut decided = Vec::new();
        for record in &records {
            let row = self.row_of(wiki, record)?;
            if record.state == AppealState::Open {
                let blocked = if record.appellant == acting.account {
                    Some(DeciderRefusal::TheAppellant)
                } else if record.action_actor.as_ref() == Some(&acting.account)
                    || self
                        .actors_of(wiki, &record.action)?
                        .contains(&acting.account)
                {
                    Some(DeciderRefusal::TheActor)
                } else {
                    None
                };
                open.push(Queued { row, blocked });
            } else {
                decided.push(row);
            }
        }
        // The store gives the newest first; the queue is read oldest first.
        open.reverse();
        Ok(AppealsView { open, decided })
    }

    /// The person's own appeals, newest first, **and nobody else's**, and what they may contest now.
    pub fn mine(&self, principal: &Principal, wiki: &WikiRecord) -> Result<MineView, AppealError> {
        let acting = self.begin(principal, wiki)?;
        let records = self
            .governance
            .appeals_by(&wiki.id, &acting.account)
            .map_err(unavailable)?;
        let appeals = records
            .iter()
            .map(|r| self.row_of(wiki, r))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(MineView {
            appeals,
            contestable: self.contestable(
                wiki,
                &acting.account,
                acting.holding.restriction.as_ref(),
            )?,
        })
    }

    /// The open appeal of `person` against the action, if they have one.
    fn open_of(
        &self,
        wiki: &WikiRecord,
        action: &AppealedAction,
        person: &AccountId,
    ) -> Result<Option<AppealId>, AppealError> {
        Ok(self
            .governance
            .appeals_about(&wiki.id, action)
            .map_err(unavailable)?
            .into_iter()
            .find(|a| a.appellant == *person && a.state == AppealState::Open)
            .map(|a| a.id))
    }

    /// Where the person stands toward an action: what a *contest this* link is drawn from. **Nothing for anyone the action does not
    /// affect**, for an action that is not in force, and for a wiki the person may not read: one answer.
    pub fn offer(&self, principal: &Principal, wiki: &WikiRecord, what: &ContestRef) -> Offer {
        self.offer_checked(principal, wiki, what)
            .unwrap_or(Offer::Nothing)
    }

    fn offer_checked(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        what: &ContestRef,
    ) -> Result<Offer, AppealError> {
        let acting = self.begin(principal, wiki)?;
        let action = what.action();
        let Some(found) = self.find(wiki, &action)? else {
            return Ok(Offer::Nothing);
        };
        if !self.affects(wiki, &found, &acting.account)? {
            return Ok(Offer::Nothing);
        }
        Ok(match self.open_of(wiki, &action, &acting.account)? {
            Some(appeal) => Offer::Filed(appeal),
            None => Offer::Contest,
        })
    }

    /// What `what` is, **for the person it affects** (the *contest this* form says what is contested); `NotAffected` for anyone
    /// else and for an action that is not in force.
    pub fn contesting(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        what: &ContestRef,
    ) -> Result<Contestable, AppealError> {
        let acting = self.begin(principal, wiki)?;
        let action = what.action();
        let found = self.find(wiki, &action)?.ok_or(AppealError::NotAffected)?;
        if !self.affects(wiki, &found, &acting.account)? {
            return Err(AppealError::NotAffected);
        }
        let what_is = self.what_is(wiki, &action, &found, &acting.account)?;
        Ok(Contestable {
            contest: what.clone(),
            what: what_is,
            filed: self.open_of(wiki, &action, &acting.account)?,
        })
    }

    /// The description of an action in force.
    fn what_is(
        &self,
        wiki: &WikiRecord,
        action: &AppealedAction,
        found: &Found,
        person: &AccountId,
    ) -> Result<ContestWhat, AppealError> {
        use inklkunn_store::{GrantDecisionId, ProtectionEventId, RestrictionEventId};
        Ok(match action.kind {
            AppealedKind::Decline => ContestWhat::Decline {
                proposal: found.proposal.clone().ok_or(AppealError::NotAffected)?,
            },
            AppealedKind::Tombstone => {
                let event = TombstoneEventId::parse(&action.id)
                    .and_then(|id| self.governance.tombstone_event(&id).transpose())
                    .transpose()
                    .map_err(unavailable)?
                    .ok_or(AppealError::NotAffected)?;
                ContestWhat::Tombstone {
                    item: event.item,
                    parts: event.parts,
                    category: event.category,
                    at: event.at,
                    rung: event.by.rung,
                }
            }
            AppealedKind::Restriction => {
                let event = RestrictionEventId::parse(&action.id)
                    .and_then(|id| self.governance.restriction_event(&id).transpose())
                    .transpose()
                    .map_err(unavailable)?
                    .ok_or(AppealError::NotAffected)?;
                ContestWhat::Restriction {
                    until: event.expires_at.unwrap_or(i64::MAX),
                    reason: event.reason.unwrap_or_default(),
                }
            }
            AppealedKind::Protection => {
                let Who::HeldBack(event) = &found.who else {
                    return Err(AppealError::NotAffected);
                };
                debug_assert!(ProtectionEventId::parse(&action.id).is_some());
                ContestWhat::Protection {
                    kind: event.kind,
                    target: event.target.clone(),
                    until: event.expires_at.unwrap_or(i64::MAX),
                    reason: event.reason.clone().unwrap_or_default(),
                    proposals: self.held_by(wiki, event, person)?,
                }
            }
            AppealedKind::Grant => {
                let decision = GrantDecisionId::parse(&action.id)
                    .and_then(|id| self.governance.grant_decision(&id).transpose())
                    .transpose()
                    .map_err(unavailable)?
                    .ok_or(AppealError::NotAffected)?;
                ContestWhat::Grant {
                    before: decision.before,
                    after: decision.after,
                    at: decision.at,
                }
            }
        })
    }

    /// Everything the person may contest in the wiki now: the restriction in force, the decision that lowered their role, the
    /// proposals of theirs that were declined, the hidings of what they wrote, and the protections that hold back a proposal of theirs.
    fn contestable(
        &self,
        wiki: &WikiRecord,
        person: &AccountId,
        restriction: Option<&RestrictionNotice>,
    ) -> Result<Vec<Contestable>, AppealError> {
        let mut out = Vec::new();
        let mut add =
            |me: &Self, contest: ContestRef, found: Option<Found>| -> Result<(), AppealError> {
                let Some(found) = found else { return Ok(()) };
                if !me.affects(wiki, &found, person)? {
                    return Ok(());
                }
                let what = me.what_is(wiki, &contest.action(), &found, person)?;
                let filed = me.open_of(wiki, &contest.action(), person)?;
                out.push(Contestable {
                    contest,
                    what,
                    filed,
                });
                Ok(())
            };
        if let Some(notice) = restriction {
            let contest = ContestRef {
                kind: AppealedKind::Restriction,
                id: notice.id.to_string(),
            };
            let found = self.find(wiki, &contest.action())?;
            add(self, contest, found)?;
        }
        // The latest decision that took a role from them or lowered it, if it is still what they hold.
        for decision in self
            .governance
            .grant_decisions(&wiki.id, Some(person), 10)
            .map_err(unavailable)?
        {
            let contest = ContestRef {
                kind: AppealedKind::Grant,
                id: decision.id.to_string(),
            };
            let found = self.find(wiki, &contest.action())?;
            if found.is_some() {
                add(self, contest, found)?;
                break;
            }
        }
        // Declined proposals of theirs: the decline is contestable while it is the proposal's last step.
        let declined = self
            .proposals
            .proposals(
                &wiki.id,
                &ProposalFilter {
                    states: Some(vec![ProposalState::Declined]),
                    author: Some(person.clone()),
                },
                Some(person),
            )
            .map_err(unavailable)?;
        for proposal in declined {
            if let Some(contest) = self.decline_of(wiki, &proposal.id)? {
                let found = self.find(wiki, &contest.action())?;
                add(self, contest, found)?;
            }
        }
        // Hidings of what they wrote, among the wiki's latest moderation records.
        for event in self
            .governance
            .tombstone_log(&wiki.id, None, LOG_SCAN)
            .map_err(unavailable)?
        {
            if event.act == TombstoneAct::Hide && event.affected.as_ref() == Some(person) {
                let contest = ContestRef {
                    kind: AppealedKind::Tombstone,
                    id: event.id.to_string(),
                };
                let found = self.find(wiki, &contest.action())?;
                add(self, contest, found)?;
            }
        }
        // Protections that hold back a proposal of theirs.
        for protection in self
            .governance
            .active_protections(&wiki.id, self.authorizer.now())
            .map_err(unavailable)?
        {
            let contest = ContestRef {
                kind: AppealedKind::Protection,
                id: protection.id.to_string(),
            };
            let found = self.find(wiki, &contest.action())?;
            add(self, contest, found)?;
        }
        Ok(out)
    }

    /// What a person may contest about **where they stand in the wiki** (the restriction in force on them, and the decision that
    /// lowered their role), unfiled ones only: what the account page draws a *contest this* link from. Nothing for a wiki the person
    /// may not read, and nothing for anyone it does not affect.
    pub fn standing_contests(&self, principal: &Principal, wiki: &WikiRecord) -> Vec<ContestRef> {
        self.standing_checked(principal, wiki).unwrap_or_default()
    }

    fn standing_checked(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
    ) -> Result<Vec<ContestRef>, AppealError> {
        let acting = self.begin(principal, wiki)?;
        let mut out = Vec::new();
        if let Some(notice) = &acting.holding.restriction {
            out.push(ContestRef {
                kind: AppealedKind::Restriction,
                id: notice.id.to_string(),
            });
        }
        for decision in self
            .governance
            .grant_decisions(&wiki.id, Some(&acting.account), 10)
            .map_err(unavailable)?
        {
            let contest = ContestRef {
                kind: AppealedKind::Grant,
                id: decision.id.to_string(),
            };
            if self.find(wiki, &contest.action())?.is_some() {
                out.push(contest);
                break;
            }
        }
        let mut open = Vec::new();
        for contest in out {
            let found = self.find(wiki, &contest.action())?;
            let affected = match &found {
                Some(found) => self.affects(wiki, found, &acting.account)?,
                None => false,
            };
            if affected
                && self
                    .open_of(wiki, &contest.action(), &acting.account)?
                    .is_none()
            {
                open.push(contest);
            }
        }
        Ok(open)
    }

    /// The handle of the decline that closed the proposal, when it is declined and the decline is its last step.
    fn decline_of(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalId,
    ) -> Result<Option<ContestRef>, AppealError> {
        let Some(position) = self
            .proposals
            .last_transition_position(proposal)
            .map_err(unavailable)?
        else {
            return Ok(None);
        };
        let step = self
            .proposals
            .transition_at(&wiki.id, position)
            .map_err(unavailable)?;
        Ok(step
            .filter(|(_, t)| t.op == ProposalOp::Decline)
            .map(|_| ContestRef {
                kind: AppealedKind::Decline,
                id: position.to_string(),
            }))
    }

    /// **What a proposal's page may offer its proposer**: the contest of its decline, and of each temporary protection that holds
    /// it back. Nothing for anyone else, whoever reads the page.
    pub fn for_proposal(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        proposal: &ProposalId,
    ) -> Vec<Contestable> {
        self.for_proposal_checked(principal, wiki, proposal)
            .unwrap_or_default()
    }

    fn for_proposal_checked(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        proposal: &ProposalId,
    ) -> Result<Vec<Contestable>, AppealError> {
        let acting = self.begin(principal, wiki)?;
        let Some(record) = self
            .proposals
            .proposal(proposal, Some(&acting.account))
            .map_err(unavailable)?
            .filter(|p| p.wiki == wiki.id && p.author == acting.account)
        else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        if record.state == ProposalState::Declined
            && let Some(contest) = self.decline_of(wiki, proposal)?
            && let Some(found) = self.find(wiki, &contest.action())?
        {
            let what = self.what_is(wiki, &contest.action(), &found, &acting.account)?;
            let filed = self.open_of(wiki, &contest.action(), &acting.account)?;
            out.push(Contestable {
                contest,
                what,
                filed,
            });
        }
        if record.state.is_open() {
            for protection in self
                .governance
                .active_protections(&wiki.id, self.authorizer.now())
                .map_err(unavailable)?
            {
                if !self
                    .held_by(wiki, &protection, &acting.account)?
                    .contains(proposal)
                {
                    continue;
                }
                let contest = ContestRef {
                    kind: AppealedKind::Protection,
                    id: protection.id.to_string(),
                };
                if let Some(found) = self.find(wiki, &contest.action())? {
                    let what = self.what_is(wiki, &contest.action(), &found, &acting.account)?;
                    let filed = self.open_of(wiki, &contest.action(), &acting.account)?;
                    out.push(Contestable {
                        contest,
                        what,
                        filed,
                    });
                }
            }
        }
        Ok(out)
    }

    /// **What the moderation log says of appeals against a hide**: that it was appealed, and how it ended. No text, no name.
    /// `None` when it was never appealed. (An overturned one wins over an upheld one that came before it; an open one over both.)
    pub fn mark_of_hide(
        &self,
        wiki: &WikiRecord,
        hide: &TombstoneEventId,
    ) -> Result<Option<AppealMark>, AppealError> {
        let appeals = self
            .governance
            .appeals_about(
                &wiki.id,
                &AppealedAction {
                    kind: AppealedKind::Tombstone,
                    id: hide.to_string(),
                },
            )
            .map_err(unavailable)?;
        let mark = AppealMark::of(&appeals);
        Ok(mark)
    }
}
