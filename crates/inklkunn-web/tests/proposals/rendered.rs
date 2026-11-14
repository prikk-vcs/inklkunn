//! What a proposal's page says of the claims and sources it changes, **at the rendered layer** (handoff 005
//! corrections, second round, K15; RFC 005 part 1 H-1: trust never moves silently).
//!
//! `KnowledgeNotShown` asks whether the page would have *a* line for a change. These tests ask what the page *prints*:
//! for every single-field change of a claim or a source, from several starting points, the proposal's real page (the
//! router, the real renderer, the in-memory repository) differs from the page of the proposal that changes nothing, and
//! it names the claim by a paragraph it is said in or the source by its name. The generators are the ones of the core's
//! `knowledge/tests/summary.rs` (`random_claim`, `random_source`, `claim_mutations`, `source_mutations`), made to fit
//! this wiki: the claim is said in the paragraph `k1` of `Bridge`, the sources are three titled ones.

use inklkunn_core::knowledge::{
    Availability, Bearing, Claim, ClaimId, CommunityStatus, Digest, Expression, Identity,
    Lifecycle, Link, Reference, ReferenceForm, Relation, RelationType, Reviewed, Snapshot, Source,
    SourceId, SourceKind, SourceLink, Standing, StatementKind, block_digest, claim_path,
    source_path,
};

use super::claims::{BRIDGE, Scene, held_unjudged, open, scene};

/// A small deterministic generator (no dependency): xorshift64*.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap_or(0)
    }
    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        *from.get(self.below(from.len())).expect("a non-empty pool")
    }
    fn chance(&mut self) -> bool {
        self.below(2) == 0
    }
}

fn claim_id(n: u8) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-0000000000{n:02x}")).expect("a claim id")
}

fn source_id(n: u8) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-0000000000{n:02x}")).expect("a source id")
}

fn digest_of(text: &str) -> Digest {
    let body = format!("{text} ^x");
    block_digest(inklkunn_markup::blocks(&body).first().expect("a block"))
}

const PROPOSAL: &str = "01930000-0000-7000-8000-00000000aaaa";

