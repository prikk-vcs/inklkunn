use crate::claims::events::{Event, Kind, Record, RevisionRec};
use crate::claims::replay::{Replayed, Version, replay_article};
use crate::claims::text::{count_words, wording_change};

/// A converted page: frontmatter, then the paragraphs, blank-line separated.
fn ver(rev: u64, paragraphs: &[&str]) -> Version {
    Version {
        rev,
        text: format!("---\ntitle: T\n---\n{}\n", paragraphs.join("\n\n")),
    }
}

fn events(r: &Replayed) -> Vec<&Event> {
    r.records
        .iter()
        .filter_map(|x| match x {
            Record::Event(e) => Some(e),
            Record::Revision(_) => None,
        })
        .collect()
}

fn revisions(r: &Replayed) -> Vec<&RevisionRec> {
    r.records
        .iter()
        .filter_map(|x| match x {
            Record::Revision(v) => Some(v),
            Record::Event(_) => None,
        })
        .collect()
}

const BRIDGE: &str = "The old bridge opened in the year 1950 near the river.";
const BRIDGE_REWORDED: &str = "The old bridge was opened in the year 1950 near the river.";
const TOWER: &str = "The signal tower stands on the northern hill above the town.";
const MILL: &str = "The grain mill closed after the long drought of that decade.";

#[test]
fn the_first_revision_mints_ids_and_has_no_events() {
    let r = replay_article(4, "en", "Old bridge", &[ver(1, &[BRIDGE, TOWER])]);
    assert!(events(&r).is_empty());
    let rev = revisions(&r);
    let first = rev.first().unwrap();
    assert_eq!((first.blocks, first.minted, first.position), (2, 2, 0));
    assert_eq!(first.parent_rev, None);
    assert_eq!(r.counts.ids_minted, 2);
}

#[test]
fn a_reworded_block_keeps_its_id_and_is_a_change() {
    let r = replay_article(
        4,
        "en",
        "Old bridge",
        &[ver(1, &[BRIDGE, TOWER]), ver(2, &[BRIDGE_REWORDED, TOWER])],
    );
    let ev = events(&r);
    assert_eq!(ev.len(), 1);
    let e = ev.first().unwrap();
    assert_eq!(e.kind, Kind::Changed);
    assert!(e.id_survived);
    assert_eq!((e.prev_block, e.new_block), (Some(0), Some(0)));
    assert_eq!(e.id_before.as_deref(), Some("4-0"));
    assert_eq!(e.id_after.as_deref(), Some("4-0"));
    assert_eq!((e.rev, e.parent_rev), (2, 1));
    assert_eq!((e.chars_changed, e.words_changed), (3, 1), "the word `was`");
    assert!(!e.unrefined && e.text_differs);
    assert_eq!(e.others_changed_in_revision, 0);
    assert_eq!((e.lang.as_str(), e.article.as_str()), ("en", "Old bridge"));
}

#[test]
fn blocks_replaced_by_unrelated_ones_between_two_anchors_do_not_keep_their_ids() {
    // Two old blocks between the anchors, two unrelated new ones: the merge's alignment leaves all four
    // unmatched, so they pair in order as replacements (it would call a single one-for-one swap a change).
    let before = ver(
        1,
        &[
            BRIDGE,
            TOWER,
            MILL,
            "The harbour office keeps the tide tables.",
        ],
    );
    let after = ver(
        2,
        &[
            BRIDGE,
            "A choir of forty singers performs every spring.",
            "Annual rainfall averages nine hundred millimetres.",
            "The harbour office keeps the tide tables.",
        ],
    );
    let r = replay_article(0, "en", "T", &[before, after]);
    let ev = events(&r);
    assert_eq!(ev.len(), 2);
    for (e, (p, n)) in ev.iter().zip([(1, 1), (2, 2)]) {
        assert_eq!(e.kind, Kind::Replaced);
        assert!(!e.id_survived);
        assert_eq!((e.prev_block, e.new_block), (Some(p), Some(n)));
        assert_ne!(e.id_before, e.id_after, "the new block has a fresh id");
        assert_eq!(e.others_changed_in_revision, 1);
    }
    assert_eq!(r.counts.replaced, 2);
    assert_eq!(r.counts.ids_minted, 4 + 2);
}

#[test]
fn a_pure_addition_and_a_pure_removal() {
    let r = replay_article(
        1,
        "en",
        "T",
        &[
            ver(1, &[BRIDGE, MILL]),
            ver(2, &[BRIDGE, TOWER, MILL]),
            ver(3, &[BRIDGE, MILL]),
        ],
    );
    let ev = events(&r);
    assert_eq!(ev.len(), 2);
    let added = ev.first().unwrap();
    assert_eq!(added.kind, Kind::Added);
    assert_eq!((added.prev_block, added.new_block), (None, Some(1)));
    assert_eq!(added.id_before, None);
    assert_eq!(added.id_after.as_deref(), Some("1-2"));
    assert!(!added.id_survived && added.chars_changed > 0 && added.words_changed > 0);
    let removed = ev.get(1).unwrap();
    assert_eq!(removed.kind, Kind::Removed);
    assert_eq!((removed.prev_block, removed.new_block), (Some(1), None));
    assert_eq!(removed.id_before.as_deref(), Some("1-2"));
    assert_eq!(removed.id_after, None);
    // The block after the removed one kept its id throughout.
    assert_eq!(revisions(&r).get(2).unwrap().blocks, 2);
}

