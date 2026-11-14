//! The curation proposal's form (RFC 005 part 2 D-3): its own small form with no page text, where a claim's
//! community status, how its sources bear on it, when a source was last checked and whether it can be had are
//! proposed. Nothing else in the product sets them; this form sends an ordinary proposal for review.
//! The repository is an in-memory double, so these run in both test runs.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{
    Availability, Bearing, Link, Standing, block_digest, claim_path, source_path,
};

use super::claims::{
    Files, NEW, Scene, claims_in, file, files_of, location, open, paths, scene, send, sources_in,
};
use crate::common::{block_id, state_id};

const FORM: &str = "/handbook/-/proposals/curate";

fn claim_uri(s: &Scene) -> String {
    format!("{FORM}?claim={}", s.claim.id)
}

/// What the form sends when nothing is changed: the values the wiki has, at the tip of version `version`.
fn unchanged(s: &Scene, version: u64) -> Vec<(&'static str, String)> {
    let link = s.claim.sources.first().expect("a link");
    vec![
        ("op", "submit".to_owned()),
        ("base_state", state_id(version)),
        ("base_block", block_id(version)),
        ("claim", s.claim.id.to_string()),
        ("links", "1".to_owned()),
        ("status", "unknown".to_owned()),
        ("c0_source", link.source.to_string()),
        ("c0_relation", "supports".to_owned()),
        ("c0_bearing", "direct".to_owned()),
        ("c0_reviewed", String::new()),
        ("c0_availability", "unknown".to_owned()),
        ("summary", "Curate the claim".to_owned()),
    ]
}

