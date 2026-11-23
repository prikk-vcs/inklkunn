#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Protections (RFC 006 D-7; handoff G-T3; acceptance criterion 7) against the in-memory repository and a scripted signer:
//! **the check is `authorize()`'s and the acceptance's, and no template is in sight.** A temporary protection is set by the
//! `moderate` rung, shown with its reason, refuses what it should and ends by itself by the clock; a standing one does the same
//! through the policy; both at once, the stricter decides and both are listed. (The criteria that need the repository to hold
//! the policy as a signed change run on a real prikk in `tests/protections.rs`.)

use std::sync::atomic::{AtomicI64, Ordering};

use inklkunn_store::{
    DecisionId, DecisionKind, DecisionRecord, GovernanceStore, MeasureEvent, ProposalState,
    ProposalStore, Role, Visibility,
};

use super::super::testing::Rig;
use super::*;
use crate::authorize::{Action, Authorizer, ProtectError, Resource, Setting, StoreAuthz};
use crate::governance::{PlatformLimits, Require, Target};
use crate::protection::{Kind, Lack, Notice};

const HOME: &str = super::super::testing::HOME;
const DAY: i64 = 86_400;
/// The one second the clocks of these tests start at (a real-looking time; the store's own checks need a positive one).
const T0: i64 = 1_800_000_000;

fn page(title: &str, id: &str) -> String {
    format!("---\ntitle: {title}\nid: {id}\n---\n\nOne.\n\nTwo.\n")
}

fn changed(text: &str) -> String {
    text.replace("One.", "Uno.")
}

/// A rig whose default view holds these files besides `Home`, with the policy `policy` when it is not empty.
fn rig_with(files: &[(&str, &str)], policy: &str) -> Rig {
    let rig = Rig::new("protect");
    rig.fake.derive_changes();
    let mut written: Vec<(String, String)> = vec![("pages/Home.md".to_owned(), HOME.to_owned())];
    written.extend(
        files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), (*t).to_owned())),
    );
    if !policy.is_empty() {
        written.push(("governance/policy.yaml".to_owned(), policy.to_owned()));
    }
    rig.fake.set_files(&written);
    rig.reads.forget_wiki(&rig.wiki);
    rig
}

/// The authorizer of the rig, with temporary protections kept in its store, whose clock is `clock` and whose platform limit
/// is `limit_days`.
fn desk(rig: &Rig, clock: fn() -> i64, limit_days: u32) -> Authorizer {
    Authorizer::new(
        Arc::new(
            StoreAuthz::new(rig.store.clone(), rig.store.clone()).with_rules(rig.reads.clone()),
        ),
        rig.audit.clone(),
    )
    .with_clock(clock)
    .with_protections(
        rig.store.clone(),
        PlatformLimits {
            limit_days,
            ..PlatformLimits::default()
        },
    )
}

fn setting(title: &str, require: Require, reason: &str, days: u32) -> Setting {
    Setting {
        target: Target::path(title).unwrap(),
        require,
        reason: reason.to_owned(),
        days,
    }
}

fn namespace_setting(title: &str, require: Require, reason: &str, days: u32) -> Setting {
    Setting {
        target: Target::namespace(title).unwrap(),
        ..setting("X", require, reason, days)
    }
}

fn refused(result: Result<Accepted, AcceptFailure>) -> AcceptFailure {
    match result {
        Err(e) => e,
        Ok(done) => panic!("accepted: {done:?}"),
    }
}

fn approve(rig: &Rig, who: &Principal, id: &ProposalId) {
    rig.proposals
        .decide(who, &rig.wiki, id, 1, DecisionKind::Approve, "")
        .unwrap_or_else(|e| panic!("approve: {e}"));
}

/// A rig and the people who matter: a maintainer (who moderates and accepts), an admin, a reviewer, a second reviewer and a
/// contributor who proposes.
struct Cast {
    rig: Rig,
    maintainer: Principal,
    admin: Principal,
    reviewer: Principal,
    second: Principal,
    author: Principal,
}

