//! The knowledge files in the merge (RFC 005 part 2 D-4; handoff K-T3): key by key, lists by the identity of their
//! entries, a value changed on both sides an overlap with both texts, nothing lost.

use super::*;
use crate::acceptance::testing::{HOME, Rig};
use crate::acceptance::{AcceptFailure, Accepted};
use crate::merge::{MergeOutcome, Overlap, PageText, Place, merge};
use inklkunn_store::Role;

fn base_claim() -> Claim {
    let mut c = Claim::new(claim_id(1));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![
        Expression::new("p1", "b1").expect("e"),
        Expression::new("p1", "b2").expect("e"),
    ];
    c.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(9),
    }];
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, None, "w1"),
        link(2, Link::Challenges, Bearing::Partial, Some("n"), "w2"),
    ];
    c.normalize();
    c
}

fn s(c: &Claim, n: usize) -> &SourceLink {
    c.sources.get(n).expect("a source link")
}

fn sm(c: &mut Claim, n: usize) -> &mut SourceLink {
    c.sources.get_mut(n).expect("a source link")
}

fn link(n: u8, relation: Link, bearing: Bearing, note: Option<&str>, words: &str) -> SourceLink {
    SourceLink {
        source: source_id(n),
        relation,
        bearing,
        note: note.map(str::to_owned),
        judged_on: digest_of(words),
    }
}

fn file(c: &Claim) -> Vec<PageText> {
    vec![PageText::new(claim_path(&c.id), c.to_yaml())]
}

fn run(base: &Claim, theirs: &Claim, ours: &Claim) -> MergeOutcome {
    merge(&file(base), &file(theirs), &file(ours))
}

fn clean_claim(outcome: MergeOutcome) -> Claim {
    let MergeOutcome::Clean { pages } = outcome else {
        panic!("expected a clean merge, got {outcome:?}");
    };
    assert_eq!(pages.len(), 1);
    let page = pages.first().expect("one file");
    Claim::parse(claim_id(1), &page.text).expect("the merged file is readable")
}

fn overlaps(outcome: MergeOutcome) -> Vec<Overlap> {
    match outcome {
        MergeOutcome::Overlaps { overlaps } => overlaps,
        other => panic!("expected overlaps, got {other:?}"),
    }
}

fn key(o: &Overlap) -> &str {
    match &o.place {
        Place::Knowledge { key } => key,
        other => panic!("not a knowledge place: {other:?}"),
    }
}

#[test]
fn expressions_are_matched_by_anchor_and_both_sides_additions_are_kept() {
    let base = base_claim();
    let mut theirs = base.clone();
    theirs
        .expressions
        .push(Expression::new("p2", "x1").expect("e"));
    let mut ours = base.clone();
    ours.expressions
        .push(Expression::new("p3", "y1").expect("e"));
    let merged = clean_claim(run(&base, &theirs, &ours));
    assert_eq!(merged.expressions.len(), 4);
    // The same anchor added on both sides is one expression.
    let mut same = base.clone();
    same.expressions
        .push(Expression::new("p2", "x1").expect("e"));
    assert_eq!(clean_claim(run(&base, &same, &same)).expressions.len(), 3);
    // A removal by one side is taken while the other adds: nothing else is lost.
    let mut removes = base.clone();
    removes.expressions.remove(0);
    let merged = clean_claim(run(&base, &removes, &ours));
    assert!(merged.expressions.iter().all(|e| e.block != "b1"));
    assert!(merged.expressions.iter().any(|e| e.block == "y1"));
    assert!(merged.expressions.iter().any(|e| e.block == "b2"));
}

#[test]
fn relations_are_matched_by_type_and_claim() {
    let base = base_claim();
    let mut theirs = base.clone();
    theirs.relations.push(Relation {
        kind: RelationType::Contradicts,
        claim: claim_id(9),
    });
    let mut ours = base.clone();
    ours.relations.push(Relation {
        kind: RelationType::Supersedes,
        claim: claim_id(8),
    });
    let merged = clean_claim(run(&base, &theirs, &ours));
    // The same claim under another type is another relation: three, none lost.
    assert_eq!(merged.relations.len(), 3);
    // One side drops the base's relation while the other adds one: dropped and added.
    let mut drops = base.clone();
    drops.relations.clear();
    let merged = clean_claim(run(&base, &drops, &ours));
    assert_eq!(merged.relations.len(), 1);
    assert_eq!(
        merged.relations.first().map(|r| r.kind),
        Some(RelationType::Supersedes)
    );
}

