//! What the knowledge layer holds on a page, for the forms (RFC 005 part 2 D-3): read from the derived index and
//! from the claim's file at the proposal's base.

use std::time::Duration;

use super::*;
use crate::knowledge::onpage::{blocks_said, claim_blocks, claims_at, claims_in};
use crate::read::ReadConfig;
use crate::read::testing::World;

const ONE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther. ^b2\n\nPlain.\n";
const NO_ID: &str = "---\ntitle: Three\n---\n\nSaid. ^b1\n";

fn config() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::from_secs(3600),
        ..ReadConfig::default()
    }
}

fn claim_at(n: u8, page: &str, block: &str) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.expressions = vec![Expression::new(page, block).expect("an expression")];
    c
}

fn world() -> World {
    let a = claim_at(1, "p1", "b1");
    let b = claim_at(2, "p1", "b1");
    let c = claim_at(3, "p1", "b2");
    let elsewhere = claim_at(4, "p2", "x");
    let files: Vec<(String, String)> = [&a, &b, &c, &elsewhere]
        .iter()
        .map(|c| (claim_path(&c.id), c.to_yaml()))
        .chain([
            ("pages/One.md".to_owned(), ONE.to_owned()),
            ("pages/Three.md".to_owned(), NO_ID.to_owned()),
        ])
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::with(&refs, config())
}

#[test]
fn the_markers_a_form_holds_are_the_blocks_of_this_page_that_claims_are_said_in() {
    let w = world();
    // A page with no id in its information block can be named by no claim, and nothing is asked to say so.
    let (none, calls) = w.counted(|| claim_blocks(&w.service, &w.wiki, None, NO_ID));
    assert!(none.expect("blocks").is_empty());
    assert_eq!(calls.total(), 0);
    let got = claim_blocks(&w.service, &w.wiki, None, ONE).expect("blocks");
    assert_eq!(
        got.iter().map(String::as_str).collect::<Vec<_>>(),
        ["b1", "b2"],
        "the other page's claim holds nothing here"
    );
}

#[test]
fn the_claims_said_at_a_block_are_read_from_their_files_at_the_base() {
    let w = world();
    let base = w.fake().block_id();
    let got = claims_at(&w.service, &w.wiki, None, &base, "p1", "b1").expect("claims");
    let mut ids: Vec<String> = got.iter().map(|c| c.id.to_string()).collect();
    ids.sort();
    assert_eq!(ids, [claim_id(1).to_string(), claim_id(2).to_string()]);
    assert!(
        claims_at(&w.service, &w.wiki, None, &base, "p1", "none")
            .expect("claims")
            .is_empty()
    );
    // A base that does not have the file: the claim is not among them.
    let older = crate::read::testing::block_id(0);
    assert!(
        claims_at(&w.service, &w.wiki, None, &older, "p1", "b1")
            .expect("claims")
            .is_empty()
    );
}

#[test]
fn a_proposals_own_claim_files_say_where_they_are_said() {
    let own = claim_at(9, "p1", "k1");
    let files = vec![
        (claim_path(&own.id), own.to_yaml()),
        (
            source_path(&source_id(1)),
            Source::new(source_id(1)).to_yaml(),
        ),
        (claim_path(&claim_id(8)), "not a claim".to_owned()),
    ];
    let found = claims_in(&files);
    assert_eq!(found, [own]);
    assert_eq!(
        blocks_said(&found, "p1").into_iter().collect::<Vec<_>>(),
        ["k1"]
    );
    assert!(blocks_said(&found, "p2").is_empty());
}
