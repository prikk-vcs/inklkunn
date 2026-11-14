#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::knowledge::summary::{Cite, ClaimChange, Item, Rejudgement, SourceChange};
use inklkunn_core::knowledge::{
    Availability, Bearing, ClaimId, Digest, Expression, Family, Identity, Lifecycle, Link,
    Reference, ReferenceForm, Relation, RelationType, Rule, SourceId, SourceKind, Standing,
    StatementKind,
};

use super::*;
use crate::catalog::{EN, JA_DEF, Lang};

fn claim() -> ClaimId {
    ClaimId::parse("01930000-0000-7000-8000-000000000001").unwrap()
}

fn other_claim() -> ClaimId {
    ClaimId::parse("01930000-0000-7000-8000-000000000002").unwrap()
}

fn source() -> SourceId {
    SourceId::parse("01931111-0000-7000-8000-000000000001").unwrap()
}

fn here(block: &str) -> Expression {
    Expression::new("p", block).unwrap()
}

type Wording = Box<dyn Fn(&[Expression], &Digest) -> Option<String>>;

fn at(lines: &[String], n: usize) -> &str {
    lines.get(n).map_or("", String::as_str)
}

/// What the lines can find out: the words of paragraphs, where claims are said, what sources call themselves.
#[derive(Default)]
struct Fake {
    paragraphs: Vec<(Expression, String)>,
    places: Vec<(ClaimId, Vec<Expression>)>,
    identities: Vec<(SourceId, Identity)>,
    wording: Option<Wording>,
}

impl Fake {
    fn with_paragraph(mut self, at: Expression, words: &str) -> Fake {
        self.paragraphs.push((at, words.to_owned()));
        self
    }
    fn with_identity(mut self, id: SourceId, title: &str) -> Fake {
        self.identities.push((
            id,
            Identity {
                title: Some(title.to_owned()),
                author: None,
                date: None,
            },
        ));
        self
    }
}

impl Fake {
    fn with_wording(
        mut self,
        f: impl Fn(&[Expression], &Digest) -> Option<String> + 'static,
    ) -> Fake {
        self.wording = Some(Box::new(f));
        self
    }
}

impl Subjects for Fake {
    fn wording(&self, said: &[Expression], digest: &Digest) -> Option<String> {
        self.wording.as_ref().and_then(|f| f(said, digest))
    }
    fn paragraph(&self, at: &Expression) -> Option<String> {
        self.paragraphs
            .iter()
            .find(|(e, _)| e == at)
            .map(|(_, w)| w.clone())
    }
    fn places(&self, claim: &ClaimId) -> Vec<Expression> {
        self.places
            .iter()
            .find(|(c, _)| c == claim)
            .map(|(_, p)| p.clone())
            .unwrap_or_default()
    }
    fn identity(&self, source: &SourceId) -> Option<Identity> {
        self.identities
            .iter()
            .find(|(s, _)| s == source)
            .map(|(_, i)| i.clone())
    }
}

fn texts(know: &Fake, items: &[Item]) -> Vec<(String, Vec<String>)> {
    lines(T::new(EN), items, know)
        .into_iter()
        .map(|l| {
            (
                l.text.as_str().to_owned(),
                l.more.iter().map(|m| m.as_str().to_owned()).collect(),
            )
        })
        .collect()
}

fn english(items: &[Item]) -> Vec<(String, Vec<String>)> {
    texts(&Fake::default(), items)
}

fn made(places: usize) -> Item {
    Item::ClaimMade {
        claim: claim(),
        kind: StatementKind::Hypothesis,
        standing: Standing::Unknown,
        judged_on: None,
        lifecycle: Lifecycle::Proposed,
        said: (0..places).map(|n| here(&format!("b{n}"))).collect(),
        relations: Vec::new(),
        cites: Vec::new(),
    }
}

// ------------------------------------------------------------------------------------ K15: every line names its subject

