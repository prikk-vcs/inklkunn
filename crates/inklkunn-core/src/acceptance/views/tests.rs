//! Creating an alternative view (RFC 005 part 2 D-9; handoff 005 K-T8) over the stand-in signer and the in-memory
//! repository: who may, what is recorded before the signer is asked, what the signer's guard sees, how each way of
//! failing leaves the records, and that the repository (not the signer's answer) settles whether the view is there.
//! The same path against a real prikk and the real signer is `tests/views.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_signer::accept::AcceptError;
use inklkunn_store::{
    AcceptanceStore, AccountId, ActStore, LeaseHolder, RecoveryFlag, RecoveryReason, Role, Scope,
    ViewActEnd, ViewActId, ViewActOutcome, ViewActRecord, ViewKind,
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
    // Refused before the lease is asked for (the lease is held and would make a taker wait, then say `Busy`, which
    // `NotAllowed` below is not; no clock, maintenance M26): nothing is recorded, the signer is never called.
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
        "the act is open: recovery settles it from every witness"
    );
    // Once the repository can be read again, the next creation is not settled from the listing (K4): the wiki is
    // flagged, with the act named, and nothing is asked of the signer.
    rig.fake.fail_views(None);
    let calls = rig.double.view_calls().len();
    let refused = made(&rig, &who, "reading");
    assert!(
        matches!(&refused, Err(ViewFailure::NeedsRecovery(f))
            if f.reason == RecoveryReason::Interrupted
                && f.act == all.first().map(|e| e.act.id.to_string())),
        "{refused:?}"
    );
    assert_eq!(rig.double.view_calls().len(), calls);
    assert!(entries(&rig).first().is_some_and(|e| e.end.is_none()));
}

#[test]
fn a_view_that_is_listed_while_prikk_did_not_say_its_creation_finished_is_neither_created_nor_failed()
 {
    // A `branch create` that failed after it published the ref's pointer lists the view and leaves the ref half
    // published: `verify` fails and every later publication refuses (prikk RFC 165 R3). The listing alone must not
    // end the act (K4, review finding K-c).
    for answer in [
        AcceptError::Refused("interrupted".to_owned()),
        AcceptError::Unavailable("stopped".to_owned()),
        AcceptError::CeilingExceeded {
            verb: "branch".to_owned(),
            bytes: 1,
        },
    ] {
        let rig = Rig::new("v11c");
        let (_, who) = reviewer(&rig);
        rig.double.then_view_listed_anyway(Err(answer.clone()));
        let made = made(&rig, &who, "reading");
        assert!(
            matches!(&made, Err(ViewFailure::Uncertain(why)) if why.contains("flagged")),
            "{answer:?}: {made:?}"
        );
        let all = entries(&rig);
        assert!(
            all.first().is_some_and(|e| e.end.is_none() && e.stands()),
            "{answer:?}: the act is open"
        );
        let flag = rig.store.recovery_flag(&rig.wiki.id).unwrap();
        assert!(
            flag.as_ref()
                .is_some_and(|f| f.reason == RecoveryReason::Interrupted
                    && f.act == all.first().map(|e| e.act.id.to_string())),
            "{answer:?}: {flag:?}"
        );
        // The wiki takes no write until recovery has looked.
        assert!(
            matches!(made_again(&rig, &who), Err(ViewFailure::NeedsRecovery(_))),
            "{answer:?}"
        );
    }
}

fn made_again(rig: &Rig, who: &Principal) -> Result<ViewCreated, ViewFailure> {
    made(rig, who, "writing")
}

