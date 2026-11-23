//! The acts: grant (and revoke) a rung, restrict, renew and lift a restriction.
//!
//! **The order of every act** is the order of every write: the wiki must be one the person may read (an absent wiki and one
//! they may not see are one answer, [`PeopleError::NotFound`]), they must be an account, the rules of the wiki must be
//! readable (a wiki that cannot read its rules refuses every write, C-3d), the act is **authorized** (which records the
//! decision with the rung and the block), and only then is the account the act is about looked up, so a person who may
//! not act learns nothing about who has an account. Then the pure rule (`check_grant`, `check_restrict`), the audit event of the
//! act, and the record.

use std::sync::Arc;

use inklkunn_store::{
    Account, AccountId, AccountStore, Actor, AuditEvent, AuditOutcome, AuditSink, GovernanceStore,
    GrantDecisionId, GrantDecisionRecord, LoginName, MeasureEvent, RestrictionEvent,
    RestrictionEventId, Role, RoleStore, StoreError, WikiRecord,
};

use super::DAY_SECS;
use super::rule::{
    GrantCheck, GrantRefusal, RestrictRefusal, check_grant, check_restrict, grants_at_all,
};
use super::text::{ReasonFault, clean_reason};
use crate::authorize::{
    Action, Authorizer, Holding, Principal, Resource, RestrictionNotice, entry_rung,
};
use crate::governance::{PlatformLimits, ReadFault, Rules, holds_only_allowed_characters};

/// Why an act was refused. Every variant is one plain fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeopleError {
    /// There is no such wiki, or the person may not see it: one answer for both (RFC 002 D-2).
    NotFound,
    /// The person may not do this: they are not an account, or their rung does not include it. Nothing says which rule
    /// they would have had to meet.
    NotAllowed,
    /// **The person is restricted**, and without the restriction they could have done this: the reason and the end.
    Restricted(RestrictionNotice),
    /// A governance file of this wiki cannot be read, so every write refuses until it is repaired (C-3d).
    RulesUnreadable(ReadFault),
    /// There is no account of that name, or that id.
    NoSuchAccount,
    /// The change of a rung is refused by [`check_grant`].
    Grant(GrantRefusal),
    /// The restriction is refused by [`check_restrict`], or its length is.
    Restrict(RestrictRefusal),
    /// The reason is refused.
    Reason(ReasonFault),
    /// The records changed while the act was being made (someone else got there first); look again.
    Changed,
    /// The facts or the records could not be had now (the text is for the operator): nothing was changed.
    Unavailable(String),
}

impl std::fmt::Display for PeopleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeopleError::NotFound => write!(f, "no such wiki"),
            PeopleError::NotAllowed => write!(f, "not allowed"),
            PeopleError::Restricted(n) => write!(f, "restricted until {}: {}", n.until, n.reason),
            PeopleError::RulesUnreadable(fault) => {
                write!(f, "the rules of this wiki cannot be read: {fault}")
            }
            PeopleError::NoSuchAccount => write!(f, "no such account"),
            PeopleError::Grant(why) => write!(f, "a rung is not changed: {why:?}"),
            PeopleError::Restrict(RestrictRefusal::DaysOutOfRange { limit_days }) => write!(
                f,
                "a restriction lasts from 1 day to {limit_days} days, the most INKLKUNN_GOVERNANCE_LIMIT_DAYS allows"
            ),
            PeopleError::Restrict(why) => write!(f, "not restricted: {why:?}"),
            PeopleError::Reason(why) => write!(f, "the reason is refused: {why:?}"),
            PeopleError::Changed => write!(f, "the records changed meanwhile"),
            PeopleError::Unavailable(why) => write!(f, "could not be done now: {why}"),
        }
    }
}

impl std::error::Error for PeopleError {}

fn unavailable(e: impl std::fmt::Display) -> PeopleError {
    PeopleError::Unavailable(e.to_string())
}

/// What a person typed to name an account, as the decision's trail keeps it: at most [`TYPED_IN_TRAIL_CHARS`] characters, with a
/// control or bidirectional character shown as `?`. (It is what they typed, whether or not it names an account.)
fn typed_for_trail(typed: &str) -> String {
    typed
        .trim()
        .chars()
        .take(TYPED_IN_TRAIL_CHARS)
        .map(|c| {
            if holds_only_allowed_characters(&c.to_string()) {
                c
            } else {
                '?'
            }
        })
        .collect()
}