/// The claim of the scene (it is said in `Bridge`'s paragraph `k1`), every list filled from small pools so that two
/// draws often agree on a field.
fn random_claim(dice: &mut Dice) -> Claim {
    let mut c = Claim::new(claim_id(1));
    c.kind = dice.pick(StatementKind::ALL);
    c.lifecycle = dice.pick(Lifecycle::ALL);
    c.community_status = CommunityStatus {
        status: dice.pick(Standing::ALL),
        proposal: dice.chance().then(|| PROPOSAL.to_owned()),
        judged_on: dice
            .chance()
            .then(|| digest_of(dice.pick(&["w1", "w2", "w3"]))),
    };
    c.expressions
        .push(Expression::new("bridge-page", "k1").expect("an expression"));
    for n in 2..5u8 {
        if dice.chance() {
            c.relations.push(Relation {
                kind: dice.pick(RelationType::ALL),
                claim: claim_id(n),
            });
        }
    }
    for n in 1..4u8 {
        if dice.chance() || n == 1 {
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

fn random_source(dice: &mut Dice, n: u8) -> Source {
    let mut s = Source::new(source_id(n));
    s.kind = dice.pick(SourceKind::ALL);
    s.availability = dice.pick(Availability::ALL);
    s.identity = Identity {
        title: Some(format!("Title {n}{}", dice.pick(&["a", "b"]))),
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
        proposal: dice.chance().then(|| PROPOSAL.to_owned()),
    });
    s.superseded_by = dice.chance().then(|| source_id(dice.pick(&[2, 3])));
    s
}

type ClaimMutation = (&'static str, Box<dyn Fn(&mut Claim)>);
type SourceMutation = (&'static str, Box<dyn Fn(&mut Source)>);

/// Every single-field change of a claim, by name; `new` says whether it also changes what a *new* claim's file says
/// and is printed (a link's note and the wording a link was judged on are not printed for a link a new claim makes).
fn claim_mutations() -> Vec<(ClaimMutation, bool)> {
    let mut m: Vec<(ClaimMutation, bool)> = Vec::new();
    let mut add =
        |name: &'static str, new: bool, f: Box<dyn Fn(&mut Claim)>| m.push(((name, f), new));
    add("kind", true, Box::new(|c| c.kind = StatementKind::Policy));
    add(
        "lifecycle",
        true,
        Box::new(|c| c.lifecycle = Lifecycle::Retired),
    );
    add(
        "status word",
        true,
        Box::new(|c| {
            c.community_status.status = if c.community_status.status == Standing::Contested {
                Standing::Accepted
            } else {
                Standing::Contested
            }
        }),
    );
    // Two wordings that no paragraph has read alike: the page says "a wording none of the paragraphs has".
    add(
        "status wording",
        false,
        Box::new(|c| c.community_status.judged_on = Some(digest_of("a wording nobody had"))),
    );
    add(
        "status wording cleared",
        true,
        Box::new(|c| c.community_status.judged_on = None),
    );
    add(
        "expression added",
        true,
        Box::new(|c| {
            c.expressions
                .push(Expression::new("bridge-page", "k9").expect("e"));
            c.normalize();
        }),
    );
    add(
        "expression removed",
        true,
        Box::new(|c| {
            c.expressions.pop();
        }),
    );
    add(
        "relation added",
        true,
        Box::new(|c| {
            c.relations.push(Relation {
                kind: RelationType::Refines,
                claim: claim_id(9),
            });
            c.normalize();
        }),
    );
    add(
        "relation removed",
        true,
        Box::new(|c| {
            c.relations.pop();
        }),
    );
    add(
        "relation retyped",
        true,
        Box::new(|c| {
            if let Some(r) = c.relations.first_mut() {
                r.kind = if r.kind == RelationType::Supersedes {
                    RelationType::Contradicts
                } else {
                    RelationType::Supersedes
                };
            }
        }),
    );
    add(
        "source cited",
        true,
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
    );
    add(
        "source uncited",
        true,
        Box::new(|c| {
            c.sources.pop();
        }),
    );
    add(
        "link relation",
        true,
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.relation = if l.relation == Link::Supports {
                    Link::Challenges
                } else {
                    Link::Supports
                };
            }
        }),
    );
    add(
        "link bearing",
        true,
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.bearing = if l.bearing == Bearing::Partial {
                    Bearing::Indirect
                } else {
                    Bearing::Partial
                };
            }
        }),
    );
    add(
        "link note",
        false,
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.note = Some("a note nobody wrote".to_owned());
            }
        }),
    );
    add(
        "link wording",
        false,
        Box::new(|c| {
            if let Some(l) = c.sources.first_mut() {
                l.judged_on = digest_of("a wording nobody had");
            }
        }),
    );
    m
}

/// Every single-field change of a source, by name (the kind of copy kept, an attachment or an archive, is not told;
/// that a copy is kept is, so the two copy changes below are between *none* and *one*).
fn source_mutations() -> Vec<SourceMutation> {
    let mut m: Vec<SourceMutation> = Vec::new();
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
            let value = s
                .reference
                .as_ref()
                .map_or_else(|| "r1".to_owned(), |r| r.value.clone());
            let form = if s.reference.as_ref().map(|r| r.form) == Some(ReferenceForm::Doi) {
                ReferenceForm::Isbn
            } else {
                ReferenceForm::Doi
            };
            s.reference = Some(Reference { form, value });
        }),
    ));
    m.push((
        "reference removed",
        Box::new(|s| {
            if s.reference.is_none() {
                s.reference = Some(Reference {
                    form: ReferenceForm::Url,
                    value: "u".to_owned(),
                });
            } else {
                s.reference = None;
            }
        }),
    ));
    m.push((
        "snapshot",
        Box::new(|s| {
            s.snapshot = match s.snapshot {
                None => Some(Snapshot::Attachment {
                    path: "attachments/zzz".to_owned(),
                }),
                Some(_) => None,
            }
        }),
    ));
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

