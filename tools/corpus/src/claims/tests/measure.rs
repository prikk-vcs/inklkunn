//! Each measure on a hand-labelled fixture. The numbers in the comments are worked out by hand.
//!
//! Pool (item: lang, kind, id before) and the labels (first / second):
//!
//! | item | lang | kind     | chain | first            | second           |
//! |------|------|----------|-------|------------------|------------------|
//! | 1    | en   | changed  | 0-1   | same-claim       | same-claim       |
//! | 2    | en   | changed  | 0-1   | same-claim       | refines          |
//! | 3    | en   | changed  | 0-2   | refines          | refines          |
//! | 4    | en   | replaced | 0-2   | new-claim        | new-claim        |
//! | 5    | ja   | changed  | 1-1   | same-claim       | same-claim       |
//! | 6    | ja   | changed  | 1-2   | contradicts      | cannot-tell      |
//! | 7    | ja   | replaced | 1-3   | not factual      | new-claim        |
//! | 8    | en   | changed  | 0-1   | same-claim       | (not answered)   |

use std::collections::BTreeMap;

use crate::claims::events::{Event, Kind, Record, RevisionRec};
use crate::claims::labels::{Judgement, Label, parse_labels};
use crate::claims::measure::{Input, compute, render};
use crate::claims::pool::PoolItem;

use super::support::event;

/// A map entry that must be there.
fn at<'a, V>(m: &'a BTreeMap<String, V>, key: &str) -> &'a V {
    m.get(key).unwrap()
}

fn pool_item(item: u64, lang: &str, article: &str, rev: u64, kind: Kind) -> PoolItem {
    PoolItem {
        item,
        lang: lang.to_owned(),
        article: article.to_owned(),
        rev,
        new_block: Some(0),
        prev_block: Some(0),
        kind,
    }
}

fn revision(lang: &str, article: &str, rev: u64, position: usize, minted: usize) -> Record {
    Record::Revision(RevisionRec {
        kind: "revision".to_owned(),
        lang: lang.to_owned(),
        article: article.to_owned(),
        rev,
        parent_rev: (position > 0).then(|| rev - 1),
        position,
        blocks: 3,
        minted,
        changed: 0,
        replaced: 0,
        added: 0,
        removed: 0,
        is_revert: false,
    })
}

fn judged(label: Label) -> Judgement {
    Judgement {
        factual: true,
        label: Some(label),
    }
}

fn fixture(sample: usize) -> Input {
    let specs = [
        (1, "en", "A", 12, Kind::Changed, "0-1"),
        (2, "en", "A", 13, Kind::Changed, "0-1"),
        (3, "en", "A", 14, Kind::Changed, "0-2"),
        (4, "en", "A", 15, Kind::Replaced, "0-2"),
        (5, "ja", "B", 22, Kind::Changed, "1-1"),
        (6, "ja", "B", 23, Kind::Changed, "1-2"),
        (7, "ja", "B", 24, Kind::Replaced, "1-3"),
        (8, "en", "A", 16, Kind::Changed, "0-1"),
    ];
    let pool: Vec<PoolItem> = specs
        .iter()
        .map(|(i, l, a, r, k, _)| pool_item(*i, l, a, *r, *k))
        .collect();
    let mut records = Vec::new();
    // English article A: 11 revisions (10 edits), 5 ids minted in the first. Japanese B: 6 revisions (5 edits), 3 ids.
    for p in 0..11 {
        records.push(revision(
            "en",
            "A",
            10 + p as u64,
            p,
            if p == 0 { 5 } else { 0 },
        ));
    }
    for p in 0..6 {
        records.push(revision(
            "ja",
            "B",
            20 + p as u64,
            p,
            if p == 0 { 3 } else { 0 },
        ));
    }
    for (_, l, a, r, k, id) in &specs {
        let mut e: Event = event(*k, l, a, *r, (Some(0), Some(0)), Some(id));
        e.text_differs = true;
        records.push(Record::Event(e));
    }
    let mut first = BTreeMap::new();
    let mut second = BTreeMap::new();
    for (item, a, b) in [
        (1, Label::SameClaim, Label::SameClaim),
        (2, Label::SameClaim, Label::Refines),
        (3, Label::Refines, Label::Refines),
        (4, Label::NewClaim, Label::NewClaim),
        (5, Label::SameClaim, Label::SameClaim),
        (6, Label::Contradicts, Label::CannotTell),
    ] {
        first.insert(item, judged(a));
        second.insert(item, judged(b));
    }
    first.insert(
        7,
        Judgement {
            factual: false,
            label: None,
        },
    );
    second.insert(7, judged(Label::NewClaim));
    first.insert(8, judged(Label::SameClaim));
    Input {
        pool,
        records,
        first,
        second,
        sample,
    }
}

