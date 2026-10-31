//! RFC 005 part 2 D-4's acceptance criterion: the knowledge merge on a corpus of knowledge-file pairs is **clean
//! where no value is changed twice, an overlap with both texts otherwise, and never a lost relation**; and the
//! clean / overlap counts are committed and held here.
//!
//! The corpus has two parts (see `tests/fixtures/KNOWLEDGE-MERGE-CORPUS.md`):
//!
//! - **hand cases**: every ordered pair of 18 edits to one claim file (expressions, relations, a source link's
//!   keys, a link added or removed, the kind, the standing), 324 pairs, each with its verdict from an oracle written
//!   out here, apart from the merge;
//! - **pairs derived from spike C's replay facts** (`rfcs/handoffs/005-knowledge-layer/spikes/replay-counts.json`:
//!   counts and nothing else, per article, per language), as M0's spike B derived its pairs for pages. For each of
//!   the 24 articles, `edits / 9` pairs: two edits made from one baseline claim file, each edit drawn by a seeded
//!   generator with the weights the article's own counts give (`changed` events re-judge a source link, `replaced`
//!   events move an expression, `added` and `removed` add and remove one, a `revert` leaves the file as it was), onto
//!   the number of targets the article's surviving block ids give. The sentences in the files are invented here
//!   (a note per edit, English for the English articles and Japanese for the others); no article text, title or editor
//!   is read or kept, and the files name an article by its number only.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;

use inklkunn_core::knowledge::{
    Bearing, Claim, ClaimId, CommunityStatus, Digest, Expression, Lifecycle, Link, Relation,
    RelationType, SourceId, SourceLink, Standing, StatementKind, block_digest, claim_path,
};
use inklkunn_core::{MergeOutcome, PageText, Place, merge};
use serde_json::Value;

const COUNTS: &str =
    include_str!("../../../rfcs/handoffs/005-knowledge-layer/spikes/replay-counts.json");

/// The committed counts. A change to the merge that moves any of them is a decision, not a refactor.
const HAND: (usize, usize) = (HAND_CLEAN, HAND_OVERLAP);
const HAND_CLEAN: usize = 306;
const HAND_OVERLAP: usize = 18;
const DERIVED: (usize, usize) = (DERIVED_CLEAN, DERIVED_OVERLAP);
const DERIVED_CLEAN: usize = 209;
const DERIVED_OVERLAP: usize = 7;

// ------------------------------------------------------------------------------------------------ ids

fn claim_id(n: u64) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-{n:012x}")).expect("a claim id")
}

fn source_id(n: u64) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-{n:012x}")).expect("a source id")
}

fn digest(words: &str) -> Digest {
    let text = format!("{words} ^x");
    block_digest(inklkunn_markup::blocks(&text).first().expect("a block"))
}

// ------------------------------------------------------------------------------------------------ edits

/// One edit to a claim file, as the corpus makes them.
#[derive(Debug, Clone, PartialEq)]
enum Edit {
    /// The side leaves the file as it was.
    Nothing,
    AddExpr(Expression),
    RemoveExpr(Expression),
    AddRel(Relation),
    RemoveRel(Relation),
    /// A key of the link to source `n`.
    Link(u64, &'static str, String),
    RemoveLink(u64),
    AddLink(SourceLink),
    Kind(StatementKind),
    Standing(CommunityStatus),
}

impl Edit {
    /// The value-bearing place the edit writes, if it writes one: (place, what it is set to).
    fn slot(&self) -> Option<(String, String)> {
        match self {
            Edit::Link(n, field, v) => Some((format!("link/{n}/{field}"), v.clone())),
            Edit::RemoveLink(n) => Some((format!("link/{n}"), "removed".to_owned())),
            Edit::AddLink(l) => Some((
                format!("link/{}", ordinal(&l.source)),
                format!("{:?}", (l.relation, l.bearing, &l.note, &l.judged_on)),
            )),
            Edit::Kind(k) => Some(("kind".to_owned(), k.word().to_owned())),
            Edit::Standing(s) => Some(("standing".to_owned(), format!("{s:?}"))),
            _ => None,
        }
    }

    /// Do two edits, one on each side, write one place differently? (The oracle for an overlap.)
    fn conflicts(&self, other: &Edit) -> bool {
        let (Some((ta, va)), Some((tb, vb))) = (self.slot(), other.slot()) else {
            return false;
        };
        let inside = |outer: &str, inner: &str| inner.starts_with(&format!("{outer}/"));
        (ta == tb && va != vb) || inside(&ta, &tb) || inside(&tb, &ta)
    }

