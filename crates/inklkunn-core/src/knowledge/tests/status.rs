//! What only a curation proposal sets (RFC 005 part 2 D-3): a claim's community status, a source's review and
//! availability. The validator refuses them in any other proposal, by default.

use std::collections::BTreeMap;

use inklkunn_store::{FileChange, RevisionContent};

use super::check::{files_of, said, sound_claim};
use super::*;
use crate::knowledge::check::{Curated, PageLookup, Pages, Touched, curated_by, validate};
use crate::knowledge::curation::is_curation;
use crate::knowledge::summary::summarize;

struct One;

impl Pages for One {
    fn with_id(&self, id: &str) -> PageLookup {
        if id == "p1" {
            PageLookup::One("pages/One.md".to_owned())
        } else {
            PageLookup::None
        }
    }
    fn text(&self, _: &str) -> Option<String> {
        Some("---\nid: p1\n---\n\nSaid. ^b1\n\nOther. ^b2\n".to_owned())
    }
}

/// The other wording a claim said in both paragraphs has (K33: a changed `judged_on` still names a wording it has).
fn other() -> Digest {
    digest_of_block("Other. ^b2")
}

/// A sound claim said in both paragraphs of [`One`].
fn said_twice(n: u8) -> Claim {
    let mut c = sound_claim(n);
    c.expressions
        .push(Expression::new("p1", "b2").expect("an expression"));
    c
}

fn decided(c: &mut Claim, status: Standing) {
    c.community_status = CommunityStatus {
        status,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(said()),
    };
}

fn run(claim: &Claim, before: Option<&Claim>, curation: bool) -> Vec<(String, Rule)> {
    let path = claim_path(&claim.id);
    let files: BTreeMap<String, Vec<u8>> = files_of(&[
        (path.clone(), claim.to_yaml()),
        (
            source_path(&source_id(1)),
            Source::new(source_id(1)).to_yaml(),
        ),
    ]);
    let mut touched = Touched {
        written: [path.clone()].into(),
        curation,
        ..Touched::default()
    };
    if let Some(b) = before {
        touched
            .curated_before
            .insert(path, Curated::of(&File::Claim(Box::new(b.clone()))));
    }
    validate(&files, &touched, &One, &One)
        .into_iter()
        .map(|r| (r.file, r.rule))
        .collect()
}

#[test]
fn a_new_claim_that_arrives_decided_is_refused_unless_the_proposal_is_a_curation() {
    let mut claim = sound_claim(1);
    assert!(run(&claim, None, false).is_empty(), "undecided is fine");
    decided(&mut claim, Standing::Accepted);
    let path = claim_path(&claim.id);
    assert_eq!(run(&claim, None, false), [(path.clone(), Rule::Status)]);
    assert!(run(&claim, None, true).is_empty());
}

#[test]
fn a_change_of_status_in_a_claim_that_exists_is_refused_unless_the_proposal_is_a_curation() {
    let mut before = sound_claim(1);
    decided(&mut before, Standing::Contested);
    let mut after = before.clone();
    after.community_status.status = Standing::Accepted;
    let path = claim_path(&after.id);
    assert_eq!(run(&after, Some(&before), false), [(path, Rule::Status)]);
    assert!(run(&after, Some(&before), true).is_empty());
}

#[test]
fn the_wording_a_status_was_judged_on_and_the_proposal_that_decided_it_are_part_of_it() {
    let mut before = said_twice(1);
    decided(&mut before, Standing::Accepted);
    let path = claim_path(&before.id);
    let mut judged = before.clone();
    judged.community_status.judged_on = Some(other());
    assert_eq!(
        run(&judged, Some(&before), false),
        [(path.clone(), Rule::Status)]
    );
    let mut forged = before.clone();
    forged.community_status.proposal = Some("01930000-0000-7000-8000-00000000bbbb".to_owned());
    assert_eq!(run(&forged, Some(&before), false), [(path, Rule::Status)]);
}

#[test]
fn a_claim_left_as_the_tip_has_it_is_no_change_of_status_whatever_it_is() {
    let mut before = sound_claim(1);
    decided(&mut before, Standing::Contested);
    let mut after = before.clone();
    after.kind = StatementKind::Opinion;
    assert!(run(&after, Some(&before), false).is_empty());
}