fn with(
    mut fields: Vec<(&'static str, String)>,
    more: &[(&'static str, &str)],
) -> Vec<(&'static str, String)> {
    for (k, v) in more {
        fields.retain(|(name, _)| name != k);
        fields.push((k, (*v).to_owned()));
    }
    fields
}

fn only_file(files: &Files, path: &str) -> String {
    file(files, path).to_owned()
}

// ---------------------------------------------------------------------------------------------- the form

#[tokio::test]
async fn the_form_shows_the_claim_as_the_wiki_has_it_and_asks_only_what_curation_sets() {
    let s = scene();
    let a = open(&s, &claim_uri(&s), &s.writer).await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(body.contains("<h1>Curate a claim</h1>"), "{body}");
    assert!(
        body.contains("It is said as: “It opened in 1950.”"),
        "{body}"
    );
    assert!(!body.contains("^k1"), "{body}");
    for name in [
        "status",
        "c0_relation",
        "c0_bearing",
        "c0_reviewed",
        "c0_availability",
        "confirm",
        "summary",
    ] {
        assert!(body.contains(&format!("name=\"{name}\"")), "{name}: {body}");
    }
    // No page text is asked for, and nothing needs a script.
    assert!(!body.contains("<textarea"), "{body}");
    assert!(!body.contains("<script"));
    // The wiki's values are the ones chosen, in words that rank nothing.
    assert!(
        body.contains("<option value=\"unknown\" selected>not decided</option>"),
        "{body}"
    );
    assert!(
        body.contains("<option value=\"supports\" selected>supports</option>"),
        "{body}"
    );
    assert!(body.contains("Source: A history of the bridge"), "{body}");
    for ranked in ["reliable", "credible", "authoritative", "trusted"] {
        assert!(!body.to_lowercase().contains(ranked), "{ranked}");
    }
}

#[tokio::test]
async fn a_source_judged_on_an_earlier_wording_is_said_so_in_the_form() {
    let s = scene();
    let mut old = s.claim.clone();
    let blocks = inklkunn_markup::blocks("It opened in 1949. ^k1");
    let earlier = block_digest(blocks.first().expect("a block"));
    old.sources.first_mut().expect("a link").judged_on = earlier.clone();
    old.community_status.status = Standing::Contested;
    old.community_status.judged_on = Some(earlier);
    s.w.fake.set(&claim_path(&old.id), &old.to_yaml());
    let body = open(&s, &claim_uri(&s), &s.writer).await.body();
    assert_eq!(
        body.matches("This was judged on an earlier wording of the claim.")
            .count(),
        2,
        "once for the status, once for the source: {body}"
    );
    // The claim as the wiki had it before says nothing of the kind.
    let fresh = scene();
    let body = open(&fresh, &claim_uri(&fresh), &fresh.writer).await.body();
    assert!(!body.contains("an earlier wording"), "{body}");
}

#[tokio::test]
async fn the_form_is_for_someone_who_may_propose_and_for_a_claim_the_wiki_has() {
    let s = scene();
    let a = open(&s, &claim_uri(&s), &s.observer).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(!a.body().contains("name=\"status\""));
    let a = crate::common::get_as(&s.w, &claim_uri(&s), None).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    // A claim the wiki does not have, and an identifier that is none: one page, the wiki's own not-found.
    for uri in [
        format!("{FORM}?claim=01930000-0000-7000-8000-0000000000ff"),
        format!("{FORM}?claim=nonsense"),
        FORM.to_owned(),
    ] {
        let a = open(&s, &uri, &s.writer).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{uri}");
    }
}

// ---------------------------------------------------------------------------------------------- sending it

#[tokio::test]
async fn a_new_status_is_proposed_as_a_change_to_the_claim_file_and_nothing_else() {
    let s = scene();
    let fields = with(unchanged(&s, 1), &[("status", "contested")]);
    let a = send(&s, FORM, &s.writer, &fields).await;
    let to = location(&a);
    let files = files_of(&s, &to);
    assert_eq!(paths(&files), [claim_path(&s.claim.id).as_str()]);
    let claims = claims_in(&files);
    let [claim] = claims.as_slice() else {
        panic!("{claims:?}")
    };
    assert_eq!(claim.community_status.status, Standing::Contested);
    // Judged on the wording the claim had at the base; the acceptance names the proposal that decided.
    let blocks = inklkunn_markup::blocks("It opened in 1950. ^k1");
    assert_eq!(
        claim.community_status.judged_on,
        Some(block_digest(blocks.first().expect("a block")))
    );
    assert_eq!(claim.community_status.proposal, None);
    // The reviewer is told what it is and what it changes, in words.
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("This is a curation proposal"), "{body}");
    assert!(
        body.contains("Its community status changes from not decided to contested."),
        "{body}"
    );
    assert!(!body.contains("It cannot be accepted as it is."), "{body}");
    assert!(body.contains("It changes 0 pages."), "{body}");
}

#[tokio::test]
async fn a_change_of_relation_or_bearing_and_a_check_of_a_source_are_proposed_per_file() {
    let s = scene();
    let fields = with(
        unchanged(&s, 1),
        &[
            ("c0_relation", "challenges"),
            ("c0_bearing", "indirect"),
            ("c0_reviewed", "2026-11-04"),
            ("c0_availability", "available"),
        ],
    );
    let files = files_of(&s, &location(&send(&s, FORM, &s.writer, &fields).await));
    assert_eq!(files.len(), 2);
    let claims = claims_in(&files);
    let link = claims
        .first()
        .and_then(|c| c.sources.first())
        .expect("a link");
    assert_eq!(
        (link.relation, link.bearing),
        (Link::Challenges, Bearing::Indirect)
    );
    let sources = sources_in(&files);
    let source = sources.first().expect("a source");
    assert_eq!(source.availability, Availability::Available);
    assert_eq!(
        source.last_reviewed.as_ref().map(|r| r.date.as_str()),
        Some("2026-11-04")
    );
    assert_eq!(
        source
            .last_reviewed
            .as_ref()
            .and_then(|r| r.proposal.clone()),
        None
    );
    // A review alone changes the source file and not the claim's.
    let s = scene();
    let fields = with(unchanged(&s, 1), &[("c0_reviewed", "2026-11-04")]);
    let files = files_of(&s, &location(&send(&s, FORM, &s.writer, &fields).await));
    assert_eq!(
        paths(&files),
        [source_path(&s.claim.sources.first().unwrap().source).as_str()]
    );
}

#[tokio::test]
async fn confirming_judges_the_sources_again_on_the_wording_the_claim_has() {
    let s = scene();
    let mut old = s.claim.clone();
    let blocks = inklkunn_markup::blocks("It opened in 1949. ^k1");
    old.sources.first_mut().expect("a link").judged_on =
        block_digest(blocks.first().expect("a block"));
    s.w.fake.set(&claim_path(&old.id), &old.to_yaml());
    // Version 2 is the tip now.
    let fields = with(unchanged(&s, 2), &[("confirm", "1")]);
    let files = files_of(&s, &location(&send(&s, FORM, &s.writer, &fields).await));
    let claims = claims_in(&files);
    let now = block_digest(
        inklkunn_markup::blocks("It opened in 1950. ^k1")
            .first()
            .expect("a block"),
    );
    assert_eq!(
        claims
            .first()
            .and_then(|c| c.sources.first())
            .map(|l| l.judged_on.clone()),
        Some(now)
    );
    // Without it, the same form changes nothing.
    let fields = unchanged(&s, 2);
    let a = send(&s, FORM, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn show_changes_lists_them_and_sends_nothing() {
    let s = scene();
    let fields = with(
        unchanged(&s, 1),
        &[("op", "changes"), ("status", "unresolved"), ("summary", "")],
    );
    let a = send(&s, FORM, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(
        body.contains("Claims and sources in this proposal"),
        "{body}"
    );
    assert!(
        body.contains("Its community status changes from not decided to unresolved."),
        "{body}"
    );
    // The form stays, with what was chosen.
    assert!(
        body.contains("<option value=\"unresolved\" selected>unresolved</option>"),
        "{body}"
    );
    let all =
        s.w.services
            .proposals
            .list(
                &super::claims::principal(&s.writer),
                &s.w.wiki,
                &Default::default(),
            )
            .expect("a list");
    assert!(all.is_empty());
}

#[tokio::test]
async fn what_is_wrong_is_said_on_the_form_and_nothing_is_sent() {
    let s = scene();
    let no_change = send(&s, FORM, &s.writer, &unchanged(&s, 1)).await;
    assert_eq!(no_change.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        no_change
            .body()
            .contains("Nothing in the form differs from what the wiki has now."),
        "{}",
        no_change.body()
    );
    let no_summary = with(unchanged(&s, 1), &[("status", "accepted"), ("summary", "")]);
    let a = send(&s, FORM, &s.writer, &no_summary).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body().contains("what and why") || a.body().contains("summary"),
        "{}",
        a.body()
    );
    let not_a_day = with(unchanged(&s, 1), &[("c0_reviewed", "yesterday")]);
    let a = send(&s, FORM, &s.writer, &not_a_day).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body()
            .contains("A date has to be written year, month, day"),
        "{}",
        a.body()
    );
    // The value is still in the field.
    assert!(a.body().contains("value=\"yesterday\""));
    let all =
        s.w.services
            .proposals
            .list(
                &super::claims::principal(&s.writer),
                &s.w.wiki,
                &Default::default(),
            )
            .expect("a list");
    assert!(all.is_empty());
}

#[tokio::test]
async fn a_post_that_is_not_the_form_a_page_offered_is_refused() {
    let s = scene();
    for fields in [
        with(unchanged(&s, 1), &[("status", "true")]),
        with(unchanged(&s, 1), &[("c0_relation", "proves")]),
        with(unchanged(&s, 1), &[("c0_bearing", "strong")]),
        with(unchanged(&s, 1), &[("c0_availability", "reliable")]),
        with(unchanged(&s, 1), &[("op", "preview")]),
        with(unchanged(&s, 1), &[("links", "2")]),
        with(unchanged(&s, 1), &[("claim", "nonsense")]),
        // A source the claim does not cite.
        with(
            unchanged(&s, 1),
            &[("c0_source", "01931111-0000-7000-8000-0000000000ee")],
        ),
    ] {
        let a = send(&s, FORM, &s.writer, &fields).await;
        assert_ne!(a.status, StatusCode::SEE_OTHER, "{fields:?}");
        assert_ne!(a.status, StatusCode::OK, "{fields:?}");
    }
    let a = send(
        &s,
        FORM,
        &s.observer,
        &with(unchanged(&s, 1), &[("status", "accepted")]),
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_proposal_the_form_makes_is_held_for_review_by_its_proposer_and_the_page_edit_form_cannot_make_one()
 {
    let s = scene();
    let fields = with(unchanged(&s, 1), &[("status", "accepted")]);
    let to = location(&send(&s, FORM, &s.writer, &fields).await);
    let id = inklkunn_store::ProposalId::parse(to.rsplit('/').next().unwrap()).unwrap();
    let record =
        s.w.services
            .proposals
            .get(&super::claims::principal(&s.writer), &s.w.wiki, &id)
            .expect("the proposal");
    assert_eq!(record.state, inklkunn_store::ProposalState::HeldForReview);
    assert_eq!(record.summary, "Curate the claim");
    // The page-edit form has no field that sets a status: a post that tries one is not read as a status.
    let page_form = vec![
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
            super::claims::BRIDGE
                .replace(" ^k1", "")
                .replace("A note.", "More."),
        ),
        ("summary", "A page edit".to_owned()),
        ("sources", String::new()),
        ("status", "accepted".to_owned()),
        ("claim", s.claim.id.to_string()),
    ];
    let to = location(&send(&s, NEW, &s.writer, &page_form).await);
    let files = files_of(&s, &to);
    assert_eq!(paths(&files), ["pages/Bridge.md"]);
    // The page text it sent is the page with its marker, and no claim file was touched.
    assert!(only_file(&files, "pages/Bridge.md").contains("It opened in 1950. ^k1"));
}

// ---------------------------------------------------------------------------------------------- what the base had

#[tokio::test]
async fn the_wording_a_curation_judges_on_is_the_bases_not_the_tips() {
    let s = scene();
    // The form was opened at version 1; the page was reworded since (version 2 is the tip).
    s.w.fake.set(
        "pages/Bridge.md",
        &super::claims::BRIDGE.replace("1950", "1951"),
    );
    let fields = with(unchanged(&s, 1), &[("status", "contested")]);
    let files = files_of(&s, &location(&send(&s, FORM, &s.writer, &fields).await));
    let claims = claims_in(&files);
    let at_base = block_digest(
        inklkunn_markup::blocks("It opened in 1950. ^k1")
            .first()
            .expect("a block"),
    );
    assert_eq!(
        claims
            .first()
            .and_then(|c| c.community_status.judged_on.clone()),
        Some(at_base)
    );
}

#[tokio::test]
async fn a_source_whose_file_is_not_there_is_not_asked_about_and_the_rest_still_goes() {
    let s = scene();
    let mut claim = s.claim.clone();
    let mut link = claim.sources.first().expect("a link").clone();
    link.source =
        inklkunn_core::knowledge::SourceId::parse("01931111-0000-7000-8000-0000000000aa").unwrap();
    claim.sources.push(link);
    claim.normalize();
    s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    let body = open(&s, &claim_uri(&s), &s.writer).await.body();
    assert!(body.contains("Source: a source without a title"), "{body}");
    assert_eq!(body.matches("name=\"c0_relation\"").count(), 1);
    assert_eq!(body.matches("name=\"c1_relation\"").count(), 1);
    assert_eq!(body.matches("name=\"c0_availability\"").count(), 1);
    assert_eq!(
        body.matches("name=\"c1_availability\"").count(),
        0,
        "{body}"
    );
    // The links are the claim's, in its order: the source with the lower id comes first.
    let second = claim.sources.get(1).expect("a second link").source.clone();
    let mut fields = with(unchanged(&s, 2), &[("links", "2"), ("status", "contested")]);
    fields.push(("c1_source", second.to_string()));
    fields.push(("c1_relation", "supports".to_owned()));
    fields.push(("c1_bearing", "direct".to_owned()));
    let a = send(&s, FORM, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
}

#[tokio::test]
async fn nobody_is_shown_what_a_curation_would_change_before_they_are_let_in() {
    let s = scene();
    let fields = with(
        unchanged(&s, 1),
        &[("op", "changes"), ("status", "contested")],
    );
    let a = send(&s, FORM, &s.observer, &fields).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(!a.body().contains("community status"), "{}", a.body());
}

#[tokio::test]
async fn a_form_with_fewer_sources_than_the_claim_cites_is_not_the_form_that_was_offered() {
    let s = scene();
    let fields = with(unchanged(&s, 1), &[("links", "0"), ("status", "accepted")]);
    let a = send(&s, FORM, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let all =
        s.w.services
            .proposals
            .list(
                &super::claims::principal(&s.writer),
                &s.w.wiki,
                &Default::default(),
            )
            .expect("a list");
    assert!(all.is_empty());
}

#[tokio::test]
async fn a_refused_curation_leaves_no_draft_behind() {
    // K38: the form makes and sends in one step, so a send the wiki's hourly limit refuses leaves nothing: not an unsent
    // draft that counts against its author and has nothing to lead to.
    let mut s = scene();
    let services = &s.w.services;
    let proposals = std::sync::Arc::new(
        inklkunn_core::proposal::ProposalService::new(
            s.w.store.clone(),
            s.w.store.clone(),
            s.w.store.clone(),
            (*services.authorizer).clone(),
            services.read.clone(),
            inklkunn_core::proposal::QuotaConfig {
                proposals_per_hour_per_wiki: 1,
                ..inklkunn_core::proposal::QuotaConfig::default()
            },
        )
        .with_clock(|| crate::common::NOW),
    );
    s.w.services = std::sync::Arc::new(inklkunn_web::Services {
        proposals,
        ..(*s.w.services).clone()
    });
    let first = with(unchanged(&s, 1), &[("status", "accepted")]);
    location(&send(&s, FORM, &s.writer, &first).await);
    let second = with(unchanged(&s, 1), &[("status", "contested")]);
    let refused = send(&s, FORM, &s.writer, &second).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    // The author's own, drafts included: only the one that was sent.
    let mine = inklkunn_store::ProposalStore::proposals(
        &*s.w.store,
        &s.w.wiki.id,
        &Default::default(),
        Some(&s.writer),
    )
    .expect("the author's own, drafts included");
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert!(
        mine.iter()
            .all(|p| p.state == inklkunn_store::ProposalState::HeldForReview),
        "{mine:?}"
    );
    assert_eq!(
        inklkunn_store::ProposalStore::open_count(&*s.w.store, &s.w.wiki.id, &s.writer).unwrap(),
        1
    );
}
