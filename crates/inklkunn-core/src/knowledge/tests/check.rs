//! The validator on its own: the files of a view, the pages of a view, and what a change did.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::knowledge::check::{PageLookup, Pages, Refusal, Touched, validate};

/// Pages in memory: `(path, text)`.
pub(super) struct Mem(pub(super) Vec<(String, String)>);

impl Pages for Mem {
    fn with_id(&self, id: &str) -> PageLookup {
        let found: Vec<&String> = self
            .0
            .iter()
            .filter(|(_, text)| {
                inklkunn_markup::parse_metadata(text)
                    .ok()
                    .and_then(|m| m.id)
                    .as_deref()
                    == Some(id)
            })
            .map(|(path, _)| path)
            .collect();
        match found.as_slice() {
            [] => PageLookup::None,
            [one] => PageLookup::One((*one).clone()),
            _ => PageLookup::Several,
        }
    }

    fn text(&self, path: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, t)| t.clone())
    }
}

pub(super) fn page(id: &str, body: &str) -> String {
    format!("---\ntitle: T-{id}\nid: {id}\n---\n\n{body}\n")
}

/// The wording `p1#b1` has in the test pages (K25: a link is judged on a wording its claim's paragraph has).
pub(super) fn said() -> Digest {
    digest_of_block("Said. ^b1")
}

/// A sound claim anchored at `p1#b1`, sourced by source 1, judged on the wording the paragraph has.
pub(super) fn sound_claim(n: u8) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.kind = StatementKind::Observation;
    c.expressions = vec![Expression::new("p1", "b1").expect("an expression")];
    c.sources = vec![SourceLink {
        source: source_id(1),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: said(),
    }];
    c
}

pub(super) fn files_of(list: &[(String, String)]) -> BTreeMap<String, Vec<u8>> {
    list.iter()
        .map(|(p, t)| (p.clone(), t.clone().into_bytes()))
        .collect()
}

fn claim_file(c: &Claim) -> (String, String) {
    (claim_path(&c.id), c.to_yaml())
}

fn source_file(s: &Source) -> (String, String) {
    (source_path(&s.id), s.to_yaml())
}

fn pages() -> Mem {
    Mem(vec![
        (
            "pages/One.md".to_owned(),
            page("p1", "Said. ^b1\n\nOther. ^b2"),
        ),
        ("pages/Two.md".to_owned(), page("p2", "Plain.")),
    ])
}

fn touched(written: &[&str]) -> Touched {
    Touched {
        written: written.iter().map(|s| (*s).to_owned()).collect(),
        ..Touched::default()
    }
}

fn rules(refusals: &[Refusal]) -> Vec<(String, Rule)> {
    refusals.iter().map(|r| (r.file.clone(), r.rule)).collect()
}

#[test]
fn a_sound_claim_with_its_source_and_its_block_is_accepted() {
    let claim = sound_claim(1);
    let files = files_of(&[claim_file(&claim), source_file(&Source::new(source_id(1)))]);
    let t = touched(&[&claim_path(&claim.id)]);
    assert!(validate(&files, &t, &pages(), &pages()).is_empty());
}

#[test]
fn a_file_that_is_not_sound_is_named_with_its_rule() {
    let path = claim_path(&claim_id(1));
    let files = files_of(&[(path.clone(), "kind: observation\nbearing: 3\n".to_owned())]);
    let got = validate(&files, &touched(&[&path]), &pages(), &pages());
    assert_eq!(rules(&got), [(path, Rule::Unknown)]);
}

#[test]
fn a_source_or_claim_the_view_lacks_is_refused() {
    let mut claim = sound_claim(1);
    claim.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(77),
    }];
    let path = claim_path(&claim.id);
    // The source is missing, and so is the claim it refines.
    let files = files_of(&[claim_file(&claim)]);
    let got = validate(&files, &touched(&[&path]), &pages(), &pages());
    let fields: Vec<_> = got.iter().map(|r| (r.rule, r.field)).collect();
    assert_eq!(
        fields,
        [(Rule::Reference, "relations"), (Rule::Reference, "sources")]
    );
    // The same names, present: sound.
    let files = files_of(&[
        claim_file(&claim),
        source_file(&Source::new(source_id(1))),
        claim_file(&sound_claim(77)),
    ]);
    assert!(validate(&files, &touched(&[&path]), &pages(), &pages()).is_empty());
    // Present but not readable is as good as absent: the claim would name something no one can read.
    let files = files_of(&[
        claim_file(&claim),
        (source_path(&source_id(1)), "not yaml: [".to_owned()),
        claim_file(&sound_claim(77)),
    ]);
    let got = validate(&files, &touched(&[&path]), &pages(), &pages());
    assert_eq!(got.len(), 1);
    assert_eq!(got.first().map(|r| r.rule), Some(Rule::Reference));
}