/// The section of a proposal's page that lists what it does to claims and sources (the note under its heading too).
fn knowledge_section(body: &str) -> String {
    let start = "<section aria-labelledby=\"knowledge\">";
    let Some(from) = body.find(start) else {
        return String::new();
    };
    let rest = body.get(from..).unwrap_or_default();
    let to = rest.find("</section>").map_or(rest.len(), |at| at + 10);
    rest.get(..to).unwrap_or_default().to_owned()
}

/// The knowledge section of the page of a proposal that writes `files`, held for review on the wiki as it is now.
async fn section_of(s: &Scene, files: Vec<(String, Option<String>)>) -> String {
    let to = held_unjudged(s, files);
    knowledge_section(&open(s, &to, &s.writer).await.body())
}

/// The wiki the sources of the generators are in: three titled ones, and the claim's paragraph.
fn put_sources(s: &Scene) {
    for n in 1..4u8 {
        let mut source = Source::new(source_id(n));
        source.identity.title = Some(format!("Source number {n}"));
        s.w.fake.set(&source_path(&source.id), &source.to_yaml());
    }
}

#[tokio::test]
async fn each_single_field_change_of_a_claim_has_its_own_rendered_line_and_names_the_claim() {
    let s = scene();
    put_sources(&s);
    let path = claim_path(&claim_id(1));
    for seed in 1..=5u64 {
        let before = random_claim(&mut Dice(seed * 7919));
        s.w.fake.set(&path, &before.to_yaml());
        let unchanged = section_of(&s, vec![(path.clone(), Some(before.to_yaml()))]).await;
        assert!(unchanged.contains("they change nothing"), "{unchanged}");
        for ((name, mutate), _) in claim_mutations() {
            let mut after = before.clone();
            mutate(&mut after);
            if unstamped_claim(&before) == unstamped_claim(&after) {
                continue;
            }
            let changed = section_of(&s, vec![(path.clone(), Some(after.to_yaml()))]).await;
            assert_ne!(changed, unchanged, "`{name}` changes nothing on the page");
            assert!(
                changed.contains("<bdi>It opened in 1950.</bdi>"),
                "`{name}` does not name the claim by its paragraph: {changed}"
            );
            assert!(
                !changed.contains("they change nothing"),
                "{name}: {changed}"
            );
            assert!(
                !changed.contains('{'),
                "`{name}` leaves a placeholder: {changed}"
            );
        }
    }
}

#[tokio::test]
async fn each_field_of_a_new_claim_that_is_printed_changes_its_rendered_line() {
    let s = scene();
    put_sources(&s);
    // A claim the wiki does not have: its id is nowhere in the repository.
    let path = claim_path(&claim_id(60));
    for seed in 1..=5u64 {
        let mut before = random_claim(&mut Dice(seed * 104_729));
        before.id = claim_id(60);
        let first = section_of(&s, vec![(path.clone(), Some(before.to_yaml()))]).await;
        assert!(
            first.contains("Records the claim “<bdi>It opened in 1950.</bdi>”"),
            "{first}"
        );
        assert!(first.contains("Its community status: "), "{first}");
        for ((name, mutate), printed) in claim_mutations() {
            if !printed {
                continue;
            }
            let mut after = before.clone();
            mutate(&mut after);
            after.id = claim_id(60);
            if unstamped_claim(&before) == unstamped_claim(&after) {
                continue;
            }
            let changed = section_of(&s, vec![(path.clone(), Some(after.to_yaml()))]).await;
            assert_ne!(
                changed, first,
                "`{name}` of a new claim changes nothing on the page"
            );
            assert!(
                !changed.contains('{'),
                "`{name}` leaves a placeholder: {changed}"
            );
        }
    }
}

