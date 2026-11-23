//! What an **overturn** writes (RFC 006 D-9): the action reversed as **its own recorded action**, in the same step as the decision.
//!
//! | Kind | The reversing record | Written |
//! |---|---|---|
//! | grant | a grant decision restoring the rung before, **only if** the person still holds the rung the decision gave them, and the decider may grant it (a maintainer's reach is `rungs.grant_up_to`, [`check_grant`]'s own rule) | in the decision's transaction (the store) |
//! | restriction | a *reversed* event after the restriction's last event, if it is still in force | in the decision's transaction |
//! | protection | the same, for the protection | in the decision's transaction |
//! | tombstone | an *unhide* of the hide, with an unhide (naming the appeal too) of every hide **the same act** made | the hide's in the decision's transaction; its siblings and the proposal beside it just before |
//! | decline | a [`ProposalOp::Reopen`] step of the proposal's log, which the decision names | just before the decision |
//!
//! **Every check that can refuse is made first** ([`AppealService::prepare`]): a later change that made the overturn moot, the
//! decider's reach, a proposal nobody could read. Then the writes of the other stores are made ([`AppealService::perform`]) under the
//! decider lock, and the decision, with its own reversing record where the store writes it, last.
//!
//! **What *the same act* is.** One hide of a change's content is one record for each page it wrote, and one for its summary and
//! author; the store has no act id, so the siblings of a hide are the hides in force on the same change, made by the same account
//! under the same category within [`SAME_ACT_SECS`] seconds of it.
//!
//! **A hide that declined a proposal.** Overturning it also reopens the proposal, **if** it is still exactly as that hide left it
//! (declined, and the hide's decline its last step) and nothing else still hides its latest revision's content. The person is made
//! whole in one decision; the decline is part of the hiding.

use inklkunn_store::{
    Actor, AppealRecord, GrantDecisionId, GrantDecisionRecord, MeasureEvent, ProposalChange,
    ProposalId, ProposalOp, ProposalState, ProposalTransition, ProtectionEvent, ProtectionEventId,
    RestrictionEvent, RestrictionEventId, Reversal, Role, TombstoneAct, TombstoneEvent,
    TombstoneEventId, WikiRecord,
};

use super::service::{
    Acting, AppealError, AppealService, OverturnRefusal, store_failure, unavailable,
};
use crate::people::{GrantCheck, GrantRefusal, check_grant};
use crate::proposal::machine;

/// How far apart, in seconds, the hides of one act may be recorded.
pub(super) const SAME_ACT_SECS: i64 = 2;

/// What an overturn will write, all of it checked.
pub(super) enum Prepared {
    /// A record the store writes in the decision's own transaction.
    Direct(Reversal),
    /// The reopening of a declined proposal, which the decision names.
    Decline {
        proposal: ProposalId,
        reopen: ProposalChange,
    },
    /// A hide's unhide, the unhides of the hides of the same act, and the reopening of the proposal that act declined.
    Tombstone {
        main: Box<TombstoneEvent>,
        siblings: Vec<TombstoneEvent>,
        reopen: Option<(ProposalId, ProposalChange)>,
    },
}

fn moot(why: OverturnRefusal) -> AppealError {
    AppealError::Overturn(why)
}

/// The reopening step of a declined proposal, by the person who decided.
fn reopening(
    proposal: &ProposalId,
    by: &inklkunn_store::AccountId,
    reason: &str,
    at: i64,
) -> Result<ProposalChange, AppealError> {
    let to = machine::apply(Some(ProposalState::Declined), ProposalOp::Reopen)
        .map_err(|_| moot(OverturnRefusal::ProposalMovedOn))?;
    Ok(ProposalChange {
        transition: ProposalTransition {
            proposal: proposal.clone(),
            op: ProposalOp::Reopen,
            from: Some(ProposalState::Declined),
            to,
            actor: by.clone(),
            reason: Some(reason.to_owned()),
            at,
        },
        text: None,
        revision: None,
        superseded_by: None,
    })
}

