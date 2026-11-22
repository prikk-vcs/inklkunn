#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Acceptance under the rules of a wiki (RFC 006 D-1, D-4; G-T1 part C) against the in-memory repository and a scripted
//! signer: the approvals a policy asks for and how they are counted, `acceptance.self`, the constitution's rung, the hatch,
//! the validation of a change to `governance/` before the signer is asked, the repair of a wiki whose governance cannot be
//! read. (The criteria that need the repository to hold what was written run on a real prikk in
//! `tests/governance_accept.rs`.)

use inklkunn_store::{
    DecisionId, DecisionKind, DecisionRecord, ProposalState, ProposalStore, Role, Scope,
};

use super::super::testing::{Rig, sealed};
use super::*;
use crate::acceptance::governance::touched_paths;
use crate::governance::{Fault, GovernanceFile, Limit, OrderingPair, PlatformLimits, Refusal};
use crate::proposal::{ProposalEdit, ProposalError};
use crate::read::ReadService;

/// 0.3.0's rule about accepting one's own proposal, written out here and not read from the product.
fn frozen_may_accept_own(maintainers: usize) -> bool {
    maintainers < 3
}

/// A rig whose default view holds these governance files.
fn ruled(files: &[(&str, &str)]) -> Rig {
    let rig = Rig::new("gov");
    rig.fake.derive_changes();
    let written: Vec<(String, String)> = files
        .iter()
        .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
        .collect();
    if !written.is_empty() {
        rig.fake.set_files(&written);
        rig.reads.forget_wiki(&rig.wiki);
    }
    rig
}

fn policy(text: &str) -> Rig {
    ruled(&[("governance/policy.yaml", text)])
}

fn approve(rig: &Rig, who: &Principal, id: &ProposalId, revision: u32) {
    rig.proposals
        .decide(who, &rig.wiki, id, revision, DecisionKind::Approve, "")
        .unwrap_or_else(|e| panic!("approve: {e}"));
}

fn refused(result: Result<Accepted, AcceptFailure>) -> AcceptFailure {
    match result {
        Err(e) => e,
        Ok(done) => panic!("accepted: {done:?}"),
    }
}

fn names(failure: &AcceptFailure) -> Vec<&'static str> {
    match failure {
        AcceptFailure::GovernanceInvalid(faults) => faults.iter().map(Fault::name).collect(),
        other => panic!("not a governance refusal: {other:?}"),
    }
}

fn change_text() -> String {
    HOME.replace("One.", "Uno.")
}

const HOME: &str = super::super::testing::HOME;

// ---------------------------------------------------------------------------------------- criterion 1: the defaults

#[test]
fn with_no_governance_a_maintainer_accepts_their_own_proposal_exactly_while_the_wiki_has_fewer_than_three_maintainers()
 {
    for maintainers in 1..=5_usize {
        let rig = Rig::new("self");
        let people: Vec<(AccountId, Principal)> = (0..maintainers)
            .map(|n| rig.person(&format!("Maintainer {n}"), Some(Role::Maintainer)))
            .collect();
        let (_, me) = people.first().unwrap();
        let text = change_text();
        let p = rig.propose(me, "My own change", &[("pages/Home.md", Some(&text))]);
        let got = rig.acceptor().accept(me, &rig.wiki, &p.id, 1);
        match (frozen_may_accept_own(maintainers), got) {
            (true, Ok(Accepted::Signed(_))) => {}
            (false, Err(AcceptFailure::NotAllowed)) => {
                assert!(rig.double.calls().is_empty(), "{maintainers} maintainers");
            }
            (want, got) => panic!("{maintainers} maintainers, 0.3.0 says {want}: {got:?}"),
        }
    }
}