/// The most characters of a typed name the trail keeps (the trail's own field is 200, and holds the rung and the block first).
const TYPED_IN_TRAIL_CHARS: usize = 100;

/// The one word the trail's *failed* event gives for why a record was not made.
fn failure_word(e: &PeopleError) -> &'static str {
    match e {
        PeopleError::Changed => "state-changed",
        PeopleError::Grant(GrantRefusal::LastAdmin) => "last-admin",
        PeopleError::Restrict(RestrictRefusal::AlreadyRestricted { .. }) => "already-restricted",
        PeopleError::NoSuchAccount => "no-such-account",
        _ => "store-error",
    }
}

fn store_failure(e: StoreError) -> PeopleError {
    match e {
        StoreError::StateChanged | StoreError::AlreadyRecorded => PeopleError::Changed,
        StoreError::NotFound | StoreError::AccountNotFound => PeopleError::NoSuchAccount,
        other => unavailable(other),
    }
}

/// A rung changed: what the decision says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantMade {
    /// The decision's record.
    pub id: GrantDecisionId,
    /// The account.
    pub account: AccountId,
    /// The rung it held through a grant before, if any.
    pub before: Option<Role>,
    /// The rung it holds through a grant now: `None` is a revocation.
    pub after: Option<Role>,
}

/// What was done to a restriction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestrictionOutcome {
    /// Put in place.
    Set,
    /// Put in place again, with a new reason and end.
    Renewed,
    /// Ended early.
    Lifted,
}

/// A restriction set, renewed or lifted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictionMade {
    /// The event's record.
    pub id: RestrictionEventId,
    /// The account.
    pub account: AccountId,
    /// What was done.
    pub outcome: RestrictionOutcome,
    /// When the restriction ends, for one that is in place.
    pub until: Option<i64>,
}

/// The people of the wikis: who holds which rung, and who is restricted.
#[derive(Clone)]
pub struct PeopleService {
    pub(super) authorizer: Authorizer,
    pub(super) governance: Arc<dyn GovernanceStore>,
    pub(super) roles: Arc<dyn RoleStore>,
    pub(super) accounts: Arc<dyn AccountStore>,
    audit: Arc<dyn AuditSink>,
    pub(super) limits: PlatformLimits,
}

impl std::fmt::Debug for PeopleService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PeopleService")
    }
}

/// What every act has settled before it looks at the account it is about.
struct Acting {
    account: AccountId,
    holding: Holding,
    rules: Arc<Rules>,
}

impl Acting {
    /// The rung the person holds now, after any restriction.
    fn rung(&self) -> Option<Role> {
        self.holding.rung
    }
}

impl PeopleService {
    /// Over the records of governance, the grants and the accounts, and an audit sink. The clock is the authorizer's
    /// (`Authorizer::with_clock`), so that a restriction is set and read by one clock. `limits` is the operator's
    /// `INKLKUNN_GOVERNANCE_LIMIT_DAYS`.
    pub fn new(
        authorizer: Authorizer,
        governance: Arc<dyn GovernanceStore>,
        roles: Arc<dyn RoleStore>,
        accounts: Arc<dyn AccountStore>,
        audit: Arc<dyn AuditSink>,
        limits: PlatformLimits,
    ) -> Self {
        PeopleService {
            authorizer,
            governance,
            roles,
            accounts,
            audit,
            limits,
        }
    }

    /// May the principal read the wiki? One answer for a wiki that is not there and one they may not see.
    pub(super) fn readable(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
    ) -> Result<(), PeopleError> {
        let resource = Resource::Wiki {
            wiki: wiki.id.clone(),
        };
        if self
            .authorizer
            .authorize(principal, Action::Read, &resource)
            .is_allowed()
        {
            Ok(())
        } else {
            Err(PeopleError::NotFound)
        }
    }

