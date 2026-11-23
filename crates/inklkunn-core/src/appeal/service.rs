//! The acts of appeals: **file** one, **decide** one (RFC 006 D-9; handoff G-T5). The pages' reads are `views`, what an action
//! is and who it affects is `find`, and what an overturn writes is `reverse`.

use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_store::{
    AccountId, AccountStore, AppealDecision, AppealId, AppealOutcome, AppealRecord, AppealState,
    AppealedKind, AuditEvent, AuditOutcome, AuditSink, GovernanceStore, ProposalId, ProposalStore,
    Reversal, Role, RoleStore, StoreError, WikiRecord,
};

use super::limits::AppealLimits;
use super::model::{ContestRef, Outcome};
use super::text::{TextFault, clean_text};
use crate::authorize::{Action, Authorizer, Holding, Principal, Resource, RestrictionNotice};
use crate::governance::{GrantUpTo, ReadFault, Rules};
use crate::people::{ReasonFault, clean_reason};
use crate::proposal::{QUOTA_DAY, QuotaLimit, QuotaRefusal, Reset, rolling_reset};
use crate::tombstone::Tombstones;

/// Why an act on an appeal was refused. Every variant is one plain fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppealError {
    /// There is no such wiki, or the person may not see it: one answer for both (RFC 002 D-2).
    NotFound,
    /// The person may not do this: they are not an account, or they do not hold the rung that decides appeals. Nothing says which.
    NotAllowed,
    /// The person holds the rung that decides appeals and is held down by a restriction: the reason and the end.
    Restricted(RestrictionNotice),
    /// A governance file of this wiki cannot be read, so every write refuses until it is repaired (C-3d).
    RulesUnreadable(ReadFault),
    /// There is no such action, or it is not about this person, or it is not in force any more: **one answer**, so that an appeal
    /// says nothing of the records of anyone else, and a person is not told of an action's life beyond what they could see.
    NotAffected,
    /// The person has appealed this action already and the appeal waits for a decision.
    AlreadyOpen {
        /// Their open appeal.
        appeal: AppealId,
    },
    /// The text is refused.
    Text(TextFault),
    /// A limit is reached ([`QuotaLimit::AppealsPerDayPerPrincipal`], [`QuotaLimit::OpenAppealsPerPerson`],
    /// [`QuotaLimit::OpenAppealsPerWiki`]): the limit by name, the
    /// numbers and when it ends.
    Quota(QuotaRefusal),
    /// There is no such appeal in this wiki.
    NoSuchAppeal,
    /// The appeal is decided already (by someone else, or by this person a moment ago): it is decided once.
    AlreadyDecided,
    /// The person may not decide this appeal.
    Decider(DeciderRefusal),
    /// The decision's reason is refused (it is required).
    Reason(ReasonFault),
    /// The action cannot be reversed now, and why: nothing was decided.
    Overturn(OverturnRefusal),
    /// The records changed while the act was being made (someone else got there first); look again.
    Changed,
    /// The facts or the records could not be had now (the text is for the operator): nothing was changed.
    Unavailable(String),
}

/// Why a person who holds the rung that decides appeals may not decide **this** one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeciderRefusal {
    /// They are the appellant: nobody decides their own appeal.
    TheAppellant,
    /// They took the action that is appealed: nobody decides an appeal against their own act.
    TheActor,
}

/// Why an overturn is refused: **later changes made it moot**, or the decider's reach does not allow it. The appeal is still open
/// and can be upheld; nothing was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverturnRefusal {
    /// The restriction has ended or been lifted since: there is nothing to reverse.
    RestrictionEnded,
    /// The protection has ended or been lifted since.
    ProtectionEnded,
    /// The person's role is not the one the decision gave them any more: a later decision changed it, and restoring the earlier
    /// one would undo that.
    RoleChanged,
    /// Restoring the earlier role is above what the decider may grant (a maintainer's reach is `rungs.grant_up_to`): an admin
    /// decides this one.
    AboveReach {
        /// What the wiki's rules let a maintainer grant.
        up_to: GrantUpTo,
    },
    /// The proposal is not as the decline left it (it is not declined any more, or a later step followed): reopening it would
    /// undo something else.
    ProposalMovedOn,
    /// The proposal's latest revision is still hidden by moderation, so a proposal nobody can read would be reopened: the
    /// hiding is what to overturn (or the moderators to unhide).
    StillHidden,
    /// The hiding is not in force any more.
    HidingEnded,
}