#[test]
fn the_sample_is_the_first_items_both_marked_factual_and_the_rest_are_counted() {
    let r = compute(&fixture(400)).unwrap();
    let s = &r.sample;
    assert_eq!(
        (s.size, s.scanned, s.skipped_not_factual, s.unlabelled),
        (6, 8, 1, 1)
    );
    assert_eq!((s.factual_first, s.factual_second), (7, 7));
    // A smaller sample stops at the n-th item both marked factual.
    let r3 = compute(&fixture(3)).unwrap();
    assert_eq!(
        (
            r3.sample.size,
            r3.sample.scanned,
            r3.sample.skipped_not_factual
        ),
        (3, 3, 0)
    );
    assert_eq!(r3.overall.sample_size, 3);
    assert_eq!(r.languages.get("en").unwrap().sample_size, 4);
    assert_eq!(r.languages.get("ja").unwrap().sample_size, 2);
}

#[test]
fn m1_is_the_share_of_surviving_ids_both_called_the_same_claim() {
    // Sample items whose id survived: 1, 2, 3, 5, 6 (the changed ones). Both same-claim: 1 and 5.
    let m1 = &compute(&fixture(400)).unwrap().overall.m1;
    assert_eq!((m1.both_same_claim.n, m1.both_same_claim.of), (2, 5));
    assert_eq!(m1.both_same_claim.share, Some(0.4));
    assert_eq!(m1.first_same_claim.n, 3, "items 1, 2, 5");
    assert_eq!(m1.second_same_claim.n, 2, "items 1, 5");
    let report = compute(&fixture(400)).unwrap();
    let en = &report.languages.get("en").unwrap().m1;
    assert_eq!((en.both_same_claim.n, en.both_same_claim.of), (1, 3));
}

#[test]
fn m2_estimates_the_reconfirmations_with_its_formula() {
    let r = compute(&fixture(400)).unwrap();
    let en = &r.languages.get("en").unwrap().m2;
    // English: 4 changed events in the replay (items 1, 2, 3, 8); pool changed items both labelled: 1, 2, 3 (8 is
    // unanswered), all factual: share 3/3. Estimate 4. Edits 10, edited blocks 4 + 1 replaced = 5.
    assert_eq!((en.edits, en.changed_events, en.edited_blocks), (10, 4, 5));
    assert_eq!((en.factual_share.n, en.factual_share.of), (3, 3));
    assert_eq!(en.reconfirmations_estimate, Some(4.0));
    assert_eq!(en.per_100_revisions, Some(40.0));
    assert_eq!(en.per_100_edited_blocks, Some(80.0));
    let ja = &r.languages.get("ja").unwrap().m2;
    // Japanese: 2 changed events (5, 6), both labelled and factual: estimate 2; edits 5; blocks 3.
    assert_eq!(ja.reconfirmations_estimate, Some(2.0));
    assert_eq!(ja.per_100_revisions, Some(40.0));
    assert_eq!(ja.per_100_edited_blocks, Some(66.6667));
    // Overall: the sum of the languages' estimates (6) over 15 edits and 8 edited blocks.
    let all = &r.overall.m2;
    assert_eq!(all.reconfirmations_estimate, Some(6.0));
    assert_eq!(all.per_100_revisions, Some(40.0));
    assert_eq!(all.per_100_edited_blocks, Some(75.0));
    assert!(all.formula.contains("changed events") && all.formula.contains("both labelled"));
}