fn cast(files: &[(&str, &str)], policy: &str) -> Cast {
    let rig = rig_with(files, policy);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, reviewer) = rig.person("Rin", Some(Role::Reviewer));
    let (_, second) = rig.person("Sam", Some(Role::Reviewer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    Cast {
        rig,
        maintainer,
        admin,
        reviewer,
        second,
        author,
    }
}

impl Cast {
    fn resource(&self, path: &str) -> Resource {
        Resource::Page {
            wiki: self.rig.wiki.id.clone(),
            path: path.to_owned(),
        }
    }

    fn propose_home(&self) -> ProposalRecord {
        self.rig.propose(
            &self.author,
            "Say it in Spanish",
            &[("pages/Home.md", Some(&changed(HOME)))],
        )
    }

    fn accept(
        &self,
        at: fn() -> i64,
        who: &Principal,
        p: &ProposalRecord,
    ) -> Result<Accepted, AcceptFailure> {
        self.rig
            .acceptor()
            .with_clock(at)
            .accept(who, &self.rig.wiki, &p.id, 1)
    }
}

// -------------------------------------------------------------------------------------------- the three acts

#[test]
fn the_moderate_rung_puts_a_protection_in_place_and_it_is_shown_with_its_kind_reason_requirement_and_end()
 {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    desk.protect(
        &c.maintainer,
        &c.rig.wiki.id,
        &setting("Home", Require::Review, "Vandalism this week.", 7),
    )
    .unwrap();
    // Anyone who may read the wiki is told, in one display.
    for who in [&c.author, &c.reviewer, &Principal::Anonymous] {
        let notices = desk.protections_on(who, &c.rig.wiki.id, "Home");
        let [notice] = notices.as_slice() else {
            panic!("one protection: {notices:?}");
        };
        assert_eq!(
            notice.kind(),
            Kind::Temporary {
                until: T0 + 7 * DAY
            }
        );
        assert_eq!(notice.reason(), "Vandalism this week.");
        assert_eq!(notice.require(), Require::Review);
        assert_eq!(notice.target(), &Target::Path("Home".to_owned()));
    }
    // Not shown on another page.
    assert!(
        desk.protections_on(&c.author, &c.rig.wiki.id, "Elsewhere")
            .is_empty()
    );
    // The record names who acted, at which rung, under which rules, and follows nothing.
    let record = c
        .rig
        .store
        .protection_events(&c.rig.wiki.id, 10)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(record.event, MeasureEvent::Set);
    assert_eq!(record.by.rung, Role::Maintainer);
    assert_eq!(record.policy_block, None);
    assert_eq!(record.target, "Home");
}

#[test]
fn only_the_moderate_rung_sets_renews_or_lifts_a_protection_and_the_policy_names_that_rung() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    let ask = setting("Home", Require::Review, "Why.", 1);
    for who in [&c.reviewer, &c.author, &Principal::Anonymous] {
        assert_eq!(
            desk.protect(who, &c.rig.wiki.id, &ask),
            Err(ProtectError::NotAllowed)
        );
    }
    // A stranger of another wiki is told no more than anyone is.
    let elsewhere = inklkunn_store::WikiId::new();
    assert_eq!(
        desk.protect(&c.maintainer, &elsewhere, &ask),
        Err(ProtectError::NotAllowed)
    );
    desk.protect(&c.maintainer, &c.rig.wiki.id, &ask).unwrap();
    for who in [&c.reviewer, &c.author] {
        assert_eq!(
            desk.renew_protection(who, &c.rig.wiki.id, &ask),
            Err(ProtectError::NotAllowed)
        );
        assert_eq!(
            desk.lift_protection(who, &c.rig.wiki.id, &ask.target, None),
            Err(ProtectError::NotAllowed)
        );
    }
    // The rung is the policy's: a wiki that says only an admin moderates does not let a maintainer.
    let strict = cast(&[], "rungs:\n  moderate: admin\n");
    let desk = self::desk(&strict.rig, clock, 90);
    assert_eq!(
        desk.protect(&strict.maintainer, &strict.rig.wiki.id, &ask),
        Err(ProtectError::NotAllowed)
    );
    desk.protect(&strict.admin, &strict.rig.wiki.id, &ask)
        .unwrap();
}

#[test]
fn a_protection_lasts_at_most_the_platforms_limit_and_at_least_a_day() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 30);
    let wiki = &c.rig.wiki.id;
    assert_eq!(
        desk.protect(
            &c.maintainer,
            wiki,
            &setting("Home", Require::Review, "Why.", 31)
        ),
        Err(ProtectError::TooLong { limit_days: 30 })
    );
    assert_eq!(
        desk.protect(
            &c.maintainer,
            wiki,
            &setting("Home", Require::Review, "Why.", 0)
        ),
        Err(ProtectError::NoDays)
    );
    assert!(c.rig.store.protection_events(wiki, 10).unwrap().is_empty());
    desk.protect(
        &c.maintainer,
        wiki,
        &setting("Home", Require::Review, "Why.", 30),
    )
    .unwrap();
    let notices = desk.protections_on(&c.author, wiki, "Home");
    assert_eq!(notices.first().and_then(Notice::until), Some(T0 + 30 * DAY));
    // The limit applies to a renewal too.
    assert_eq!(
        desk.renew_protection(
            &c.maintainer,
            wiki,
            &setting("Home", Require::Review, "Why.", 31)
        ),
        Err(ProtectError::TooLong { limit_days: 30 })
    );
}

