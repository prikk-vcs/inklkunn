//! The trail never names an act that did not happen (handoff G-T1 correction F-4). An act's event is written *allowed* before
//! its record; where the record then fails, a second event of the same act and resource says *failed* and why in one word. The
//! record is made to fail by a hook on the trail: the moment the act's event is kept, another act gets in first.

use std::sync::Arc;

use inklkunn_store::{
    Actor, AuditEvent, AuditOutcome, MeasureEvent, MemoryAudit, ProtectionEvent, ProtectionEventId,
    ProtectionKind, ProtectionRequirement, RestrictionEvent, RestrictionEventId, Role, RoleGrant,
};

use super::support::{DAY, HookedAudit, KINDS, T0, World, set_restriction, steady_world};
use crate::authorize::{Authorizer, ProtectError, Setting};
use crate::governance::{Require, Target};
use crate::people::{GrantRefusal, PeopleError, PeopleService, RestrictRefusal};

/// The service of the world, writing its act events to a trail that runs `hook` on each one once it is kept.
fn people_with(
    w: &World,
    hook: impl Fn(&AuditEvent) + Send + Sync + 'static,
) -> (PeopleService, Arc<MemoryAudit>) {
    let trail = Arc::new(MemoryAudit::new());
    let sink = HookedAudit {
        inner: trail.clone(),
        hook: Box::new(hook),
    };
    let people = PeopleService::new(
        w.authorizer.clone(),
        w.store.clone(),
        w.store.clone(),
        w.store.clone(),
        Arc::new(sink),
        w.limits,
    );
    (people, trail)
}

/// An authorizer over the world's facts, with the same trail and hook.
fn authorizer_with(
    w: &World,
    hook: impl Fn(&AuditEvent) + Send + Sync + 'static,
) -> (Authorizer, Arc<MemoryAudit>) {
    let trail = Arc::new(MemoryAudit::new());
    let sink = HookedAudit {
        inner: trail.clone(),
        hook: Box::new(hook),
    };
    let authorizer = Authorizer::new(w.source.clone(), Arc::new(sink))
        .with_clock(w.clock)
        .with_protections(w.store.clone(), w.limits);
    (authorizer, trail)
}

/// The events of one act, in order: how each ended, and the detail of the last.
fn of_act(trail: &MemoryAudit, action: &str) -> Vec<AuditEvent> {
    trail
        .events()
        .into_iter()
        .filter(|e| e.action == action)
        .collect()
}

fn assert_allowed_then_failed(events: &[AuditEvent], word: &str) {
    let [first, second] = events else {
        panic!("two events: {events:?}");
    };
    assert_eq!(first.outcome, AuditOutcome::Allowed, "{first:?}");
    assert_eq!(second.outcome, AuditOutcome::Failed, "{second:?}");
    assert_eq!(first.resource, second.resource, "the same resource");
    assert_eq!(first.principal, second.principal, "the same principal");
    assert!(
        second.detail.ends_with(&format!("failed={word}")),
        "{second:?}"
    );
}

fn lifted(w: &World, by: &inklkunn_store::AccountId, of: &RestrictionEvent) -> RestrictionEvent {
    RestrictionEvent {
        id: RestrictionEventId::new(),
        wiki: w.wiki.id.clone(),
        at: T0,
        event: MeasureEvent::Lifted,
        account: of.account.clone(),
        by: Actor {
            account: by.clone(),
            rung: Role::Admin,
        },
        policy_block: None,
        reason: None,
        expires_at: None,
        of_event: Some(of.id.clone()),
        appeal: None,
    }
}