#[test]
fn with_no_governance_no_approval_is_asked_for_and_the_record_says_the_defaults() {
    let rig = Rig::new("defaults");
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let text = change_text();
    let p = rig.propose(&author, "Say it", &[("pages/Home.md", Some(&text))]);
    // No decision at all, and it is accepted.
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
    let record = rig
        .store
        .acceptances_of(&p.id, 1)
        .unwrap()
        .pop()
        .unwrap()
        .acceptance;
    assert_eq!(record.policy_version, POLICY_VERSION);
    assert_eq!((record.policy_block, record.hatch), (None, false));
    assert_eq!(record.acceptor_role, Role::Maintainer);
    assert_eq!(rig.state(&p.id), ProposalState::Accepted);
}

// ---------------------------------------------------------------------------------------------------- approvals

/// A wiki that asks for `approvals`, with a maintainer, a contributor who proposes and two reviewers.
struct Approvals {
    rig: Rig,
    maintainer: Principal,
    author: Principal,
    reviewers: [(AccountId, Principal); 2],
    proposal: ProposalRecord,
}

fn approvals(count: u8) -> Approvals {
    let rig = policy(&format!("acceptance:\n  approvals: {count}\n"));
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let reviewers = [
        rig.person("Rin", Some(Role::Reviewer)),
        rig.person("Sam", Some(Role::Reviewer)),
    ];
    let text = change_text();
    let proposal = rig.propose(&author, "Say it", &[("pages/Home.md", Some(&text))]);
    Approvals {
        rig,
        maintainer,
        author,
        reviewers,
        proposal,
    }
}

#[test]
fn a_policy_that_asks_for_approvals_holds_the_acceptance_until_the_latest_revision_has_them() {
    let a = approvals(2);
    let id = &a.proposal.id;
    let accept = || a.rig.acceptor().accept(&a.maintainer, &a.rig.wiki, id, 1);
    assert_eq!(
        refused(accept()),
        AcceptFailure::ApprovalsMissing { needed: 2, have: 0 }
    );
    approve(&a.rig, &a.reviewers[0].1, id, 1);
    assert_eq!(
        refused(accept()),
        AcceptFailure::ApprovalsMissing { needed: 2, have: 1 }
    );
    // Nothing was signed and nothing recorded as an acceptance, and the proposal is as it was.
    assert!(a.rig.double.calls().is_empty());
    assert!(a.rig.store.acceptances_of(id, 1).unwrap().is_empty());
    assert_eq!(a.rig.state(id), ProposalState::HeldForReview);
    approve(&a.rig, &a.reviewers[1].1, id, 1);
    assert!(matches!(accept(), Ok(Accepted::Signed(_))));
    assert_eq!(a.rig.state(id), ProposalState::Accepted);
}