    fn begin(&self, principal: &Principal, wiki: &WikiRecord) -> Result<Acting, PeopleError> {
        self.readable(principal, wiki)?;
        let Some(account) = principal.account_id().cloned() else {
            return Err(PeopleError::NotAllowed);
        };
        let rules = self.authorizer.rules(&wiki.id).map_err(unavailable)?;
        if let Some(fault) = rules.fault() {
            return Err(PeopleError::RulesUnreadable(fault.clone()));
        }
        let holding = self
            .authorizer
            .holding(principal, &wiki.id)
            .map_err(unavailable)?;
        Ok(Acting {
            account,
            holding,
            rules,
        })
    }

    /// Authorize `action` on the wiki, **recording the decision** (rung and block, and the sign-in name or id the person
    /// typed to name whom the act is about), or say why not: the restriction when it is the reason, else *not allowed*.
    fn authorize(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        action: Action,
        typed: &str,
    ) -> Result<(), PeopleError> {
        let resource = Resource::Wiki {
            wiki: wiki.id.clone(),
        };
        if self
            .authorizer
            .authorize_noting(principal, action, &resource, Some(&typed_for_trail(typed)))
            .is_allowed()
        {
            return Ok(());
        }
        Err(self
            .authorizer
            .restriction_refusal(principal, action, &resource)
            .map_or(PeopleError::NotAllowed, PeopleError::Restricted))
    }

    /// The account a person means: by its id, or by its sign-in name (the two the operator's command line takes).
    fn find(&self, who: &str) -> Result<Account, PeopleError> {
        let who = who.trim();
        let by_id = AccountId::parse(who).map(|id| self.accounts.account(&id));
        let found = match by_id {
            Some(found) => found,
            None => match LoginName::parse(who) {
                Ok(login) => self.accounts.account_by_login(&login),
                Err(_) => return Err(PeopleError::NoSuchAccount),
            },
        };
        found
            .map_err(unavailable)?
            .ok_or(PeopleError::NoSuchAccount)
    }

    /// **The event of the act in the audit trail**, written before the record is: `rung=…; policy=…` as every decision
    /// is, and what the act changes. Nothing consequential happens unrecorded, so a trail that cannot be written refuses
    /// the act.
    fn audit_act(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        acting: &Acting,
        act: &str,
        account: &AccountId,
        detail: &str,
    ) -> Result<(), PeopleError> {
        let event = AuditEvent::new(
            self.authorizer.now(),
            &principal.audit_name(),
            act,
            &format!("member:{}:{}", wiki.id, account),
            AuditOutcome::Allowed,
            &format!(
                "rung={}; policy={}; {detail}",
                acting.rung().map_or("none", Role::as_str),
                acting.rules.short_block()
            ),
        );
        self.audit.record(&event).map_err(unavailable)
    }

    /// Make the record of an act whose event the trail already holds as *allowed*, and **if the record fails, say so in the
    /// trail** (F-4): a second event of the same act and resource, outcome *failed*, naming the reason in one word, so that the
    /// trail never names an act that did not happen. A trail that cannot take that event changes nothing: the act is refused
    /// whatever it says.
    fn recording<T>(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        acting: &Acting,
        act: &str,
        account: &AccountId,
        record: impl FnOnce() -> Result<T, PeopleError>,
    ) -> Result<T, PeopleError> {
        record().inspect_err(|e| {
            let event = AuditEvent::new(
                self.authorizer.now(),
                &principal.audit_name(),
                act,
                &format!("member:{}:{}", wiki.id, account),
                AuditOutcome::Failed,
                &format!(
                    "rung={}; policy={}; failed={}",
                    acting.rung().map_or("none", Role::as_str),
                    acting.rules.short_block(),
                    failure_word(e)
                ),
            );
            let _ = self.audit.record(&event);
        })
    }

    // ---------------------------------------------------------------------------------------------------- grants

