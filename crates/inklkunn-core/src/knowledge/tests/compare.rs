//! Comparing two views' claims (RFC 005 part 2 D-9; handoff 005 K-T8): only differences are listed, in wording,
//! status and sources; a claim in one view only is said so; a file that cannot be read is not "no such claim"; the
//! comparison treats its two sides alike. Over hand-made rows, so each fact is set by itself.

use inklkunn_store::{
    KnowledgeAnchorRow, KnowledgeClaimRow, KnowledgeInvalidRow, KnowledgeLinkRow, KnowledgeRows,
    KnowledgeSourceRow,
};

use super::*;
use crate::knowledge::compare::{ClaimDifference, Presence, compare};
use crate::knowledge::index::{CURRENT, EARLIER, KnowledgeIndex, PRESENT};

fn claim(n: u8, standing: &str) -> KnowledgeClaimRow {
    KnowledgeClaimRow {
        id: claim_id(n).to_string(),
        kind: "observation".to_owned(),
        lifecycle: "proposed".to_owned(),
        standing: standing.to_owned(),
        standing_proposal: None,
        standing_judgement: "none".to_owned(),
    }
}

fn said(n: u8, block: &str, state: &str, digest: Option<&str>) -> KnowledgeAnchorRow {
    KnowledgeAnchorRow {
        claim: claim_id(n).to_string(),
        page_id: "page-home".to_owned(),
        block: block.to_owned(),
        page_path: Some("pages/Home.md".to_owned()),
        state: state.to_owned(),
        digest: digest.map(str::to_owned),
    }
}

fn cites(n: u8, source: u8, relation: &str, bearing: &str, judgement: &str) -> KnowledgeLinkRow {
    KnowledgeLinkRow {
        claim: claim_id(n).to_string(),
        source: source_id(source).to_string(),
        relation: relation.to_owned(),
        bearing: bearing.to_owned(),
        judgement: judgement.to_owned(),
    }
}

fn source(n: u8, kind: &str, availability: &str) -> KnowledgeSourceRow {
    KnowledgeSourceRow {
        id: source_id(n).to_string(),
        kind: kind.to_owned(),
        availability: availability.to_owned(),
        last_reviewed: None,
        superseded_by: None,
        title: None,
        author: None,
        date: None,
        reference: None,
    }
}

fn index(tip: &str, rows: KnowledgeRows) -> KnowledgeIndex {
    KnowledgeIndex::new(tip, rows)
}

/// A view with one claim, said in `b1` as `digest`, decided `standing`, citing source 1 as it is given.
fn one(standing: &str, digest: &str, links: Vec<KnowledgeLinkRow>) -> KnowledgeRows {
    KnowledgeRows {
        claims: vec![claim(1, standing)],
        sources: vec![
            source(1, "primary", "available"),
            source(2, "secondary", "available"),
        ],
        links,
        anchors: vec![said(1, "b1", PRESENT, Some(digest))],
        ..KnowledgeRows::default()
    }
}

fn supports() -> Vec<KnowledgeLinkRow> {
    vec![cites(1, 1, "supports", "direct", CURRENT)]
}

#[test]
fn views_that_agree_have_no_difference() {
    let a = index("a", one("accepted", "d1", supports()));
    let b = index("b", one("accepted", "d1", supports()));
    assert_eq!(compare(&a, &b), Vec::<ClaimDifference>::new());
    assert_eq!(compare(&a, &a), Vec::<ClaimDifference>::new());
}

#[test]
fn a_claim_worded_differently_differs_in_wording_and_in_nothing_else() {
    let a = index("a", one("accepted", "d1", supports()));
    let b = index("b", one("accepted", "d2", supports()));
    assert_eq!(
        compare(&a, &b),
        vec![ClaimDifference {
            id: claim_id(1).to_string(),
            presence: Presence::Both,
            wording: true,
            status: None,
            sources: false,
        }]
    );
}

#[test]
fn a_claim_decided_differently_differs_in_status_and_the_two_words_are_given_in_order() {
    let a = index("a", one("accepted", "d1", supports()));
    let b = index("b", one("contested", "d1", supports()));
    let got = compare(&a, &b);
    assert_eq!(got.len(), 1);
    let d = got.first().unwrap();
    assert_eq!(
        d.status,
        Some(("accepted".to_owned(), "contested".to_owned()))
    );
    assert!(!d.wording && !d.sources);
    // The other way round says the same thing with the sides swapped.
    let back = compare(&b, &a);
    assert_eq!(
        back.first().unwrap().status,
        Some(("contested".to_owned(), "accepted".to_owned()))
    );
}

#[test]
fn the_proposal_that_decided_a_status_is_not_a_difference_when_the_status_is_the_same() {
    let mut rows = one("accepted", "d1", supports());
    if let Some(c) = rows.claims.first_mut() {
        c.standing_proposal = Some("01930000-0000-7000-8000-00000000aaaa".to_owned());
        c.standing_judgement = EARLIER.to_owned();
    }
    let a = index("a", rows);
    let b = index("b", one("accepted", "d1", supports()));
    assert!(compare(&a, &b).is_empty());
}