#[test]
fn a_protection_needs_a_title_and_a_reason_that_can_be_shown() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    let wiki = &c.rig.wiki.id;
    for reason in [
        "",
        "   ",
        "two\nlines",
        "bidi \u{202e} text",
        &"x".repeat(301),
    ] {
        assert_eq!(
            desk.protect(
                &c.maintainer,
                wiki,
                &setting("Home", Require::Review, reason, 1)
            ),
            Err(ProtectError::BadReason),
            "{reason:?}"
        );
    }
    // A title in a form no page has (`Help/`) is no title of a protection, and says so before anything of the wiki is asked.
    let bad = Setting {
        target: Target::Path("Help/".to_owned()),
        ..setting("Home", Require::Review, "Why.", 1)
    };
    assert_eq!(
        desk.protect(&Principal::Anonymous, wiki, &bad),
        Err(ProtectError::BadTarget)
    );
    assert_eq!(
        desk.protect(&c.maintainer, wiki, &bad),
        Err(ProtectError::BadTarget)
    );
    assert!(c.rig.store.protection_events(wiki, 10).unwrap().is_empty());
}

#[test]
fn renewing_extends_it_lifting_ends_it_and_neither_works_on_nothing() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    let wiki = &c.rig.wiki.id;
    let first = setting("Home", Require::Review, "Dispute.", 2);
    assert_eq!(
        desk.renew_protection(&c.maintainer, wiki, &first),
        Err(ProtectError::NotInForce)
    );
    assert_eq!(
        desk.lift_protection(&c.maintainer, wiki, &first.target, None),
        Err(ProtectError::NotInForce)
    );
    let set = desk.protect(&c.maintainer, wiki, &first).unwrap();
    // A second setting of what is in force is a renewal, and says so.
    assert_eq!(
        desk.protect(&c.maintainer, wiki, &first),
        Err(ProtectError::AlreadyInForce)
    );
    // Renewing: a new requirement, reason and end, one in force, the record following the one it renews.
    let renewed = desk
        .renew_protection(
            &c.admin,
            wiki,
            &setting("Home", Require::Rung(Role::Admin), "Still disputed.", 10),
        )
        .unwrap();
    let notices = desk.protections_on(&c.author, wiki, "Home");
    let [notice] = notices.as_slice() else {
        panic!("one protection in force: {notices:?}");
    };
    assert_eq!(notice.until(), Some(T0 + 10 * DAY));
    assert_eq!(notice.reason(), "Still disputed.");
    assert_eq!(notice.require(), Require::Rung(Role::Admin));
    let renewal = c.rig.store.protection_event(&renewed).unwrap().unwrap();
    assert_eq!(renewal.event, MeasureEvent::Renewed);
    assert_eq!(renewal.of_event, Some(set));
    // Lifting ends it before its end, with the reason when there is one; the page is told no more.
    let lifted = desk
        .lift_protection(
            &c.maintainer,
            wiki,
            &first.target,
            Some("The dispute is over."),
        )
        .unwrap();
    assert!(desk.protections_on(&c.author, wiki, "Home").is_empty());
    let lifting = c.rig.store.protection_event(&lifted).unwrap().unwrap();
    assert_eq!(
        (lifting.event, lifting.of_event),
        (MeasureEvent::Lifted, Some(renewed))
    );
    assert_eq!(lifting.reason.as_deref(), Some("The dispute is over."));
    // And it can be put in place again.
    desk.protect(&c.maintainer, wiki, &first).unwrap();
    assert_eq!(desk.protections_on(&c.author, wiki, "Home").len(), 1);
}

