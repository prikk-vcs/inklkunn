//! RFC 004 D-5's acceptance criterion: the 120 pairs of M0's spike B, merged by the product's function, yield
//! **exactly 99 clean merges and 21 overlaps**, and every overlap shows both texts.
//!
//! The pairs are `tests/fixtures/merge-spike-b-pairs.json` (see `MERGE-FIXTURE-SOURCE.md` there for how it is
//! made from the spike's report). Each pair is two edits from one baseline: edit A landed on the tip
//! ("theirs"), edit B is the proposal ("ours").
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;

use inklkunn_core::{LineLevel, MergeOutcome, PageText, Place, merge};
use serde_json::Value;

struct Pair {
    n: u64,
    scenario: String,
    spike_block_clean: bool,
    base: Vec<PageText>,
    theirs: Vec<PageText>,
    ours: Vec<PageText>,
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.get(key)
        .unwrap_or_else(|| panic!("no `{key}` in the fixture"))
}

fn text_of(v: &Value, key: &str) -> String {
    field(v, key).as_str().expect("a string").to_owned()
}

fn pairs() -> Vec<Pair> {
    let raw = include_str!("fixtures/merge-spike-b-pairs.json");
    let doc: Value = serde_json::from_str(raw).expect("the fixture is JSON");
    let mut out = Vec::new();
    for pair in field(&doc, "pairs").as_array().expect("an array") {
        let (mut base, mut theirs, mut ours) = (Vec::new(), Vec::new(), Vec::new());
        for entry in field(pair, "pages").as_array().expect("an array") {
            let b = field(entry, "base");
            let base_text = text_of(b, "text");
            base.push(PageText::new(text_of(b, "path"), base_text.clone()));
            for (side, into) in [("theirs", &mut theirs), ("ours", &mut ours)] {
                let s = field(entry, side);
                // A side's text is absent when it is the base's.
                let text = s
                    .get("text")
                    .and_then(Value::as_str)
                    .map_or_else(|| base_text.clone(), str::to_owned);
                into.push(PageText::new(text_of(s, "path"), text));
            }
        }
        out.push(Pair {
            n: field(pair, "n").as_u64().expect("a number"),
            scenario: text_of(pair, "scenario"),
            spike_block_clean: field(pair, "spike_block_clean").as_bool().expect("a bool"),
            base,
            theirs,
            ours,
        });
    }
    out
}

#[test]
fn the_spike_s_120_pairs_yield_exactly_99_clean_and_21_overlaps() {
    let pairs = pairs();
    assert_eq!(pairs.len(), 120);
    let (mut clean, mut overlap) = (0, 0);
    let mut by_scenario: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for p in &pairs {
        let outcome = merge(&p.base, &p.theirs, &p.ours);
        let slot = by_scenario.entry(p.scenario.clone()).or_default();
        match &outcome {
            MergeOutcome::Clean { .. } => {
                clean += 1;
                slot.0 += 1;
            }
            MergeOutcome::Overlaps { .. } => {
                overlap += 1;
                slot.1 += 1;
            }
        }
        // The product function agrees with the spike's own model on every pair, not only in total.
        assert_eq!(
            outcome.is_clean(),
            p.spike_block_clean,
            "pair {} ({})",
            p.n,
            p.scenario
        );
    }
    assert_eq!((clean, overlap), (99, 21));
    // The order of the two sides never changes the verdict, nor the texts an overlap shows (each the other
    // way round).
    for p in &pairs {
        let (ab, ba) = (
            merge(&p.base, &p.theirs, &p.ours),
            merge(&p.base, &p.ours, &p.theirs),
        );
        assert_eq!(ab.is_clean(), ba.is_clean(), "pair {}", p.n);
        if let (MergeOutcome::Overlaps { overlaps: x }, MergeOutcome::Overlaps { overlaps: y }) =
            (&ab, &ba)
        {
            assert_eq!(x.len(), y.len(), "pair {}", p.n);
            for (a, b) in x.iter().zip(y) {
                assert_eq!((&a.theirs, &a.ours), (&b.ours, &b.theirs), "pair {}", p.n);
            }
        }
    }
    // The 21 are the pairs where both sides replaced the same paragraph; nothing else overlaps.
    let overlapping: Vec<(&String, usize)> = by_scenario
        .iter()
        .filter(|(_, (_, o))| *o > 0)
        .map(|(s, (_, o))| (s, *o))
        .collect();
    assert_eq!(overlapping, [(&"SameParagraph".to_owned(), 21)]);
}