#[tokio::test]
async fn each_single_field_change_of_a_source_has_its_own_rendered_line_and_names_the_source() {
    let s = scene();
    put_sources(&s);
    for seed in 1..=5u64 {
        let before = random_source(&mut Dice(seed * 6007), 1);
        let path = source_path(&before.id);
        s.w.fake.set(&path, &before.to_yaml());
        let unchanged = section_of(&s, vec![(path.clone(), Some(before.to_yaml()))]).await;
        assert!(unchanged.contains("they change nothing"), "{unchanged}");
        let name_before = format!(
            "<cite><bdi>“{}”",
            before.identity.title.as_deref().unwrap_or_default()
        );
        for (name, mutate) in source_mutations() {
            let mut after = before.clone();
            mutate(&mut after);
            if unstamped_source(&before) == unstamped_source(&after) {
                continue;
            }
            let changed = section_of(&s, vec![(path.clone(), Some(after.to_yaml()))]).await;
            assert_ne!(changed, unchanged, "`{name}` changes nothing on the page");
            assert!(
                changed.contains(&name_before),
                "`{name}` does not name the source by its name ({name_before}): {changed}"
            );
            assert!(
                !changed.contains('{'),
                "`{name}` leaves a placeholder: {changed}"
            );
        }
    }
}

#[tokio::test]
async fn each_field_of_a_new_source_that_is_printed_changes_its_rendered_line() {
    let s = scene();
    put_sources(&s);
    let path = source_path(&source_id(61));
    for seed in 1..=5u64 {
        let mut before = random_source(&mut Dice(seed * 3571), 61);
        before.id = source_id(61);
        let first = section_of(&s, vec![(path.clone(), Some(before.to_yaml()))]).await;
        assert!(first.contains("Records the source <cite><bdi>"), "{first}");
        assert!(first.contains("Whether it can be had: "), "{first}");
        for (name, mutate) in source_mutations() {
            let mut after = before.clone();
            mutate(&mut after);
            after.id = source_id(61);
            if unstamped_source(&before) == unstamped_source(&after) {
                continue;
            }
            let changed = section_of(&s, vec![(path.clone(), Some(after.to_yaml()))]).await;
            assert_ne!(
                changed, first,
                "`{name}` of a new source changes nothing on the page"
            );
            assert!(
                !changed.contains('{'),
                "`{name}` leaves a placeholder: {changed}"
            );
        }
    }
}

#[tokio::test]
async fn the_standing_of_a_new_claim_and_the_availability_and_review_day_of_a_new_source_are_on_their_lines()
 {
    let s = scene();
    let mut claim = Claim::new(claim_id(62));
    claim
        .expressions
        .push(Expression::new("bridge-page", "k1").expect("an expression"));
    claim.kind = StatementKind::Observation;
    claim.community_status.status = Standing::Contested;
    let got = section_of(&s, vec![(claim_path(&claim.id), Some(claim.to_yaml()))]).await;
    assert!(
        got.contains("(an observation), said in 1 place. Its community status: contested."),
        "{got}"
    );
    claim.community_status.status = Standing::Accepted;
    let got = section_of(&s, vec![(claim_path(&claim.id), Some(claim.to_yaml()))]).await;
    assert!(got.contains("Its community status: accepted."), "{got}");

    let mut source = Source::new(source_id(63));
    source.identity.title = Some("A fresh source".to_owned());
    source.availability = Availability::Unavailable;
    source.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: None,
    });
    let got = section_of(&s, vec![(source_path(&source.id), Some(source.to_yaml()))]).await;
    assert!(
        got.contains("<cite><bdi>“A fresh source”</bdi></cite>"),
        "{got}"
    );
    assert!(
        got.contains("Whether it can be had: it cannot be had."),
        "{got}"
    );
    assert!(got.contains("Last checked on 2026-10-31."), "{got}");
}