#[test]
fn a_source_link_changed_on_both_sides_differently_is_an_overlap_with_both_texts() {
    let base = base_claim();
    let mut theirs = base.clone();
    sm(&mut theirs, 0).bearing = Bearing::Indirect;
    let mut ours = base.clone();
    sm(&mut ours, 0).bearing = Bearing::Partial;
    let found = overlaps(run(&base, &theirs, &ours));
    assert_eq!(found.len(), 1);
    let o = found.first().expect("one");
    assert_eq!(key(o), format!("sources/{}/bearing", source_id(1)));
    assert_eq!(o.base.as_deref(), Some("direct"));
    assert_eq!(o.theirs.as_deref(), Some("indirect"));
    assert_eq!(o.ours.as_deref(), Some("partial"));
    assert_eq!(o.page.path, claim_path(&claim_id(1)));
    // Changed alike: once.
    let mut alike = base.clone();
    sm(&mut alike, 0).bearing = Bearing::Indirect;
    assert_eq!(
        s(&clean_claim(run(&base, &theirs, &alike)), 0).bearing,
        Bearing::Indirect
    );
}

#[test]
fn two_sides_changing_different_keys_of_one_link_are_both_taken() {
    let base = base_claim();
    let mut theirs = base.clone();
    sm(&mut theirs, 0).bearing = Bearing::Indirect;
    let mut ours = base.clone();
    sm(&mut ours, 0).note = Some("a note from the proposal".to_owned());
    let merged = clean_claim(run(&base, &theirs, &ours));
    assert_eq!(s(&merged, 0).bearing, Bearing::Indirect);
    assert_eq!(
        s(&merged, 0).note.as_deref(),
        Some("a note from the proposal")
    );
}

#[test]
fn each_key_of_a_link_can_clash_and_names_itself() {
    let base = base_claim();
    type Apply = fn(&mut SourceLink, u8);
    let cases: [(&str, Apply); 2] = [
        ("note", |l, v| l.note = Some(format!("note {v}"))),
        ("judged_on", |l, v| {
            l.judged_on = digest_of(&format!("words {v}"))
        }),
    ];
    for (field, apply) in cases {
        let (mut theirs, mut ours) = (base.clone(), base.clone());
        apply(sm(&mut theirs, 0), 1);
        apply(sm(&mut ours, 0), 2);
        let found = overlaps(run(&base, &theirs, &ours));
        assert_eq!(found.len(), 1, "{field}");
        assert_eq!(
            key(found.first().expect("one")),
            format!("sources/{}/{field}", source_id(1))
        );
    }
    // `relation` has two words: both sides moving to the other one is the same change, once.
    let (mut theirs, mut ours) = (base.clone(), base.clone());
    sm(&mut theirs, 0).relation = Link::Challenges;
    sm(&mut ours, 0).relation = Link::Challenges;
    assert!(run(&base, &theirs, &ours).is_clean());
}

#[test]
fn a_link_one_side_removed_and_the_other_changed_is_an_overlap_and_an_untouched_one_is_removed() {
    let base = base_claim();
    let mut removes = base.clone();
    removes.sources.remove(0);
    let mut changes = base.clone();
    sm(&mut changes, 0).note = Some("kept alive".to_owned());
    let found = overlaps(run(&base, &removes, &changes));
    assert_eq!(found.len(), 1);
    let o = found.first().expect("one");
    assert_eq!(key(o), format!("sources/{}", source_id(1)));
    assert!(o.base.is_some() && o.theirs.is_none() && o.ours.is_some());
    // The other way round, the texts swap sides.
    let found = overlaps(run(&base, &changes, &removes));
    let o = found.first().expect("one");
    assert!(o.base.is_some() && o.theirs.is_some() && o.ours.is_none());
    // The other side did not touch it: removed.
    let merged = clean_claim(run(&base, &removes, &base));
    assert_eq!(merged.sources.len(), 1);
    assert_eq!(s(&merged, 0).source, source_id(2));
}

