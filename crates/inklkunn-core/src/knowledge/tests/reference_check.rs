//! A check belongs to the reference it was made at (RFC 005 part 2 D-3, *correcting a source*, amended again; handoff 005
//! corrections, eighth round, K71): a source found elsewhere carries a review day or an availability only if the same
//! proposal states that check. The rule holds at acceptance, and the merge makes a reference changed on one side and a
//! check changed on the other an overlap, so a re-authored proposal asks a person instead of showing a check of one
//! place on another.

use super::check::{Mem, files_of, sound_claim};
use super::*;
use crate::acceptance::testing::Rig;
use crate::acceptance::{AcceptFailure, Accepted};
use crate::authorize::Principal;
use crate::knowledge::check::{Curated, Refusal, Touched, UNSTATED_CHECK, carried_by, validate};
use crate::knowledge::compose::{Curation, LinkEdit, SourceEdit, Work, curate};
use crate::merge::{MergeOutcome, Overlap, PageText, Place, merge};
use crate::proposal::Invalid;
use inklkunn_store::{ProposalState, Role};

const ONE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther.\n";

/// A source nobody checked, found at an address (A).
fn at_a() -> Source {
    let mut source = Source::new(source_id(1));
    source.identity.title = Some("A history".to_owned());
    source.reference = Some(Reference {
        form: ReferenceForm::Url,
        value: "https://example.org/a".to_owned(),
    });
    source
}

/// The claim that cites it, judged on the wording its page has.
fn claim() -> Claim {
    sound_claim(1)
}

fn doi_b(source: &Source) -> Work {
    Work {
        reference_form: ReferenceForm::Doi,
        reference: "10.1000/182".to_owned(),
        ..Work::of(source)
    }
}

/// What the form sends for `source` as it is, with the edits a test gives it.
fn edit(
    source: &Source,
    reviewed: &str,
    availability: Availability,
    work: Option<Work>,
) -> Curation {
    Curation {
        status: Standing::Unknown,
        links: vec![LinkEdit {
            source: source.id.clone(),
            relation: Link::Supports,
            bearing: Bearing::Direct,
        }],
        removed: Vec::new(),
        sources: vec![SourceEdit {
            source: source.id.clone(),
            reviewed: reviewed.to_owned(),
            availability,
            work,
        }],
        confirm: false,
    }
}

/// The text of the source file a curation of `source` writes (there is one).
fn curated(source: &Source, curation: &Curation) -> String {
    let files = curate(
        &claim(),
        std::slice::from_ref(source),
        Some(&super::check::said()),
        curation,
    )
    .expect("curated");
    let path = source_path(&source.id);
    files
        .into_iter()
        .find(|(p, _)| *p == path)
        .map(|(_, text)| text)
        .expect("the source file")
}

/// P1: a person checked address A on 2026-10-31, made against the base.
fn p1_checks_a(base: &Source) -> String {
    curated(
        base,
        &edit(base, "2026-10-31", Availability::Available, None),
    )
}

/// P2: the source is found at a DOI (B) now. It says nothing of a check: there was none to clear.
fn p2_moves_to_b(base: &Source) -> String {
    curated(
        base,
        &edit(base, "", Availability::Unknown, Some(doi_b(base))),
    )
}

fn side(source_text: &str) -> Vec<PageText> {
    vec![PageText::new(source_path(&source_id(1)), source_text)]
}

fn overlaps(outcome: MergeOutcome) -> Vec<Overlap> {
    match outcome {
        MergeOutcome::Overlaps { overlaps } => overlaps,
        other => panic!("expected an overlap, got {other:?}"),
    }
}

fn key(o: &Overlap) -> &str {
    match &o.place {
        Place::Knowledge { key } => key,
        other => panic!("not a knowledge place: {other:?}"),
    }
}

fn parsed(text: &str) -> Source {
    Source::parse(source_id(1), text).expect("a source file")
}

