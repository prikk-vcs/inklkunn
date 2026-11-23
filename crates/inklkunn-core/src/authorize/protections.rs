//! Protections beside `authorize()` (RFC 006 D-7; handoff G-T3; threat model C-3g): where the temporary ones are kept, how the
//! pages read them, and the three acts of the `moderate` rung that set, renew and lift one.
//!
//! **What is decided where.** The rung a protection asks of the acceptor is enforced by the acceptance's `judge`, and nowhere
//! else (handoff G-T1 correction F-3: the product authorizes *accept* on a proposal, never on a page, so a check inside
//! `authorize()` would never be reached). The judge asks [`Authorizer::protections_for_change`] about every page a change touches,
//! under the rules and the clock of its own act, before the lease and under it. The review a protection asks is a fact of one
//! proposal (its approvals), so the acceptance counts it, as it counts `acceptance.approvals`, with the notices this module hands
//! it. A protection limits **acceptance only**. **Nothing here is a template's or a handler's**: the web layer only draws the
//! notices.
//!
//! **Fails closed.** Where a protection cannot be read (the store is down), the acceptance is refused. The display shows what
//! can be read and says nothing of the rest, because it decides nothing.
//!
//! **Existence-neutral.** [`Authorizer::protections_on`] answers only a principal who may read the wiki; to anyone else, and
//! for a wiki that is not there, it answers with none, in the same cost, as every other read does.

use std::sync::Arc;

use inklkunn_markup::title_to_path;
use inklkunn_store::{
    Actor, AuditEvent, AuditOutcome, GovernanceStore, MeasureEvent, ProtectionEvent,
    ProtectionEventId, ProtectionKind, ProtectionRequirement, StoreError, WikiId,
};

use super::{
    Action, Authorizer, Principal, Resource, SourceError, authority_detail, decide_under, rung_of,
};
use crate::governance::{
    PlatformLimits, Require, Rules, Target, is_canonical_title, is_valid_reason,
};
use crate::protection::{Notice, notices_for, titles_of};

/// Where an [`Authorizer`] keeps and reads temporary protections: the governance store, and the platform's limit on how long
/// one may last.
#[derive(Clone)]
pub(super) struct Book {
    store: Arc<dyn GovernanceStore>,
    limits: PlatformLimits,
}

const DAY_SECS: i64 = 86_400;

/// What the `moderate` rung asks to be put in place: a protection of a page or a namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// What it protects, by title: a page or a namespace.
    pub target: Target,
    /// What it requires of an acceptance.
    pub require: Require,
    /// Why (one line, at most [`MAX_REASON_CHARS`](crate::governance::MAX_REASON_CHARS) characters): shown on the page, in the
    /// proposal form and on every refusal it causes.
    pub reason: String,
    /// For how many days (of 24 hours) from now, from 1 to the platform's limit (`INKLKUNN_GOVERNANCE_LIMIT_DAYS`).
    pub days: u32,
}

/// Why the `moderate` rung's act on a protection was not done. Nothing was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectError {
    /// The principal may not moderate here, the wiki is not there, or the rules of the wiki refuse every write now: one
    /// answer for all, as everywhere.
    NotAllowed,
    /// This server keeps no temporary protections (a build or a test without the store).
    NotKept,
    /// The title is not a page's or a namespace's title in the one form a title takes.
    BadTarget,
    /// The reason is empty, longer than the limit, or holds a character a text of the rules may not.
    BadReason,
    /// An expiry of no days: a protection ends after it begins.
    NoDays,
    /// An expiry longer than the platform allows: the limit, in days (`INKLKUNN_GOVERNANCE_LIMIT_DAYS`).
    TooLong {
        /// The platform's limit.
        limit_days: u32,
    },
    /// A temporary protection of this page or namespace is in force: renew it, or lift it first.
    AlreadyInForce,
    /// No temporary protection of this page or namespace is in force: there is nothing to renew or to lift.
    NotInForce,
    /// The store failed (the text is for operators).
    Store(String),
}

impl std::fmt::Display for ProtectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtectError::NotAllowed => f.write_str("not allowed"),
            ProtectError::NotKept => f.write_str("this server keeps no temporary protections"),
            ProtectError::BadTarget => f.write_str("not a page or namespace title"),
            ProtectError::BadReason => f.write_str("the reason is not usable"),
            ProtectError::NoDays => f.write_str("a protection lasts at least one day"),
            ProtectError::TooLong { limit_days } => write!(
                f,
                "a protection lasts at most {limit_days} days (INKLKUNN_GOVERNANCE_LIMIT_DAYS)"
            ),
            ProtectError::AlreadyInForce => {
                f.write_str("a temporary protection is in force there already")
            }
            ProtectError::NotInForce => f.write_str("no temporary protection is in force there"),
            ProtectError::Store(why) => write!(f, "the protection could not be recorded: {why}"),
        }
    }
}

