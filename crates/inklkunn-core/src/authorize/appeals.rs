//! The two decisions an appeal asks of `authorize()` (RFC 006 D-9; handoff G-T5; threat model C-14g): **may this person file an
//! appeal here**, and **may this person decide one**. Each is audited as every consequential decision is, allowed or not, with
//! the rung the person held and the block of the rules it was decided under, and an allowed one whose record cannot be written
//! is denied.
//!
//! **Filing is not an action of the ladder.** It is authorized by being an account that may read the wiki, and by nothing
//! else: **not by `propose` or `comment`, and never capped by a restriction**. A restriction caps a person at `observer` for
//! writing, and the appeal of a restriction is by definition a restricted person's, so the cap must not reach it. What a person
//! may *appeal* (that the action is about them) is the appeal service's rule; what they may *say* is one bounded text, and the
//! path cannot be used to comment, to propose or to review.
//!
//! **Deciding is the rung `rungs.decide_appeal` names** (`maintainer` or `admin`, default `admin`), held **after** a
//! restriction: a restricted maintainer holds `observer`, and decides nothing. That the decider is not the one who took the
//! action, and not the appellant, is the service's rule (and the database's).

use inklkunn_store::{AuditEvent, AuditOutcome, Scope, WikiId};

use super::{
    Action, Authorizer, Decision, Facts, Principal, Resource, authority_detail, decide_under,
    rung_of,
};

impl Authorizer {
    /// **May `principal` file an appeal in `wiki`?** An account that may read the wiki: yes, whatever its rung and whatever
    /// restriction holds it down, while the wiki's rules can be read (a wiki that cannot read its rules refuses every write, and
    /// so refuses this). Anyone else, and a wiki that is not there, is denied, in the same words and at the same cost.
    ///
    /// Audited as `file-appeal`, allowed or not, with the rung (after any restriction, so that the trail shows what held the
    /// person down) and the policy block.
    pub fn authorize_appeal_filing(&self, principal: &Principal, wiki: &WikiId) -> Decision {
        // A write is asked, so that the rules and the restriction are read: the rung in the record is the held one.
        let (decision, outcome, detail) = match self.facts(principal, Action::Comment, wiki) {
            Ok(facts) => {
                let reads =
                    decide_under(facts.snapshot, &facts.rules, principal, Action::Read, false)
                        .is_allowed();
                let allowed = matches!(principal, Principal::Account(_))
                    && reads
                    && facts.rules.fault().is_none();
                (
                    verdict(allowed),
                    outcome(allowed),
                    authority_detail(principal, &facts),
                )
            }
            Err(_) => (
                Decision::Deny,
                AuditOutcome::Failed,
                "the facts for the decision could not be read".to_owned(),
            ),
        };
        self.record_appeal_decision("file-appeal", principal, wiki, decision, outcome, &detail)
    }

    /// **May `principal` decide appeals in `wiki`?** An account whose rung, after any restriction, is the one the wiki's
    /// `rungs.decide_appeal` names or higher, while the rules can be read, with a credential that carries what a maintainer's acts
    /// need. Audited as `decide-appeal`, allowed or not, with the rung and the block.
    pub fn authorize_appeal_decision(&self, principal: &Principal, wiki: &WikiId) -> Decision {
        let (decision, outcome, detail) = match self.facts(principal, Action::Moderate, wiki) {
            Ok(facts) => {
                let allowed = decides(principal, &facts);
                (
                    verdict(allowed),
                    outcome(allowed),
                    authority_detail(principal, &facts),
                )
            }
            Err(_) => (
                Decision::Deny,
                AuditOutcome::Failed,
                "the facts for the decision could not be read".to_owned(),
            ),
        };
        self.record_appeal_decision("decide-appeal", principal, wiki, decision, outcome, &detail)
    }

    /// May `principal` decide appeals in `wiki`? **Nothing is recorded**: what a page asks to know whether to show the queue. The
    /// act is [`Authorizer::authorize_appeal_decision`], which is.
    pub fn may_decide_appeals(&self, principal: &Principal, wiki: &WikiId) -> bool {
        self.facts(principal, Action::Moderate, wiki)
            .is_ok_and(|facts| decides(principal, &facts))
    }

    fn record_appeal_decision(
        &self,
        act: &str,
        principal: &Principal,
        wiki: &WikiId,
        decision: Decision,
        outcome: AuditOutcome,
        detail: &str,
    ) -> Decision {
        let event = AuditEvent::new(
            (self.clock)(),
            &principal.audit_name(),
            act,
            &Resource::Wiki { wiki: wiki.clone() }.audit_name(),
            outcome,
            detail,
        );
        if self.audit.record(&event).is_err() && decision.is_allowed() {
            return Decision::Deny;
        }
        decision
    }
}

/// The rule of who decides appeals: one place for the audited decision and the page's question.
fn decides(principal: &Principal, facts: &Facts) -> bool {
    match principal {
        Principal::Account(a) => {
            a.scope() >= Scope::Accept
                && facts.rules.fault().is_none()
                && rung_of(facts.snapshot, &facts.rules)
                    .is_some_and(|rung| rung >= facts.rules.policy().rungs().decide_appeal().role())
        }
        Principal::Anonymous | Principal::System(_) => false,
    }
}

fn verdict(allowed: bool) -> Decision {
    if allowed {
        Decision::Allow
    } else {
        Decision::Deny
    }
}

fn outcome(allowed: bool) -> AuditOutcome {
    if allowed {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    }
}
