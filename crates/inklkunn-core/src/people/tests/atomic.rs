//! The store's own step (handoff G-T1 correction F-2), on both stores, with no thread to hope for: a restriction or a protection
//! is not set while one is in force, the last admin who can sign in is not taken away, the service names each refusal, and
//! lifting ends every restriction in force.

use inklkunn_store::{
    Actor, GrantDecisionId, GrantDecisionRecord, MeasureEvent, ProtectionEvent, ProtectionEventId,
    ProtectionKind, ProtectionRequirement, Role, StoreError,
};

use super::support::{DAY, KINDS, Person, T0, set_restriction, steady_world as world};
use crate::people::{GrantRefusal, PeopleError};

#[test]
fn the_store_refuses_a_second_set_while_one_is_in_force_and_another_person_is_another_chain() {
    for kind in KINDS {
        let w = world(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        w.store
            .record_restriction_event_if_none_in_force(&set_restriction(
                &w,
                &admin,
                &person,
                T0 + DAY,
            ))
            .unwrap();
        assert_eq!(
            w.store
                .record_restriction_event_if_none_in_force(&set_restriction(
                    &w,
                    &admin,
                    &person,
                    T0 + 2 * DAY
                )),
            Err(StoreError::StateChanged),
            "{kind:?}: one is in force"
        );
        assert_eq!(
            w.store
                .active_restrictions(&w.wiki.id, &person.id, T0)
                .unwrap()
                .len(),
            1
        );
        let other = w.person("Olu", Some(Role::Contributor));
        w.store
            .record_restriction_event_if_none_in_force(&set_restriction(
                &w,
                &admin,
                &other,
                T0 + DAY,
            ))
            .unwrap();
    }
}

#[test]
fn the_store_refuses_a_second_protection_of_the_same_title_and_takes_another_title_or_kind() {
    for kind in KINDS {
        let w = world(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let protect = |target: &str, kind: ProtectionKind| ProtectionEvent {
            id: ProtectionEventId::new(),
            wiki: w.wiki.id.clone(),
            at: T0,
            event: MeasureEvent::Set,
            kind,
            target: target.to_owned(),
            requirement: Some(ProtectionRequirement::Review),
            by: Actor {
                account: admin.id.clone(),
                rung: Role::Admin,
            },
            policy_block: None,
            reason: Some("A dispute.".to_owned()),
            expires_at: Some(T0 + DAY),
            of_event: None,
            appeal: None,
        };
        w.store
            .record_protection_event_if_none_in_force(&protect("Help", ProtectionKind::Page))
            .unwrap();
        assert_eq!(
            w.store
                .record_protection_event_if_none_in_force(&protect("Help", ProtectionKind::Page)),
            Err(StoreError::StateChanged),
            "{kind:?}"
        );
        for (target, k) in [
            ("Help", ProtectionKind::Namespace),
            ("Home", ProtectionKind::Page),
        ] {
            w.store
                .record_protection_event_if_none_in_force(&protect(target, k))
                .unwrap();
        }
    }
}

#[test]
fn the_store_will_not_take_the_last_admin_who_can_sign_in_and_does_not_count_a_disabled_one() {
    for kind in KINDS {
        let w = world(kind);
        let ada = w.person("Ada", Some(Role::Admin));
        let ben = w.person("Ben", Some(Role::Admin));
        let cai = w.person("Cai", Some(Role::Admin));
        w.disable(&cai);
        let decision = |target: &Person, before, after| GrantDecisionRecord {
            id: GrantDecisionId::new(),
            wiki: w.wiki.id.clone(),
            at: T0,
            by: None,
            policy_block: None,
            target: target.id.clone(),
            before,
            after,
            reason: None,
            appeal: None,
        };
        // Cai is disabled: Ben is the only other admin who can sign in, so Ada may leave while Ben stays.
        w.store
            .record_grant_decision_keeping_an_admin(&decision(
                &ada,
                Some(Role::Admin),
                Some(Role::Contributor),
            ))
            .unwrap();
        assert_eq!(
            w.store.record_grant_decision_keeping_an_admin(&decision(
                &ben,
                Some(Role::Admin),
                None
            )),
            Err(StoreError::StateChanged),
            "{kind:?}: Ben is the last who can sign in"
        );
        assert_eq!(w.grant_of(&ben), Some(Role::Admin));
        // A change that takes no admin grant away is not the guard's.
        let dee = w.person("Dee", None);
        w.store
            .record_grant_decision_keeping_an_admin(&decision(&dee, None, Some(Role::Reviewer)))
            .unwrap();
    }
}

#[test]
fn the_people_service_names_the_last_admin_when_the_guard_refused_and_changed_when_the_grant_moved()
{
    for kind in KINDS {
        let w = world(kind);
        let ada = w.person("Ada", Some(Role::Admin));
        // The account still holds what the act began with: the guard refused it.
        assert_eq!(
            w.people.grant_refusal(
                StoreError::StateChanged,
                &w.wiki,
                &ada.id,
                Some(Role::Admin)
            ),
            PeopleError::Grant(GrantRefusal::LastAdmin)
        );
        // It holds something else now: someone changed it first.
        assert_eq!(
            w.people.grant_refusal(
                StoreError::StateChanged,
                &w.wiki,
                &ada.id,
                Some(Role::Reviewer)
            ),
            PeopleError::Changed
        );
    }
}

#[test]
fn lifting_a_protection_ends_every_one_in_force_on_the_title() {
    for kind in KINDS {
        let w = world(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        for ends in [T0 + DAY, T0 + 2 * DAY] {
            w.store
                .record_protection_event(&ProtectionEvent {
                    id: ProtectionEventId::new(),
                    wiki: w.wiki.id.clone(),
                    at: T0,
                    event: MeasureEvent::Set,
                    kind: ProtectionKind::Page,
                    target: "Help".to_owned(),
                    requirement: Some(ProtectionRequirement::Review),
                    by: Actor {
                        account: admin.id.clone(),
                        rung: Role::Admin,
                    },
                    policy_block: None,
                    reason: Some("A dispute.".to_owned()),
                    expires_at: Some(ends),
                    of_event: None,
                    appeal: None,
                })
                .unwrap();
        }
        w.authorizer
            .lift_protection(
                &admin.principal,
                &w.wiki.id,
                &crate::governance::Target::path("Help").unwrap(),
                None,
            )
            .unwrap();
        assert!(
            w.store
                .active_protections(&w.wiki.id, T0)
                .unwrap()
                .is_empty(),
            "{kind:?}: the page is unprotected after *lifted*"
        );
    }
}

#[test]
fn lifting_ends_every_restriction_in_force_on_the_person_and_not_only_the_one_that_ends_last() {
    for kind in KINDS {
        let w = world(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        // Two in force at once, which only an older state or an overturn can leave (the plain record takes both).
        for ends in [T0 + DAY, T0 + 2 * DAY] {
            w.store
                .record_restriction_event(&set_restriction(&w, &admin, &person, ends))
                .unwrap();
        }
        assert_eq!(
            w.store
                .active_restrictions(&w.wiki.id, &person.id, T0)
                .unwrap()
                .len(),
            2
        );
        w.people
            .lift(&admin.principal, &w.wiki, &person.id.to_string(), "")
            .unwrap();
        assert!(
            w.store
                .active_restrictions(&w.wiki.id, &person.id, T0)
                .unwrap()
                .is_empty(),
            "{kind:?}: the person is free after *lifted*"
        );
    }
}