#[test]
fn a_link_both_sides_added_is_one_if_alike_and_an_overlap_if_not() {
    let base = base_claim();
    let mut a = base.clone();
    a.sources
        .push(link(3, Link::Supports, Bearing::Direct, None, "w3"));
    let mut b = base.clone();
    b.sources
        .push(link(3, Link::Challenges, Bearing::Direct, None, "w3"));
    a.normalize();
    b.normalize();
    assert_eq!(clean_claim(run(&base, &a, &a)).sources.len(), 3);
    let found = overlaps(run(&base, &a, &b));
    assert_eq!(found.len(), 1);
    let o = found.first().expect("one");
    assert_eq!(key(o), format!("sources/{}", source_id(3)));
    assert!(o.base.is_none() && o.theirs.is_some() && o.ours.is_some());
    // Different links added: both.
    let mut c = base.clone();
    c.sources
        .push(link(4, Link::Supports, Bearing::Indirect, None, "w4"));
    c.normalize();
    assert_eq!(clean_claim(run(&base, &a, &c)).sources.len(), 4);
}

#[test]
fn the_standing_is_one_value() {
    let base = base_claim();
    let decide = |status: Standing, proposal: &str| CommunityStatus {
        status,
        proposal: Some(proposal.to_owned()),
        judged_on: Some(digest_of("s")),
    };
    let mut theirs = base.clone();
    theirs.community_status = decide(Standing::Contested, "01930000-aaaa-7000-8000-000000000001");
    let mut ours = base.clone();
    ours.community_status = decide(Standing::Accepted, "01930000-aaaa-7000-8000-000000000002");
    let found = overlaps(run(&base, &theirs, &ours));
    assert_eq!(key(found.first().expect("one")), "community_status");
    let theirs_text = found.first().and_then(|o| o.theirs.clone()).expect("text");
    assert!(theirs_text.contains("status: contested") && theirs_text.contains("8000-000000000001"));
    // One side decided: taken whole, with its proposal and its digest.
    let merged = clean_claim(run(&base, &theirs, &base));
    assert_eq!(merged.community_status, theirs.community_status);
}

#[test]
fn kind_and_lifecycle_are_keys_of_their_own() {
    let base = base_claim();
    let (mut theirs, mut ours) = (base.clone(), base.clone());
    theirs.kind = StatementKind::Hypothesis;
    ours.lifecycle = Lifecycle::Adopted;
    let merged = clean_claim(run(&base, &theirs, &ours));
    assert_eq!(merged.kind, StatementKind::Hypothesis);
    assert_eq!(merged.lifecycle, Lifecycle::Adopted);
    ours.kind = StatementKind::Opinion;
    let found = overlaps(run(&base, &theirs, &ours));
    assert_eq!(key(found.first().expect("one")), "kind");
    assert_eq!(
        found.first().and_then(|o| o.ours.clone()).as_deref(),
        Some("opinion")
    );
}

#[test]
fn several_clashes_in_one_file_are_all_reported() {
    let base = base_claim();
    let (mut theirs, mut ours) = (base.clone(), base.clone());
    theirs.kind = StatementKind::Hypothesis;
    ours.kind = StatementKind::Opinion;
    sm(&mut theirs, 1).bearing = Bearing::Direct;
    sm(&mut ours, 1).bearing = Bearing::Indirect;
    // The first clash stops the file (a file is merged as a whole or not at all); it is named.
    let found = overlaps(run(&base, &theirs, &ours));
    assert_eq!(found.len(), 1);
    assert_eq!(key(found.first().expect("one")), "kind");
}