#[test]
fn setting_renewing_and_lifting_are_audited_with_the_actor_the_rung_and_the_block() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "acceptance:\n  approvals: 1\n");
    let block = c
        .rig
        .reads
        .rules_of(&c.rig.wiki)
        .unwrap()
        .short_block()
        .to_owned();
    let desk = desk(&c.rig, clock, 90);
    let wiki = &c.rig.wiki.id;
    let ask = setting("Home", Require::Review, "Why.", 3);
    let Principal::Account(maya) = &c.maintainer else {
        panic!("an account");
    };
    desk.protect(&c.maintainer, wiki, &ask).unwrap();
    desk.renew_protection(&c.maintainer, wiki, &ask).unwrap();
    desk.lift_protection(&c.maintainer, wiki, &ask.target, None)
        .unwrap();
    // A refusal is audited too, and says who and at which rung.
    assert_eq!(
        desk.protect(&c.reviewer, wiki, &ask),
        Err(ProtectError::NotAllowed)
    );
    let events = c.rig.audit.events();
    let acts: Vec<(&str, &str, String)> = events
        .iter()
        .filter(|e| e.action.starts_with("protection-"))
        .map(|e| (e.action.as_str(), e.principal.as_str(), e.detail.clone()))
        .collect();
    let detail = format!("rung=maintainer; policy={block}");
    let who = format!("account:{}", maya.id());
    assert_eq!(
        acts,
        vec![
            ("protection-set", who.as_str(), detail.clone()),
            ("protection-renewed", who.as_str(), detail.clone()),
            ("protection-lifted", who.as_str(), detail.clone()),
        ]
    );
    // The decision to moderate is recorded beside each act: allowed for the maintainer, denied for the reviewer.
    let decisions: Vec<(String, String)> = events
        .iter()
        .filter(|e| e.action == "moderate")
        .map(|e| (e.outcome.as_str().to_owned(), e.detail.clone()))
        .collect();
    assert_eq!(decisions.len(), 4, "{decisions:?}");
    assert_eq!(decisions.last().map(|d| d.0.as_str()), Some("denied"));
    assert_eq!(
        decisions.last().map(|d| d.1.as_str()),
        Some(format!("rung=reviewer; policy={block}").as_str())
    );
}

// ------------------------------------------------------------------------------------ what it refuses, and when

#[test]
fn a_temporary_protection_that_requires_review_holds_the_acceptance_until_someone_other_than_the_proposer_approves()
 {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    desk.protect(
        &c.maintainer,
        &c.rig.wiki.id,
        &setting("Home", Require::Review, "Vandalism.", 7),
    )
    .unwrap();
    let p = c.propose_home();
    // The proposer is not stopped from proposing: the protection is about acceptance.
    assert!(
        desk.authorize(&c.author, Action::Propose, &home_for(&c))
            .is_allowed()
    );
    let got = refused(c.accept(clock, &c.maintainer, &p));
    let AcceptFailure::Protected { notices, lack } = &got else {
        panic!("not protected: {got:?}");
    };
    assert_eq!(*lack, Lack::Review);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices.first().map(Notice::reason), Some("Vandalism."));
    assert_eq!(got.word(), "protected");
    // Nothing was signed, nothing recorded as accepted, the proposal is as it was.
    assert!(c.rig.double.calls().is_empty());
    assert!(c.rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    assert_eq!(c.rig.state(&p.id), ProposalState::HeldForReview);
    // A request for changes is not an approval.
    c.rig
        .proposals
        .decide(
            &c.reviewer,
            &c.rig.wiki,
            &p.id,
            1,
            DecisionKind::RequestChanges,
            "Not yet",
        )
        .unwrap();
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &p)),
        AcceptFailure::Protected {
            lack: Lack::Review,
            ..
        }
    ));
    // One approval of someone else who holds the review rung is enough.
    approve(&c.rig, &c.second, &p.id);
    assert!(matches!(
        c.accept(clock, &c.maintainer, &p),
        Ok(Accepted::Signed(_))
    ));
    assert_eq!(c.rig.state(&p.id), ProposalState::Accepted);
}

fn home_for(c: &Cast) -> Resource {
    c.resource("pages/Home.md")
}

#[test]
fn the_proposers_own_approval_does_not_satisfy_a_protections_review() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    desk.protect(
        &c.maintainer,
        &c.rig.wiki.id,
        &setting("Home", Require::Review, "Why.", 7),
    )
    .unwrap();
    let (proposer_id, proposer) = c.rig.person("Rae", Some(Role::Reviewer));
    let text = changed(HOME);
    let p = c
        .rig
        .propose(&proposer, "My own", &[("pages/Home.md", Some(&text))]);
    // A record that reached the store another way (the service itself refuses a proposer's own decision) is still not counted.
    c.rig
        .store
        .add_decision(&DecisionRecord {
            id: DecisionId::new(),
            proposal: p.id.clone(),
            revision: 1,
            reviewer: proposer_id,
            kind: DecisionKind::Approve,
            body: String::new(),
            reviewer_rung: Some(Role::Reviewer),
            policy_block: None,
            created_at: 5,
        })
        .unwrap();
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &p)),
        AcceptFailure::Protected {
            lack: Lack::Review,
            ..
        }
    ));
}