#[test]
fn a_reference_moved_on_one_side_and_a_check_made_on_the_other_is_an_overlap_in_either_order() {
    let base = at_a();
    let (base_text, p1, p2) = (base.to_yaml(), p1_checks_a(&base), p2_moves_to_b(&base));
    // The reviewer's first order: P2 was accepted (the tip is at B), then P1, made on the old base, is re-authored.
    let first = overlaps(merge(&side(&base_text), &side(&p2), &side(&p1)));
    // The second: P1 was accepted (the tip is checked at A), then P2 is re-authored.
    let second = overlaps(merge(&side(&base_text), &side(&p1), &side(&p2)));
    for found in [&first, &second] {
        assert_eq!(found.len(), 1);
        let one = found.first().expect("one overlap");
        assert_eq!(key(one), "reference");
        // Both sides are shown whole: where it is found and what was checked there.
        let (theirs, ours) = (one.theirs.as_deref(), one.ours.as_deref());
        assert!(
            [theirs, ours]
                .iter()
                .any(|t| t.is_some_and(|t| t.contains("2026-10-31") && t.contains("example.org/a")))
        );
        assert!(
            [theirs, ours]
                .iter()
                .any(|t| t.is_some_and(|t| t.contains("10.1000/182")))
        );
    }
}

#[test]
fn a_move_that_states_its_own_check_is_taken_with_what_the_other_side_changed_elsewhere() {
    let base = at_a();
    let base_text = base.to_yaml();
    // P2 states the check of B itself.
    let stated = curated(
        &base,
        &edit(
            &base,
            "2026-11-10",
            Availability::Available,
            Some(doi_b(&base)),
        ),
    );
    // The tip only retitled the source.
    let mut retitled = base.clone();
    retitled.identity.title = Some("A history, revised".to_owned());
    let tip = retitled.to_yaml();
    for (theirs, ours) in [(&tip, &stated), (&stated, &tip)] {
        let MergeOutcome::Clean { pages } = merge(&side(&base_text), &side(theirs), &side(ours))
        else {
            panic!("clean");
        };
        let merged = parsed(&pages.first().expect("one").text);
        assert_eq!(
            merged.reference,
            Some(Reference {
                form: ReferenceForm::Doi,
                value: "10.1000/182".to_owned()
            })
        );
        assert_eq!(
            merged.last_reviewed.as_ref().map(|r| r.date.as_str()),
            Some("2026-11-10")
        );
        assert_eq!(merged.identity.title.as_deref(), Some("A history, revised"));
    }
}

#[test]
fn a_check_and_a_respelling_of_the_reference_are_taken_together() {
    // White space alone is not another place (K72): the check made on the other side stays.
    let mut base = at_a();
    base.reference = Some(Reference {
        form: ReferenceForm::Citation,
        value: "Smith 1962, p. 4".to_owned(),
    });
    let mut spaced = base.clone();
    spaced.reference = Some(Reference {
        form: ReferenceForm::Citation,
        value: "Smith  1962, p. 4".to_owned(),
    });
    let mut checked = base.clone();
    checked.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: None,
    });
    checked.availability = Availability::Available;
    for (theirs, ours) in [(&spaced, &checked), (&checked, &spaced)] {
        let MergeOutcome::Clean { pages } = merge(
            &side(&base.to_yaml()),
            &side(&theirs.to_yaml()),
            &side(&ours.to_yaml()),
        ) else {
            panic!("clean");
        };
        let merged = parsed(&pages.first().expect("one").text);
        assert_eq!(merged.reference, spaced.reference);
        assert_eq!(merged.availability, Availability::Available);
        assert!(merged.last_reviewed.is_some());
    }
}

#[test]
fn a_check_changed_on_both_sides_without_a_move_is_still_judged_by_its_own_keys() {
    // Nothing about the reference: the keys merge as they did (a day on one side, an answer on the other).
    let base = at_a();
    let mut day = base.clone();
    day.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: None,
    });
    let mut answer = base.clone();
    answer.availability = Availability::Unavailable;
    let MergeOutcome::Clean { pages } = merge(
        &side(&base.to_yaml()),
        &side(&day.to_yaml()),
        &side(&answer.to_yaml()),
    ) else {
        panic!("clean");
    };
    let merged = parsed(&pages.first().expect("one").text);
    assert_eq!(merged.availability, Availability::Unavailable);
    assert!(merged.last_reviewed.is_some());
}