impl std::error::Error for ProtectError {}

impl From<StoreError> for ProtectError {
    fn from(e: StoreError) -> Self {
        ProtectError::Store(e.to_string())
    }
}

/// The three acts on a temporary protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Set,
    Renew,
    Lift,
}

impl Act {
    /// The operation the trail names.
    const fn operation(self) -> &'static str {
        match self {
            Act::Set => "protection-set",
            Act::Renew => "protection-renewed",
            Act::Lift => "protection-lifted",
        }
    }

    const fn event(self) -> MeasureEvent {
        match self {
            Act::Set => MeasureEvent::Set,
            Act::Renew => MeasureEvent::Renewed,
            Act::Lift => MeasureEvent::Lifted,
        }
    }
}

fn kind_of(target: &Target) -> ProtectionKind {
    match target {
        Target::Path(_) => ProtectionKind::Page,
        Target::Namespace(_) => ProtectionKind::Namespace,
    }
}

impl Authorizer {
    /// The same, **keeping and reading temporary protections** in `store`, none of which may last longer than
    /// `limits.limit_days`. Without this an authorizer knows the policy's standing protections and no others.
    #[must_use]
    pub fn with_protections(
        mut self,
        store: Arc<dyn GovernanceStore>,
        limits: PlatformLimits,
    ) -> Self {
        self.protections = Some(Book { store, limits });
        self
    }

    /// The temporary protections of the wiki that are in force at `at`, as the store holds them (empty for an authorizer that
    /// keeps none).
    fn temporary_protections(
        &self,
        wiki: &WikiId,
        at: i64,
    ) -> Result<Vec<ProtectionEvent>, SourceError> {
        match &self.protections {
            Some(book) => book
                .store
                .active_protections(wiki, at)
                .map_err(|e| SourceError(e.to_string())),
            None => Ok(Vec::new()),
        }
    }

    /// The protections in force at `at` on the pages these repository `paths` are (a change's: what it writes, removes or
    /// renames to or from), under `rules` and the temporary ones read now: standing first, then temporary, each once.
    /// **Never cached**: the temporary ones are asked of the store, and `rules` are the caller's (under the lease, those of the
    /// tip the change is signed on).
    pub(crate) fn protections_for_change(
        &self,
        wiki: &WikiId,
        paths: &[String],
        rules: &Rules,
        at: i64,
    ) -> Result<Vec<Notice>, SourceError> {
        let titles = titles_of(paths.iter().map(String::as_str));
        self.protections_for_titles(wiki, &titles, rules, at)
    }

    fn protections_for_titles(
        &self,
        wiki: &WikiId,
        titles: &[String],
        rules: &Rules,
        at: i64,
    ) -> Result<Vec<Notice>, SourceError> {
        if titles.is_empty() {
            return Ok(Vec::new());
        }
        let temporary = self.temporary_protections(wiki, at)?;
        Ok(notices_for(rules.policy(), &temporary, titles, at))
    }

    /// The protections `principal` is told of on the page titled `title`: **one display for both kinds** (the page, the
    /// proposal form and every refusal show these, once each). **Only to someone who may read the wiki**: anyone else, and a
    /// wiki that is not there, gets none, as for every read. A protection that cannot be read now is left out (this decides
    /// nothing; the decision fails closed).
    pub fn protections_on(&self, principal: &Principal, wiki: &WikiId, title: &str) -> Vec<Notice> {
        self.visible_protections(principal, wiki, &[title.to_owned()])
    }

    /// [`Authorizer::protections_on`] for a change: the protections on every page the repository `paths` are (a rename's two
    /// paths both), for the proposal's page and its form.
    pub fn protections_on_change(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        paths: &[String],
    ) -> Vec<Notice> {
        self.visible_protections(
            principal,
            wiki,
            &titles_of(paths.iter().map(String::as_str)),
        )
    }

    fn visible_protections(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        titles: &[String],
    ) -> Vec<Notice> {
        let Ok(facts) = self.facts(principal, Action::Read, wiki) else {
            return Vec::new();
        };
        let reads = decide_under(facts.snapshot, &facts.rules, principal, Action::Read, false);
        if !reads.is_allowed() {
            return Vec::new();
        }
        let Ok(rules) = self.source.rules(wiki) else {
            return Vec::new();
        };
        self.protections_for_titles(wiki, titles, &rules, (self.clock)())
            .unwrap_or_default()
    }