#[test]
fn a_review_protection_asks_one_approval_even_where_the_policy_asks_none_and_the_greater_of_the_two_applies()
 {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    // The policy asks two approvals: the protection adds nothing, and the policy's refusal is the one given.
    let c = cast(&[], "acceptance:\n  approvals: 2\n");
    let desk = desk(&c.rig, clock, 90);
    desk.protect(
        &c.maintainer,
        &c.rig.wiki.id,
        &setting("Home", Require::Review, "Why.", 7),
    )
    .unwrap();
    let p = c.propose_home();
    assert_eq!(
        refused(c.accept(clock, &c.maintainer, &p)),
        AcceptFailure::ApprovalsMissing { needed: 2, have: 0 }
    );
    approve(&c.rig, &c.reviewer, &p.id);
    assert_eq!(
        refused(c.accept(clock, &c.maintainer, &p)),
        AcceptFailure::ApprovalsMissing { needed: 2, have: 1 }
    );
    approve(&c.rig, &c.second, &p.id);
    assert!(matches!(
        c.accept(clock, &c.maintainer, &p),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_disabled_accounts_approval_does_not_satisfy_a_protections_review() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let rig = rig_with(&[], "");
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let (reviewer_id, reviewer) = rig.person("Rin", Some(Role::Reviewer));
    let desk = desk(&rig, clock, 90);
    desk.protect(
        &maintainer,
        &rig.wiki.id,
        &setting("Home", Require::Review, "Why.", 7),
    )
    .unwrap();
    let p = rig.propose(
        &author,
        "Say it",
        &[("pages/Home.md", Some(&changed(HOME)))],
    );
    approve(&rig, &reviewer, &p.id);
    rig.store.set_disabled(&reviewer_id, true).unwrap();
    let acceptor = rig.acceptor().with_clock(clock);
    assert!(matches!(
        refused(acceptor.accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::Protected {
            lack: Lack::Review,
            ..
        }
    ));
    rig.store.set_disabled(&reviewer_id, false).unwrap();
    assert!(matches!(
        acceptor.accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_protection_that_restricts_acceptance_to_a_rung_refuses_below_it_and_the_admin_is_not_exempt_from_a_higher_one()
 {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    let wiki = &c.rig.wiki.id;
    desk.protect(
        &c.maintainer,
        wiki,
        &setting("Home", Require::Rung(Role::Admin), "Only the admin.", 7),
    )
    .unwrap();
    // `authorize()` does not look at protections (F-3): the acceptance's `judge` is the one place that enforces them, and a
    // protection limits acceptance only. Another page, proposing and reading are as they were.
    let home = home_for(&c);
    assert!(
        desk.authorize(&c.maintainer, Action::Accept, &home)
            .is_allowed(),
        "the rule of authorize() is the policy's, whatever protects the page"
    );
    assert!(
        desk.authorize(&c.author, Action::Propose, &home)
            .is_allowed()
    );
    assert!(desk.authorize(&c.author, Action::Read, &home).is_allowed());
    // The acceptance refuses and names the protection.
    let p = c.propose_home();
    match refused(c.accept(clock, &c.maintainer, &p)) {
        AcceptFailure::Protected {
            notices,
            lack: Lack::Rung(Role::Admin),
        } => assert_eq!(notices.first().map(Notice::reason), Some("Only the admin.")),
        other => panic!("{other:?}"),
    }
    assert!(c.rig.double.calls().is_empty());
    assert!(matches!(
        c.accept(clock, &c.admin, &p),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn the_admin_is_held_to_a_protection_the_hatch_covers_no_page() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    // `rungs.accept: admin` and a protection of the same page that asks a review: the admin needs the approval too.
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    desk.protect(
        &c.admin,
        &c.rig.wiki.id,
        &setting("Home", Require::Review, "Why.", 7),
    )
    .unwrap();
    let p = c.propose_home();
    assert!(matches!(
        refused(c.accept(clock, &c.admin, &p)),
        AcceptFailure::Protected {
            lack: Lack::Review,
            ..
        }
    ));
    // An admin may lift it (the `moderate` rung can): then the acceptance goes through.
    desk.lift_protection(
        &c.admin,
        &c.rig.wiki.id,
        &Target::Path("Home".to_owned()),
        None,
    )
    .unwrap();
    assert!(matches!(
        c.accept(clock, &c.admin, &p),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_temporary_protection_ends_by_itself_the_second_before_it_still_holds_and_the_instant_of_its_expiry_it_does_not()
 {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    let wiki = &c.rig.wiki.id;
    desk.protect(
        &c.maintainer,
        wiki,
        &setting("Home", Require::Rung(Role::Admin), "Dispute.", 1),
    )
    .unwrap();
    let until = T0 + DAY;
    let p = c.propose_home();
    // The second before: refused by the acceptance, and shown.
    NOW.store(until - 1, Ordering::SeqCst);
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &p)),
        AcceptFailure::Protected { .. }
    ));
    assert_eq!(desk.protections_on(&c.author, wiki, "Home").len(), 1);
    // The instant of expiry: nothing is written, and the protection is gone from every reading of it.
    let records = c.rig.store.protection_events(wiki, 10).unwrap().len();
    NOW.store(until, Ordering::SeqCst);
    assert!(desk.protections_on(&c.author, wiki, "Home").is_empty());
    assert!(matches!(
        c.accept(clock, &c.maintainer, &p),
        Ok(Accepted::Signed(_))
    ));
    assert_eq!(
        c.rig.store.protection_events(wiki, 10).unwrap().len(),
        records
    );
}

#[test]
fn a_protection_set_between_the_early_judgement_and_the_lease_is_seen_under_the_lease() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    // Set after the early judgement, before the signer: the lease's judgement sees it.
    let d = cast(&[], "");
    let desk = self::desk(&d.rig, clock, 90);
    let q = d.propose_home();
    let maya = d.maintainer.clone();
    let wiki = d.rig.wiki.id.clone();
    let accepted = d
        .rig
        .acceptor()
        .with_clock(clock)
        .with_observer(Arc::new(move |step| {
            if step == crate::acceptance::AcceptStep::LeaseTaken {
                desk.protect(
                    &maya,
                    &wiki,
                    &setting(
                        "Home",
                        Require::Rung(Role::Admin),
                        "Set at the last moment.",
                        1,
                    ),
                )
                .unwrap();
            }
        }))
        .accept(&d.maintainer, &d.rig.wiki, &q.id, 1);
    assert!(
        matches!(
            accepted,
            Err(AcceptFailure::Protected {
                lack: Lack::Rung(Role::Admin),
                ..
            })
        ),
        "{accepted:?}"
    );
    assert!(d.rig.double.calls().is_empty());
    assert_eq!(d.rig.state(&q.id), ProposalState::HeldForReview);
}

// ------------------------------------------------------------------------------- a standing protection, the policy's

#[test]
fn a_standing_protection_does_the_same_through_the_policy_and_is_shown_as_by_this_wikis_rules() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let help = page("Start", "p-start");
    let c = cast(
        &[("pages/Help/Start.md", &help)],
        "protect:\n  - namespace: Help\n    require:\n      rung: admin\n    reason: \"The help is read by everyone.\"\n  - path: Home\n    require: review\n    reason: \"The front page is looked at twice.\"\n",
    );
    let desk = desk(&c.rig, clock, 90);
    let wiki = &c.rig.wiki.id;
    let notices = desk.protections_on(&c.author, wiki, "Help/Start");
    let [notice] = notices.as_slice() else {
        panic!("{notices:?}");
    };
    assert_eq!(notice.kind(), Kind::Standing);
    assert_eq!(notice.reason(), "The help is read by everyone.");
    assert_eq!(notice.until(), None);
    // Refuses a maintainer, accepts an admin.
    let p = c.rig.propose(
        &c.author,
        "Fix the help",
        &[("pages/Help/Start.md", Some(&changed(&help)))],
    );
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &p)),
        AcceptFailure::Protected {
            lack: Lack::Rung(Role::Admin),
            ..
        }
    ));
    assert!(matches!(
        c.accept(clock, &c.admin, &p),
        Ok(Accepted::Signed(_))
    ));
    // The review one holds the front page until someone else approves.
    let home = c.propose_home();
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &home)),
        AcceptFailure::Protected {
            lack: Lack::Review,
            ..
        }
    ));
    approve(&c.rig, &c.reviewer, &home.id);
    assert!(matches!(
        c.accept(clock, &c.maintainer, &home),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn both_kinds_at_once_are_listed_and_the_stricter_decides() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(
        &[],
        "protect:\n  - path: Home\n    require:\n      rung: maintainer\n    reason: \"Standing.\"\n",
    );
    let desk = desk(&c.rig, clock, 90);
    let wiki = &c.rig.wiki.id;
    desk.protect(
        &c.maintainer,
        wiki,
        &setting("Home", Require::Rung(Role::Admin), "Temporary.", 7),
    )
    .unwrap();
    let notices = desk.protections_on(&c.author, wiki, "Home");
    assert_eq!(
        notices
            .iter()
            .map(|n| (n.kind(), n.reason()))
            .collect::<Vec<_>>(),
        vec![
            (Kind::Standing, "Standing."),
            (
                Kind::Temporary {
                    until: T0 + 7 * DAY
                },
                "Temporary."
            )
        ]
    );
    // The higher rung decides: a maintainer meets the standing one and not the temporary one.
    let p = c.propose_home();
    match refused(c.accept(clock, &c.maintainer, &p)) {
        AcceptFailure::Protected { notices, lack } => {
            assert_eq!(lack, Lack::Rung(Role::Admin));
            assert_eq!(notices.len(), 2, "every protection is carried, once each");
        }
        other => panic!("{other:?}"),
    }
    // When the temporary one has ended the standing one still holds, at its own rung.
    NOW.store(T0 + 7 * DAY, Ordering::SeqCst);
    assert_eq!(desk.protections_on(&c.author, wiki, "Home").len(), 1);
    assert!(matches!(
        c.accept(clock, &c.maintainer, &p),
        Ok(Accepted::Signed(_))
    ));
    NOW.store(T0, Ordering::SeqCst);
}

// ------------------------------------------------------------------------------------------- what a title covers

#[test]
fn a_namespace_protection_is_off_by_no_segment_and_a_page_is_protected_by_its_own_title_alone() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let a = page("A", "p-a");
    let b = page("B", "p-b");
    let c = page("C", "p-c");
    let files = [
        ("pages/Help/A.md", a.as_str()),
        ("pages/Helpful/B.md", b.as_str()),
        ("pages/Help.md", c.as_str()),
    ];
    let people = cast(&files, "");
    let desk = desk(&people.rig, clock, 90);
    let wiki = &people.rig.wiki.id;
    desk.protect(
        &people.maintainer,
        wiki,
        &namespace_setting("Help", Require::Rung(Role::Admin), "The help.", 7),
    )
    .unwrap();
    let covered = |title: &str| !desk.protections_on(&people.author, wiki, title).is_empty();
    assert!(covered("Help/A"), "a page below the namespace");
    assert!(
        !covered("Helpful/B"),
        "a namespace is whole segments, not a prefix of text"
    );
    assert!(
        !covered("Help"),
        "the page of the namespace's own name is a page"
    );
    // The same through the acceptance: only the page below is held.
    let below = people.rig.propose(
        &people.author,
        "A",
        &[("pages/Help/A.md", Some(&changed(&a)))],
    );
    let near = people.rig.propose(
        &people.author,
        "B",
        &[("pages/Helpful/B.md", Some(&changed(&b)))],
    );
    let own = people.rig.propose(
        &people.author,
        "C",
        &[("pages/Help.md", Some(&changed(&c)))],
    );
    assert!(matches!(
        refused(people.accept(clock, &people.maintainer, &below)),
        AcceptFailure::Protected { .. }
    ));
    for ok in [&near, &own] {
        assert!(matches!(
            people.accept(clock, &people.maintainer, ok),
            Ok(Accepted::Signed(_))
        ));
    }
    // A namespace one segment short covers neither.
    let short = desk.protect(
        &people.maintainer,
        wiki,
        &namespace_setting("Hel", Require::Review, "Too short.", 7),
    );
    assert!(short.is_ok());
    assert!(desk.protections_on(&people.author, wiki, "Help/A").len() == 1);
}

