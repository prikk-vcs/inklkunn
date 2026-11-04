//! Creating an alternative view (RFC 005 part 2 D-9; handoff 005 K-T8) over the stand-in signer and the in-memory
//! repository: who may, what is recorded before the signer is asked, what the signer's guard sees, how each way of
//! failing leaves the records, and that the repository (not the signer's answer) settles whether the view is there.
//! The same path against a real prikk and the real signer is `tests/views.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use inklkunn_signer::accept::AcceptError;
use inklkunn_store::{
    AcceptanceStore, AccountId, ActStore, LeaseHolder, RecoveryFlag, RecoveryReason, Role, Scope,
    SqliteStore, ViewActEnd, ViewActId, ViewActOutcome, ViewActRecord, ViewKind,
};

use super::*;
use crate::acceptance::testing::{Rig, quick};
use crate::authorize::IdentityAuthority;
use crate::views::{self, MAX_NAME_CHARS};

fn reviewer(rig: &Rig) -> (AccountId, Principal) {
    rig.person("Rae Reviewer", Some(Role::Reviewer))
}

fn made(rig: &Rig, who: &Principal, name: &str) -> Result<ViewCreated, ViewFailure> {
    rig.acceptor()
        .create_view(who, &rig.wiki, name, None, ViewKind::Interpretation)
}

fn entries(rig: &Rig) -> Vec<inklkunn_store::ViewActEntry> {
    rig.store.view_acts(&rig.wiki.id).unwrap()
}

#[test]
fn a_reviewer_creates_a_view_and_every_record_is_in_place() {
    let rig = Rig::new("v1");
    let (rae, who) = reviewer(&rig);
    let tip = rig.reads.tip(&rig.wiki, None).unwrap().block_id;
    let created = rig
        .acceptor()
        .create_view(&who, &rig.wiki, "reading", None, ViewKind::Interpretation)
        .unwrap();
    assert_eq!(created.name, "reading");
    assert_eq!(created.forked_from, "main");
    assert!(!created.seeds_left);
    // The act was recorded, before the signer was asked, with all the facts: it is the signer's guard that read it.
    let calls = rig.double.view_calls();
    assert_eq!(calls.len(), 1);
    let call = calls.first().unwrap();
    assert_eq!(
        (call.name.as_str(), call.from.as_str()),
        ("heads/reading", "heads/main")
    );
    assert_eq!(call.act.as_str(), created.act.as_str());
    assert_eq!(
        call.lease_purpose.as_deref(),
        Some("view"),
        "the signer was asked inside the wiki's lease, held for a view"
    );
    let facts = call
        .facts
        .clone()
        .expect("the act was open when the signer was asked");
    assert_eq!(
        (facts.wiki, facts.name.as_str(), facts.from.as_str()),
        (rig.wiki.id.clone(), "heads/reading", "heads/main")
    );
    // The record is whole and ended as created.
    let all = entries(&rig);
    assert_eq!(all.len(), 1);
    let entry = all.first().unwrap();
    assert_eq!(entry.act.id, created.act);
    assert_eq!(entry.act.name, "reading");
    assert_eq!(entry.act.forked_from, "main");
    assert_eq!(
        entry.act.from_block, tip,
        "the tip of the view it was forked from"
    );
    assert_eq!(entry.act.kind, ViewKind::Interpretation);
    assert_eq!(entry.act.created_by, rae);
    assert_eq!(
        entry.end.as_ref().map(|e| &e.outcome),
        Some(&ViewActOutcome::Created)
    );
    // The wiki's caches were forgotten: the new view has a tip to read at once (asked before anything lists the
    // views, which would hold a fresh answer of its own).
    assert!(rig.reads.tip(&rig.wiki, Some("heads/reading")).is_ok());
    // The view is among the wiki's views, with its origin, and no lease or flag is left.
    let list = views::list(&rig.reads, &*rig.store, &rig.wiki).unwrap();
    let row = list.view("reading").expect("listed");
    assert_eq!(
        row.origin.as_ref().map(|o| o.kind),
        Some(ViewKind::Interpretation)
    );
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    assert_eq!(rig.store.lease_holder(&rig.wiki.id).unwrap(), None);
}