    /// Would a request to protect a page be heard? **A hint for what to show** (the link on a page, the form), decided as the
    /// act will be and recorded nowhere: the act itself ([`Authorizer::protect`]) authorizes again, in the trail, and is the one
    /// that counts. False for a server that keeps no temporary protections.
    pub fn may_moderate(&self, principal: &Principal, wiki: &WikiId) -> bool {
        self.protections.is_some()
            && self
                .facts(principal, Action::Moderate, wiki)
                .is_ok_and(|facts| {
                    decide_under(
                        facts.snapshot,
                        &facts.rules,
                        principal,
                        Action::Moderate,
                        false,
                    )
                    .is_allowed()
                })
    }

    /// The most days a temporary protection may last here (the platform's limit), or `None` for a server that keeps none.
    pub fn protection_limit_days(&self) -> Option<u32> {
        self.protections.as_ref().map(|book| book.limits.limit_days)
    }

    /// The temporary protection in force **exactly on** this page or namespace, if there is one and `principal` may read the
    /// wiki: what a renewal or a lifting is of. (Which protections *apply* to a page is [`Authorizer::protections_on`].)
    pub fn temporary_on(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        target: &Target,
    ) -> Option<Notice> {
        let book = self.protections.as_ref()?;
        let facts = self.facts(principal, Action::Read, wiki).ok()?;
        decide_under(facts.snapshot, &facts.rules, principal, Action::Read, false)
            .is_allowed()
            .then_some(())?;
        let now = (self.clock)();
        book.store
            .active_protections_on(wiki, kind_of(target), target.title(), now)
            .ok()?
            .iter()
            .find_map(|event| Notice::of_record(event, now))
    }

    // ------------------------------------------------------------------------------------------- the acts

    /// **Put a temporary protection in place** on a page or a namespace, as the `moderate` rung (RFC 006 D-7). It requires a
    /// review of every proposal there or restricts acceptance there to a rung, for a reason and a number of days within the
    /// platform's limit. Recorded, append-only, with the actor, their rung and the block of the rules; audited as
    /// `protection-set`. It **ends by itself**, `days` times 24 hours from now, and writes nothing to end.
    pub fn protect(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        setting: &Setting,
    ) -> Result<ProtectionEventId, ProtectError> {
        self.act(
            principal,
            wiki,
            &setting.target,
            Act::Set,
            Some(setting),
            None,
        )
    }

    /// **Renew** the temporary protection in force on the page or namespace: a new requirement, reason and end, as a new
    /// record that follows the one in force (renewing is a new action). Audited as `protection-renewed`.
    pub fn renew_protection(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        setting: &Setting,
    ) -> Result<ProtectionEventId, ProtectError> {
        self.act(
            principal,
            wiki,
            &setting.target,
            Act::Renew,
            Some(setting),
            None,
        )
    }

    /// **Lift** the temporary protection in force on the page or namespace before its end, with a reason when one is given
    /// (the same bounds as any reason). Lifting a standing protection is a change to the policy, by proposal. Audited as
    /// `protection-lifted`.
    pub fn lift_protection(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        target: &Target,
        reason: Option<&str>,
    ) -> Result<ProtectionEventId, ProtectError> {
        self.act(principal, wiki, target, Act::Lift, None, reason)
    }

