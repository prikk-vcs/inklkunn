//! What an action is **now**: whether it is in force, who it affects, and who took it (RFC 006 D-9). The table in the module's text
//! is this file's: each kind has one function, and each answers `Ok(None)` for *no such action* and for *not in force*, which a
//! surface cannot tell apart.

use inklkunn_store::{
    AccountId, AppealedAction, AppealedKind, ProposalFilter, ProposalId, ProposalOp, ProposalState,
    ProtectionEvent, ProtectionEventId, RestrictionEventId, TombstoneAct, TombstoneEventId,
    WikiRecord,
};

use super::service::{AppealError, AppealService, unavailable};
use crate::protection::{Notice, titles_of};
use crate::tombstone::Hidden;
use inklkunn_store::{GrantDecisionId, GrantDecisionRecord};

/// Whom an action affects.
pub(super) enum Who {
    /// One account: the proposer of a declined proposal, the author of what was hidden, the restricted account, the person whose
    /// role was lowered.
    Account(AccountId),
    /// **Whoever a temporary protection holds back**: the author of an open proposal that touches a page it covers. Not one
    /// account, so the rule is asked of each person ([`AppealService::affects`]).
    HeldBack(Box<ProtectionEvent>),
}

/// An action in force.
pub(super) struct Found {
    /// Whom it affects.
    pub(super) who: Who,
    /// Who took it, when an account did.
    pub(super) actor: Option<AccountId>,
    /// For a decline: the proposal it declined.
    pub(super) proposal: Option<ProposalId>,
}

/// One event of a measure's chain, as far as who acted goes.
struct Link {
    id: String,
    follows: Option<String>,
    by: AccountId,
}

/// The people who acted on the chain **through** the event `named`: who set it and everyone who renewed it, before or after the
/// event the appeal names. A line, so it is walked back to its first event and forward to its last.
fn chain_actors(links: &[Link], named: &str) -> Vec<AccountId> {
    let mut first = named;
    for _ in 0..links.len() {
        match links
            .iter()
            .find(|l| l.id == first)
            .and_then(|l| l.follows.as_deref())
        {
            Some(before) => first = before,
            None => break,
        }
    }
    let mut out = Vec::new();
    let mut at = Some(first);
    for _ in 0..=links.len() {
        let Some(id) = at else { break };
        let Some(link) = links.iter().find(|l| l.id == id) else {
            break;
        };
        if !out.contains(&link.by) {
            out.push(link.by.clone());
        }
        at = links
            .iter()
            .find(|l| l.follows.as_deref() == Some(id))
            .map(|l| l.id.as_str());
    }
    out
}

/// The position of a step of the proposal log, from its spelling: **decimal, from 1, no sign, no leading zero** (so that the
/// spelling is the one the appeal records).
pub(super) fn position_of(id: &str) -> Option<i64> {
    let n: i64 = id.parse().ok()?;
    (n >= 1 && n.to_string() == id).then_some(n)
}

impl AppealService {
    /// The action in force, or `None` when there is no such action in this wiki or it is not in force.
    pub(super) fn find(
        &self,
        wiki: &WikiRecord,
        action: &AppealedAction,
    ) -> Result<Option<Found>, AppealError> {
        match action.kind {
            AppealedKind::Decline => self.find_decline(wiki, &action.id),
            AppealedKind::Tombstone => self.find_tombstone(wiki, &action.id),
            AppealedKind::Restriction => self.find_restriction(wiki, &action.id),
            AppealedKind::Protection => self.find_protection(wiki, &action.id),
            AppealedKind::Grant => self.find_grant(wiki, &action.id),
        }
    }

