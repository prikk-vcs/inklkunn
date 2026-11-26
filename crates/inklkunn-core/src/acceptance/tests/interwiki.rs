#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The interwiki map at acceptance (RFC 007 D-8; handoff I-T4): a change to `governance/interwiki.yaml` is a governance proposal at
//! the **policy-change** rung with the policy-change approvals, it is its own proposal (a page never travels with it), each rule of
//! the file is refused by name before the signer is asked (the prefix's clash with a namespace of the wiki among them), a file that
//! does not read makes the wiki refuse every write, and an admin repairs it with a change that writes or removes it.

use inklkunn_store::{DecisionKind, ProposalState, Role};

use super::super::testing::Rig;
use super::*;
use crate::governance::Fault;
use crate::proposal::Invalid;
use crate::proposal::{ProposalEdit, ProposalError};

const MAP: &str = "wikipedia: https://en.wikipedia.org/wiki/{title}\n";
const PATH: &str = "governance/interwiki.yaml";

fn ruled(files: &[(&str, &str)]) -> Rig {
    let rig = Rig::new("interwiki");
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

fn people(rig: &Rig) -> (Principal, Principal, Principal, Principal) {
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, admin) = rig.person("Ada", Some(Role::Admin));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let (_, reviewer) = rig.person("Rin", Some(Role::Reviewer));
    (maintainer, admin, author, reviewer)
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

/// What an admin's acceptance of `text` as the map comes to, with the signer not asked when it is refused.
fn refusal_of(rig: &Rig, text: &str) -> AcceptFailure {
    let (_, admin, author, _) = people(rig);
    let p = rig.propose(&author, "The map", &[(PATH, Some(text))]);
    let failure = refused(rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1));
    assert!(rig.double.calls().is_empty(), "the signer was asked");
    assert!(rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
    failure
}

#[test]
fn a_maintainer_accepts_a_sound_map_at_the_policy_change_rung_under_the_default_constitution() {
    let rig = ruled(&[]);
    let (maintainer, _, author, _) = people(&rig);
    let p = rig.propose(&author, "Where wikipedia: leads", &[(PATH, Some(MAP))]);
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
    assert!(!record.hatch, "a maintainer is not an admin");
    assert_eq!(record.acceptor_role, Role::Maintainer);
    // It was judged by the rules in force, which are the defaults in a wiki that has no governance file yet.
    assert_eq!(record.policy_block, None);
}

#[test]
fn the_constitutions_policy_change_rung_and_approvals_are_what_a_change_to_the_map_needs() {
    let rig = ruled(&[(
        "governance/constitution.yaml",
        "policy_change:\n  rung: admin\n  approvals: 1\n",
    )]);
    let (maintainer, admin, author, reviewer) = people(&rig);
    let p = rig.propose(&author, "The map", &[(PATH, Some(MAP))]);
    // The rung first: a maintainer is below it.
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::GovernanceRung(Role::Admin)
    );
    // An admin's hatch needs no approval, as for the policy.
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
    let _ = reviewer;
}

#[test]
fn a_maintainer_needs_the_policy_changes_approvals_for_the_map_too() {
    let rig = ruled(&[(
        "governance/constitution.yaml",
        "policy_change:\n  approvals: 1\n",
    )]);
    let (maintainer, _, author, reviewer) = people(&rig);
    let p = rig.propose(&author, "The map", &[(PATH, Some(MAP))]);
    assert_eq!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::ApprovalsMissing { needed: 1, have: 0 }
    );
    rig.proposals
        .decide(&reviewer, &rig.wiki, &p.id, 1, DecisionKind::Approve, "")
        .unwrap();
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn each_rule_of_the_file_is_refused_by_name_before_the_signer_is_asked() {
    for (text, name) in [
        ("wp: http://example.org/{title}\n", "template-not-https"),
        ("wp: https://u@example.org/{title}\n", "template-userinfo"),
        ("wp: https:///{title}\n", "template-host"),
        ("wp: https://bad_host.example/{title}\n", "template-host"),
        ("wp: https://example.org/page\n", "template-placeholder"),
        (
            "wp: https://example.org/{title}/{title}\n",
            "template-placeholder",
        ),
        ("wp: https://{title}.example.org/\n", "template-placeholder"),
        (
            "wp: https://example.org/{title}#{title}\n",
            "template-placeholder",
        ),
        (
            "wp: https://example.org/{lang}/{title}\n",
            "template-placeholder",
        ),
        (
            "wp: \"https://example.org/é{title}\"\n",
            "template-character",
        ),
        ("bad_prefix: https://example.org/{title}\n", "unknown-key"),
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa: https://example.org/{title}\n",
            "bounds",
        ),
        ("wp: 7\n", "wrong-shape"),
        (
            "wp: https://a.example/{title}\nwp: https://b.example/{title}\n",
            "repeated-key",
        ),
    ] {
        let rig = ruled(&[]);
        let failure = refusal_of(&rig, text);
        assert_eq!(names(&failure), [name], "{text:?}");
        let AcceptFailure::GovernanceInvalid(faults) = &failure else {
            unreachable!()
        };
        assert_eq!(faults.first().unwrap().path(), PATH);
        assert!(!failure.to_string().contains("bad_prefix"), "{failure}");
        assert_eq!(failure.word(), words::GOVERNANCE_INVALID);
    }
}

