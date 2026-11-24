//! A proposal written against a base is read at that base, and a base is not a tip (RFC 006 D-8; INV-13; handoff G-T4, correction
//! J-3): the knowledge index it names a source by is the index of the base **with what a hidden change wrote taken out**, so the
//! proposal's page says *hidden* where it would have printed the title of a source whose file a hidden change last wrote, and
//! says it with the tombstone. The repository is an in-memory double, so these run in both test runs. Every sentence is invented.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{
    Bearing, Claim, ClaimId, Expression, Lifecycle, Link, Source, SourceLink, StatementKind,
    block_digest, claim_path, source_path,
};

use super::claims::{claim_id, held_unjudged, open, scene, source_id};
use crate::common::{op, post_form, shown};

const SECRET_TITLE: &str = "ZEBRA secret title";

fn second_claim() -> Claim {
    let mut claim =
        Claim::new(ClaimId::parse("01930000-0000-7000-8000-000000000002").expect("a claim id"));
    claim.kind = StatementKind::Observation;
    claim.lifecycle = Lifecycle::Proposed;
    claim.expressions = vec![Expression::new("bridge-page", "k1").expect("an expression")];
    let blocks = inklkunn_markup::blocks("It opened in 1950. ^k1");
    claim.sources = vec![SourceLink {
        source: source_id(),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: block_digest(blocks.first().expect("a block")),
    }];
    claim
}

#[tokio::test]
async fn a_source_a_hidden_change_wrote_is_named_hidden_on_the_page_of_a_proposal_written_against_it()
 {
    let s = scene();
    let source_file = source_path(&source_id());
    let claim_file = claim_path(&claim_id());
    // 1 created the wiki's files; 2 (B) retitles the source with text that is then hidden.
    s.w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Bridge.md"]),
            op("create-file", &["pages/Plain.md"]),
            op("create-file", &[claim_file.as_str()]),
            op("create-file", &[source_file.as_str()]),
        ]),
    );
    let mut retitled = Source::new(source_id());
    retitled.identity.title = Some(SECRET_TITLE.to_owned());
    s.w.fake
        .set_files(&[(source_file.as_str(), retitled.to_yaml().as_str())]);
    s.w.fake
        .show(2, shown(vec![op("edit-text", &[source_file.as_str()])]));
    s.w.services.read.forget_wiki(&s.w.wiki);
    // A proposal written at B: a second claim that cites the source.
    let second = second_claim();
    let to = held_unjudged(&s, vec![(claim_path(&second.id), Some(second.to_yaml()))]);
    // 3 (C) takes the source out, so that no tip holds what B wrote and B may be hidden.
    s.w.fake.remove(&source_file);
    s.w.fake
        .show(3, shown(vec![op("delete-node", &[source_file.as_str()])]));
    s.w.services.read.forget_wiki(&s.w.wiki);

    // Before the hiding, the proposal names the source by what the base had.
    let before = open(&s, &to, &s.writer).await.body();
    assert!(before.contains(SECRET_TITLE), "{before}");

    let hidden = post_form(
        &s.w,
        "/handbook/-/moderate",
        Some(&crate::common::person(4)),
        &[
            ("kind", "page-revision"),
            ("block", crate::common::block_id(2).as_str()),
            ("part_content", "1"),
            ("category", "personal-data"),
            ("note", ""),
        ],
    )
    .await;
    assert_eq!(hidden.status, StatusCode::OK, "{}", hidden.body());

    // After it, the page says the source is hidden, with the tombstone, and prints nothing of what it called itself.
    for who in [&s.writer, &s.observer] {
        let a = open(&s, &to, who).await;
        assert_eq!(a.status, StatusCode::OK);
        let body = a.body();
        assert!(!body.contains("ZEBRA"), "{body}");
        assert!(body.contains("Hidden by moderation"), "{body}");
        // The claim's own line is still there: the source is named by its whole identifier.
        assert!(body.contains(&source_id().to_string()), "{body}");
    }
}