#[test]
fn a_grant_whose_record_fails_leaves_the_allowed_event_and_then_a_failed_one() {
    for kind in KINDS {
        let w = steady_world(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let target = w.person("Sam", None);
        let (store, wiki, id) = (w.store.clone(), w.wiki.id.clone(), target.id.clone());
        // Someone else gives Sam a rung between the act's event and its record: the record's `before` is stale.
        let (people, trail) = people_with(&w, move |e| {
            if e.action == "grant" && e.outcome == AuditOutcome::Allowed {
                store
                    .grant(RoleGrant {
                        account: id.clone(),
                        wiki: wiki.clone(),
                        role: Role::Observer,
                        granted_by: None,
                        granted_at: 1,
                    })
                    .unwrap();
            }
        });
        let result = people.grant(
            &admin.principal,
            &w.wiki,
            &target.id.to_string(),
            Some(Role::Contributor),
            "",
        );
        assert_eq!(result.unwrap_err(), PeopleError::Changed, "{kind:?}");
        assert_allowed_then_failed(&of_act(&trail, "grant"), "state-changed");
    }
}

#[test]
fn a_restriction_set_that_lost_to_another_leaves_the_allowed_event_and_then_a_failed_one() {
    for kind in KINDS {
        let w = steady_world(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let other = w.person("Ben", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        let (store, first) = (
            w.store.clone(),
            set_restriction(&w, &other, &person, T0 + DAY),
        );
        let (people, trail) = people_with(&w, move |e| {
            if e.action == "restrict" && e.outcome == AuditOutcome::Allowed {
                store.record_restriction_event(&first).unwrap();
            }
        });
        let result = people.restrict(&admin.principal, &w.wiki, &person.id.to_string(), 3, "Why.");
        assert!(
            matches!(
                result,
                Err(PeopleError::Restrict(
                    RestrictRefusal::AlreadyRestricted { .. }
                ))
            ),
            "{kind:?}: {result:?}"
        );
        assert_allowed_then_failed(&of_act(&trail, "restrict"), "already-restricted");
    }
}

#[test]
fn a_renewal_and_a_lifting_that_lost_to_another_leave_the_allowed_event_and_then_a_failed_one() {
    for kind in KINDS {
        for act in ["renew-restriction", "lift-restriction"] {
            let w = steady_world(kind);
            let admin = w.person("Ada", Some(Role::Admin));
            let other = w.person("Ben", Some(Role::Admin));
            let person = w.person("Pat", Some(Role::Contributor));
            w.people
                .restrict(&admin.principal, &w.wiki, &person.id.to_string(), 3, "Why.")
                .unwrap();
            let active = w
                .store
                .active_restrictions(&w.wiki.id, &person.id, T0)
                .unwrap()
                .remove(0);
            let (store, ender) = (w.store.clone(), lifted(&w, &other.id, &active));
            let (people, trail) = people_with(&w, move |e| {
                if e.action == act && e.outcome == AuditOutcome::Allowed {
                    store.record_restriction_event(&ender).unwrap();
                }
            });
            let who = person.id.to_string();
            let result = if act == "renew-restriction" {
                people.renew(&admin.principal, &w.wiki, &who, 3, "Still.")
            } else {
                people.lift(&admin.principal, &w.wiki, &who, "")
            };
            assert_eq!(result.unwrap_err(), PeopleError::Changed, "{kind:?} {act}");
            assert_allowed_then_failed(&of_act(&trail, act), "state-changed");
        }
    }
}

fn setting(title: &str) -> Setting {
    Setting {
        target: Target::path(title).unwrap(),
        require: Require::Review,
        reason: "A dispute.".to_owned(),
        days: 3,
    }
}

fn protection(w: &World, by: &super::support::Person, title: &str) -> ProtectionEvent {
    ProtectionEvent {
        id: ProtectionEventId::new(),
        wiki: w.wiki.id.clone(),
        at: T0,
        event: MeasureEvent::Set,
        kind: ProtectionKind::Page,
        target: title.to_owned(),
        requirement: Some(ProtectionRequirement::Review),
        by: Actor {
            account: by.id.clone(),
            rung: Role::Admin,
        },
        policy_block: None,
        reason: Some("A dispute.".to_owned()),
        expires_at: Some(T0 + DAY),
        of_event: None,
        appeal: None,
    }
}

#[test]
fn a_protection_set_that_lost_to_another_leaves_the_allowed_event_and_then_a_failed_one() {
    for kind in KINDS {
        let w = steady_world(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let other = w.person("Ben", Some(Role::Admin));
        let (store, first) = (w.store.clone(), protection(&w, &other, "Help"));
        let (authorizer, trail) = authorizer_with(&w, move |e| {
            if e.action == "protection-set" && e.outcome == AuditOutcome::Allowed {
                store.record_protection_event(&first).unwrap();
            }
        });
        let result = authorizer.protect(&admin.principal, &w.wiki.id, &setting("Help"));
        assert_eq!(
            result.unwrap_err(),
            ProtectError::AlreadyInForce,
            "{kind:?}"
        );
        assert_allowed_then_failed(&of_act(&trail, "protection-set"), "already-in-force");
    }
}

#[test]
fn a_protection_renewal_and_lifting_that_lost_to_another_leave_the_allowed_event_and_then_a_failed_one()
 {
    for kind in KINDS {
        for act in ["protection-renewed", "protection-lifted"] {
            let w = steady_world(kind);
            let admin = w.person("Ada", Some(Role::Admin));
            w.authorizer
                .protect(&admin.principal, &w.wiki.id, &setting("Help"))
                .unwrap();
            let current = w
                .store
                .active_protections(&w.wiki.id, T0)
                .unwrap()
                .remove(0);
            let ender = ProtectionEvent {
                id: ProtectionEventId::new(),
                event: MeasureEvent::Lifted,
                requirement: None,
                reason: None,
                expires_at: None,
                of_event: Some(current.id.clone()),
                ..current.clone()
            };
            let store = w.store.clone();
            let (authorizer, trail) = authorizer_with(&w, move |e| {
                if e.action == act && e.outcome == AuditOutcome::Allowed {
                    store.record_protection_event(&ender).unwrap();
                }
            });
            let result = if act == "protection-renewed" {
                authorizer.renew_protection(&admin.principal, &w.wiki.id, &setting("Help"))
            } else {
                authorizer.lift_protection(
                    &admin.principal,
                    &w.wiki.id,
                    &Target::path("Help").unwrap(),
                    None,
                )
            };
            assert!(
                matches!(result, Err(ProtectError::Store(_))),
                "{kind:?} {act}: {result:?}"
            );
            assert_allowed_then_failed(&of_act(&trail, act), "store-error");
        }
    }
}

#[test]
fn a_grant_the_last_admin_guard_refused_says_so_in_the_failed_event() {
    for kind in KINDS {
        let w = steady_world(kind);
        let ada = w.person("Ada", Some(Role::Admin));
        let ben = w.person("Ben", Some(Role::Admin));
        // Ada stops being an admin between her act's event and its record, so that Ben is the last admin the record would take.
        let (store, wiki) = (w.store.clone(), w.wiki.id.clone());
        let ada_id = ada.id.clone();
        let (people, trail) = people_with(&w, move |e| {
            if e.action == "revoke" && e.outcome == AuditOutcome::Allowed {
                // Ada herself is no longer an admin who can sign in: the account the act is about is the last one.
                store.revoke(&ada_id, &wiki).unwrap();
            }
        });
        let result = people.grant(&ada.principal, &w.wiki, &ben.id.to_string(), None, "");
        assert_eq!(
            result.unwrap_err(),
            PeopleError::Grant(GrantRefusal::LastAdmin),
            "{kind:?}"
        );
        assert_allowed_then_failed(&of_act(&trail, "revoke"), "last-admin");
    }
}