#[test]
fn a_source_that_is_replaced_by_a_source_the_view_lacks_is_refused() {
    let mut s = Source::new(source_id(1));
    s.superseded_by = Some(source_id(2));
    let path = source_path(&s.id);
    let files = files_of(&[source_file(&s)]);
    let got = validate(&files, &touched(&[&path]), &pages(), &pages());
    assert_eq!(rules(&got), [(path.clone(), Rule::Reference)]);
    let files = files_of(&[source_file(&s), source_file(&Source::new(source_id(2)))]);
    assert!(validate(&files, &touched(&[&path]), &pages(), &pages()).is_empty());
}

/// A source replaced by `later` (none: replaced by nothing).
fn replaced_by(n: u8, later: Option<u8>) -> Source {
    let mut s = Source::new(source_id(n));
    s.superseded_by = later.map(source_id);
    s
}

#[test]
fn sources_that_replace_one_another_never_return_to_a_source_they_passed() {
    let cycle_refusals = |list: &[Source], written: &[u8]| {
        let files = files_of(&list.iter().map(source_file).collect::<Vec<_>>());
        let paths: Vec<String> = written
            .iter()
            .map(|n| source_path(&source_id(*n)))
            .collect();
        let t = touched(&paths.iter().map(String::as_str).collect::<Vec<_>>());
        validate(&files, &t, &pages(), &pages())
            .iter()
            .map(|r| (r.file.clone(), r.rule, r.field))
            .collect::<Vec<_>>()
    };
    let at = |n: u8| source_path(&source_id(n));
    // A chain that ends is sound, however long.
    let chain = [
        replaced_by(1, Some(2)),
        replaced_by(2, Some(3)),
        replaced_by(3, None),
    ];
    assert!(cycle_refusals(&chain, &[1, 2, 3]).is_empty());
    // A source replaced by itself is refused by the format before a chain is walked.
    assert_eq!(
        cycle_refusals(&[replaced_by(1, Some(1))], &[1]),
        [(at(1), Rule::Value, "superseded_by")]
    );
    // Two written in one change: A to B and B to A, each refused by its own file.
    let two = [replaced_by(1, Some(2)), replaced_by(2, Some(1))];
    assert_eq!(
        cycle_refusals(&two, &[1, 2]),
        [
            (at(1), Rule::Reference, "superseded_by"),
            (at(2), Rule::Reference, "superseded_by")
        ]
    );
    // Across two changes: the tip has A to B, and this change writes B to A, the only file it writes.
    assert_eq!(
        cycle_refusals(&two, &[2]),
        [(at(2), Rule::Reference, "superseded_by")]
    );
    // A longer circle, and a chain that joins one that was already there.
    let three = [
        replaced_by(1, Some(2)),
        replaced_by(2, Some(3)),
        replaced_by(3, Some(1)),
    ];
    assert_eq!(cycle_refusals(&three, &[3]).len(), 1);
    let joins = [
        replaced_by(4, Some(1)),
        replaced_by(1, Some(2)),
        replaced_by(2, Some(1)),
    ];
    assert_eq!(
        cycle_refusals(&joins, &[4]),
        [(at(4), Rule::Reference, "superseded_by")]
    );
    // Clearing the successor that closed the circle, or pointing it elsewhere, is sound.
    let opened = [replaced_by(1, Some(2)), replaced_by(2, None)];
    assert!(cycle_refusals(&opened, &[2]).is_empty());
}

#[test]
fn the_chain_walk_reads_each_source_once_and_stops_at_the_end_or_the_return() {
    use crate::knowledge::check::successor_returns;
    let mut asked = Vec::new();
    let got = successor_returns(&source_id(1), |id| -> Result<_, ()> {
        asked.push(id.clone());
        Ok(match id {
            i if *i == source_id(1) => Some(source_id(2)),
            i if *i == source_id(2) => Some(source_id(3)),
            _ => None,
        })
    });
    assert_eq!(
        (got, asked),
        (Ok(false), vec![source_id(1), source_id(2), source_id(3)])
    );
    // A source that cannot be read ends the walk with the reason.
    let unreadable = successor_returns(&source_id(1), |_| Err::<Option<_>, _>("unreadable"));
    assert_eq!(unreadable, Err("unreadable"));
}