#[test]
fn a_listed_view_is_created_when_the_signer_said_it_finished_and_a_view_not_listed_failed_whatever_it_said()
 {
    // The signer finished (and could not remove its seeds): created. The signer said all was well and the view is
    // not listed: failed, said as it is. A refusal with nothing listed: failed with its word.
    let rig = Rig::new("v11d");
    let (_, who) = reviewer(&rig);
    rig.double
        .then_view(Err(AcceptError::SeedsNotRemoved { sealed: None }));
    assert!(made(&rig, &who, "one").is_ok());
    rig.double
        .then_view(Err(AcceptError::Refused("no".to_owned())));
    assert!(matches!(
        made(&rig, &who, "two"),
        Err(ViewFailure::Failed { word, .. }) if word == words::REFUSED
    ));
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
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
    parts.acts = std::sync::Arc::new(KeepsNoViewActs(rig.store.clone()));
    let acceptor = ProposalAcceptor::new(parts).with_reconciliation(false);
    assert_eq!(
        acceptor.create_view(&who, &rig.wiki, "x", None, ViewKind::Experimental),
        Err(ViewFailure::NotKept)
    );
    assert!(rig.double.view_calls().is_empty());
}

/// An act store that keeps no record of a view's creation (K3: neither of the real stores is one any more): the older
/// methods go to the store it wraps, the view-act ones are the trait's defaults.
struct KeepsNoViewActs(std::sync::Arc<dyn ActStore>);

type R<T> = inklkunn_store::Result<T>;

impl ActStore for KeepsNoViewActs {
    fn recovery_flag(&self, w: &inklkunn_store::WikiId) -> R<Option<inklkunn_store::RecoveryFlag>> {
        self.0.recovery_flag(w)
    }
    fn set_recovery_flag(&self, f: &inklkunn_store::RecoveryFlag) -> R<bool> {
        self.0.set_recovery_flag(f)
    }
    fn clear_recovery_flag(&self, w: &inklkunn_store::WikiId) -> R<bool> {
        self.0.clear_recovery_flag(w)
    }
    fn record_overlap(&self, o: &inklkunn_store::OverlapRecord) -> R<()> {
        self.0.record_overlap(o)
    }
    fn overlaps_of(&self, p: &inklkunn_store::ProposalId) -> R<Vec<inklkunn_store::OverlapRecord>> {
        self.0.overlaps_of(p)
    }
    fn begin_system_act(&self, a: &inklkunn_store::SystemActRecord) -> R<()> {
        self.0.begin_system_act(a)
    }
    fn finish_system_act(&self, e: &inklkunn_store::SystemActEnd) -> R<()> {
        self.0.finish_system_act(e)
    }
    fn open_system_act(
        &self,
        i: &inklkunn_store::record::SystemActId,
    ) -> R<Option<inklkunn_store::SystemActRecord>> {
        self.0.open_system_act(i)
    }
    fn system_acts(&self, w: &inklkunn_store::WikiId) -> R<Vec<inklkunn_store::SystemActEntry>> {
        self.0.system_acts(w)
    }
    fn insert_wiki_with_creation_act(
        &self,
        r: inklkunn_store::WikiRecord,
        b: &inklkunn_store::SystemActRecord,
        e: &inklkunn_store::SystemActEnd,
    ) -> R<inklkunn_store::WikiRecord> {
        self.0.insert_wiki_with_creation_act(r, b, e)
    }
    fn begin_recovery_act(&self, a: &inklkunn_store::RecoveryActRecord) -> R<()> {
        self.0.begin_recovery_act(a)
    }
    fn finish_recovery_act(&self, e: &inklkunn_store::RecoveryActEnd) -> R<()> {
        self.0.finish_recovery_act(e)
    }
    fn open_recovery_act(
        &self,
        i: &inklkunn_store::record::RecoveryActId,
    ) -> R<Option<inklkunn_store::RecoveryActRecord>> {
        self.0.open_recovery_act(i)
    }
    fn recovery_acts(
        &self,
        w: &inklkunn_store::WikiId,
    ) -> R<Vec<inklkunn_store::RecoveryActEntry>> {
        self.0.recovery_acts(w)
    }
}

