//! When a claim file says it was judged again on the wording its paragraph has now (K17).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::judged_on_now;
use crate::knowledge::{
    Bearing, Claim, ClaimId, CommunityStatus, Digest, Link, SourceId, SourceLink, Standing,
    block_digest,
};

fn digest_of(text: &str) -> Digest {
    let body = format!("{text} ^x");
    let found = inklkunn_markup::blocks(&body);
    block_digest(found.first().expect("a block"))
}

fn claim_id(n: u8) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-0000000000{n:02x}")).expect("a claim id")
}

fn source_id(n: u8) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-0000000000{n:02x}")).expect("a source id")
}

fn link(source: u8, wording: &str) -> SourceLink {
    SourceLink {
        source: source_id(source),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: digest_of(wording),
    }
}

fn claim(status_on: Option<&str>, links: &[(u8, &str)]) -> Claim {
    let mut c = Claim::new(claim_id(1));
    c.community_status = CommunityStatus {
        status: if status_on.is_some() {
            Standing::Contested
        } else {
            Standing::Unknown
        },
        proposal: None,
        judged_on: status_on.map(digest_of),
    };
    c.sources = links.iter().map(|(s, w)| link(*s, w)).collect();
    c
}

#[test]
fn a_claim_is_judged_again_only_when_every_judgement_it_carries_names_the_current_wording() {
    let now = [digest_of("new"), digest_of("other place")];
    // Every one: the status's and each link's (any of the places the claim is said in).
    assert!(judged_on_now(
        &claim(Some("new"), &[(1, "new"), (2, "other place")]),
        &now
    ));
    // Links alone, or the status alone, when that is all it holds.
    assert!(judged_on_now(&claim(None, &[(1, "new")]), &now));
    assert!(judged_on_now(&claim(Some("new"), &[]), &now));
    // One of them on an older wording: nothing is judged again.
    assert!(!judged_on_now(&claim(Some("new"), &[(1, "old")]), &now));
    assert!(!judged_on_now(
        &claim(Some("old"), &[(1, "new"), (2, "new")]),
        &now
    ));
    assert!(!judged_on_now(
        &claim(None, &[(1, "new"), (2, "old")]),
        &now
    ));
}

#[test]
fn a_claim_that_carries_no_judgement_judges_nothing_again() {
    let now = [digest_of("new")];
    assert!(!judged_on_now(&claim(None, &[]), &now));
    // A claim whose paragraph is nowhere (no wording now) is judged on none.
    assert!(!judged_on_now(&claim(Some("new"), &[(1, "new")]), &[]));
}
