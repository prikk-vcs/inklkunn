//! The curation proposal in the acceptance path (RFC 005 part 2 D-3): `community_status` changes **only** through
//! an accepted curation proposal. Each other way a change can arrive is tried here against the one gate, and the
//! signer is not asked for any of them.

use inklkunn_signer::message::SystemLabel;
use inklkunn_store::{ProposalState, Role};

use super::check::sound_claim;
use super::*;
use crate::acceptance::testing::Rig;
use crate::acceptance::{AcceptFailure, Accepted, SystemActFailure, SystemRequest};
use crate::authorize::Principal;
use crate::knowledge::compose::{Curation, LinkEdit, SourceEdit, curate};
use inklkunn_signer::PreparedFile;
use inklkunn_store::SystemActKind;

const ONE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther.\n";

struct Scene {
    rig: Rig,
    maintainer: Principal,
    author: Principal,
    claim: Claim,
    source: Source,
}

/// A wiki that has a claim, its source and the page the claim is said in.
fn scene(tag: &str) -> Scene {
    let rig = Rig::new(tag);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Admin));
    let mut claim = sound_claim(1);
    let source = Source::new(source_id(1));
    claim.sources.first_mut().expect("a link").judged_on = digest_of_block("Said. ^b1");
    rig.fake.set("pages/One.md", ONE);
    rig.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    rig.fake.set(&source_path(&source.id), &source.to_yaml());
    Scene {
        rig,
        maintainer,
        author,
        claim,
        source,
    }
}

fn accept(s: &Scene, files: &[(&str, Option<&str>)]) -> Result<Accepted, AcceptFailure> {
    let p = s.rig.propose(&s.author, "A knowledge change", files);
    let outcome = s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    if outcome.is_err() {
        assert_eq!(s.rig.state(&p.id), ProposalState::HeldForReview);
    }
    outcome
}

/// What the signer was handed, by path.
fn handed(s: &Scene) -> Vec<(String, String)> {
    s.rig
        .double
        .calls()
        .first()
        .map(|c| {
            c.change
                .files
                .iter()
                .map(|f| {
                    (
                        f.path.to_string_lossy().into_owned(),
                        String::from_utf8(f.content.clone().unwrap_or_default()).expect("text"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn status_refusal(outcome: Result<Accepted, AcceptFailure>) -> Vec<(String, Rule)> {
    match outcome {
        Err(AcceptFailure::KnowledgeInvalid(r)) => {
            r.into_iter().map(|r| (r.file, r.rule)).collect()
        }
        other => panic!("expected the knowledge refusal, got {other:?}"),
    }
}

fn contested(s: &Scene) -> String {
    let mut after = s.claim.clone();
    after.community_status = CommunityStatus {
        status: Standing::Contested,
        // A proposal that is not this one: the acceptance names its own.
        proposal: Some("01930000-0000-7000-8000-00000000cccc".to_owned()),
        judged_on: Some(digest_of_block("Said. ^b1")),
    };
    after.to_yaml()
}

#[test]
fn a_page_edit_that_also_sets_a_status_is_refused_and_the_signer_is_not_asked() {
    let s = scene("k5-page-edit");
    let outcome = accept(
        &s,
        &[
            (&claim_path(&s.claim.id), Some(&contested(&s))),
            ("pages/One.md", Some(&ONE.replace("Other.", "Other, more."))),
        ],
    );
    assert_eq!(
        status_refusal(outcome),
        [(claim_path(&s.claim.id), Rule::Status)]
    );
    assert!(s.rig.double.calls().is_empty());
}

#[test]
fn a_new_claim_that_arrives_decided_in_a_page_edit_is_refused_too() {
    let s = scene("k5-new-decided");
    let mut fresh = sound_claim(2);
    fresh.sources.first_mut().expect("a link").judged_on = digest_of_block("Said. ^b1");
    fresh.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: None,
        judged_on: Some(digest_of_block("Said. ^b1")),
    };
    let outcome = accept(
        &s,
        &[
            (&claim_path(&fresh.id), Some(&fresh.to_yaml())),
            ("pages/One.md", Some(&ONE.replace("Other.", "Other, more."))),
        ],
    );
    assert_eq!(
        status_refusal(outcome),
        [(claim_path(&fresh.id), Rule::Status)]
    );
}

#[test]
fn a_curation_proposal_sets_the_status_and_the_acceptance_names_itself_as_the_decision() {
    let s = scene("k5-curation");
    let p = s.rig.propose(
        &s.author,
        "Contested",
        &[(&claim_path(&s.claim.id), Some(&contested(&s)))],
    );
    let outcome = s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    let files = handed(&s);
    let (_, text) = files.first().expect("the claim file");
    let File::Claim(written) = read_file(&claim_path(&s.claim.id), text.as_bytes()).expect("read")
    else {
        panic!("a claim");
    };
    assert_eq!(written.community_status.status, Standing::Contested);
    // Not the proposal the file named, but the one that was accepted.
    assert_eq!(
        written.community_status.proposal.as_deref(),
        Some(p.id.as_str())
    );
}

#[test]
fn a_status_the_tip_already_has_keeps_its_deciding_proposal_whatever_a_curation_writes() {
    let s = scene("k5-forged");
    // The tip: contested, decided by proposal aaaa.
    let mut decided = s.claim.clone();
    decided.community_status = CommunityStatus {
        status: Standing::Contested,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of_block("Said. ^b1")),
    };
    s.rig.fake.set(&claim_path(&decided.id), &decided.to_yaml());
    // A curation that moves the claim to another kind and, in passing, claims bbbb decided the status.
    let mut forged = decided.clone();
    forged.kind = StatementKind::Opinion;
    forged.community_status.proposal = Some("01930000-0000-7000-8000-00000000bbbb".to_owned());
    let p = s.rig.propose(
        &s.author,
        "Another kind",
        &[(&claim_path(&forged.id), Some(&forged.to_yaml()))],
    );
    let outcome = s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    let files = handed(&s);
    let (_, text) = files.first().expect("the claim file");
    assert!(
        text.contains("01930000-0000-7000-8000-00000000aaaa"),
        "{text}"
    );
    assert!(!text.contains("bbbb"), "{text}");
}

#[test]
fn a_review_date_is_named_by_the_proposal_that_set_it_and_only_a_curation_may_set_it() {
    let s = scene("k5-reviewed");
    let now = digest_of_block("Said. ^b1");
    let edit = Curation {
        status: Standing::Unknown,
        links: vec![LinkEdit {
            source: s.source.id.clone(),
            relation: Link::Supports,
            bearing: Bearing::Direct,
        }],
        sources: vec![SourceEdit {
            source: s.source.id.clone(),
            reviewed: "2026-11-04".to_owned(),
            availability: Availability::Available,
        }],
        confirm: false,
    };
    let files =
        curate(&s.claim, std::slice::from_ref(&s.source), Some(&now), &edit).expect("curated");
    let refs: Vec<(&str, Option<&str>)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), Some(t.as_str())))
        .collect();
    // Beside a page edit: refused.
    let mut with_page = refs.clone();
    let edited = ONE.replace("Other.", "Other, more.");
    with_page.push(("pages/One.md", Some(&edited)));
    let outcome = accept(&s, &with_page);
    assert_eq!(
        status_refusal(outcome),
        [(source_path(&s.source.id), Rule::Status)]
    );
    // Alone: accepted, and the review names the proposal.
    let p = s.rig.propose(&s.author, "Checked", &refs);
    let outcome = s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    let written = handed(&s);
    let (_, text) = written.first().expect("the source file");
    assert!(text.contains(p.id.as_str()), "{text}");
    assert!(text.contains("2026-11-04"));
}

#[test]
fn a_decided_claim_is_not_removed_by_a_page_edit() {
    let s = scene("k5-removal");
    let mut decided = s.claim.clone();
    decided.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of_block("Said. ^b1")),
    };
    s.rig.fake.set(&claim_path(&decided.id), &decided.to_yaml());
    let outcome = accept(
        &s,
        &[
            (&claim_path(&decided.id), None),
            ("pages/One.md", Some(&ONE.replace("Other.", "Other, more."))),
        ],
    );
    assert_eq!(
        status_refusal(outcome),
        [(claim_path(&decided.id), Rule::Status)]
    );
}

