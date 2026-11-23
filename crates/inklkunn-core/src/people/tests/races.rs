//! Acts made at the same moment (handoff G-T1 correction F-2): **the check that nothing is in force and the record are one
//! step in the store**, so that two people acting at once cannot both succeed. Each test runs the pair many times on the SQLite
//! store, each pair released by one barrier; exactly one of the two succeeds and the other is refused by name.

use std::sync::{Arc, Barrier};
use std::thread;

use inklkunn_store::Role;

use super::support::{Kind, Person, T0, World};
use crate::authorize::{ProtectError, Setting};
use crate::governance::{PlatformLimits, Require, Target};
use crate::people::{GrantRefusal, PeopleError, RestrictRefusal};

/// A clock every thread reads alike.
fn steady() -> i64 {
    T0
}

const ROUNDS: usize = 60;

fn world() -> World {
    World::with_clock(
        Kind::Sqlite,
        inklkunn_store::Visibility::Public,
        None,
        PlatformLimits::default(),
        steady,
    )
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