#[test]
fn a_change_that_touches_a_protected_page_beside_others_is_judged_by_that_page_and_a_renames_old_path_counts()
 {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let other = page("Other", "p-other");
    let c = cast(&[("pages/Other.md", other.as_str())], "");
    let desk = desk(&c.rig, clock, 90);
    desk.protect(
        &c.maintainer,
        &c.rig.wiki.id,
        &setting("Home", Require::Rung(Role::Admin), "Dispute.", 7),
    )
    .unwrap();
    // Two pages, one of them protected.
    let both = c.rig.propose(
        &c.author,
        "Both",
        &[
            ("pages/Other.md", Some(&changed(&other))),
            ("pages/Home.md", Some(&changed(HOME))),
        ],
    );
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &both)),
        AcceptFailure::Protected { .. }
    ));
    // A rename whose **old** path is the protected page and whose new path is not.
    let moved = page("Moved", "p-moved");
    let edit = crate::proposal::ProposalEdit {
        summary: "Move the front page".to_owned(),
        files: vec![inklkunn_store::FileChange {
            path: "pages/Moved.md".to_owned(),
            text: Some(moved.clone()),
        }],
        renames: vec![inklkunn_store::Rename {
            from: "pages/Home.md".to_owned(),
            to: "pages/Moved.md".to_owned(),
        }],
        ..crate::proposal::ProposalEdit::default()
    };
    let draft = c
        .rig
        .proposals
        .create(&c.author, &c.rig.wiki, None, edit)
        .unwrap();
    let rename = c
        .rig
        .proposals
        .submit(&c.author, &c.rig.wiki, &draft.id)
        .unwrap();
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &rename)),
        AcceptFailure::Protected { .. }
    ));
    // And the page protected only by its new path: a rename into a protected title.
    let desk2 = desk;
    desk2
        .lift_protection(
            &c.maintainer,
            &c.rig.wiki.id,
            &Target::Path("Home".to_owned()),
            None,
        )
        .unwrap();
    desk2
        .protect(
            &c.maintainer,
            &c.rig.wiki.id,
            &setting("Moved", Require::Rung(Role::Admin), "A protected title.", 7),
        )
        .unwrap();
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &rename)),
        AcceptFailure::Protected { .. }
    ));
}

