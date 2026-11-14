//! What a revision's knowledge files do, in facts (RFC 005 part 2 D-3).

use std::collections::BTreeMap;

use super::check::sound_claim;
use super::*;
use crate::knowledge::summary::{
    Cite, ClaimChange, Item, Rejudgement, SourceChange, summarize, undescribed, undescribed_by,
};

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
                standing: Standing::Unknown,
                judged_on: None,
                lifecycle: claim.lifecycle,
                said: claim.expressions.clone(),
                relations: claim.relations.clone(),
                cites: vec![Cite {
                    source: source_id(1),
                    relation: Link::Supports,
                    bearing: Bearing::Direct
                }],
            },
            Item::SourceRecorded {
                source: source.id.clone(),
                kind: SourceKind::Primary,
                identity: Identity {
                    title: Some("A title".to_owned()),
                    author: None,
                    date: None,
                },
                reference: None,
                snapshot: false,
                reviewed: None,
                superseded_by: None,
                availability: Availability::Unknown,
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
    let [Item::ClaimChanged { claim, changes, .. }] = got.as_slice() else {
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
fn what_changes_in_a_source_is_said_with_what_it_changed_to() {
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
            SourceChange::Identity(Identity {
                title: Some("Another".to_owned()),
                author: None,
                date: None,
            }),
            SourceChange::Reference(Some(Reference {
                form: ReferenceForm::Citation,
                value: "Smith".to_owned(),
            })),
            SourceChange::Reviewed(Some("2026-11-04".to_owned())),
            SourceChange::Availability(Availability::Unavailable),
            SourceChange::Superseded(Some(source_id(2))),
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
                id: claim_id(4).to_string(),
                rule: Rule::Unknown
            },
        ]
    );
    assert!(!format!("{got:?}").contains("secret"));
}

// ------------------------------------------------------------------------------------ K5: trust never moves unseen

fn changes_of(before: &Claim, after: &Claim) -> Vec<ClaimChange> {
    let path = claim_path(&before.id);
    let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
    match summarize(&[(path, Some(after.to_yaml()))], &had(old)).as_slice() {
        [Item::ClaimChanged { changes, .. }] => changes.clone(),
        [] => Vec::new(),
        other => panic!("{other:?}"),
    }
}

fn decided_on(status: Standing, wording: &str) -> CommunityStatus {
    CommunityStatus {
        status,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of(wording)),
    }
}

#[test]
fn a_source_link_rebound_to_other_wording_is_a_line_of_its_own() {
    let before = sound_claim(1);
    let mut after = before.clone();
    after.sources.first_mut().expect("a link").judged_on = digest_of("the new wording");
    assert_eq!(
        changes_of(&before, &after),
        [ClaimChange::Rejudged {
            on: Rejudgement::Source(source_id(1)),
            to: Some(digest_of("the new wording")),
        }]
    );
}

#[test]
fn a_status_whose_wording_alone_changes_is_not_read_as_accepted_to_accepted() {
    let mut before = sound_claim(1);
    before.community_status = decided_on(Standing::Accepted, "old");
    let mut after = before.clone();
    after.community_status.judged_on = Some(digest_of("new"));
    // The word does not change, the wording does: one line, about the wording, and none about the word.
    assert_eq!(
        changes_of(&before, &after),
        [ClaimChange::Rejudged {
            on: Rejudgement::Standing,
            to: Some(digest_of("new")),
        }]
    );
    // The word and the wording both: two lines.
    let mut both = after.clone();
    both.community_status.status = Standing::Contested;
    assert_eq!(
        changes_of(&before, &both),
        [
            ClaimChange::Standing {
                from: Standing::Accepted,
                to: Standing::Contested
            },
            ClaimChange::Rejudged {
                on: Rejudgement::Standing,
                to: Some(digest_of("new")),
            },
        ]
    );
    // The word alone: the word.
    let mut word = before.clone();
    word.community_status.status = Standing::Unresolved;
    assert_eq!(
        changes_of(&before, &word),
        [ClaimChange::Standing {
            from: Standing::Accepted,
            to: Standing::Unresolved
        }]
    );
    // The wording taken away (un-deciding, K14) is said too.
    let mut cleared = before.clone();
    cleared.community_status = CommunityStatus {
        status: Standing::Unknown,
        proposal: None,
        judged_on: None,
    };
    assert_eq!(
        changes_of(&before, &cleared),
        [
            ClaimChange::Standing {
                from: Standing::Accepted,
                to: Standing::Unknown
            },
            ClaimChange::Rejudged {
                on: Rejudgement::Standing,
                to: None,
            },
        ]
    );
}