#[test]
fn a_claim_with_other_sources_differs_in_sources_and_how_a_source_bears_counts() {
    let a = index("a", one("accepted", "d1", supports()));
    // A second source cited.
    let mut more = supports();
    more.push(cites(1, 2, "challenges", "partial", CURRENT));
    let b = index("b", one("accepted", "d1", more));
    let got = compare(&a, &b);
    assert_eq!((got.len(), got.first().unwrap().sources), (1, true));
    assert!(!got.first().unwrap().wording);
    // The same source, borne on differently.
    let c = index(
        "c",
        one(
            "accepted",
            "d1",
            vec![cites(1, 1, "supports", "partial", CURRENT)],
        ),
    );
    assert!(compare(&a, &c).first().is_some_and(|d| d.sources));
    let d = index(
        "d",
        one(
            "accepted",
            "d1",
            vec![cites(1, 1, "challenges", "direct", CURRENT)],
        ),
    );
    assert!(compare(&a, &d).first().is_some_and(|d| d.sources));
}

#[test]
fn a_source_that_is_another_kind_or_cannot_be_had_in_one_view_is_a_difference_of_sources() {
    let a = index("a", one("accepted", "d1", supports()));
    let mut rows = one("accepted", "d1", supports());
    if let Some(s) = rows.sources.first_mut() {
        s.kind = "secondary".to_owned();
    }
    assert!(
        compare(&a, &index("b", rows))
            .first()
            .is_some_and(|d| d.sources)
    );
    let mut rows = one("accepted", "d1", supports());
    if let Some(s) = rows.sources.first_mut() {
        s.availability = "unavailable".to_owned();
    }
    assert!(
        compare(&a, &index("b", rows))
            .first()
            .is_some_and(|d| d.sources)
    );
}

#[test]
fn whether_a_relation_was_judged_on_the_wording_it_has_now_is_not_a_difference_of_sources() {
    let a = index("a", one("accepted", "d1", supports()));
    let b = index(
        "b",
        one(
            "accepted",
            "d1",
            vec![cites(1, 1, "supports", "direct", EARLIER)],
        ),
    );
    assert!(compare(&a, &b).is_empty());
}

#[test]
fn a_paragraph_that_is_not_there_has_no_wording_and_that_is_a_difference() {
    let a = index("a", one("accepted", "d1", supports()));
    let mut rows = one("accepted", "d1", supports());
    rows.anchors = vec![said(1, "b1", "block-missing", None)];
    let got = compare(&a, &index("b", rows));
    assert!(got.first().is_some_and(|d| d.wording));
}

#[test]
fn only_a_paragraph_the_page_really_has_once_gives_a_claim_its_wording() {
    // A row whose state says the block is repeated is no wording, whatever digest a stale row still carries.
    let a = index("a", one("accepted", "d1", supports()));
    let mut rows = one("accepted", "d1", supports());
    rows.anchors = vec![said(1, "b1", "block-repeated", Some("d1"))];
    assert!(
        compare(&a, &index("b", rows))
            .first()
            .is_some_and(|d| d.wording)
    );
}

#[test]
fn where_a_claim_is_said_is_not_its_wording_only_what_the_paragraphs_say_is() {
    let a = index("a", one("accepted", "d1", supports()));
    let mut rows = one("accepted", "d1", supports());
    rows.anchors = vec![said(1, "b9", PRESENT, Some("d1"))];
    assert!(compare(&a, &index("b", rows)).is_empty());
}

#[test]
fn a_claim_in_one_view_only_is_said_so_and_a_claim_is_never_called_missing_from_a_view_that_could_not_be_read()
 {
    let a = index("a", one("accepted", "d1", supports()));
    let empty = index("b", KnowledgeRows::default());
    let got = compare(&a, &empty);
    assert_eq!(got.len(), 1);
    assert_eq!(got.first().unwrap().presence, Presence::OnlyA);
    assert_eq!(
        compare(&empty, &a).first().unwrap().presence,
        Presence::OnlyB
    );
    // The second view has a file of that name which cannot be read: it is not "not there".
    let broken = index(
        "b",
        KnowledgeRows {
            invalid: vec![KnowledgeInvalidRow {
                path: format!("knowledge/claims/{}.yaml", claim_id(1)),
                rule: "unknown".to_owned(),
            }],
            ..KnowledgeRows::default()
        },
    );
    assert_eq!(
        compare(&a, &broken).first().unwrap().presence,
        Presence::UnreadableInB
    );
    assert_eq!(
        compare(&broken, &a).first().unwrap().presence,
        Presence::UnreadableInA
    );
    // A broken file for another claim says nothing of this one.
    let other = index(
        "b",
        KnowledgeRows {
            invalid: vec![KnowledgeInvalidRow {
                path: format!("knowledge/claims/{}.yaml", claim_id(9)),
                rule: "unknown".to_owned(),
            }],
            ..KnowledgeRows::default()
        },
    );
    assert_eq!(
        compare(&a, &other).first().unwrap().presence,
        Presence::OnlyA
    );
}

#[test]
fn the_differences_come_in_claim_order_and_a_claim_that_differs_in_all_three_says_all_three() {
    let mut a_rows = one("accepted", "d1", supports());
    a_rows.claims.push(claim(2, "accepted"));
    let mut b_rows = one(
        "contested",
        "d2",
        vec![cites(1, 2, "supports", "direct", CURRENT)],
    );
    b_rows.claims.push(claim(2, "accepted"));
    b_rows.claims.push(claim(3, "unresolved"));
    let got = compare(&index("a", a_rows), &index("b", b_rows));
    let ids: Vec<&str> = got.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            claim_id(1).to_string().as_str(),
            claim_id(3).to_string().as_str()
        ]
    );
    let first = got.first().unwrap();
    assert!(first.wording && first.sources && first.status.is_some());
    assert_eq!(got.get(1).unwrap().presence, Presence::OnlyB);
}