// ------------------------------------------------------------------------------------------- who is told

#[test]
fn a_protection_is_not_shown_to_a_stranger_of_a_restricted_wiki_nor_for_a_wiki_that_is_not_there() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let rig = Rig::of_visibility("restricted", Visibility::Restricted);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, member) = rig.person("Mo", Some(Role::Observer));
    let (_, stranger) = rig.person("Stan", None);
    let desk = desk(&rig, clock, 90);
    desk.protect(
        &maintainer,
        &rig.wiki.id,
        &setting("Home", Require::Review, "Why.", 3),
    )
    .unwrap();
    assert_eq!(desk.protections_on(&member, &rig.wiki.id, "Home").len(), 1);
    assert_eq!(
        desk.protections_on(&maintainer, &rig.wiki.id, "Home").len(),
        1
    );
    for who in [&stranger, &Principal::Anonymous] {
        assert!(desk.protections_on(who, &rig.wiki.id, "Home").is_empty());
    }
    // A wiki that is not there answers the same: with none.
    assert!(
        desk.protections_on(&maintainer, &inklkunn_store::WikiId::new(), "Home")
            .is_empty()
    );
}

#[test]
fn an_authorizer_that_keeps_protections_but_has_none_decides_every_action_on_a_page_as_0_3_0_did() {
    use crate::authorize::{Snapshot, decide};
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let rig = rig_with(&[], "");
    let desk = desk(&rig, clock, 90);
    let page = Resource::Page {
        wiki: rig.wiki.id.clone(),
        path: "pages/Home.md".to_owned(),
    };
    let mut people: Vec<(Option<Role>, Principal)> = Role::ALL
        .into_iter()
        .map(|role| (Some(role), rig.person(role.as_str(), Some(role)).1))
        .collect();
    people.push((None, rig.person("Nobody", None).1));
    people.push((None, Principal::Anonymous));
    for (role, who) in &people {
        for action in Action::ALL {
            let snapshot = Snapshot {
                visibility: Some(Visibility::Public),
                role: *role,
                cap: None,
            };
            assert_eq!(
                desk.authorize(who, action, &page),
                decide(snapshot, who, action),
                "{role:?} {action:?}"
            );
        }
    }
}