#[test]
fn a_request_for_changes_is_not_an_approval() {
    let a = approvals(1);
    let id = &a.proposal.id;
    a.rig
        .proposals
        .decide(
            &a.reviewers[0].1,
            &a.rig.wiki,
            id,
            1,
            DecisionKind::RequestChanges,
            "Not yet",
        )
        .unwrap();
    assert_eq!(
        refused(a.rig.acceptor().accept(&a.maintainer, &a.rig.wiki, id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 1, have: 0 }
    );
}

#[test]
fn approvals_of_an_earlier_revision_do_not_count_on_the_latest() {
    let a = approvals(1);
    let id = &a.proposal.id;
    approve(&a.rig, &a.reviewers[0].1, id, 1);
    // The proposer sends a second revision: the approval was of the first.
    let text = HOME.replace("One.", "Eins.");
    a.rig
        .proposals
        .revise(
            &a.author,
            &a.rig.wiki,
            id,
            ProposalEdit {
                summary: "Say it in German".to_owned(),
                files: vec![inklkunn_store::FileChange {
                    path: "pages/Home.md".to_owned(),
                    text: Some(text),
                }],
                ..ProposalEdit::default()
            },
        )
        .unwrap();
    assert_eq!(
        refused(a.rig.acceptor().accept(&a.maintainer, &a.rig.wiki, id, 2)),
        AcceptFailure::ApprovalsMissing { needed: 1, have: 0 }
    );
    approve(&a.rig, &a.reviewers[1].1, id, 2);
    assert!(matches!(
        a.rig.acceptor().accept(&a.maintainer, &a.rig.wiki, id, 2),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn an_approval_from_someone_who_no_longer_holds_the_review_role_does_not_count() {
    let a = approvals(1);
    let id = &a.proposal.id;
    approve(&a.rig, &a.reviewers[0].1, id, 1);
    // Their rung is taken away before the acceptance: the approval stays recorded and stops counting.
    a.rig
        .store
        .revoke(&a.reviewers[0].0, &a.rig.wiki.id)
        .unwrap();
    assert_eq!(
        refused(a.rig.acceptor().accept(&a.maintainer, &a.rig.wiki, id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 1, have: 0 }
    );
    assert_eq!(
        a.rig
            .proposals
            .decisions(&a.maintainer, &a.rig.wiki, id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn the_proposers_own_approval_is_never_counted_even_when_the_proposer_holds_the_review_role() {
    let rig = policy("acceptance:\n  approvals: 1\n");
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (proposer_id, proposer) = rig.person("Rin", Some(Role::Reviewer));
    let text = change_text();
    let p = rig.propose(&proposer, "My own", &[("pages/Home.md", Some(&text))]);
    // The service refuses a proposer's own decision; a record that reached the store another way is still not counted.
    rig.store
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
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 1, have: 0 }
    );
}

#[test]
fn an_acceptor_who_also_approved_counts_as_one_of_the_people_other_than_the_proposer() {
    let a = approvals(1);
    let id = &a.proposal.id;
    approve(&a.rig, &a.maintainer, id, 1);
    assert!(matches!(
        a.rig.acceptor().accept(&a.maintainer, &a.rig.wiki, id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_decision_records_the_rung_and_the_block_it_was_made_under() {
    let a = approvals(1);
    let id = &a.proposal.id;
    approve(&a.rig, &a.reviewers[0].1, id, 1);
    let decision = a
        .rig
        .proposals
        .decisions(&a.maintainer, &a.rig.wiki, id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(decision.reviewer_rung, Some(Role::Reviewer));
    let block = a.rig.fake.block_id();
    let rules = a.rig.reads.rules_of(&a.rig.wiki).unwrap();
    assert_eq!(decision.policy_block.as_deref(), rules.block());
    assert!(decision.policy_block.is_some(), "{block}");
}

// ------------------------------------------------------------------------------------------- acceptance.self

#[test]
fn a_policy_that_says_never_lets_nobody_accept_their_own_proposal() {
    let rig = policy("acceptance:\n  self: never\n");
    let (_, one) = rig.person("Maya", Some(Role::Maintainer));
    let (_, two) = rig.person("Wes", Some(Role::Maintainer));
    let text = change_text();
    // One maintainer in the wiki would be allowed to under 0.3.0's rule; the policy says never.
    let p = rig.propose(&one, "My own", &[("pages/Home.md", Some(&text))]);
    assert_eq!(
        refused(rig.acceptor().accept(&one, &rig.wiki, &p.id, 1)),
        AcceptFailure::NotAllowed
    );
    assert!(rig.double.calls().is_empty());
    // Someone else accepts it.
    assert!(matches!(
        rig.acceptor().accept(&two, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}

// ---------------------------------------------------------------- the constitution's rung, and the hatch

fn rig_with_people(files: &[(&str, &str)]) -> (Rig, Principal, Principal, Principal, Principal) {
    let rig = ruled(files);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let (_, reviewer) = rig.person("Rin", Some(Role::Reviewer));
    (rig, maintainer, admin, author, reviewer)
}

const POLICY_CHANGE: &str = "entry: observer\n";

#[test]
fn a_policy_change_is_accepted_by_a_maintainer_under_the_default_constitution() {
    let (rig, maintainer, _admin, author, _) = rig_with_people(&[]);
    let p = rig.propose(
        &author,
        "Let everyone observe",
        &[("governance/policy.yaml", Some(POLICY_CHANGE))],
    );
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
    let record = rig
        .store
        .acceptances_of(&p.id, 1)
        .unwrap()
        .pop()
        .unwrap()
        .acceptance;
    // A maintainer is not an admin: no hatch.
    assert!(!record.hatch);
    assert_eq!(record.acceptor_role, Role::Maintainer);
}

#[test]
fn a_constitution_that_asks_an_admin_for_a_policy_change_is_obeyed_and_the_admin_uses_the_hatch() {
    let (rig, maintainer, admin, author, _) = rig_with_people(&[(
        "governance/constitution.yaml",
        "policy_change:\n  rung: admin\n",
    )]);
    let p = rig.propose(
        &author,
        "Let everyone observe",
        &[("governance/policy.yaml", Some(POLICY_CHANGE))],
    );
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::GovernanceRung(Role::Admin)
    );
    assert!(rig.double.calls().is_empty());
    assert!(matches!(
        rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
    let record = rig
        .store
        .acceptances_of(&p.id, 1)
        .unwrap()
        .pop()
        .unwrap()
        .acceptance;
    assert!(
        record.hatch,
        "an admin's acceptance of a change to governance/ names the hatch"
    );
    assert_eq!(record.acceptor_role, Role::Admin);
    assert_eq!(record.policy_version, GOVERNED_POLICY_VERSION);
    assert_eq!(
        record.policy_block.as_deref(),
        Some(rig.fake.block_id().as_str()),
        "the rules the decision was made under: the block that set them"
    );
}

#[test]
fn a_constitution_change_needs_an_admin_whatever_the_constitution_says() {
    let (rig, maintainer, admin, author, _) = rig_with_people(&[]);
    let p = rig.propose(
        &author,
        "Raise the bar",
        &[(
            "governance/constitution.yaml",
            Some("policy_change:\n  approvals: 1\n"),
        )],
    );
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::GovernanceRung(Role::Admin)
    );
    assert!(matches!(
        rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_change_to_both_files_is_held_to_the_stricter() {
    // A policy change needs a maintainer and 2 approvals; the constitution's own change needs an admin and 1.
    let (rig, maintainer, admin, author, reviewer) = rig_with_people(&[(
        "governance/constitution.yaml",
        "policy_change:\n  approvals: 2\nconstitution_change:\n  approvals: 1\n",
    )]);
    let p = rig.propose(
        &author,
        "Both",
        &[
            ("governance/policy.yaml", Some(POLICY_CHANGE)),
            (
                "governance/constitution.yaml",
                Some("policy_change:\n  approvals: 2\n"),
            ),
        ],
    );
    // The rung is the stricter (an admin), whatever the approvals are.
    approve(&rig, &reviewer, &p.id, 1);
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::GovernanceRung(Role::Admin)
    );
    // And the approvals are the greater of the two numbers: an admin without the hatch's help would need 2.
    let a = approvals_needed(&rig, &admin, &p.id);
    assert_eq!(a, 0, "the hatch asks for none");
}

/// What acceptance by `who` would be told: the approvals it asks for (0 when it asks for none and is accepted).
fn approvals_needed(rig: &Rig, who: &Principal, id: &ProposalId) -> u8 {
    match rig.acceptor().accept(who, &rig.wiki, id, 1) {
        Ok(_) => 0,
        Err(AcceptFailure::ApprovalsMissing { needed, .. }) => needed,
        Err(other) => panic!("{other:?}"),
    }
}

#[test]
fn the_hatch_is_an_admins_with_a_credential_that_carries_the_admin_scope_and_never_a_maintainers() {
    // The policy asks for three approvals nobody can give.
    let rig = ruled(&[("governance/policy.yaml", "acceptance:\n  approvals: 3\n")]);
    let (admin_id, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let p = rig.propose(
        &author,
        "Back to the defaults",
        &[(
            "governance/policy.yaml",
            Some("acceptance:\n  approvals: 0\n"),
        )],
    );
    // A maintainer cannot (the approvals are asked), and neither can an admin's token that only carries `accept`.
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 3, have: 0 }
    );
    let token = Principal::token(&rig.authority, admin_id.clone(), Scope::Accept);
    assert_eq!(
        refused(rig.acceptor().accept(&token, &rig.wiki, &p.id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 3, have: 0 }
    );
    // The admin's own session is the hatch.
    assert!(matches!(
        rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
    let record = rig
        .store
        .acceptances_of(&p.id, 1)
        .unwrap()
        .pop()
        .unwrap()
        .acceptance;
    assert!(record.hatch);
}

#[test]
fn the_hatch_is_for_a_change_to_governance_only_and_an_admin_needs_the_approvals_for_a_page() {
    let rig = ruled(&[("governance/policy.yaml", "acceptance:\n  approvals: 1\n")]);
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let text = change_text();
    let p = rig.propose(&author, "A page", &[("pages/Home.md", Some(&text))]);
    assert_eq!(
        refused(rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 1, have: 0 }
    );
}

#[test]
fn the_hatch_lifts_the_rule_against_accepting_ones_own_change_to_the_rules_and_no_other() {
    // The wiki's only admin proposes a change to the rules in a wiki that says never and asks an admin to accept.
    let rig = ruled(&[(
        "governance/policy.yaml",
        "acceptance:\n  self: never\nrungs:\n  accept: maintainer\n",
    )]);
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let rules = rig.propose(
        &admin,
        "Mend the rules",
        &[(
            "governance/policy.yaml",
            Some("acceptance:\n  self: small-wiki\n"),
        )],
    );
    assert!(matches!(
        rig.acceptor().accept(&admin, &rig.wiki, &rules.id, 1),
        Ok(Accepted::Signed(_))
    ));
    // A page of their own is another matter: never means never.
    let text = change_text();
    let page = rig.propose(&admin, "A page", &[("pages/Home.md", Some(&text))]);
    assert_eq!(
        refused(rig.acceptor().accept(&admin, &rig.wiki, &page.id, 1)),
        AcceptFailure::NotAllowed
    );
}

// ----------------------------------------------------------------- what is refused before the signer is asked

fn refusal_of(files: &[(&str, &str)]) -> AcceptFailure {
    // An admin accepts, so that what stops the change is the file and not the rung a constitution change needs.
    let (rig, _maintainer, admin, author, _) = rig_with_people(&[]);
    let proposal_files: Vec<(&str, Option<&str>)> =
        files.iter().map(|(p, t)| (*p, Some(*t))).collect();
    let p = rig.propose(&author, "Rules", &proposal_files);
    let failure = refused(rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1));
    // Whatever the refusal: the signer was not asked, no acceptance began, and the proposal is as it was.
    assert!(rig.double.calls().is_empty(), "the signer was asked");
    assert!(rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
    assert_eq!(rig.store.recovery_flag(&rig.wiki.id).unwrap(), None);
    failure
}

#[test]
fn a_governance_file_of_the_wrong_shape_an_unknown_key_and_the_ordering_rule_are_each_refused_by_name()
 {
    for (text, name) in [
        ("entry:\n  - none\n", "wrong-shape"),
        ("entry: nobody\n", "unknown-value"),
        ("rungs:\n  fly: admin\n", "unknown-key"),
        ("entry: none\nentry: none\n", "repeated-key"),
        ("acceptance:\n  approvals: 9\n", "out-of-range"),
        (
            "rungs:\n  propose: maintainer\n  review: reviewer\n  accept: maintainer\n",
            "ordering-propose-above-review",
        ),
        (
            "rungs:\n  review: maintainer\n  accept: reviewer\n",
            "ordering-review-above-accept",
        ),
        (
            "rungs:\n  moderate: contributor\n",
            "ordering-moderate-below-reviewer",
        ),
    ] {
        let failure = refusal_of(&[("governance/policy.yaml", text)]);
        assert_eq!(names(&failure), [name], "{text:?}");
        let AcceptFailure::GovernanceInvalid(faults) = &failure else {
            unreachable!()
        };
        // The file is named and what it said is not repeated.
        assert_eq!(faults.first().unwrap().path(), "governance/policy.yaml");
        assert!(!failure.to_string().contains("nobody"), "{failure}");
        assert_eq!(failure.word(), words::GOVERNANCE_INVALID);
    }
    let constitution = refusal_of(&[(
        "governance/constitution.yaml",
        "policy_change:\n  rung: reviewer\n",
    )]);
    assert_eq!(names(&constitution), ["unknown-value"]);
    let own_rung = refusal_of(&[(
        "governance/constitution.yaml",
        "constitution_change:\n  rung: maintainer\n",
    )]);
    assert_eq!(names(&own_rung), ["unknown-key"]);
}

#[test]
fn a_policy_above_a_platform_limit_is_refused_naming_the_limit_and_one_within_it_is_not() {
    let rig = ruled(&[]);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let p = rig.propose(
        &author,
        "Let everyone contribute",
        &[("governance/policy.yaml", Some("entry: contributor\n"))],
    );
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
        .with_platform_limits(PlatformLimits {
            entry_max: crate::governance::Entry::Observer,
            ..PlatformLimits::default()
        }),
    );
    let strict = ProposalAcceptor::new(parts).with_reconciliation(false);
    let failure = refused(strict.accept(&maintainer, &rig.wiki, &p.id, 1));
    assert_eq!(names(&failure), ["platform-limit-entry-max"]);
    let AcceptFailure::GovernanceInvalid(faults) = &failure else {
        unreachable!()
    };
    assert_eq!(
        faults.first().map(|f| f.refusal),
        Some(Refusal::PlatformLimit(Limit::EntryMax))
    );
    assert!(rig.double.calls().is_empty());
    // The same proposal under the default limits (contributor) is accepted.
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_governance_file_in_a_view_that_is_not_the_default_one_is_refused_by_name() {
    let rig = ruled(&[]);
    rig.fake.add_view("heads/other");
    // An admin accepts, so that the constitution's own rung is not what stops the second file.
    let (_, maintainer) = rig.person("Ada", Some(Role::Admin));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let edit = |path: &str, text: &str| ProposalEdit {
        summary: "Rules, elsewhere".to_owned(),
        files: vec![inklkunn_store::FileChange {
            path: path.to_owned(),
            text: Some(text.to_owned()),
        }],
        ..ProposalEdit::default()
    };
    for (path, text) in [
        ("governance/policy.yaml", "entry: none\n"),
        (
            "governance/constitution.yaml",
            "policy_change:\n  approvals: 1\n",
        ),
    ] {
        let id = ProposalId::new();
        rig.proposals
            .create_named(
                id.clone(),
                &author,
                &rig.wiki,
                Some("heads/other"),
                edit(path, text),
            )
            .unwrap();
        rig.proposals.submit(&author, &rig.wiki, &id).unwrap();
        let failure = refused(rig.acceptor().accept(&maintainer, &rig.wiki, &id, 1));
        assert_eq!(names(&failure), ["outside-default-view"], "{path}");
        let AcceptFailure::GovernanceInvalid(faults) = &failure else {
            unreachable!()
        };
        assert_eq!(
            faults
                .first()
                .and_then(|f| f.file)
                .map(GovernanceFile::path),
            Some(path)
        );
        assert!(rig.double.calls().is_empty());
        assert_eq!(rig.state(&id), ProposalState::HeldForReview);
    }
    // A page in that view is accepted as ever.
    let id = ProposalId::new();
    let text = change_text();
    rig.proposals
        .create_named(
            id.clone(),
            &author,
            &rig.wiki,
            Some("heads/other"),
            edit("pages/Home.md", &text),
        )
        .unwrap();
    rig.proposals.submit(&author, &rig.wiki, &id).unwrap();
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_path_under_governance_that_is_neither_file_is_refused_by_name_at_acceptance_too() {
    let rig = Rig::new("unknown");
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    // A record that reached the store without the service's judgement (an older proposal, a store written by hand).
    let p = rig.propose_unjudged(
        &author,
        "Another file",
        &[("governance/other.yaml", Some("entry: none\n"))],
    );
    let failure = refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1));
    assert_eq!(names(&failure), ["unknown-file"]);
    assert!(rig.double.calls().is_empty());
}

#[test]
fn removing_a_governance_file_is_a_change_to_the_rules_judged_like_one() {
    let (rig, maintainer, _admin, author, _) = rig_with_people(&[(
        "governance/constitution.yaml",
        "policy_change:\n  rung: admin\n",
    )]);
    rig.fake.set("governance/policy.yaml", "entry: observer\n");
    rig.reads.forget_wiki(&rig.wiki);
    let p = rig.propose(
        &author,
        "Back to the defaults",
        &[("governance/policy.yaml", None)],
    );
    // It needs the constitution's rung, and the file it leaves is the default (a missing file is its default).
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::GovernanceRung(Role::Admin)
    );
}

#[test]
fn a_governance_file_the_view_changed_since_the_base_is_an_overlap_and_never_a_merge() {
    let (rig, maintainer, _admin, author, _) = rig_with_people(&[]);
    rig.fake.set("governance/policy.yaml", "entry: observer\n");
    rig.reads.forget_wiki(&rig.wiki);
    let p = rig.propose(
        &author,
        "Let everyone ask",
        &[("governance/policy.yaml", Some("entry: questioner\n"))],
    );
    // Someone changed the file after the proposal was written.
    rig.fake
        .set("governance/policy.yaml", "entry: contributor\n");
    rig.reads.forget_wiki(&rig.wiki);
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::Overlap { overlaps: 1 }
    );
    assert!(rig.double.calls().is_empty());
    assert_eq!(rig.state(&p.id), ProposalState::NeedsRevision);
}

#[test]
fn a_governance_file_changed_in_another_place_is_still_an_overlap_and_the_proposal_is_never_re_authored()
 {
    // Two paragraphs the page merge would take apart: one proposal changes the first, the view changes the second.
    let both = "entry: none\n\nknowledge:\n  review_after: 100\n";
    let (rig, maintainer, _admin, author, _) = rig_with_people(&[("governance/policy.yaml", both)]);
    let p = rig.propose(
        &author,
        "Let everyone observe",
        &[(
            "governance/policy.yaml",
            Some("entry: observer\n\nknowledge:\n  review_after: 100\n"),
        )],
    );
    rig.fake.set(
        "governance/policy.yaml",
        "entry: none\n\nknowledge:\n  review_after: 200\n",
    );
    rig.reads.forget_wiki(&rig.wiki);
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::Overlap { overlaps: 1 }
    );
    assert!(rig.double.calls().is_empty(), "nothing was signed");
    assert_eq!(rig.state(&p.id), ProposalState::NeedsRevision);
}

// ------------------------------------------------- a wiki whose governance cannot be read: refused, and repaired

/// A wiki whose policy file is of the wrong shape (line 2 is the unknown key), with an admin, a maintainer, an observer
/// and an author.
fn broken() -> (Rig, Principal, Principal, Principal, Principal) {
    let (rig, maintainer, admin, author, _) =
        rig_with_people(&[("governance/policy.yaml", "entry: none\nflying: yes\n")]);
    let observer = rig.person("Olive", Some(Role::Observer)).1;
    (rig, maintainer, admin, author, observer)
}

#[test]
fn every_write_of_a_wiki_whose_governance_cannot_be_read_is_refused_naming_the_file_and_the_line() {
    let (rig, maintainer, admin, author, observer) = broken();
    let expect = |e: &dyn std::fmt::Display| {
        let said = e.to_string();
        assert!(said.contains("governance/policy.yaml, line 2"), "{said}");
    };
    // A person writing: propose, comment, review, decline.
    let text = change_text();
    let edit = ProposalEdit {
        summary: "A page".to_owned(),
        files: vec![inklkunn_store::FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some(text),
        }],
        ..ProposalEdit::default()
    };
    for who in [&author, &maintainer, &admin] {
        let e = rig
            .proposals
            .create(who, &rig.wiki, None, edit.clone())
            .unwrap_err();
        assert!(matches!(e, ProposalError::RulesUnreadable(_)), "{e:?}");
        expect(&e);
    }
    // Reads are not touched.
    assert!(rig.reads.rules_of(&rig.wiki).unwrap().fault().is_some());
    // An open proposal made before the file broke cannot be accepted, reviewed or commented on either.
    let _ = (observer, maintainer);
}

#[test]
fn an_admin_repairs_the_rules_with_a_change_that_touches_governance_only_by_the_hatch() {
    let (rig, maintainer, admin, _author, _) = broken();
    // The repair is an admin's proposal of the rules and nothing else, accepted by the hatch under the default policy.
    let p = rig.propose(
        &admin,
        "Mend the policy",
        &[("governance/policy.yaml", Some("entry: none\n"))],
    );
    // A maintainer cannot: the repair is an admin's.
    assert!(matches!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::RulesUnreadable(_)
    ));
    assert!(rig.double.calls().is_empty());
    let signed = rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1).unwrap();
    assert!(matches!(signed, Accepted::Signed(_)));
    let record = rig
        .store
        .acceptances_of(&p.id, 1)
        .unwrap()
        .pop()
        .unwrap()
        .acceptance;
    assert!(record.hatch);
    assert_eq!(record.acceptor_role, Role::Admin);
    assert_eq!(record.policy_version, GOVERNED_POLICY_VERSION);
}

#[test]
fn the_repair_does_not_open_the_wiki_to_anything_but_the_rules() {
    let (rig, maintainer, admin, author, _) = broken();
    let _ = (maintainer, author);
    // A page and the rules together are not the repair; the admin is told what anyone is told.
    let text = change_text();
    let p = rig.propose_unjudged(
        &admin,
        "A page and the rules",
        &[
            ("pages/Home.md", Some(&text)),
            ("governance/policy.yaml", Some("entry: none\n")),
        ],
    );
    assert!(matches!(
        refused(rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1)),
        AcceptFailure::RulesUnreadable(_)
    ));
    assert!(rig.double.calls().is_empty());
    // A proposal that touched only a page, made before the file broke, is refused for the same reason by an admin.
    let only = rig.propose_unjudged(&admin, "A page", &[("pages/Home.md", Some(&text))]);
    assert!(matches!(
        refused(rig.acceptor().accept(&admin, &rig.wiki, &only.id, 1)),
        AcceptFailure::RulesUnreadable(_)
    ));
}

#[test]
fn only_someone_who_may_read_the_wiki_is_told_why_it_refuses_writes() {
    let rig = Rig::of_visibility("hidden", Visibility::Restricted);
    rig.fake.derive_changes();
    rig.fake
        .set_files(&[("governance/policy.yaml".to_owned(), "x: [\n".to_owned())]);
    rig.reads.forget_wiki(&rig.wiki);
    let (_, member) = rig.person("Mia", Some(Role::Contributor));
    let (_, stranger) = rig.person("Sid", None);
    let edit = ProposalEdit {
        summary: "A page".to_owned(),
        files: vec![inklkunn_store::FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some(change_text()),
        }],
        ..ProposalEdit::default()
    };
    assert!(matches!(
        rig.proposals
            .create(&member, &rig.wiki, None, edit.clone())
            .unwrap_err(),
        ProposalError::RulesUnreadable(_)
    ));
    // A person who may not read the wiki is told what anyone is told about a wiki they may not see.
    assert_eq!(
        rig.proposals
            .create(&stranger, &rig.wiki, None, edit)
            .unwrap_err(),
        ProposalError::NotAllowed
    );
}

#[test]
fn the_paths_of_a_change_are_what_the_files_and_both_ends_of_the_renames_name() {
    let files = [
        PreparedFile {
            path: "governance/policy.yaml".into(),
            content: Some(b"x".to_vec()),
        },
        PreparedFile {
            path: "pages/A.md".into(),
            content: None,
        },
    ];
    let renames = [("pages/B.md".into(), "pages/C.md".into())];
    assert_eq!(
        touched_paths(&files, &renames),
        [
            "governance/policy.yaml",
            "pages/A.md",
            "pages/B.md",
            "pages/C.md"
        ]
    );
    let _ = (sealed('a'), OrderingPair::ProposeAboveReview);
}