#[test]
fn a_claim_whose_status_was_decided_is_not_removed_by_any_other_proposal() {
    let mut before = sound_claim(1);
    decided(&mut before, Standing::Accepted);
    let path = claim_path(&before.id);
    let mut touched = Touched {
        removed: [path.clone()].into(),
        ..Touched::default()
    };
    touched
        .curated_before
        .insert(path.clone(), Curated::of(&File::Claim(Box::new(before))));
    let got = validate(&BTreeMap::new(), &touched, &One, &One);
    assert_eq!(
        got.iter()
            .map(|r| (r.file.clone(), r.rule))
            .collect::<Vec<_>>(),
        [(path.clone(), Rule::Status)]
    );
    touched.curation = true;
    assert!(validate(&BTreeMap::new(), &touched, &One, &One).is_empty());
    // A claim that holds nothing curated (undecided, no link) may go, as before.
    let mut touched = Touched {
        removed: [path.clone()].into(),
        ..Touched::default()
    };
    touched.curated_before.insert(
        path,
        Curated::of(&File::Claim(Box::new(with_links(vec![])))),
    );
    assert!(validate(&BTreeMap::new(), &touched, &One, &One).is_empty());
}

fn source_with(f: impl FnOnce(&mut Source)) -> Source {
    let mut s = Source::new(source_id(1));
    f(&mut s);
    s
}

fn run_source(source: &Source, before: Option<&Source>, curation: bool) -> Vec<(String, Rule)> {
    let path = source_path(&source.id);
    let files = files_of(&[(path.clone(), source.to_yaml())]);
    let mut touched = Touched {
        written: [path.clone()].into(),
        curation,
        ..Touched::default()
    };
    if let Some(b) = before {
        touched
            .curated_before
            .insert(path, Curated::of(&File::Source(Box::new(b.clone()))));
    }
    validate(&files, &touched, &One, &One)
        .into_iter()
        .map(|r| (r.file, r.rule))
        .collect()
}

#[test]
fn a_sources_review_and_availability_are_set_by_a_curation_proposal_only() {
    let plain = source_with(|_| {});
    assert!(run_source(&plain, None, false).is_empty());
    let path = source_path(&plain.id);
    let checked = source_with(|s| s.availability = Availability::Available);
    assert_eq!(
        run_source(&checked, None, false),
        [(path.clone(), Rule::Status)]
    );
    assert!(run_source(&checked, None, true).is_empty());
    let reviewed = source_with(|s| {
        s.last_reviewed = Some(Reviewed {
            date: "2026-11-04".to_owned(),
            proposal: None,
        });
    });
    assert_eq!(
        run_source(&reviewed, Some(&plain), false),
        [(path, Rule::Status)]
    );
    // The title of a source is no curation.
    let retitled = source_with(|s| s.identity.title = Some("A title".to_owned()));
    assert!(run_source(&retitled, Some(&plain), false).is_empty());
}

#[test]
fn a_refusal_names_the_field_it_is_about_for_a_source_on_both_surfaces() {
    // K29: `availability` and `last_reviewed`, never `community_status`; the submission and the acceptance say the same.
    let plain = source_with(|_| {});
    let path = source_path(&plain.id);
    let held = texts_of(&[], &[&plain]);
    let available = source_with(|s| s.availability = Availability::Available);
    let reviewed = source_with(|s| {
        s.last_reviewed = Some(Reviewed {
            date: "2026-11-04".to_owned(),
            proposal: None,
        });
    });
    for (after, field) in [(&available, "availability"), (&reviewed, "last_reviewed")] {
        let given = vec![(path.clone(), Some(after.to_yaml()))];
        assert_eq!(
            refused_files(&given, &held),
            [(path.clone(), field)],
            "{field}"
        );
        let files = files_of(&[(path.clone(), after.to_yaml())]);
        let mut touched = Touched {
            written: [path.clone()].into(),
            ..Touched::default()
        };
        touched.curated_before.insert(
            path.clone(),
            Curated::of(&File::Source(Box::new(plain.clone()))),
        );
        assert_eq!(
            validate(&files, &touched, &One, &One)
                .iter()
                .map(|r| (r.rule, r.field))
                .collect::<Vec<_>>(),
            [(Rule::Status, field)]
        );
    }
    // Both changed at once: the availability is named first.
    let both = source_with(|s| {
        s.availability = Availability::Available;
        s.last_reviewed = Some(Reviewed {
            date: "2026-11-04".to_owned(),
            proposal: None,
        });
    });
    assert_eq!(
        refused_files(&[(path.clone(), Some(both.to_yaml()))], &held),
        [(path, "availability")]
    );
}

fn reference(value: &str) -> Reference {
    Reference {
        form: ReferenceForm::Url,
        value: value.to_owned(),
    }
}

