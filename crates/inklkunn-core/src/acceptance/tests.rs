#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The acceptance orchestration against the in-memory repository and a scripted signer (no prikk needed; the
//! tests that run the real signer against a real repository are `tests/acceptance.rs`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use inklkunn_signer::accept::{AcceptError, Unconfirmed};
use inklkunn_signer::message::{Kind, Message};
use inklkunn_store::{
    AcceptanceOutcome, AcceptanceStore, ActStore, Anchor, InboxEventKind, InboxStore, IntentEnd,
    ProposalState, Quote, RecoveryReason, Role, RoleStore, Visibility,
};

use super::testing::{HOME, Rig, quick, sealed};
use super::*;
use crate::lease::Purpose;
use crate::proposal::{NewComment, QuotaConfig};
use inklkunn_store::SystemActId;

const MAIN: &str = "heads/main";

/// A rig with a maintainer, a contributor and a proposal by the contributor that rewrites the first paragraph.
fn rig_with_proposal() -> (
    Rig,
    Principal,
    AccountId,
    Principal,
    AccountId,
    ProposalRecord,
) {
    let rig = Rig::new("t4");
    let (maintainer_id, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (author_id, author) = rig.person("Noor", Some(Role::Contributor));
    let text = HOME.replace("One.", "Uno.");
    let proposal = rig.propose(
        &author,
        "Say it in Spanish",
        &[("pages/Home.md", Some(&text))],
    );
    (rig, maintainer, maintainer_id, author, author_id, proposal)
}

#[test]
fn an_acceptance_whose_base_is_the_tip_is_signed_with_every_record_in_place() {
    let (rig, maintainer, maintainer_id, _author, author_id, p) = rig_with_proposal();
    let seen_by_the_reads = Arc::new(AtomicUsize::new(0));
    let acceptor = rig.acceptor();
    let signed = match acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap() {
        Accepted::Signed(s) => s,
        other => panic!("{other:?}"),
    };
    drop(seen_by_the_reads);
    assert!(!signed.reauthored);
    assert_eq!(signed.block_id, "f".repeat(64));
    // The signer's guard read the store: the facts it is bound to, and the tip the change must still be on.
    let calls = rig.double.calls();
    assert_eq!(calls.len(), 1);
    let call = calls.first().unwrap();
    assert_eq!(call.view, MAIN);
    let facts = call
        .pending
        .clone()
        .expect("the record was pending when the signer was asked");
    assert_eq!(facts.kind, Kind::Proposal);
    assert_eq!(facts.proposal.as_ref(), Some(&p.id));
    assert_eq!(facts.change_digest, call.change.digest());
    assert_eq!(
        facts.pinned_tip.as_ref().map(|b| b.as_str().to_owned()),
        Some(rig.fake.block_id())
    );
    // The message is CT-03 with Kind: proposal, naming the proposal, both people and the base.
    let message = Message::parse(&call.change.message).unwrap();
    assert_eq!(message.kind(), Kind::Proposal);
    assert_eq!(message.summary().as_str(), "Say it in Spanish");
    assert_eq!(
        message.base().map(|b| b.as_str().to_owned()),
        Some(rig.fake.block_id())
    );
    // The records: the acceptance sealed with the digest of the files, the intent completed, the proposal
    // accepted, no flag, no lease record left.
    let entries = rig.store.acceptances_of(&p.id, 1).unwrap();
    let entry = entries.first().unwrap();
    assert_eq!(entry.acceptance.digest, call.change.digest().to_hex());
    assert_eq!(
        call.token.as_str(),
        entry.acceptance.id.as_str(),
        "the token is the acceptance's id"
    );
    assert_eq!(entry.acceptance.acceptor, maintainer_id);
    assert_eq!(entry.acceptance.proposer, author_id);
    assert_eq!(entry.acceptance.acceptor_role, Role::Maintainer);
    assert_eq!(entry.acceptance.policy_version, POLICY_VERSION);
    assert!(matches!(
        entry.result.as_ref().map(|r| &r.outcome),
        Some(AcceptanceOutcome::Sealed { block_id, .. }) if *block_id == signed.block_id
    ));
    let intents = rig.store.intents(&rig.wiki.id).unwrap();
    assert_eq!(intents.len(), 1);
    assert_eq!(
        intents.first().unwrap().completion.as_ref().map(|c| c.end),
        Some(IntentEnd::Completed)
    );
    assert_eq!(rig.state(&p.id), ProposalState::Accepted);
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    assert_eq!(rig.store.lease_holder(&rig.wiki.id).unwrap(), None);
    // The proposer was told.
    let inbox = rig.store.inbox(&author_id, false, 10).unwrap();
    assert_eq!(
        inbox.first().map(|e| e.kind),
        Some(InboxEventKind::Accepted)
    );
    assert_eq!(signed.notified, 1);
}

#[test]
fn a_moved_base_without_an_overlap_is_re_authored_and_writes_only_what_the_tip_does_not_have() {
    let (rig, maintainer, _m, _a, _author_id, p) = rig_with_proposal();
    // Someone else changed another page and the third paragraph of the same page after the proposal was made.
    rig.fake.set(
        "pages/Other.md",
        "---\ntitle: Other\nid: p-other\n---\n\nHi.\n",
    );
    let moved = HOME.replace("Three.", "Tres.");
    rig.fake.set("pages/Home.md", &moved);
    let signed = match rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap()
    {
        Accepted::Signed(s) => s,
        other => panic!("{other:?}"),
    };
    assert!(signed.reauthored);
    let call = rig.double.calls().remove(0);
    // One file: Home, with the proposer's paragraph and the tip's; Other is not touched.
    assert_eq!(call.change.files.len(), 1);
    let file = call.change.files.first().unwrap();
    assert_eq!(file.path.to_str(), Some("pages/Home.md"));
    let text = String::from_utf8(file.content.clone().unwrap()).unwrap();
    assert!(
        text.contains("Uno.") && text.contains("Tres.") && !text.contains("One."),
        "{text}"
    );
    // The message's Base is the block the proposer saw; the pin is the tip the change was written on.
    let message = Message::parse(&call.change.message).unwrap();
    assert_ne!(
        message.base().map(|b| b.as_str().to_owned()),
        Some(rig.fake.block_id())
    );
    let pin = call.pending.unwrap().pinned_tip.unwrap();
    assert_eq!(pin.as_str(), rig.fake.block_id());
}

#[test]
fn an_overlap_stops_before_the_signer_records_both_texts_and_sends_the_proposal_back() {
    let (rig, maintainer, maintainer_id, _a, author_id, p) = rig_with_proposal();
    let theirs = HOME.replace("One.", "Eins.");
    rig.fake.set("pages/Home.md", &theirs);
    let tip = rig.fake.block_id();
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap_err();
    assert_eq!(err, AcceptFailure::Overlap { overlaps: 1 });
    assert_eq!(err.word(), words::OVERLAP);
    assert!(rig.double.calls().is_empty(), "the signer was never asked");
    assert_eq!(rig.state(&p.id), ProposalState::NeedsRevision);
    // Both texts, and where, on the proposal.
    let recorded = rig.store.overlaps_of(&p.id).unwrap();
    assert_eq!(recorded.len(), 1);
    let one = recorded.first().unwrap();
    assert_eq!((one.revision, one.tip_block.as_str()), (1, tip.as_str()));
    let text = one.texts.first().unwrap();
    assert_eq!(text.page, "pages/Home.md");
    assert!(text.theirs.as_deref().unwrap().contains("Eins."));
    assert!(text.ours.as_deref().unwrap().contains("Uno."));
    assert!(text.base.as_deref().unwrap().contains("One."));
    // No acceptance, no intent, no flag, the lease is free; the proposer and the acceptor are told.
    assert!(rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    assert!(rig.store.intents(&rig.wiki.id).unwrap().is_empty());
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    assert_eq!(
        rig.leases
            .try_acquire(&rig.wiki.id, Purpose::Export, None)
            .map(drop),
        Ok(())
    );
    for who in [&author_id, &maintainer_id] {
        let kinds: Vec<_> = rig
            .store
            .inbox(who, false, 10)
            .unwrap()
            .iter()
            .map(|e| e.kind)
            .collect();
        assert_eq!(kinds, vec![InboxEventKind::NeedsRevision]);
    }
}

#[test]
fn an_unauthorized_acceptor_is_refused_before_the_lease_is_taken() {
    let (rig, _m, _mid, author, _aid, p) = rig_with_proposal();
    // The lease is held by someone else and would make any taker wait 5 s: the refusal comes at once.
    let held = rig
        .leases
        .acquire(&rig.wiki.id, Purpose::Export, None)
        .unwrap();
    let started = Instant::now();
    let (_, bystander) = rig.person("Bea", Some(Role::Reviewer));
    for who in [&author, &bystander, &Principal::Anonymous] {
        let err = rig.acceptor().accept(who, &rig.wiki, &p.id, 1).unwrap_err();
        assert_eq!(err, AcceptFailure::NotAllowed);
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "it waited for the lease"
    );
    assert_eq!(rig.leases.waiting(&rig.wiki.id), 0);
    assert!(rig.double.calls().is_empty());
    drop(held);
    assert!(rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    // The refusal was audited by the authorizer (a consequential action, allowed or not).
    assert!(rig.audit.events().iter().any(|e| e.action == "accept"));
}

#[test]
fn a_retried_accept_of_the_same_revision_finds_the_record_and_signs_once() {
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    let acceptor = rig.acceptor();
    let first = acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap();
    let Accepted::Signed(first) = first else {
        panic!()
    };
    let again = acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap();
    assert_eq!(
        again,
        Accepted::AlreadySigned {
            acceptance: first.acceptance.clone(),
            block_id: first.block_id.clone(),
            patch_id: first.patch_id.clone(),
        }
    );
    assert_eq!(rig.double.calls().len(), 1, "the signer was asked once");
    assert_eq!(rig.store.acceptances_of(&p.id, 1).unwrap().len(), 1);
}

#[test]
fn a_failure_before_the_commit_leaves_the_record_failed_with_the_word_and_the_proposal_open() {
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    rig.double
        .then(Err(AcceptError::Refused("prikk said no".to_owned())));
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap_err();
    let AcceptFailure::Failed {
        acceptance, word, ..
    } = &err
    else {
        panic!("{err:?}")
    };
    assert_eq!(*word, words::REFUSED);
    let entry = rig.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert_eq!(&entry.acceptance.id, acceptance);
    assert!(matches!(
        entry.result.map(|r| r.outcome),
        Some(AcceptanceOutcome::Failed { word }) if word == words::REFUSED
    ));
    let intents = rig.store.intents(&rig.wiki.id).unwrap();
    assert_eq!(
        intents.first().unwrap().completion.as_ref().map(|c| c.end),
        Some(IntentEnd::Abandoned)
    );
    assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
    assert_eq!(
        rig.store.recovery_flag(&rig.wiki.id).unwrap(),
        None,
        "nothing to recover"
    );
    assert!(rig.store.open_intents(&rig.wiki.id).unwrap().is_empty());
    // And the acceptor may try again: a failed record does not stand in the way.
    let again = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap();
    assert!(matches!(again, Accepted::Signed(_)));
    assert_eq!(rig.store.acceptances_of(&p.id, 1).unwrap().len(), 2);
}

#[test]
fn each_failure_before_the_commit_has_its_own_word() {
    let cases = [
        (AcceptError::Busy { holder: Some(7) }, words::BUSY),
        (
            AcceptError::Unavailable("no prikk".to_owned()),
            words::UNAVAILABLE,
        ),
        (
            AcceptError::CeilingExceeded {
                verb: "status".to_owned(),
                bytes: 1,
            },
            words::MEMORY_LIMIT,
        ),
        (AcceptError::ChangeMismatch, words::REFUSED),
        (AcceptError::MessageMismatch, words::REFUSED),
        (AcceptError::NotPending, words::REFUSED),
    ];
    for (answer, word) in cases {
        let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
        rig.double.then(Err(answer));
        let err = rig
            .acceptor()
            .accept(&maintainer, &rig.wiki, &p.id, 1)
            .unwrap_err();
        assert!(
            matches!(&err, AcceptFailure::Failed { word: w, .. } if *w == word),
            "{err:?}"
        );
        assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
        assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    }
}

#[test]
fn a_failure_after_the_commit_flags_the_wiki_and_nothing_is_committed_again() {
    for (answer, reason) in [
        (
            AcceptError::Queued {
                step: "seal",
                detail: "x".to_owned(),
                ceiling: None,
            },
            RecoveryReason::Queued,
        ),
        (
            AcceptError::WorktreeDirty("x".to_owned()),
            RecoveryReason::WorktreeDirty,
        ),
        (
            AcceptError::SealedUnconfirmed {
                sealed: None,
                why: Unconfirmed::BlockNotRead,
                detail: "x".to_owned(),
            },
            RecoveryReason::SealedUnconfirmed,
        ),
    ] {
        let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
        rig.double.then(Err(answer));
        let acceptor = rig.acceptor();
        let err = acceptor
            .accept(&maintainer, &rig.wiki, &p.id, 1)
            .unwrap_err();
        let AcceptFailure::Incomplete {
            reason: got,
            flagged,
            acceptance,
            ..
        } = &err
        else {
            panic!("{err:?}")
        };
        assert_eq!((*got, *flagged), (reason, true));
        assert_eq!(err.word(), words::NEEDS_RECOVERY);
        // The record stays pending seal and the intent open: recovery's witnesses.
        let entry = rig.store.acceptances_of(&p.id, 1).unwrap().remove(0);
        assert_eq!(&entry.acceptance.id, acceptance);
        assert_eq!(entry.result, None);
        assert_eq!(rig.store.open_intents(&rig.wiki.id).unwrap().len(), 1);
        assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
        let flag = rig.store.recovery_flag(&rig.wiki.id).unwrap().unwrap();
        assert_eq!(
            (flag.reason, flag.act),
            (reason, Some(acceptance.to_string()))
        );
        // Writes are refused with the word, by this proposal's retry and by any other proposal.
        let retry = acceptor
            .accept(&maintainer, &rig.wiki, &p.id, 1)
            .unwrap_err();
        assert!(matches!(retry, AcceptFailure::NeedsRecovery(f) if f.reason == reason));
        let (_, author) = rig.person("Other", Some(Role::Contributor));
        let q = rig.propose(
            &author,
            "Another",
            &[("pages/New.md", Some("---\ntitle: New\n---\n\nx\n"))],
        );
        assert!(matches!(
            acceptor
                .accept(&maintainer, &rig.wiki, &q.id, 1)
                .unwrap_err(),
            AcceptFailure::NeedsRecovery(_)
        ));
        assert_eq!(
            rig.double.calls().len(),
            1,
            "the signer was asked once and never again"
        );
        // Reads are still served.
        assert!(rig.reads.tip(&rig.wiki, None).is_ok());
    }
}

#[test]
fn a_seal_that_could_not_be_confirmed_is_signed_and_flagged_for_a_look() {
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    rig.double.then(Err(AcceptError::SealedUnconfirmed {
        sealed: Some(sealed('a')),
        why: Unconfirmed::VerifyCouldNotRun,
        detail: "x".to_owned(),
    }));
    let acceptor = rig.acceptor();
    let Accepted::Signed(signed) = acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap()
    else {
        panic!()
    };
    assert_eq!(signed.unconfirmed, Some(Unconfirmed::VerifyCouldNotRun));
    assert_eq!(signed.block_id, "a".repeat(64));
    assert_eq!(rig.state(&p.id), ProposalState::Accepted);
    assert_eq!(
        rig.store
            .recovery_flag(&rig.wiki.id)
            .unwrap()
            .map(|f| f.reason),
        Some(RecoveryReason::SealedUnconfirmed)
    );
    // Never retried: the retry finds the record.
    assert!(matches!(
        acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap(),
        Accepted::AlreadySigned { .. }
    ));
    assert_eq!(rig.double.calls().len(), 1);
}

#[test]
fn the_seeds_left_behind_by_a_sealed_change_are_reported_and_the_change_is_signed() {
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    rig.double.then(Err(AcceptError::SeedsNotRemoved {
        sealed: Some(sealed('c')),
    }));
    let Accepted::Signed(signed) = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap()
    else {
        panic!()
    };
    assert!(signed.seeds_left);
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
}

#[test]
fn an_earlier_act_that_never_ended_makes_the_wiki_refuse_writes_and_flags_it() {
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    // A killed process left an open intent behind.
    let q = rig.propose(
        &rig.person("Z", Some(Role::Contributor)).1,
        "Another",
        &[("pages/Z.md", Some("---\ntitle: Z\n---\n\nz\n"))],
    );
    rig.double.then(Err(AcceptError::Queued {
        step: "seal",
        detail: "x".to_owned(),
        ceiling: None,
    }));
    let _ = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &q.id, 1)
        .unwrap_err();
    // A restart: the flag is cleared by hand here (recovery is T5) but the intent is open.
    rig.store.clear_recovery_flag(&rig.wiki.id).unwrap();
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap_err();
    assert!(
        matches!(&err, AcceptFailure::NeedsRecovery(f) if f.reason == RecoveryReason::Interrupted),
        "{err:?}"
    );
    assert_eq!(rig.double.calls().len(), 1);
    assert!(rig.store.recovery_flag(&rig.wiki.id).unwrap().is_some());
}

#[test]
fn the_state_the_revision_and_the_people_are_checked_before_the_lease() {
    let (rig, maintainer, _m, author, author_id, p) = rig_with_proposal();
    let held = rig
        .leases
        .acquire(&rig.wiki.id, Purpose::Export, None)
        .unwrap();
    // A revision that is not the latest.
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 2)
        .unwrap_err();
    assert!(
        matches!(
            err,
            AcceptFailure::Proposal(ProposalError::StaleRevision { latest: 1 })
        ),
        "{err:?}"
    );
    // A proposal that does not exist, and one in another state.
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &ProposalId::new(), 1)
        .unwrap_err();
    assert_eq!(err, AcceptFailure::NotFound);
    rig.proposals
        .withdraw(&author, &rig.wiki, &p.id, "changed my mind")
        .unwrap();
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap_err();
    assert!(
        matches!(
            err,
            AcceptFailure::Proposal(ProposalError::NotInThisState { .. })
        ),
        "{err:?}"
    );
    drop(held);
    let _ = author_id;
    assert!(rig.double.calls().is_empty());
}