impl std::fmt::Display for AppealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppealError::NotFound => write!(f, "no such wiki"),
            AppealError::NotAllowed => write!(f, "not allowed"),
            AppealError::Restricted(n) => write!(f, "restricted until {}: {}", n.until, n.reason),
            AppealError::RulesUnreadable(fault) => {
                write!(f, "the rules of this wiki cannot be read: {fault}")
            }
            AppealError::NotAffected => write!(f, "no such action is about you"),
            AppealError::AlreadyOpen { appeal } => write!(f, "appeal {appeal} is open already"),
            AppealError::Text(why) => write!(f, "the appeal's text is refused: {why:?}"),
            AppealError::Quota(q) => write!(f, "limit reached: {}", q.limit.name()),
            AppealError::NoSuchAppeal => write!(f, "no such appeal"),
            AppealError::AlreadyDecided => write!(f, "decided already"),
            AppealError::Decider(why) => write!(f, "you may not decide this appeal: {why:?}"),
            AppealError::Reason(why) => write!(f, "the reason is refused: {why:?}"),
            AppealError::Overturn(why) => write!(f, "the action cannot be reversed: {why:?}"),
            AppealError::Changed => write!(f, "the records changed meanwhile"),
            AppealError::Unavailable(why) => write!(f, "could not be done now: {why}"),
        }
    }
}

impl std::error::Error for AppealError {}

pub(super) fn unavailable(e: impl std::fmt::Display) -> AppealError {
    AppealError::Unavailable(e.to_string())
}

/// A store's refusal of a record, as the person is told it.
pub(super) fn store_failure(e: StoreError) -> AppealError {
    match e {
        StoreError::StateChanged | StoreError::AlreadyRecorded => AppealError::Changed,
        StoreError::NotFound => AppealError::NoSuchAppeal,
        other => unavailable(other),
    }
}

/// The one word the trail's *failed* event gives for why a record was not made.
fn failure_word(e: &AppealError) -> &'static str {
    match e {
        AppealError::Changed | AppealError::Overturn(OverturnRefusal::ProposalMovedOn) => {
            "state-changed"
        }
        AppealError::AlreadyOpen { .. } => "already-open",
        AppealError::AlreadyDecided => "already-decided",
        AppealError::NoSuchAppeal => "no-such-appeal",
        _ => "store-error",
    }
}

/// An appeal that was filed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealFiled {
    /// The appeal's record.
    pub id: AppealId,
    /// What it is against.
    pub kind: AppealedKind,
}

/// What a decision came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverturnMade {
    /// Upheld or overturned.
    pub outcome: Outcome,
    /// The proposal the decision brought back to review, when it did (an overturned decline, or an overturned hiding whose act
    /// had declined it).
    pub reopened: Option<ProposalId>,
}

/// What every act has settled before it looks at the appeal.
pub(super) struct Acting {
    pub(super) account: AccountId,
    pub(super) holding: Holding,
    pub(super) rules: Arc<Rules>,
}

impl Acting {
    /// The rung the person holds now, after any restriction.
    pub(super) fn rung(&self) -> Option<Role> {
        self.holding.rung
    }
}

/// The appeals of the wikis.
#[derive(Clone)]
pub struct AppealService {
    pub(super) authorizer: Authorizer,
    pub(super) governance: Arc<dyn GovernanceStore>,
    pub(super) proposals: Arc<dyn ProposalStore>,
    pub(super) roles: Arc<dyn RoleStore>,
    pub(super) accounts: Arc<dyn AccountStore>,
    pub(super) tombstones: Arc<Tombstones>,
    audit: Arc<dyn AuditSink>,
    pub(super) limits: AppealLimits,
    /// Filing is one step: the quotas are read, and the record is made, under this lock.
    admission: Arc<Mutex<()>>,
    /// The decisions on a decline and on a hiding go one at a time, so that the checks of a decision (the proposal's log, the hides
    /// in force) are read before another decision writes. **It is not what keeps the records whole**: the decision and every record of
    /// its reversal are one transaction of the store (A-1), which a moderator's unhide, taking no lock, cannot split.
    pub(super) deciding: Arc<Mutex<()>>,
}

impl std::fmt::Debug for AppealService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppealService")
    }
}

