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
use crate::proposal::Invalid;
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
    let p = s
        .rig
        .propose_unjudged(&s.author, "A knowledge change", files);
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

// ------------------------------------------------------------------------------------ K7: refused when it is sent

fn edit(files: &[(&str, Option<&str>)]) -> crate::proposal::ProposalEdit {
    crate::proposal::ProposalEdit {
        summary: "A knowledge change".to_owned(),
        files: files
            .iter()
            .map(|(path, text)| inklkunn_store::FileChange {
                path: (*path).to_owned(),
                text: text.map(str::to_owned),
            })
            .collect(),
        ..crate::proposal::ProposalEdit::default()
    }
}

/// What submitting `files` as a draft comes to, and what a revision of a held proposal with them comes to.
fn sent(s: &Scene, files: &[(&str, Option<&str>)]) -> (Result<(), Invalid>, Result<(), Invalid>) {
    let as_invalid = |r: Result<_, crate::proposal::ProposalError>| match r {
        Ok(_) => Ok(()),
        Err(crate::proposal::ProposalError::Invalid(why)) => Err(why),
        Err(other) => panic!("not the refusal of the knowledge files: {other:?}"),
    };
    let service = &s.rig.proposals;
    let draft = service
        .create(&s.author, &s.rig.wiki, None, edit(files))
        .expect("a draft is saved whatever it holds");
    let submitted = as_invalid(service.submit(&s.author, &s.rig.wiki, &draft.id));
    // A revision: of a proposal that is held for review (a plain one first).
    let held = s.rig.propose(
        &s.author,
        "A page edit",
        &[("pages/One.md", Some(&format!("{ONE}\nMore.\n")))],
    );
    let revised = as_invalid(service.revise(&s.author, &s.rig.wiki, &held.id, edit(files)));
    (submitted, revised)
}

#[test]
fn a_status_beside_a_page_edit_is_refused_when_it_is_sent_and_at_every_revision() {
    let s = scene("k7-status");
    let edited = ONE.replace("Other.", "Other, more.");
    let files = [
        (claim_path(&s.claim.id), contested(&s)),
        ("pages/One.md".to_owned(), edited),
    ];
    let given: Vec<(&str, Option<&str>)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), Some(t.as_str())))
        .collect();
    assert_eq!(
        sent(&s, &given),
        (
            Err(Invalid::KnowledgeCurated),
            Err(Invalid::KnowledgeCurated)
        )
    );
}

#[test]
fn every_way_the_acceptance_refuses_is_refused_when_it_is_sent() {
    let s = scene("k7-ways");
    let page = ("pages/One.md", Some(ONE));
    let refused = |files: &[(&str, Option<&str>)]| {
        let mut with_page = files.to_vec();
        with_page.push(page);
        sent(&s, &with_page)
    };
    let both = (
        Err(Invalid::KnowledgeCurated),
        Err(Invalid::KnowledgeCurated),
    );
    // A source's availability.
    let mut available = s.source.clone();
    available.availability = Availability::Available;
    assert_eq!(
        refused(&[(&source_path(&available.id), Some(&available.to_yaml()))]),
        both
    );
    // A link the claim already has, changed in each of its fields (K6).
    for edit_link in [
        (|l: &mut SourceLink| l.relation = Link::Challenges) as fn(&mut SourceLink),
        |l| l.bearing = Bearing::Indirect,
        |l| l.judged_on = digest_of("elsewhere"),
    ] {
        let mut relinked = s.claim.clone();
        edit_link(relinked.sources.first_mut().expect("a link"));
        assert_eq!(
            refused(&[(&claim_path(&relinked.id), Some(&relinked.to_yaml()))]),
            both
        );
    }
    // The link taken away.
    let mut unlinked = s.claim.clone();
    unlinked.sources.clear();
    assert_eq!(
        refused(&[(&claim_path(&unlinked.id), Some(&unlinked.to_yaml()))]),
        both
    );
    // A claim whose standing was decided, removed (K7).
    let mut decided = s.claim.clone();
    decided.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of_block("Said. ^b1")),
    };
    s.rig.fake.set(&claim_path(&decided.id), &decided.to_yaml());
    assert_eq!(refused(&[(&claim_path(&decided.id), None)]), both);
}

#[test]
fn what_the_acceptance_lets_through_is_sent() {
    let s = scene("k7-through");
    // A new link beside the page edit it belongs to, a new claim and a new source, and a retitled source.
    let mut more = s.claim.clone();
    more.sources.push(SourceLink {
        source: source_id(2),
        relation: Link::Supports,
        bearing: Bearing::Partial,
        note: None,
        judged_on: digest_of_block("Said. ^b1"),
    });
    let other = Source::new(source_id(2));
    let mut retitled = s.source.clone();
    retitled.identity.title = Some("A title".to_owned());
    let files = [
        (claim_path(&more.id), more.to_yaml()),
        (source_path(&other.id), other.to_yaml()),
        (source_path(&retitled.id), retitled.to_yaml()),
        ("pages/One.md".to_owned(), ONE.replace("Other.", "More.")),
    ];
    let given: Vec<(&str, Option<&str>)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), Some(t.as_str())))
        .collect();
    assert_eq!(sent(&s, &given), (Ok(()), Ok(())));
}

