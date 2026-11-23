//! The in-memory records of governance and moderation: the reference the SQLite store must agree with
//! ([`crate::governance`] says what they are). It keeps the same rules the database's triggers keep, in the same words.

use std::sync::{MutexGuard, PoisonError};

use super::MemoryStore;
use crate::access::{Role, RoleGrant};
use crate::governance::{
    AppealDecision, AppealOutcome, AppealRecord, AppealState, AppealedAction, AppealedKind,
    Decided, GovernanceStore, GrantDecisionRecord, HiddenItem, MAX_PAGE, MeasureEvent,
    ProtectionEvent, ProtectionKind, RestrictionEvent, Reversal, TombstoneAct, TombstoneEvent,
};
use crate::record::{
    AccountId, AppealId, GrantDecisionId, ProtectionEventId, RestrictionEventId, TombstoneEventId,
    WikiId,
};
use crate::{Result, StoreError};

/// Every governance record, each list in the order written.
#[derive(Debug, Default, Clone)]
pub(super) struct Governance {
    grants: Vec<GrantDecisionRecord>,
    restrictions: Vec<RestrictionEvent>,
    protections: Vec<ProtectionEvent>,
    tombstones: Vec<TombstoneEvent>,
    appeals: Vec<AppealRecord>,
}

fn broken<T>(rule: &str) -> Result<T> {
    Err(StoreError::Backend(rule.to_owned()))
}

fn clamp(limit: usize) -> usize {
    limit.clamp(1, MAX_PAGE)
}

impl Governance {
    /// An appeal a record names must be there, open, of its kind, in its wiki.
    fn check_appeal(
        &self,
        named: Option<&AppealId>,
        wiki: &WikiId,
        kind: AppealedKind,
        words: &str,
    ) -> Result<()> {
        let Some(named) = named else {
            return Ok(());
        };
        let Some(appeal) = self.appeals.iter().find(|a| a.id == *named) else {
            return Err(StoreError::NotFound);
        };
        if appeal.wiki != *wiki || appeal.state != AppealState::Open || appeal.action.kind != kind {
            return broken(words);
        }
        Ok(())
    }

