#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The acceptor must still hold the rung that accepts **under the lease** (handoff G-T1 correction F-1): the early
//! authorization asks it before the lease, and what waits for the lease may be a restriction or a demotion that came between.
//! In-memory, with the lease taken by the test's own observer, so that the act lands exactly between the two.

use inklkunn_store::{
    GovernanceStore, MeasureEvent, ProposalState, RestrictionEvent, RestrictionEventId, RoleGrant,
    RoleStore,
};

use super::super::testing::{HOME, Rig};
use super::*;
use crate::acceptance::AcceptStep;

fn changed(text: &str) -> String {
    text.replace("One.", "Uno.")
}

struct Scene {
    rig: Rig,
    maintainer: (AccountId, Principal),
    author: Principal,
    /// Whoever records a restriction: the store knows the account.
    moderator: AccountId,
}

fn scene(policy: &str) -> Scene {
    let rig = Rig::new("lease-rung");
    rig.fake.derive_changes();
    if !policy.is_empty() {
        rig.fake
            .set_files(&[("governance/policy.yaml".to_owned(), policy.to_owned())]);
        rig.reads.forget_wiki(&rig.wiki);
    }
    let maintainer = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let (moderator, _) = rig.person("Ada", Some(Role::Admin));
    Scene {
        rig,
        maintainer,
        author,
        moderator,
    }
}

impl Scene {
    fn propose(&self) -> ProposalRecord {
        self.rig.propose(
            &self.author,
            "Say it in Spanish",
            &[("pages/Home.md", Some(&changed(HOME)))],
        )
    }

    /// Accept as the maintainer, with `between` done the moment the lease is taken.
    fn accept_with(
        &self,
        p: &ProposalRecord,
        between: impl Fn() + Send + Sync + 'static,
    ) -> Result<Accepted, AcceptFailure> {
        self.rig
            .acceptor()
            .with_observer(Arc::new(move |step| {
                if step == AcceptStep::LeaseTaken {
                    between();
                }
            }))
            .accept(&self.maintainer.1, &self.rig.wiki, &p.id, 1)
    }

    /// Restrict `who` for a day, as another maintainer would have recorded it (the store's own checks only).
    fn restrict(&self, who: &AccountId) -> impl Fn() + Send + Sync + 'static + use<> {
        let store = self.rig.store.clone();
        let wiki = self.rig.wiki.id.clone();
        let who = who.clone();
        let by = self.moderator.clone();
        move || {
            let now = inklkunn_store::record::now_secs();
            store
                .record_restriction_event(&RestrictionEvent {
                    id: RestrictionEventId::new(),
                    wiki: wiki.clone(),
                    at: now,
                    event: MeasureEvent::Set,
                    account: who.clone(),
                    by: inklkunn_store::Actor {
                        account: by.clone(),
                        rung: Role::Admin,
                    },
                    policy_block: None,
                    reason: Some("Edit-warring.".to_owned()),
                    expires_at: Some(now + 86_400),
                    of_event: None,
                    appeal: None,
                })
                .unwrap();
        }
    }
}

#[test]
fn an_acceptor_restricted_between_the_early_judgement_and_the_lease_is_refused_restricted_and_nothing_is_signed()
 {
    let s = scene("");
    let p = s.propose();
    let result = s.accept_with(&p, s.restrict(&s.maintainer.0));
    let Err(AcceptFailure::Restricted(notice)) = &result else {
        panic!("{result:?}");
    };
    assert_eq!(notice.reason, "Edit-warring.");
    assert!(s.rig.double.calls().is_empty(), "nothing is signed");
    assert_eq!(s.rig.state(&p.id), ProposalState::HeldForReview);
}

#[test]
fn an_acceptor_demoted_by_a_grant_between_the_early_judgement_and_the_lease_is_refused_not_allowed_and_nothing_is_signed()
 {
    let s = scene("");
    let p = s.propose();
    let store = s.rig.store.clone();
    let (who, wiki) = (s.maintainer.0.clone(), s.rig.wiki.id.clone());
    let result = s.accept_with(&p, move || {
        store
            .grant(RoleGrant {
                account: who.clone(),
                wiki: wiki.clone(),
                role: Role::Reviewer,
                granted_by: None,
                granted_at: 2,
            })
            .unwrap();
    });
    assert!(
        matches!(result, Err(AcceptFailure::NotAllowed)),
        "{result:?}"
    );
    assert!(s.rig.double.calls().is_empty());
    assert_eq!(s.rig.state(&p.id), ProposalState::HeldForReview);
}

#[test]
fn a_restriction_on_a_person_who_would_be_refused_anyway_is_not_named() {
    // Demoted to nothing and restricted: the restriction is not the cause, and the person is told nothing of it.
    let s = scene("");
    let p = s.propose();
    let store = s.rig.store.clone();
    let (who, wiki) = (s.maintainer.0.clone(), s.rig.wiki.id.clone());
    let restrict = s.restrict(&s.maintainer.0);
    let result = s.accept_with(&p, move || {
        store.revoke(&who, &wiki).unwrap();
        restrict();
    });
    assert!(
        matches!(result, Err(AcceptFailure::NotAllowed)),
        "{result:?}"
    );
}

#[test]
fn the_rung_is_judged_before_the_protection_so_a_restricted_acceptor_is_told_the_restriction_and_not_the_protection()
 {
    // The page asks a maintainer, which the restricted maintainer no longer is: the lack the protection would name is the
    // restriction's, and the restriction is what is said.
    let s = scene(
        "protect:\n  - path: Home\n    require:\n      rung: maintainer\n    reason: \"The front page.\"\n",
    );
    let p = s.propose();
    let result = s.accept_with(&p, s.restrict(&s.maintainer.0));
    assert!(
        matches!(result, Err(AcceptFailure::Restricted(_))),
        "{result:?}"
    );
}
