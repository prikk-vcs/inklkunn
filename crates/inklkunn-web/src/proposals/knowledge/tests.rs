#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::knowledge::summary::{Cite, ClaimChange, Item, Rejudgement, SourceChange};
use inklkunn_core::knowledge::{
    Availability, Bearing, ClaimId, Digest, Expression, Family, Lifecycle, Link, Relation,
    RelationType, Rule, SourceId, SourceKind, Standing, StatementKind,
};

use super::*;
use crate::catalog::{EN, JA_DEF, Lang};

fn claim() -> ClaimId {
    ClaimId::parse("01930000-0000-7000-8000-000000000001").unwrap()
}

fn source() -> SourceId {
    SourceId::parse("01931111-0000-7000-8000-000000000001").unwrap()
}

fn english(items: &[Item]) -> Vec<(String, Vec<String>)> {
    lines(T::new(EN), items, &|_, _| None)
        .into_iter()
        .map(|l| (l.text, l.more))
        .collect()
}

#[test]
fn a_claim_that_is_made_says_its_kind_where_it_is_said_and_each_source_it_cites() {
    let item = |places: usize| Item::ClaimMade {
        claim: claim(),
        kind: StatementKind::Hypothesis,
        standing: Standing::Unknown,
        said: (0..places)
            .map(|n| Expression::new("p", &format!("b{n}")).unwrap())
            .collect(),
        cites: vec![
            Cite {
                source: source(),
                relation: Link::Supports,
                bearing: Bearing::Direct,
            },
            Cite {
                source: source(),
                relation: Link::Challenges,
                bearing: Bearing::Indirect,
            },
        ],
    };
    assert_eq!(
        english(&[item(1)]),
        [(
            "Records a claim (a hypothesis), said in 1 place.".to_owned(),
            vec![
                "Cites a source: it supports the claim directly.".to_owned(),
                "Cites a source: it challenges the claim by implication.".to_owned(),
            ]
        )]
    );
    assert_eq!(
        english(&[item(3)]).first().map(|(t, _)| t.as_str()),
        Some("Records a claim (a hypothesis), said in 3 places.")
    );
}

#[test]
fn every_change_to_a_claim_that_exists_is_said_in_its_own_words() {
    let item = Item::ClaimChanged {
        claim: claim(),
        said: Vec::new(),
        changes: vec![
            ClaimChange::Kind(StatementKind::Policy),
            ClaimChange::Lifecycle(Lifecycle::Retired),
            ClaimChange::Standing {
                from: Standing::Unknown,
                to: Standing::Contested,
            },
            ClaimChange::Said(Expression::new("p", "b").unwrap()),
            ClaimChange::Unsaid(Expression::new("p", "b").unwrap()),
            ClaimChange::Related(Relation {
                kind: RelationType::Contradicts,
                claim: claim(),
            }),
            ClaimChange::Unrelated(Relation {
                kind: RelationType::SplitFrom,
                claim: claim(),
            }),
            ClaimChange::Cited(Cite {
                source: source(),
                relation: Link::Supports,
                bearing: Bearing::Partial,
            }),
            ClaimChange::Uncited(source()),
            ClaimChange::Recited(Cite {
                source: source(),
                relation: Link::Challenges,
                bearing: Bearing::Direct,
            }),
            ClaimChange::Noted(source()),
        ],
    };
    let got = english(&[item]);
    let (text, more) = got.first().unwrap();
    assert_eq!(text, "Changes a claim:");
    assert_eq!(
        more,
        &[
            "The kind of statement becomes a policy.",
            "Where it stands in its life becomes: retired.",
            "Its community status changes from not decided to contested.",
            "It is said in one more paragraph.",
            "It is no longer said in a paragraph: the paragraph is removed, or the claim lets it go.",
            "A relation to another claim is added: is at odds with.",
            "A relation to another claim is removed: came out of.",
            "Cites a source: it supports the claim in part.",
            "It no longer cites a source.",
            "A source it cites now challenges it directly.",
            "The note on a source it cites changes.",
        ]
    );
}