    fn add_grant(&mut self, grants: &mut Vec<RoleGrant>, d: &GrantDecisionRecord) -> Result<()> {
        d.validate()?;
        if self.grants.iter().any(|g| g.id == d.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        if let Some(appeal) = &d.appeal
            && !self.appeals.iter().any(|a| a.id == *appeal)
        {
            return Err(StoreError::NotFound);
        }
        let held = grants
            .iter()
            .find(|g| g.account == d.target && g.wiki == d.wiki)
            .map(|g| g.role);
        if held != d.before {
            return Err(StoreError::StateChanged);
        }
        self.check_appeal(
            d.appeal.as_ref(),
            &d.wiki,
            AppealedKind::Grant,
            "an appeal named here is an open appeal of a grant in this wiki",
        )?;
        self.grants.push(d.clone());
        grants.retain(|g| !(g.account == d.target && g.wiki == d.wiki));
        if let Some(after) = d.after {
            grants.push(RoleGrant {
                account: d.target.clone(),
                wiki: d.wiki.clone(),
                role: after,
                granted_by: d.by.as_ref().map(|a| a.account.clone()),
                granted_at: d.at,
            });
        }
        Ok(())
    }

    fn add_restriction(&mut self, e: &RestrictionEvent) -> Result<()> {
        e.validate()?;
        if self.restrictions.iter().any(|r| r.id == e.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        if let Some(of) = &e.of_event {
            if self
                .restrictions
                .iter()
                .any(|r| r.of_event.as_ref() == Some(of))
            {
                return Err(StoreError::AlreadyRecorded);
            }
            if !self.restrictions.iter().any(|r| r.id == *of) {
                return Err(StoreError::NotFound);
            }
        }
        if let Some(appeal) = &e.appeal
            && !self.appeals.iter().any(|a| a.id == *appeal)
        {
            return Err(StoreError::NotFound);
        }
        if let Some(of) = &e.of_event {
            let follows = self.restrictions.iter().any(|p| {
                p.id == *of
                    && p.wiki == e.wiki
                    && p.account == e.account
                    && matches!(p.event, MeasureEvent::Set | MeasureEvent::Renewed)
                    && p.at <= e.at
            });
            if !follows {
                return broken(
                    "an event follows a set or renewed event of the same account in the same wiki",
                );
            }
        }
        self.check_appeal(
            e.appeal.as_ref(),
            &e.wiki,
            AppealedKind::Restriction,
            "an appeal named here is an open appeal of a restriction in this wiki",
        )?;
        self.restrictions.push(e.clone());
        Ok(())
    }

    fn add_protection(&mut self, e: &ProtectionEvent) -> Result<()> {
        e.validate()?;
        if self.protections.iter().any(|p| p.id == e.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        if let Some(of) = &e.of_event {
            if self
                .protections
                .iter()
                .any(|p| p.of_event.as_ref() == Some(of))
            {
                return Err(StoreError::AlreadyRecorded);
            }
            if !self.protections.iter().any(|p| p.id == *of) {
                return Err(StoreError::NotFound);
            }
        }
        if let Some(appeal) = &e.appeal
            && !self.appeals.iter().any(|a| a.id == *appeal)
        {
            return Err(StoreError::NotFound);
        }
        if let Some(of) = &e.of_event {
            let follows = self.protections.iter().any(|p| {
                p.id == *of
                    && p.wiki == e.wiki
                    && p.kind == e.kind
                    && p.target == e.target
                    && matches!(p.event, MeasureEvent::Set | MeasureEvent::Renewed)
                    && p.at <= e.at
            });
            if !follows {
                return broken(
                    "an event follows a set or renewed event of the same page or namespace in the same wiki",
                );
            }
        }
        self.check_appeal(
            e.appeal.as_ref(),
            &e.wiki,
            AppealedKind::Protection,
            "an appeal named here is an open appeal of a protection in this wiki",
        )?;
        self.protections.push(e.clone());
        Ok(())
    }

    fn add_tombstone(&mut self, e: &TombstoneEvent) -> Result<()> {
        e.validate()?;
        if self.tombstones.iter().any(|t| t.id == e.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        if let Some(of) = &e.of_event {
            if self
                .tombstones
                .iter()
                .any(|t| t.of_event.as_ref() == Some(of))
            {
                return Err(StoreError::AlreadyRecorded);
            }
            if !self.tombstones.iter().any(|t| t.id == *of) {
                return Err(StoreError::NotFound);
            }
        }
        if let Some(appeal) = &e.appeal
            && !self.appeals.iter().any(|a| a.id == *appeal)
        {
            return Err(StoreError::NotFound);
        }
        if let Some(of) = &e.of_event {
            let follows = self.tombstones.iter().any(|h| {
                h.id == *of
                    && h.act == TombstoneAct::Hide
                    && h.wiki == e.wiki
                    && h.item == e.item
                    && h.parts == e.parts
                    && h.at <= e.at
            });
            if !follows {
                return broken("an unhide follows a hide of the same item and parts");
            }
        }
        self.check_appeal(
            e.appeal.as_ref(),
            &e.wiki,
            AppealedKind::Tombstone,
            "an appeal named here is an open appeal of a tombstone in this wiki",
        )?;
        if self.tombstones.iter().any(|t| {
            t.act_id == e.act_id
                && (t.wiki != e.wiki
                    || t.at != e.at
                    || t.by.account != e.by.account
                    || t.act != e.act)
        }) {
            return broken(
                "the events of one act are of one wiki, one time, one person and one kind",
            );
        }
        self.tombstones.push(e.clone());
        Ok(())
    }
}

/// A chain's events in force at `at`: a `Set` or `Renewed` event that has begun, has not ended, and nothing follows.
/// What [`in_force`] reads of a restriction event.
fn restriction_parts(e: &RestrictionEvent) -> (MeasureEvent, i64, Option<i64>, Option<&str>) {
    (
        e.event,
        e.at,
        e.expires_at,
        e.of_event.as_ref().map(RestrictionEventId::as_str),
    )
}

/// What [`in_force`] reads of a protection event.
fn protection_parts(e: &ProtectionEvent) -> (MeasureEvent, i64, Option<i64>, Option<&str>) {
    (
        e.event,
        e.at,
        e.expires_at,
        e.of_event.as_ref().map(ProtectionEventId::as_str),
    )
}

fn in_force<E>(
    events: &[E],
    at: i64,
    id_of: impl Fn(&E) -> &str,
    parts: impl Fn(&E) -> (MeasureEvent, i64, Option<i64>, Option<&str>),
) -> Vec<&E> {
    let mut live: Vec<&E> = events
        .iter()
        .filter(|e| {
            let (event, begun, ends, _) = parts(e);
            event.puts_in_place()
                && begun <= at
                && ends.is_some_and(|end| end > at)
                && !events.iter().any(|n| parts(n).3 == Some(id_of(e)))
        })
        .collect();
    // The one that ends last first, and of those the later written first (the SQL reads `ORDER BY expires_at DESC, seq DESC`).
    live.reverse();
    live.sort_by_key(|e| std::cmp::Reverse(parts(e).2));
    live
}

impl MemoryStore {
    fn governance_lock(&self) -> MutexGuard<'_, Governance> {
        self.governance
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The wiki, the accounts and the proposal-log position a record names are there.
    fn governance_refs(
        &self,
        wiki: &WikiId,
        accounts: &[&AccountId],
        transition: Option<i64>,
    ) -> Result<()> {
        if !self.wiki_exists(wiki) {
            return Err(StoreError::NotFound);
        }
        if accounts.iter().any(|a| !self.has_account(a)) {
            return Err(StoreError::AccountNotFound);
        }
        if let Some(seq) = transition
            && self.transition_wiki(seq).as_ref() != Some(wiki)
        {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    fn grant_refs(&self, d: &GrantDecisionRecord) -> Result<()> {
        let mut who = vec![&d.target];
        who.extend(d.by.as_ref().map(|a| &a.account));
        self.governance_refs(&d.wiki, &who, None)
    }

    fn restriction_refs(&self, e: &RestrictionEvent) -> Result<()> {
        self.governance_refs(&e.wiki, &[&e.account, &e.by.account], None)
    }

    fn protection_refs(&self, e: &ProtectionEvent) -> Result<()> {
        self.governance_refs(&e.wiki, &[&e.by.account], None)
    }

    fn tombstone_refs(&self, e: &TombstoneEvent) -> Result<()> {
        let mut who = vec![&e.by.account];
        who.extend(e.affected.as_ref());
        self.governance_refs(&e.wiki, &who, e.declined_transition)
    }
}

impl GovernanceStore for MemoryStore {
    fn record_grant_decision(&self, decision: &GrantDecisionRecord) -> Result<()> {
        decision.validate()?;
        self.grant_refs(decision)?;
        let mut g = self.governance_lock();
        let mut grants = self.grants.lock().unwrap_or_else(PoisonError::into_inner);
        g.add_grant(&mut grants, decision)
    }

    fn record_grant_decision_keeping_an_admin(&self, decision: &GrantDecisionRecord) -> Result<()> {
        decision.validate()?;
        self.grant_refs(decision)?;
        let mut g = self.governance_lock();
        let mut grants = self.grants.lock().unwrap_or_else(PoisonError::into_inner);
        if decision.before == Some(Role::Admin) && decision.after != Some(Role::Admin) {
            let others = grants
                .iter()
                .filter(|h| {
                    h.wiki == decision.wiki
                        && h.role == Role::Admin
                        && h.account != decision.target
                        && self.can_sign_in(&h.account)
                })
                .count();
            if others == 0 {
                return Err(StoreError::StateChanged);
            }
        }
        g.add_grant(&mut grants, decision)
    }

    fn grant_decision(&self, id: &GrantDecisionId) -> Result<Option<GrantDecisionRecord>> {
        Ok(self
            .governance_lock()
            .grants
            .iter()
            .find(|g| g.id == *id)
            .cloned())
    }

    fn grant_decisions(
        &self,
        wiki: &WikiId,
        target: Option<&AccountId>,
        limit: usize,
    ) -> Result<Vec<GrantDecisionRecord>> {
        Ok(self
            .governance_lock()
            .grants
            .iter()
            .rev()
            .filter(|g| g.wiki == *wiki && target.is_none_or(|t| g.target == *t))
            .take(clamp(limit))
            .cloned()
            .collect())
    }

    fn record_restriction_event(&self, event: &RestrictionEvent) -> Result<()> {
        event.validate()?;
        self.restriction_refs(event)?;
        self.governance_lock().add_restriction(event)
    }

    fn record_restriction_event_if_none_in_force(&self, event: &RestrictionEvent) -> Result<()> {
        event.validate()?;
        self.restriction_refs(event)?;
        let mut g = self.governance_lock();
        let of_account: Vec<RestrictionEvent> = g
            .restrictions
            .iter()
            .filter(|r| r.wiki == event.wiki && r.account == event.account)
            .cloned()
            .collect();
        if !in_force(&of_account, event.at, |e| e.id.as_str(), restriction_parts).is_empty() {
            return Err(StoreError::StateChanged);
        }
        g.add_restriction(event)
    }

    fn restriction_event(&self, id: &RestrictionEventId) -> Result<Option<RestrictionEvent>> {
        Ok(self
            .governance_lock()
            .restrictions
            .iter()
            .find(|r| r.id == *id)
            .cloned())
    }

    fn restriction_events(
        &self,
        wiki: &WikiId,
        account: &AccountId,
    ) -> Result<Vec<RestrictionEvent>> {
        Ok(self
            .governance_lock()
            .restrictions
            .iter()
            .filter(|r| r.wiki == *wiki && r.account == *account)
            .cloned()
            .collect())
    }

    fn active_restrictions(
        &self,
        wiki: &WikiId,
        account: &AccountId,
        at: i64,
    ) -> Result<Vec<RestrictionEvent>> {
        let g = self.governance_lock();
        let of_account: Vec<RestrictionEvent> = g
            .restrictions
            .iter()
            .filter(|r| r.wiki == *wiki && r.account == *account)
            .cloned()
            .collect();
        // A chain never crosses an account, so reading the account's events alone finds every follower.
        Ok(in_force(
            &of_account,
            at,
            |e| e.id.as_str(),
            |e| {
                (
                    e.event,
                    e.at,
                    e.expires_at,
                    e.of_event.as_ref().map(RestrictionEventId::as_str),
                )
            },
        )
        .into_iter()
        .cloned()
        .collect())
    }

    fn active_restrictions_in(
        &self,
        wiki: &WikiId,
        at: i64,
        limit: usize,
    ) -> Result<Vec<RestrictionEvent>> {
        let g = self.governance_lock();
        let of_wiki: Vec<RestrictionEvent> = g
            .restrictions
            .iter()
            .filter(|r| r.wiki == *wiki)
            .cloned()
            .collect();
        Ok(in_force(
            &of_wiki,
            at,
            |e| e.id.as_str(),
            |e| {
                (
                    e.event,
                    e.at,
                    e.expires_at,
                    e.of_event.as_ref().map(RestrictionEventId::as_str),
                )
            },
        )
        .into_iter()
        .take(clamp(limit))
        .cloned()
        .collect())
    }

    fn record_protection_event(&self, event: &ProtectionEvent) -> Result<()> {
        event.validate()?;
        self.protection_refs(event)?;
        self.governance_lock().add_protection(event)
    }

    fn record_protection_event_if_none_in_force(&self, event: &ProtectionEvent) -> Result<()> {
        event.validate()?;
        self.protection_refs(event)?;
        let mut g = self.governance_lock();
        let of_target: Vec<ProtectionEvent> = g
            .protections
            .iter()
            .filter(|p| p.wiki == event.wiki && p.kind == event.kind && p.target == event.target)
            .cloned()
            .collect();
        if !in_force(&of_target, event.at, |e| e.id.as_str(), protection_parts).is_empty() {
            return Err(StoreError::StateChanged);
        }
        g.add_protection(event)
    }

    fn protection_event(&self, id: &ProtectionEventId) -> Result<Option<ProtectionEvent>> {
        Ok(self
            .governance_lock()
            .protections
            .iter()
            .find(|p| p.id == *id)
            .cloned())
    }

    fn protection_events(&self, wiki: &WikiId, limit: usize) -> Result<Vec<ProtectionEvent>> {
        Ok(self
            .governance_lock()
            .protections
            .iter()
            .rev()
            .filter(|p| p.wiki == *wiki)
            .take(clamp(limit))
            .cloned()
            .collect())
    }

    fn active_protections(&self, wiki: &WikiId, at: i64) -> Result<Vec<ProtectionEvent>> {
        self.active_protections_where(wiki, at, |_| true)
    }

    fn active_protections_on(
        &self,
        wiki: &WikiId,
        kind: ProtectionKind,
        target: &str,
        at: i64,
    ) -> Result<Vec<ProtectionEvent>> {
        self.active_protections_where(wiki, at, |p| p.kind == kind && p.target == target)
    }

    fn record_tombstone_event(&self, event: &TombstoneEvent) -> Result<()> {
        event.validate()?;
        self.tombstone_refs(event)?;
        self.governance_lock().add_tombstone(event)
    }

    fn tombstone_event(&self, id: &TombstoneEventId) -> Result<Option<TombstoneEvent>> {
        Ok(self
            .governance_lock()
            .tombstones
            .iter()
            .find(|t| t.id == *id)
            .cloned())
    }

    fn tombstone_events_of(&self, wiki: &WikiId, item: &HiddenItem) -> Result<Vec<TombstoneEvent>> {
        Ok(self
            .governance_lock()
            .tombstones
            .iter()
            .filter(|t| t.wiki == *wiki && t.item == *item)
            .cloned()
            .collect())
    }

    fn tombstone_events_of_id(
        &self,
        wiki: &WikiId,
        kind: crate::governance::HiddenItemKind,
        id: &str,
    ) -> Result<Vec<TombstoneEvent>> {
        Ok(self
            .governance_lock()
            .tombstones
            .iter()
            .filter(|t| t.wiki == *wiki && t.item.kind == kind && t.item.id == id)
            .cloned()
            .collect())
    }

    fn hidden_ids(
        &self,
        wiki: &WikiId,
        kind: crate::governance::HiddenItemKind,
        part: crate::governance::HiddenPart,
        limit: usize,
    ) -> Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        for t in self.governance_lock().tombstones.iter().filter(|t| {
            t.wiki == *wiki
                && t.item.kind == kind
                && t.act == crate::governance::TombstoneAct::Hide
                && part.in_parts(t.parts)
        }) {
            if !out.contains(&t.item.id) {
                out.push(t.item.id.clone());
            }
        }
        out.truncate(limit.clamp(1, crate::governance::MAX_HIDDEN_IDS + 1));
        Ok(out)
    }

    fn tombstone_log(
        &self,
        wiki: &WikiId,
        before: Option<&TombstoneEventId>,
        limit: usize,
    ) -> Result<Vec<TombstoneEvent>> {
        let g = self.governance_lock();
        let end = match before {
            None => g.tombstones.len(),
            Some(id) => g
                .tombstones
                .iter()
                .position(|t| t.id == *id && t.wiki == *wiki)
                .ok_or(StoreError::NotFound)?,
        };
        Ok(g.tombstones
            .iter()
            .take(end)
            .rev()
            .filter(|t| t.wiki == *wiki)
            .take(clamp(limit))
            .cloned()
            .collect())
    }

    fn file_appeal(&self, appeal: &AppealRecord) -> Result<()> {
        appeal.validate()?;
        let mut who = vec![&appeal.appellant];
        who.extend(appeal.action_actor.as_ref());
        self.governance_refs(&appeal.wiki, &who, None)?;
        let mut g = self.governance_lock();
        if g.appeals.iter().any(|a| a.id == appeal.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        if g.appeals.iter().any(|a| {
            a.state == AppealState::Open
                && a.wiki == appeal.wiki
                && a.action == appeal.action
                && a.appellant == appeal.appellant
        }) {
            return Err(StoreError::AlreadyRecorded);
        }
        g.appeals.push(appeal.clone());
        Ok(())
    }

    fn appeal(&self, id: &AppealId) -> Result<Option<AppealRecord>> {
        Ok(self
            .governance_lock()
            .appeals
            .iter()
            .find(|a| a.id == *id)
            .cloned())
    }

    fn decide_appeal(&self, id: &AppealId, decision: &AppealDecision) -> Result<()> {
        decision.validate()?;
        let mut g = self.governance_lock();
        let mut grants = self.grants.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(appeal) = g.appeals.iter().find(|a| a.id == *id).cloned() else {
            return Err(StoreError::NotFound);
        };
        if appeal.state != AppealState::Open {
            return Err(StoreError::StateChanged);
        }
        if !self.has_account(&decision.by) {
            return Err(StoreError::AccountNotFound);
        }
        // Work on copies, and keep them only when every step has been taken: the decision is one step, with every record of
        // its reversal (A-1).
        let (mut work, mut work_grants) = (g.clone(), grants.clone());
        let mut record_id = None;
        let mut reopening = None;
        let mut overturned = false;
        if let AppealOutcome::Overturned(reversal) = &decision.outcome {
            overturned = true;
            if reversal.kind() != appeal.action.kind {
                return broken("an overturn reverses an action of the kind that was appealed");
            }
            if let Some((named, wiki)) = reversal.names()
                && (named != id || *wiki != appeal.wiki)
            {
                return broken("the record that reverses an action names the appeal it overturns");
            }
            if !reversal.companions_name(id, &appeal.wiki) {
                return broken("the record that reverses an action names the appeal it overturns");
            }
            match &**reversal {
                Reversal::Grant(d) => {
                    self.grant_refs(d)?;
                    work.add_grant(&mut work_grants, d)?;
                }
                Reversal::Restriction(e) => {
                    self.restriction_refs(e)?;
                    work.add_restriction(e)?;
                }
                Reversal::Protection(e) => {
                    self.protection_refs(e)?;
                    work.add_protection(e)?;
                }
                Reversal::Tombstone(r) => {
                    for e in std::iter::once(&r.main).chain(&r.siblings) {
                        self.tombstone_refs(e)?;
                        work.add_tombstone(e)?;
                    }
                }
                Reversal::Decline { .. } => {}
            }
            record_id = reversal.record_id();
            reopening = reversal.reopening();
        }
        if decision.by == appeal.appellant || appeal.action_actor.as_ref() == Some(&decision.by) {
            return broken(
                "the database refuses the record: an appeal is decided by neither the appellant nor the one who took the action",
            );
        }
        let decided = Decided {
            at: decision.at,
            by: decision.by.clone(),
            reason: decision.reason.clone(),
        };
        if decided.at < appeal.at {
            return broken(
                "the database refuses the record: an appeal is decided after it is filed",
            );
        }
        let Some(slot) = work.appeals.iter_mut().find(|a| a.id == *id) else {
            return Err(StoreError::NotFound);
        };
        // The proposal's reopening is the one record that is not in the copy (the proposals have their own lock): it is checked
        // and written last, when nothing else can refuse the decision, and the copies are kept only if it was written.
        if let Some(change) = reopening {
            let known = self.has_account(&change.transition.actor);
            let mut proposals = self.proposals_lock();
            if proposals.wiki_of(&change.transition.proposal).as_ref() != Some(&appeal.wiki) {
                return Err(StoreError::NotFound);
            }
            let declines = record_id.is_none();
            if declines && !proposals.is_step_of(&appeal.action.id, &change.transition.proposal) {
                return broken("a decline is undone by reopening the proposal it declined");
            }
            proposals.apply(change, known)?;
            if declines {
                record_id = Some(proposals.last_position().to_string());
            }
        }
        slot.state = match (overturned, record_id) {
            (true, Some(record)) => AppealState::Overturned(decided, record),
            _ => AppealState::Upheld(decided),
        };
        *g = work;
        *grants = work_grants;
        Ok(())
    }

    fn open_appeals(&self, wiki: &WikiId, limit: usize) -> Result<Vec<AppealRecord>> {
        Ok(self
            .governance_lock()
            .appeals
            .iter()
            .filter(|a| a.wiki == *wiki && a.state == AppealState::Open)
            .take(clamp(limit))
            .cloned()
            .collect())
    }

    fn recent_appeals(&self, wiki: &WikiId, limit: usize) -> Result<Vec<AppealRecord>> {
        Ok(self
            .governance_lock()
            .appeals
            .iter()
            .rev()
            .filter(|a| a.wiki == *wiki)
            .take(clamp(limit))
            .cloned()
            .collect())
    }

    fn appeals_by(&self, wiki: &WikiId, appellant: &AccountId) -> Result<Vec<AppealRecord>> {
        Ok(self
            .governance_lock()
            .appeals
            .iter()
            .rev()
            .filter(|a| a.wiki == *wiki && a.appellant == *appellant)
            .cloned()
            .collect())
    }

    fn appeals_about(&self, wiki: &WikiId, action: &AppealedAction) -> Result<Vec<AppealRecord>> {
        Ok(self
            .governance_lock()
            .appeals
            .iter()
            .filter(|a| a.wiki == *wiki && a.action == *action)
            .cloned()
            .collect())
    }

    fn open_appeal_count(&self, wiki: &WikiId, appellant: &AccountId) -> Result<u64> {
        let n = self
            .governance_lock()
            .appeals
            .iter()
            .filter(|a| {
                a.wiki == *wiki && a.appellant == *appellant && a.state == AppealState::Open
            })
            .count();
        Ok(u64::try_from(n).unwrap_or(u64::MAX))
    }

    fn appeals_since(
        &self,
        wiki: &WikiId,
        appellant: Option<&AccountId>,
        since: i64,
    ) -> Result<Vec<i64>> {
        let mut times: Vec<i64> = self
            .governance_lock()
            .appeals
            .iter()
            .filter(|a| {
                a.wiki == *wiki && a.at >= since && appellant.is_none_or(|p| a.appellant == *p)
            })
            .map(|a| a.at)
            .collect();
        times.sort_unstable();
        Ok(times)
    }
}

impl MemoryStore {
    fn active_protections_where(
        &self,
        wiki: &WikiId,
        at: i64,
        keep: impl Fn(&ProtectionEvent) -> bool,
    ) -> Result<Vec<ProtectionEvent>> {
        let g = self.governance_lock();
        let of_wiki: Vec<ProtectionEvent> = g
            .protections
            .iter()
            .filter(|p| p.wiki == *wiki)
            .cloned()
            .collect();
        Ok(in_force(
            &of_wiki,
            at,
            |e| e.id.as_str(),
            |e| {
                (
                    e.event,
                    e.at,
                    e.expires_at,
                    e.of_event.as_ref().map(ProtectionEventId::as_str),
                )
            },
        )
        .into_iter()
        .filter(|p| keep(p))
        .cloned()
        .collect())
    }
}
