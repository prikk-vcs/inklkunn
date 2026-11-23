//! Acts made at the same moment (handoff G-T1 correction F-2): **the check that nothing is in force and the record are one
//! step in the store**, so that two people acting at once cannot both succeed. Each test runs the pair many times on the SQLite
//! store, each pair released by one barrier; exactly one of the two succeeds and the other is refused by name.

use std::sync::{Arc, Barrier};
use std::thread;

use inklkunn_store::Role;

use inklkunn_store::{AuditEvent, AuditOutcome, MemoryAudit};

use super::support::{HookedAudit, Kind, Person, World, steady, steady_world};
use crate::authorize::{Authorizer, ProtectError, Setting};
use crate::governance::{Require, Target};
use crate::people::{GrantRefusal, PeopleError, PeopleService, RestrictRefusal};

const ROUNDS: usize = 60;

fn world() -> World {
    steady_world(Kind::Sqlite)
}

/// Run `a` and `b` at the same moment and give what each answered.
fn together<A: Send + 'static, B: Send + 'static>(
    a: impl FnOnce() -> A + Send + 'static,
    b: impl FnOnce() -> B + Send + 'static,
) -> (A, B) {
    let gate = Arc::new(Barrier::new(2));
    let (g1, g2) = (gate.clone(), gate);
    let one = thread::spawn(move || {
        g1.wait();
        a()
    });
    let two = thread::spawn(move || {
        g2.wait();
        b()
    });
    (one.join().expect("a thread"), two.join().expect("a thread"))
}

#[test]
fn two_moderators_restricting_one_person_at_once_leave_one_restriction_and_the_late_one_is_told_it_is_there()
 {
    let w = world();
    let admin = w.person("Ada", Some(Role::Admin));
    let admin_two = w.person("Ben", Some(Role::Admin));
    for round in 0..ROUNDS {
        let person = w.person(&format!("Pat{round}"), Some(Role::Contributor));
        let act = |by: &Person| {
            let (people, wiki, who, principal) = (
                w.people.clone(),
                w.wiki.clone(),
                person.id.to_string(),
                by.principal.clone(),
            );
            move || people.restrict(&principal, &wiki, &who, 3, "Edit-warring.")
        };
        let (a, b) = together(act(&admin), act(&admin_two));
        let results = [a, b];
        let made = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(made, 1, "round {round}: {results:?}");
        assert!(
            results.iter().any(|r| matches!(
                r,
                Err(PeopleError::Restrict(
                    RestrictRefusal::AlreadyRestricted { .. }
                ))
            )),
            "round {round}: refused by name, {results:?}"
        );
        let active = w
            .store
            .active_restrictions(&w.wiki.id, &person.id, steady())
            .unwrap();
        assert_eq!(active.len(), 1, "round {round}");
    }
}

#[test]
fn two_moderators_protecting_one_page_at_once_leave_one_protection_and_the_late_one_is_told_it_is_there()
 {
    let w = world();
    let admin = w.person("Ada", Some(Role::Admin));
    let admin_two = w.person("Ben", Some(Role::Admin));
    for round in 0..ROUNDS {
        let title = format!("Page{round}");
        let act = |by: &Person| {
            let (authorizer, wiki, principal, title) = (
                w.authorizer.clone(),
                w.wiki.id.clone(),
                by.principal.clone(),
                title.clone(),
            );
            move || {
                authorizer.protect(
                    &principal,
                    &wiki,
                    &Setting {
                        target: Target::path(&title).expect("a title"),
                        require: Require::Review,
                        reason: "A dispute.".to_owned(),
                        days: 3,
                    },
                )
            }
        };
        let (a, b) = together(act(&admin), act(&admin_two));
        let results = [a, b];
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "round {round}: {results:?}"
        );
        assert!(
            results
                .iter()
                .any(|r| matches!(r, Err(ProtectError::AlreadyInForce))),
            "round {round}: refused by name, {results:?}"
        );
        let active = w
            .store
            .active_protections(&w.wiki.id, steady())
            .unwrap()
            .into_iter()
            .filter(|p| p.target == title)
            .count();
        assert_eq!(active, 1, "round {round}");
    }
}