#[test]
fn m2_shrinks_the_estimate_by_the_share_of_changed_items_that_are_factual() {
    // Item 2 is not factual for the second labeller: of the 3 changed pool items both answered (1, 2, 3) two are
    // factual from both, so the English estimate is 4 x 2/3.
    let mut input = fixture(400);
    input.second.insert(
        2,
        Judgement {
            factual: false,
            label: None,
        },
    );
    let report = compute(&input).unwrap();
    let en = &report.languages.get("en").unwrap().m2;
    assert_eq!((en.factual_share.n, en.factual_share.of), (2, 3));
    assert_eq!(en.reconfirmations_estimate, Some(2.6667));
}

#[test]
fn m2b_is_the_share_of_labelled_meaning_changes_whose_id_survived() {
    let m = &compute(&fixture(400)).unwrap().overall.m2b;
    // Both gave a meaning-change label: items 3 (refines/refines, id survived) and 4 (new-claim, replaced).
    assert_eq!((m.both.id_survived.n, m.both.id_survived.of), (1, 2));
    // First alone: 3, 4, 6 (contradicts); survived 3, 6. Second alone: 2, 3, 4; survived 2, 3.
    assert_eq!((m.first.id_survived.n, m.first.id_survived.of), (2, 3));
    assert_eq!((m.second.id_survived.n, m.second.id_survived.of), (2, 3));
    // The check point 3 keys on: the surviving one has different old and new text.
    assert_eq!(
        (m.check.items, m.check.old_new_differ, m.check.all_differ),
        (1, 1, true)
    );
    let mut input = fixture(400);
    for r in &mut input.records {
        if let Record::Event(e) = r
            && e.rev == 14
        {
            e.text_differs = false;
        }
    }
    let m = &compute(&input).unwrap().overall.m2b;
    assert_eq!(
        (m.check.items, m.check.old_new_differ, m.check.all_differ),
        (1, 0, false)
    );
}

#[test]
fn m3_counts_the_disagreement_the_matrix_kappa_and_the_splits() {
    let m = &compute(&fixture(400)).unwrap().overall.m3;
    assert_eq!((m.disagreement.n, m.disagreement.of), (2, 6));
    assert_eq!(m.disagreement.share, Some(0.3333));
    // po = 4/6; first: same 3, refines 1, new 1, contradicts 1; second: same 2, refines 2, new 1, cannot 1;
    // pe = (3*2 + 1*2 + 1*1) / 36 = 9/36 = .25; kappa = (.6667 - .25) / .75 = .5556.
    assert_eq!(m.kappa, Some(0.5556));
    assert_eq!(*at(at(&m.confusion, "same-claim"), "refines"), 1);
    assert_eq!(*at(at(&m.confusion, "contradicts"), "cannot-tell"), 1);
    assert_eq!(*at(at(&m.confusion, "same-claim"), "same-claim"), 2);
    let same = at(&m.per_label, "same-claim");
    assert_eq!(
        (same.first_labelled, same.disagreed, same.share),
        (3, 1, Some(0.3333))
    );
    assert_eq!(at(&m.per_label, "new-claim").disagreed, 0);
    assert_eq!(m.splits.get("refines/same-claim"), Some(&1));
    assert_eq!(m.splits.get("cannot-tell/contradicts"), Some(&1));
    assert_eq!(m.splits.len(), 2);
    assert_eq!(at(&m.splits_by_kind, "changed").len(), 2);
}

#[test]
fn m4_counts_the_changes_of_a_chain_and_the_share_that_touch_only_wording() {
    let m = &compute(&fixture(400)).unwrap().overall.m4;
    // Chains with a `changed` event: 0-1 (items 1, 2, 8: 3), 0-2 (3: 1), 1-1 (1), 1-2 (1).
    assert_eq!(m.chains_with_changes, 4);
    let d = &m.changes_per_chain;
    assert_eq!(
        (d.mean, d.median, d.p90, d.max),
        (Some(1.5), Some(1.0), Some(3.0), Some(3.0))
    );
    // 8 ids minted in all (5 + 3); 6 changed events: 0.75 per id.
    assert_eq!((m.ids_minted, m.changes_per_id), (8, Some(0.75)));
    // Sample changed items: 1, 2, 3, 5, 6; both same-claim: 1 and 5.
    assert_eq!((m.wording_only.n, m.wording_only.of), (2, 5));
}

