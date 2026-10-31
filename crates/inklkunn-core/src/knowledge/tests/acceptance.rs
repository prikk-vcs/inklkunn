//! The validator in the acceptance path (RFC 005 part 2, D-5): an unsound knowledge file stops the acceptance
//! **before the signer is asked**, with the typed refusal; a wiki with no knowledge layer pays nothing.

use inklkunn_prikk::StorageError;
use inklkunn_store::{ProposalState, Role};

use super::check::sound_claim;
use super::*;
use crate::acceptance::testing::{HOME, Rig};
use crate::acceptance::{AcceptFailure, Accepted, words};
use crate::authorize::Principal;

/// A page with a block a claim can stand on.
const ONE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther.\n";

struct Scene {
    rig: Rig,
    maintainer: Principal,
    author: Principal,
}

fn scene(tag: &str) -> Scene {
    let rig = Rig::new(tag);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Admin));
    rig.fake.set("pages/One.md", ONE);
    Scene {
        rig,
        maintainer,
        author,
    }
}

fn accept_proposal(
    s: &Scene,
    files: &[(&str, Option<&str>)],
) -> (
    Result<Accepted, AcceptFailure>,
    usize,
    inklkunn_store::ProposalId,
) {
    let p = s.rig.propose(&s.author, "Add knowledge", files);
    let acceptor = s.rig.acceptor();
    let outcome = acceptor.accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    if outcome.is_err() {
        assert_eq!(
            s.rig.state(&p.id),
            ProposalState::HeldForReview,
            "the proposal stays open"
        );
    }
    (outcome, s.rig.double.calls().len(), p.id)
}

fn accept(s: &Scene, files: &[(&str, Option<&str>)]) -> (Result<Accepted, AcceptFailure>, usize) {
    let (outcome, calls, _) = accept_proposal(s, files);
    (outcome, calls)
}

fn refusals(outcome: Result<Accepted, AcceptFailure>) -> Vec<(String, Rule)> {
    match outcome {
        Err(AcceptFailure::KnowledgeInvalid(r)) => {
            r.into_iter().map(|r| (r.file, r.rule)).collect()
        }
        other => panic!("expected the knowledge refusal, got {other:?}"),
    }
}

#[test]
fn an_unsound_claim_stops_the_acceptance_before_the_signer_is_asked() {
    let s = scene("k2-bad");
    let path = claim_path(&claim_id(1));
    let (outcome, signer_calls, id) =
        accept_proposal(&s, &[(&path, Some("kind: observation\nlifecycle: nope\n"))]);
    assert_eq!(signer_calls, 0, "the signer was not asked");
    let failure = outcome.as_ref().expect_err("refused");
    assert_eq!(failure.word(), words::KNOWLEDGE_INVALID);
    assert_eq!(refusals(outcome), [(path, Rule::Value)]);
    // No acceptance was recorded: nothing begins before the check.
    assert!(
        inklkunn_store::AcceptanceStore::acceptances_of(&*s.rig.store, &id, 1)
            .expect("records")
            .is_empty()
    );
}

#[test]
fn a_claim_whose_source_is_missing_is_refused_by_name_and_rule() {
    let s = scene("k2-ref");
    let claim = sound_claim(1);
    let path = claim_path(&claim.id);
    let (outcome, calls) = accept(&s, &[(&path, Some(&claim.to_yaml()))]);
    assert_eq!(calls, 0);
    let got = refusals(outcome);
    assert_eq!(got, [(path, Rule::Reference)]);
}

#[test]
fn a_sound_claim_with_its_source_and_a_page_edit_in_one_proposal_is_accepted() {
    let s = scene("k2-ok");
    let claim = sound_claim(1);
    let source = Source::new(source_id(1));
    let edited = ONE.replace("Other.", "Other, more.");
    let (outcome, calls) = accept(
        &s,
        &[
            (&claim_path(&claim.id), Some(&claim.to_yaml())),
            (&source_path(&source.id), Some(&source.to_yaml())),
            ("pages/One.md", Some(&edited)),
        ],
    );
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    assert_eq!(calls, 1);
    // The signer was handed the three files.
    let handed = s.rig.double.calls();
    let paths: Vec<String> = handed
        .first()
        .expect("a call")
        .change
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(paths.len(), 3, "{paths:?}");
}