    fn apply(&self, c: &mut Claim) {
        match self {
            Edit::Nothing => {}
            Edit::AddExpr(e) => {
                if !c.expressions.contains(e) {
                    c.expressions.push(e.clone());
                }
            }
            Edit::RemoveExpr(e) => c.expressions.retain(|x| x != e),
            Edit::AddRel(r) => {
                if !c.relations.contains(r) {
                    c.relations.push(r.clone());
                }
            }
            Edit::RemoveRel(r) => c.relations.retain(|x| x != r),
            Edit::Link(n, field, v) => {
                if let Some(l) = c.sources.iter_mut().find(|l| l.source == source_id(*n)) {
                    match *field {
                        "bearing" => {
                            l.bearing = Bearing::from_word(v).expect("a bearing");
                        }
                        "note" => l.note = Some(v.clone()),
                        "judged_on" => l.judged_on = Digest::parse(v).expect("a digest"),
                        _ => panic!("no such key"),
                    }
                }
            }
            Edit::RemoveLink(n) => c.sources.retain(|l| l.source != source_id(*n)),
            Edit::AddLink(l) => {
                if !c.sources.iter().any(|x| x.source == l.source) {
                    c.sources.push(l.clone());
                }
            }
            Edit::Kind(k) => c.kind = *k,
            Edit::Standing(s) => c.community_status = s.clone(),
        }
        c.normalize();
    }
}

/// The number a source id was made from.
fn ordinal(id: &SourceId) -> u64 {
    u64::from_str_radix(id.as_str().rsplit('-').next().expect("a tail"), 16).expect("hex")
}

fn expr(page: &str, block: &str) -> Expression {
    Expression::new(page, block).expect("an expression")
}

fn link(n: u64, relation: Link, bearing: Bearing, note: Option<&str>, words: &str) -> SourceLink {
    SourceLink {
        source: source_id(n),
        relation,
        bearing,
        note: note.map(str::to_owned),
        judged_on: digest(words),
    }
}

fn file(c: &Claim) -> Vec<PageText> {
    vec![PageText::new(claim_path(&c.id), c.to_yaml())]
}

// ------------------------------------------------------------------------------------------------ the oracle

/// What a merge of `theirs` and `ours` (each a list of edits to `base`) must be, if no two edits conflict: the
/// edits applied in turn (they commute when they do not conflict).
fn expected(base: &Claim, theirs: &[Edit], ours: &[Edit]) -> Claim {
    let mut c = base.clone();
    for e in theirs.iter().chain(ours) {
        e.apply(&mut c);
    }
    c
}

fn applied(base: &Claim, edits: &[Edit]) -> Claim {
    expected(base, edits, &[])
}

/// Nothing is lost: every entry a side added is there unless the other side removed it, and every entry of the
/// base that neither side removed is there.
fn nothing_lost(base: &Claim, theirs: &Claim, ours: &Claim, merged: &Claim) {
    let set = |c: &Claim| -> BTreeSet<String> {
        c.expressions
            .iter()
            .map(|e| format!("e {}#{}", e.page_id, e.block))
            .chain(
                c.relations
                    .iter()
                    .map(|r| format!("r {} {}", r.kind.word(), r.claim)),
            )
            .chain(c.sources.iter().map(|l| format!("s {}", l.source)))
            .collect()
    };
    let (b, t, o, m) = (set(base), set(theirs), set(ours), set(merged));
    for entry in t.union(&o).chain(&b) {
        let (in_b, in_t, in_o) = (b.contains(entry), t.contains(entry), o.contains(entry));
        let should = match (in_t, in_o) {
            (true, true) => true,
            (true, false) | (false, true) => !in_b,
            (false, false) => false,
        };
        assert_eq!(m.contains(entry), should, "{entry}");
    }
}

/// The verdict of one pair, with every check that holds for every pair.
fn judge(name: &str, base: &Claim, theirs: &[Edit], ours: &[Edit]) -> bool {
    let (t, o) = (applied(base, theirs), applied(base, ours));
    let outcome = merge(&file(base), &file(&t), &file(&o));
    let swapped = merge(&file(base), &file(&o), &file(&t));
    let must_overlap = theirs.iter().any(|a| ours.iter().any(|b| a.conflicts(b)));
    assert_eq!(
        outcome.is_clean(),
        !must_overlap,
        "{name}: the verdict is the oracle's"
    );
    // The order of the sides never changes the verdict.
    assert_eq!(outcome.is_clean(), swapped.is_clean(), "{name}");
    match &outcome {
        MergeOutcome::Clean { pages } => {
            let page = pages.first().expect("the claim file");
            let merged = Claim::parse(base.id.clone(), &page.text).expect("the merged file reads");
            assert_eq!(
                merged,
                expected(base, theirs, ours),
                "{name}: the merged claim"
            );
            nothing_lost(base, &t, &o, &merged);
            // And the other way round, the same file.
            let MergeOutcome::Clean { pages: other } = swapped else {
                panic!("{name}");
            };
            assert_eq!(pages, &other, "{name}: symmetric");
            true
        }
        MergeOutcome::Overlaps { overlaps } => {
            assert!(!overlaps.is_empty(), "{name}");
            for o in overlaps {
                assert!(matches!(o.place, Place::Knowledge { .. }), "{name}");
                assert_eq!(o.page.path, claim_path(&base.id), "{name}");
                // Both texts are there (and the base's, unless both sides added it).
                assert!(o.theirs.is_some() || o.ours.is_some(), "{name}");
            }
            let MergeOutcome::Overlaps { overlaps: other } = swapped else {
                panic!("{name}");
            };
            assert_eq!(overlaps.len(), other.len(), "{name}");
            for (x, y) in overlaps.iter().zip(&other) {
                assert_eq!(
                    (&x.theirs, &x.ours, &x.base),
                    (&y.ours, &y.theirs, &y.base),
                    "{name}"
                );
            }
            false
        }
    }
}

// ------------------------------------------------------------------------------------------------ hand cases

fn hand_base() -> Claim {
    let mut c = Claim::new(claim_id(1));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![expr("p1", "b1"), expr("p1", "b2")];
    c.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(9),
    }];
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, None, "w1"),
        link(2, Link::Challenges, Bearing::Partial, Some("n"), "w2"),
    ];
    c.normalize();
    c
}