impl AppealService {
    /// Over the records of governance, the proposals, the grants and the accounts, what is hidden, and an audit sink. The
    /// clock is the authorizer's (`Authorizer::with_clock`), so that a restriction is read and an appeal filed by one clock.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        authorizer: Authorizer,
        governance: Arc<dyn GovernanceStore>,
        proposals: Arc<dyn ProposalStore>,
        roles: Arc<dyn RoleStore>,
        accounts: Arc<dyn AccountStore>,
        tombstones: Arc<Tombstones>,
        audit: Arc<dyn AuditSink>,
        limits: AppealLimits,
    ) -> Self {
        AppealService {
            authorizer,
            governance,
            proposals,
            roles,
            accounts,
            tombstones,
            audit,
            limits,
            admission: Arc::new(Mutex::new(())),
            deciding: Arc::new(Mutex::new(())),
        }
    }

    /// The limits in force.
    pub fn limits(&self) -> AppealLimits {
        self.limits
    }

    /// May the principal read the wiki? One answer for a wiki that is not there and one they may not see.
    pub(super) fn readable(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
    ) -> Result<(), AppealError> {
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
            Err(AppealError::NotFound)
        }
    }

    /// What every act settles first: the wiki is one the person may read, they are an account, and the wiki's rules can be read.
    pub(super) fn begin(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
    ) -> Result<Acting, AppealError> {
        self.readable(principal, wiki)?;
        let Some(account) = principal.account_id().cloned() else {
            return Err(AppealError::NotAllowed);
        };
        let rules = self.authorizer.rules(&wiki.id).map_err(unavailable)?;
        if let Some(fault) = rules.fault() {
            return Err(AppealError::RulesUnreadable(fault.clone()));
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

    /// The name a person is shown by.
    pub(super) fn name_of(&self, account: &AccountId) -> String {
        match self.accounts.account(account) {
            Ok(Some(found)) => found.display_name.as_str().to_owned(),
            _ => account.to_string(),
        }
    }

    /// **The event of the act in the audit trail** (`rung=…; policy=…; <what>`): written before the record is. Never a name and
    /// never the text. A trail that cannot be written refuses the act.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn audit_act(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        acting: &Acting,
        act: &str,
        appeal: &AppealId,
        outcome: AuditOutcome,
        detail: &str,
    ) -> Result<(), AppealError> {
        let event = AuditEvent::new(
            self.authorizer.now(),
            &principal.audit_name(),
            act,
            &format!("appeal:{}:{}", wiki.id, appeal),
            outcome,
            &format!(
                "rung={}; policy={}; {detail}",
                acting.rung().map_or("none", Role::as_str),
                acting.rules.short_block()
            ),
        );
        self.audit.record(&event).map_err(unavailable)
    }

    /// Make the record of an act whose event the trail already holds as *allowed*, and **if the record fails, say so in the
    /// trail** (F-4): the same act and resource, outcome *failed*, the reason in one word.
    pub(super) fn recording<T>(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        acting: &Acting,
        act: &str,
        appeal: &AppealId,
        record: impl FnOnce() -> Result<T, AppealError>,
    ) -> Result<T, AppealError> {
        record().inspect_err(|e| {
            let _ = self.audit_act(
                principal,
                wiki,
                acting,
                act,
                appeal,
                AuditOutcome::Failed,
                &format!("failed={}", failure_word(e)),
            );
        })
    }

    // ------------------------------------------------------------------------------------------------- filing

    /// **File an appeal** against the action `what`, in the person's own words. The person must be an account that may read the
    /// wiki and **the person the action affects** (never capped by a restriction: see
    /// [`Authorizer::authorize_appeal_filing`]); the action must be in force; they may have one appeal open against it; and the
    /// quotas hold.
    pub fn file(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        what: &ContestRef,
        text: &str,
    ) -> Result<AppealFiled, AppealError> {
        let acting = self.begin(principal, wiki)?;
        if !self
            .authorizer
            .authorize_appeal_filing(principal, &wiki.id)
            .is_allowed()
        {
            return Err(AppealError::NotAllowed);
        }
        let action = what.action();
        // Whether the action exists, is in force and is about this person is **one question**: a stranger is told what anyone
        // is told of an action that is not there.
        let found = self.find(wiki, &action)?;
        if !found
            .as_ref()
            .map_or(Ok(false), |f| self.affects(wiki, f, &acting.account))?
        {
            return Err(AppealError::NotAffected);
        }
        let text = clean_text(text).map_err(AppealError::Text)?;
        let id = AppealId::new();
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let existing = self
            .governance
            .appeals_about(&wiki.id, &action)
            .map_err(unavailable)?
            .into_iter()
            .find(|a| a.appellant == acting.account && a.state == AppealState::Open);
        if let Some(open) = existing {
            return Err(AppealError::AlreadyOpen { appeal: open.id });
        }
        self.check_quotas(wiki, &acting.account)?;
        let detail = format!("filing; kind={}", action.kind.as_str());
        self.audit_act(
            principal,
            wiki,
            &acting,
            "appeal-filed",
            &id,
            AuditOutcome::Allowed,
            &detail,
        )?;
        self.recording(principal, wiki, &acting, "appeal-filed", &id, || {
            self.governance
                .file_appeal(&AppealRecord {
                    id: id.clone(),
                    wiki: wiki.id.clone(),
                    at: self.authorizer.now(),
                    action: action.clone(),
                    action_actor: found.as_ref().and_then(|f| f.actor.clone()),
                    appellant: acting.account.clone(),
                    text,
                    state: AppealState::Open,
                })
                .map_err(|e| match e {
                    // Another appeal of this person's against this action was filed first.
                    StoreError::AlreadyRecorded => self
                        .governance
                        .appeals_about(&wiki.id, &action)
                        .ok()
                        .and_then(|all| {
                            all.into_iter().find(|a| {
                                a.appellant == acting.account && a.state == AppealState::Open
                            })
                        })
                        .map_or(AppealError::Changed, |open| AppealError::AlreadyOpen {
                            appeal: open.id,
                        }),
                    other => store_failure(other),
                })
        })?;
        Ok(AppealFiled {
            id,
            kind: action.kind,
        })
    }

    /// The quotas of RFC 004 D-11 for appeals, read from the records: the person's rate in a rolling day, and the wiki's bound on
    /// open appeals. The refusal names the limit.
    fn check_quotas(&self, wiki: &WikiRecord, appellant: &AccountId) -> Result<(), AppealError> {
        let now = self.authorizer.now();
        let since = now - QUOTA_DAY + 1;
        let allowed = self.limits.per_person_per_day;
        let mine = self
            .governance
            .appeals_since(&wiki.id, Some(appellant), since)
            .map_err(unavailable)?;
        if let Some(reset) = rolling_reset(&mine, allowed, QUOTA_DAY) {
            return Err(AppealError::Quota(QuotaRefusal {
                limit: QuotaLimit::AppealsPerDayPerPrincipal,
                allowed,
                current: u64::try_from(mine.len()).unwrap_or(u64::MAX),
                asked: 1,
                reset,
            }));
        }
        // A person's own open appeals: a few accounts cannot fill the wiki's.
        let own_bound = self.limits.open_per_person;
        let own_open = self
            .governance
            .open_appeal_count(&wiki.id, appellant)
            .map_err(unavailable)?;
        if own_open >= own_bound {
            return Err(AppealError::Quota(QuotaRefusal {
                limit: QuotaLimit::OpenAppealsPerPerson,
                allowed: own_bound,
                current: own_open,
                asked: 1,
                reset: Reset::WhenOneOfYoursIsDecided,
            }));
        }
        let bound = self.limits.open_per_wiki;
        let open = self
            .governance
            .open_appeals(&wiki.id, inklkunn_store::governance::MAX_PAGE)
            .map_err(unavailable)?;
        if u64::try_from(open.len()).unwrap_or(u64::MAX) >= bound {
            // Like the proposals' per-wiki limit, it does not say how many there are: the count shown is the limit itself.
            return Err(AppealError::Quota(QuotaRefusal {
                limit: QuotaLimit::OpenAppealsPerWiki,
                allowed: bound,
                current: bound,
                asked: 1,
                reset: Reset::WhenAnAppealIsDecided,
            }));
        }
        Ok(())
    }

    // ----------------------------------------------------------------------------------------------- deciding

    /// A refusal made while the appeal was being decided, **unless the appeal was decided in the meantime by someone else**: then the
    /// person is told that (what they lost to), not that the action it was about is gone (their rival's overturn ended it).
    fn or_decided_meanwhile(&self, appeal: &AppealId, refusal: AppealError) -> AppealError {
        match self.governance.appeal(appeal) {
            Ok(Some(now)) if now.state != AppealState::Open => AppealError::AlreadyDecided,
            _ => refusal,
        }
    }

    /// **Decide** an open appeal, once: *upheld* (the action stands) or *overturned* (the action is reversed, in the same step,
    /// as its own record), with a reason. By a person who holds the rung `rungs.decide_appeal` names, who is **not the appellant
    /// and did not take the action**: each refused by name.
    pub fn decide(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        appeal: &AppealId,
        outcome: Outcome,
        reason: &str,
    ) -> Result<OverturnMade, AppealError> {
        let acting = self.begin(principal, wiki)?;
        if !self
            .authorizer
            .authorize_appeal_decision(principal, &wiki.id)
            .is_allowed()
        {
            let decides = acting.rules.policy().rungs().decide_appeal().role();
            return Err(match &acting.holding.restriction {
                Some(restriction) if acting.holding.held().is_some_and(|held| held >= decides) => {
                    AppealError::Restricted(restriction.clone())
                }
                _ => AppealError::NotAllowed,
            });
        }
        let Some(rung) = acting.rung() else {
            return Err(AppealError::NotAllowed);
        };
        let record = self
            .governance
            .appeal(appeal)
            .map_err(unavailable)?
            .filter(|a| a.wiki == wiki.id)
            .ok_or(AppealError::NoSuchAppeal)?;
        if record.state != AppealState::Open {
            return Err(AppealError::AlreadyDecided);
        }
        if record.appellant == acting.account {
            return Err(AppealError::Decider(DeciderRefusal::TheAppellant));
        }
        if record.action_actor.as_ref() == Some(&acting.account)
            || self
                .actors_of(wiki, &record.action)?
                .contains(&acting.account)
        {
            return Err(AppealError::Decider(DeciderRefusal::TheActor));
        }
        let reason = clean_reason(reason)
            .map_err(AppealError::Reason)?
            .ok_or(AppealError::Reason(ReasonFault::Missing))?;
        // Every decision on a decline or a hiding, upheld or overturned, goes one at a time, so that what is checked is what the next
        // decision finds. Every check that can refuse is made before the first record is written, and the records are written in
        // one transaction.
        let serial = matches!(
            record.action.kind,
            AppealedKind::Decline | AppealedKind::Tombstone
        )
        .then(|| self.deciding.lock().unwrap_or_else(PoisonError::into_inner));
        if serial.is_some() {
            // Decided meanwhile, by the person who held the lock before.
            let now = self
                .governance
                .appeal(appeal)
                .map_err(unavailable)?
                .ok_or(AppealError::NoSuchAppeal)?;
            if now.state != AppealState::Open {
                return Err(AppealError::AlreadyDecided);
            }
        }
        let reversal = match outcome {
            Outcome::Upheld => None,
            Outcome::Overturned => Some(
                self.prepare(wiki, &acting, rung, &record, &reason)
                    .map_err(|e| self.or_decided_meanwhile(appeal, e))?,
            ),
        };
        let detail = format!(
            "decision; kind={}; outcome={}",
            record.action.kind.as_str(),
            match outcome {
                Outcome::Upheld => "upheld",
                Outcome::Overturned => "overturned",
            }
        );
        self.audit_act(
            principal,
            wiki,
            &acting,
            "appeal-decided",
            appeal,
            AuditOutcome::Allowed,
            &detail,
        )?;
        self.recording(principal, wiki, &acting, "appeal-decided", appeal, || {
            // The proposal an overturn brings back to review: the reopening is written by the store, in the decision's own
            // transaction, with the rest of the reversal (A-1).
            let reopened = reversal
                .as_ref()
                .and_then(Reversal::reopening)
                .map(|change| change.transition.proposal.clone());
            let outcome_record = match reversal {
                None => AppealOutcome::Upheld,
                Some(reversal) => AppealOutcome::Overturned(Box::new(reversal)),
            };
            self.governance
                .decide_appeal(
                    appeal,
                    &AppealDecision {
                        at: self.authorizer.now(),
                        by: acting.account.clone(),
                        reason: reason.clone(),
                        outcome: outcome_record,
                    },
                )
                .map_err(|e| match e {
                    // Decided in the meantime by someone else (or the records moved): which one, the record says.
                    StoreError::StateChanged => match self.governance.appeal(appeal) {
                        Ok(Some(now)) if now.state != AppealState::Open => {
                            AppealError::AlreadyDecided
                        }
                        // The one other record of an overturned decline is the proposal's reopening: the proposal is not as the
                        // decline left it any more, and nothing was written.
                        _ if record.action.kind == AppealedKind::Decline => {
                            AppealError::Overturn(OverturnRefusal::ProposalMovedOn)
                        }
                        _ => AppealError::Changed,
                    },
                    other => store_failure(other),
                })?;
            Ok(OverturnMade { outcome, reopened })
        })
    }
}
