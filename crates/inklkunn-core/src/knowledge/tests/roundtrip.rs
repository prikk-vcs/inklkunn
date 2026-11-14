//! Every field of both files round-trips: write the canonical text, read it back, get the same value; and the
//! text is stable.

use super::check::sound_claim;
use super::*;

/// Words that exercise the writer's quoting: quotes, `#`, `:`, a backslash, a line break, a tab, an apostrophe,
/// Japanese, a leading dash, YAML's own words.
const AWKWARD: [&str; 10] = [
    "He said \"yes\" # not a comment",
    "a lone \" quote # and more",
    "a: b, c: d",
    "back\\slash and 'apostrophe'",
    "two\nlines\tand a tab",
    "橋は一九五〇年に開通した。",
    "- not a list",
    "yes",
    "2026-10-31",
    "",
];

fn full_claim(n: u8) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.kind = StatementKind::Interpretation;
    c.lifecycle = Lifecycle::Adopted;
    c.community_status = CommunityStatus {
        status: Standing::Contested,
        proposal: Some("01930000-aaaa-7000-8000-000000000001".to_owned()),
        judged_on: Some(digest_of("status wording")),
    };
    c.expressions = vec![
        Expression::new("page-en", "b1").expect("an expression"),
        Expression::new("page-ja", "b-2_x").expect("an expression"),
    ];
    c.relations = vec![
        Relation {
            kind: RelationType::Refines,
            claim: claim_id(90),
        },
        Relation {
            kind: RelationType::Contradicts,
            claim: claim_id(91),
        },
    ];
    c.sources = vec![
        SourceLink {
            source: source_id(1),
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: Some("a note".to_owned()),
            judged_on: digest_of("first"),
        },
        SourceLink {
            source: source_id(2),
            relation: Link::Challenges,
            bearing: Bearing::Indirect,
            note: None,
            judged_on: digest_of("second"),
        },
    ];
    c.normalize();
    c
}

fn round_trips(c: &Claim) {
    let text = c.to_yaml();
    let back = Claim::parse(c.id.clone(), &text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(&back, c, "{text}");
    assert_eq!(back.to_yaml(), text, "the text is stable");
}

#[test]
fn a_claim_with_every_field_round_trips() {
    round_trips(&full_claim(1));
}

#[test]
fn an_empty_claim_round_trips() {
    round_trips(&Claim::new(claim_id(2)));
}

#[test]
fn every_word_of_every_claim_enumeration_round_trips() {
    for kind in StatementKind::ALL {
        for lifecycle in Lifecycle::ALL {
            let mut c = Claim::new(claim_id(3));
            c.kind = *kind;
            c.lifecycle = *lifecycle;
            round_trips(&c);
        }
    }
    for standing in Standing::ALL {
        let mut c = full_claim(4);
        c.community_status.status = *standing;
        round_trips(&c);
    }
    for relation in RelationType::ALL {
        let mut c = full_claim(5);
        c.relations = vec![Relation {
            kind: *relation,
            claim: claim_id(92),
        }];
        round_trips(&c);
    }
    for link in Link::ALL {
        for bearing in Bearing::ALL {
            let mut c = full_claim(6);
            for s in &mut c.sources {
                s.relation = *link;
                s.bearing = *bearing;
            }
            round_trips(&c);
        }
    }
}

#[test]
fn the_enumerations_are_the_ones_the_rfc_lists() {
    let words = |all: &[&str]| all.iter().map(ToString::to_string).collect::<Vec<_>>();
    let of = |f: &dyn Fn() -> Vec<&'static str>| f();
    assert_eq!(
        of(&|| StatementKind::ALL.iter().map(|w| w.word()).collect()),
        words(&[
            "observation",
            "interpretation",
            "hypothesis",
            "opinion",
            "decision",
            "policy",
            "question",
            "unknown"
        ])
    );
    assert_eq!(
        of(&|| Lifecycle::ALL.iter().map(|w| w.word()).collect()),
        words(&["proposed", "adopted", "retired", "unknown"])
    );
    assert_eq!(
        of(&|| Standing::ALL.iter().map(|w| w.word()).collect()),
        words(&["accepted", "contested", "unresolved", "unknown"])
    );
    assert_eq!(
        of(&|| RelationType::ALL.iter().map(|w| w.word()).collect()),
        words(&[
            "refines",
            "supersedes",
            "contradicts",
            "split-from",
            "merged-into"
        ])
    );
    assert_eq!(
        of(&|| Bearing::ALL.iter().map(|w| w.word()).collect()),
        words(&["direct", "partial", "indirect"])
    );
    assert_eq!(
        of(&|| SourceKind::ALL.iter().map(|w| w.word()).collect()),
        words(&[
            "primary",
            "secondary",
            "observational",
            "computational",
            "anecdotal",
            "other"
        ])
    );
    assert_eq!(
        of(&|| Availability::ALL.iter().map(|w| w.word()).collect()),
        words(&["available", "unavailable", "unknown"])
    );
}

#[test]
fn awkward_texts_survive_in_every_text_field() {
    for text in AWKWARD {
        let mut c = full_claim(7);
        for s in &mut c.sources {
            s.note = Some(text.to_owned());
        }
        round_trips(&c);

        let mut s = Source::new(source_id(7));
        s.identity = Identity {
            title: Some(text.to_owned()),
            author: Some(text.to_owned()),
            date: Some(text.to_owned()),
        };
        s.reference = Some(Reference {
            form: ReferenceForm::Citation,
            value: if text.is_empty() {
                "x".to_owned()
            } else {
                text.to_owned()
            },
        });
        source_round_trips(&s);
    }
}

fn full_source(n: u8) -> Source {
    let mut s = Source::new(source_id(n));
    s.kind = SourceKind::Observational;
    s.availability = Availability::Available;
    s.identity = Identity {
        title: Some("A survey of the river".to_owned()),
        author: Some("The river office".to_owned()),
        date: Some("spring 1951".to_owned()),
    };
    s.reference = Some(Reference {
        form: ReferenceForm::Doi,
        value: "10.1000/example".to_owned(),
    });
    s.snapshot = Some(Snapshot::Archive {
        address: "https://archive.example/copy".to_owned(),
        digest: digest_of("archived"),
    });
    s.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: Some("01930000-bbbb-7000-8000-000000000002".to_owned()),
    });
    s.superseded_by = Some(source_id(99));
    s
}