#[test]
fn every_overlap_carries_both_texts_and_where() {
    let mut seen = 0;
    for p in pairs() {
        let MergeOutcome::Overlaps { overlaps } = merge(&p.base, &p.theirs, &p.ours) else {
            continue;
        };
        assert_eq!(
            overlaps.len(),
            1,
            "pair {}: one paragraph, one overlap",
            p.n
        );
        for o in overlaps {
            seen += 1;
            let theirs = o.theirs.as_deref().expect("the tip's text");
            let ours = o.ours.as_deref().expect("the proposal's text");
            let base = o.base.as_deref().expect("the base's text");
            let (a, b) = (
                format!("Revised by A in trial {}:", p.n),
                format!("Revised by B in trial {}:", p.n),
            );
            assert!(theirs.contains(&a), "pair {}: {theirs}", p.n);
            assert!(ours.contains(&b), "pair {}: {ours}", p.n);
            assert!(
                !base.contains(&a) && !base.contains(&b),
                "pair {}: {base}",
                p.n
            );
            // Where: the paragraph, by its `^id`, and its number in all three texts.
            let Place::Block {
                id,
                base: b,
                theirs: t,
                ours: u,
            } = &o.place
            else {
                panic!("pair {}: not a block: {:?}", p.n, o.place);
            };
            assert!(id.as_deref().is_some_and(|i| i.starts_with('p')), "{id:?}");
            assert!(b.is_some() && t == b && u == b);
            assert!(o.page.id.is_some());
        }
    }
    assert_eq!(seen, 21);
}

#[test]
fn nothing_either_side_wrote_is_lost_in_a_clean_merge() {
    for p in pairs() {
        let MergeOutcome::Clean { pages } = merge(&p.base, &p.theirs, &p.ours) else {
            continue;
        };
        assert_eq!(
            pages.len(),
            p.base.len(),
            "pair {}: no page appears or goes",
            p.n
        );
        for (i, base) in p.base.iter().enumerate() {
            // The page is where the tip put it (a rename), with both sides' new lines in it.
            let theirs = p.theirs.get(i).expect("the tip's page");
            let ours = p.ours.get(i).expect("the proposal's page");
            let merged = pages
                .iter()
                .find(|m| m.path == theirs.path)
                .unwrap_or_else(|| panic!("pair {}: no page at {}", p.n, theirs.path));
            for side in [theirs, ours] {
                for line in side.text.lines().filter(|l| !base.text.contains(l)) {
                    assert!(
                        merged.text.contains(line),
                        "pair {} ({}): lost `{line}`",
                        p.n,
                        p.scenario
                    );
                }
            }
        }
    }
}

#[test]
fn a_rename_beside_an_edit_lands_the_edit_on_the_new_path() {
    let mut renames = 0;
    for p in pairs()
        .into_iter()
        .filter(|p| p.scenario == "RenameAndEdit")
    {
        renames += 1;
        let MergeOutcome::Clean { pages } = merge(&p.base, &p.theirs, &p.ours) else {
            panic!("pair {}: a rename and an edit overlapped", p.n);
        };
        let old = p.base.first().expect("a page").path.clone();
        let new = p.theirs.first().expect("a page").path.clone();
        assert_ne!(old, new);
        assert!(
            pages.iter().all(|m| m.path != old),
            "pair {}: the old path stayed",
            p.n
        );
        let moved = pages.iter().find(|m| m.path == new).expect("the new path");
        let edit = format!("Revised by B in trial {}:", p.n);
        assert!(
            moved.text.contains(&edit),
            "pair {}: the edit is on the new path",
            p.n
        );
    }
    assert_eq!(renames, 12);
}

/// The measurement hook's numbers on the spike (never applied): of the 21 overlaps, how many a line-level
/// merge of the block would have declared clean. Each overlap is a whole paragraph replaced on both sides
/// by a different text on one line, so a line-level merge would have conflicted on every one.
#[test]
fn the_measurement_hook_counts_what_a_line_level_merge_would_have_made_clean() {
    let (mut clean, mut conflict, mut overlaps) = (0, 0, 0);
    for p in pairs() {
        let outcome = merge(&p.base, &p.theirs, &p.ours);
        clean += outcome.line_level_clean();
        if let MergeOutcome::Overlaps { overlaps: all } = &outcome {
            for o in all {
                overlaps += 1;
                match o.line_level {
                    LineLevel::WouldBeClean => {}
                    LineLevel::WouldConflict => conflict += 1,
                    LineLevel::NotText => panic!("a text overlap said it is not text"),
                }
            }
        }
    }
    assert_eq!(overlaps, 21);
    assert_eq!((clean, conflict), (0, 21));
}