#[test]
fn a_view_can_be_forked_from_another_view_and_kinds_are_recorded_as_given() {
    let rig = Rig::new("v2");
    let (_, who) = reviewer(&rig);
    let acceptor = rig.acceptor();
    acceptor
        .create_view(&who, &rig.wiki, "first", None, ViewKind::Experimental)
        .unwrap();
    let second = acceptor
        .create_view(
            &who,
            &rig.wiki,
            "second",
            Some("first"),
            ViewKind::Interpretation,
        )
        .unwrap();
    assert_eq!(second.forked_from, "first");
    let call = rig.double.view_calls().pop().unwrap();
    assert_eq!(call.from, "heads/first");
    let kinds: Vec<(String, ViewKind, String)> = entries(&rig)
        .into_iter()
        .map(|e| (e.act.name, e.act.kind, e.act.forked_from))
        .collect();
    assert_eq!(
        kinds,
        [
            (
                "first".to_owned(),
                ViewKind::Experimental,
                "main".to_owned()
            ),
            (
                "second".to_owned(),
                ViewKind::Interpretation,
                "first".to_owned()
            ),
        ]
    );
}

#[test]
fn reviewers_and_above_may_and_nobody_below_or_unknown_may() {
    let rig = Rig::new("v3");
    let held = rig
        .leases
        .acquire(&rig.wiki.id, Purpose::Export, None)
        .unwrap();
    let started = std::time::Instant::now();
    // Refused before the lease is asked for (the lease is held and would make a taker wait): nothing is recorded,
    // the signer is never called.
    for role in [
        None,
        Some(Role::Observer),
        Some(Role::Questioner),
        Some(Role::Contributor),
    ] {
        let (_, who) = rig.person("Someone", role);
        assert_eq!(
            made(&rig, &who, "x"),
            Err(ViewFailure::NotAllowed),
            "{role:?}"
        );
    }
    assert_eq!(
        made(&rig, &Principal::Anonymous, "x"),
        Err(ViewFailure::NotAllowed)
    );
    let system = Principal::system(inklkunn_signer::message::SystemLabel::inklkunn());
    assert_eq!(made(&rig, &system, "x"), Err(ViewFailure::NotAllowed));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "it waited for the lease"
    );
    assert!(rig.double.view_calls().is_empty());
    assert!(entries(&rig).is_empty());
    drop(held);
    // And the roles that may.
    for (n, role) in [Role::Reviewer, Role::Maintainer, Role::Admin]
        .into_iter()
        .enumerate()
    {
        let (_, who) = rig.person("Allowed", Some(role));
        assert!(made(&rig, &who, &format!("v{n}")).is_ok(), "{role:?}");
    }
}

#[test]
fn a_token_needs_the_review_scope() {
    let rig = Rig::new("v4");
    let (id, _) = reviewer(&rig);
    let authority = IdentityAuthority::for_tests();
    let token = |scope| Principal::token(&authority, id.clone(), scope);
    for scope in [Scope::Read, Scope::Propose] {
        assert_eq!(
            made(&rig, &token(scope), "x"),
            Err(ViewFailure::NotAllowed),
            "{scope:?}"
        );
    }
    for (n, scope) in [Scope::Review, Scope::Accept, Scope::Admin]
        .into_iter()
        .enumerate()
    {
        assert!(
            made(&rig, &token(scope), &format!("t{n}")).is_ok(),
            "{scope:?}"
        );
    }
}

#[test]
fn a_name_that_is_not_a_views_is_refused_before_anything_is_recorded() {
    let rig = Rig::new("v5");
    let (_, who) = reviewer(&rig);
    for (name, why) in [
        ("", NameFault::Empty),
        ("Draft", NameFault::Start),
        ("-x", NameFault::Start),
        ("a b", NameFault::Characters),
        ("a/b", NameFault::Characters),
        ("a..b", NameFault::Characters),
        (&"x".repeat(MAX_NAME_CHARS + 1), NameFault::TooLong),
    ] {
        assert_eq!(
            made(&rig, &who, name),
            Err(ViewFailure::InvalidName(why)),
            "{name:?}"
        );
    }
    assert!(entries(&rig).is_empty());
    assert!(rig.double.view_calls().is_empty());
}