#[test]
fn only_a_check_the_proposal_states_goes_with_a_changed_reference() {
    let day = |d: &str, proposal: Option<&str>| {
        Some(Reviewed {
            date: d.to_owned(),
            proposal: proposal.map(str::to_owned),
        })
    };
    let tip = |reviewed, availability| {
        let mut s = at_a();
        s.last_reviewed = reviewed;
        s.availability = availability;
        Curated::of(&File::Source(Box::new(s)))
    };
    let after = |reviewed, availability, moved: bool| {
        let mut s = at_a();
        if moved {
            s.reference = Some(Reference {
                form: ReferenceForm::Doi,
                value: "10.1000/182".to_owned(),
            });
        }
        s.last_reviewed = reviewed;
        s.availability = availability;
        Curated::of(&File::Source(Box::new(s)))
    };
    let checked = tip(day("2026-10-31", Some("p0")), Availability::Available);
    // The day kept, or the answer kept, on another reference: carried.
    assert!(
        after(day("2026-10-31", Some("p0")), Availability::Available, true)
            .carries_unstated_check(&checked)
    );
    assert!(
        after(day("2026-10-31", Some("p0")), Availability::Unknown, true)
            .carries_unstated_check(&checked)
    );
    assert!(after(None, Availability::Available, true).carries_unstated_check(&checked));
    // Nothing kept: not carried. A day that is gone is a check that is not carried.
    assert!(!after(None, Availability::Unknown, true).carries_unstated_check(&checked));
    // A check the proposal states: another day (even an earlier one is a statement; whether it is a good one is for the
    // day's own rule), the same day with the proposal not yet named, another answer.
    assert!(
        !after(day("2026-11-10", None), Availability::Available, true)
            .carries_unstated_check(&checked)
    );
    assert!(
        !after(day("2026-10-31", None), Availability::Available, true)
            .carries_unstated_check(&checked)
    );
    assert!(!after(None, Availability::Unavailable, true).carries_unstated_check(&checked));
    // The same reference keeps its check, whatever else changes.
    assert!(
        !after(
            day("2026-10-31", Some("p0")),
            Availability::Available,
            false
        )
        .carries_unstated_check(&checked)
    );
    // A source that was not checked and is found elsewhere carries nothing.
    let unchecked = tip(None, Availability::Unknown);
    assert!(!after(None, Availability::Unknown, true).carries_unstated_check(&unchecked));
    // A claim carries nothing of the kind.
    let claim = Curated::of(&File::Claim(Box::new(claim())));
    assert!(!claim.carries_unstated_check(&claim));
}

fn refusals_of(base: &Source, now: &Source) -> Vec<Refusal> {
    let path = source_path(&now.id);
    let files = files_of(&[(path.clone(), now.to_yaml())]);
    let touched = Touched {
        written: [path.clone()].into(),
        curated_before: [(path, Curated::of(&File::Source(Box::new(base.clone()))))].into(),
        curation: true,
        ..Touched::default()
    };
    validate(&files, &touched, &Mem(Vec::new()), &Mem(Vec::new()))
}