#[test]
fn the_places_a_claim_is_said_come_with_the_change_for_a_surface_to_find_the_wording_in() {
    let before = sound_claim(1);
    let mut after = before.clone();
    after.sources.first_mut().expect("a link").judged_on = digest_of("elsewhere");
    let path = claim_path(&before.id);
    let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
    let got = summarize(&[(path, Some(after.to_yaml()))], &had(old));
    let [Item::ClaimChanged { said, .. }] = got.as_slice() else {
        panic!("{got:?}");
    };
    assert_eq!(said, &after.expressions);
}

// -- every change kind is described: property-style over every field and over random pairs

/// A small deterministic generator (no dependency): xorshift64*.
pub(super) struct Dice(pub(super) u64);

impl Dice {
    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    pub(super) fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap_or(0)
    }
    pub(super) fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        *from.get(self.below(from.len())).expect("a non-empty pool")
    }
    pub(super) fn chance(&mut self) -> bool {
        self.below(2) == 0
    }
}

/// A claim with every field filled, drawn from small pools so that two draws often agree on a field.
fn random_claim(dice: &mut Dice) -> Claim {
    let mut c = Claim::new(claim_id(1));
    c.kind = dice.pick(StatementKind::ALL);
    c.lifecycle = dice.pick(Lifecycle::ALL);
    c.community_status = CommunityStatus {
        status: dice.pick(Standing::ALL),
        proposal: dice
            .chance()
            .then(|| "01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: dice
            .chance()
            .then(|| digest_of(dice.pick(&["w1", "w2", "w3"]))),
    };
    for block in ["b1", "b2", "b3"] {
        if dice.chance() {
            c.expressions
                .push(Expression::new(dice.pick(&["p1", "p2"]), block).expect("e"));
        }
    }
    for n in 2..5u8 {
        if dice.chance() {
            c.relations.push(Relation {
                kind: dice.pick(RelationType::ALL),
                claim: claim_id(n),
            });
        }
    }
    for n in 1..4u8 {
        if dice.chance() {
            c.sources.push(SourceLink {
                source: source_id(n),
                relation: dice.pick(Link::ALL),
                bearing: dice.pick(Bearing::ALL),
                note: dice.chance().then(|| dice.pick(&["n1", "n2"]).to_owned()),
                judged_on: digest_of(dice.pick(&["w1", "w2", "w3"])),
            });
        }
    }
    c.normalize();
    c
}

fn random_source(dice: &mut Dice) -> Source {
    let mut s = Source::new(source_id(1));
    s.kind = dice.pick(SourceKind::ALL);
    s.availability = dice.pick(Availability::ALL);
    s.identity = Identity {
        title: dice.chance().then(|| dice.pick(&["t1", "t2"]).to_owned()),
        author: dice.chance().then(|| dice.pick(&["a1", "a2"]).to_owned()),
        date: dice
            .chance()
            .then(|| dice.pick(&["2020", "2021"]).to_owned()),
    };
    s.reference = dice.chance().then(|| Reference {
        form: dice.pick(ReferenceForm::ALL),
        value: dice.pick(&["r1", "r2"]).to_owned(),
    });
    s.snapshot = match dice.below(3) {
        0 => None,
        1 => Some(Snapshot::Attachment {
            path: dice.pick(&["attachments/a", "attachments/b"]).to_owned(),
        }),
        _ => Some(Snapshot::Archive {
            address: dice.pick(&["x1", "x2"]).to_owned(),
            digest: digest_of(dice.pick(&["w1", "w2"])),
        }),
    };
    s.last_reviewed = dice.chance().then(|| Reviewed {
        date: dice.pick(&["2026-11-01", "2026-11-02"]).to_owned(),
        proposal: dice
            .chance()
            .then(|| "01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    s.superseded_by = dice.chance().then(|| source_id(dice.pick(&[2, 3])));
    s
}

/// The claim without what the service writes itself at acceptance.
fn unstamped_claim(c: &Claim) -> Claim {
    let mut c = c.clone();
    c.community_status.proposal = None;
    c
}

fn unstamped_source(s: &Source) -> Source {
    let mut s = s.clone();
    if let Some(r) = &mut s.last_reviewed {
        r.proposal = None;
    }
    s
}

#[test]
fn every_change_to_a_claim_has_a_line_and_nothing_that_is_no_change_has_one() {
    let mut dice = Dice(0x9e37_79b9_7f4a_7c15);
    let mut changed = 0;
    for _ in 0..4000 {
        let (before, after) = (random_claim(&mut dice), random_claim(&mut dice));
        let path = claim_path(&before.id);
        let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
        let files = [(path, Some(after.to_yaml()))];
        let items = summarize(&files, &had(old.clone()));
        let differs = unstamped_claim(&before) != unstamped_claim(&after);
        assert_eq!(
            !items.is_empty(),
            differs,
            "a line exactly when something changes\n{before:?}\n{after:?}"
        );
        assert!(
            undescribed(&files, &had(old)).is_empty(),
            "described\n{before:?}\n{after:?}"
        );
        changed += usize::from(differs);
    }
    assert!(
        changed > 3000,
        "the draws must exercise changes, not only no-ops"
    );
}

#[test]
fn every_change_to_a_source_has_a_line_and_nothing_that_is_no_change_has_one() {
    let mut dice = Dice(0x1234_5678_9abc_def1);
    let mut changed = 0;
    for _ in 0..4000 {
        let (before, after) = (random_source(&mut dice), random_source(&mut dice));
        let path = source_path(&before.id);
        let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
        let files = [(path, Some(after.to_yaml()))];
        let items = summarize(&files, &had(old.clone()));
        let differs = unstamped_source(&before) != unstamped_source(&after);
        assert_eq!(
            !items.is_empty(),
            differs,
            "a line exactly when something changes\n{before:?}\n{after:?}"
        );
        assert!(undescribed(&files, &had(old)).is_empty());
        changed += usize::from(differs);
    }
    assert!(changed > 3000);
}

#[test]
fn the_proposal_ids_the_service_writes_itself_are_no_change_a_person_made() {
    let mut before = sound_claim(1);
    before.community_status = decided_on(Standing::Accepted, "w");
    let mut after = before.clone();
    after.community_status.proposal = Some("01930000-0000-7000-8000-00000000bbbb".to_owned());
    assert!(changes_of(&before, &after).is_empty());
    let claim_old: BTreeMap<String, String> = [(claim_path(&before.id), before.to_yaml())].into();
    let claim_files = [(claim_path(&before.id), Some(after.to_yaml()))];
    assert!(undescribed(&claim_files, &had(claim_old)).is_empty());
    let mut source = Source::new(source_id(1));
    source.last_reviewed = Some(Reviewed {
        date: "2026-11-01".to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    let mut other = source.clone();
    other.last_reviewed.as_mut().expect("a review").proposal =
        Some("01930000-0000-7000-8000-00000000bbbb".to_owned());
    let path = source_path(&source.id);
    let old: BTreeMap<String, String> = [(path.clone(), source.to_yaml())].into();
    let files = [(path, Some(other.to_yaml()))];
    assert!(summarize(&files, &had(old.clone())).is_empty());
    // Not a change the service would have to send, and not one it refuses to send as undescribed.
    assert!(undescribed(&files, &had(old)).is_empty());
}

#[test]
fn a_change_no_line_describes_is_found_and_named_by_its_file() {
    // The describer that sees nothing stands for a field the summary does not know.
    let blind = |_: &[(String, Option<String>)], _: &dyn Fn(&str) -> Option<String>| Vec::new();
    let before = sound_claim(1);
    let mut after = before.clone();
    after.kind = StatementKind::Opinion;
    let path = claim_path(&before.id);
    let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
    let files = [(path.clone(), Some(after.to_yaml()))];
    assert_eq!(
        undescribed_by(&files, &had(old.clone()), &blind),
        std::slice::from_ref(&path)
    );
    // A file that changes nothing is not asked about, a new file and a removal are items of their own.
    let same = [(path.clone(), Some(before.to_yaml()))];
    assert!(undescribed_by(&same, &had(old.clone()), &blind).is_empty());
    assert!(undescribed_by(&files, &nothing, &blind).is_empty());
    assert!(undescribed_by(&[(path, None)], &had(old.clone()), &blind).is_empty());
    // The real describer sees it.
    assert!(undescribed(&files, &had(old)).is_empty());
}

#[test]
fn the_proposal_service_sends_no_change_its_page_would_not_describe() {
    use crate::proposal::Invalid;
    use crate::proposal::files::knowledge_verdict;
    let blind = |_: &[(String, Option<String>)], _: &dyn Fn(&str) -> Option<String>| Vec::new();
    let before = sound_claim(1);
    let mut after = before.clone();
    after.sources.first_mut().expect("a link").judged_on = digest_of("elsewhere");
    let path = claim_path(&before.id);
    let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
    let files = [(path, Some(after.to_yaml()))];
    let was = had(old);
    // A curation proposal is sent when the page describes it, and not when it would not.
    assert_eq!(knowledge_verdict(&files, &was, true, &summarize), Ok(()));
    assert_eq!(
        knowledge_verdict(&files, &was, true, &blind),
        Err(Invalid::KnowledgeNotShown)
    );
    // Outside a curation proposal the same change is curation's, and that is said first.
    assert_eq!(
        knowledge_verdict(&files, &was, false, &summarize),
        Err(Invalid::KnowledgeCurated)
    );
}

/// Every single-field change of a claim, by name: the property is over each of them from several starting points,
/// since two random draws almost never differ in one field only.
#[allow(clippy::type_complexity)]
fn claim_mutations() -> Vec<(&'static str, Box<dyn Fn(&mut Claim)>)> {
    let mut m: Vec<(&'static str, Box<dyn Fn(&mut Claim)>)> = Vec::new();
    m.push(("kind", Box::new(|c| c.kind = StatementKind::Policy)));
    m.push(("lifecycle", Box::new(|c| c.lifecycle = Lifecycle::Retired)));
    m.push((
        "status word",
        Box::new(|c| {
            c.community_status.status = if c.community_status.status == Standing::Contested {
                Standing::Accepted
            } else {
                Standing::Contested
            }
        }),
    ));
    m.push((
        "status wording",
        Box::new(|c| c.community_status.judged_on = Some(digest_of("a wording nobody had"))),
    ));
    m.push((
        "status wording cleared",
        Box::new(|c| c.community_status.judged_on = None),
    ));
    m.push((
        "expression added",
        Box::new(|c| {
            c.expressions.push(Expression::new("p9", "z9").expect("e"));
            c.normalize();
        }),
    ));
    m.push((
        "expression removed",
        Box::new(|c| {
            c.expressions.pop();
        }),
    ));
    m.push((
        "relation added",
        Box::new(|c| {
            c.relations.push(Relation {
                kind: RelationType::Refines,
                claim: claim_id(9),
            });
            c.normalize();
        }),
    ));
    m.push((
        "relation removed",
        Box::new(|c| {
            c.relations.pop();
        }),
    ));
    m.push((
        "relation retyped",
        Box::new(|c| {
            if let Some(r) = c.relations.first_mut() {
                r.kind = if r.kind == RelationType::Supersedes {
                    RelationType::Contradicts
                } else {
                    RelationType::Supersedes
                };
            }
        }),
    ));
    m.push((
        "source cited",
        Box::new(|c| {
            c.sources.push(SourceLink {
                source: source_id(9),
                relation: Link::Supports,
                bearing: Bearing::Direct,
                note: None,
                judged_on: digest_of("w"),
            });
            c.normalize();
        }),
    ));
    m.push((
        "source uncited",
        Box::new(|c| {
            c.sources.pop();
        }),
    ));
    m.push((
        "link relation",
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.relation = if l.relation == Link::Supports {
                    Link::Challenges
                } else {
                    Link::Supports
                };
            }
        }),
    ));
    m.push((
        "link bearing",
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.bearing = if l.bearing == Bearing::Partial {
                    Bearing::Indirect
                } else {
                    Bearing::Partial
                };
            }
        }),
    ));
    m.push((
        "link note",
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.note = Some("a note nobody wrote".to_owned());
            }
        }),
    ));
    m.push((
        "link wording",
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.judged_on = digest_of("a wording nobody had");
            }
        }),
    ));
    m
}

