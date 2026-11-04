//! The claims a change touches (RFC 005 part 1 D-1 c; part 2 D-3): a proposal's page lists the claims whose
//! paragraph it rewords or removes, with what each was judged on and will not be judged on any more, and, once the
//! proposal is accepted, one step that proposes judging all of them again. The repository is an in-memory double,
//! so these run in both test runs.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{Claim, CommunityStatus, Standing, block_digest, claim_path};
use inklkunn_store::{AccountId, ProposalId};

use super::claims::{
    BRIDGE, NEW, Scene, claims_in, files_of, location, open, part, principal, scene, send,
};
use crate::common::{accept_by_hand, get_as, person, post_form};

fn digest_of(block: &str) -> inklkunn_core::knowledge::Digest {
    block_digest(inklkunn_markup::blocks(block).first().expect("a block"))
}

/// A proposal that rewords the claim's paragraph, held for review; its address.
async fn rewording(s: &Scene) -> String {
    let fields = part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1951.",
        "submit",
    );
    location(&send(s, NEW, &s.writer, &fields).await)
}

fn id_in(address: &str) -> ProposalId {
    ProposalId::parse(address.rsplit('/').next().expect("an id")).expect("a proposal id")
}

/// The claim with its status decided on the wording it has, as a wiki that has been curated would hold it.
fn decide(s: &Scene) {
    let mut claim: Claim = s.claim.clone();
    claim.community_status = CommunityStatus {
        status: Standing::Contested,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of("It opened in 1950. ^k1")),
    };
    s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
}

#[tokio::test]
async fn a_rewording_lists_the_claim_it_touches_and_what_will_no_longer_be_current() {
    let s = scene();
    let to = rewording(&s).await;
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("Claims this change touches"), "{body}");
    assert!(
        body.contains("The paragraph “It opened in 1951.” is reworded."),
        "{body}"
    );
    assert!(
        body.contains("It stays the same claim, because its marker stays."),
        "{body}"
    );
    assert!(
        body.contains("1 source was judged on the earlier wording. It will be shown as judged on an earlier wording."),
        "{body}"
    );
    // The status was never decided: nothing to say of it. And the service does not say what the rewording means.
    assert!(!body.contains("community status ("), "{body}");
    for verdict in ["same meaning", "different meaning", "not the same"] {
        assert!(!body.contains(verdict), "{verdict}");
    }
    // Held for review: there is nothing to judge again yet, and the page says where the step will be.
    assert!(body.contains("Once this proposal is accepted"), "{body}");
    assert!(!body.contains("Propose judging them again"), "{body}");
}

#[tokio::test]
async fn a_decided_status_is_said_to_have_been_decided_on_the_earlier_wording() {
    let s = scene();
    decide(&s);
    // Version 2 is the tip now: the form is opened on it.
    let mut fields = part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1951.",
        "submit",
    );
    fields.retain(|(k, _)| *k != "base_state" && *k != "base_block");
    fields.push(("base_state", crate::common::state_id(2)));
    fields.push(("base_block", crate::common::block_id(2)));
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(
        body.contains("Its community status (contested) was decided on the earlier wording."),
        "{body}"
    );
}

#[tokio::test]
async fn a_removal_is_listed_as_the_claim_no_longer_said_there() {
    let s = scene();
    let mut fields = part("pages/Bridge.md", "Bridge", 0, "", "submit");
    fields.push(("drop_k1", "1".to_owned()));
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(
        body.contains(
            "The paragraph “It opened in 1950.” is removed. The claim is no longer said there."
        ),
        "{body}"
    );
}

#[tokio::test]
async fn a_change_that_touches_no_claim_says_nothing_of_claims() {
    let s = scene();
    let fields = part("pages/Bridge.md", "Bridge", 1, "It is very long.", "submit");
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(!body.contains("Claims this change touches"), "{body}");
    assert!(!body.contains("judged on an earlier wording"), "{body}");
}

#[tokio::test]
async fn once_accepted_one_step_proposes_judging_every_touched_claim_again_on_the_wording_it_has() {
    let s = scene();
    decide(&s);
    let mut fields = part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1951.",
        "submit",
    );
    // The longest summary there is: the one of the step must fit a summary however long this one is.
    fields.retain(|(k, _)| !["base_state", "base_block", "summary"].contains(k));
    fields.push(("summary", "x".repeat(120)));
    fields.push(("base_state", crate::common::state_id(2)));
    fields.push(("base_block", crate::common::block_id(2)));
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    let id = id_in(&to);
    // Accepted: the page is in the history now (the double's tip moves) and the step is on the proposal's page.
    let reviewer: AccountId = person(4);
    accept_by_hand(&s.w, &id, &reviewer, &s.writer);
    s.w.fake
        .set("pages/Bridge.md", &BRIDGE.replace("1950", "1951"));
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("Propose judging them again"), "{body}");
    assert!(!body.contains("Once this proposal is accepted"), "{body}");

    let reconfirm = format!("{to}/reconfirm");
    let a = send(&s, &reconfirm, &s.writer, &[]).await;
    let made = location(&a);
    assert_ne!(made, to);
    let files = files_of(&s, &made);
    // One curation proposal, one file for the claim: no page; the sources and the status judged on the new words.
    assert_eq!(files.len(), 1, "{files:?}");
    let claims = claims_in(&files);
    let [claim] = claims.as_slice() else {
        panic!("{claims:?}")
    };
    let now = digest_of("It opened in 1951. ^k1");
    assert!(claim.sources.iter().all(|l| l.judged_on == now));
    assert_eq!(claim.community_status.status, Standing::Contested);
    assert_eq!(claim.community_status.judged_on, Some(now));
    let page = open(&s, &made, &s.writer).await.body();
    assert!(page.contains("This is a curation proposal"), "{page}");
    assert!(page.contains("Judge the claims again after:"), "{page}");
}