#[test]
fn a_prefix_that_is_a_namespace_of_the_wiki_is_refused_at_acceptance() {
    let rig = ruled(&[]);
    rig.fake.set_files(&[
        (
            "pages/Help/Contents.md".to_owned(),
            "# Contents\n".to_owned(),
        ),
        ("pages/Talk:Home.md".to_owned(), "# Talk\n".to_owned()),
    ]);
    rig.reads.forget_wiki(&rig.wiki);
    for prefix in ["Help", "help", "Talk"] {
        let failure = refusal_of(&rig, &format!("{prefix}: https://example.org/{{title}}\n"));
        assert_eq!(names(&failure), ["prefix-is-namespace"], "{prefix}");
    }
    // A prefix that is no namespace of this wiki is fine, and so is the same file in a wiki that has no such namespace.
    let (maintainer, _, author, _) = people(&rig);
    let p = rig.propose(
        &author,
        "The map",
        &[(PATH, Some("docs: https://example.org/{title}\n"))],
    );
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
    let bare = ruled(&[]);
    let (maintainer, _, author, _) = people(&bare);
    let p = bare.propose(
        &author,
        "The map",
        &[(PATH, Some("help: https://example.org/{title}\n"))],
    );
    assert!(matches!(
        bare.acceptor().accept(&maintainer, &bare.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_change_to_the_map_is_its_own_proposal_a_page_never_travels_with_it() {
    let rig = ruled(&[]);
    let (_, admin, _, _) = people(&rig);
    let text = super::super::testing::HOME.replace("One.", "Uno.");
    let edit = ProposalEdit {
        summary: "A page and the map".to_owned(),
        files: vec![
            inklkunn_store::FileChange {
                path: "pages/Home.md".to_owned(),
                text: Some(text.clone()),
            },
            inklkunn_store::FileChange {
                path: PATH.to_owned(),
                text: Some(MAP.to_owned()),
            },
        ],
        ..ProposalEdit::default()
    };
    assert_eq!(
        rig.proposals
            .create_named(ProposalId::new(), &admin, &rig.wiki, None, edit)
            .map(|_| ()),
        Err(ProposalError::Invalid(Invalid::GovernanceMixed))
    );
    // One made before the rule is refused when it is accepted, by the same word, and the signer is not asked.
    let p = rig.propose_unjudged(
        &admin,
        "A page and the map",
        &[("pages/Home.md", Some(&text)), (PATH, Some(MAP))],
    );
    let failure = refused(rig.acceptor().accept(&admin, &rig.wiki, &p.id, 1));
    assert_eq!(
        failure,
        AcceptFailure::Proposal(ProposalError::Invalid(Invalid::GovernanceMixed))
    );
    assert_eq!(failure.word(), words::GOVERNANCE_MIXED);
    assert!(rig.double.calls().is_empty());
    assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
}

#[test]
fn a_map_that_does_not_read_makes_every_write_refuse_naming_the_file_and_the_line() {
    let rig = ruled(&[(
        PATH,
        "ok: https://example.org/{title}\nbad: http://example.org/{title}\n",
    )]);
    let (maintainer, admin, author, _) = people(&rig);
    let text = super::super::testing::HOME.replace("One.", "Uno.");
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
        let ProposalError::RulesUnreadable(fault) = &e else {
            panic!("{e:?}")
        };
        assert!(
            fault
                .to_string()
                .contains("governance/interwiki.yaml, line 2"),
            "{fault}"
        );
    }
    // Reading is not touched, and the fault is the rules' own.
    let rules = rig.reads.rules_of(&rig.wiki).unwrap();
    let fault = rules.fault().unwrap();
    assert_eq!(fault.file(), crate::governance::GovernanceFile::Interwiki);
}

#[test]
fn an_admin_repairs_a_map_that_does_not_read_by_rewriting_it_or_taking_it_away() {
    for repair in [Some("ok: https://example.org/{title}\n"), None] {
        let rig = ruled(&[(PATH, "bad: http://example.org/{title}\n")]);
        let (maintainer, admin, _, _) = people(&rig);
        let p = rig.propose(&admin, "Mend the map", &[(PATH, repair)]);
        // A maintainer cannot: the repair is an admin's, by the hatch.
        assert!(matches!(
            refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
            AcceptFailure::RulesUnreadable(_)
        ));
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
        assert!(record.hatch, "{repair:?}");
        assert_eq!(record.acceptor_role, Role::Admin);
    }
}

// ------------------------------------------------------------------------------------------- W-5: the rule is symmetric

const GH: &str = "gh: https://github.com/org/repo/wiki/{title}\n";

fn page_titled(title: &str, id: &str) -> String {
    format!("---\ntitle: {title}\nid: p-{id}\n---\n\nBody.\n")
}

fn path_of(title: &str) -> String {
    inklkunn_markup::path::title_to_path(title).expect("a path")
}

#[test]
fn a_new_page_in_a_mapped_namespace_is_refused_at_acceptance_before_the_signer_is_asked() {
    // The proposal was made while the wiki had no map (the proposing judges by the map it sees; this one is the acceptance's own).
    let rig = ruled(&[]);
    let (maintainer, _, author, _) = people(&rig);
    let path = path_of("GH:Notes");
    let text = page_titled("GH:Notes", "n");
    let p = rig.propose(&author, "A page", &[(&path, Some(&text))]);
    rig.fake.set_files(&[(PATH.to_owned(), GH.to_owned())]);
    rig.reads.forget_wiki(&rig.wiki);
    match refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)) {
        AcceptFailure::Proposal(ProposalError::NamespaceMapped(taken)) => {
            assert_eq!((taken.prefix(), taken.host()), ("gh", "github.com"));
        }
        other => panic!("not refused as a mapped namespace: {other:?}"),
    }
    assert!(rig.double.calls().is_empty(), "the signer was asked");
    assert!(rig.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    assert_eq!(rig.state(&p.id), ProposalState::HeldForReview);
    // The refusal has its own word.
    let taken = crate::governance::read_interwiki(GH.as_bytes())
        .unwrap()
        .takes_namespace_of("gh:x")
        .unwrap();
    assert_eq!(
        AcceptFailure::Proposal(ProposalError::NamespaceMapped(taken)).word(),
        "namespace-mapped"
    );
}

#[test]
fn a_wiki_without_the_prefix_in_its_map_accepts_the_same_page() {
    let rig = ruled(&[(PATH, MAP)]);
    let (maintainer, _, author, _) = people(&rig);
    let path = path_of("GH:Notes");
    let text = page_titled("GH:Notes", "n");
    let p = rig.propose(&author, "A page", &[(&path, Some(&text))]);
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}

#[test]
fn a_page_moved_into_a_mapped_namespace_is_refused_at_acceptance() {
    let rig = ruled(&[]);
    let (maintainer, _, author, _) = people(&rig);
    let moved = path_of("gh:Home");
    let edit = ProposalEdit {
        summary: "Move the front page".to_owned(),
        files: vec![inklkunn_store::FileChange {
            path: moved.clone(),
            text: Some(page_titled("gh:Home", "p-home")),
        }],
        renames: vec![inklkunn_store::Rename {
            from: "pages/Home.md".to_owned(),
            to: moved,
        }],
        ..ProposalEdit::default()
    };
    let draft = rig
        .proposals
        .create(&author, &rig.wiki, None, edit)
        .expect("a draft");
    let p = rig
        .proposals
        .submit(&author, &rig.wiki, &draft.id)
        .expect("sent");
    rig.fake.set_files(&[(PATH.to_owned(), GH.to_owned())]);
    rig.reads.forget_wiki(&rig.wiki);
    assert!(matches!(
        refused(rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1)),
        AcceptFailure::Proposal(ProposalError::NamespaceMapped(_))
    ));
    assert!(rig.double.calls().is_empty(), "the signer was asked");
}

#[test]
fn a_page_that_was_there_before_the_map_is_edited_as_before() {
    // `GH:Legacy` was made when the wiki had no `gh`; the map arrived after it. Reads are as they were (the map wins in the resolver),
    // and an edit of the page is an edit: only new pages and the place a page is moved to are refused.
    let legacy = path_of("GH:Legacy");
    let before = page_titled("GH:Legacy", "legacy");
    let rig = ruled(&[(legacy.as_str(), before.as_str()), (PATH, GH)]);
    let (maintainer, _, author, _) = people(&rig);
    let after = before.replace("Body.", "Edited.");
    let p = rig.propose(&author, "An edit", &[(&legacy, Some(&after))]);
    assert!(matches!(
        rig.acceptor().accept(&maintainer, &rig.wiki, &p.id, 1),
        Ok(Accepted::Signed(_))
    ));
}