#[test]
fn the_sources_a_proposal_records_and_changes_and_the_files_it_removes_are_said() {
    let items = [
        Item::SourceRecorded {
            source: source(),
            kind: SourceKind::Primary,
            title: Some("A title".to_owned()),
            checked: false,
        },
        Item::SourceRecorded {
            source: source(),
            kind: SourceKind::Other,
            title: None,
            checked: false,
        },
        Item::SourceChanged {
            source: source(),
            changes: vec![
                SourceChange::Kind(SourceKind::Computational),
                SourceChange::Identity,
                SourceChange::Reference,
                SourceChange::Snapshot,
                SourceChange::Reviewed(Some("2026-11-04".to_owned())),
                SourceChange::Reviewed(None),
                SourceChange::Availability(Availability::Unavailable),
                SourceChange::Superseded,
            ],
        },
        Item::Removed {
            family: Family::Claim,
            id: claim().to_string(),
        },
        Item::Removed {
            family: Family::Source,
            id: source().to_string(),
        },
        Item::Unreadable {
            family: Family::Claim,
            rule: Rule::Unknown,
        },
    ];
    let got = english(&items);
    let texts: Vec<&str> = got.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Records a source (first-hand): A title",
            "Records a source (of another kind).",
            "Changes a source:",
            "Removes a claim.",
            "Removes a source.",
            "A file in this proposal is not in the form the wiki reads. The file has a key the format does not have.",
        ]
    );
    assert_eq!(
        got.get(2).map(|(_, m)| m.clone()).unwrap(),
        [
            "Its kind becomes the result of a computation.",
            "Who or what it is, as written, changes.",
            "Where it is found changes.",
            "The copy kept of it changes.",
            "Last checked on 2026-11-04.",
            "It is no longer marked as checked.",
            "Whether it can be had becomes: it cannot be had.",
            "It is now replaced by another source.",
        ]
    );
}

#[test]
fn a_file_that_does_not_read_is_said_by_its_rule_and_never_by_what_it_held() {
    let items = [Item::Unreadable {
        family: Family::Source,
        rule: Rule::Value,
    }];
    let line = lines(T::new(EN), &items, &|_, _| None).pop().unwrap();
    assert!(line.text.contains("outside the words"), "{}", line.text);
}

#[test]
fn a_file_that_cannot_be_read_and_is_replaced_has_a_line_of_its_own_in_both_languages() {
    let items = [
        Item::UnreadableReplaced {
            family: Family::Claim,
            id: claim().to_string(),
        },
        Item::UnreadableReplaced {
            family: Family::Source,
            id: source().to_string(),
        },
    ];
    let got = english(&items);
    assert_eq!(
        got.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
        [
            "Replaces or removes a claim file that cannot be read (01930000-0000-7000-8000-000000000001). What it held is not known.",
            "Replaces or removes a source file that cannot be read (01931111-0000-7000-8000-000000000001). What it held is not known.",
        ]
    );
    for line in lines(T::new(Lang::from_def(&JA_DEF)), &items, &|_, _| None) {
        assert!(!line.text.contains("[know."), "{}", line.text);
        assert!(line.text.contains("読み取れない"), "{}", line.text);
    }
}

#[test]
fn the_japanese_preview_has_every_one_and_none_is_a_marker_for_a_missing_message() {
    let t = T::new(Lang::from_def(&JA_DEF));
    let items = [
        Item::ClaimMade {
            claim: claim(),
            kind: StatementKind::Opinion,
            standing: Standing::Unknown,
            said: vec![Expression::new("p", "b").unwrap()],
            cites: vec![Cite {
                source: source(),
                relation: Link::Supports,
                bearing: Bearing::Direct,
            }],
        },
        Item::ClaimChanged {
            claim: claim(),
            said: Vec::new(),
            changes: vec![ClaimChange::Standing {
                from: Standing::Accepted,
                to: Standing::Unresolved,
            }],
        },
    ];
    for line in lines(t, &items, &|_, _| None) {
        for text in std::iter::once(line.text).chain(line.more) {
            assert!(!text.contains("[know."), "{text}");
            assert!(!text.is_empty());
        }
    }
}