#[test]
fn the_validator_refuses_a_source_found_elsewhere_that_carries_the_old_check_by_field() {
    let mut tip = at_a();
    tip.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    tip.availability = Availability::Available;
    let mut carried = tip.clone();
    carried.reference = doi_reference();
    let got = refusals_of(&tip, &carried);
    assert_eq!(got.len(), 1, "{got:?}");
    let one = got.first().expect("one");
    assert_eq!((one.rule, one.field), (Rule::Status, "last_reviewed"));
    assert_eq!(one.sentence(), UNSTATED_CHECK);
    assert!(one.curation_form_can_make(), "a form states a check");
    // Stated: another day.
    let mut stated = carried.clone();
    stated.last_reviewed = Some(Reviewed {
        date: "2026-11-10".to_owned(),
        proposal: None,
    });
    assert!(refusals_of(&tip, &stated).is_empty());
    // Not carried: the check is gone.
    let mut cleared = carried.clone();
    cleared.last_reviewed = None;
    cleared.availability = Availability::Unknown;
    assert!(refusals_of(&tip, &cleared).is_empty());
    // Outside a curation proposal the source's reference is curation's anyway: one refusal, not two.
    let path = source_path(&tip.id);
    let files = files_of(&[(path.clone(), carried.to_yaml())]);
    let touched = Touched {
        written: [path.clone()].into(),
        curated_before: [(path, Curated::of(&File::Source(Box::new(tip.clone()))))].into(),
        ..Touched::default()
    };
    let got = validate(&files, &touched, &Mem(Vec::new()), &Mem(Vec::new()));
    assert_eq!(
        got.iter().map(|r| (r.field, r.carries)).collect::<Vec<_>>(),
        [("reference", false)]
    );
}

fn doi_reference() -> Option<Reference> {
    Some(Reference {
        form: ReferenceForm::Doi,
        value: "10.1000/182".to_owned(),
    })
}

#[test]
fn the_tree_free_rule_is_the_validators() {
    let mut tip = at_a();
    tip.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    let mut carried = tip.clone();
    carried.reference = doi_reference();
    let path = source_path(&tip.id);
    let was = tip.to_yaml();
    let before = |p: &str| (p == path).then(|| was.clone());
    let got = carried_by(&[(path.clone(), Some(carried.to_yaml()))], &before);
    assert_eq!(got.len(), 1);
    assert_eq!(got.first().map(|r| r.field), Some("last_reviewed"));
    // A removal, a source the base lacks and a file that does not read are not this rule's.
    assert!(carried_by(&[(path.clone(), None)], &before).is_empty());
    assert!(carried_by(&[(path.clone(), Some(carried.to_yaml()))], &|_| None).is_empty());
    assert!(
        carried_by(
            &[(path.clone(), Some("not: [a source".to_owned()))],
            &before
        )
        .is_empty()
    );
}

// ------------------------------------------------------------------------------------ in the acceptance path

struct Scene {
    rig: Rig,
    maintainer: Principal,
    author: Principal,
}

/// A wiki with a claim, the page it is said in and its source (at the base: `source`).
fn scene(tag: &str, source: &Source) -> Scene {
    let rig = Rig::new(tag);
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Admin));
    rig.fake.set("pages/One.md", ONE);
    rig.fake.set(&claim_path(&claim().id), &claim().to_yaml());
    rig.fake.set(&source_path(&source.id), &source.to_yaml());
    Scene {
        rig,
        maintainer,
        author,
    }
}

fn accept(s: &Scene, p: &inklkunn_store::ProposalRecord) -> Result<Accepted, AcceptFailure> {
    s.rig
        .acceptor()
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1)
}

#[test]
fn the_reviewers_two_orders_stop_the_acceptance_with_both_texts_and_never_write_a_check_on_the_new_reference()
 {
    let base = at_a();
    let (p1, p2) = (p1_checks_a(&base), p2_moves_to_b(&base));
    let path = source_path(&base.id);
    for (tag, tip, proposed) in [
        // P2 is accepted first; P1, made on the old base, comes to a tip that is at B.
        ("k71-order-one", &p2, &p1),
        // P1 is accepted first; P2 comes to a tip that is checked at A.
        ("k71-order-two", &p1, &p2),
    ] {
        let s = scene(tag, &base);
        let p = s.rig.propose(
            &s.author,
            "A change of a source",
            &[(&path, Some(proposed))],
        );
        s.rig.fake.set(&path, tip);
        let failure = accept(&s, &p).expect_err("stopped");
        assert!(
            matches!(failure, AcceptFailure::Overlap { overlaps: 1 }),
            "{tag}: {failure:?}"
        );
        assert_eq!(s.rig.double.calls().len(), 0, "the signer was not asked");
        assert_eq!(s.rig.state(&p.id), ProposalState::NeedsRevision);
        let recorded =
            inklkunn_store::ActStore::overlaps_of(&*s.rig.store, &p.id).expect("records");
        let texts = &recorded.first().expect("one record").texts;
        let t = texts.first().expect("one text");
        assert_eq!(t.page, path);
        assert_eq!(t.place, "knowledge reference");
    }
}