#[test]
fn a_file_one_side_left_alone_is_taken_as_written() {
    let base = base_claim();
    let mut theirs = base.clone();
    theirs.kind = StatementKind::Policy;
    // Hand-written formatting on the proposal's side survives when the tip changed nothing in the file.
    let mut ours_text = base.to_yaml();
    ours_text = ours_text.replace("kind: observation", "# hand-written\nkind:   decision");
    let outcome = merge(
        &file(&base),
        &file(&base),
        &[PageText::new(claim_path(&base.id), ours_text.clone())],
    );
    let MergeOutcome::Clean { pages } = outcome else {
        panic!("clean");
    };
    assert_eq!(pages.first().map(|p| p.text.clone()), Some(ours_text));
    // And a file both changed is written in the canonical form.
    let mut ours = base.clone();
    ours.lifecycle = Lifecycle::Retired;
    let merged = merge(&file(&base), &file(&theirs), &file(&ours));
    let MergeOutcome::Clean { pages } = merged else {
        panic!("clean");
    };
    let mut want = base.clone();
    want.kind = StatementKind::Policy;
    want.lifecycle = Lifecycle::Retired;
    assert_eq!(pages.first().map(|p| p.text.clone()), Some(want.to_yaml()));
}

#[test]
fn a_file_that_cannot_be_read_on_a_side_is_not_merged_key_by_key() {
    let base = base_claim();
    let mut theirs = base.clone();
    theirs.kind = StatementKind::Policy;
    let broken = "kind: observation\nscore: 3\n".to_owned();
    let found = overlaps(merge(
        &file(&base),
        &file(&theirs),
        &[PageText::new(claim_path(&base.id), broken.clone())],
    ));
    assert_eq!(found.len(), 1);
    let o = found.first().expect("one");
    assert_eq!(key(o), "");
    assert_eq!(o.ours.as_deref(), Some(broken.as_str()));
    assert_eq!(o.theirs.as_deref(), Some(theirs.to_yaml().as_str()));
}

#[test]
fn whole_files_added_removed_and_changed_on_the_sides() {
    let c = base_claim();
    let path = claim_path(&c.id);
    let none: Vec<PageText> = Vec::new();
    // Added on one side: taken. On both, alike: once; differently: an overlap with both texts.
    assert_eq!(clean_claim(merge(&none, &file(&c), &none)).id, c.id);
    assert_eq!(clean_claim(merge(&none, &file(&c), &file(&c))).id, c.id);
    let mut other = c.clone();
    other.kind = StatementKind::Policy;
    let found = overlaps(merge(&none, &file(&c), &file(&other)));
    assert_eq!(key(found.first().expect("one")), "");
    assert!(found.first().is_some_and(|o| o.base.is_none()));
    // Removed on one side: taken if the other left it, an overlap if the other changed it.
    let merged = merge(&file(&c), &none, &file(&c));
    assert!(matches!(merged, MergeOutcome::Clean { ref pages } if pages.is_empty()));
    let found = overlaps(merge(&file(&c), &none, &file(&other)));
    assert_eq!(found.first().map(|o| o.page.path.clone()), Some(path));
}

#[test]
fn a_source_file_is_merged_by_its_keys_with_identity_apart() {
    let mut base = Source::new(source_id(1));
    base.identity.title = Some("A survey".to_owned());
    base.reference = Some(Reference {
        form: ReferenceForm::Url,
        value: "https://example.org/a".to_owned(),
    });
    let (mut theirs, mut ours) = (base.clone(), base.clone());
    theirs.identity.author = Some("The office".to_owned());
    theirs.availability = Availability::Unavailable;
    ours.identity.title = Some("A survey, revised".to_owned());
    ours.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: None,
    });
    let side = |s: &Source| vec![PageText::new(source_path(&s.id), s.to_yaml())];
    let MergeOutcome::Clean { pages } = merge(&side(&base), &side(&theirs), &side(&ours)) else {
        panic!("clean");
    };
    let merged = Source::parse(source_id(1), &pages.first().expect("one").text).expect("readable");
    assert_eq!(merged.identity.title.as_deref(), Some("A survey, revised"));
    assert_eq!(merged.identity.author.as_deref(), Some("The office"));
    assert_eq!(merged.availability, Availability::Unavailable);
    assert!(merged.last_reviewed.is_some());
    // The reference is one value: changed on both sides, an overlap.
    theirs.reference = Some(Reference {
        form: ReferenceForm::Doi,
        value: "10.1000/x".to_owned(),
    });
    ours.reference = Some(Reference {
        form: ReferenceForm::Url,
        value: "https://example.org/b".to_owned(),
    });
    let found = overlaps(merge(&side(&base), &side(&theirs), &side(&ours)));
    assert_eq!(key(found.first().expect("one")), "reference");
    assert!(
        found
            .first()
            .and_then(|o| o.theirs.clone())
            .is_some_and(|t| t.contains("doi"))
    );
    // The same title on both sides is one value.
    let (mut a, mut b) = (base.clone(), base.clone());
    a.identity.title = Some("Same".to_owned());
    b.identity.title = Some("Same".to_owned());
    assert!(merge(&side(&base), &side(&a), &side(&b)).is_clean());
}

