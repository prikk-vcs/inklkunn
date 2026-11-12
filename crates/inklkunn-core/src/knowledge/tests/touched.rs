//! The claims a change touches and the consequence for what they were judged on (RFC 005 part 1 D-1 c).

use std::collections::BTreeSet;
use std::time::Duration;

use super::*;
use crate::knowledge::onpage::touches;
use crate::knowledge::touched::{How, PageChange, Touch, changed_blocks, judge};
use crate::read::ReadConfig;
use crate::read::testing::World;

const ONE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther. ^b2\n\nPlain.\n";
const TWO: &str = "---\ntitle: Two\nid: p2\n---\n\nSaid elsewhere. ^c1\n";

fn judged_on(block: &str) -> Digest {
    digest_of_block(block)
}

/// A claim said at `p1#b1` and `p2#c1`, whose source was judged on the first wording and whose status on the second.
fn claim() -> Claim {
    let mut c = Claim::new(claim_id(1));
    c.expressions = vec![
        Expression::new("p1", "b1").unwrap(),
        Expression::new("p2", "c1").unwrap(),
    ];
    c.sources = vec![
        SourceLink {
            source: source_id(1),
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            judged_on: judged_on("Said. ^b1"),
        },
        SourceLink {
            source: source_id(2),
            relation: Link::Challenges,
            bearing: Bearing::Partial,
            note: None,
            judged_on: judged_on("Said elsewhere. ^c1"),
        },
    ];
    c.community_status = CommunityStatus {
        status: Standing::Contested,
        proposal: None,
        judged_on: Some(judged_on("Said. ^b1")),
    };
    c
}

fn rest(e: &Expression) -> Option<Digest> {
    (e.page_id == "p2").then(|| judged_on("Said elsewhere. ^c1"))
}

fn change<'a>(old: &'a str, new: Option<&'a str>) -> PageChange<'a> {
    PageChange {
        path: "pages/One.md",
        old: Some(old),
        new,
    }
}

#[test]
fn a_reworded_paragraph_makes_what_was_judged_on_it_earlier_and_what_was_judged_on_another_wording_stays()
 {
    let new = ONE.replace("Said.", "Said otherwise.");
    let got = judge(&claim(), &[change(ONE, Some(&new))], &rest).expect("touched");
    assert_eq!(
        got,
        Touch {
            claim: claim_id(1),
            how: How::Reworded,
            path: "pages/One.md".to_owned(),
            block: "b1".to_owned(),
            words: "Said otherwise.".to_owned(),
            // The source judged on the first wording and the status; the source judged on the other page's wording
            // stays as it is.
            links: vec![source_id(1)],
            standing: Some(Standing::Contested),
        }
    );
}

#[test]
fn a_paragraph_that_goes_is_removed_and_says_what_it_said() {
    let got = judge(&claim(), &[change(ONE, None)], &rest).expect("touched");
    assert_eq!((got.how, got.words.as_str()), (How::Removed, "Said."));
    assert_eq!(got.links, [source_id(1)]);
    // The page goes only in its text: the same paragraph is no longer in it.
    let without = ONE.replace("Said. ^b1\n\n", "");
    let got = judge(&claim(), &[change(ONE, Some(&without))], &rest).expect("touched");
    assert_eq!(got.how, How::Removed);
}

#[test]
fn a_claim_said_in_two_places_whose_first_is_reworded_and_whose_second_goes_is_told_as_removed() {
    // The paragraph that goes is the stronger fact (the claim loses an expression), whichever comes first.
    let reworded = ONE.replace("Said.", "Said otherwise.");
    let two_without = TWO.replace("Said elsewhere. ^c1\n", "");
    let changes = [
        change(ONE, Some(&reworded)),
        PageChange {
            path: "pages/Two.md",
            old: Some(TWO),
            new: Some(&two_without),
        },
    ];
    let got = judge(&claim(), &changes, &rest).expect("touched");
    assert_eq!((got.how, got.block.as_str()), (How::Removed, "c1"));
    assert_eq!(got.path, "pages/Two.md");
    // With the second reworded instead, the first is the one told.
    let two_reworded = TWO.replace("Said elsewhere.", "Said somewhere.");
    let changes = [
        change(ONE, Some(&reworded)),
        PageChange {
            path: "pages/Two.md",
            old: Some(TWO),
            new: Some(&two_reworded),
        },
    ];
    let got = judge(&claim(), &changes, &rest).expect("touched");
    assert_eq!((got.how, got.block.as_str()), (How::Reworded, "b1"));
}

#[test]
fn a_change_that_leaves_the_words_alone_touches_nothing() {
    // Another paragraph changes; and the same paragraph written the same way.
    let other = ONE.replace("Plain.", "Plain, more.");
    assert_eq!(judge(&claim(), &[change(ONE, Some(&other))], &rest), None);
    assert_eq!(judge(&claim(), &[change(ONE, Some(ONE))], &rest), None);
    // A page the change does not write is not touched.
    assert_eq!(judge(&claim(), &[], &rest), None);
}

