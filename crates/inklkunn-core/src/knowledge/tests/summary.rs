//! What a revision's knowledge files do, in facts (RFC 005 part 2 D-3).

use std::collections::BTreeMap;

use super::check::sound_claim;
use super::*;
use crate::knowledge::summary::{Cite, ClaimChange, Item, SourceChange, summarize};

fn nothing(_: &str) -> Option<String> {
    None
}

fn had(map: BTreeMap<String, String>) -> impl Fn(&str) -> Option<String> {
    move |path| map.get(path).cloned()
}

#[test]
fn a_new_claim_and_a_new_source_are_made_and_recorded() {
    let claim = sound_claim(1);
    let mut source = Source::new(source_id(1));
    source.kind = SourceKind::Primary;
    source.identity.title = Some("A title".to_owned());
    let files = vec![
        (source_path(&source.id), Some(source.to_yaml())),
        (claim_path(&claim.id), Some(claim.to_yaml())),
        ("pages/One.md".to_owned(), Some("a page".to_owned())),
    ];
    // Claims first (path order), the page is not the layer's.
    assert_eq!(
        summarize(&files, &nothing),
        [
            Item::ClaimMade {
                claim: claim.id.clone(),
                kind: StatementKind::Observation,
                said: claim.expressions.clone(),
                cites: vec![Cite {
                    source: source_id(1),
                    relation: Link::Supports,
                    bearing: Bearing::Direct
                }],
            },
            Item::SourceRecorded {
                source: source.id.clone(),
                kind: SourceKind::Primary,
                title: Some("A title".to_owned()),
            },
        ]
    );
}

#[test]
fn what_changes_in_a_claim_that_exists_is_said_change_by_change() {
    let before = sound_claim(1);
    let mut after = before.clone();
    after.kind = StatementKind::Opinion;
    after.lifecycle = Lifecycle::Adopted;
    after.community_status.status = Standing::Contested;
    after.expressions = vec![Expression::new("p2", "z").expect("e")];
    after.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(9),
    }];
    after.sources = vec![SourceLink {
        source: source_id(2),
        relation: Link::Challenges,
        bearing: Bearing::Partial,
        note: None,
        judged_on: digest_of("w"),
    }];
    let path = claim_path(&before.id);
    let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
    let got = summarize(&[(path, Some(after.to_yaml()))], &had(old));
    let [Item::ClaimChanged { claim, changes }] = got.as_slice() else {
        panic!("{got:?}");
    };
    assert_eq!(claim, &before.id);
    for expected in [
        ClaimChange::Kind(StatementKind::Opinion),
        ClaimChange::Lifecycle(Lifecycle::Adopted),
        ClaimChange::Standing {
            from: Standing::Unknown,
            to: Standing::Contested,
        },
        ClaimChange::Said(Expression::new("p2", "z").expect("e")),
        ClaimChange::Unsaid(Expression::new("p1", "b1").expect("e")),
        ClaimChange::Related(Relation {
            kind: RelationType::Refines,
            claim: claim_id(9),
        }),
        ClaimChange::Cited(Cite {
            source: source_id(2),
            relation: Link::Challenges,
            bearing: Bearing::Partial,
        }),
        ClaimChange::Uncited(source_id(1)),
    ] {
        assert!(changes.contains(&expected), "{expected:?} in {changes:?}");
    }
    assert_eq!(changes.len(), 8);
}

#[test]
fn a_recited_source_and_a_changed_note_are_not_a_new_citation() {
    let before = sound_claim(1);
    let mut after = before.clone();
    let link = after.sources.first_mut().expect("a link");
    link.bearing = Bearing::Indirect;
    link.note = Some("a note".to_owned());
    let path = claim_path(&before.id);
    let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
    let got = summarize(&[(path, Some(after.to_yaml()))], &had(old));
    let [Item::ClaimChanged { changes, .. }] = got.as_slice() else {
        panic!("{got:?}");
    };
    assert_eq!(
        changes,
        &[
            ClaimChange::Recited(Cite {
                source: source_id(1),
                relation: Link::Supports,
                bearing: Bearing::Indirect
            }),
            ClaimChange::Noted(source_id(1)),
        ]
    );
}

#[test]
fn a_file_that_says_what_the_tip_says_is_no_item() {
    let claim = sound_claim(1);
    let source = Source::new(source_id(1));
    let old: BTreeMap<String, String> = [
        (claim_path(&claim.id), claim.to_yaml()),
        (source_path(&source.id), source.to_yaml()),
    ]
    .into();
    let files = vec![
        (claim_path(&claim.id), Some(claim.to_yaml())),
        (source_path(&source.id), Some(source.to_yaml())),
    ];
    assert!(summarize(&files, &had(old)).is_empty());
}

#[test]
fn what_changes_in_a_source_is_said_without_what_it_changed_to_except_a_day_and_an_availability() {
    let before = Source::new(source_id(1));
    let mut after = before.clone();
    after.kind = SourceKind::Primary;
    after.identity.title = Some("Another".to_owned());
    after.reference = Some(Reference {
        form: ReferenceForm::Citation,
        value: "Smith".to_owned(),
    });
    after.last_reviewed = Some(Reviewed {
        date: "2026-11-04".to_owned(),
        proposal: None,
    });
    after.availability = Availability::Unavailable;
    after.superseded_by = Some(source_id(2));
    let path = source_path(&before.id);
    let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
    let got = summarize(&[(path, Some(after.to_yaml()))], &had(old));
    let [Item::SourceChanged { changes, .. }] = got.as_slice() else {
        panic!("{got:?}");
    };
    assert_eq!(
        changes,
        &[
            SourceChange::Kind(SourceKind::Primary),
            SourceChange::Identity,
            SourceChange::Reference,
            SourceChange::Reviewed(Some("2026-11-04".to_owned())),
            SourceChange::Availability(Availability::Unavailable),
            SourceChange::Superseded,
        ]
    );
}

#[test]
fn a_removal_is_said_by_the_id_and_a_file_that_does_not_read_by_its_rule_only() {
    let id = claim_id(3);
    let broken = claim_path(&claim_id(4));
    let files = vec![
        (claim_path(&id), None),
        (
            broken,
            Some("kind: observation\nsecret: words\n".to_owned()),
        ),
    ];
    let got = summarize(&files, &nothing);
    assert_eq!(
        got,
        [
            Item::Removed {
                family: Family::Claim,
                id: id.to_string()
            },
            Item::Unreadable {
                family: Family::Claim,
                rule: Rule::Unknown
            },
        ]
    );
    assert!(!format!("{got:?}").contains("secret"));
}