fn source_round_trips(s: &Source) {
    let text = s.to_yaml();
    let back = Source::parse(s.id.clone(), &text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(&back, s, "{text}");
    assert_eq!(back.to_yaml(), text, "the text is stable");
}

#[test]
fn a_source_with_every_field_round_trips() {
    source_round_trips(&full_source(1));
}

#[test]
fn an_empty_source_round_trips() {
    source_round_trips(&Source::new(source_id(2)));
}

#[test]
fn every_word_of_every_source_enumeration_and_both_snapshots_round_trip() {
    for kind in SourceKind::ALL {
        let mut s = full_source(3);
        s.kind = *kind;
        source_round_trips(&s);
    }
    for availability in Availability::ALL {
        let mut s = full_source(4);
        s.availability = *availability;
        source_round_trips(&s);
    }
    for form in ReferenceForm::ALL {
        let mut s = full_source(5);
        s.reference = Some(Reference {
            form: *form,
            value: "x".to_owned(),
        });
        source_round_trips(&s);
    }
    let mut s = full_source(6);
    s.snapshot = Some(Snapshot::Attachment {
        path: "attachments/copy.pdf".to_owned(),
    });
    source_round_trips(&s);
    s.snapshot = None;
    s.last_reviewed = Some(Reviewed {
        date: "2020-01-02".to_owned(),
        proposal: None,
    });
    source_round_trips(&s);
}

#[test]
fn the_lists_of_a_claim_have_one_order() {
    let a = full_claim(8);
    let mut b = a.clone();
    b.expressions.reverse();
    b.relations.reverse();
    b.sources.reverse();
    // The same claim, said in another order, is the same text and the same value once read.
    assert_eq!(b.to_yaml(), a.to_yaml());
    assert_eq!(Claim::parse(a.id.clone(), &b.to_yaml()).expect("read"), a);
}

#[test]
fn a_hand_written_claim_in_the_plain_subset_is_read() {
    let text = format!(
        "# a claim\nkind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\n\
         expressions:\n  - page_id: p1\n    block: b1\nsources:\n  - source: {}\n    relation: supports\n    \
         bearing: partial\n    judged_on: {}\n",
        source_id(1),
        digest_of("words")
    );
    let c = Claim::parse(claim_id(9), &text).expect("read");
    assert_eq!(c.kind, StatementKind::Observation);
    assert_eq!(c.sources.len(), 1);
    assert_eq!(c.sources.first().map(|s| s.bearing), Some(Bearing::Partial));
    assert!(c.relations.is_empty());
}

#[test]
fn ids_are_version_seven_uuids_and_only_those() {
    assert!(ClaimId::parse(ClaimId::new().as_str()).is_some());
    assert!(SourceId::parse(SourceId::new().as_str()).is_some());
    assert!(
        ClaimId::parse("01930000-0000-4000-8000-000000000001").is_none(),
        "version 4"
    );
    assert!(
        ClaimId::parse("01930000-0000-7000-c000-000000000001").is_none(),
        "variant"
    );
    assert!(ClaimId::parse("01930000-0000-7000-8000-00000000000G").is_none());
    assert!(
        ClaimId::parse("01930000-0000-7000-8000-00000000000A").is_none(),
        "upper case"
    );
    assert!(ClaimId::parse("../etc/passwd").is_none());
    assert!(ClaimId::parse("").is_none());
}

#[test]
fn files_are_read_from_their_path_and_bytes() {
    let c = full_claim(10);
    let path = claim_path(&c.id);
    assert_eq!(path, format!("knowledge/claims/{}.yaml", c.id));
    assert_eq!(
        read_file(&path, c.to_yaml().as_bytes()),
        Ok(File::Claim(Box::new(c.clone())))
    );
    let s = full_source(10);
    assert_eq!(
        read_file(&source_path(&s.id), s.to_yaml().as_bytes()),
        Ok(File::Source(Box::new(s)))
    );
    // A claim's text under a source's name is refused: the schemas differ.
    assert!(read_file(&source_path(&source_id(11)), c.to_yaml().as_bytes()).is_err());
}

// ------------------------------------------------------------------------------------ K14: direction characters

/// Every bidirectional control and format character the files refuse: U+202A - U+202E, U+2066 - U+2069, U+200E, U+200F
/// and the Arabic letter mark U+061C (K23).
const DIRECTION: [char; 12] = [
    '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}',
    '\u{2069}', '\u{200e}', '\u{200f}', '\u{061c}',
];

/// Their neighbours in the code space, which are text.
const NOT_DIRECTION: [char; 8] = [
    '\u{200b}', '\u{200c}', '\u{200d}', '\u{2029}', '\u{2065}', '\u{206a}', '\u{061b}', '\u{061d}',
];

#[test]
fn a_text_of_a_claim_or_source_file_with_a_direction_character_does_not_read() {
    for c in DIRECTION {
        let text = format!("fine {c} not fine");
        // A claim's note (the writer keeps what it is given; the reader is the gate).
        let mut claim = sound_claim(1);
        claim.sources.first_mut().expect("a link").note = Some(text.clone());
        let got = Claim::parse(claim.id.clone(), &claim.to_yaml());
        assert_eq!(
            got.map(|_| ()).map_err(|f| (f.rule, f.field)),
            Err((Rule::Value, "note")),
            "{c:?} in a note"
        );
        // Each text of a source.
        for field in ["title", "author", "date", "value"] {
            let mut source = Source::new(source_id(1));
            match field {
                "title" => source.identity.title = Some(text.clone()),
                "author" => source.identity.author = Some(text.clone()),
                "date" => source.identity.date = Some(text.clone()),
                _ => {
                    source.reference = Some(Reference {
                        form: ReferenceForm::Citation,
                        value: text.clone(),
                    });
                }
            }
            let got = Source::parse(source.id.clone(), &source.to_yaml());
            assert_eq!(
                got.map(|_| ()).map_err(|f| (f.rule, f.field)),
                Err((Rule::Value, field)),
                "{c:?} in {field}"
            );
        }
    }
    for c in NOT_DIRECTION {
        let mut source = Source::new(source_id(1));
        source.identity.title = Some(format!("a{c}b"));
        assert!(
            Source::parse(source.id.clone(), &source.to_yaml()).is_ok(),
            "{c:?}"
        );
    }
}

#[test]
fn a_form_refuses_a_direction_character_in_every_text_it_takes() {
    use crate::knowledge::compose::{Mistake, clean_reference, clean_text};
    for c in DIRECTION {
        let text = format!("a{c}b");
        assert_eq!(
            clean_text(&text, "title"),
            Err(Mistake::Direction("title")),
            "{c:?}"
        );
        assert_eq!(
            clean_reference(ReferenceForm::Citation, &text),
            Err(Mistake::Direction("reference")),
            "{c:?}"
        );
    }
    for c in NOT_DIRECTION {
        assert!(clean_text(&format!("a{c}b"), "title").is_ok(), "{c:?}");
    }
}
