#![allow(clippy::expect_used, clippy::unwrap_used)]
//! **INV-14, the wiki cannot lock itself** (RFC 006 D-3, D-4; threat model INV-14; readiness gap 2): what could hold the hatch up
//! and does not. An admin of the wiki can always change its `governance/`, with no approvals and no rung beyond their own, so
//! neither a protection that names a governance path or the namespace of one, standing in the policy or put in place for a time,
//! nor a platform limit the operator lowers after the rules were accepted, may stop an admin's change to the rules. (The hatch
//! itself, its scope and its record are in `governance.rs`; the real-prikk proofs are `tests/governance_accept.rs`.)

use std::sync::atomic::{AtomicI64, Ordering};

use inklkunn_store::{ProposalRecord, Role};

use super::super::testing::Rig;
use super::*;
use crate::authorize::{Authorizer, Setting, StoreAuthz};
use crate::governance::{Entry, Limit, PlatformLimits, Refusal, Require, Target};
use crate::protection::Lack;
use crate::read::ReadService;

const HOME: &str = super::super::testing::HOME;
/// The second the clock of these tests stands at (a real-looking time: the store's own checks need a positive one).
const T0: i64 = 1_800_000_000;

fn clock() -> i64 {
    NOW.load(Ordering::SeqCst)
}

static NOW: AtomicI64 = AtomicI64::new(T0);

/// A rig whose default view holds `Home` and this policy.
fn ruled(policy: &str) -> Rig {
    let rig = Rig::new("hatch");
    rig.fake.derive_changes();
    rig.fake.set_files(&[
        ("pages/Home.md".to_owned(), HOME.to_owned()),
        ("governance/policy.yaml".to_owned(), policy.to_owned()),
    ]);
    rig.reads.forget_wiki(&rig.wiki);
    rig
}

fn refused(result: Result<Accepted, AcceptFailure>) -> AcceptFailure {
    match result {
        Err(e) => e,
        Ok(done) => panic!("accepted: {done:?}"),
    }
}

/// The record of the one acceptance of `p`'s first revision.
fn acceptance_of(rig: &Rig, p: &ProposalRecord) -> inklkunn_store::AcceptanceRecord {
    rig.store
        .acceptances_of(&p.id, 1)
        .unwrap()
        .pop()
        .unwrap()
        .acceptance
}

fn desk(rig: &Rig) -> Authorizer {
    Authorizer::new(
        Arc::new(
            StoreAuthz::new(rig.store.clone(), rig.store.clone()).with_rules(rig.reads.clone()),
        ),
        rig.audit.clone(),
    )
    .with_clock(clock)
    .with_protections(rig.store.clone(), PlatformLimits::default())
}

fn temporary(target: Target, require: Require, reason: &str) -> Setting {
    Setting {
        target,
        require,
        reason: reason.to_owned(),
        days: 7,
    }
}

/// The policy a standing protection of the rules is written in: a page whose title is the file's path, and the namespace of the
/// directory, each asking a review and the admin's rung, with `approvals` approvals asked of every change.
fn protecting_the_rules(approvals: u8) -> String {
    format!(
        "acceptance:\n  approvals: {approvals}\nprotect:\n  - path: governance/policy.yaml\n    require: review\n    reason: \"The rules are read twice.\"\n  - path: governance/constitution.yaml\n    require: review\n    reason: \"The rules are read twice.\"\n  - namespace: governance\n    require:\n      rung: admin\n    reason: \"Only an admin changes the rules.\"\n"
    )
}