#[test]
fn a_revert_is_a_revision_identical_to_an_earlier_one_that_is_not_its_parent() {
    let a = [BRIDGE, TOWER];
    let b = [BRIDGE_REWORDED, TOWER];
    let r = replay_article(
        0,
        "en",
        "T",
        &[ver(1, &a), ver(2, &b), ver(3, &a), ver(4, &a), ver(5, &b)],
    );
    let flags: Vec<bool> = revisions(&r).iter().map(|v| v.is_revert).collect();
    // 3 repeats 1 (not its parent); 4 repeats its parent (a null edit, not a revert); 5 repeats 2.
    assert_eq!(flags, [false, false, true, false, true]);
    assert_eq!(r.counts.reverts, 2);
    assert!(
        events(&r)
            .iter()
            .all(|e| e.is_revert == (e.rev == 3 || e.rev == 5))
    );
    // The id survives a revert and its re-edit: the chain is the same.
    assert!(
        events(&r)
            .iter()
            .all(|e| e.id_before.as_deref() == Some("0-0"))
    );
}

#[test]
fn a_pair_the_diff_cannot_refine_is_counted_whole_and_flagged() {
    let r = replay_article(
        0,
        "en",
        "T",
        &[
            ver(1, &["Alpha beta gamma delta"]),
            ver(2, &["Omega sigma tau"]),
        ],
    );
    let e = *events(&r).first().unwrap();
    assert_eq!(
        e.kind,
        Kind::Changed,
        "one block for one block is the merge's change"
    );
    assert!(e.unrefined);
    assert_eq!(
        e.chars_changed,
        "Alphabetagammadelta".len() as u64 + "Omegasigmatau".len() as u64
    );
    assert_eq!(e.words_changed, 4 + 3);
}

#[test]
fn wording_is_counted_in_both_scripts() {
    let en = wording_change(
        "The bridge opened in 1950.",
        "The bridge was opened in 1950.",
    );
    assert_eq!((en.chars, en.words, en.unrefined), (3, 1, false));
    // Japanese has no spaces between words; the product's marks go character by character, the count by script.
    let ja = wording_change("橋は1950年に開通した。", "橋は1950年代に開通した。");
    assert_eq!((ja.chars, ja.words, ja.unrefined), (1, 1, false));
    let same = wording_change("same line", "same line");
    assert_eq!((same.chars, same.words, same.unrefined), (0, 0, false));
    // A second line that only one side has is counted whole, without the flag.
    let extra = wording_change("one line here", "one line here\nand a second line");
    assert!(!extra.unrefined && extra.chars > 0);
    assert_eq!(
        count_words("橋は一九五〇年に開通した。"),
        6,
        "han, hiragana, han, hiragana, han, hiragana runs"
    );
}

#[test]
fn japanese_paragraphs_are_replayed_like_english_ones() {
    let r = replay_article(
        2,
        "ja",
        "古い橋",
        &[
            ver(
                1,
                &["橋は一九五〇年に開通した。", "塔は北の丘の上に立っている。"],
            ),
            ver(
                2,
                &[
                    "橋は一九五〇年代の初めに開通した。",
                    "塔は北の丘の上に立っている。",
                ],
            ),
        ],
    );
    let ev = events(&r);
    assert_eq!(ev.len(), 1);
    let e = ev.first().unwrap();
    assert_eq!((e.kind, e.id_survived), (Kind::Changed, true));
    assert!(e.chars_changed >= 1 && e.words_changed >= 1);
    assert_eq!(e.lang, "ja");
}

#[test]
fn counts_add_up_and_the_records_are_deterministic() {
    let versions = [
        ver(1, &[BRIDGE, TOWER, MILL]),
        ver(2, &[BRIDGE_REWORDED, TOWER, MILL]),
        ver(3, &[BRIDGE_REWORDED, MILL]),
    ];
    let one = replay_article(0, "en", "T", &versions);
    let two = replay_article(0, "en", "T", &versions);
    let lines =
        |r: &Replayed| -> Vec<String> { r.records.iter().map(|x| x.to_line().unwrap()).collect() };
    assert_eq!(lines(&one), lines(&two));
    let c = &one.counts;
    assert_eq!(
        (
            c.revisions,
            c.edits,
            c.changed,
            c.removed,
            c.added,
            c.replaced
        ),
        (3, 2, 1, 1, 0, 0)
    );
    assert_eq!(c.ids_minted, 3);
    assert_eq!(c.first_ids_alive_at_end, 2);
    // No line of a record holds any of the article's words.
    assert!(
        lines(&one)
            .iter()
            .all(|l| !l.contains("bridge") && !l.contains("tower"))
    );
}