#[test]
fn a_name_that_is_taken_or_a_source_that_is_not_there_is_refused_and_nothing_is_recorded() {
    let rig = Rig::new("v6");
    let (_, who) = reviewer(&rig);
    rig.fake.add_view("heads/Existing");
    let acceptor = rig.acceptor();
    let make = |name: &str, from: Option<&str>| {
        acceptor.create_view(&who, &rig.wiki, name, from, ViewKind::Experimental)
    };
    assert_eq!(make("main", None), Err(ViewFailure::NameTaken));
    // Without regard to case: a view cannot differ from another only by it.
    assert_eq!(make("existing", None), Err(ViewFailure::NameTaken));
    assert_eq!(
        make("fresh", Some("nowhere")),
        Err(ViewFailure::NoSuchSource)
    );
    assert_eq!(
        make("fresh", Some("heads/main")),
        Err(ViewFailure::NoSuchSource)
    );
    assert!(entries(&rig).is_empty());
    assert!(rig.double.view_calls().is_empty());
}

#[test]
fn a_wiki_with_as_many_open_views_as_may_be_makes_no_more() {
    let rig = Rig::new("v7");
    let (_, who) = reviewer(&rig);
    for n in 0..(MAX_VIEWS - 1) {
        rig.fake.add_view(&format!("heads/v{n:03}"));
    }
    assert_eq!(
        made(&rig, &who, "one-too-many"),
        Err(ViewFailure::TooMany { bound: MAX_VIEWS })
    );
    assert!(entries(&rig).is_empty());
}

#[test]
fn the_signers_refusal_ends_the_act_as_failed_frees_the_name_and_a_retry_succeeds() {
    let rig = Rig::new("v8");
    let (_, who) = reviewer(&rig);
    rig.double
        .then_view(Err(AcceptError::Refused("prikk would not".to_owned())));
    let failure = made(&rig, &who, "reading").unwrap_err();
    let ViewFailure::Failed { act, word, detail } = failure else {
        panic!("{failure:?}")
    };
    assert_eq!(word, words::REFUSED);
    assert!(
        detail.contains("prikk would not"),
        "the operators' text is kept: {detail}"
    );
    let all = entries(&rig);
    assert_eq!(all.len(), 1);
    assert_eq!(all.first().unwrap().act.id, act);
    assert_eq!(
        all.first().unwrap().end.as_ref().map(|e| &e.outcome),
        Some(&ViewActOutcome::Failed {
            word: words::REFUSED.to_owned()
        })
    );
    assert!(!all.first().unwrap().stands());
    // Nothing is flagged and the view is not there; the name is free.
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    let list = views::list(&rig.reads, &*rig.store, &rig.wiki).unwrap();
    assert!(list.view("reading").is_none());
    assert!(made(&rig, &who, "reading").is_ok());
    assert_eq!(entries(&rig).len(), 2);
}

#[test]
fn each_way_the_signer_can_decline_has_its_fixed_word() {
    let rig = Rig::new("v9");
    let (_, who) = reviewer(&rig);
    let cases = [
        (AcceptError::NotPending, words::REFUSED),
        (AcceptError::Unavailable("x".to_owned()), words::UNAVAILABLE),
        (
            AcceptError::CeilingExceeded {
                verb: "branch".to_owned(),
                bytes: 1,
            },
            words::MEMORY_LIMIT,
        ),
    ];
    for (n, (error, want)) in cases.into_iter().enumerate() {
        rig.double.then_view(Err(error));
        let failure = made(&rig, &who, &format!("n{n}")).unwrap_err();
        let ViewFailure::Failed { word, .. } = failure else {
            panic!("{failure:?}")
        };
        assert_eq!(word, want);
    }
}

#[test]
fn a_signer_that_found_the_wiki_held_by_another_process_is_not_a_failure_of_the_view() {
    let rig = Rig::new("v9b");
    let (_, who) = reviewer(&rig);
    rig.double
        .then_view(Err(AcceptError::Busy { holder: Some(7) }));
    assert!(matches!(
        made(&rig, &who, "reading"),
        Err(ViewFailure::Unavailable(_))
    ));
    assert_eq!(
        entries(&rig)
            .first()
            .and_then(|e| e.end.as_ref().map(|e| e.outcome.clone())),
        Some(ViewActOutcome::Failed {
            word: words::UNAVAILABLE.to_owned()
        })
    );
    assert!(made(&rig, &who, "reading").is_ok(), "the name is free");
}