#[tokio::test]
async fn a_changed_sources_new_identity_and_reference_are_on_its_lines() {
    let s = scene();
    let before = {
        let mut b = Source::new(source_id(64));
        b.identity.title = Some("The old title".to_owned());
        b.reference = Some(Reference {
            form: ReferenceForm::Url,
            value: "https://example.org/old".to_owned(),
        });
        b
    };
    let path = source_path(&before.id);
    s.w.fake.set(&path, &before.to_yaml());
    let with = |title: &str, reference: &str| {
        let mut a = before.clone();
        a.identity.title = Some(title.to_owned());
        a.reference = Some(Reference {
            form: ReferenceForm::Url,
            value: reference.to_owned(),
        });
        vec![(path.clone(), Some(a.to_yaml()))]
    };
    let one = section_of(&s, with("The first new title", "https://example.org/one")).await;
    let two = section_of(&s, with("The second new title", "https://example.org/two")).await;
    assert_ne!(one, two);
    // The source is known by the name the wiki has; what it becomes is on the lines under it.
    for (got, title, reference) in [
        (&one, "The first new title", "https://example.org/one"),
        (&two, "The second new title", "https://example.org/two"),
    ] {
        assert!(
            got.contains("Changes the source <cite><bdi>“The old title”</bdi></cite> [source <code>0193…0040</code>]:"),
            "{got}"
        );
        assert!(
            got.contains(&format!("becomes <cite><bdi>“{title}”</bdi></cite>.")),
            "{got}"
        );
        assert!(got.contains(&format!("<bdi>{reference}</bdi>.")), "{got}");
    }
}

#[tokio::test]
async fn two_flipped_links_are_told_apart_by_the_sources_they_name() {
    let s = scene();
    put_sources(&s);
    let path = claim_path(&claim_id(1));
    let mut before = Claim::new(claim_id(1));
    before
        .expressions
        .push(Expression::new("bridge-page", "k1").expect("an expression"));
    for n in [2u8, 3] {
        before.sources.push(SourceLink {
            source: source_id(n),
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            judged_on: digest_of("It opened in 1950."),
        });
    }
    before.normalize();
    s.w.fake.set(&path, &before.to_yaml());
    let flipped = |which: u8| {
        let mut after = before.clone();
        for link in &mut after.sources {
            if link.source == source_id(which) {
                link.relation = Link::Challenges;
            }
        }
        vec![(path.clone(), Some(after.to_yaml()))]
    };
    let two = section_of(&s, flipped(2)).await;
    let three = section_of(&s, flipped(3)).await;
    assert_ne!(two, three, "a flipped link says which source it is");
    assert!(
        two.contains("The source <cite><bdi>“Source number 2”</bdi></cite> [source <code>0193…0002</code>], which it cites, now challenges it directly."),
        "{two}"
    );
    assert!(
        three.contains("The source <cite><bdi>“Source number 3”</bdi></cite> [source <code>0193…0003</code>], which it cites, now challenges it directly."),
        "{three}"
    );
}

#[tokio::test]
async fn a_name_and_a_paragraph_written_to_mislead_stay_inside_their_own_boundary() {
    let s = scene();
    // The claim's paragraph is markup with a direction character, and a source's title is written to read like another
    // source's whole line.
    let hostile = BRIDGE.replace(
        "It opened in 1950. ^k1",
        "It <b>opened</b> \u{202e}in 1950. ^k1",
    );
    s.w.fake.set("pages/Bridge.md", &hostile);
    let crafted = "Real”</cite> (first-hand). Cites the source <cite>“Another";
    let mut source = Source::new(source_id(65));
    source.identity.title = Some(crafted.to_owned());
    s.w.fake.set(&source_path(&source.id), &source.to_yaml());
    let mut claim = Claim::new(claim_id(1));
    claim
        .expressions
        .push(Expression::new("bridge-page", "k1").expect("an expression"));
    claim.sources.push(SourceLink {
        source: source.id.clone(),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: digest_of("w"),
    });
    s.w.fake
        .set(&claim_path(&claim.id), &Claim::new(claim_id(1)).to_yaml());
    let got = section_of(&s, vec![(claim_path(&claim.id), Some(claim.to_yaml()))]).await;
    assert!(!got.contains("<b>opened</b>"), "{got}");
    assert!(
        got.contains("<bdi>It &lt;b&gt;opened&lt;/b&gt; \u{202e}in 1950.</bdi>"),
        "{got}"
    );
    assert!(!got.contains("</cite> (first-hand)"), "{got}");
    assert_eq!(got.matches("<cite>").count(), 1, "{got}");
    assert_eq!(got.matches("</cite>").count(), 1, "{got}");
}