/// A claim with every list filled.
fn rich_claim(seed: u64) -> Claim {
    let mut dice = Dice(seed);
    let mut c = random_claim(&mut dice);
    c.expressions = vec![Expression::new("p1", "b1").expect("e")];
    c.relations = vec![Relation {
        kind: RelationType::SplitFrom,
        claim: claim_id(2),
    }];
    if c.sources.is_empty() {
        c.sources = sound_claim(1).sources;
    }
    c.community_status.judged_on = Some(digest_of("w1"));
    c
}

#[test]
fn each_single_field_change_of_a_claim_is_described_from_any_starting_point() {
    for seed in 1..40u64 {
        let before = rich_claim(seed * 7919);
        for (name, mutate) in claim_mutations() {
            let mut after = before.clone();
            mutate(&mut after);
            if unstamped_claim(&before) == unstamped_claim(&after) {
                continue;
            }
            let path = claim_path(&before.id);
            let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
            let files = [(path, Some(after.to_yaml()))];
            assert!(
                !summarize(&files, &had(old.clone())).is_empty(),
                "a change of `{name}` has no line\n{before:?}"
            );
            assert!(undescribed(&files, &had(old)).is_empty(), "{name}");
        }
    }
}

#[allow(clippy::type_complexity)]
fn source_mutations() -> Vec<(&'static str, Box<dyn Fn(&mut Source)>)> {
    let mut m: Vec<(&'static str, Box<dyn Fn(&mut Source)>)> = Vec::new();
    m.push((
        "kind",
        Box::new(|s| {
            s.kind = if s.kind == SourceKind::Primary {
                SourceKind::Secondary
            } else {
                SourceKind::Primary
            }
        }),
    ));
    m.push((
        "title",
        Box::new(|s| s.identity.title = Some("a title nobody wrote".to_owned())),
    ));
    m.push((
        "author",
        Box::new(|s| s.identity.author = Some("an author nobody wrote".to_owned())),
    ));
    m.push((
        "date",
        Box::new(|s| s.identity.date = Some("1066".to_owned())),
    ));
    m.push((
        "reference value",
        Box::new(|s| {
            s.reference = Some(Reference {
                form: ReferenceForm::Citation,
                value: "a citation nobody wrote".to_owned(),
            });
        }),
    ));
    m.push((
        "reference form",
        Box::new(|s| {
            s.reference = Some(Reference {
                form: ReferenceForm::Doi,
                value: "r1".to_owned(),
            });
        }),
    ));
    m.push((
        "snapshot attachment",
        Box::new(|s| {
            s.snapshot = Some(Snapshot::Attachment {
                path: "attachments/zzz".to_owned(),
            });
        }),
    ));
    m.push((
        "snapshot archive",
        Box::new(|s| {
            s.snapshot = Some(Snapshot::Archive {
                address: "zzz".to_owned(),
                digest: digest_of("zzz"),
            });
        }),
    ));
    m.push(("snapshot removed", Box::new(|s| s.snapshot = None)));
    m.push((
        "review day",
        Box::new(|s| {
            s.last_reviewed = Some(Reviewed {
                date: "1999-01-01".to_owned(),
                proposal: None,
            });
        }),
    ));
    m.push(("review removed", Box::new(|s| s.last_reviewed = None)));
    m.push((
        "availability",
        Box::new(|s| {
            s.availability = if s.availability == Availability::Available {
                Availability::Unavailable
            } else {
                Availability::Available
            }
        }),
    ));
    m.push((
        "superseded",
        Box::new(|s| s.superseded_by = Some(source_id(77))),
    ));
    m.push(("superseded removed", Box::new(|s| s.superseded_by = None)));
    m
}

#[test]
fn each_single_field_change_of_a_source_is_described_from_any_starting_point() {
    for seed in 1..40u64 {
        let before = random_source(&mut Dice(seed * 104_729));
        for (name, mutate) in source_mutations() {
            let mut after = before.clone();
            mutate(&mut after);
            if unstamped_source(&before) == unstamped_source(&after) {
                continue;
            }
            let path = source_path(&before.id);
            let old: BTreeMap<String, String> = [(path.clone(), before.to_yaml())].into();
            let files = [(path, Some(after.to_yaml()))];
            assert!(
                !summarize(&files, &had(old.clone())).is_empty(),
                "a change of `{name}` has no line\n{before:?}"
            );
            assert!(undescribed(&files, &had(old)).is_empty(), "{name}");
        }
    }
}