#[test]
fn an_unfinished_retirement_pauses_it_with_the_signers_sentence_and_the_act_is_failed() {
    let rig = Rig::new("v10");
    let (_, who) = reviewer(&rig);
    rig.double.then_view(Err(AcceptError::RetirementUnfinished(
        "finish the retirement first".to_owned(),
    )));
    assert_eq!(
        made(&rig, &who, "reading"),
        Err(ViewFailure::RetirementUnfinished(
            "finish the retirement first".to_owned()
        ))
    );
    assert_eq!(
        entries(&rig)
            .first()
            .and_then(|e| e.end.as_ref().map(|e| e.outcome.clone())),
        Some(ViewActOutcome::Failed {
            word: words::RETIREMENT_UNFINISHED.to_owned()
        })
    );
}

#[test]
fn the_repository_settles_whether_the_view_is_there_not_the_signers_answer() {
    let rig = Rig::new("v11");
    let (_, who) = reviewer(&rig);
    // The signer made the view and could not remove its seeds: an error that is not a refusal. The view is there.
    rig.double
        .then_view(Err(AcceptError::SeedsNotRemoved { sealed: None }));
    let created = made(&rig, &who, "reading").unwrap();
    assert!(created.seeds_left);
    assert_eq!(
        entries(&rig)
            .first()
            .and_then(|e| e.end.as_ref().map(|e| e.outcome.clone())),
        Some(ViewActOutcome::Created)
    );
}

#[test]
fn a_repository_that_cannot_be_read_after_the_signer_ran_leaves_the_act_open_and_says_it_is_uncertain()
 {
    let rig = Rig::new("v11b");
    let (_, who) = reviewer(&rig);
    rig.double.then_views_cannot_be_listed();
    assert!(matches!(
        made(&rig, &who, "reading"),
        Err(ViewFailure::Uncertain(_))
    ));
    let all = entries(&rig);
    assert_eq!(all.len(), 1);
    assert!(
        all.first().is_some_and(|e| e.end.is_none() && e.stands()),
        "the act is open: the next creation settles it from the repository"
    );
    // Once the repository can be read again, the next creation settles it (the view is there) and the name is taken.
    rig.fake.fail_views(None);
    assert_eq!(made(&rig, &who, "reading"), Err(ViewFailure::NameTaken));
    assert_eq!(
        entries(&rig)
            .first()
            .and_then(|e| e.end.as_ref().map(|e| e.outcome.clone())),
        Some(ViewActOutcome::Created)
    );
}

#[test]
fn a_wiki_that_is_busy_or_needs_recovery_is_told_so_and_nothing_is_recorded() {
    let rig = Rig::with("v12", quick(30), crate::proposal::QuotaConfig::default());
    let (_, who) = reviewer(&rig);
    let held = rig
        .leases
        .acquire(&rig.wiki.id, Purpose::Acceptance, None)
        .unwrap();
    assert!(matches!(made(&rig, &who, "x"), Err(ViewFailure::Busy(_))));
    drop(held);
    rig.store
        .set_recovery_flag(&RecoveryFlag {
            wiki: rig.wiki.id.clone(),
            reason: RecoveryReason::Queued,
            act: None,
            since: 1,
        })
        .unwrap();
    assert!(matches!(
        made(&rig, &who, "x"),
        Err(ViewFailure::NeedsRecovery(f)) if f.reason == RecoveryReason::Queued
    ));
    assert!(entries(&rig).is_empty());
    assert!(rig.double.view_calls().is_empty());
}

#[test]
fn a_store_that_keeps_no_record_is_refused_at_once() {
    let rig = Rig::new("v13");
    let (_, who) = reviewer(&rig);
    let mut parts = rig.parts(rig.double.clone());
    parts.acts = std::sync::Arc::new(SqliteStore::open_in_memory().unwrap());
    let acceptor = ProposalAcceptor::new(parts).with_reconciliation(false);
    assert_eq!(
        acceptor.create_view(&who, &rig.wiki, "x", None, ViewKind::Experimental),
        Err(ViewFailure::NotKept)
    );
    assert!(rig.double.view_calls().is_empty());
}