fn open_act(rig: &Rig, name: &str) -> ViewActRecord {
    let act = ViewActRecord {
        id: ViewActId::new(),
        wiki: rig.wiki.id.clone(),
        name: name.to_owned(),
        forked_from: "main".to_owned(),
        from_block: "e".repeat(64),
        kind: ViewKind::Experimental,
        created_by: rig.person("Quinn Creator", None).0,
        created_at: 1,
    };
    rig.store.begin_view_act(&act).unwrap();
    act
}

#[test]
fn the_name_of_a_view_that_was_closed_stays_taken() {
    let rig = Rig::new("v14b");
    let (_, who) = reviewer(&rig);
    // An act that created its view, whose view is no longer among the open ones.
    let old = open_act(&rig, "retired");
    rig.store
        .finish_view_act(&ViewActEnd {
            act: old.id,
            outcome: ViewActOutcome::Created,
            at: 2,
        })
        .unwrap();
    assert_eq!(made(&rig, &who, "retired"), Err(ViewFailure::NameTaken));
    assert!(rig.double.view_calls().is_empty());
    assert_eq!(entries(&rig).len(), 1);
}

#[test]
fn an_act_that_never_ended_is_not_settled_from_the_listing_the_wiki_is_flagged_for_recovery() {
    let rig = Rig::new("v14");
    let (_, who) = reviewer(&rig);
    // One whose view is listed (the process died after the branch and before the end), one whose view is not. The
    // listing cannot tell a whole view from a half-published one, so neither is ended here: recovery reads `doctor`
    // too (K4, review finding K-c).
    let there = open_act(&rig, "there");
    rig.fake.add_view("heads/there");
    let gone = open_act(&rig, "gone");
    for _ in 0..2 {
        let refused = made(&rig, &who, "fresh");
        assert!(
            matches!(&refused, Err(ViewFailure::NeedsRecovery(f)) if f.reason == RecoveryReason::Interrupted),
            "{refused:?}"
        );
    }
    assert!(
        rig.double.view_calls().is_empty(),
        "the signer is not asked"
    );
    for act in [&there, &gone] {
        assert!(
            entries(&rig)
                .into_iter()
                .find(|e| e.act.id == act.id)
                .is_some_and(|e| e.end.is_none()),
            "the act is not ended"
        );
    }
    // The flag names an open act.
    let flag = rig.store.recovery_flag(&rig.wiki.id).unwrap().unwrap();
    assert!(
        flag.act
            .is_some_and(|a| a == there.id.to_string() || a == gone.id.to_string())
    );
}