#[test]
fn a_source_moved_with_the_old_check_left_on_it_is_refused_before_the_signer_is_asked() {
    let mut tip = at_a();
    tip.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    tip.availability = Availability::Available;
    let s = scene("k71-carried", &tip);
    let path = source_path(&tip.id);
    let mut carried = tip.clone();
    carried.reference = doi_reference();
    // Made by hand (a script, a client of the generic write API): the form never writes it.
    let p = s.rig.propose_unjudged(
        &s.author,
        "A change of a source",
        &[(&path, Some(&carried.to_yaml()))],
    );
    let Err(AcceptFailure::KnowledgeInvalid(refusals)) = accept(&s, &p) else {
        panic!("expected the knowledge refusal");
    };
    assert!(
        refusals
            .iter()
            .any(|r| r.carries && r.field == "last_reviewed" && r.rule == Rule::Status),
        "{refusals:?}"
    );
    assert_eq!(s.rig.double.calls().len(), 0, "the signer was not asked");
    assert_eq!(s.rig.state(&p.id), ProposalState::HeldForReview);

    // The same proposal with the check of the new reference stated (another day) is accepted.
    let mut stated = carried.clone();
    stated.last_reviewed = Some(Reviewed {
        date: "2026-11-10".to_owned(),
        proposal: None,
    });
    let p = s.rig.propose_unjudged(
        &s.author,
        "A change of a source, checked",
        &[(&path, Some(&stated.to_yaml()))],
    );
    let outcome = accept(&s, &p);
    assert!(matches!(outcome, Ok(Accepted::Signed(_))), "{outcome:?}");
}

#[test]
fn the_same_rule_refuses_it_when_it_is_sent_and_at_every_revision() {
    let mut tip = at_a();
    tip.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    tip.availability = Availability::Available;
    let s = scene("k71-sent", &tip);
    let path = source_path(&tip.id);
    let mut carried = tip.clone();
    carried.reference = doi_reference();
    let given = |text: &str| crate::proposal::ProposalEdit {
        summary: "A change of a source".to_owned(),
        files: vec![inklkunn_store::FileChange {
            path: path.clone(),
            text: Some(text.to_owned()),
        }],
        ..crate::proposal::ProposalEdit::default()
    };
    let service = &s.rig.proposals;
    let as_invalid = |r: Result<_, crate::proposal::ProposalError>| match r {
        Ok(_) => Ok(()),
        Err(crate::proposal::ProposalError::Invalid(why)) => Err(why),
        Err(other) => panic!("not the refusal of the knowledge files: {other:?}"),
    };
    // A draft is saved whatever it holds; sending it is refused.
    let draft = service
        .create(&s.author, &s.rig.wiki, None, given(&carried.to_yaml()))
        .expect("a draft");
    assert_eq!(
        as_invalid(service.submit(&s.author, &s.rig.wiki, &draft.id)),
        Err(Invalid::KnowledgeCheckUnstated)
    );
    // A revision of a held proposal too.
    let held = s.rig.propose(
        &s.author,
        "A page edit",
        &[("pages/One.md", Some(&format!("{ONE}\nMore.\n")))],
    );
    assert_eq!(
        as_invalid(service.revise(&s.author, &s.rig.wiki, &held.id, given(&carried.to_yaml()))),
        Err(Invalid::KnowledgeCheckUnstated)
    );
    // The check stated: sent.
    let mut stated = carried.clone();
    stated.last_reviewed = Some(Reviewed {
        date: "2026-11-10".to_owned(),
        proposal: None,
    });
    let draft = service
        .create(&s.author, &s.rig.wiki, None, given(&stated.to_yaml()))
        .expect("a draft");
    assert_eq!(
        as_invalid(service.submit(&s.author, &s.rig.wiki, &draft.id)),
        Ok(())
    );
}