#[test]
fn a_maintainer_accepts_their_own_proposal_only_while_the_wiki_has_fewer_than_three() {
    let rig = Rig::new("self");
    let (_a, one) = rig.person("A", Some(Role::Maintainer));
    let p = rig.propose(
        &one,
        "Mine",
        &[("pages/A.md", Some("---\ntitle: A\n---\n\na\n"))],
    );
    // One maintainer, and another: both still below three.
    let (_b, _two) = rig.person("B", Some(Role::Maintainer));
    assert!(matches!(
        rig.acceptor().accept(&one, &rig.wiki, &p.id, 1).unwrap(),
        Accepted::Signed(_)
    ));
    // Three maintainers: the proposer may not accept their own.
    let (_c, _three) = rig.person("C", Some(Role::Maintainer));
    let q = rig.propose(
        &one,
        "Mine again",
        &[("pages/B.md", Some("---\ntitle: B\n---\n\nb\n"))],
    );
    assert_eq!(
        rig.acceptor()
            .accept(&one, &rig.wiki, &q.id, 1)
            .unwrap_err(),
        AcceptFailure::NotAllowed
    );
    let (_, other) = rig.person("D", Some(Role::Admin));
    assert!(matches!(
        rig.acceptor().accept(&other, &rig.wiki, &q.id, 1).unwrap(),
        Accepted::Signed(_)
    ));
}