#[test]
fn a_claim_that_is_made_is_named_by_a_paragraph_and_says_its_kind_standing_life_places_and_sources()
{
    let know = Fake::default()
        .with_paragraph(here("b0"), "It opened in 1950.")
        .with_identity(source(), "A history of the bridge");
    let item = Item::ClaimMade {
        claim: claim(),
        kind: StatementKind::Hypothesis,
        standing: Standing::Contested,
        judged_on: None,
        lifecycle: Lifecycle::Adopted,
        said: vec![here("b0"), here("b1")],
        relations: vec![Relation {
            kind: RelationType::Contradicts,
            claim: other_claim(),
        }],
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
        texts(&know, &[item]),
        [(
            "Records the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0001</code>] (a hypothesis), said in 2 places. Its community status: contested."
                .to_owned(),
            vec![
                "Where it stands in its life: taken up.".to_owned(),
                format!(
                    "It has a relation to the claim “<bdi>{}</bdi>”: is at odds with.",
                    other_claim().as_str()
                ),
                "Cites the source <cite><bdi>“A history of the bridge”</bdi></cite> [source <code>0193…0001</code>]: it supports the claim directly."
                    .to_owned(),
                "Cites the source <cite><bdi>“A history of the bridge”</bdi></cite> [source <code>0193…0001</code>]: it challenges the claim by implication."
                    .to_owned(),
            ]
        )]
    );
    // One place and many are told apart.
    let one = english(&[made(1)]);
    assert!(
        one.first()
            .is_some_and(|(t, _)| t.contains("said in 1 place. Its")),
        "{one:?}"
    );
}

#[test]
fn a_claim_said_nowhere_that_can_be_found_is_named_by_its_identifier() {
    let got = english(&[made(1)]);
    assert!(
        got.first().is_some_and(|(t, _)| t
            .starts_with("Records the claim “<bdi>01930000-0000-7000-8000-000000000001</bdi>”")),
        "{got:?}"
    );
}

#[test]
fn a_status_decided_on_a_wording_says_the_wording_on_a_new_claim() {
    let know = Fake::default().with_wording(|said, d| {
        assert_eq!(said, &[here("b0")]);
        Some(format!("Words for {}.", d.as_str().get(..2).unwrap()))
    });
    let item = Item::ClaimMade {
        claim: claim(),
        kind: StatementKind::Observation,
        standing: Standing::Accepted,
        judged_on: Some(digest(1)),
        lifecycle: Lifecycle::Proposed,
        said: vec![here("b0")],
        relations: Vec::new(),
        cites: Vec::new(),
    };
    let got = texts(&know, &[item]);
    let (text, more) = got.first().unwrap();
    assert!(text.ends_with("Its community status: accepted."), "{text}");
    assert!(
        more.contains(
            &"What its community status was judged on becomes this wording: “<bdi>Words for 01.</bdi>”"
                .to_owned()
        ),
        "{more:?}"
    );
}