#[test]
fn a_protection_that_names_the_rules_does_not_hold_the_hatch_up_and_does_not_open_it_to_anyone_else()
 {
    // The two counts that could each be the one that holds an admin: no approval asked (a protection that asks a review would
    // then ask one by itself) and three nobody can give.
    for approvals in [0_u8, 3] {
        NOW.store(T0, Ordering::SeqCst);
        let rig = ruled(&protecting_the_rules(approvals));
        let (_, admin) = rig.person("Ada", Some(Role::Admin));
        let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
        let (_, author) = rig.person("Noor", Some(Role::Contributor));
        // Put in place for a time too, by the `moderate` rung: the same names.
        let desk = desk(&rig);
        for setting in [
            temporary(
                Target::path("governance/policy.yaml").unwrap(),
                Require::Rung(Role::Admin),
                "A dispute.",
            ),
            temporary(
                Target::namespace("governance").unwrap(),
                Require::Review,
                "A dispute.",
            ),
        ] {
            desk.protect(&maintainer, &rig.wiki.id, &setting).unwrap();
        }
        // The protections are in force for the names they were written for (the control: they are no empty words).
        assert_eq!(
            desk.protections_on(&admin, &rig.wiki.id, "governance/policy.yaml")
                .len(),
            4,
            "{approvals} approvals: the policy's path and namespace, and the two put in place for a time"
        );
        let p = rig.propose(
            &author,
            "Back to the defaults",
            &[(
                "governance/policy.yaml",
                Some("acceptance:\n  approvals: 0\n"),
            )],
        );
        if approvals > 0 {
            // A maintainer is held by the approvals nobody can give; nothing opens the change to them.
            assert!(
                matches!(
                    refused(rig.acceptor().with_clock(clock).accept(
                        &maintainer,
                        &rig.wiki,
                        &p.id,
                        1
                    )),
                    AcceptFailure::ApprovalsMissing { needed: 3, have: 0 }
                ),
                "{approvals} approvals"
            );
        }
        // The admin's session: the hatch, with no approvals, whatever the policy and the protections say.
        let done = rig
            .acceptor()
            .with_clock(clock)
            .accept(&admin, &rig.wiki, &p.id, 1);
        assert!(
            matches!(done, Ok(Accepted::Signed(_))),
            "{approvals} approvals: the hatch was held up: {done:?}"
        );
        let record = acceptance_of(&rig, &p);
        assert!(record.hatch, "the record names the hatch");
        assert_eq!(record.acceptor_role, Role::Admin);
    }
}

#[test]
fn the_hatch_is_not_held_up_by_a_protection_that_asks_a_review_when_the_policy_asks_for_no_approval()
 {
    // The nearest case to the hatch's own count: `needed` is 0 for the hatch, and a protection that applied would then ask one
    // approval by itself (`Lack::Review`). The admin's own change to a rules file meets none, whatever names the protections have.
    NOW.store(T0, Ordering::SeqCst);
    let rig = ruled("entry: observer\n");
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let desk = desk(&rig);
    for target in [
        Target::path("governance/policy.yaml").unwrap(),
        Target::path("governance/constitution.yaml").unwrap(),
        Target::namespace("governance").unwrap(),
    ] {
        desk.protect(
            &admin,
            &rig.wiki.id,
            &temporary(target, Require::Review, "A dispute."),
        )
        .unwrap();
    }
    let p = rig.propose(
        &author,
        "Let everyone contribute",
        &[("governance/policy.yaml", Some("entry: contributor\n"))],
    );
    let done = rig
        .acceptor()
        .with_clock(clock)
        .accept(&admin, &rig.wiki, &p.id, 1);
    assert!(
        !matches!(
            done,
            Err(AcceptFailure::Protected {
                lack: Lack::Review,
                ..
            })
        ),
        "a protection naming a rules file held the hatch: {done:?}"
    );
    assert!(matches!(done, Ok(Accepted::Signed(_))), "{done:?}");
    // Nothing about the protection is in the record of the acceptance of the rules. (The clock is not moved on here: it is one
    // static of this file, and a test that moves it makes the others, which run beside it, see another time.)
    assert!(acceptance_of(&rig, &p).hatch);
}

/// The rules in force in `rig` as read under `limits`.
fn rig_rules(rig: &Rig, limits: PlatformLimits) -> Arc<crate::governance::Rules> {
    let reads = ReadService::new(
        rig.fake.clone(),
        rig.store.clone(),
        rig.store.clone(),
        rig.layout.clone(),
        crate::read::ReadConfig::default(),
        None,
    )
    .with_platform_limits(limits);
    reads.rules_of(&rig.wiki).unwrap()
}