#[test]
fn a_protection_limits_acceptance_only_so_declining_asking_for_a_revision_and_superseding_stay_with_the_policy()
 {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    desk.protect(
        &c.maintainer,
        &c.rig.wiki.id,
        &setting("Home", Require::Rung(Role::Admin), "Only the admin.", 7),
    )
    .unwrap();
    let proposals = &c.rig.proposals;
    let wiki = &c.rig.wiki;
    // The maintainer may not accept a change to Home, and may still decline one, ask for a revision of one, and see one
    // superseded: none of them changes the page (decision 13).
    let (first, second, third) = (c.propose_home(), c.propose_home(), c.propose_home());
    assert!(matches!(
        refused(c.accept(clock, &c.maintainer, &first)),
        AcceptFailure::Protected { .. }
    ));
    proposals
        .decline(&c.maintainer, wiki, &first.id, "Not this.")
        .unwrap();
    proposals
        .ask_for_revision(&c.maintainer, wiki, &second.id, "Say more.")
        .unwrap();
    let newer = c.propose_home();
    proposals
        .supersede(&c.author, wiki, &third.id, &newer.id)
        .unwrap();
}

#[test]
fn a_wiki_with_no_protection_decides_every_acceptance_as_it_did() {
    static NOW: AtomicI64 = AtomicI64::new(T0);
    fn clock() -> i64 {
        NOW.load(Ordering::SeqCst)
    }
    let c = cast(&[], "");
    let desk = desk(&c.rig, clock, 90);
    let home = home_for(&c);
    for (who, accepts) in [
        (&c.maintainer, true),
        (&c.admin, true),
        (&c.reviewer, false),
        (&c.author, false),
        (&Principal::Anonymous, false),
    ] {
        assert_eq!(
            desk.authorize(who, Action::Accept, &home).is_allowed(),
            accepts
        );
    }
    let p = c.propose_home();
    assert!(matches!(
        c.accept(clock, &c.maintainer, &p),
        Ok(Accepted::Signed(_))
    ));
}