#[test]
fn every_change_to_a_claim_that_exists_names_the_claim_and_what_it_is_about() {
    let know = Fake::default()
        .with_paragraph(here("b0"), "It opened in 1950.")
        .with_paragraph(here("old"), "A paragraph it let go.")
        .with_paragraph(here("new"), "A paragraph it gained.")
        .with_identity(source(), "A history");
    let mut know = know;
    know.places.push((other_claim(), vec![here("b0")]));
    let item = Item::ClaimChanged {
        claim: claim(),
        said: vec![here("b0")],
        was_said: vec![here("old")],
        changes: vec![
            ClaimChange::Kind(StatementKind::Policy),
            ClaimChange::Lifecycle(Lifecycle::Retired),
            ClaimChange::Standing {
                from: Standing::Unknown,
                to: Standing::Contested,
            },
            ClaimChange::Said(here("new")),
            ClaimChange::Unsaid(here("old")),
            ClaimChange::Related(Relation {
                kind: RelationType::Contradicts,
                claim: other_claim(),
            }),
            ClaimChange::Unrelated(Relation {
                kind: RelationType::SplitFrom,
                claim: other_claim(),
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
    let got = texts(&know, &[item]);
    let (text, more) = got.first().unwrap();
    // The other claim is said in the same paragraph: the two are told apart by their identifiers (K24).
    assert_eq!(
        text,
        "Changes the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0001</code>]:"
    );
    let named = "<cite><bdi>“A history”</bdi></cite> [source <code>0193…0001</code>]";
    assert_eq!(
        more,
        &[
            "The kind of statement becomes a policy.".to_owned(),
            "Where it stands in its life becomes: retired.".to_owned(),
            "Its community status changes from not decided to contested.".to_owned(),
            "It is said in one more paragraph: “<bdi>A paragraph it gained.</bdi>”.".to_owned(),
            "It is no longer said in the paragraph “<bdi>A paragraph it let go.</bdi>”: the paragraph is removed, or the claim lets it go."
                .to_owned(),
            "A relation to the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0002</code>] is added: is at odds with."
                .to_owned(),
            "A relation to the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0002</code>] is removed: came out of."
                .to_owned(),
            format!("Cites the source {named}: it supports the claim in part."),
            format!("It no longer cites the source {named}."),
            format!("The source {named}, which it cites, now challenges it directly."),
            format!("The note on the source {named}, which it cites, changes."),
        ]
    );
}

#[test]
fn a_claim_that_lets_go_of_its_last_paragraph_is_still_named_by_the_one_it_had() {
    let know = Fake::default().with_paragraph(here("old"), "The paragraph it had.");
    let item = Item::ClaimChanged {
        claim: claim(),
        said: Vec::new(),
        was_said: vec![here("old")],
        changes: vec![ClaimChange::Unsaid(here("old"))],
    };
    let got = texts(&know, &[item]);
    assert_eq!(
        got.first().map(|(t, _)| t.as_str()),
        Some(
            "Changes the claim “<bdi>The paragraph it had.</bdi>” [claim <code>0193…0001</code>]:"
        )
    );
}

#[test]
fn a_paragraph_that_cannot_be_found_is_named_by_its_marker() {
    let item = Item::ClaimChanged {
        claim: claim(),
        said: vec![here("b0")],
        was_said: Vec::new(),
        changes: vec![ClaimChange::Said(here("b0"))],
    };
    let got = english(&[item]);
    let (_, more) = got.first().unwrap();
    assert_eq!(
        more,
        &["It is said in one more paragraph: “<bdi>^b0</bdi>”."]
    );
}

fn titled(title: &str) -> Identity {
    Identity {
        title: Some(title.to_owned()),
        author: Some("A. Writer".to_owned()),
        date: Some("1962".to_owned()),
    }
}

#[test]
fn a_source_that_is_recorded_has_its_name_where_it_is_found_whether_it_can_be_had_and_the_day_it_was_checked()
 {
    let item = Item::SourceRecorded {
        source: source(),
        kind: SourceKind::Primary,
        identity: titled("A title"),
        reference: Some(Reference {
            form: ReferenceForm::Doi,
            value: "10.1000/182".to_owned(),
        }),
        snapshot: true,
        reviewed: Some("2026-11-04".to_owned()),
        superseded_by: Some(other_source()),
        availability: Availability::Unavailable,
    };
    let know = Fake::default().with_identity(other_source(), "The later source");
    let got = texts(&know, &[item]);
    let (text, more) = got.first().unwrap();
    assert_eq!(
        text,
        "Records the source <cite><bdi>“A title”, by A. Writer, dated 1962</bdi></cite> [source <code>0193…0001</code>] (first-hand)."
    );
    assert_eq!(more.len(), 5, "{more:?}");
    assert!(
        at(more, 0).starts_with("Where it is found (A DOI"),
        "{more:?}"
    );
    assert!(at(more, 0).ends_with("<bdi>10.1000/182</bdi>."), "{more:?}");
    assert_eq!(at(more, 1), "Whether it can be had: it cannot be had.");
    assert_eq!(at(more, 2), "Last checked on 2026-11-04.");
    assert_eq!(at(more, 3), "A copy of it is kept.");
    assert_eq!(
        at(more, 4),
        "It is now replaced by the source <cite><bdi>“The later source”</bdi></cite> [source <code>0193…0002</code>]."
    );
}

fn other_source() -> SourceId {
    SourceId::parse("01931111-0000-7000-8000-000000000002").unwrap()
}

#[test]
fn a_source_with_no_name_is_told_by_its_identifier_and_a_bare_one_says_it_is_not_known_if_it_can_be_had()
 {
    let item = Item::SourceRecorded {
        source: source(),
        kind: SourceKind::Other,
        identity: Identity::default(),
        reference: None,
        snapshot: false,
        reviewed: None,
        superseded_by: None,
        availability: Availability::Unknown,
    };
    let got = english(&[item]);
    let (text, more) = got.first().unwrap();
    assert_eq!(
        text,
        "Records the source <cite><bdi>A source without a title (identifier 01931111-0000-7000-8000-000000000001)</bdi></cite> (of another kind)."
    );
    assert_eq!(more, &["Whether it can be had: not known."]);
    // An author alone is a name too.
    let item = Item::SourceRecorded {
        source: source(),
        kind: SourceKind::Other,
        identity: Identity {
            title: None,
            author: Some("A. Writer".to_owned()),
            date: None,
        },
        reference: None,
        snapshot: false,
        reviewed: None,
        superseded_by: None,
        availability: Availability::Unknown,
    };
    assert!(english(&[item]).first().is_some_and(|(t, _)| {
        t.contains("<cite><bdi>A source without a title, by A. Writer</bdi></cite>")
    }),);
}

#[test]
fn a_changed_source_is_named_as_the_base_has_it_and_says_what_it_becomes() {
    let item = Item::SourceChanged {
        source: source(),
        named: titled("The old title"),
        changes: vec![
            SourceChange::Kind(SourceKind::Computational),
            SourceChange::Identity(titled("The new title")),
            SourceChange::Reference(Some(Reference {
                form: ReferenceForm::Url,
                value: "https://example.org/new".to_owned(),
            })),
            SourceChange::Reference(None),
            SourceChange::Snapshot,
            SourceChange::Reviewed(Some("2026-11-04".to_owned())),
            SourceChange::Reviewed(None),
            SourceChange::Availability(Availability::Unavailable),
            SourceChange::Superseded(Some(other_source())),
            SourceChange::Superseded(None),
        ],
    };
    let know = Fake::default().with_identity(other_source(), "The later source");
    let got = texts(&know, &[item]);
    let (text, more) = got.first().unwrap();
    assert_eq!(
        text,
        "Changes the source <cite><bdi>“The old title”, by A. Writer, dated 1962</bdi></cite> [source <code>0193…0001</code>]:"
    );
    assert_eq!(more.len(), 10, "{more:?}");
    assert_eq!(at(more, 0), "Its kind becomes the result of a computation.");
    assert_eq!(
        at(more, 1),
        "Who or what it is, as written, becomes <cite><bdi>“The new title”, by A. Writer, dated 1962</bdi></cite>."
    );
    assert!(
        at(more, 2).starts_with("Where it is found becomes (A web address"),
        "{more:?}"
    );
    assert!(
        at(more, 2).ends_with("<bdi>https://example.org/new</bdi>."),
        "{more:?}"
    );
    assert_eq!(at(more, 3), "It no longer says where it is found.");
    assert_eq!(at(more, 4), "The copy kept of it changes.");
    assert_eq!(at(more, 5), "Last checked on 2026-11-04.");
    assert_eq!(at(more, 6), "It is no longer marked as checked.");
    assert_eq!(
        at(more, 7),
        "Whether it can be had becomes: it cannot be had."
    );
    assert_eq!(
        at(more, 8),
        "It is now replaced by the source <cite><bdi>“The later source”</bdi></cite> [source <code>0193…0002</code>]."
    );
    assert_eq!(at(more, 9), "It is no longer marked as replaced.");
}

#[test]
fn removals_and_unreadable_files_are_named_by_what_they_were_and_never_by_what_they_held() {
    let know = Fake::default()
        .with_paragraph(here("b0"), "The claim that goes.")
        .with_identity(source(), "The source that goes");
    let mut know = know;
    know.places.push((claim(), vec![here("b0")]));
    let items = [
        Item::Removed {
            family: Family::Claim,
            id: claim().to_string(),
        },
        Item::Removed {
            family: Family::Source,
            id: source().to_string(),
        },
        Item::Removed {
            family: Family::Claim,
            id: "not-an-id".to_owned(),
        },
        Item::Unreadable {
            family: Family::Claim,
            id: claim().to_string(),
            rule: Rule::Unknown,
        },
        Item::UnreadableReplaced {
            family: Family::Claim,
            id: claim().to_string(),
        },
        Item::UnreadableReplaced {
            family: Family::Source,
            id: source().to_string(),
        },
    ];
    let got: Vec<String> = texts(&know, &items).into_iter().map(|(t, _)| t).collect();
    assert_eq!(
        got,
        [
            "Removes the claim “<bdi>The claim that goes.</bdi>” [claim <code>0193…0001</code>].",
            "Removes the source <cite><bdi>“The source that goes”</bdi></cite> [source <code>0193…0001</code>].",
            "Removes the claim “<bdi>not-an-id</bdi>”.",
            "The file <bdi>01930000-0000-7000-8000-000000000001</bdi> in this proposal is not in the form the wiki reads. The file has a key the format does not have.",
            "Replaces or removes a claim file that cannot be read (<bdi>01930000-0000-7000-8000-000000000001</bdi>). What it held is not known.",
            "Replaces or removes a source file that cannot be read (<bdi>01931111-0000-7000-8000-000000000001</bdi>). What it held is not known.",
        ]
    );
}

#[test]
fn the_words_of_a_paragraph_and_a_name_are_escaped_isolated_and_bounded() {
    let hostile = "<script>alert(1)</script> & \u{202e}evil \u{2067}";
    let long = "word ".repeat(100);
    let know = Fake::default()
        .with_paragraph(here("b0"), hostile)
        .with_paragraph(here("b1"), &long);
    // A claim named by a paragraph that holds markup and a direction character: escaped, and inside `<bdi>` so that it
    // cannot reorder the line around it.
    let item = Item::ClaimChanged {
        claim: claim(),
        said: vec![here("b0")],
        was_said: Vec::new(),
        changes: Vec::new(),
    };
    let got = texts(&know, &[item]);
    let text = &got.first().unwrap().0;
    assert!(!text.contains("<script>"), "{text}");
    assert!(text.contains("&lt;script&gt;"), "{text}");
    assert!(text.starts_with("Changes the claim “<bdi>"), "{text}");
    assert!(text.ends_with("</code>]:"), "{text}");
    // A long paragraph is cut, with a mark that it was.
    let item = Item::ClaimChanged {
        claim: claim(),
        said: vec![here("b1")],
        was_said: Vec::new(),
        changes: Vec::new(),
    };
    let text = texts(&know, &[item]).remove(0).0;
    assert!(text.contains("…</bdi>” [claim "), "{text}");
    assert!(text.chars().count() < 160, "{}", text.chars().count());
    // A source whose title is written to read like another source's whole line is one source's name, set apart.
    let crafted = "Real source”</cite> (first-hand). Cites the source <cite>“Another”";
    let item = Item::SourceChanged {
        source: source(),
        named: Identity {
            title: Some(crafted.to_owned()),
            author: None,
            date: None,
        },
        changes: vec![SourceChange::Snapshot],
    };
    let text = english(&[item]).remove(0).0;
    assert!(!text.contains("</cite> (first-hand)"), "{text}");
    assert!(text.starts_with("Changes the source <cite><bdi>"), "{text}");
    assert!(
        text.ends_with("</bdi></cite> [source <code>0193…0001</code>]:"),
        "{text}"
    );
    assert_eq!(text.matches("<cite>").count(), 1, "{text}");
    assert_eq!(text.matches("</cite>").count(), 1, "{text}");
}

#[test]
fn the_japanese_preview_has_every_line_and_none_is_a_marker_for_a_missing_message() {
    let t = T::new(Lang::from_def(&JA_DEF));
    let know = Fake::default()
        .with_paragraph(here("b0"), "橋は一九五〇年に開通した。")
        .with_identity(source(), "橋の歴史");
    let items = [
        Item::ClaimMade {
            claim: claim(),
            kind: StatementKind::Opinion,
            standing: Standing::Accepted,
            judged_on: Some(digest(1)),
            lifecycle: Lifecycle::Retired,
            said: vec![here("b0")],
            relations: vec![Relation {
                kind: RelationType::Refines,
                claim: other_claim(),
            }],
            cites: vec![Cite {
                source: source(),
                relation: Link::Supports,
                bearing: Bearing::Direct,
            }],
        },
        Item::ClaimChanged {
            claim: claim(),
            said: vec![here("b0")],
            was_said: Vec::new(),
            changes: vec![
                ClaimChange::Standing {
                    from: Standing::Accepted,
                    to: Standing::Unresolved,
                },
                ClaimChange::Said(here("b0")),
                ClaimChange::Unsaid(here("b0")),
                ClaimChange::Related(Relation {
                    kind: RelationType::Refines,
                    claim: other_claim(),
                }),
                ClaimChange::Unrelated(Relation {
                    kind: RelationType::Refines,
                    claim: other_claim(),
                }),
                ClaimChange::Uncited(source()),
                ClaimChange::Recited(Cite {
                    source: source(),
                    relation: Link::Challenges,
                    bearing: Bearing::Partial,
                }),
                ClaimChange::Noted(source()),
                ClaimChange::Rejudged {
                    on: Rejudgement::Source(source()),
                    to: None,
                },
            ],
        },
        Item::SourceRecorded {
            source: source(),
            kind: SourceKind::Primary,
            identity: titled("橋の歴史"),
            reference: Some(Reference {
                form: ReferenceForm::Isbn,
                value: "978".to_owned(),
            }),
            snapshot: true,
            reviewed: Some("2026-11-04".to_owned()),
            superseded_by: Some(source()),
            availability: Availability::Available,
        },
        Item::SourceChanged {
            source: source(),
            named: Identity::default(),
            changes: vec![
                SourceChange::Identity(titled("橋")),
                SourceChange::Reference(Some(Reference {
                    form: ReferenceForm::Page,
                    value: "12".to_owned(),
                })),
                SourceChange::Reference(None),
                SourceChange::Superseded(Some(source())),
                SourceChange::Superseded(None),
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
            family: Family::Source,
            id: source().to_string(),
            rule: Rule::Value,
        },
        Item::UnreadableReplaced {
            family: Family::Source,
            id: source().to_string(),
        },
    ];
    for line in lines(t, &items, &know) {
        for text in std::iter::once(line.text).chain(line.more) {
            let text = text.as_str();
            assert!(!text.contains("[know."), "{text}");
            assert!(!text.contains("[claim."), "{text}");
            assert!(!text.contains("[attach."), "{text}");
            assert!(!text.is_empty());
            assert!(!text.contains('{'), "an unfilled placeholder: {text}");
        }
    }
    let first = lines(t, &items, &know).remove(0);
    assert!(
        first
            .text
            .as_str()
            .contains("「<bdi>橋は一九五〇年に開通した。</bdi>」")
    );
}

#[test]
fn the_sources_a_proposal_records_and_the_files_it_removes_carry_no_verdict() {
    // No line says that a claim or a source is good, true or settled.
    let item = Item::SourceRecorded {
        source: source(),
        kind: SourceKind::Secondary,
        identity: titled("A title"),
        reference: None,
        snapshot: false,
        reviewed: None,
        superseded_by: None,
        availability: Availability::Available,
    };
    for (text, more) in english(&[item, made(1)]) {
        for word in [
            "true",
            "false",
            "verified",
            "proven",
            "canonical",
            "official",
        ] {
            assert!(!text.contains(word), "{text}");
            assert!(more.iter().all(|m| !m.contains(word)), "{more:?}");
        }
    }
}

// ------------------------------------------------------------------------------------ K5: re-judgements

fn rejudged(changes: Vec<ClaimChange>) -> Item {
    Item::ClaimChanged {
        claim: claim(),
        said: vec![here("b")],
        was_said: Vec::new(),
        changes,
    }
}

fn digest(n: u8) -> Digest {
    Digest::parse(&format!("{n:02x}").repeat(32)).unwrap()
}

fn said_with(
    items: &[Item],
    wording: impl Fn(&[Expression], &Digest) -> Option<String> + 'static,
) -> Vec<String> {
    let mut know = Fake::default().with_identity(source(), "S");
    know.wording = Some(Box::new(wording));
    texts(&know, items)
        .into_iter()
        .flat_map(|(_, more)| more)
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
        assert_eq!(said, &[here("b")]);
        Some(format!("Words for {}.", d.as_str().get(..2).unwrap()))
    };
    assert_eq!(
        said_with(&items, wording),
        [
            "What its community status was judged on becomes this wording: “<bdi>Words for 01.</bdi>”",
            "What the source <cite><bdi>“S”</bdi></cite> [source <code>0193…0001</code>] that it cites was judged on becomes this wording: “<bdi>Words for 02.</bdi>”",
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
        said_with(&items, |_, _| None),
        [
            "What its community status was judged on becomes a wording that none of the claim&#39;s paragraphs has once this proposal is made.",
            "It is no longer recorded which wording the source <cite><bdi>“S”</bdi></cite> [source <code>0193…0001</code>] that it cites was judged on.",
        ]
    );
}

#[test]
fn the_wording_is_one_bounded_line() {
    let items = [rejudged(vec![ClaimChange::Rejudged {
        on: Rejudgement::Standing,
        to: Some(digest(3)),
    }])];
    // White space and line breaks collapse: two wordings that differ after a break must not read alike.
    let got = said_with(&items, |_, _| {
        Some("First line.\n\n  Second   line.".to_owned())
    });
    assert_eq!(
        got,
        [
            "What its community status was judged on becomes this wording: “<bdi>First line. Second line.</bdi>”"
        ]
    );
    // Long wording is cut, with a mark that it was.
    let long = "a".repeat(1000);
    let got = said_with(&items, move |_, _| Some(long.clone()));
    let line = got.first().unwrap();
    assert!(line.ends_with("…</bdi>”"), "{line}");
    assert!(line.chars().count() < 380, "{}", line.chars().count());
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
    let mut know = Fake::default().with_identity(source(), "出典");
    know.wording = Some(Box::new(|_, d| {
        (*d == digest(1)).then(|| "文面。".to_owned())
    }));
    let got: Vec<String> = lines(t, &items, &know)
        .into_iter()
        .flat_map(|l| l.more)
        .map(|m| m.as_str().to_owned())
        .collect();
    assert_eq!(got.len(), 3);
    assert!(
        got.first().unwrap().contains("「<bdi>文面。</bdi>」"),
        "{got:?}"
    );
    for text in &got {
        assert!(!text.contains("[know."), "{text}");
    }
}

// ---------------------------------------------------------------------------------------------------------
// K24: no two subjects read alike

fn claim_n(n: u8) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-0000000000{n:02x}")).unwrap()
}

fn source_n(n: u8) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-0000000000{n:02x}")).unwrap()
}

/// A change to the claim `n`, which the words of paragraph `block` name.
fn flip(n: u8, block: &str) -> Item {
    Item::ClaimChanged {
        claim: claim_n(n),
        said: vec![here(block)],
        was_said: Vec::new(),
        changes: vec![ClaimChange::Lifecycle(Lifecycle::Retired)],
    }
}

fn heading(know: &Fake, item: Item) -> String {
    texts(know, &[item]).remove(0).0
}

#[test]
fn two_claims_said_in_one_paragraph_are_told_apart_by_their_identifiers() {
    let know = Fake::default().with_paragraph(here("b0"), "It opened in 1950.");
    let (a, b) = (heading(&know, flip(1, "b0")), heading(&know, flip(2, "b0")));
    assert_ne!(a, b);
    assert_eq!(
        a,
        "Changes the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0001</code>]:"
    );
    assert_eq!(
        b,
        "Changes the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0002</code>]:"
    );
}

#[test]
fn two_paragraphs_that_share_their_first_eighty_characters_are_told_apart_by_the_claims_identifiers()
 {
    let common =
        "It opened to traffic in the spring of that year, after a long delay and a hard winter, ";
    assert!(common.chars().count() > SUBJECT_CHARS);
    let know = Fake::default()
        .with_paragraph(here("b0"), &format!("{common}by the council."))
        .with_paragraph(here("b1"), &format!("{common}by the guild."));
    let (a, b) = (heading(&know, flip(1, "b0")), heading(&know, flip(2, "b1")));
    assert_ne!(a, b);
    assert!(a.contains("[claim <code>0193…0001</code>]"), "{a}");
    assert!(b.contains("[claim <code>0193…0002</code>]"), "{b}");
    // The same excerpt on both: the words alone do not tell.
    assert_eq!(a.split(" [claim ").next(), b.split(" [claim ").next());
}

#[test]
fn two_sources_of_one_name_are_told_apart_by_their_identifiers() {
    let mut know = Fake::default();
    for n in [1u8, 2] {
        know.identities.push((
            source_n(n),
            Identity {
                title: Some("A history".to_owned()),
                author: Some("A. Writer".to_owned()),
                date: Some("1962".to_owned()),
            },
        ));
    }
    let uncite = |n: u8| Item::ClaimChanged {
        claim: claim_n(9),
        said: Vec::new(),
        was_said: Vec::new(),
        changes: vec![ClaimChange::Uncited(source_n(n))],
    };
    let (a, b) = (texts(&know, &[uncite(1)]), texts(&know, &[uncite(2)]));
    assert_ne!(a, b);
    let line = |got: &[(String, Vec<String>)]| got.first().unwrap().1.first().unwrap().clone();
    assert_eq!(
        line(&a),
        "It no longer cites the source <cite><bdi>“A history”, by A. Writer, dated 1962</bdi></cite> [source <code>0193…0001</code>]."
    );
    assert!(
        line(&b).contains("[source <code>0193…0002</code>]"),
        "{}",
        line(&b)
    );
}

#[test]
fn a_subject_told_by_its_identifier_already_is_not_told_twice() {
    // A claim said nowhere that can be found, and a source that names itself in no way.
    let know = Fake::default();
    let got = texts(&know, &[flip(1, "nowhere")]);
    let text = &got.first().unwrap().0;
    assert_eq!(
        text,
        "Changes the claim “<bdi>01930000-0000-7000-8000-000000000001</bdi>”:"
    );
    let got = texts(
        &know,
        &[Item::Removed {
            family: Family::Source,
            id: source_n(1).to_string(),
        }],
    );
    assert!(
        !got.first().unwrap().0.contains("[source"),
        "{:?}",
        got.first()
    );
}

#[test]
fn identifiers_on_one_page_never_abbreviate_alike() {
    use super::names::abbreviations;
    use std::collections::BTreeSet;
    // Identifiers that agree in their first four and their last four characters: the abbreviation grows until they
    // differ, and is the whole identifier when that is what it takes.
    let mut ids = BTreeSet::new();
    for middle in ["a", "b", "c", "0", "1"] {
        for deep in ["7", "8"] {
            ids.insert(format!("01930000-0000-7000-{deep}000-0000000{middle}0001"));
        }
    }
    let forms = abbreviations(&ids);
    assert_eq!(forms.len(), ids.len());
    let distinct: BTreeSet<&String> = forms.values().collect();
    assert_eq!(distinct.len(), ids.len(), "{forms:?}");
    for (id, form) in &forms {
        assert!(form.chars().count() <= id.chars().count(), "{form}");
        assert!(form == id || form.starts_with("0193…"), "{form}");
    }
    // Where four characters of the end tell them apart, four are kept.
    let two: BTreeSet<String> = (1..=2u8).map(|n| claim_n(n).to_string()).collect();
    assert_eq!(
        abbreviations(&two).values().cloned().collect::<Vec<_>>(),
        ["0193…0001", "0193…0002"]
    );
    // Identifiers that differ only in their head cannot be told by a shortening of the end: they are told anyway.
    let heads: BTreeSet<String> = [
        "01930000-0000-7000-8000-000000000001",
        "01930001-0000-7000-8000-000000000001",
    ]
    .map(str::to_owned)
    .into();
    let forms = abbreviations(&heads);
    let distinct: BTreeSet<&String> = forms.values().collect();
    assert_eq!(distinct.len(), 2, "{forms:?}");
}