#[test]
fn what_a_checked_source_is_and_where_it_is_found_is_curation_and_before_the_check_it_is_not() {
    // K30: once a person checked a source (a review day or an availability), a page edit may not make it another work.
    let named = |f: &dyn Fn(&mut Source)| {
        source_with(|s| {
            s.identity = Identity {
                title: Some("A history".to_owned()),
                author: Some("An author".to_owned()),
                date: Some("1950".to_owned()),
            };
            s.reference = Some(reference("https://example.org/a"));
            f(s);
        })
    };
    let reviewed = Reviewed {
        date: "2026-11-04".to_owned(),
        proposal: None,
    };
    let path = source_path(&source_id(1));
    let both_surfaces = |after: &Source, before: &Source, want: Option<&'static str>, why: &str| {
        let held = texts_of(&[], &[before]);
        let given = vec![(path.clone(), Some(after.to_yaml()))];
        let by_submission: Vec<_> = refused_files(&given, &held);
        let files = files_of(&[(path.clone(), after.to_yaml())]);
        let mut touched = Touched {
            written: [path.clone()].into(),
            ..Touched::default()
        };
        touched.curated_before.insert(
            path.clone(),
            Curated::of(&File::Source(Box::new(before.clone()))),
        );
        let by_acceptance: Vec<_> = validate(&files, &touched, &One, &One)
            .iter()
            .map(|r| (r.file.clone(), r.field))
            .collect();
        let expected: Vec<(String, &str)> = want.map(|f| (path.clone(), f)).into_iter().collect();
        assert_eq!(by_submission, expected, "submission: {why}");
        assert_eq!(by_acceptance, expected, "acceptance: {why}");
        // A curation proposal may.
        touched.curation = true;
        assert!(validate(&files, &touched, &One, &One).is_empty(), "{why}");
    };
    // A source checked by its review day or by its availability.
    for (checked, how) in [
        (
            named(&|s| s.last_reviewed = Some(reviewed.clone())),
            "reviewed",
        ),
        (
            named(&|s| s.availability = Availability::Available),
            "available",
        ),
        (
            named(&|s| s.availability = Availability::Unavailable),
            "unavailable",
        ),
    ] {
        let keep = |after: &mut Source| {
            after.last_reviewed = checked.last_reviewed.clone();
            after.availability = checked.availability;
        };
        let changed = |f: &dyn Fn(&mut Source)| {
            let mut s = checked.clone();
            f(&mut s);
            keep(&mut s);
            s
        };
        both_surfaces(&checked, &checked, None, how);
        both_surfaces(
            &changed(&|s| s.identity.title = Some("Another".to_owned())),
            &checked,
            Some("identity"),
            how,
        );
        both_surfaces(
            &changed(&|s| s.identity.author = None),
            &checked,
            Some("identity"),
            how,
        );
        both_surfaces(
            &changed(&|s| s.identity.date = Some("1951".to_owned())),
            &checked,
            Some("identity"),
            how,
        );
        both_surfaces(
            &changed(&|s| s.reference = Some(reference("https://example.org/b"))),
            &checked,
            Some("reference"),
            how,
        );
        both_surfaces(
            &changed(&|s| s.reference = None),
            &checked,
            Some("reference"),
            how,
        );
        // What the check does not vouch for stays a page edit's: the kind of source.
        both_surfaces(
            &changed(&|s| s.kind = SourceKind::Primary),
            &checked,
            None,
            how,
        );
    }
    // An unchecked source's identity and reference are the page edit's, and so is a new source's.
    let unchecked = named(&|_| {});
    let mut retitled = unchecked.clone();
    retitled.identity.title = Some("Another".to_owned());
    retitled.reference = Some(reference("https://example.org/b"));
    both_surfaces(&retitled, &unchecked, None, "unchecked");
    assert!(
        refused_files(&[(path.clone(), Some(retitled.to_yaml()))], &Texts::new()).is_empty(),
        "a new source"
    );
    // Changed together with the check itself, the check is named first (it is what moves).
    let mut moved = retitled.clone();
    moved.availability = Availability::Available;
    both_surfaces(&moved, &unchecked, Some("availability"), "checked now");
    // And at submit the proposal is the page's rule: refused unless it is a curation proposal.
    let checked = named(&|s| s.availability = Availability::Available);
    let mut other = checked.clone();
    other.identity.title = Some("Another".to_owned());
    let held = texts_of(&[], &[&checked]);
    let was = |p: &str| held.get(p).cloned();
    let given = vec![(path, Some(other.to_yaml()))];
    assert_eq!(
        crate::proposal::files::knowledge_verdict(&given, &was, false, &summarize, &One),
        Err(crate::proposal::Invalid::KnowledgeCurated)
    );
    assert_eq!(
        crate::proposal::files::knowledge_verdict(&given, &was, true, &summarize, &One),
        Ok(())
    );
}