#[test]
fn m5_counts_the_revisions_in_which_a_watched_chain_changed() {
    let m = &compute(&fixture(400)).unwrap().overall.m5;
    // Per chain, changes per 100 edits of its article: 0-1: 3/10 -> 30; 0-2: 1/10 -> 10; 1-1: 1/5 -> 20; 1-2: 20.
    let d = &m.changed.per_100_revisions;
    assert_eq!(m.changed.chains, 4);
    assert_eq!(
        (d.mean, d.median, d.p90, d.max),
        (Some(20.0), Some(20.0), Some(30.0), Some(30.0))
    );
    // Counting the end of a chain too: item 4 replaced the block of chain 0-2, so it is 2/10 -> 20.
    let e = &m.changed_or_ended.per_100_revisions;
    assert_eq!((e.mean, e.max), (Some(22.5), Some(30.0)));
}

#[test]
fn m6_counts_agreed_labels_and_names_the_items_to_place_by_hand() {
    let m = &compute(&fixture(400)).unwrap().overall.m6;
    assert_eq!(*at(&m.agreed, "same-claim"), 2);
    assert_eq!(*at(&m.agreed, "refines"), 1);
    assert_eq!(*at(&m.agreed, "new-claim"), 1);
    assert_eq!(*at(&m.agreed, "contradicts"), 0);
    let items: Vec<u64> = m.to_place_by_hand.iter().map(|f| f.item).collect();
    assert_eq!(items, [6], "contradicts / cannot-tell");
    assert_eq!(m.to_place_by_hand.first().unwrap().first, "contradicts");
    // Either labeller's cannot-tell or contradicts flags the item.
    let mut input = fixture(400);
    input.second.insert(1, judged(Label::CannotTell));
    let items: Vec<u64> = compute(&input)
        .unwrap()
        .overall
        .m6
        .to_place_by_hand
        .iter()
        .map(|f| f.item)
        .collect();
    assert_eq!(items, [1, 6]);
}

#[test]
fn the_labels_files_are_checked_and_the_output_holds_no_text() {
    let ok = parse_labels("{\"item\":1,\"factual\":true,\"label\":\"same-claim\"}\n{\"item\":2,\"factual\":false}\n\n").unwrap();
    assert_eq!(ok.len(), 2);
    assert_eq!(ok.get(&2).unwrap().label, None);
    assert!(
        parse_labels("{\"item\":1,\"factual\":true}\n").is_err(),
        "factual needs a label"
    );
    assert!(parse_labels("{\"item\":1,\"factual\":true,\"label\":\"maybe\"}\n").is_err());
    assert!(
        parse_labels("{\"item\":1,\"factual\":false}\n{\"item\":1,\"factual\":false}\n").is_err()
    );
    let mut input = fixture(400);
    input.first.insert(99, judged(Label::SameClaim));
    assert!(compute(&input).unwrap_err().contains("not in the pool"));

    let report = compute(&fixture(400)).unwrap();
    let json = serde_json::to_string(&report).unwrap();
    let table = render(&report);
    for out in [&json, &table] {
        assert!(!out.contains("bridge") && !out.contains("tower"));
    }
    assert!(
        table.contains("M-1") && table.contains("M-3 Cohen's kappa") && table.contains("overall")
    );
}

#[test]
fn an_item_missing_from_the_replay_is_an_error() {
    let mut input = fixture(400);
    input
        .records
        .retain(|r| !matches!(r, Record::Event(e) if e.rev == 12));
    let err = compute(&input).unwrap_err();
    assert!(err.contains("item 1"), "{err}");
}