#[test]
fn a_status_nobody_decided_has_no_consequence_and_a_link_that_was_already_earlier_is_not_one_either()
 {
    let mut c = claim();
    c.community_status = CommunityStatus::undecided();
    c.sources.first_mut().unwrap().judged_on = judged_on("Said long ago. ^b1");
    let new = ONE.replace("Said.", "Said otherwise.");
    let got = judge(&c, &[change(ONE, Some(&new))], &rest).expect("touched");
    assert_eq!(got.standing, None);
    assert!(got.links.is_empty(), "{:?}", got.links);
    // It is still touched: its paragraph was reworded.
    assert_eq!(got.how, How::Reworded);
    // A file that says a wording a status was judged on and no status says no consequence either.
    let mut c = claim();
    c.community_status = CommunityStatus {
        status: Standing::Unknown,
        proposal: None,
        judged_on: Some(judged_on("Said. ^b1")),
    };
    let got = judge(&c, &[change(ONE, Some(&new))], &rest).expect("touched");
    assert_eq!(got.standing, None);
}

#[test]
fn a_source_judged_on_a_wording_the_claim_still_has_elsewhere_stays_current() {
    // The claim is said in two pages with the same words: rewording one leaves the wording in the other, so the
    // source judged on it is still judged on a wording the claim has.
    let mut c = claim();
    c.expressions = vec![
        Expression::new("p1", "x").unwrap(),
        Expression::new("p2", "x").unwrap(),
    ];
    let same = judged_on("Same. ^x");
    c.sources.first_mut().unwrap().judged_on = same.clone();
    c.sources.get_mut(1).unwrap().judged_on = judged_on("Other. ^x");
    c.community_status.judged_on = Some(same.clone());
    let old = "---\nid: p1\n---\n\nSame. ^x\n";
    let new = "---\nid: p1\n---\n\nChanged. ^x\n";
    let elsewhere = |e: &Expression| (e.page_id == "p2").then(|| same.clone());
    let got = judge(&c, &[change(old, Some(new))], &elsewhere).expect("touched");
    assert_eq!(got.how, How::Reworded);
    assert!(got.links.is_empty(), "{:?}", got.links);
    assert_eq!(got.standing, None);
    // Were the other page's wording different, they would be earlier.
    let different = |e: &Expression| (e.page_id == "p2").then(|| judged_on("Different. ^x"));
    let got = judge(&c, &[change(old, Some(new))], &different).expect("touched");
    assert_eq!(got.links, [source_id(1)]);
    assert_eq!(got.standing, Some(Standing::Contested));
}

#[test]
fn the_blocks_whose_words_change_or_go_are_the_only_ones_named() {
    let blocks: BTreeSet<String> = ["b1", "b2", "gone"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let new = ONE.replace("Said.", "Said otherwise.");
    assert_eq!(changed_blocks(&change(ONE, Some(&new)), &blocks), ["b1"]);
    assert_eq!(changed_blocks(&change(ONE, None), &blocks), ["b1", "b2"]);
    assert!(changed_blocks(&change(ONE, Some(ONE)), &blocks).is_empty());
    // A page the change creates has no paragraph that changes.
    let created = PageChange {
        path: "pages/One.md",
        old: None,
        new: Some(ONE),
    };
    assert!(changed_blocks(&created, &blocks).is_empty());
}

// --- read from the view

fn config() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::from_secs(3600),
        ..ReadConfig::default()
    }
}

#[test]
fn the_claims_a_change_touches_are_found_in_the_view_and_judged_on_the_files_at_the_base() {
    let c = claim();
    let files = [
        (claim_path(&c.id), c.to_yaml()),
        ("pages/One.md".to_owned(), ONE.to_owned()),
        ("pages/Two.md".to_owned(), TWO.to_owned()),
    ];
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let w = World::with(&refs, config());
    let base = w.fake().block_id();
    let new = ONE.replace("Said.", "Said otherwise.");
    let got = touches(
        &w.service,
        &w.wiki,
        None,
        &base,
        &[PageChange {
            path: "pages/One.md",
            old: Some(ONE),
            new: Some(&new),
        }],
    )
    .expect("touches");
    assert_eq!(got.len(), 1);
    let touch = got.first().unwrap();
    assert_eq!((touch.how, touch.block.as_str()), (How::Reworded, "b1"));
    assert_eq!(touch.links, [source_id(1)]);
    assert_eq!(touch.standing, Some(Standing::Contested));
    // A change to a paragraph no claim is said in touches none, and asks for no claim file.
    let plain = ONE.replace("Plain.", "Plain, more.");
    let (none, calls) = w.counted(|| {
        touches(
            &w.service,
            &w.wiki,
            None,
            &base,
            &[PageChange {
                path: "pages/One.md",
                old: Some(ONE),
                new: Some(&plain),
            }],
        )
    });
    assert!(none.expect("touches").is_empty());
    assert_eq!(calls.total(), 0);
}

#[test]
fn a_change_to_a_page_with_no_id_touches_no_claim_and_does_not_even_ask_for_the_index() {
    let c = claim();
    let files = [
        (claim_path(&c.id), c.to_yaml()),
        ("pages/One.md".to_owned(), ONE.to_owned()),
    ];
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    // A fresh service: no index has been built yet.
    let w = World::with(&refs, config());
    let base = w.fake().block_id();
    let (none, calls) = w.counted(|| {
        touches(
            &w.service,
            &w.wiki,
            None,
            &base,
            &[PageChange {
                path: "pages/Plain.md",
                old: Some("---\ntitle: Plain\n---\n\nWords.\n"),
                new: Some("---\ntitle: Plain\n---\n\nOther words.\n"),
            }],
        )
    });
    assert!(none.expect("touches").is_empty());
    assert_eq!(calls.total(), 0, "no repository call, so no index built");
}