    /// Change the rung `who` holds through a grant to `new` (`None`: take it away), with an optional reason.
    ///
    /// An admin acts as `admin`, a maintainer as `moderate`, and [`check_grant`] decides the rest.
    pub fn grant(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        who: &str,
        new: Option<Role>,
        reason: &str,
    ) -> Result<GrantMade, PeopleError> {
        let acting = self.begin(principal, wiki)?;
        let up_to = acting.rules.policy().rungs().grant_up_to();
        // A person who could never grant is told so before the act is decided: nothing to record about a person who
        // asks for what no rule gives them.
        let Some(rung) = acting.rung().filter(|r| grants_at_all(*r, up_to)) else {
            let capped = acting
                .holding
                .held()
                .is_some_and(|held| grants_at_all(held, up_to));
            return Err(match (&acting.holding.restriction, capped) {
                (Some(restriction), true) => PeopleError::Restricted(restriction.clone()),
                _ => PeopleError::NotAllowed,
            });
        };
        let action = if rung == Role::Admin {
            Action::Admin
        } else {
            Action::Moderate
        };
        self.authorize(principal, wiki, action, who)?;
        let reason = clean_reason(reason).map_err(PeopleError::Reason)?;
        let target = self.find(who)?;
        let current = self.roles.role(&target.id, &wiki.id).map_err(unavailable)?;
        let admins = self.other_admins_who_can_sign_in(wiki, &target.id)?;
        check_grant(&GrantCheck {
            actor: rung,
            own: target.id == acting.account,
            current,
            new,
            grant_up_to: up_to,
            other_admins_who_can_sign_in: admins,
        })
        .map_err(PeopleError::Grant)?;
        let act = if new.is_some() { "grant" } else { "revoke" };
        self.audit_act(
            principal,
            wiki,
            &acting,
            act,
            &target.id,
            &format!(
                "from={}; to={}",
                current.map_or("none", Role::as_str),
                new.map_or("none", Role::as_str)
            ),
        )?;
        self.recording(principal, wiki, &acting, act, &target.id, || {
            let id = GrantDecisionId::new();
            // The last-admin guard and the record are one step in the store (F-2): two admins acting on each other at once
            // cannot both leave.
            self.governance
                .record_grant_decision_keeping_an_admin(&GrantDecisionRecord {
                    id: id.clone(),
                    wiki: wiki.id.clone(),
                    at: self.authorizer.now(),
                    by: Some(Actor {
                        account: acting.account.clone(),
                        rung,
                    }),
                    policy_block: acting.rules.block().map(str::to_owned),
                    target: target.id.clone(),
                    before: current,
                    after: new,
                    reason,
                    appeal: None,
                })
                .map_err(|e| self.grant_refusal(e, wiki, &target.id, current))?;
            Ok(GrantMade {
                id,
                account: target.id.clone(),
                before: current,
                after: new,
            })
        })
    }

    /// Why the store refused the record of a change of a rung: **the last admin** when the account still holds the rung it
    /// held when the act began (so what refused it was the guard that counts admins, in the store's own step), otherwise that
    /// the records changed meanwhile.
    pub(super) fn grant_refusal(
        &self,
        e: StoreError,
        wiki: &WikiRecord,
        target: &AccountId,
        current: Option<Role>,
    ) -> PeopleError {
        match e {
            StoreError::StateChanged => match self.roles.role(target, &wiki.id) {
                Ok(now) if now == current => PeopleError::Grant(GrantRefusal::LastAdmin),
                _ => PeopleError::Changed,
            },
            other => store_failure(other),
        }
    }

    /// How many accounts other than `except` hold an admin grant in the wiki and can still sign in.
    fn other_admins_who_can_sign_in(
        &self,
        wiki: &WikiRecord,
        except: &AccountId,
    ) -> Result<usize, PeopleError> {
        let grants = self.roles.grants_in(&wiki.id).map_err(unavailable)?;
        let mut count = 0;
        for grant in grants
            .iter()
            .filter(|g| g.role == Role::Admin && g.account != *except)
        {
            let account = self.accounts.account(&grant.account).map_err(unavailable)?;
            if account.is_some_and(|a| !a.disabled) {
                count += 1;
            }
        }
        Ok(count)
    }

    // ---------------------------------------------------------------------------------------------- restrictions

    /// The rung the account holds in the wiki before any cap: the higher of its grant and the wiki's `entry`.
    fn held_by(
        &self,
        wiki: &WikiRecord,
        rules: &Rules,
        account: &AccountId,
    ) -> Result<Option<Role>, PeopleError> {
        let grant = self.roles.role(account, &wiki.id).map_err(unavailable)?;
        Ok(grant.max(entry_rung(wiki.visibility, rules)))
    }