#[test]
fn a_walk_over_many_chains_costs_a_step_for_each_source_and_agrees_with_walking_each_chain_alone() {
    use crate::knowledge::check::{SuccessorWalk, successor_returns};
    // K69: sources 1 ... 200, each replaced by the next. `end` says what the last one is replaced by.
    let next_of = |end: Option<u8>| {
        move |id: &SourceId| -> Result<Option<SourceId>, ()> {
            Ok((1..200)
                .find(|n| source_id(*n) == *id)
                .map(|n| source_id(n + 1))
                .or_else(|| {
                    (*id == source_id(200))
                        .then(|| end.map(source_id))
                        .flatten()
                }))
        }
    };
    for end in [None, Some(1), Some(100), Some(200)] {
        let (mut walk, mut steps) = (SuccessorWalk::default(), 0);
        for start in (1..=200).rev().chain(1..=200) {
            let alone = successor_returns(&source_id(start), next_of(end));
            let ask = next_of(end);
            let got = walk.returns(&source_id(start), |id| {
                steps += 1;
                ask(id)
            });
            assert_eq!(got, alone, "from {start}, ending in {end:?}");
        }
        assert_eq!(
            steps, 200,
            "each source is asked about once, ending in {end:?}"
        );
    }
    // A source that cannot be read ends the walk with the reason, and what was learned before it still holds.
    let mut walk = SuccessorWalk::default();
    let broken = walk.returns(&source_id(1), |id| {
        if *id == source_id(1) {
            Ok(Some(source_id(2)))
        } else {
            Err("unreadable")
        }
    });
    assert_eq!(broken, Err("unreadable"));
    assert_eq!(
        walk.returns(&source_id(2), |_| Ok::<_, &str>(None)),
        Ok(false)
    );
}

#[test]
fn an_anchor_must_name_a_page_and_a_block_the_result_has_once() {
    let source = source_file(&Source::new(source_id(1)));
    let check = |page_id: &str, block: &str, on: &Mem| {
        let mut claim = sound_claim(1);
        claim.expressions = vec![Expression::new(page_id, block).expect("an expression")];
        // About the anchor alone: a new link would be judged on a wording too (K25), and there is none to judge on.
        claim.sources.clear();
        let path = claim_path(&claim.id);
        let files = files_of(&[claim_file(&claim), source.clone()]);
        rules(&validate(&files, &touched(&[&path]), on, on))
    };
    let path = claim_path(&claim_id(1));
    assert!(check("p1", "b1", &pages()).is_empty());
    assert!(check("p2", "b1", &pages()).len() == 1, "p2 has no b1");
    assert_eq!(
        check("p1", "gone", &pages()),
        [(path.clone(), Rule::Anchor)]
    );
    assert_eq!(
        check("nope", "b1", &pages()),
        [(path.clone(), Rule::Anchor)]
    );
    // Two pages with one id: the anchor does not say which.
    let twice = Mem(vec![
        ("pages/A.md".to_owned(), page("p1", "x ^b1")),
        ("pages/B.md".to_owned(), page("p1", "y ^b1")),
    ]);
    assert_eq!(check("p1", "b1", &twice), [(path.clone(), Rule::Anchor)]);
    // Two blocks with one id in the page.
    let repeated = Mem(vec![(
        "pages/A.md".to_owned(),
        page("p1", "x ^b1\n\ny ^b1"),
    )]);
    assert_eq!(check("p1", "b1", &repeated), [(path, Rule::Anchor)]);
}

#[test]
fn a_page_change_that_takes_out_a_block_a_claim_stands_on_is_refused_naming_the_claim() {
    let claim = sound_claim(1);
    let path = claim_path(&claim.id);
    let files = files_of(&[claim_file(&claim), source_file(&Source::new(source_id(1)))]);
    // The change writes a page only: p1 without its b1.
    let after = Mem(vec![
        ("pages/One.md".to_owned(), page("p1", "Said, with no id.")),
        ("pages/Two.md".to_owned(), page("p2", "Plain.")),
    ]);
    let t = Touched {
        page_ids: BTreeSet::from(["p1".to_owned()]),
        ..Touched::default()
    };
    let got = validate(&files, &t, &after, &pages());
    assert_eq!(rules(&got), [(path, Rule::Anchor)]);
    // A change to another page leaves it alone.
    let t = Touched {
        page_ids: BTreeSet::from(["p2".to_owned()]),
        ..Touched::default()
    };
    assert!(validate(&files, &t, &after, &pages()).is_empty());
    // Rewording the block, keeping its id, is fine here (the judgement turns *earlier*, a reviewer's matter).
    let reworded = Mem(vec![(
        "pages/One.md".to_owned(),
        page("p1", "Said again. ^b1"),
    )]);
    let t = Touched {
        page_ids: BTreeSet::from(["p1".to_owned()]),
        ..Touched::default()
    };
    assert!(validate(&files, &t, &reworded, &pages()).is_empty());
}