#[test]
fn a_holder_that_died_making_a_view_flags_the_wiki_as_one_that_died_accepting_does_and_an_export_does_not()
 {
    let rig = Rig::new("v15");
    let (_, who) = reviewer(&rig);
    let dead = |purpose: &str| LeaseHolder {
        wiki: rig.wiki.id.clone(),
        holder: "a-process-that-is-gone".to_owned(),
        purpose: purpose.to_owned(),
        acceptance: None,
        since: 1,
    };
    // An export's and a refresh's leave nothing in the repository to recover.
    for purpose in ["export", "refresh"] {
        rig.store.record_lease_holder(&dead(purpose)).unwrap();
        assert!(
            made(&rig, &who, &format!("after-{purpose}")).is_ok(),
            "{purpose}: nothing to recover"
        );
        assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    }
    // A view's creation may have left the ref half published (K4): it is recovered like an acceptance.
    rig.store.record_lease_holder(&dead("view")).unwrap();
    assert!(matches!(
        made(&rig, &who, "second"),
        Err(ViewFailure::NeedsRecovery(f)) if f.reason == RecoveryReason::Interrupted
    ));
    assert!(rig.store.recovery_flag(&rig.wiki.id).unwrap().is_some());
    rig.store.clear_recovery_flag(&rig.wiki.id).unwrap();
    rig.store.record_lease_holder(&dead("acceptance")).unwrap();
    assert!(matches!(
        made(&rig, &who, "third"),
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

#[test]
fn a_name_that_a_closed_view_has_is_taken_in_any_case_and_nothing_is_recorded() {
    // K12 (review finding K-i): prikk makes no branch of a closed one's name again, in any case, so the name check
    // sees every view. Without it a closed `heads/a_` made outside the service ended *Failed (refused)*.
    let rig = Rig::new("v6b");
    let (_, who) = reviewer(&rig);
    rig.fake.add_closed_view("heads/a_");
    rig.fake.add_closed_view("heads/Older");
    assert_eq!(made(&rig, &who, "a_"), Err(ViewFailure::NameTaken));
    assert_eq!(made(&rig, &who, "older"), Err(ViewFailure::NameTaken));
    // A name no view has is free, closed ones or not.
    assert!(made(&rig, &who, "a_b").is_ok());
    assert!(entries(&rig).len() == 1);
    assert_eq!(rig.double.view_calls().len(), 1);
}

#[test]
fn closed_views_do_not_count_toward_the_bound_on_open_ones() {
    let rig = Rig::new("v7b");
    let (_, who) = reviewer(&rig);
    for n in 0..(MAX_VIEWS - 2) {
        rig.fake.add_view(&format!("heads/v{n:03}"));
    }
    for n in 0..5 {
        rig.fake.add_closed_view(&format!("heads/old{n}"));
    }
    // 99 views are open with main; the 5 closed ones are not among them, so one more is allowed.
    assert!(made(&rig, &who, "the-hundredth").is_ok());
    assert_eq!(
        made(&rig, &who, "one-too-many"),
        Err(ViewFailure::TooMany { bound: MAX_VIEWS })
    );
}

#[test]
fn a_name_that_is_not_a_views_is_checked_before_it_is_built_into_the_resource_authorization_is_asked_about()
 {
    // K12: the name is validated before it enters any resource or audit name. The check is a pure function of what
    // was typed, so it comes first: even a visitor who could not create a view is told the name is not one (the rules
    // are on the form), and the authorization is never asked about a name that is not a view's.
    let rig = Rig::new("v5b");
    let (_, observer) = rig.person("Olga Observer", Some(Role::Observer));
    let nuisance = "bad name\nwith\u{202e}controls and a very very long tail ".repeat(8);
    for name in [nuisance.as_str(), "dra ft", "Draft", "", "official"] {
        assert!(
            matches!(
                made(&rig, &observer, name),
                Err(ViewFailure::InvalidName(_))
            ),
            "{name:?}"
        );
    }
    // A name that is one is asked about as it always was, and the observer may not.
    assert_eq!(made(&rig, &observer, "fine"), Err(ViewFailure::NotAllowed));
    assert!(entries(&rig).is_empty());
}

#[test]
fn a_retired_word_is_refused_as_a_name_before_anything_is_recorded() {
    let rig = Rig::new("v5c");
    let (_, who) = reviewer(&rig);
    for word in views::RETIRED_NAMES {
        assert_eq!(
            made(&rig, &who, word),
            Err(ViewFailure::InvalidName(NameFault::Retired)),
            "{word}"
        );
    }
    assert!(entries(&rig).is_empty());
    assert!(rig.double.view_calls().is_empty());
    assert!(made(&rig, &who, "official-draft").is_ok());
}

#[test]
fn a_view_that_was_made_and_closed_before_the_creation_looked_is_still_a_view_that_was_made() {
    // The creation settles from the full listing: a created view that is already closed is created, not failed.
    let rig = Rig::new("v11e");
    let (_, who) = reviewer(&rig);
    rig.double.then_views_it_creates_are_closed();
    let created = made(&rig, &who, "brief").unwrap();
    assert_eq!(created.name, "brief");
    assert_eq!(
        entries(&rig)
            .first()
            .and_then(|e| e.end.as_ref().map(|e| e.outcome.clone())),
        Some(ViewActOutcome::Created)
    );
    // And its name stays taken.
    assert_eq!(made(&rig, &who, "brief"), Err(ViewFailure::NameTaken));
}