    /// The restriction in force on the account now, if any.
    fn in_force(
        &self,
        wiki: &WikiRecord,
        account: &AccountId,
    ) -> Result<Option<RestrictionEvent>, PeopleError> {
        Ok(self
            .governance
            .active_restrictions(&wiki.id, account, self.authorizer.now())
            .map_err(unavailable)?
            .into_iter()
            .next())
    }

    /// The days of a restriction, as an end: from 1 day to the operator's limit.
    fn end_after(&self, days: u32) -> Result<i64, PeopleError> {
        let limit_days = self.limits.limit_days;
        if days == 0 || !self.limits.allows_days(days) {
            return Err(PeopleError::Restrict(RestrictRefusal::DaysOutOfRange {
                limit_days,
            }));
        }
        i64::from(days)
            .checked_mul(DAY_SECS)
            .and_then(|span| self.authorizer.now().checked_add(span))
            .ok_or(PeopleError::Restrict(RestrictRefusal::DaysOutOfRange {
                limit_days,
            }))
    }

    /// The reason of a restriction: required, one line, bounded.
    fn restriction_reason(reason: &str) -> Result<String, PeopleError> {
        clean_reason(reason)
            .map_err(PeopleError::Reason)?
            .ok_or(PeopleError::Reason(ReasonFault::Missing))
    }

    /// **Restrict** `who` in this wiki for `days` days, with a reason they are told: they may read, and may not comment,
    /// propose or review, until the end. Refused for an account that is restricted already (renew it), an admin, an
    /// account whose rung is not below the person's own, and oneself.
    pub fn restrict(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        who: &str,
        days: u32,
        reason: &str,
    ) -> Result<RestrictionMade, PeopleError> {
        self.set_or_renew(principal, wiki, who, days, reason, MeasureEvent::Set)
    }

    /// **Renew** the restriction in force on `who`: a new reason and a new end, from now. Refused when the account is not
    /// restricted.
    pub fn renew(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        who: &str,
        days: u32,
        reason: &str,
    ) -> Result<RestrictionMade, PeopleError> {
        self.set_or_renew(principal, wiki, who, days, reason, MeasureEvent::Renewed)
    }

    fn set_or_renew(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        who: &str,
        days: u32,
        reason: &str,
        event: MeasureEvent,
    ) -> Result<RestrictionMade, PeopleError> {
        let (acting, rung, target, active) = self.start_restriction(principal, wiki, who)?;
        let reason = Self::restriction_reason(reason)?;
        let ends = self.end_after(days)?;
        let (of_event, outcome, act) = match (event, active) {
            (MeasureEvent::Set, Some(active)) => {
                return Err(PeopleError::Restrict(RestrictRefusal::AlreadyRestricted {
                    until: active.expires_at.unwrap_or(i64::MAX),
                }));
            }
            (MeasureEvent::Set, None) => (None, RestrictionOutcome::Set, "restrict"),
            (_, Some(active)) => (
                Some(active.id),
                RestrictionOutcome::Renewed,
                "renew-restriction",
            ),
            (_, None) => return Err(PeopleError::Restrict(RestrictRefusal::NotRestricted)),
        };
        self.audit_act(
            principal,
            wiki,
            &acting,
            act,
            &target,
            &format!("days={days}"),
        )?;
        self.recording(principal, wiki, &acting, act, &target, || {
            let id = RestrictionEventId::new();
            let record = RestrictionEvent {
                id: id.clone(),
                wiki: wiki.id.clone(),
                at: self.authorizer.now(),
                event,
                account: target.clone(),
                by: Actor {
                    account: acting.account.clone(),
                    rung,
                },
                policy_block: acting.rules.block().map(str::to_owned),
                reason: Some(reason),
                expires_at: Some(ends),
                of_event,
                appeal: None,
            };
            // A restriction is set only where none is in force, and the check and the record are one step in the store (F-2):
            // two moderators at once do not both set one, and the one that is late is told by name.
            match event {
                MeasureEvent::Set => self
                    .governance
                    .record_restriction_event_if_none_in_force(&record)
                    .map_err(|e| self.already_restricted(e, wiki, &target))?,
                _ => self
                    .governance
                    .record_restriction_event(&record)
                    .map_err(store_failure)?,
            }
            Ok(RestrictionMade {
                id,
                account: target.clone(),
                outcome,
                until: Some(ends),
            })
        })
    }

