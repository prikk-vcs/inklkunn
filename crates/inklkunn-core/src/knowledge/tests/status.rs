//! What only a curation proposal sets (RFC 005 part 2 D-3): a claim's community status, a source's review and
//! availability. The validator refuses them in any other proposal, by default.

use std::collections::BTreeMap;

use inklkunn_store::{FileChange, RevisionContent};

use super::check::{files_of, sound_claim};
use super::*;
use crate::knowledge::check::{Curated, PageLookup, Pages, Touched, curated_by, validate};
use crate::knowledge::curation::is_curation;

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
        Some("---\nid: p1\n---\n\nSaid. ^b1\n".to_owned())
    }
}

fn decided(c: &mut Claim, status: Standing) {
    c.community_status = CommunityStatus {
        status,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of("w")),
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
    let mut before = sound_claim(1);
    decided(&mut before, Standing::Accepted);
    let path = claim_path(&before.id);
    let mut judged = before.clone();
    judged.community_status.judged_on = Some(digest_of("another wording"));
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
    // An undecided claim may go, as before.
    let mut touched = Touched {
        removed: [path.clone()].into(),
        ..Touched::default()
    };
    touched
        .curated_before
        .insert(path, Curated::of(&File::Claim(Box::new(sound_claim(1)))));
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
        judged_on: digest_of("w"),
    };
    f(&mut l);
    l
}

fn with_links(links: Vec<SourceLink>) -> Claim {
    let mut c = sound_claim(1);
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
            with_links(vec![link(1, |l| {
                l.judged_on = digest_of("another wording")
            })]),
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
    other_wording.community_status.judged_on = Some(digest_of("another"));
    let mut kinded = decided_claim.clone();
    kinded.kind = StatementKind::Opinion;
    let relinked = with_links(vec![link(1, |l| l.relation = Link::Challenges)]);
    let rebound = with_links(vec![link(1, |l| l.judged_on = digest_of("elsewhere"))]);
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
    let plain = sound_claim(1);
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
    // Sources are removed by anyone: a source holds no standing.
    let source = Source::new(source_id(1));
    let held = texts_of(&[], &[&source]);
    assert!(refused_files(&[(source_path(&source.id), None)], &held).is_empty());
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