fn content(files: Vec<FileChange>, renames: Vec<inklkunn_store::Rename>) -> RevisionContent {
    RevisionContent {
        base: inklkunn_store::Base::new(&"a".repeat(64), &"b".repeat(64)).expect("a base"),
        files,
        renames,
        minor: false,
        automated: false,
    }
}

fn change(path: &str) -> FileChange {
    FileChange {
        path: path.to_owned(),
        text: Some("x".to_owned()),
    }
}

#[test]
fn a_curation_is_a_revision_of_knowledge_files_alone() {
    let claim = claim_path(&claim_id(1));
    let source = source_path(&source_id(1));
    assert!(is_curation(&content(vec![change(&claim)], vec![])));
    assert!(is_curation(&content(
        vec![change(&claim), change(&source)],
        vec![]
    )));
    // No file: nothing is proposed. A page among the files: a page edit. A rename: a page edit.
    assert!(!is_curation(&content(vec![], vec![])));
    assert!(!is_curation(&content(
        vec![change(&claim), change("pages/Home.md")],
        vec![]
    )));
    assert!(!is_curation(&content(
        vec![change(&claim)],
        vec![inklkunn_store::Rename {
            from: "pages/A.md".to_owned(),
            to: "pages/B.md".to_owned()
        }]
    )));
    // A file under knowledge/ that is neither a claim's nor a source's is not curation either.
    assert!(!is_curation(&content(
        vec![change("knowledge/notes.txt")],
        vec![]
    )));
}

// ------------------------------------------------------------------------------------ K6: the gate is D-3

/// What `validate` says of one claim, with the key of the format each refusal is about.
fn run_keys(claim: &Claim, before: Option<&Claim>, curation: bool) -> Vec<(Rule, &'static str)> {
    let path = claim_path(&claim.id);
    let files: BTreeMap<String, Vec<u8>> = files_of(&[
        (path.clone(), claim.to_yaml()),
        (
            source_path(&source_id(1)),
            Source::new(source_id(1)).to_yaml(),
        ),
        (
            source_path(&source_id(2)),
            Source::new(source_id(2)).to_yaml(),
        ),
    ]);
    let mut touched = Touched {
        written: [path.clone()].into(),
        curation,
        ..Touched::default()
    };
    if let Some(b) = before {
        touched
            .curated_before
            .insert(path, Curated::of(&File::Claim(Box::new(b.clone()))));
    }
    validate(&files, &touched, &One, &One)
        .into_iter()
        .map(|r| (r.rule, r.field))
        .collect()
}

fn link(source: u8, f: impl FnOnce(&mut SourceLink)) -> SourceLink {
    let mut l = SourceLink {
        source: source_id(source),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: said(),
    };
    f(&mut l);
    l
}

fn with_links(links: Vec<SourceLink>) -> Claim {
    let mut c = said_twice(1);
    c.sources = links;
    c
}

#[test]
fn every_field_of_an_existing_source_link_is_curation_but_the_note() {
    let before = with_links(vec![link(1, |_| {})]);
    let sources = (Rule::Status, "sources");
    let changed: [(&str, Claim); 3] = [
        (
            "relation",
            with_links(vec![link(1, |l| l.relation = Link::Challenges)]),
        ),
        (
            "bearing",
            with_links(vec![link(1, |l| l.bearing = Bearing::Indirect)]),
        ),
        (
            "judged_on",
            with_links(vec![link(1, |l| l.judged_on = other())]),
        ),
    ];
    for (what, after) in &changed {
        assert_eq!(
            run_keys(after, Some(&before), false),
            [sources],
            "a changed {what} is refused outside a curation proposal"
        );
        assert!(
            run_keys(after, Some(&before), true).is_empty(),
            "a changed {what} is what a curation proposal is for"
        );
    }
    // The note is words, and goes with the page edit.
    let noted = with_links(vec![link(1, |l| l.note = Some("a note".to_owned()))]);
    assert!(run_keys(&noted, Some(&before), false).is_empty());
}

#[test]
fn taking_an_existing_source_link_away_is_curation_and_adding_one_is_not() {
    let before = with_links(vec![link(1, |_| {}), link(2, |_| {})]);
    let removed = with_links(vec![link(1, |_| {})]);
    assert_eq!(
        run_keys(&removed, Some(&before), false),
        [(Rule::Status, "sources")]
    );
    assert!(run_keys(&removed, Some(&before), true).is_empty());
    let none = with_links(vec![]);
    assert_eq!(
        run_keys(&none, Some(&before), false),
        [(Rule::Status, "sources")]
    );
    // Attaching a new link stays with the page edit it belongs to: the old one untouched.
    let narrower = with_links(vec![link(1, |_| {})]);
    let added = with_links(vec![link(1, |_| {}), link(2, |_| {})]);
    assert!(run_keys(&added, Some(&narrower), false).is_empty());
    // A new claim brings whatever links it likes.
    assert!(run_keys(&added, None, false).is_empty());
    // Swapping one link for another is a removal and an addition: refused for the removal.
    let swapped = with_links(vec![link(2, |_| {})]);
    assert_eq!(
        run_keys(&swapped, Some(&narrower), false),
        [(Rule::Status, "sources")]
    );
}