fn hand_edits() -> Vec<(&'static str, Edit)> {
    let standing = |s, n: u64| CommunityStatus {
        status: s,
        proposal: Some(format!("01930000-aaaa-7000-8000-{n:012x}")),
        judged_on: Some(digest("standing")),
    };
    vec![
        ("add-expression-x", Edit::AddExpr(expr("p2", "x"))),
        ("add-expression-y", Edit::AddExpr(expr("p3", "y"))),
        ("remove-expression-b1", Edit::RemoveExpr(expr("p1", "b1"))),
        (
            "add-relation-contradicts",
            Edit::AddRel(Relation {
                kind: RelationType::Contradicts,
                claim: claim_id(9),
            }),
        ),
        (
            "remove-relation-refines",
            Edit::RemoveRel(Relation {
                kind: RelationType::Refines,
                claim: claim_id(9),
            }),
        ),
        (
            "bearing-indirect",
            Edit::Link(1, "bearing", "indirect".to_owned()),
        ),
        (
            "bearing-partial",
            Edit::Link(1, "bearing", "partial".to_owned()),
        ),
        ("note-a", Edit::Link(1, "note", "a note".to_owned())),
        ("note-b", Edit::Link(1, "note", "another note".to_owned())),
        (
            "rejudge-link-2",
            Edit::Link(2, "judged_on", digest("w5").to_string()),
        ),
        ("remove-link-1", Edit::RemoveLink(1)),
        (
            "add-link-3-direct",
            Edit::AddLink(link(3, Link::Supports, Bearing::Direct, None, "w3")),
        ),
        (
            "add-link-3-indirect",
            Edit::AddLink(link(3, Link::Supports, Bearing::Indirect, None, "w3")),
        ),
        ("kind-policy", Edit::Kind(StatementKind::Policy)),
        ("kind-opinion", Edit::Kind(StatementKind::Opinion)),
        (
            "standing-contested",
            Edit::Standing(standing(Standing::Contested, 1)),
        ),
        (
            "standing-accepted",
            Edit::Standing(standing(Standing::Accepted, 2)),
        ),
        ("no-change", Edit::Nothing),
    ]
}

fn run_hand() -> (usize, usize) {
    let base = hand_base();
    let edits = hand_edits();
    assert_eq!(edits.len(), 18);
    let (mut clean, mut overlap) = (0, 0);
    for (na, a) in &edits {
        for (nb, b) in &edits {
            let name = format!("{na} / {nb}");
            if judge(
                &name,
                &base,
                std::slice::from_ref(a),
                std::slice::from_ref(b),
            ) {
                clean += 1;
            } else {
                overlap += 1;
            }
        }
    }
    (clean, overlap)
}

#[test]
fn the_hand_cases_merge_as_the_oracle_says_and_the_counts_are_held() {
    assert_eq!(run_hand(), HAND);
}

// ------------------------------------------------------------------------------------------------ derived pairs

/// splitmix64: a seeded stream, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// One article's facts, as the replay counted them.
struct Facts {
    english: bool,
    changed: u64,
    replaced: u64,
    added: u64,
    removed: u64,
    reverts: u64,
    ids_minted: u64,
    alive: u64,
    edits: u64,
}