#[tokio::test]
async fn a_relation_names_the_other_claim_and_a_replacement_names_the_other_source() {
    let s = scene();
    put_sources(&s);
    // Another claim the wiki has, said in the paragraph `k1` too (the scene's claim is the first).
    let mut other = Claim::new(claim_id(2));
    other
        .expressions
        .push(Expression::new("bridge-page", "k1").expect("an expression"));
    s.w.fake.set(&claim_path(&other.id), &other.to_yaml());
    let path = claim_path(&claim_id(1));
    let mut before = Claim::new(claim_id(1));
    before
        .expressions
        .push(Expression::new("bridge-page", "k1").expect("an expression"));
    s.w.fake.set(&path, &before.to_yaml());
    let mut after = before.clone();
    after.relations.push(Relation {
        kind: RelationType::Contradicts,
        claim: claim_id(2),
    });
    let got = section_of(&s, vec![(path.clone(), Some(after.to_yaml()))]).await;
    assert!(
        got.contains(
            "A relation to the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0002</code>] is added: is at odds with."
        ),
        "{got}"
    );
    // The same relation on a claim that is new.
    let mut fresh = after.clone();
    fresh.id = claim_id(66);
    let got = section_of(&s, vec![(claim_path(&fresh.id), Some(fresh.to_yaml()))]).await;
    assert!(
        got.contains(
            "It has a relation to the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0002</code>]: is at odds with."
        ),
        "{got}"
    );
    // A source that is replaced by another names it.
    let mut old = Source::new(source_id(67));
    old.identity.title = Some("The old one".to_owned());
    s.w.fake.set(&source_path(&old.id), &old.to_yaml());
    let mut replaced = old.clone();
    replaced.superseded_by = Some(source_id(2));
    let got = section_of(&s, vec![(source_path(&old.id), Some(replaced.to_yaml()))]).await;
    assert!(
        got.contains("It is now replaced by the source <cite><bdi>“Source number 2”</bdi></cite> [source <code>0193…0002</code>]."),
        "{got}"
    );
}

// ---------------------------------------------------------------------------------------------------------
// K24: no two subjects read alike

/// A claim said in `page`'s paragraph `block`, citing the sources `cites` (each supporting it directly).
fn claim_citing(n: u8, page: &str, block: &str, cites: &[u8]) -> Claim {
    let mut claim = Claim::new(claim_id(n));
    claim
        .expressions
        .push(Expression::new(page, block).expect("an expression"));
    for source in cites {
        claim.sources.push(SourceLink {
            source: source_id(*source),
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            judged_on: digest_of("w1"),
        });
    }
    claim.normalize();
    claim
}

/// The claim with the link to `source` turned to challenge it: the whole proposal, as the files it writes.
fn flipped(claim: &Claim, source: u8) -> Vec<(String, Option<String>)> {
    let mut after = claim.clone();
    for link in &mut after.sources {
        if link.source == source_id(source) {
            link.relation = Link::Challenges;
        }
    }
    vec![(claim_path(&claim.id), Some(after.to_yaml()))]
}

/// Whether the section tells the claim (or source) `n` by its identifier, abbreviated.
fn names_by_id(section: &str, kind: &str, id: &str) -> bool {
    section.contains(&format!("[{kind} <code>{id}</code>]"))
}