#[test]
fn two_admins_demoting_each_other_at_once_leave_one_admin_and_the_late_one_is_told_it_is_the_last()
{
    for round in 0..ROUNDS {
        let w = world();
        let ada = w.person("Ada", Some(Role::Admin));
        let ben = w.person("Ben", Some(Role::Admin));
        let demote = |by: &Person, target: &Person| {
            let (people, wiki, who, principal) = (
                w.people.clone(),
                w.wiki.clone(),
                target.id.to_string(),
                by.principal.clone(),
            );
            move || people.grant(&principal, &wiki, &who, Some(Role::Contributor), "")
        };
        let (a, b) = together(demote(&ada, &ben), demote(&ben, &ada));
        let results = [a, b];
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "round {round}: {results:?}"
        );
        // The one that was late is refused by name when it met the guard, and is not allowed at all when the other's act had
        // been recorded before it was authorized (the act is then not made twice either).
        assert!(
            results.iter().any(|r| matches!(
                r,
                Err(PeopleError::Grant(GrantRefusal::LastAdmin) | PeopleError::NotAllowed)
            )),
            "round {round}: refused, {results:?}"
        );
        let admins = [&ada, &ben]
            .iter()
            .filter(|p| w.grant_of(p) == Some(Role::Admin))
            .count();
        assert_eq!(admins, 1, "round {round}: an admin is left");
    }
}

// ------------------------------------------------------------------------------------ N-1: renew and lift

/// An act, by name, that both of two threads reach before either records: the trail's event of the act is written *allowed*
/// after the record in force was read and before the record is made, so a barrier there makes the two read the same record.
fn met_at(
    action: &'static str,
    gate: Arc<Barrier>,
) -> impl Fn(&AuditEvent) + Send + Sync + 'static {
    move |event| {
        if event.action == action && event.outcome == AuditOutcome::Allowed {
            gate.wait();
        }
    }
}

fn people_meeting(w: &World, action: &'static str) -> Arc<PeopleService> {
    let sink = HookedAudit {
        inner: Arc::new(MemoryAudit::new()),
        hook: Box::new(met_at(action, Arc::new(Barrier::new(2)))),
    };
    Arc::new(PeopleService::new(
        w.authorizer.clone(),
        w.store.clone(),
        w.store.clone(),
        w.store.clone(),
        Arc::new(sink),
        w.limits,
    ))
}

fn authorizer_meeting(w: &World, action: &'static str) -> Authorizer {
    let sink = HookedAudit {
        inner: Arc::new(MemoryAudit::new()),
        hook: Box::new(met_at(action, Arc::new(Barrier::new(2)))),
    };
    Authorizer::new(w.source.clone(), Arc::new(sink))
        .with_clock(w.clock)
        .with_protections(w.store.clone(), w.limits)
}

/// Of two answers, exactly one succeeded and the other says **the record changed** by name.
fn one_won_one_changed<T: std::fmt::Debug, E: std::fmt::Debug>(
    round: usize,
    results: &[Result<T, E>; 2],
    changed: impl Fn(&E) -> bool,
) {
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "round {round}: exactly one wins, {results:?}"
    );
    assert!(
        results
            .iter()
            .any(|r| r.as_ref().err().is_some_and(&changed)),
        "round {round}: the other is told it changed, by name, {results:?}"
    );
}