    fn act(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        target: &Target,
        act: Act,
        setting: Option<&Setting>,
        lift_reason: Option<&str>,
    ) -> Result<ProtectionEventId, ProtectError> {
        let title = target.title();
        // The shape of a title is a fact of the input alone, so it is said before anything of the wiki is asked.
        if !is_canonical_title(title) {
            return Err(ProtectError::BadTarget);
        }
        let resource = match target {
            Target::Path(title) => Resource::Page {
                wiki: wiki.clone(),
                path: title_to_path(title).map_err(|_| ProtectError::BadTarget)?,
            },
            Target::Namespace(prefix) => Resource::Namespace {
                wiki: wiki.clone(),
                prefix: prefix.clone(),
            },
        };
        // The `moderate` rung, under the rules of the wiki; audited whatever the answer. Nothing else is asked first, so that
        // a person without the rung learns nothing of what is protected.
        if !self
            .authorize(principal, Action::Moderate, &resource)
            .is_allowed()
        {
            return Err(ProtectError::NotAllowed);
        }
        let book = self.protections.as_ref().ok_or(ProtectError::NotKept)?;
        let account = principal
            .account_id()
            .ok_or(ProtectError::NotAllowed)?
            .clone();
        let now = (self.clock)();
        let (requirement, reason, expires_at) = match setting {
            Some(setting) => {
                if !is_valid_reason(&setting.reason) {
                    return Err(ProtectError::BadReason);
                }
                if setting.days == 0 {
                    return Err(ProtectError::NoDays);
                }
                if !book.limits.allows_days(setting.days) {
                    return Err(ProtectError::TooLong {
                        limit_days: book.limits.limit_days,
                    });
                }
                let requirement = match setting.require {
                    Require::Review => ProtectionRequirement::Review,
                    Require::Rung(rung) => ProtectionRequirement::Rung(rung),
                };
                (
                    Some(requirement),
                    Some(setting.reason.clone()),
                    Some(now.saturating_add(i64::from(setting.days).saturating_mul(DAY_SECS))),
                )
            }
            None => {
                if lift_reason.is_some_and(|reason| !is_valid_reason(reason)) {
                    return Err(ProtectError::BadReason);
                }
                (None, lift_reason.map(str::to_owned), None)
            }
        };
        let kind = kind_of(target);
        let in_force = book.store.active_protections_on(wiki, kind, title, now)?;
        // The record that ends last, which a renewal follows; a lifting follows every record in force (there is one, unless an
        // older state left more, and the person who lifts must find the page unprotected afterwards).
        let current = in_force.first();
        let follows = match (act, current) {
            (Act::Set, Some(_)) => return Err(ProtectError::AlreadyInForce),
            (Act::Set, None) => None,
            (Act::Renew | Act::Lift, None) => return Err(ProtectError::NotInForce),
            (Act::Renew | Act::Lift, Some(current)) => Some(current.id.clone()),
        };
        let facts = self
            .facts(principal, Action::Moderate, wiki)
            .map_err(|e| ProtectError::Store(e.0))?;
        let rung = rung_of(facts.snapshot, &facts.rules).ok_or(ProtectError::NotAllowed)?;
        // The trail first: an act whose record could not be written does not happen (RFC 002 D-9).
        let operation = AuditEvent::new(
            now,
            &principal.audit_name(),
            act.operation(),
            &resource.audit_name(),
            AuditOutcome::Allowed,
            &authority_detail(principal, &facts),
        );
        self.audit
            .record(&operation)
            .map_err(|e| ProtectError::Store(e.to_string()))?;
        let event = ProtectionEvent {
            id: ProtectionEventId::new(),
            wiki: wiki.clone(),
            at: now,
            event: act.event(),
            kind,
            target: title.to_owned(),
            requirement,
            by: Actor { account, rung },
            policy_block: facts.rules.block().map(str::to_owned),
            reason,
            expires_at,
            of_event: follows,
            appeal: None,
        };
        let id = event.id.clone();
        // The check that nothing is in force and the record are one step in the store, so that two acts at once do not both set.
        let recorded: Result<(), ProtectError> = match act {
            Act::Set => book
                .store
                .record_protection_event_if_none_in_force(&event)
                .map_err(|e| match e {
                    StoreError::StateChanged => ProtectError::AlreadyInForce,
                    other => other.into(),
                }),
            Act::Renew => book
                .store
                .record_protection_event(&event)
                .map_err(Into::into),
            Act::Lift => {
                // Every record in force ends, each by a lifting that names it: the first is the one this act is named by.
                in_force
                    .iter()
                    .skip(1)
                    .try_for_each(|older| {
                        book.store.record_protection_event(&ProtectionEvent {
                            id: ProtectionEventId::new(),
                            of_event: Some(older.id.clone()),
                            ..event.clone()
                        })
                    })
                    .and_then(|()| book.store.record_protection_event(&event))
                    .map_err(Into::into)
            }
        };
        // The trail named the act before its record; where the record fails it says so, so that it never names an act that did
        // not happen (F-4). A trail that cannot take this event changes nothing: the act failed whatever it says.
        if let Err(e) = &recorded {
            let word = match e {
                ProtectError::AlreadyInForce => "already-in-force",
                _ => "store-error",
            };
            let _ = self.audit.record(&AuditEvent::new(
                now,
                &principal.audit_name(),
                act.operation(),
                &resource.audit_name(),
                AuditOutcome::Failed,
                &format!("{}; failed={word}", authority_detail(principal, &facts)),
            ));
        }
        recorded.map(|()| id)
    }
}