#[tokio::test]
async fn a_curation_that_flips_a_link_on_one_of_two_claims_said_in_one_paragraph_reads_differently_from_one_on_the_other()
 {
    let s = scene();
    put_sources(&s);
    let (a, b) = (
        claim_citing(1, "bridge-page", "k1", &[2]),
        claim_citing(2, "bridge-page", "k1", &[2]),
    );
    for claim in [&a, &b] {
        s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    }
    let on_a = section_of(&s, flipped(&a, 2)).await;
    let on_b = section_of(&s, flipped(&b, 2)).await;
    assert_ne!(on_a, on_b, "the two proposals read alike:\n{on_a}");
    assert!(
        names_by_id(&on_a, "claim", "0193…0001")
            && !on_a.contains("[claim <code>0193…0002</code>]"),
        "{on_a}"
    );
    assert!(
        names_by_id(&on_b, "claim", "0193…0002")
            && !on_b.contains("[claim <code>0193…0001</code>]"),
        "{on_b}"
    );
    // Both still say what the claim says.
    assert!(on_a.contains("<bdi>It opened in 1950.</bdi>"), "{on_a}");
}

#[tokio::test]
async fn two_paragraphs_that_share_their_first_eighty_characters_do_not_make_two_claims_read_alike()
{
    let s = scene();
    put_sources(&s);
    let common =
        "The bridge was opened to traffic in the spring of that year, after a long delay, ";
    assert!(common.chars().count() >= 80);
    let page = format!(
        "---\ntitle: Twins\nid: twins-page\n---\n\n{common}by the council. ^t1\n\n{common}by the guild. ^t2\n"
    );
    s.w.fake.set("pages/Twins.md", &page);
    let (a, b) = (
        claim_citing(3, "twins-page", "t1", &[2]),
        claim_citing(4, "twins-page", "t2", &[2]),
    );
    for claim in [&a, &b] {
        s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    }
    let on_a = section_of(&s, flipped(&a, 2)).await;
    let on_b = section_of(&s, flipped(&b, 2)).await;
    assert_ne!(on_a, on_b, "the two proposals read alike:\n{on_a}");
    assert!(names_by_id(&on_a, "claim", "0193…0003"), "{on_a}");
    assert!(names_by_id(&on_b, "claim", "0193…0004"), "{on_b}");
}

#[tokio::test]
async fn two_sources_of_one_name_are_told_apart_by_their_identifiers_and_a_source_of_its_own_name_is_not()
 {
    let s = scene();
    put_sources(&s);
    for n in [5u8, 6] {
        let mut source = Source::new(source_id(n));
        source.identity = Identity {
            title: Some("The same title".to_owned()),
            author: Some("A. Writer".to_owned()),
            date: Some("1962".to_owned()),
        };
        s.w.fake.set(&source_path(&source.id), &source.to_yaml());
    }
    let claim = claim_citing(1, "bridge-page", "k1", &[2, 5, 6]);
    s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    let on_five = section_of(&s, flipped(&claim, 5)).await;
    let on_six = section_of(&s, flipped(&claim, 6)).await;
    assert_ne!(on_five, on_six, "the two proposals read alike:\n{on_five}");
    assert!(names_by_id(&on_five, "source", "0193…0005"), "{on_five}");
    assert!(names_by_id(&on_six, "source", "0193…0006"), "{on_six}");
    // A source that has a name of its own is told by it and by its identifier too: the page cannot know what else the
    // wiki holds (K24).
    let on_two = section_of(&s, flipped(&claim, 2)).await;
    assert!(names_by_id(&on_two, "source", "0193…0002"), "{on_two}");
}

// ---------------------------------------------------------------------------------------------------------
// K27: what goes with a removal