fn open_act(rig: &Rig, name: &str) -> ViewActRecord {
    let act = ViewActRecord {
        id: ViewActId::new(),
        wiki: rig.wiki.id.clone(),
        name: name.to_owned(),
        forked_from: "main".to_owned(),
        from_block: "e".repeat(64),
        kind: ViewKind::Experimental,
        created_by: AccountId::new(),
        created_at: 1,
    };
    rig.store.begin_view_act(&act).unwrap();
    act
}

#[test]
fn an_act_that_never_ended_is_settled_from_the_repository_before_the_next_creation() {
    let rig = Rig::new("v14");
    let (_, who) = reviewer(&rig);
    // One whose view is there (the process died after the branch and before the end), one whose view is not.
    let there = open_act(&rig, "there");
    rig.fake.add_view("heads/there");
    let gone = open_act(&rig, "gone");
    assert!(made(&rig, &who, "fresh").is_ok());
    let outcome = |id: &ViewActId| {
        entries(&rig)
            .into_iter()
            .find(|e| e.act.id == *id)
            .and_then(|e| e.end.map(|e| e.outcome))
    };
    assert_eq!(outcome(&there.id), Some(ViewActOutcome::Created));
    assert_eq!(
        outcome(&gone.id),
        Some(ViewActOutcome::Failed {
            word: words::INTERRUPTED.to_owned()
        })
    );
    // The name the dead act held is free again.
    assert!(made(&rig, &who, "gone").is_ok());
}

#[test]
fn a_holder_that_died_making_a_view_flags_nothing_and_one_that_died_accepting_does() {
    let rig = Rig::new("v15");
    let (_, who) = reviewer(&rig);
    let dead = |purpose: &str| LeaseHolder {
        wiki: rig.wiki.id.clone(),
        holder: "a-process-that-is-gone".to_owned(),
        purpose: purpose.to_owned(),
        acceptance: None,
        since: 1,
    };
    rig.store.record_lease_holder(&dead("view")).unwrap();
    assert!(
        made(&rig, &who, "first").is_ok(),
        "a view's creation leaves nothing to recover"
    );
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    rig.store.record_lease_holder(&dead("acceptance")).unwrap();
    assert!(matches!(
        made(&rig, &who, "second"),
        Err(ViewFailure::NeedsRecovery(_))
    ));
}

#[test]
fn the_signer_is_bound_to_the_record_a_creation_it_was_not_recorded_for_is_not_pending() {
    use inklkunn_signer::accept::{Accept, ViewCreationAct};
    let rig = Rig::new("v16");
    let act = open_act(&rig, "bound");
    let double = &rig.double;
    let wiki = &rig.wiki.id;
    let the_act = ViewCreationAct::new(act.id.as_str());
    // Another name, another source, another wiki, an act nobody recorded, an id that is not one: all refused.
    for (w, name, from, a) in [
        (wiki.clone(), "heads/other", "heads/main", the_act.clone()),
        (
            wiki.clone(),
            "heads/bound",
            "heads/elsewhere",
            the_act.clone(),
        ),
        (
            inklkunn_store::WikiId::new(),
            "heads/bound",
            "heads/main",
            the_act.clone(),
        ),
        (
            wiki.clone(),
            "heads/bound",
            "heads/main",
            ViewCreationAct::new(ViewActId::new().as_str()),
        ),
        (
            wiki.clone(),
            "heads/bound",
            "heads/main",
            ViewCreationAct::new("not-an-id"),
        ),
    ] {
        assert!(
            matches!(
                double.create_view(&w, name, from, &a),
                Err(AcceptError::NotPending)
            ),
            "{name} {from}"
        );
    }
    // The one it was recorded for is allowed, and once it has ended it is no longer pending.
    assert!(
        double
            .create_view(wiki, "heads/bound", "heads/main", &the_act)
            .is_ok()
    );
    rig.store
        .finish_view_act(&ViewActEnd {
            act: act.id.clone(),
            outcome: ViewActOutcome::Created,
            at: 2,
        })
        .unwrap();
    assert!(matches!(
        double.create_view(wiki, "heads/bound", "heads/main", &the_act),
        Err(AcceptError::NotPending)
    ));
}