#[test]
fn a_change_the_service_makes_for_itself_does_not_write_the_knowledge_layer() {
    let s = scene("k5-system");
    let acts = s.rig.system_acts(s.rig.double.clone());
    let label = SystemLabel::new("importer").expect("a label");
    let request = |path: &str| SystemRequest {
        kind: SystemActKind::System,
        summary: "A change".to_owned(),
        files: vec![PreparedFile {
            path: path.into(),
            content: Some(contested(&s).into_bytes()),
        }],
        renames: vec![],
        imported_from: None,
        planned_against: None,
    };
    let got = acts.run(&s.rig.wiki, &label, request(&claim_path(&s.claim.id)));
    assert!(matches!(got, Err(SystemActFailure::Invalid(_))), "{got:?}");
    assert!(s.rig.double.calls().is_empty());
    // A page is the system's to write, as before.
    let got = acts.run(&s.rig.wiki, &label, request("pages/Other.md"));
    assert!(got.is_ok(), "{got:?}");
}

#[test]
fn a_review_date_the_tip_already_has_keeps_the_proposal_that_set_it() {
    let s = scene("k5-same-day");
    let mut reviewed = s.source.clone();
    reviewed.last_reviewed = Some(Reviewed {
        date: "2026-11-04".to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    s.rig
        .fake
        .set(&source_path(&reviewed.id), &reviewed.to_yaml());
    // A curation that says the same day and names another proposal, while it makes the source available.
    let mut forged = reviewed.clone();
    forged.availability = Availability::Available;
    forged.last_reviewed = Some(Reviewed {
        date: "2026-11-04".to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000bbbb".to_owned()),
    });
    let p = s.rig.propose(
        &s.author,
        "Available",
        &[(&source_path(&forged.id), Some(&forged.to_yaml()))],
    );
    let outcome = s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    let files = handed(&s);
    let (_, text) = files.first().expect("the source file");
    assert!(text.contains("aaaa") && !text.contains("bbbb"), "{text}");
    assert!(text.contains("availability: available"));
}