#[tokio::test]
async fn a_removed_claim_names_its_status_and_each_link_and_each_of_them_changes_the_page() {
    let s = scene();
    put_sources(&s);
    let path = claim_path(&claim_id(1));
    let mut had = claim_citing(1, "bridge-page", "k1", &[2, 3]);
    had.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some(PROPOSAL.to_owned()),
        judged_on: Some(digest_of("It opened in 1950.")),
    };
    s.w.fake.set(&path, &had.to_yaml());
    let removal = || vec![(path.clone(), None)];
    let first = section_of(&s, removal()).await;
    for want in [
        "Removes the claim “<bdi>It opened in 1950.</bdi>” [claim <code>0193…0001</code>].",
        "Its community status goes with it: accepted.",
        "Its link to the source <cite><bdi>“Source number 2”</bdi></cite> [source <code>0193…0002</code>] goes with it: it supports the claim directly.",
        "Its link to the source <cite><bdi>“Source number 3”</bdi></cite> [source <code>0193…0003</code>] goes with it: it supports the claim directly.",
    ] {
        assert!(first.contains(want), "{want}\n{first}");
    }
    // Each thing that leaves is on the page: the page of a removal of a claim that differs in only that one reads
    // differently.
    type Change = (&'static str, Box<dyn Fn(&mut Claim)>);
    let changes: Vec<Change> = vec![
        (
            "status",
            Box::new(|c| c.community_status.status = Standing::Contested),
        ),
        (
            "a link's relation",
            Box::new(|c| {
                if let Some(l) = c.sources.first_mut() {
                    l.relation = Link::Challenges;
                }
            }),
        ),
        (
            "a link's bearing",
            Box::new(|c| {
                if let Some(l) = c.sources.first_mut() {
                    l.bearing = Bearing::Partial;
                }
            }),
        ),
        (
            "a link's source",
            Box::new(|c| {
                if let Some(l) = c.sources.first_mut() {
                    l.source = source_id(1);
                }
                c.normalize();
            }),
        ),
        (
            "a link fewer",
            Box::new(|c| {
                c.sources.pop();
            }),
        ),
        (
            "a link more",
            Box::new(|c| {
                c.sources.push(SourceLink {
                    source: source_id(1),
                    relation: Link::Supports,
                    bearing: Bearing::Direct,
                    note: None,
                    judged_on: digest_of("It opened in 1950."),
                });
                c.normalize();
            }),
        ),
    ];
    for (name, mutate) in changes {
        let mut other = had.clone();
        mutate(&mut other);
        s.w.fake.set(&path, &other.to_yaml());
        let changed = section_of(&s, removal()).await;
        assert_ne!(
            changed, first,
            "`{name}` does not change the removal's page"
        );
    }
}

#[tokio::test]
async fn a_removed_source_names_its_review_day_and_availability_and_each_of_them_changes_the_page()
{
    let s = scene();
    put_sources(&s);
    let mut had = Source::new(source_id(5));
    had.identity.title = Some("A source that goes".to_owned());
    had.availability = Availability::Unavailable;
    had.last_reviewed = Some(Reviewed {
        date: "2026-11-01".to_owned(),
        proposal: Some(PROPOSAL.to_owned()),
    });
    let path = source_path(&had.id);
    s.w.fake.set(&path, &had.to_yaml());
    let removal = || vec![(path.clone(), None)];
    let first = section_of(&s, removal()).await;
    for want in [
        "Removes the source <cite><bdi>“A source that goes”</bdi></cite> [source <code>0193…0005</code>].",
        "The day it was last checked goes with it: 2026-11-01.",
        "What is recorded of whether it can be had goes with it: it cannot be had.",
    ] {
        assert!(first.contains(want), "{want}\n{first}");
    }
    type Change = (&'static str, Box<dyn Fn(&mut Source)>);
    let changes: Vec<Change> = vec![
        (
            "the review day",
            Box::new(|s| {
                s.last_reviewed = Some(Reviewed {
                    date: "2026-11-02".to_owned(),
                    proposal: None,
                });
            }),
        ),
        ("no review", Box::new(|s| s.last_reviewed = None)),
        (
            "availability",
            Box::new(|s| s.availability = Availability::Available),
        ),
    ];
    for (name, mutate) in changes {
        let mut other = had.clone();
        mutate(&mut other);
        s.w.fake.set(&path, &other.to_yaml());
        let changed = section_of(&s, removal()).await;
        assert_ne!(
            changed, first,
            "`{name}` does not change the removal's page"
        );
    }
}