    /// **Lift** the restriction in force on `who`, now, with an optional reason. Whoever may restrict an account may lift
    /// its restriction, and no one else.
    pub fn lift(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        who: &str,
        reason: &str,
    ) -> Result<RestrictionMade, PeopleError> {
        let (acting, rung, target, active) = self.start_restriction(principal, wiki, who)?;
        let reason = clean_reason(reason).map_err(PeopleError::Reason)?;
        let Some(active) = active else {
            return Err(PeopleError::Restrict(RestrictRefusal::NotRestricted));
        };
        self.audit_act(principal, wiki, &acting, "lift-restriction", &target, "")?;
        self.recording(
            principal,
            wiki,
            &acting,
            "lift-restriction",
            &target,
            || {
                let id = RestrictionEventId::new();
                // **Every** restriction in force on the person ends, not only the one that ends last: after *lifted* they are free.
                let others: Vec<RestrictionEvent> = self
                    .governance
                    .active_restrictions(&wiki.id, &target, self.authorizer.now())
                    .map_err(unavailable)?
                    .into_iter()
                    .filter(|r| r.id != active.id)
                    .collect();
                let lifting = |id: RestrictionEventId, of: RestrictionEventId| RestrictionEvent {
                    id,
                    wiki: wiki.id.clone(),
                    at: self.authorizer.now(),
                    event: MeasureEvent::Lifted,
                    account: target.clone(),
                    by: Actor {
                        account: acting.account.clone(),
                        rung,
                    },
                    policy_block: acting.rules.block().map(str::to_owned),
                    reason: reason.clone(),
                    expires_at: None,
                    of_event: Some(of),
                    appeal: None,
                };
                for other in others {
                    self.governance
                        .record_restriction_event(&lifting(RestrictionEventId::new(), other.id))
                        .map_err(store_failure)?;
                }
                self.governance
                    .record_restriction_event(&lifting(id.clone(), active.id.clone()))
                    .map_err(store_failure)?;
                Ok(RestrictionMade {
                    id,
                    account: target.clone(),
                    outcome: RestrictionOutcome::Lifted,
                    until: None,
                })
            },
        )
    }

    /// Why the store refused to set a restriction: someone set one first (named, with its end), or the store's own word.
    pub(super) fn already_restricted(
        &self,
        e: StoreError,
        wiki: &WikiRecord,
        target: &AccountId,
    ) -> PeopleError {
        match e {
            StoreError::StateChanged => match self.in_force(wiki, target) {
                Ok(Some(active)) => PeopleError::Restrict(RestrictRefusal::AlreadyRestricted {
                    until: active.expires_at.unwrap_or(i64::MAX),
                }),
                _ => PeopleError::Changed,
            },
            other => store_failure(other),
        }
    }

    /// What a restricting act settles first: the person's own standing, the authorization (`moderate`), the account, the
    /// rule over the two rungs, and the restriction in force. Returns the person's rung (after any restriction).
    fn start_restriction(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        who: &str,
    ) -> Result<(Acting, Role, AccountId, Option<RestrictionEvent>), PeopleError> {
        let acting = self.begin(principal, wiki)?;
        self.authorize(principal, wiki, Action::Moderate, who)?;
        // Allowed to moderate means a rung; the authorizer would have refused otherwise.
        let Some(rung) = acting.rung() else {
            return Err(PeopleError::NotAllowed);
        };
        let target = self.find(who)?;
        let held = self.held_by(wiki, &acting.rules, &target.id)?;
        check_restrict(
            rung,
            acting.rules.policy().rungs().moderate(),
            held,
            target.id == acting.account,
        )
        .map_err(PeopleError::Restrict)?;
        let active = self.in_force(wiki, &target.id)?;
        Ok((acting, rung, target.id, active))
    }
}
