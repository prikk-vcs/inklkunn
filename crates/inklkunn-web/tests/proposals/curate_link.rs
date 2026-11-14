//! The way from a refusal to the curation form (RFC 005 part 2 D-3; handoff 005 K39): where a proposal's page, or a
//! refusal, says a change needs a curation proposal, it leads to the form of each claim the change touches, for those
//! who may propose and for nobody else. The repository is an in-memory double, so these run in both test runs.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{
    Claim, ClaimId, Expression, Lifecycle, Standing, StatementKind, claim_path,
};

use super::claims::{BRIDGE, Scene, held_unjudged, open, principal, scene, send};
use crate::common::{block_id, state_id};

const WORDS: &str = "Propose a change to its status or sources";

fn form_of(claim: &ClaimId) -> String {
    format!("/handbook/-/proposals/curate?claim={claim}")
}

/// `claim` with a community status decided: what only a curation proposal may set.
fn decided(claim: &Claim) -> (String, Option<String>) {
    let mut decided = claim.clone();
    decided.community_status.status = Standing::Accepted;
    (claim_path(&decided.id), Some(decided.to_yaml()))
}

/// An edit of a page beside it: the proposal is then not a curation proposal.
fn page_edit() -> (String, Option<String>) {
    (
        "pages/Bridge.md".to_owned(),
        Some(BRIDGE.replace("A note.", "A longer note.")),
    )
}

/// A second claim, said in the same paragraph, written into the wiki.
fn second_claim(s: &Scene) -> Claim {
    let id = ClaimId::parse("01930000-0000-7000-8000-000000000002").expect("a claim id");
    let mut claim = Claim::new(id);
    claim.kind = StatementKind::Observation;
    claim.lifecycle = Lifecycle::Proposed;
    claim.expressions = vec![Expression::new("bridge-page", "k1").expect("an expression")];
    s.w.fake
        .set_files(&[(claim_path(&claim.id).as_str(), claim.to_yaml().as_str())]);
    claim
}

#[tokio::test]
async fn a_proposal_that_needs_curation_leads_to_the_form_of_the_claim_for_who_may_propose() {
    let s = scene();
    let to = held_unjudged(&s, vec![decided(&s.claim), page_edit()]);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(
        body.contains(&format!("<a href=\"{}\">{WORDS}</a>", form_of(&s.claim.id))),
        "{body}"
    );
    // Someone who may not propose is told the same and is not offered a form they cannot use.
    let body = open(&s, &to, &s.observer).await.body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(!body.contains("proposals/curate"), "{body}");
    // The link opens the form of that claim.
    let form = open(&s, &form_of(&s.claim.id), &s.writer).await;
    assert_eq!(form.status, StatusCode::OK);
    assert!(form.body().contains("<h1>Curate a claim</h1>"));
    // A proposal that is a curation proposal, or changes no status, offers nothing: it has nothing to be told.
    let body = open(
        &s,
        &held_unjudged(&s, vec![(claim_path(&s.claim.id), Some(s.claim.to_yaml()))]),
        &s.writer,
    )
    .await
    .body();
    assert!(!body.contains("proposals/curate"), "{body}");
}

#[tokio::test]
async fn several_claims_are_each_offered_a_link_that_names_the_claim() {
    let s = scene();
    let second = second_claim(&s);
    let to = held_unjudged(&s, vec![decided(&s.claim), decided(&second), page_edit()]);
    let body = open(&s, &to, &s.writer).await.body();
    for claim in [&s.claim.id, &second.id] {
        assert!(
            body.contains(&format!(
                "<a href=\"{}\">Propose a change to the status or sources of the claim {claim}</a>",
                form_of(claim)
            )),
            "{body}"
        );
    }
    assert!(!body.contains(WORDS), "{body}");
}

#[tokio::test]
async fn a_refused_submission_of_a_change_that_needs_curation_leads_to_the_form() {
    let s = scene();
    // A draft that holds a decided status (the service keeps what a person made, and refuses it when it is sent).
    let draft =
        s.w.services
            .proposals
            .create(
                &principal(&s.writer),
                &s.w.wiki,
                None,
                inklkunn_core::proposal::ProposalEdit {
                    summary: "A status".to_owned(),
                    files: [decided(&s.claim), page_edit()]
                        .into_iter()
                        .map(|(path, text)| inklkunn_store::FileChange { path, text })
                        .collect(),
                    ..Default::default()
                },
            )
            .expect("a draft");
    let blocks: std::collections::BTreeSet<String> = ["k1".to_owned()].into();
    let fields = vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", state_id(1)),
        ("base_block", block_id(1)),
        ("p0_path", "pages/Bridge.md".to_owned()),
        ("p0_title", "Bridge".to_owned()),
        (
            "p0_text",
            inklkunn_core::knowledge::marker::hide_markers(
                &page_edit().1.expect("a page"),
                &blocks,
                true,
            ),
        ),
        ("summary", "A status".to_owned()),
        ("sources", String::new()),
    ];
    let uri = format!("/handbook/-/proposals/{}/edit", draft.id);
    let a = send(&s, &uri, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    let body = a.body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(
        body.contains(&format!("<a href=\"{}\">{WORDS}</a>", form_of(&s.claim.id))),
        "{body}"
    );
}
