//! What a person holds in a wiki, and the restriction that holds them down (RFC 006 D-5, D-7; handoff G-T2): the
//! rung they hold, **why** (their grant or the wiki's `entry`), and the restriction in force, with its reason and its end.
//!
//! This is a child of [`authorize`](super) on purpose: it reads the loader's private facts, so a surface asks the
//! [`Authorizer`] and cannot assert anything about a wiki. **A restriction is told to the restricted person alone**: every
//! function here is about the principal asking, and nothing here names anyone else's.

use inklkunn_store::{RestrictionEvent, RestrictionEventId, Role, WikiId};

use super::{Action, Authorizer, Principal, Resource, Snapshot, SourceError, decide_under};
use super::{entry_rung, is_governance_file, rung_of};

/// A restriction as the restricted person is told of it: the reason and the end. (The id is the hook for *contest this*,
/// which is G-T5's.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictionNotice {
    /// The restriction's own event: the one in force.
    pub id: RestrictionEventId,
    /// HeldBy, as the moderator wrote it.
    pub reason: String,
    /// When it ends, in Unix seconds: from that second on it no longer applies.
    pub until: i64,
}

impl RestrictionNotice {
    pub(super) fn of(event: &RestrictionEvent) -> RestrictionNotice {
        RestrictionNotice {
            id: event.id.clone(),
            reason: event.reason.clone().unwrap_or_default(),
            // A restriction in force always has an end; the store's own check says so.
            until: event.expires_at.unwrap_or(i64::MAX),
        }
    }
}

/// HeldBy a person holds the rung they hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldBy {
    /// They hold no rung: they may read what the wiki's visibility lets them read, and nothing else.
    Nothing,
    /// A grant: someone gave it to them.
    Grant,
    /// The wiki's `entry`: every signed-in account holds it.
    Entry,
}

/// What one account holds in one wiki, now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holding {
    /// The rung they hold, **after** a restriction: what decides what they may do. `None` is no rung.
    pub rung: Option<Role>,
    /// The rung they were granted, if any.
    pub grant: Option<Role>,
    /// The rung the wiki's `entry` gives them, if any (never in a `restricted` wiki).
    pub entry: Option<Role>,
    /// The restriction in force, when there is one and the person is not an admin (an admin is never held down).
    pub restriction: Option<RestrictionNotice>,
    /// The block that set the rules in force, or `None` for the defaults.
    pub policy_block: Option<String>,
}

impl Holding {
    /// The rung they hold before a restriction: the higher of the grant and `entry`.
    pub fn held(&self) -> Option<Role> {
        self.grant.max(self.entry)
    }

    /// HeldBy they hold it.
    pub fn held_by(&self) -> HeldBy {
        match (self.grant, self.entry) {
            (None, None) => HeldBy::Nothing,
            (Some(grant), entry) if Some(grant) >= entry => HeldBy::Grant,
            _ => HeldBy::Entry,
        }
    }
}

impl Authorizer {
    /// The clock the authorizer reads restrictions by, in Unix seconds.
    pub fn now(&self) -> i64 {
        (self.clock)()
    }

    /// Where `principal` stands in `wiki` now (RFC 006 D-5: the higher of the grant and `entry`, capped by a restriction).
    /// Nothing is recorded. For a principal that is not an account it is nothing at all. An error is *could not be read
    /// now*.
    pub fn holding(&self, principal: &Principal, wiki: &WikiId) -> Result<Holding, SourceError> {
        // A write is asked, so that the rules and the restriction are read: the rung is theirs.
        let facts = self.facts(principal, Action::Propose, wiki)?;
        let (rung, grant, entry) = match principal {
            Principal::Account(_) => (
                rung_of(facts.snapshot, &facts.rules),
                facts.snapshot.role,
                facts
                    .snapshot
                    .visibility
                    .and_then(|v| entry_rung(v, &facts.rules)),
            ),
            Principal::Anonymous | Principal::System(_) => (None, None, None),
        };
        let held = grant.max(entry);
        Ok(Holding {
            rung,
            grant,
            entry,
            restriction: facts
                .restriction
                .as_ref()
                .filter(|_| held != Some(Role::Admin))
                .map(RestrictionNotice::of),
            policy_block: facts.rules.block().map(str::to_owned),
        })
    }

    /// **Is the restriction the reason `principal` may not do `action` to `resource`?** `Some` only when the principal is
    /// restricted and would have been allowed without the restriction: so a refusal that has another cause (a role that is
    /// too low, a scope, a wiki that is not there, rules that cannot be read) is never put down to a restriction, and
    /// nobody but the restricted person is ever told one (a restriction is the person's own business and the moderators').
    /// Nothing is recorded.
    pub fn restriction_refusal(
        &self,
        principal: &Principal,
        action: Action,
        resource: &Resource,
    ) -> Option<RestrictionNotice> {
        let facts = self.facts(principal, action, resource.wiki()).ok()?;
        let event = facts.restriction.as_ref()?;
        let on_governance_file = is_governance_file(resource);
        let uncapped = Snapshot {
            cap: None,
            ..facts.snapshot
        };
        let would = decide_under(
            uncapped,
            &facts.rules,
            principal,
            action,
            on_governance_file,
        );
        let does = decide_under(
            facts.snapshot,
            &facts.rules,
            principal,
            action,
            on_governance_file,
        );
        (would.is_allowed() && !does.is_allowed()).then(|| RestrictionNotice::of(event))
    }
}