/// The acceptor of `rig` reading the rules under `limits`, and nothing else changed.
fn acceptor_under(rig: &Rig, limits: PlatformLimits) -> ProposalAcceptor {
    let mut parts = rig.parts(rig.double.clone());
    parts.reads = Arc::new(
        ReadService::new(
            rig.fake.clone(),
            rig.store.clone(),
            rig.store.clone(),
            rig.layout.clone(),
            crate::read::ReadConfig::default(),
            None,
        )
        .with_platform_limits(limits),
    );
    ProposalAcceptor::new(parts).with_reconciliation(false)
}

#[test]
fn a_platform_limit_lowered_after_the_rules_were_accepted_does_not_hold_the_hatch_up() {
    // The rules were accepted under the default limits: `entry: contributor`, and three approvals nobody can give.
    let rig = ruled("entry: contributor\nacceptance:\n  approvals: 3\n");
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    // The operator then lowers the limit: the rules are read as if `entry` were at it (D-3), and say so.
    let lowered = PlatformLimits {
        entry_max: Entry::Observer,
        ..PlatformLimits::default()
    };
    let strict = acceptor_under(&rig, lowered);
    let in_force = rig_rules(&rig, lowered);
    assert_eq!(
        in_force.lowered().len(),
        1,
        "the limit lowers what the wiki's rules say: {:?}",
        in_force.lowered()
    );
    // A change that brings `entry` within the limit and takes the approvals away: a maintainer is held by the three approvals,
    // and the admin is not (the hatch: no approvals, whatever the lowered rules ask).
    let within = rig.propose(
        &author,
        "Back to what the server allows",
        &[(
            "governance/policy.yaml",
            Some("entry: observer\nacceptance:\n  approvals: 0\n"),
        )],
    );
    assert!(matches!(
        refused(strict.accept(&maintainer, &rig.wiki, &within.id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 3, have: 0 }
    ));
    assert!(matches!(
        strict.accept(&admin, &rig.wiki, &within.id, 1),
        Ok(Accepted::Signed(_))
    ));
    assert!(acceptance_of(&rig, &within).hatch);
}

#[test]
fn a_change_that_stays_above_a_lowered_limit_is_refused_naming_the_limit_even_to_an_admin_and_the_admin_lowers_it_in_the_same_change()
 {
    // What the code does, and what INV-14 says: "An admin of a wiki can always change its `governance/`". The hatch lifts the
    // rung and the approvals; it does not lift what the files may say (D-3: *a policy that exceeds a platform limit is refused at
    // acceptance, naming the limit*). So a change that leaves `entry` above the limit is refused, by name, and the same admin's
    // change that sets it within the limit is accepted: no limit leaves the wiki unable to change its rules.
    let rig = ruled("entry: contributor\nacceptance:\n  approvals: 3\n");
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let strict = acceptor_under(
        &rig,
        PlatformLimits {
            entry_max: Entry::Observer,
            ..PlatformLimits::default()
        },
    );
    let above = rig.propose(
        &author,
        "Only the approvals",
        &[(
            "governance/policy.yaml",
            Some("entry: contributor\nacceptance:\n  approvals: 0\n"),
        )],
    );
    let failure = refused(strict.accept(&admin, &rig.wiki, &above.id, 1));
    let AcceptFailure::GovernanceInvalid(faults) = &failure else {
        panic!("not a refusal of the file: {failure:?}");
    };
    assert_eq!(
        faults.first().map(|f| f.refusal),
        Some(Refusal::PlatformLimit(Limit::EntryMax))
    );
    assert!(rig.double.calls().is_empty(), "nothing was signed");
    let within = rig.propose(
        &author,
        "Within the limit",
        &[(
            "governance/policy.yaml",
            Some("entry: observer\nacceptance:\n  approvals: 0\n"),
        )],
    );
    assert!(matches!(
        strict.accept(&admin, &rig.wiki, &within.id, 1),
        Ok(Accepted::Signed(_))
    ));
}