#[tokio::test]
async fn the_step_is_for_an_accepted_proposal_and_for_someone_who_may_propose() {
    let s = scene();
    let to = rewording(&s).await;
    let reconfirm = format!("{to}/reconfirm");
    // Held for review: nothing was left judged on an earlier wording yet.
    let a = send(&s, &reconfirm, &s.writer, &[]).await;
    assert_eq!(a.status, StatusCode::CONFLICT, "{}", a.body());
    assert!(
        !a.body().contains("Nothing needs judging again"),
        "{}",
        a.body()
    );
    // Anonymous, and someone who may only read.
    let anonymous = post_form(&s.w, &reconfirm, None, &[]).await;
    assert_eq!(anonymous.status, StatusCode::FORBIDDEN);
    let accepted_to = rewording(&s).await;
    accept_by_hand(&s.w, &id_in(&accepted_to), &person(4), &s.writer);
    s.w.fake
        .set("pages/Bridge.md", &BRIDGE.replace("1950", "1951"));
    let observer = send(&s, &format!("{accepted_to}/reconfirm"), &s.observer, &[]).await;
    assert_eq!(observer.status, StatusCode::FORBIDDEN);
    // A proposal of another wiki or that does not exist is as absent as anywhere.
    let missing = send(
        &s,
        "/handbook/-/proposals/019a0000-0000-7000-8000-0000000000ff/reconfirm",
        &s.writer,
        &[],
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    // And a get is no way in.
    let g = get_as(&s.w, &reconfirm, Some(&s.writer)).await;
    assert_ne!(g.status, StatusCode::OK);
    let _ = principal(&s.writer);
}

#[tokio::test]
async fn when_every_claim_is_already_judged_on_the_wording_it_has_nothing_is_proposed() {
    let s = scene();
    let to = rewording(&s).await;
    accept_by_hand(&s.w, &id_in(&to), &person(4), &s.writer);
    // The history has the new words and the claim was judged on them (a curation came first).
    s.w.fake
        .set("pages/Bridge.md", &BRIDGE.replace("1950", "1951"));
    let mut claim: Claim = s.claim.clone();
    let now = digest_of("It opened in 1951. ^k1");
    for link in &mut claim.sources {
        link.judged_on = now.clone();
    }
    s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    let a = send(&s, &format!("{to}/reconfirm"), &s.writer, &[]).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert!(
        a.body().contains("Nothing needs judging again"),
        "{}",
        a.body()
    );
    let all =
        s.w.services
            .proposals
            .list(&principal(&s.writer), &s.w.wiki, &Default::default())
            .expect("a list");
    assert_eq!(all.len(), 1, "no second proposal");
}

#[tokio::test]
async fn a_reworded_paragraph_that_left_nothing_judged_on_it_has_no_step_to_offer() {
    let s = scene();
    // The link was judged on some other wording long ago, and no status was ever decided.
    let mut claim: Claim = s.claim.clone();
    claim.sources.first_mut().expect("a link").judged_on = digest_of("It opened long ago. ^k1");
    s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    let mut fields = part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1951.",
        "submit",
    );
    fields.retain(|(k, _)| *k != "base_state" && *k != "base_block");
    fields.push(("base_state", crate::common::state_id(2)));
    fields.push(("base_block", crate::common::block_id(2)));
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(
        body.contains("Nothing was judged on the earlier wording."),
        "{body}"
    );
    assert!(!body.contains("Once this proposal is accepted"), "{body}");
    accept_by_hand(&s.w, &id_in(&to), &person(4), &s.writer);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("Claims this change touches"), "{body}");
    assert!(!body.contains("Propose judging them again"), "{body}");
}

#[tokio::test]
async fn a_removed_paragraph_whose_claim_is_said_elsewhere_can_have_its_sources_judged_again() {
    let s = scene();
    // The claim is said in a second paragraph of another page, so it has a wording after the first goes.
    let mut claim: Claim = s.claim.clone();
    claim.expressions.push(
        inklkunn_core::knowledge::Expression::new("plain-page", "z1").expect("an expression"),
    );
    claim.normalize();
    s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    s.w.fake.set(
        "pages/Plain.md",
        "---\ntitle: Plain\nid: plain-page\n---\n\nAlso said. ^z1\n",
    );
    let mut fields = part("pages/Bridge.md", "Bridge", 0, "", "submit");
    fields.retain(|(k, _)| *k != "base_state" && *k != "base_block");
    fields.push(("base_state", crate::common::state_id(3)));
    fields.push(("base_block", crate::common::block_id(3)));
    fields.push(("drop_k1", "1".to_owned()));
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(
        body.contains("is removed. The claim is no longer said there."),
        "{body}"
    );
    assert!(
        body.contains("1 source was judged on the earlier wording."),
        "{body}"
    );
    accept_by_hand(&s.w, &id_in(&to), &person(4), &s.writer);
    s.w.fake.set(
        "pages/Bridge.md",
        &BRIDGE.replace("It opened in 1950. ^k1\n\n", ""),
    );
    let made = location(&send(&s, &format!("{to}/reconfirm"), &s.writer, &[]).await);
    let files = files_of(&s, &made);
    let claims = claims_in(&files);
    let now = digest_of("Also said. ^z1");
    assert_eq!(
        claims
            .first()
            .and_then(|c| c.sources.first())
            .map(|l| l.judged_on.clone()),
        Some(now)
    );
}