#[test]
fn pages_and_knowledge_files_are_merged_together_and_an_overlap_of_either_stops_it() {
    let c = base_claim();
    let page = |text: &str| PageText::new("pages/Home.md", text);
    let (b, t, o) = (
        "---\nid: p1\n---\n\nOne. ^b1\n\nTwo.\n",
        "---\nid: p1\n---\n\nOne, said again. ^b1\n\nTwo.\n",
        "---\nid: p1\n---\n\nOne. ^b1\n\nTwo, said again.\n",
    );
    let mut theirs = c.clone();
    theirs.kind = StatementKind::Policy;
    let with = |p: &str, k: &Claim| {
        let mut v = file(k);
        v.push(page(p));
        v
    };
    // Both clean: both files come out, in path order.
    let MergeOutcome::Clean { pages } = merge(&with(b, &c), &with(t, &theirs), &with(o, &c)) else {
        panic!("clean");
    };
    assert_eq!(pages.len(), 2);
    assert!(pages.iter().any(
        |p| p.path == "pages/Home.md" && p.text.contains("said again. ^b1")
            || p.path != "pages/Home.md"
    ));
    assert!(pages.is_sorted_by_key(|p| p.path.clone()));
    // A knowledge overlap with clean pages: nothing is merged.
    let mut ours = c.clone();
    ours.kind = StatementKind::Opinion;
    let found = overlaps(merge(&with(b, &c), &with(t, &theirs), &with(o, &ours)));
    assert_eq!(found.len(), 1);
    // A page overlap and a knowledge overlap: both are reported.
    let both = overlaps(merge(
        &with(b, &c),
        &with(t, &theirs),
        &with(&t.replace("said again", "said twice"), &ours),
    ));
    assert_eq!(both.len(), 2);
    assert!(
        both.iter()
            .any(|o| matches!(o.place, Place::Knowledge { .. }))
    );
    assert!(both.iter().any(|o| matches!(o.place, Place::Block { .. })));
}

#[test]
fn the_order_of_the_sides_never_changes_the_verdict_nor_the_merged_text() {
    let base = base_claim();
    let mut theirs = base.clone();
    sm(&mut theirs, 0).bearing = Bearing::Indirect;
    theirs
        .expressions
        .push(Expression::new("p9", "z").expect("e"));
    let mut ours = base.clone();
    sm(&mut ours, 0).note = Some("x".to_owned());
    ours.relations.clear();
    let (ab, ba) = (run(&base, &theirs, &ours), run(&base, &ours, &theirs));
    assert_eq!(ab, ba);
    sm(&mut ours, 0).bearing = Bearing::Partial;
    let (ab, ba) = (
        overlaps(run(&base, &theirs, &ours)),
        overlaps(run(&base, &ours, &theirs)),
    );
    assert_eq!(ab.len(), ba.len());
    for (x, y) in ab.iter().zip(&ba) {
        assert_eq!((&x.theirs, &x.ours), (&y.ours, &y.theirs));
        assert_eq!(x.base, y.base);
    }
}

#[test]
fn nothing_numeric_and_no_ranking_is_made_by_a_merge() {
    // A merged file is read by the same reader, so a merge can only produce what the format allows.
    let base = base_claim();
    let mut theirs = base.clone();
    sm(&mut theirs, 0).bearing = Bearing::Indirect;
    let text = clean_claim(run(&base, &theirs, &base)).to_yaml();
    assert!(Claim::parse(claim_id(1), &text).is_ok());
}

// ------------------------------------------------------------------------------------ in the acceptance path