#[test]
fn a_status_and_a_link_changed_together_name_the_status() {
    let mut before = with_links(vec![link(1, |_| {})]);
    decided(&mut before, Standing::Contested);
    let mut after = before.clone();
    after.community_status.status = Standing::Accepted;
    after.sources = vec![link(1, |l| l.bearing = Bearing::Partial)];
    assert_eq!(
        run_keys(&after, Some(&before), false),
        [(Rule::Status, "community_status")]
    );
}

// ------------------------------------------------------------------------------------ K7: the same rule at submit

type Texts = BTreeMap<String, String>;

fn texts_of(claims: &[&Claim], sources: &[&Source]) -> Texts {
    let mut out = Texts::new();
    for c in claims {
        out.insert(claim_path(&c.id), c.to_yaml());
    }
    for s in sources {
        out.insert(source_path(&s.id), s.to_yaml());
    }
    out
}

fn as_written(claims: &[&Claim], sources: &[&Source]) -> Vec<(String, Option<String>)> {
    texts_of(claims, sources)
        .into_iter()
        .map(|(p, t)| (p, Some(t)))
        .collect()
}

fn refused_files(
    written: &[(String, Option<String>)],
    before: &Texts,
) -> Vec<(String, &'static str)> {
    curated_by(written, &|p| before.get(p).cloned())
        .into_iter()
        .map(|r| {
            assert_eq!(r.rule, Rule::Status);
            (r.file, r.field)
        })
        .collect()
}

#[test]
fn the_rule_the_page_and_the_submission_use_is_the_one_the_acceptance_uses() {
    // Every change of the kinds the rule is about, judged by the acceptance's `validate` and by `curated_by`: they
    // agree on whether the file is refused, and on the key it is refused for.
    let plain = with_links(vec![link(1, |_| {})]);
    let mut decided_claim = plain.clone();
    decided(&mut decided_claim, Standing::Accepted);
    let mut other_status = decided_claim.clone();
    other_status.community_status.status = Standing::Contested;
    let mut other_wording = decided_claim.clone();
    other_wording.community_status.judged_on = Some(other());
    let mut kinded = decided_claim.clone();
    kinded.kind = StatementKind::Opinion;
    let relinked = with_links(vec![link(1, |l| l.relation = Link::Challenges)]);
    let rebound = with_links(vec![link(1, |l| l.judged_on = other())]);
    let rebearing = with_links(vec![link(1, |l| l.bearing = Bearing::Partial)]);
    let unlinked = with_links(vec![]);
    let added = with_links(vec![link(1, |_| {}), link(2, |_| {})]);
    let cases: Vec<(&Claim, Option<&Claim>)> = vec![
        (&plain, None),
        (&decided_claim, None),
        (&decided_claim, Some(&plain)),
        (&plain, Some(&decided_claim)),
        (&other_status, Some(&decided_claim)),
        (&other_wording, Some(&decided_claim)),
        (&kinded, Some(&decided_claim)),
        (&decided_claim, Some(&decided_claim)),
        (&relinked, Some(&plain)),
        (&rebound, Some(&plain)),
        (&rebearing, Some(&plain)),
        (&unlinked, Some(&plain)),
        (&added, Some(&plain)),
        (&plain, Some(&added)),
    ];
    for (n, (after, before)) in cases.into_iter().enumerate() {
        let acceptance = run_keys(after, before, false);
        let held: Texts = before.map_or_else(Texts::new, |b| texts_of(&[b], &[]));
        let submit = refused_files(&as_written(&[after], &[]), &held);
        assert_eq!(
            acceptance.iter().map(|(_, k)| *k).collect::<Vec<_>>(),
            submit.iter().map(|(_, k)| *k).collect::<Vec<_>>(),
            "case {n}: the acceptance and the submission disagree"
        );
        // A curation proposal is judged by neither: the caller does not ask.
        assert!(run_keys(after, before, true).is_empty(), "case {n}");
    }
}