fn facts() -> Vec<Facts> {
    let doc: Value = serde_json::from_str(COUNTS).expect("the counts are JSON");
    let mut out = Vec::new();
    for (lang, v) in doc
        .get("languages")
        .and_then(Value::as_object)
        .expect("languages")
    {
        for (_, a) in v
            .get("articles")
            .and_then(Value::as_object)
            .expect("articles")
        {
            let n = |k: &str| a.get(k).and_then(Value::as_u64).expect(k);
            out.push(Facts {
                english: lang == "en",
                changed: n("changed"),
                replaced: n("replaced"),
                added: n("added"),
                removed: n("removed"),
                reverts: n("reverts"),
                ids_minted: n("ids_minted"),
                alive: n("first_ids_alive_at_end"),
                edits: n("edits"),
            });
        }
    }
    out
}

/// The base claim of article `a`: as many expressions as the article's surviving block ids give, as many links as
/// its minted ids give, each judged on a first wording.
fn derived_base(a: usize, f: &Facts) -> (Claim, u64, u64) {
    let targets = (f.alive / 20).clamp(2, 12);
    let links = (f.ids_minted / 400).clamp(1, 4);
    let mut c = Claim::new(claim_id(10_000 + a as u64));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Adopted;
    c.expressions = (0..targets)
        .map(|i| expr(&format!("page-{a}"), &format!("b{i}")))
        .collect();
    c.sources = (1..=links)
        .map(|n| link(n, Link::Supports, Bearing::Direct, None, "first wording"))
        .collect();
    c.normalize();
    (c, targets, links)
}

/// An invented sentence for an edit: English for the English articles, Japanese for the rest.
fn note(f: &Facts, a: usize, k: u64) -> String {
    if f.english {
        format!("Note {k} about the sample of article {a}.")
    } else {
        format!("記事 {a} の標本についての注記 {k}。")
    }
}

/// One side's edits for one pair: the kind is drawn with the article's own weights.
fn side(rng: &mut Rng, a: usize, f: &Facts, targets: u64, links: u64) -> Vec<Edit> {
    let total = f.changed + f.replaced + f.added + f.removed + f.reverts;
    let mut r = rng.below(total);
    let page = format!("page-{a}");
    let mut take = |w: u64| {
        let hit = r < w;
        r = r.saturating_sub(w);
        hit
    };
    if take(f.changed) {
        // The edit kept the block id and changed its words: a reviewer re-judges a link, and says so in a note.
        let n = 1 + rng.below(links);
        let k = 1 + rng.below(3);
        let mut out = vec![Edit::Link(
            n,
            "judged_on",
            digest(&format!("wording {k}")).to_string(),
        )];
        if rng.below(2) == 0 {
            out.push(Edit::Link(n, "note", note(f, a, k)));
        }
        out
    } else if take(f.replaced) {
        // The block id did not survive: the expression moves.
        let from = rng.below(targets);
        let to = targets + rng.below(4);
        vec![
            Edit::RemoveExpr(expr(&page, &format!("b{from}"))),
            Edit::AddExpr(expr(&page, &format!("b{to}"))),
        ]
    } else if take(f.added) {
        vec![Edit::AddExpr(expr(
            &page,
            &format!("b{}", targets + rng.below(4)),
        ))]
    } else if take(f.removed) {
        vec![Edit::RemoveExpr(expr(
            &page,
            &format!("b{}", rng.below(targets)),
        ))]
    } else {
        // A revert leaves the file as it was.
        Vec::new()
    }
}

fn run_derived() -> (usize, usize, usize) {
    let (mut clean, mut overlap, mut pairs) = (0, 0, 0);
    for (a, f) in facts().iter().enumerate() {
        let (base, targets, links) = derived_base(a, f);
        for p in 0..(f.edits / 9) {
            let mut rng = Rng(a as u64 * 1_000_003 + p);
            let theirs = side(&mut rng, a, f, targets, links);
            let ours = side(&mut rng, a, f, targets, links);
            let name = format!("article {a}, pair {p}");
            if judge(&name, &base, &theirs, &ours) {
                clean += 1;
            } else {
                overlap += 1;
            }
            pairs += 1;
        }
    }
    (clean, overlap, pairs)
}

#[test]
fn the_pairs_derived_from_spike_cs_replay_facts_merge_as_the_oracle_says_and_the_counts_are_held() {
    let (clean, overlap, pairs) = run_derived();
    assert_eq!(pairs, clean + overlap);
    assert_eq!((clean, overlap), DERIVED, "{pairs} pairs");
}

#[test]
fn the_corpus_has_the_size_it_says() {
    let all = facts();
    assert_eq!(all.len(), 24, "12 English and 12 Japanese articles");
    assert_eq!(all.iter().filter(|f| f.english).count(), 12);
    let (c, o, pairs) = run_derived();
    assert!(pairs >= 200, "{pairs}");
    assert!(o > 0 && c > o, "{c} clean, {o} overlaps");
    let (hc, ho) = run_hand();
    assert_eq!(hc + ho, 324);
}