#[test]
fn a_curation_proposal_is_sent_and_a_draft_is_saved_whatever_it_holds() {
    let s = scene("k7-curation");
    let service = &s.rig.proposals;
    let curated = [(claim_path(&s.claim.id), contested(&s))];
    let given: Vec<(&str, Option<&str>)> = curated
        .iter()
        .map(|(p, t)| (p.as_str(), Some(t.as_str())))
        .collect();
    let draft = service
        .create(&s.author, &s.rig.wiki, None, edit(&given))
        .expect("a draft");
    assert!(service.submit(&s.author, &s.rig.wiki, &draft.id).is_ok());
    // Saving a draft is not sending it: a draft may hold what it will not be allowed to send.
    let with_page = [
        (claim_path(&s.claim.id), contested(&s)),
        ("pages/One.md".to_owned(), ONE.replace("Other.", "More.")),
    ];
    let given: Vec<(&str, Option<&str>)> = with_page
        .iter()
        .map(|(p, t)| (p.as_str(), Some(t.as_str())))
        .collect();
    let draft = service
        .create(&s.author, &s.rig.wiki, None, edit(&given))
        .expect("a draft");
    assert!(
        service
            .save_draft(&s.author, &s.rig.wiki, &draft.id, edit(&given))
            .is_ok()
    );
}

// ------------------------------------------------------------------------------------ K14: un-deciding is a decision

#[test]
fn setting_a_status_back_to_not_decided_records_the_proposal_that_did_it() {
    let s = scene("k14-undecide");
    let mut decided = s.claim.clone();
    decided.community_status = CommunityStatus {
        status: Standing::Contested,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of_block("Said. ^b1")),
    };
    s.rig.fake.set(&claim_path(&decided.id), &decided.to_yaml());
    // A curation that un-decides it; the file names no proposal (or a forged one): the acceptance names its own.
    let mut undecided = decided.clone();
    undecided.community_status = CommunityStatus {
        status: Standing::Unknown,
        proposal: Some("01930000-0000-7000-8000-00000000bbbb".to_owned()),
        judged_on: None,
    };
    let p = s.rig.propose(
        &s.author,
        "Not decided after all",
        &[(&claim_path(&undecided.id), Some(&undecided.to_yaml()))],
    );
    let outcome = s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    let files = handed(&s);
    let (_, text) = files.first().expect("the claim file");
    let Ok(File::Claim(written)) = read_file(&claim_path(&s.claim.id), text.as_bytes()) else {
        panic!("a claim");
    };
    assert_eq!(written.community_status.status, Standing::Unknown);
    assert_eq!(
        written.community_status.proposal.as_deref(),
        Some(p.id.as_str())
    );
}

#[test]
fn a_claim_nobody_decided_stays_without_a_deciding_proposal_when_a_curation_leaves_it_so() {
    let s = scene("k14-still-undecided");
    // The tip: not decided. A curation that changes only the kind and says not decided.
    let mut same = s.claim.clone();
    same.kind = StatementKind::Opinion;
    let p = s.rig.propose(
        &s.author,
        "Another kind",
        &[(&claim_path(&same.id), Some(&same.to_yaml()))],
    );
    let outcome = s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
    let files = handed(&s);
    let (_, text) = files.first().expect("the claim file");
    let Ok(File::Claim(written)) = read_file(&claim_path(&s.claim.id), text.as_bytes()) else {
        panic!("a claim");
    };
    assert_eq!(written.community_status.proposal, None);
}

// ------------------------------------------------------------------------------------ K26: the reconfirm step's proposal

#[test]
fn the_reconfirm_steps_proposal_is_judged_on_its_knowledge_files_as_a_submission_is_and_leaves_nothing_when_refused()
 {
    use crate::proposal::ProposalError;
    use inklkunn_store::ProposalStore;
    let s = scene("k26-judged");
    let edited = ONE.replace("Other.", "Other, more.");
    let page = [("pages/One.md", Some(edited.as_str()))];
    let p = s.rig.propose(&s.author, "A page edit", &page);
    let change = match s
        .rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1)
    {
        Ok(Accepted::Signed(signed)) => signed.acceptance,
        other => panic!("{other:?}"),
    };
    let service = &s.rig.proposals;
    let proposals = || {
        service
            .list(&s.author, &s.rig.wiki, &Default::default())
            .expect("a list")
            .len()
    };
    let notes = || {
        service
            .reconfirmations_of(&s.author, &s.rig.wiki, &change)
            .expect("the notes")
            .len()
    };
    let claim = contested(&s);
    let path = claim_path(&s.claim.id);
    let Principal::Account(person) = &s.author else {
        panic!("a person");
    };
    let author_id = person.id().clone();
    let (before, opened) = (
        proposals(),
        s.rig.store.open_count(&s.rig.wiki.id, &author_id).unwrap(),
    );

    // A status beside a page edit is not a curation proposal: refused in the page's words, as a submission is.
    let mixed = edit(&[(path.as_str(), Some(claim.as_str())), page[0]]);
    let refused = service.create_reconfirm(&s.author, &s.rig.wiki, None, mixed, &change);
    assert!(
        matches!(
            refused,
            Err(ProposalError::Invalid(Invalid::KnowledgeCurated))
        ),
        "{refused:?}"
    );
    assert_eq!(
        (
            proposals(),
            notes(),
            s.rig.store.open_count(&s.rig.wiki.id, &author_id).unwrap()
        ),
        (before, 0, opened),
        "a refusal leaves no proposal, no draft and no note"
    );

    // The same status alone is a curation proposal: made, already held for review.
    let alone = edit(&[(path.as_str(), Some(claim.as_str()))]);
    let made = service
        .create_reconfirm(&s.author, &s.rig.wiki, None, alone, &change)
        .expect("a curation proposal");
    assert_eq!(made.state, ProposalState::HeldForReview);
    assert_eq!((proposals(), notes()), (before + 1, 1));
}