#[test]
fn a_proposal_whose_base_moved_has_its_knowledge_file_merged_with_the_tips() {
    let rig = Rig::new("k3-accept");
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let page = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther. ^b2\n";
    rig.fake.set("pages/One.md", page);
    let mut claim = base_claim();
    claim.expressions = vec![Expression::new("p1", "b1").expect("e")];
    claim.relations.clear();
    claim.sources = vec![
        link(1, Link::Supports, Bearing::Direct, None, "w1"),
        link(2, Link::Supports, Bearing::Direct, None, "w2"),
    ];
    claim.normalize();
    let path = claim_path(&claim.id);
    rig.fake.set(&path, &claim.to_yaml());
    rig.fake.set(
        &source_path(&source_id(1)),
        &Source::new(source_id(1)).to_yaml(),
    );
    rig.fake.set(
        &source_path(&source_id(2)),
        &Source::new(source_id(2)).to_yaml(),
    );

    // The proposal changes the note of the first link, against this tip.
    let mut ours = claim.clone();
    sm(&mut ours, 0).note = Some("from the proposal".to_owned());
    let p = rig.propose(&author, "A note", &[(&path, Some(&ours.to_yaml()))]);
    // The tip moves: another change set the bearing of the same link, and the second link's note.
    let mut tip = claim.clone();
    sm(&mut tip, 0).bearing = Bearing::Indirect;
    sm(&mut tip, 1).note = Some("from the tip".to_owned());
    rig.fake.set(&path, &tip.to_yaml());
    let acceptor = rig.acceptor();
    let signed = match acceptor
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .expect("accepted")
    {
        Accepted::Signed(s) => s,
        other => panic!("{other:?}"),
    };
    assert!(signed.reauthored, "the merge made the change");
    let call = rig
        .double
        .calls()
        .into_iter()
        .next()
        .expect("the signer was asked");
    let written = call
        .change
        .files
        .iter()
        .find(|f| f.path.to_string_lossy() == path)
        .and_then(|f| f.content.clone())
        .expect("the claim file");
    let merged = Claim::parse(
        claim.id.clone(),
        std::str::from_utf8(&written).expect("text"),
    )
    .expect("readable");
    assert_eq!(
        s(&merged, 0).bearing,
        Bearing::Indirect,
        "the tip's change is kept"
    );
    assert_eq!(s(&merged, 0).note.as_deref(), Some("from the proposal"));
    assert_eq!(s(&merged, 1).note.as_deref(), Some("from the tip"));
    let _ = HOME;
}

#[test]
fn an_overlap_in_a_knowledge_file_stops_the_acceptance_with_both_texts_recorded() {
    let rig = Rig::new("k3-overlap");
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    rig.fake.set(
        "pages/One.md",
        "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n",
    );
    let mut claim = base_claim();
    claim.expressions = vec![Expression::new("p1", "b1").expect("e")];
    claim.relations.clear();
    claim.sources = vec![link(1, Link::Supports, Bearing::Direct, None, "w1")];
    let path = claim_path(&claim.id);
    rig.fake.set(&path, &claim.to_yaml());
    rig.fake.set(
        &source_path(&source_id(1)),
        &Source::new(source_id(1)).to_yaml(),
    );
    let mut ours = claim.clone();
    sm(&mut ours, 0).bearing = Bearing::Partial;
    let p = rig.propose(&author, "Partial", &[(&path, Some(&ours.to_yaml()))]);
    let mut tip = claim.clone();
    sm(&mut tip, 0).bearing = Bearing::Indirect;
    rig.fake.set(&path, &tip.to_yaml());
    let failure = rig
        .acceptor()
        .accept(&maintainer, &rig.wiki, &p.id, 1)
        .expect_err("stopped");
    assert!(
        matches!(failure, AcceptFailure::Overlap { overlaps: 1 }),
        "{failure:?}"
    );
    assert_eq!(rig.double.calls().len(), 0, "the signer was not asked");
    let recorded = inklkunn_store::ActStore::overlaps_of(&*rig.store, &p.id).expect("records");
    let texts = &recorded.first().expect("one record").texts;
    assert_eq!(texts.len(), 1);
    let t = texts.first().expect("one");
    assert_eq!(t.page, path);
    assert!(t.place.starts_with("knowledge sources/"), "{}", t.place);
    assert_eq!(t.theirs.as_deref(), Some("indirect"));
    assert_eq!(t.ours.as_deref(), Some("partial"));
}