// ------------------------------------------------------------------------------------ K5: re-judgements

fn rejudged(changes: Vec<ClaimChange>) -> Item {
    Item::ClaimChanged {
        claim: claim(),
        said: vec![Expression::new("p", "b").unwrap()],
        changes,
    }
}

fn digest(n: u8) -> Digest {
    Digest::parse(&format!("{n:02x}").repeat(32)).unwrap()
}

fn said_with(
    items: &[Item],
    wording: &dyn Fn(&[Expression], &Digest) -> Option<String>,
) -> Vec<String> {
    lines(T::new(EN), items, wording)
        .into_iter()
        .flat_map(|l| l.more)
        .collect()
}

#[test]
fn a_re_judgement_is_a_line_of_its_own_with_the_wording_it_moved_to() {
    let items = [rejudged(vec![
        ClaimChange::Rejudged {
            on: Rejudgement::Standing,
            to: Some(digest(1)),
        },
        ClaimChange::Rejudged {
            on: Rejudgement::Source(source()),
            to: Some(digest(2)),
        },
    ])];
    let wording = |said: &[Expression], d: &Digest| {
        assert_eq!(said, &[Expression::new("p", "b").unwrap()]);
        Some(format!("Words for {}.", d.as_str().get(..2).unwrap()))
    };
    assert_eq!(
        said_with(&items, &wording),
        [
            "What its community status was judged on becomes this wording: “Words for 01.”",
            "What a source it cites was judged on becomes this wording: “Words for 02.”",
        ]
    );
}

#[test]
fn a_wording_nobody_has_and_a_wording_taken_away_are_said_as_what_they_are() {
    let items = [rejudged(vec![
        ClaimChange::Rejudged {
            on: Rejudgement::Standing,
            to: Some(digest(1)),
        },
        ClaimChange::Rejudged {
            on: Rejudgement::Source(source()),
            to: None,
        },
    ])];
    assert_eq!(
        said_with(&items, &|_, _| None),
        [
            "What its community status was judged on becomes a wording that none of the claim's paragraphs has once this proposal is made.",
            "It is no longer recorded which wording a source it cites was judged on.",
        ]
    );
}

#[test]
fn the_wording_is_one_bounded_line() {
    let items = [rejudged(vec![ClaimChange::Rejudged {
        on: Rejudgement::Source(source()),
        to: Some(digest(3)),
    }])];
    // White space and line breaks collapse: two wordings that differ after a break must not read alike.
    let got = said_with(&items, &|_, _| {
        Some("First line.\n\n  Second   line.".to_owned())
    });
    assert_eq!(
        got,
        ["What a source it cites was judged on becomes this wording: “First line. Second line.”"]
    );
    // Long wording is cut, with a mark that it was.
    let long = "a".repeat(1000);
    let got = said_with(&items, &|_, _| Some(long.clone()));
    let line = got.first().unwrap();
    assert!(line.ends_with("…”"), "{line}");
    assert!(line.chars().count() < 340, "{}", line.chars().count());
}

#[test]
fn the_japanese_preview_has_every_re_judgement_line() {
    let t = T::new(Lang::from_def(&JA_DEF));
    let items = [rejudged(vec![
        ClaimChange::Rejudged {
            on: Rejudgement::Standing,
            to: Some(digest(1)),
        },
        ClaimChange::Rejudged {
            on: Rejudgement::Source(source()),
            to: Some(digest(2)),
        },
        ClaimChange::Rejudged {
            on: Rejudgement::Standing,
            to: None,
        },
    ])];
    let found = |_: &[Expression], d: &Digest| (*d == digest(1)).then(|| "文面。".to_owned());
    let got: Vec<String> = lines(t, &items, &found)
        .into_iter()
        .flat_map(|l| l.more)
        .collect();
    assert_eq!(got.len(), 3);
    assert!(got.first().unwrap().contains("「文面。」"), "{got:?}");
    for text in &got {
        assert!(!text.contains("[know."), "{text}");
    }
}
