//! The claims a change touches (RFC 005 part 1 D-1 c; part 2 D-3): a proposal's page lists the claims whose
//! paragraph it rewords or removes, with what each was judged on and will not be judged on any more, and, once the
//! proposal is accepted, one step that proposes judging all of them again. The repository is an in-memory double,
//! so these run in both test runs.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{Claim, CommunityStatus, Standing, block_digest, claim_path};
use inklkunn_store::{AccountId, ProposalId};

use super::claims::{
    BRIDGE, NEW, Scene, claim_id, claims_in, files_of, location, open, part, principal, scene, send,
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

// ------------------------------------------------------------------------------------ the review record (K3 b, K11)

/// Accept a proposal's revision by hand the way the acceptance path leaves it: the acceptance record, the claims its
/// change touched (and, for a curation proposal, the claims it re-confirms for an earlier change), the seal, and the
/// state.
fn accept_recording(
    s: &Scene,
    id: &ProposalId,
    revision: u32,
    outcomes: &[inklkunn_store::ClaimOutcome],
    reconfirming: Option<(&inklkunn_store::AcceptanceId, &[String])>,
) -> inklkunn_store::AcceptanceId {
    use inklkunn_store::{
        AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, AcceptanceStore,
        ProposalChange, ProposalOp, ProposalStore, ProposalTransition, Role,
    };
    let record =
        s.w.store
            .proposal(id, Some(&s.writer))
            .unwrap()
            .expect("a proposal");
    s.w.store
        .change_proposal(&ProposalChange {
            transition: ProposalTransition {
                proposal: id.clone(),
                op: ProposalOp::Accept,
                from: Some(inklkunn_store::ProposalState::HeldForReview),
                to: inklkunn_store::ProposalState::Accepted,
                actor: person(4),
                reason: None,
                at: 5,
            },
            text: None,
            revision: None,
            superseded_by: None,
        })
        .unwrap();
    let acceptance = AcceptanceId::new();
    s.w.store
        .begin_acceptance(&AcceptanceRecord {
            id: acceptance.clone(),
            proposal: id.clone(),
            revision,
            wiki: s.w.wiki.id.clone(),
            view: record.view.clone(),
            acceptor: person(4),
            acceptor_role: Role::Maintainer,
            policy_version: "bootstrap".to_owned(),
            proposer: record.author.clone(),
            digest: "d".repeat(64),
            bytes: 10,
            created_at: 5,
        })
        .unwrap();
    s.w.store
        .record_claim_reviews(&acceptance, outcomes)
        .unwrap();
    if let Some((change, claims)) = reconfirming {
        s.w.store
            .record_reconfirmations(&acceptance, change, claims)
            .unwrap();
    }
    s.w.store
        .finish_acceptance(&AcceptanceResult {
            acceptance: acceptance.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: "b".repeat(64),
                patch_id: "c".repeat(64),
            },
            at: 6,
        })
        .unwrap();
    acceptance
}

fn kept() -> Vec<inklkunn_store::ClaimOutcome> {
    vec![inklkunn_store::ClaimOutcome {
        claim: claim_id().to_string(),
        how: inklkunn_store::ClaimHow::Kept,
    }]
}

fn reviewer_of(s: &Scene, n: u8) -> AccountId {
    let id = person(n);
    crate::common::add_person(&s.w, &id, "Rin Sato", Some(inklkunn_store::Role::Reviewer));
    id
}

#[tokio::test]
async fn a_request_for_changes_on_a_proposal_that_touches_a_claim_is_recorded_with_it_and_no_other_is()
 {
    use inklkunn_store::ProposalStore;
    let s = scene();
    let (r1, r2) = (reviewer_of(&s, 7), reviewer_of(&s, 8));
    let touching = rewording(&s).await;
    let plain = location(
        &send(
            &s,
            NEW,
            &s.writer,
            &part("pages/Bridge.md", "Bridge", 1, "It is very long.", "submit"),
        )
        .await,
    );
    let decide = |to: &str, who: &AccountId, op: &'static str| {
        let uri = format!("{to}/decide");
        let fields = vec![("op", op.to_owned()), ("revision", "1".to_owned())];
        let who = who.clone();
        let s = &s;
        async move { send(s, &uri, &who, &fields).await }
    };
    assert!(
        s.w.store
            .touching_decisions(&s.w.wiki.id)
            .unwrap()
            .is_empty()
    );
    // An approval of a proposal that touches a claim, and a request for changes on one that touches none: neither.
    assert!(
        decide(&touching, &r2, "approve")
            .await
            .status
            .is_redirection()
    );
    assert!(
        decide(&plain, &r1, "request-changes")
            .await
            .status
            .is_redirection()
    );
    assert!(
        s.w.store
            .touching_decisions(&s.w.wiki.id)
            .unwrap()
            .is_empty()
    );
    // A request for changes on the proposal that touches the claim: recorded with it, once.
    let a = decide(&touching, &r1, "request-changes").await;
    assert!(a.status.is_redirection(), "{}", a.body());
    let found = s.w.store.touching_decisions(&s.w.wiki.id).unwrap();
    let want =
        s.w.store
            .decisions(&id_in(&touching), Some(&r1))
            .unwrap()
            .into_iter()
            .filter(|d| d.reviewer == r1)
            .map(|d| d.id)
            .collect::<Vec<_>>();
    assert_eq!(found, want);
    // A reviewer who is refused leaves nothing: the observer may not decide.
    let refused = decide(&touching, &s.observer, "request-changes").await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(s.w.store.touching_decisions(&s.w.wiki.id).unwrap(), want);
}