    /// **Everyone who took the action** an appeal names (A-5): the person the appeal recorded as its actor, and, for a restriction
    /// or a temporary protection, everyone who set or renewed the chain it is a link of, since a renewal is also an act on it.
    /// Nobody of them decides the appeal. (The database refuses the first; the rest is this check.)
    pub(super) fn actors_of(
        &self,
        wiki: &WikiRecord,
        action: &AppealedAction,
    ) -> Result<Vec<AccountId>, AppealError> {
        match action.kind {
            AppealedKind::Restriction => {
                let Some(event) = RestrictionEventId::parse(&action.id)
                    .and_then(|id| self.governance.restriction_event(&id).transpose())
                    .transpose()
                    .map_err(unavailable)?
                    .filter(|e| e.wiki == wiki.id)
                else {
                    return Ok(Vec::new());
                };
                let links: Vec<Link> = self
                    .governance
                    .restriction_events(&wiki.id, &event.account)
                    .map_err(unavailable)?
                    .into_iter()
                    .filter(|e| e.event.puts_in_place())
                    .map(|e| Link {
                        id: e.id.to_string(),
                        follows: e.of_event.map(|o| o.to_string()),
                        by: e.by.account,
                    })
                    .collect();
                Ok(chain_actors(&links, &action.id))
            }
            AppealedKind::Protection => {
                let links: Vec<Link> = self
                    .governance
                    .protection_events(&wiki.id, inklkunn_store::governance::MAX_PAGE)
                    .map_err(unavailable)?
                    .into_iter()
                    .filter(|e| e.event.puts_in_place())
                    .map(|e| Link {
                        id: e.id.to_string(),
                        follows: e.of_event.map(|o| o.to_string()),
                        by: e.by.account,
                    })
                    .collect();
                Ok(chain_actors(&links, &action.id))
            }
            _ => Ok(Vec::new()),
        }
    }

    /// A **decline** is in force while it is the last step of the proposal's log and the proposal is declined.
    fn find_decline(&self, wiki: &WikiRecord, id: &str) -> Result<Option<Found>, AppealError> {
        let Some(position) = position_of(id) else {
            return Ok(None);
        };
        let Some((proposal, step)) = self
            .proposals
            .transition_at(&wiki.id, position)
            .map_err(unavailable)?
        else {
            return Ok(None);
        };
        if step.op != ProposalOp::Decline {
            return Ok(None);
        }
        let Some(record) = self
            .proposals
            .proposal(&proposal, None)
            .map_err(unavailable)?
            .filter(|p| p.wiki == wiki.id && p.state == ProposalState::Declined)
        else {
            return Ok(None);
        };
        let last = self
            .proposals
            .last_transition_position(&proposal)
            .map_err(unavailable)?;
        if last != Some(position) {
            return Ok(None);
        }
        Ok(Some(Found {
            who: Who::Account(record.author),
            actor: Some(step.actor),
            proposal: Some(proposal),
        }))
    }

    /// A **tombstone** is a hide no unhide has followed, of something that has an author with an account.
    fn find_tombstone(&self, wiki: &WikiRecord, id: &str) -> Result<Option<Found>, AppealError> {
        let Some(id) = TombstoneEventId::parse(id) else {
            return Ok(None);
        };
        let Some(event) = self
            .governance
            .tombstone_event(&id)
            .map_err(unavailable)?
            .filter(|e| e.wiki == wiki.id && e.act == TombstoneAct::Hide)
        else {
            return Ok(None);
        };
        let events = self
            .governance
            .tombstone_events_of(&wiki.id, &event.item)
            .map_err(unavailable)?;
        if !Hidden::of_events(&events)
            .marks()
            .iter()
            .any(|m| m.event == event.id)
        {
            return Ok(None);
        }
        Ok(event.affected.map(|affected| Found {
            who: Who::Account(affected),
            actor: Some(event.by.account),
            proposal: None,
        }))
    }

    /// A **restriction** is the `Set` or `Renewed` event in force: begun, not ended, nothing following it.
    fn find_restriction(&self, wiki: &WikiRecord, id: &str) -> Result<Option<Found>, AppealError> {
        let Some(id) = RestrictionEventId::parse(id) else {
            return Ok(None);
        };
        let Some(event) = self
            .governance
            .restriction_event(&id)
            .map_err(unavailable)?
            .filter(|e| e.wiki == wiki.id && e.event.puts_in_place())
        else {
            return Ok(None);
        };
        let in_force = self
            .governance
            .active_restrictions(&wiki.id, &event.account, self.authorizer.now())
            .map_err(unavailable)?
            .iter()
            .any(|e| e.id == event.id);
        Ok(in_force.then_some(Found {
            who: Who::Account(event.account),
            actor: Some(event.by.account),
            proposal: None,
        }))
    }