impl AppealService {
    /// Everything an overturn needs, **checked**: the records it will write, and the refusals that stop it.
    pub(super) fn prepare(
        &self,
        wiki: &WikiRecord,
        acting: &Acting,
        rung: Role,
        appeal: &AppealRecord,
        reason: &str,
    ) -> Result<Prepared, AppealError> {
        let now = self.authorizer.now();
        let by = Actor {
            account: acting.account.clone(),
            rung,
        };
        let block = acting.rules.block().map(str::to_owned);
        match appeal.action.kind {
            inklkunn_store::AppealedKind::Grant => {
                self.prepare_grant(wiki, acting, rung, appeal, reason, now)
            }
            inklkunn_store::AppealedKind::Restriction => {
                self.prepare_restriction(wiki, appeal, by, block, reason, now)
            }
            inklkunn_store::AppealedKind::Protection => {
                self.prepare_protection(wiki, appeal, by, block, reason, now)
            }
            inklkunn_store::AppealedKind::Tombstone => {
                self.prepare_tombstone(wiki, acting, appeal, by, block, reason, now)
            }
            inklkunn_store::AppealedKind::Decline => {
                self.prepare_decline(wiki, acting, appeal, reason, now)
            }
        }
    }

    fn prepare_grant(
        &self,
        wiki: &WikiRecord,
        acting: &Acting,
        rung: Role,
        appeal: &AppealRecord,
        reason: &str,
        now: i64,
    ) -> Result<Prepared, AppealError> {
        let decision = GrantDecisionId::parse(&appeal.action.id)
            .and_then(|id| self.governance.grant_decision(&id).transpose())
            .transpose()
            .map_err(unavailable)?
            .filter(|d| d.wiki == wiki.id)
            .ok_or(moot(OverturnRefusal::RoleChanged))?;
        // The person must still hold what the decision gave them: a later change is theirs to keep.
        let current = self
            .roles
            .role(&decision.target, &wiki.id)
            .map_err(unavailable)?;
        if current != decision.after {
            return Err(moot(OverturnRefusal::RoleChanged));
        }
        let grant_up_to = acting.rules.policy().rungs().grant_up_to();
        check_grant(&GrantCheck {
            actor: rung,
            own: decision.target == acting.account,
            current,
            new: decision.before,
            grant_up_to,
            // Restoring a rung that was lowered never takes an admin grant away.
            other_admins_who_can_sign_in: 1,
        })
        .map_err(|why| match why {
            GrantRefusal::AboveReach { up_to } => moot(OverturnRefusal::AboveReach { up_to }),
            GrantRefusal::NotAllowed => moot(OverturnRefusal::AboveReach { up_to: grant_up_to }),
            GrantRefusal::OwnRung | GrantRefusal::NothingChanges | GrantRefusal::LastAdmin => {
                moot(OverturnRefusal::RoleChanged)
            }
        })?;
        Ok(Prepared::Direct(Reversal::Grant(GrantDecisionRecord {
            id: GrantDecisionId::new(),
            wiki: wiki.id.clone(),
            at: now,
            by: Some(Actor {
                account: acting.account.clone(),
                rung,
            }),
            policy_block: acting.rules.block().map(str::to_owned),
            target: decision.target,
            before: current,
            after: decision.before,
            reason: Some(reason.to_owned()),
            appeal: Some(appeal.id.clone()),
        })))
    }

    fn prepare_restriction(
        &self,
        wiki: &WikiRecord,
        appeal: &AppealRecord,
        by: Actor,
        block: Option<String>,
        reason: &str,
        now: i64,
    ) -> Result<Prepared, AppealError> {
        let first = RestrictionEventId::parse(&appeal.action.id)
            .and_then(|id| self.governance.restriction_event(&id).transpose())
            .transpose()
            .map_err(unavailable)?
            .filter(|e| e.wiki == wiki.id)
            .ok_or(moot(OverturnRefusal::RestrictionEnded))?;
        // The restriction may have been renewed since the appeal was filed: the one in force is the last of its chain.
        let events = self
            .governance
            .restriction_events(&wiki.id, &first.account)
            .map_err(unavailable)?;
        let mut tail = first.id.clone();
        for _ in 0..events.len() {
            match events.iter().find(|e| e.of_event.as_ref() == Some(&tail)) {
                Some(next) => tail = next.id.clone(),
                None => break,
            }
        }
        let in_force = self
            .governance
            .active_restrictions(&wiki.id, &first.account, now)
            .map_err(unavailable)?
            .into_iter()
            .any(|e| e.id == tail);
        if !in_force {
            return Err(moot(OverturnRefusal::RestrictionEnded));
        }
        Ok(Prepared::Direct(Reversal::Restriction(RestrictionEvent {
            id: RestrictionEventId::new(),
            wiki: wiki.id.clone(),
            at: now,
            event: MeasureEvent::Reversed,
            account: first.account,
            by,
            policy_block: block,
            reason: Some(reason.to_owned()),
            expires_at: None,
            of_event: Some(tail),
            appeal: Some(appeal.id.clone()),
        })))
    }