#[test]
fn a_removal_of_a_decided_claim_is_refused_at_submit_and_an_undecided_one_is_not() {
    let mut plain = with_links(vec![]);
    plain.id = claim_id(1);
    let mut decided_claim = sound_claim(2);
    decided(&mut decided_claim, Standing::Accepted);
    let before = texts_of(&[&plain, &decided_claim], &[]);
    let gone = |c: &Claim| vec![(claim_path(&c.id), None)];
    assert!(refused_files(&gone(&plain), &before).is_empty());
    assert_eq!(
        refused_files(&gone(&decided_claim), &before),
        [(claim_path(&decided_claim.id), "community_status")]
    );
    // A file the base never had is not a decided claim.
    assert!(refused_files(&gone(&decided_claim), &Texts::new()).is_empty());
    // A source that nobody checked holds nothing curated: anyone may remove it.
    let source = Source::new(source_id(1));
    let held = texts_of(&[], &[&source]);
    assert!(refused_files(&[(source_path(&source.id), None)], &held).is_empty());
}

// ------------------------------------------------------------------------------------ K16: removal is curation

fn gone(path: String) -> Vec<(String, Option<String>)> {
    vec![(path, None)]
}

#[test]
fn removing_a_claim_file_is_curation_whenever_it_takes_anything_curated_with_it() {
    let at = |c: &Claim| claim_path(&c.id);
    let held = |c: &Claim| texts_of(&[c], &[]);
    let nothing_curated = with_links(vec![]);
    assert!(refused_files(&gone(at(&nothing_curated)), &held(&nothing_curated)).is_empty());
    let refused = |c: &Claim, field: &'static str| {
        assert_eq!(
            refused_files(&gone(at(c)), &held(c)),
            [(at(c), field)],
            "{c:?}"
        );
        // The acceptance's rule is the same one: `validate` refuses it outside a curation proposal and not within.
        let mut touched = Touched {
            removed: [at(c)].into(),
            ..Touched::default()
        };
        touched
            .curated_before
            .insert(at(c), Curated::of(&File::Claim(Box::new(c.clone()))));
        assert_eq!(
            validate(&BTreeMap::new(), &touched, &One, &One)
                .iter()
                .map(|r| (r.file.clone(), r.rule, r.field))
                .collect::<Vec<_>>(),
            [(at(c), Rule::Status, field)],
            "the acceptance names the same field"
        );
        touched.curation = true;
        assert!(validate(&BTreeMap::new(), &touched, &One, &One).is_empty());
    };
    // A source link, of either relation: the link is curation's to take away, and so is the file that holds it.
    refused(&with_links(vec![link(1, |_| {})]), "sources");
    refused(
        &with_links(vec![link(1, |l| l.relation = Link::Challenges)]),
        "sources",
    );
    // A status that was decided; and a record that un-decided it (K14: no status, the deciding proposal kept).
    let mut decided_claim = with_links(vec![]);
    decided(&mut decided_claim, Standing::Contested);
    refused(&decided_claim, "community_status");
    let mut undecided_again = with_links(vec![]);
    undecided_again.community_status = CommunityStatus {
        status: Standing::Unknown,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: None,
    };
    refused(&undecided_again, "community_status");
    // A status that was judged on a wording and no more.
    let mut wording_only = with_links(vec![]);
    wording_only.community_status.judged_on = Some(digest_of("w"));
    refused(&wording_only, "community_status");
}

#[test]
fn removing_a_source_file_is_curation_when_it_was_checked() {
    let at = |s: &Source| source_path(&s.id);
    let refused = |s: &Source, field: &'static str| {
        assert_eq!(
            refused_files(&gone(at(s)), &texts_of(&[], &[s])),
            [(at(s), field)]
        );
        // The acceptance names the same field.
        let mut touched = Touched {
            removed: [at(s)].into(),
            ..Touched::default()
        };
        touched
            .curated_before
            .insert(at(s), Curated::of(&File::Source(Box::new(s.clone()))));
        assert_eq!(
            validate(&BTreeMap::new(), &touched, &One, &One)
                .iter()
                .map(|r| (r.rule, r.field))
                .collect::<Vec<_>>(),
            [(Rule::Status, field)]
        );
    };
    let plain = source_with(|_| {});
    assert!(refused_files(&gone(at(&plain)), &texts_of(&[], &[&plain])).is_empty());
    refused(
        &source_with(|s| s.availability = Availability::Unavailable),
        "availability",
    );
    refused(
        &source_with(|s| {
            s.last_reviewed = Some(Reviewed {
                date: "2026-11-04".to_owned(),
                proposal: None,
            });
        }),
        "last_reviewed",
    );
    // Both set: the availability is named (the order `departure` reads them in).
    refused(
        &source_with(|s| {
            s.availability = Availability::Available;
            s.last_reviewed = Some(Reviewed {
                date: "2026-11-04".to_owned(),
                proposal: None,
            });
        }),
        "availability",
    );
    // The same through `validate`, outside and inside a curation proposal.
    let checked = source_with(|s| s.availability = Availability::Available);
    let mut touched = Touched {
        removed: [at(&checked)].into(),
        ..Touched::default()
    };
    touched.curated_before.insert(
        at(&checked),
        Curated::of(&File::Source(Box::new(checked.clone()))),
    );
    assert_eq!(validate(&BTreeMap::new(), &touched, &One, &One).len(), 1);
    touched.curation = true;
    assert!(validate(&BTreeMap::new(), &touched, &One, &One).is_empty());
}