    /// A **temporary protection** is the `Set` or `Renewed` event in force, which holds back the proposals that touch what it covers.
    fn find_protection(&self, wiki: &WikiRecord, id: &str) -> Result<Option<Found>, AppealError> {
        let Some(id) = ProtectionEventId::parse(id) else {
            return Ok(None);
        };
        let Some(event) = self
            .governance
            .protection_event(&id)
            .map_err(unavailable)?
            .filter(|e| e.wiki == wiki.id && e.event.puts_in_place())
        else {
            return Ok(None);
        };
        let in_force = self
            .governance
            .active_protections(&wiki.id, self.authorizer.now())
            .map_err(unavailable)?
            .iter()
            .any(|e| e.id == event.id);
        Ok(in_force.then(|| Found {
            actor: Some(event.by.account.clone()),
            who: Who::HeldBack(Box::new(event)),
            proposal: None,
        }))
    }

    /// Is this grant decision **the latest** about its person in the wiki? A decision is in force only while it is: after D1 (admin to
    /// maintainer), D2 (to observer) and D3 (to maintainer) the person holds what D1 gave them, but D1 is not their standing any more,
    /// and overturning it would undo D3 (A-3).
    pub(super) fn is_latest_grant(
        &self,
        wiki: &WikiRecord,
        decision: &GrantDecisionRecord,
    ) -> Result<bool, AppealError> {
        let latest = self
            .governance
            .grant_decisions(&wiki.id, Some(&decision.target), 1)
            .map_err(unavailable)?;
        Ok(latest.first().is_some_and(|d| d.id == decision.id))
    }

    /// A **grant decision** that took a rung away or lowered it is in force while it is **the person's latest decision in the
    /// wiki** and they still hold the rung it gave them. (A decision that raised a rung, or gave a first one, is no one's
    /// grievance.)
    fn find_grant(&self, wiki: &WikiRecord, id: &str) -> Result<Option<Found>, AppealError> {
        let Some(id) = GrantDecisionId::parse(id) else {
            return Ok(None);
        };
        let Some(decision) = self
            .governance
            .grant_decision(&id)
            .map_err(unavailable)?
            .filter(|d| d.wiki == wiki.id)
        else {
            return Ok(None);
        };
        let lowered = match (decision.before, decision.after) {
            (Some(before), Some(after)) => after < before,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let holds = self
            .roles
            .role(&decision.target, &wiki.id)
            .map_err(unavailable)?;
        if !(lowered && holds == decision.after && self.is_latest_grant(wiki, &decision)?) {
            return Ok(None);
        }
        Ok(Some(Found {
            who: Who::Account(decision.target),
            actor: decision.by.map(|a| a.account),
            proposal: None,
        }))
    }

    /// **Does the action affect this person?** The one rule of who may appeal. For a protection: do they have a proposal, open
    /// (held for review or needing a revision), whose latest revision touches a page the protection covers (a rename counts both its
    /// paths)? A bystander has none, and is not affected.
    pub(super) fn affects(
        &self,
        wiki: &WikiRecord,
        found: &Found,
        person: &AccountId,
    ) -> Result<bool, AppealError> {
        match &found.who {
            Who::Account(account) => Ok(account == person),
            Who::HeldBack(protection) => Ok(!self.held_by(wiki, protection, person)?.is_empty()),
        }
    }

    /// The open proposals of `person` that the temporary protection holds back.
    pub(super) fn held_by(
        &self,
        wiki: &WikiRecord,
        protection: &ProtectionEvent,
        person: &AccountId,
    ) -> Result<Vec<ProposalId>, AppealError> {
        let Some(notice) = Notice::of_record(protection, self.authorizer.now()) else {
            return Ok(Vec::new());
        };
        let open = self
            .proposals
            .proposals(
                &wiki.id,
                &ProposalFilter {
                    states: Some(vec![
                        ProposalState::HeldForReview,
                        ProposalState::NeedsRevision,
                    ]),
                    author: Some(person.clone()),
                },
                Some(person),
            )
            .map_err(unavailable)?;
        let mut held = Vec::new();
        for proposal in open {
            let revisions = self
                .proposals
                .revisions(&proposal.id, Some(person))
                .map_err(unavailable)?;
            let Some(latest) = revisions.last() else {
                continue;
            };
            let content = &latest.content;
            let paths = content.files.iter().map(|f| f.path.as_str()).chain(
                content
                    .renames
                    .iter()
                    .flat_map(|r| [r.from.as_str(), r.to.as_str()]),
            );
            if titles_of(paths).iter().any(|t| notice.covers_title(t)) {
                held.push(proposal.id);
            }
        }
        Ok(held)
    }
}