#[test]
fn two_moderators_renewing_one_restriction_at_once_leave_one_renewal_and_the_late_one_is_told_it_changed()
 {
    let w = world();
    let admin = w.person("Ada", Some(Role::Admin));
    let admin_two = w.person("Ben", Some(Role::Admin));
    for round in 0..ROUNDS {
        let people = people_meeting(&w, "renew-restriction");
        let person = w.person(&format!("Pat{round}"), Some(Role::Contributor));
        w.people
            .restrict(
                &admin.principal,
                &w.wiki,
                &person.id.to_string(),
                3,
                "Edit-warring.",
            )
            .unwrap();
        let act = |by: &Person| {
            let (people, wiki, who, principal) = (
                people.clone(),
                w.wiki.clone(),
                person.id.to_string(),
                by.principal.clone(),
            );
            move || people.renew(&principal, &wiki, &who, 5, "Still at it.")
        };
        let (a, b) = together(act(&admin), act(&admin_two));
        one_won_one_changed(round, &[a, b], |e| matches!(e, PeopleError::Changed));
    }
}

#[test]
fn two_moderators_lifting_one_restriction_at_once_leave_one_lifting_and_the_late_one_is_told_it_changed()
 {
    let w = world();
    let admin = w.person("Ada", Some(Role::Admin));
    let admin_two = w.person("Ben", Some(Role::Admin));
    for round in 0..ROUNDS {
        let people = people_meeting(&w, "lift-restriction");
        let person = w.person(&format!("Pat{round}"), Some(Role::Contributor));
        w.people
            .restrict(
                &admin.principal,
                &w.wiki,
                &person.id.to_string(),
                3,
                "Edit-warring.",
            )
            .unwrap();
        let act = |by: &Person| {
            let (people, wiki, who, principal) = (
                people.clone(),
                w.wiki.clone(),
                person.id.to_string(),
                by.principal.clone(),
            );
            move || people.lift(&principal, &wiki, &who, "Settled.")
        };
        let (a, b) = together(act(&admin), act(&admin_two));
        one_won_one_changed(round, &[a, b], |e| matches!(e, PeopleError::Changed));
    }
}

fn setting(title: &str) -> Setting {
    Setting {
        target: Target::path(title).expect("a title"),
        require: Require::Review,
        reason: "A dispute.".to_owned(),
        days: 3,
    }
}

#[test]
fn two_moderators_renewing_one_protection_at_once_leave_one_renewal_and_the_late_one_is_told_it_changed()
 {
    let w = world();
    let admin = w.person("Ada", Some(Role::Admin));
    let admin_two = w.person("Ben", Some(Role::Admin));
    for round in 0..ROUNDS {
        let authorizer = authorizer_meeting(&w, "protection-renewed");
        let title = format!("Page{round}");
        w.authorizer
            .protect(&admin.principal, &w.wiki.id, &setting(&title))
            .unwrap();
        let act = |by: &Person| {
            let (authorizer, wiki, principal, title) = (
                authorizer.clone(),
                w.wiki.id.clone(),
                by.principal.clone(),
                title.clone(),
            );
            move || authorizer.renew_protection(&principal, &wiki, &setting(&title))
        };
        let (a, b) = together(act(&admin), act(&admin_two));
        one_won_one_changed(round, &[a, b], |e| matches!(e, ProtectError::Changed));
    }
}

#[test]
fn two_moderators_lifting_one_protection_at_once_leave_one_lifting_and_the_late_one_is_told_it_changed()
 {
    let w = world();
    let admin = w.person("Ada", Some(Role::Admin));
    let admin_two = w.person("Ben", Some(Role::Admin));
    for round in 0..ROUNDS {
        let authorizer = authorizer_meeting(&w, "protection-lifted");
        let title = format!("Page{round}");
        w.authorizer
            .protect(&admin.principal, &w.wiki.id, &setting(&title))
            .unwrap();
        let act = |by: &Person| {
            let (authorizer, wiki, principal, title) = (
                authorizer.clone(),
                w.wiki.id.clone(),
                by.principal.clone(),
                title.clone(),
            );
            move || {
                authorizer.lift_protection(
                    &principal,
                    &wiki,
                    &Target::path(&title).expect("a title"),
                    Some("Settled."),
                )
            }
        };
        let (a, b) = together(act(&admin), act(&admin_two));
        one_won_one_changed(round, &[a, b], |e| matches!(e, ProtectError::Changed));
    }
}