#[test]
fn a_claim_that_was_already_broken_does_not_hold_up_an_edit_of_its_page() {
    let mut claim = sound_claim(1);
    claim.expressions = vec![Expression::new("p1", "never-there").expect("an expression")];
    let files = files_of(&[claim_file(&claim), source_file(&Source::new(source_id(1)))]);
    let t = Touched {
        page_ids: BTreeSet::from(["p1".to_owned()]),
        ..Touched::default()
    };
    // The page is edited and the claim was dangling before as it is after: not this change's doing.
    assert!(validate(&files, &t, &pages(), &pages()).is_empty());
}

#[test]
fn files_already_invalid_and_not_touched_are_left_to_the_health_list() {
    let bad = (claim_path(&claim_id(5)), "{{{ not yaml".to_owned());
    let claim = sound_claim(1);
    let files = files_of(&[
        claim_file(&claim),
        source_file(&Source::new(source_id(1))),
        bad,
    ]);
    let t = touched(&[&claim_path(&claim.id)]);
    assert!(validate(&files, &t, &pages(), &pages()).is_empty());
}

#[test]
fn a_file_removed_while_another_still_names_it_is_refused() {
    let claim = sound_claim(1);
    let source = source_path(&source_id(1));
    // The change removes the source; the claim stays and names it.
    let files = files_of(&[claim_file(&claim)]);
    let t = Touched {
        removed: BTreeSet::from([source.clone()]),
        ..Touched::default()
    };
    assert_eq!(
        rules(&validate(&files, &t, &pages(), &pages())),
        [(source, Rule::InUse)]
    );
    // Removing a claim another claim relates to.
    let mut other = sound_claim(2);
    other.relations = vec![Relation {
        kind: RelationType::Supersedes,
        claim: claim_id(1),
    }];
    let files = files_of(&[claim_file(&other), source_file(&Source::new(source_id(1)))]);
    let t = Touched {
        removed: BTreeSet::from([claim_path(&claim_id(1))]),
        ..Touched::default()
    };
    assert_eq!(
        rules(&validate(&files, &t, &pages(), &pages())),
        [(claim_path(&claim_id(1)), Rule::InUse)]
    );
    // Removing both together is sound: nothing is left naming either.
    let t = Touched {
        removed: BTreeSet::from([claim_path(&claim_id(1)), source_path(&source_id(1))]),
        ..Touched::default()
    };
    assert!(validate(&BTreeMap::new(), &t, &pages(), &pages()).is_empty());
}

#[test]
fn a_change_to_the_knowledge_layer_may_not_change_an_owned_page_key() {
    let claim = sound_claim(1);
    let path = claim_path(&claim.id);
    let files = files_of(&[claim_file(&claim), source_file(&Source::new(source_id(1)))]);
    let mut t = touched(&[&path]);
    t.owned.insert("pages/One.md".to_owned());
    assert_eq!(
        rules(&validate(&files, &t, &pages(), &pages())),
        [("pages/One.md".to_owned(), Rule::Owned)]
    );
    // A change that does not touch the layer is not this check's business (the page form has its own).
    let mut t = Touched::default();
    t.owned.insert("pages/One.md".to_owned());
    assert!(validate(&files, &t, &pages(), &pages()).is_empty());
}

#[test]
fn a_refusal_names_the_file_and_the_rule_and_never_what_the_file_said() {
    let secret = "SECRET-7f3a";
    // A path with a control character is escaped as the import escapes a name.
    let odd = format!("knowledge/claims/{secret}\u{7}\n.yaml");
    let files = files_of(&[(odd.clone(), format!("kind: {secret}\n"))]);
    let got = validate(&files, &touched(&[&odd]), &pages(), &pages());
    let shown: Vec<String> = got.iter().map(ToString::to_string).collect();
    assert_eq!(shown.len(), 1);
    let line = shown.first().expect("one");
    assert!(!line.contains('\u{7}') && !line.contains('\n'), "{line:?}");
    assert!(line.contains("\\u{7}") && line.contains("\\n"), "{line}");
    // The content is not in it, whatever the rule.
    let path = claim_path(&claim_id(1));
    for text in [
        format!("kind: {secret}\n"),
        format!("{secret}: x\n"),
        format!("kind: observation\nlifecycle: {secret}\n"),
    ] {
        let files = files_of(&[(path.clone(), text)]);
        for r in validate(&files, &touched(&[&path]), &pages(), &pages()) {
            let line = format!("{r} {r:?}");
            assert!(!line.contains(secret), "{line}");
        }
    }
}

#[test]
fn a_path_under_knowledge_that_is_neither_a_claim_nor_a_source_is_refused_when_written() {
    let path = "knowledge/notes.txt".to_owned();
    let files = files_of(&[(path.clone(), "hello".to_owned())]);
    let got = validate(&files, &touched(&[&path]), &pages(), &pages());
    assert_eq!(rules(&got), [(path, Rule::Path)]);
}