#[test]
fn a_challenge_is_not_laundered_out_by_removing_its_claim_and_writing_another_on_the_same_paragraph()
 {
    // L-b: claim C (undecided, with a *challenges* link) is removed, and C2 is recorded on the same paragraph with only a
    // *supports* link. Each file alone looks harmless; the removal of C is what takes the challenge away.
    let c = with_links(vec![link(1, |l| l.relation = Link::Challenges)]);
    let mut c2 = with_links(vec![link(1, |_| {})]);
    c2.id = claim_id(2);
    let held = texts_of(&[&c], &[]);
    let written = vec![
        (claim_path(&c.id), None),
        (claim_path(&c2.id), Some(c2.to_yaml())),
    ];
    assert_eq!(
        refused_files(&written, &held),
        [(claim_path(&c.id), "sources")]
    );
    // The proposal service says so at submit: with a page beside it the proposal is not a curation proposal.
    let was = |p: &str| held.get(p).cloned();
    assert_eq!(
        crate::proposal::files::knowledge_verdict(&written, &was, false, &summarize, &One),
        Err(crate::proposal::Invalid::KnowledgeCurated)
    );
    assert_eq!(
        crate::proposal::files::knowledge_verdict(&written, &was, true, &summarize, &One),
        Ok(())
    );
}

// -- a file the tip has and cannot read

/// A claim file that does not read (a direction character in a text, as K14 makes invalid).
const UNREAD: &str = "kind: observation\nsecret: words\n";

#[test]
fn a_file_the_base_has_and_cannot_read_is_not_replaced_or_removed_by_any_other_proposal() {
    let path = claim_path(&claim_id(5));
    let held: Texts = [(path.clone(), UNREAD.to_owned())].into();
    // Removed: curation's.
    assert_eq!(
        refused_files(&gone(path.clone()), &held),
        [(path.clone(), "unreadable")]
    );
    // Written over, with a claim that reads: curation's too, and not a new claim.
    let mut now = sound_claim(5);
    now.id = claim_id(5);
    let over = vec![(path.clone(), Some(now.to_yaml()))];
    assert_eq!(refused_files(&over, &held), [(path.clone(), "unreadable")]);
    // A file the base never had is a new one, and nothing is replaced.
    assert!(refused_files(&over, &Texts::new()).is_empty());
    // The page's lines: its own line, and the new claim beside it, never *Removes a claim*.
    let was = |p: &str| held.get(p).cloned();
    let removal = summarize(&gone(path.clone()), &was);
    assert_eq!(
        removal,
        [crate::knowledge::summary::Item::UnreadableReplaced {
            family: Family::Claim,
            id: claim_id(5).to_string()
        }]
    );
    let overwritten = summarize(&over, &was);
    assert!(matches!(
        overwritten.as_slice(),
        [
            crate::knowledge::summary::Item::UnreadableReplaced { .. },
            crate::knowledge::summary::Item::ClaimMade { .. }
        ]
    ));
    // The service refuses it at submit outside a curation proposal and sends it as one.
    use crate::proposal::{Invalid, files::knowledge_verdict};
    assert_eq!(
        knowledge_verdict(&over, &was, false, &summarize, &One),
        Err(Invalid::KnowledgeCurated)
    );
    assert_eq!(
        knowledge_verdict(&over, &was, true, &summarize, &One),
        Ok(())
    );
    assert_eq!(
        knowledge_verdict(&gone(path), &was, false, &summarize, &One),
        Err(Invalid::KnowledgeCurated)
    );
}