#[tokio::test]
async fn the_step_works_from_the_accepted_revision_names_its_change_and_is_not_offered_again_while_its_proposal_is_open()
 {
    use inklkunn_core::proposal::ProposalEdit;
    use inklkunn_store::FileChange;
    let s = scene();
    let to = rewording(&s).await;
    let id = id_in(&to);
    // A later revision of the same proposal that touches no claim: the last revision is not the accepted one.
    let later = ProposalEdit {
        summary: "A different change".to_owned(),
        files: vec![FileChange {
            path: "pages/Bridge.md".to_owned(),
            text: Some(BRIDGE.replace("It is long.", "It is very long.")),
        }],
        ..ProposalEdit::default()
    };
    s.w.services
        .proposals
        .revise(&principal(&s.writer), &s.w.wiki, &id, later)
        .expect("a second revision");
    let change = accept_recording(&s, &id, 1, &kept(), None);
    s.w.fake
        .set("pages/Bridge.md", &BRIDGE.replace("1950", "1951"));

    // The page offers the step for what revision 1 touched.
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("Propose judging them again"), "{body}");
    let reconfirm = format!("{to}/reconfirm");
    let a = send(&s, &reconfirm, &s.writer, &[]).await;
    let made = location(&a);
    assert_ne!(made, to, "{}", a.body());
    let made_id = id_in(&made);
    // It judges the claim revision 1 touched, and it names the change: the record, not only the summary.
    let files = files_of(&s, &made);
    assert_eq!(claims_in(&files).len(), 1, "{files:?}");
    assert_eq!(
        s.w.services
            .proposals
            .reconfirm_of(&principal(&s.writer), &s.w.wiki, &made_id)
            .unwrap(),
        Some(change.clone())
    );
    assert_eq!(
        s.w.services
            .proposals
            .reconfirmations_of(&principal(&s.writer), &s.w.wiki, &change)
            .unwrap(),
        vec![made_id.clone()]
    );

    // While that proposal is open the step is not offered again, and pressing it again makes nothing.
    let body = open(&s, &to, &s.writer).await.body();
    assert!(!body.contains("Propose judging them again"), "{body}");
    assert!(
        body.contains("Judging these claims again is already proposed:"),
        "{body}"
    );
    assert!(body.contains(&format!("href=\"{made}\"")), "{body}");
    let again = send(&s, &reconfirm, &s.writer, &[]).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.body());
    assert!(
        again.body().contains("already proposed"),
        "{}",
        again.body()
    );
    let count = |s: &Scene| {
        s.w.services
            .proposals
            .list(&principal(&s.writer), &s.w.wiki, &Default::default())
            .expect("a list")
            .len()
    };
    assert_eq!(count(&s), 2, "the proposal and the one step: no third");

    // Closed without being accepted, it no longer stands in the way.
    s.w.services
        .proposals
        .withdraw(&principal(&s.writer), &s.w.wiki, &made_id, "Not now.")
        .expect("withdrawn");
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("Propose judging them again"), "{body}");
    let second = location(&send(&s, &reconfirm, &s.writer, &[]).await);
    assert_ne!(second, made);
    assert_eq!(count(&s), 3);

    // Accepted, with the review record naming what it re-confirmed, it settles the matter: not offered, nothing to make.
    let claim = [claim_id().to_string()];
    accept_recording(&s, &id_in(&second), 1, &[], Some((&change, &claim)));
    let body = open(&s, &to, &s.writer).await.body();
    assert!(!body.contains("Propose judging them again"), "{body}");
    assert!(
        body.contains("Each of these claims was judged again on the wording it has now"),
        "{body}"
    );
    let last = send(&s, &reconfirm, &s.writer, &[]).await;
    assert!(
        last.body().contains("Nothing needs judging again"),
        "{}",
        last.body()
    );
    assert_eq!(count(&s), 3);
}