#[test]
fn an_anchor_must_name_a_block_the_proposals_own_result_has() {
    let s = scene("k2-anchor");
    let source = Source::new(source_id(1));
    let mut claim = sound_claim(1);
    // The block does not exist in the tip: the same proposal writes it into the page, and that is sound.
    claim.expressions = vec![Expression::new("p1", "fresh").expect("an expression")];
    let with_block = ONE.replace("Other.", "Other. ^fresh");
    let files = |page_text: &str| {
        vec![
            (claim_path(&claim.id), Some(claim.to_yaml())),
            (source_path(&source.id), Some(source.to_yaml())),
            ("pages/One.md".to_owned(), Some(page_text.to_owned())),
        ]
    };
    let sound = files(&with_block);
    let sound_refs: Vec<(&str, Option<&str>)> = sound
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_deref()))
        .collect();
    let (outcome, _) = accept(&s, &sound_refs);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");

    // The next: the page says nothing of the block, though the proposal's claim does.
    let s = scene("k2-anchor-2");
    let broken = files(ONE);
    let broken_refs: Vec<(&str, Option<&str>)> = broken
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_deref()))
        .collect();
    let (outcome, calls) = accept(&s, &broken_refs);
    assert_eq!(calls, 0);
    assert_eq!(refusals(outcome), [(claim_path(&claim.id), Rule::Anchor)]);
}

#[test]
fn a_page_only_proposal_that_takes_out_a_claims_block_is_refused_naming_the_claim() {
    let s = scene("k2-page");
    let claim = sound_claim(1);
    let source = Source::new(source_id(1));
    s.rig.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    s.rig.fake.set(&source_path(&source.id), &source.to_yaml());
    let (outcome, calls) = accept(
        &s,
        &[("pages/One.md", Some(&ONE.replace("Said. ^b1", "Said.")))],
    );
    assert_eq!(calls, 0);
    assert_eq!(refusals(outcome), [(claim_path(&claim.id), Rule::Anchor)]);
    // Rewording it and keeping the id is allowed.
    let s = scene("k2-page-2");
    s.rig.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    s.rig.fake.set(&source_path(&source.id), &source.to_yaml());
    let (outcome, _) = accept(
        &s,
        &[("pages/One.md", Some(&ONE.replace("Said.", "Said, again.")))],
    );
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
}

#[test]
fn removing_a_source_a_claim_still_names_is_refused() {
    let s = scene("k2-inuse");
    let claim = sound_claim(1);
    let source = Source::new(source_id(1));
    s.rig.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    s.rig.fake.set(&source_path(&source.id), &source.to_yaml());
    let (outcome, calls) = accept(&s, &[(&source_path(&source.id), None)]);
    assert_eq!(calls, 0);
    assert_eq!(refusals(outcome), [(source_path(&source.id), Rule::InUse)]);
}

#[test]
fn a_repository_with_invalid_files_still_takes_unrelated_changes() {
    let s = scene("k2-legacy");
    s.rig
        .fake
        .set(&claim_path(&claim_id(9)), "this is: [not a claim");
    let (outcome, calls) = accept(
        &s,
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    assert_eq!(calls, 1);
}

#[test]
fn an_owned_page_key_cannot_ride_along_with_a_knowledge_change() {
    let s = scene("k2-owned");
    let claim = sound_claim(1);
    let source = Source::new(source_id(1));
    let owned = ONE.replace("title: One", "title: One\nvisibility: private");
    let (outcome, calls) = accept(
        &s,
        &[
            (&claim_path(&claim.id), Some(&claim.to_yaml())),
            (&source_path(&source.id), Some(&source.to_yaml())),
            ("pages/One.md", Some(&owned)),
        ],
    );
    assert_eq!(calls, 0);
    assert_eq!(
        refusals(outcome),
        [("pages/One.md".to_owned(), Rule::Owned)]
    );
}

#[test]
fn a_wiki_with_no_knowledge_layer_extracts_nothing_for_the_check() {
    // Were the tip extracted for an ordinary page edit, this would fail the acceptance.
    let s = scene("k2-free");
    s.rig.fake.fail_materialize(Some(StorageError::Driver {
        verb: "extract",
        detail: "must not be asked".to_owned(),
    }));
    let (outcome, calls) = accept(
        &s,
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    assert_eq!(calls, 1);
    // A change that does touch the layer needs the extract, and its failure is *unavailable*, not a verdict.
    let claim = sound_claim(1);
    let (outcome, calls) = accept(&s, &[(&claim_path(&claim.id), Some(&claim.to_yaml()))]);
    assert!(
        matches!(outcome, Err(AcceptFailure::Unavailable(_))),
        "{outcome:?}"
    );
    assert_eq!(calls, 1, "no second signer call");
}

#[test]
fn a_wiki_whose_knowledge_directory_holds_only_its_keep_file_extracts_nothing_either() {
    // A new wiki has `knowledge/.keep`: it is the common case, and it costs an acceptance nothing.
    let s = scene("k2-keep");
    s.rig.fake.set("knowledge/.keep", "");
    s.rig.fake.fail_materialize(Some(StorageError::Driver {
        verb: "extract",
        detail: "must not be asked".to_owned(),
    }));
    let (outcome, calls) = accept(
        &s,
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    assert_eq!(calls, 1);
    // (The keep-file is not a proposal's to write: the service refuses any path that is neither a page nor a
    // claim or source file, `Invalid::PathOutsidePages`, so a proposal cannot touch it at all.)
}