#[test]
fn the_acceptance_refuses_the_same_outside_a_curation_proposal() {
    let path = claim_path(&claim_id(5));
    let mut now = sound_claim(5);
    now.id = claim_id(5);
    let files: BTreeMap<String, Vec<u8>> = files_of(&[
        (path.clone(), now.to_yaml()),
        (
            source_path(&source_id(1)),
            Source::new(source_id(1)).to_yaml(),
        ),
    ]);
    let mut touched = Touched {
        written: [path.clone()].into(),
        ..Touched::default()
    };
    touched.unreadable_before.insert(path.clone());
    let got = validate(&files, &touched, &One, &One);
    assert_eq!(
        got.iter().map(|r| (r.rule, r.field)).collect::<Vec<_>>(),
        [(Rule::Status, "unreadable")]
    );
    assert_eq!(got.first().map(|r| r.file.as_str()), Some(path.as_str()));
    touched.curation = true;
    assert!(validate(&files, &touched, &One, &One).is_empty());
    // A removal, likewise.
    let mut touched = Touched {
        removed: [path.clone()].into(),
        ..Touched::default()
    };
    touched.unreadable_before.insert(path);
    assert_eq!(validate(&BTreeMap::new(), &touched, &One, &One).len(), 1);
    touched.curation = true;
    assert!(validate(&BTreeMap::new(), &touched, &One, &One).is_empty());
}

#[test]
fn what_only_a_curation_proposal_sets_is_told_from_everything_else() {
    let plain_claim = sound_claim(1);
    let mut decided_claim = sound_claim(2);
    decided(&mut decided_claim, Standing::Accepted);
    let plain_source = Source::new(source_id(1));
    let checked = source_with(|s| s.availability = Availability::Available);
    let nothing = Texts::new();
    assert!(refused_files(&as_written(&[&plain_claim], &[]), &nothing).is_empty());
    assert_eq!(
        refused_files(&as_written(&[&decided_claim], &[]), &nothing).len(),
        1
    );
    assert!(refused_files(&as_written(&[], &[&plain_source]), &nothing).is_empty());
    assert_eq!(
        refused_files(&as_written(&[], &[&checked]), &nothing),
        [(source_path(&checked.id), "availability")]
    );
    // A review date in a source that exists, a retitled source, and a file that does not read.
    let held = texts_of(&[&plain_claim], &[&plain_source]);
    let reviewed = source_with(|s| {
        s.last_reviewed = Some(Reviewed {
            date: "2026-11-04".to_owned(),
            proposal: None,
        });
    });
    assert_eq!(
        refused_files(&as_written(&[], &[&reviewed]), &held),
        [(source_path(&reviewed.id), "last_reviewed")]
    );
    let retitled = source_with(|s| s.identity.title = Some("T".to_owned()));
    assert!(refused_files(&as_written(&[], &[&retitled]), &held).is_empty());
    let broken = vec![(claim_path(&claim_id(9)), Some("not: a claim".to_owned()))];
    assert!(refused_files(&broken, &nothing).is_empty());
    // Pages are not the layer's.
    let page = vec![("pages/One.md".to_owned(), Some("text".to_owned()))];
    assert!(refused_files(&page, &nothing).is_empty());
}

#[test]
fn the_tree_the_acceptance_reads_names_a_tip_file_it_cannot_read_and_curation_may_replace_it() {
    use crate::knowledge::check::judge;
    use inklkunn_signer::PreparedFile;
    let root = std::env::temp_dir().join(format!("inklkunn-k16-judge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let path = claim_path(&claim_id(5));
    let source = Source::new(source_id(1));
    for (rel, text) in [
        (path.clone(), UNREAD.to_owned()),
        (source_path(&source.id), source.to_yaml()),
        (
            "pages/One.md".to_owned(),
            "---\nid: p1\n---\n\nSaid. ^b1\n".to_owned(),
        ),
    ] {
        let at = root.join(&rel);
        std::fs::create_dir_all(at.parent().expect("a parent")).expect("a directory");
        std::fs::write(at, text).expect("a file");
    }
    let mut now = sound_claim(5);
    now.id = claim_id(5);
    let remove = [PreparedFile {
        path: path.clone().into(),
        content: None,
    }];
    let write = [PreparedFile {
        path: path.clone().into(),
        content: Some(now.to_yaml().into_bytes()),
    }];
    let judged = |files: &[PreparedFile], curation: bool| {
        judge(&root, 1000, 1 << 20, files, &[], curation)
            .expect("judged")
            .iter()
            .map(|r| (r.file.clone(), r.rule, r.field))
            .collect::<Vec<_>>()
    };
    let want = vec![(path.clone(), Rule::Status, "unreadable")];
    assert_eq!(judged(&remove, false), want);
    assert_eq!(judged(&write, false), want);
    assert!(judged(&remove, true).is_empty());
    assert!(judged(&write, true).is_empty());
    // A proposal that does not touch the file leaves it to the health list.
    assert!(judged(&[], false).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