#[test]
fn the_accepted_bytes_quota_refuses_by_name_before_anything_is_written() {
    let rig = Rig::with(
        "bytes",
        quick(200),
        QuotaConfig {
            accepted_bytes_per_day: 20,
            ..QuotaConfig::default()
        },
    );
    let (_m, maintainer) = rig.person("M", Some(Role::Maintainer));
    let (_a, author) = rig.person("A", Some(Role::Contributor));
    let p = rig.propose(
        &author,
        "Big",
        &[(
            "pages/Big.md",
            Some("---\ntitle: Big\n---\n\nthis is long enough\n"),
        )],
    );
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap_err();
    let AcceptFailure::Refused(q) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(q.limit.name(), "accepted-bytes-per-day");
    assert_eq!(err.word(), words::REFUSED_LIMIT);
    assert!(rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    assert!(rig.double.calls().is_empty());
    assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
}

#[test]
fn a_waiter_that_waits_out_is_told_busy_with_the_depth_and_an_estimate() {
    let rig = Rig::with("busy", quick(150), QuotaConfig::default());
    let (_m, maintainer) = rig.person("M", Some(Role::Maintainer));
    let (_a, author) = rig.person("A", Some(Role::Contributor));
    let p = rig.propose(
        &author,
        "One",
        &[("pages/A.md", Some("---\ntitle: A\n---\n\na\n"))],
    );
    let held = rig
        .leases
        .acquire(&rig.wiki.id, Purpose::Export, None)
        .unwrap();
    let err = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap_err();
    let AcceptFailure::Busy(busy) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(busy.depth, 1, "the holder");
    assert!(busy.estimate > Duration::ZERO);
    assert_eq!(busy.why, BusyWhy::Waited);
    assert_eq!(err.word(), words::BUSY);
    assert_eq!(
        rig.leases.waiting(&rig.wiki.id),
        0,
        "a waiter that gave up left the queue"
    );
    assert!(rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    drop(held);
    assert!(matches!(
        rig.acceptor()
            .accept(&maintainer, &rig.wiki, &p.id, 1)
            .unwrap(),
        Accepted::Signed(_)
    ));
}

#[test]
fn two_acceptances_racing_on_one_wiki_serialize_and_the_second_is_told_busy() {
    let rig = Rig::with("race", quick(150), QuotaConfig::default());
    let (_m, maintainer) = rig.person("M", Some(Role::Maintainer));
    let (_a, author) = rig.person("A", Some(Role::Contributor));
    let first = rig.propose(
        &author,
        "First",
        &[("pages/A.md", Some("---\ntitle: A\n---\n\na\n"))],
    );
    let second = rig.propose(
        &author,
        "Second",
        &[("pages/B.md", Some("---\ntitle: B\n---\n\nb\n"))],
    );
    // The first acceptance sits in the signer until the test lets it go.
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let go_rx = Mutex::new(go_rx);
    let entered_tx = Mutex::new(entered_tx);
    rig.double.on_call(move |_| {
        let _ = entered_tx.lock().unwrap().send(());
        let _ = go_rx.lock().unwrap().recv_timeout(Duration::from_secs(10));
    });
    let acceptor = Arc::new(rig.acceptor());
    let (wiki, who) = (rig.wiki.clone(), maintainer.clone());
    let one = {
        let (acceptor, wiki, who, id) = (
            acceptor.clone(),
            wiki.clone(),
            who.clone(),
            first.id.clone(),
        );
        std::thread::spawn(move || acceptor.accept(&who, &wiki, &id, 1))
    };
    entered_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the first is inside the signer");
    let err = acceptor
        .accept(&maintainer, &wiki, &second.id, 1)
        .unwrap_err();
    let AcceptFailure::Busy(busy) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(busy.depth, 1);
    // The durable record names the holder and the acceptance while it is inside.
    let holder = rig.store.lease_holder(&rig.wiki.id).unwrap().unwrap();
    assert_eq!(
        (holder.purpose.as_str(), holder.holder.as_str()),
        ("acceptance", "test-process")
    );
    assert!(holder.acceptance.is_some());
    go_tx.send(()).unwrap();
    assert!(matches!(one.join().unwrap().unwrap(), Accepted::Signed(_)));
    assert_eq!(
        rig.store.lease_holder(&rig.wiki.id).unwrap(),
        None,
        "cleared at release"
    );
    // The second was told busy, did nothing, and is accepted once it is its turn.
    assert!(rig.store.acceptances_of(&second.id, 1).unwrap().is_empty());
    rig.double.on_call(|_| {});
    assert!(matches!(
        acceptor.accept(&maintainer, &wiki, &second.id, 1).unwrap(),
        Accepted::Signed(_)
    ));
}

#[test]
fn a_retry_waiting_behind_the_first_finds_its_record_when_its_turn_comes() {
    let rig = Rig::with("retrywait", patient(), QuotaConfig::default());
    let (_m, maintainer) = rig.person("M", Some(Role::Maintainer));
    let (_a, author) = rig.person("A", Some(Role::Contributor));
    let p = rig.propose(
        &author,
        "Once",
        &[("pages/A.md", Some("---\ntitle: A\n---\n\na\n"))],
    );
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let entered_tx = Mutex::new(entered_tx);
    rig.double.on_call(move |_| {
        let _ = entered_tx.lock().unwrap().send(());
        std::thread::sleep(Duration::from_millis(300));
    });
    let acceptor = Arc::new(rig.acceptor());
    let (a1, w1, m1, id1) = (
        acceptor.clone(),
        rig.wiki.clone(),
        maintainer.clone(),
        p.id.clone(),
    );
    let first = std::thread::spawn(move || a1.accept(&m1, &w1, &id1, 1));
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let retry = acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap();
    assert!(matches!(retry, Accepted::AlreadySigned { .. }), "{retry:?}");
    assert!(matches!(
        first.join().unwrap().unwrap(),
        Accepted::Signed(_)
    ));
    assert_eq!(rig.double.calls().len(), 1, "sealed once");
}

fn patient() -> crate::lease::LeaseConfig {
    crate::lease::LeaseConfig {
        max_wait: Duration::from_secs(10),
        poll: Duration::from_millis(5),
        ..crate::lease::LeaseConfig::default()
    }
}

#[test]
fn the_queue_is_bounded_and_the_one_after_the_bound_is_refused_by_name() {
    let lease = crate::lease::LeaseConfig {
        queue_bound: 2,
        max_wait: Duration::from_secs(10),
        poll: Duration::from_millis(5),
        ..crate::lease::LeaseConfig::default()
    };
    let rig = Rig::with("bound", lease, QuotaConfig::default());
    let (_m, maintainer) = rig.person("M", Some(Role::Maintainer));
    let (_a, author) = rig.person("A", Some(Role::Contributor));
    let proposals: Vec<_> = (0..3)
        .map(|i| {
            let page = format!("pages/P{i}.md");
            let text = format!("---\ntitle: P{i}\n---\n\nx\n");
            rig.propose(&author, "Queued", &[(page.as_str(), Some(text.as_str()))])
        })
        .collect();
    let held = rig
        .leases
        .acquire(&rig.wiki.id, Purpose::Export, None)
        .unwrap();
    let acceptor = Arc::new(rig.acceptor());
    let mut waiting = Vec::new();
    for p in proposals.iter().take(2) {
        let (acceptor, wiki, who, id) = (
            acceptor.clone(),
            rig.wiki.clone(),
            maintainer.clone(),
            p.id.clone(),
        );
        waiting.push(std::thread::spawn(move || {
            acceptor.accept(&who, &wiki, &id, 1)
        }));
    }
    // Wait until both are in the line.
    let start = Instant::now();
    while rig.leases.waiting(&rig.wiki.id) < 2 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the waiters never queued"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let third = proposals.last().unwrap();
    let err = acceptor
        .accept(&maintainer, &rig.wiki, &third.id, 1)
        .unwrap_err();
    let AcceptFailure::Refused(q) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(
        (q.limit.name(), q.allowed, q.current),
        ("acceptance-queue", 2, 2)
    );
    drop(held);
    for w in waiting {
        assert!(matches!(w.join().unwrap().unwrap(), Accepted::Signed(_)));
    }
}

#[test]
fn the_caches_are_forgotten_and_the_follow_ups_asked_for_after_the_lease_is_released() {
    struct Count(AtomicUsize, AtomicUsize, Arc<Rig>);
    impl AfterSeal for Count {
        fn warm_title_index(&self, wiki: &WikiRecord) {
            // Asked after the lease is free: the follow-up could take it itself.
            let free = self
                .2
                .leases
                .try_acquire(&wiki.id, Purpose::Refresh, None)
                .is_ok();
            self.0.fetch_add(usize::from(free), Ordering::SeqCst);
        }
        fn enqueue_verification(&self, _: &WikiRecord) {
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    let before = rig.reads.tip(&rig.wiki, None).unwrap();
    let rig = Arc::new(rig);
    let count = Arc::new(Count(AtomicUsize::new(0), AtomicUsize::new(0), rig.clone()));
    let acceptor = rig.acceptor().with_follow_up(count.clone());
    // The tip moves while the signer works; the held answer would still say `before`.
    let fake = rig.fake.clone();
    rig.double
        .on_call(move |_| fake.set("pages/Home.md", "---\ntitle: Home\n---\n\nsealed\n"));
    acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap();
    assert_eq!(
        count.0.load(Ordering::SeqCst),
        1,
        "the title index is asked for with the lease free"
    );
    assert_eq!(
        count.1.load(Ordering::SeqCst),
        1,
        "the verification is enqueued once"
    );
    let after = rig.reads.tip(&rig.wiki, None).unwrap();
    assert_ne!(
        before.state_id, after.state_id,
        "forget_wiki dropped the held tip"
    );
}

#[test]
fn a_comments_block_gets_its_id_in_the_same_change_and_a_block_with_one_is_left_alone() {
    let (rig, maintainer, _m, _a, author_id, p) = rig_with_proposal();
    let reviewer = rig.person("Rae", Some(Role::Reviewer));
    // A comment on the second paragraph, anchored by a quote (it has no ^id).
    rig.proposals
        .comment(
            &reviewer.1,
            &rig.wiki,
            &p.id,
            NewComment {
                body: "Is this right?".to_owned(),
                parent: None,
                anchor: Some(Anchor {
                    page: "pages/Home.md".to_owned(),
                    block_id: None,
                    quote: Some(Quote {
                        exact: "Two.".to_owned(),
                        prefix: String::new(),
                        suffix: String::new(),
                    }),
                    ordinal: 1,
                }),
            },
        )
        .unwrap();
    let Accepted::Signed(signed) = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(signed.ids_attached, 1);
    let call = rig.double.calls().remove(0);
    let text =
        String::from_utf8(call.change.files.first().unwrap().content.clone().unwrap()).unwrap();
    assert!(text.contains("Two. ^c"), "{text}");
    assert!(
        text.contains("Uno.") && !text.contains("Uno. ^"),
        "the other blocks are as proposed: {text}"
    );
    // The digest the record kept is of the files with the id in them.
    let entry = rig.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert_eq!(entry.acceptance.digest, call.change.digest().to_hex());
    let _ = author_id;
}

#[test]
fn an_event_goes_only_to_someone_who_can_read_the_wiki() {
    let rig = Rig::of_visibility("notify", Visibility::Restricted);
    let (_m, maintainer) = rig.person("M", Some(Role::Maintainer));
    let (author_id, author) = rig.person("A", Some(Role::Contributor));
    let p = rig.propose(
        &author,
        "One",
        &[("pages/A.md", Some("---\ntitle: A\n---\n\na\n"))],
    );
    // The wiki is restricted and the proposer lost their role before the acceptance: they may not read it.
    rig.store.revoke(&author_id, &rig.wiki.id).unwrap();
    let Accepted::Signed(signed) = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(signed.notified, 0);
    assert!(rig.store.inbox(&author_id, false, 10).unwrap().is_empty());
}

// --- the records behind the signer's guard and the reconciliation

#[test]
fn the_ledger_finds_a_pending_acceptance_only_while_it_is_pending_and_reads_every_fact_from_the_records()
 {
    use crate::reconcile::RecordedActs;
    use inklkunn_signer::accept::{AcceptanceLedger, AcceptanceToken};
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    let acceptor = rig.acceptor();
    // During the call the guard saw the facts; afterwards nothing is pending under that token.
    acceptor.accept(&maintainer, &rig.wiki, &p.id, 1).unwrap();
    let call = rig.double.calls().remove(0);
    assert!(call.pending.is_some());
    assert_eq!(
        rig.ledger.pending(&call.token).unwrap(),
        None,
        "sealed is not pending"
    );
    // A token that names no record is not pending, whatever its shape.
    for token in [
        "",
        "not-an-id",
        &AcceptanceId::new().to_string(),
        &SystemActId::new().to_string(),
    ] {
        assert_eq!(
            rig.ledger.pending(&AcceptanceToken::new(token)).unwrap(),
            None,
            "{token}"
        );
    }
    // The reconciliation's records: the sealed acceptance, with the facts the message carries.
    let records = StoreActs::new(rig.store.clone(), rig.store.clone());
    let accepted = records.accepted(&rig.wiki.id).unwrap();
    assert_eq!(accepted.len(), 1);
    let one = accepted.first().unwrap();
    assert_eq!(one.block_id, "f".repeat(64));
    assert!(
        one.facts
            .matches(&Message::parse(&call.change.message).unwrap())
    );
    assert_eq!(one.facts.view, MAIN);
    assert!(records.system_acts(&rig.wiki.id).unwrap().is_empty());
}

#[test]
fn an_intent_whose_message_is_not_the_one_its_digest_was_made_from_is_refused_by_the_ledger() {
    use inklkunn_signer::accept::{AcceptanceLedger, AcceptanceToken};
    let (rig, maintainer, _m, _a, _aid, p) = rig_with_proposal();
    // The record the signer is asked about is read back whole while it is pending.
    let seen = Arc::new(Mutex::new(None));
    let (seen2, ledger) = (seen.clone(), rig.ledger.clone());
    rig.double.on_call(move |call| {
        *seen2.lock().unwrap() = Some(ledger.pending(&call.token));
    });
    rig.acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .unwrap();
    assert!(matches!(seen.lock().unwrap().take(), Some(Ok(Some(_)))));
    // Built by hand: a record whose digest is not its message's.
    let q = rig.propose(
        &rig.person("Q", Some(Role::Contributor)).1,
        "Q",
        &[("pages/Q.md", Some("---\ntitle: Q\n---\n\nq\n"))],
    );
    let acceptance = AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: q.id.clone(),
        revision: 1,
        wiki: rig.wiki.id.clone(),
        view: MAIN.to_owned(),
        acceptor: AccountId::new(),
        acceptor_role: Role::Maintainer,
        policy_version: POLICY_VERSION.to_owned(),
        proposer: q.author.clone(),
        digest: "0".repeat(64),
        bytes: 1,
        created_at: 1,
    };
    let (acceptor_id, _) = rig.person("R", Some(Role::Maintainer));
    let acceptance = AcceptanceRecord {
        acceptor: acceptor_id,
        ..acceptance
    };
    rig.store.begin_acceptance(&acceptance).unwrap();
    rig.store
        .record_intent(&IntentRecord {
            id: IntentId::new(),
            acceptance: acceptance.id.clone(),
            wiki: rig.wiki.id.clone(),
            view: MAIN.to_owned(),
            base_block: "a".repeat(64),
            message_digest: "b".repeat(64),
            message: "A summary\n\nKind: system\n".to_owned(),
            created_at: 1,
        })
        .unwrap();
    let err = rig
        .ledger
        .pending(&AcceptanceToken::new(acceptance.id.as_str()))
        .unwrap_err();
    assert!(err.0.contains("digest"), "{err}");
}

#[test]
fn a_system_act_is_pending_until_it_ends_and_a_failed_one_is_no_record_of_a_change() {
    use crate::reconcile::RecordedActs;
    use inklkunn_signer::accept::{AcceptanceLedger, AcceptanceToken};
    use inklkunn_store::{SystemActEnd, SystemActKind, SystemActRecord};
    let rig = Rig::new("acts-ledger");
    let message = crate::wiki::system_message("Tidy up");
    let act = |kind| SystemActRecord {
        id: SystemActId::new(),
        wiki: rig.wiki.id.clone(),
        view: MAIN.to_owned(),
        kind,
        base_block: "c".repeat(64),
        message_digest: message_digest(&message),
        message: message.clone(),
        digest: "d".repeat(64),
        bytes: 1,
        created_at: 1,
    };
    let (open, sealed_act, failed) = (
        act(SystemActKind::System),
        act(SystemActKind::System),
        act(SystemActKind::System),
    );
    for a in [&open, &sealed_act, &failed] {
        rig.store.begin_system_act(a).unwrap();
    }
    let facts = rig
        .ledger
        .pending(&AcceptanceToken::new(open.id.as_str()))
        .unwrap()
        .unwrap();
    assert_eq!((facts.kind, facts.view.as_str()), (Kind::System, MAIN));
    assert_eq!(
        facts.pinned_tip.map(|b| b.as_str().to_owned()),
        Some("c".repeat(64))
    );
    rig.store
        .finish_system_act(&SystemActEnd {
            act: sealed_act.id.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: "e".repeat(64),
                patch_id: "f".repeat(64),
            },
            at: 2,
        })
        .unwrap();
    rig.store
        .finish_system_act(&SystemActEnd {
            act: failed.id.clone(),
            outcome: AcceptanceOutcome::Failed {
                word: words::REFUSED.to_owned(),
            },
            at: 2,
        })
        .unwrap();
    for ended in [&sealed_act, &failed] {
        assert_eq!(
            rig.ledger
                .pending(&AcceptanceToken::new(ended.id.as_str()))
                .unwrap(),
            None
        );
    }
    let records = StoreActs::new(rig.store.clone(), rig.store.clone())
        .system_acts(&rig.wiki.id)
        .unwrap();
    let by_id = |id: &SystemActId| records.iter().find(|r| r.id == id.to_string());
    assert_eq!(
        by_id(&open.id).map(|r| r.block_id.clone()),
        Some(None),
        "open: no block yet"
    );
    assert_eq!(
        by_id(&sealed_act.id).map(|r| r.block_id.clone()),
        Some(Some("e".repeat(64)))
    );
    assert!(by_id(&failed.id).is_none(), "a failed act took nothing in");
}

#[test]
fn a_digest_is_sixty_four_lower_case_hex_characters_and_nothing_else() {
    for bad in [
        "",
        "abc",
        &"g".repeat(64),
        &"A".repeat(64),
        &"a".repeat(63),
        &"a".repeat(65),
    ] {
        assert!(digest_from_hex(bad).is_none(), "{bad}");
    }
    let digest = digest_from_hex(&"0123456789abcdef".repeat(4)).unwrap();
    assert_eq!(digest.to_hex(), "0123456789abcdef".repeat(4));
    assert_eq!(message_digest("a").len(), 64);
}