    fn prepare_protection(
        &self,
        wiki: &WikiRecord,
        appeal: &AppealRecord,
        by: Actor,
        block: Option<String>,
        reason: &str,
        now: i64,
    ) -> Result<Prepared, AppealError> {
        let first = ProtectionEventId::parse(&appeal.action.id)
            .and_then(|id| self.governance.protection_event(&id).transpose())
            .transpose()
            .map_err(unavailable)?
            .filter(|e| e.wiki == wiki.id)
            .ok_or(moot(OverturnRefusal::ProtectionEnded))?;
        let events = self
            .governance
            .protection_events(&wiki.id, inklkunn_store::governance::MAX_PAGE)
            .map_err(unavailable)?;
        let mut tail = first.id.clone();
        for _ in 0..events.len() {
            match events.iter().find(|e| e.of_event.as_ref() == Some(&tail)) {
                Some(next) => tail = next.id.clone(),
                None => break,
            }
        }
        let in_force = self
            .governance
            .active_protections(&wiki.id, now)
            .map_err(unavailable)?
            .into_iter()
            .any(|e| e.id == tail);
        if !in_force {
            return Err(moot(OverturnRefusal::ProtectionEnded));
        }
        Ok(Prepared::Direct(Reversal::Protection(ProtectionEvent {
            id: ProtectionEventId::new(),
            wiki: wiki.id.clone(),
            at: now,
            event: MeasureEvent::Reversed,
            kind: first.kind,
            target: first.target,
            requirement: None,
            by,
            policy_block: block,
            reason: Some(reason.to_owned()),
            expires_at: None,
            of_event: Some(tail),
            appeal: Some(appeal.id.clone()),
        })))
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_tombstone(
        &self,
        wiki: &WikiRecord,
        acting: &Acting,
        appeal: &AppealRecord,
        by: Actor,
        block: Option<String>,
        reason: &str,
        now: i64,
    ) -> Result<Prepared, AppealError> {
        // Still in force: nobody unhid it meanwhile.
        if self.find(wiki, &appeal.action)?.is_none() {
            return Err(moot(OverturnRefusal::HidingEnded));
        }
        let hide = TombstoneEventId::parse(&appeal.action.id)
            .and_then(|id| self.governance.tombstone_event(&id).transpose())
            .transpose()
            .map_err(unavailable)?
            .ok_or(moot(OverturnRefusal::HidingEnded))?;
        let all = self
            .governance
            .tombstone_events_of_id(&wiki.id, hide.item.kind, &hide.item.id)
            .map_err(unavailable)?;
        let in_force: Vec<&TombstoneEvent> = all
            .iter()
            .filter(|h| h.act == TombstoneAct::Hide)
            .filter(|h| {
                !all.iter()
                    .any(|e| e.act == TombstoneAct::Unhide && e.of_event.as_ref() == Some(&h.id))
            })
            .collect();
        let unhide = |of: &TombstoneEvent, id: TombstoneEventId| TombstoneEvent {
            id,
            wiki: wiki.id.clone(),
            at: now,
            act: TombstoneAct::Unhide,
            item: of.item.clone(),
            parts: of.parts,
            category: None,
            note: None,
            affected: of.affected.clone(),
            by: by.clone(),
            policy_block: block.clone(),
            of_event: Some(of.id.clone()),
            appeal: Some(appeal.id.clone()),
            declined_transition: None,
        };
        let siblings: Vec<&TombstoneEvent> = in_force
            .iter()
            .copied()
            .filter(|h| {
                h.id != hide.id
                    && h.by.account == hide.by.account
                    && h.category == hide.category
                    && (h.at - hide.at).abs() <= SAME_ACT_SECS
            })
            .collect();
        let reopen = match hide.declined_transition {
            Some(position) => {
                let ours: Vec<&TombstoneEventId> = std::iter::once(&hide.id)
                    .chain(siblings.iter().map(|s| &s.id))
                    .collect();
                self.reopening_after_unhide(wiki, acting, position, &ours, reason, now)?
            }
            None => None,
        };
        Ok(Prepared::Tombstone {
            main: Box::new(unhide(&hide, TombstoneEventId::new())),
            siblings: siblings
                .into_iter()
                .map(|s| unhide(s, TombstoneEventId::new()))
                .collect(),
            reopen,
        })
    }

    /// The reopening of the proposal a hide declined, **if** it is still as the hide left it and nothing but the hides being
    /// reversed hides its latest revision's content. Otherwise there is none (the proposal stays as it is; its proposer may appeal
    /// the decline itself).
    fn reopening_after_unhide(
        &self,
        wiki: &WikiRecord,
        acting: &Acting,
        position: i64,
        ours: &[&TombstoneEventId],
        reason: &str,
        now: i64,
    ) -> Result<Option<(ProposalId, ProposalChange)>, AppealError> {
        let Some((proposal, step)) = self
            .proposals
            .transition_at(&wiki.id, position)
            .map_err(unavailable)?
        else {
            return Ok(None);
        };
        let Some(record) = self
            .proposals
            .proposal(&proposal, None)
            .map_err(unavailable)?
            .filter(|p| p.state == ProposalState::Declined)
        else {
            return Ok(None);
        };
        let last = self
            .proposals
            .last_transition_position(&proposal)
            .map_err(unavailable)?;
        if step.op != ProposalOp::Decline || last != Some(position) {
            return Ok(None);
        }
        if self.latest_content_hidden_beyond(wiki, &proposal, ours)? {
            return Ok(None);
        }
        Ok(Some((
            record.id,
            reopening(&proposal, &acting.account, reason, now)?,
        )))
    }

    /// Is the content of the proposal's latest revision hidden by any hide **other than** `except`?
    fn latest_content_hidden_beyond(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalId,
        except: &[&TombstoneEventId],
    ) -> Result<bool, AppealError> {
        let latest = self
            .proposals
            .revisions(proposal, None)
            .map_err(unavailable)?
            .last()
            .map(|r| r.seq);
        let Some(latest) = latest else {
            return Ok(false);
        };
        let hidden = self
            .tombstones
            .proposal_revision(&wiki.id, proposal, latest)
            .map_err(unavailable)?;
        Ok(hidden
            .marks()
            .iter()
            .any(|m| m.parts.content && !except.contains(&&m.event)))
    }

    fn prepare_decline(
        &self,
        wiki: &WikiRecord,
        acting: &Acting,
        appeal: &AppealRecord,
        reason: &str,
        now: i64,
    ) -> Result<Prepared, AppealError> {
        let found = self
            .find(wiki, &appeal.action)?
            .and_then(|f| f.proposal)
            .ok_or(moot(OverturnRefusal::ProposalMovedOn))?;
        // A proposal nobody can read cannot be reviewed: what hides it is what to overturn.
        if self.latest_content_hidden_beyond(wiki, &found, &[])? {
            return Err(moot(OverturnRefusal::StillHidden));
        }
        Ok(Prepared::Decline {
            reopen: reopening(&found, &acting.account, reason, now)?,
            proposal: found,
        })
    }

    /// Write what an overturn writes **outside the decision's transaction**, and return the record the decision carries (and the
    /// proposal it brought back, if it did). Called with the decider lock held for the two kinds that need it.
    pub(super) fn perform(
        &self,
        prepared: Prepared,
    ) -> Result<(Reversal, Option<ProposalId>), AppealError> {
        match prepared {
            Prepared::Direct(reversal) => Ok((reversal, None)),
            Prepared::Decline { proposal, reopen } => {
                self.proposals
                    .change_proposal(&reopen)
                    .map_err(|e| match e {
                        inklkunn_store::StoreError::StateChanged => {
                            moot(OverturnRefusal::ProposalMovedOn)
                        }
                        other => store_failure(other),
                    })?;
                let position = self
                    .proposals
                    .last_transition_position(&proposal)
                    .map_err(unavailable)?
                    .ok_or(AppealError::Changed)?;
                Ok((
                    Reversal::Decline {
                        transition: position,
                    },
                    Some(proposal),
                ))
            }
            Prepared::Tombstone {
                main,
                siblings,
                reopen,
            } => {
                for sibling in &siblings {
                    self.governance
                        .record_tombstone_event(sibling)
                        .map_err(store_failure)?;
                }
                let reopened = match reopen {
                    Some((proposal, change)) => {
                        match self.proposals.change_proposal(&change) {
                            Ok(_) => Some(proposal),
                            // Moved on since it was checked (it cannot, under the decider lock, but the answer is plain): the
                            // hiding is reversed all the same.
                            Err(inklkunn_store::StoreError::StateChanged) => None,
                            Err(other) => return Err(store_failure(other)),
                        }
                    }
                    None => None,
                };
                Ok((Reversal::Tombstone(*main), reopened))
            }
        }
    }
}
