//! What only a curation proposal sets (RFC 005 part 2 D-3): a claim's community status, a source's review and
//! availability. The validator refuses them in any other proposal, by default.

use std::collections::BTreeMap;

use inklkunn_store::{FileChange, RevisionContent};

use super::check::{files_of, sound_claim};
use super::*;
use crate::knowledge::check::{Curated, PageLookup, Pages, Touched, validate};
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
