//! A restriction through the write API (RFC 006 D-7, D-12; threat model C-3g): the restricted account is refused with a
//! `forbidden` problem that **says why**, with the reason it was given and the end as members, and every other refusal stays
//! the plain `forbidden`. The API only reads people: nothing here sets or lifts a restriction (those are web-only).

use inklkunn_store::{
    Actor, GovernanceStore, MeasureEvent, RestrictionEvent, RestrictionEventId, Role,
};
use serde_json::json;

use super::super::At;
use super::super::common::NOW;
use super::{API, Cast, cast, opening, post, problem_type, session, tip, token, urn};

const REASON: &str = "posting the same link into every page";

fn restrict(c: &Cast, who: &inklkunn_store::AccountId, ends: i64) {
    c.w.store
        .record_restriction_event(&RestrictionEvent {
            id: RestrictionEventId::new(),
            wiki: c.w.wiki.id.clone(),
            at: NOW - 60,
            event: MeasureEvent::Set,
            account: who.clone(),
            by: Actor {
                account: c.admin.clone(),
                rung: Role::Admin,
            },
            policy_block: None,
            reason: Some(REASON.to_owned()),
            expires_at: Some(ends),
            of_event: None,
            appeal: None,
        })
        .unwrap();
}

#[tokio::test]
async fn a_restricted_account_is_refused_with_the_reason_and_the_end_as_members_and_in_the_callers_language()
 {
    let c = cast();
    let uri = format!("{API}/proposals");
    let body = || opening("Improve it", "Changed.", true);
    // Before the restriction: both a session and a token open a proposal.
    let a = post(
        &c.w,
        &uri,
        &[token("propose", &c.contributor), tip()],
        body(),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    restrict(&c, &c.contributor, NOW + 3 * 86_400);
    for credential in [
        token("propose", &c.contributor),
        token("admin", &c.contributor),
        session(&c.contributor),
    ] {
        let a = post(&c.w, &uri, &[credential.clone(), tip()], body()).await;
        assert_eq!(a.status, 403, "{credential:?}: {}", a.text());
        let p = a.json();
        assert_eq!(problem_type(&a), urn("forbidden"));
        assert_eq!(p.at("/reason"), &json!("restricted"));
        assert_eq!(p.at("/restriction_reason"), &json!(REASON));
        assert_eq!(p.at("/until"), &json!(NOW + 3 * 86_400));
        let said = p.at("/explanation").as_str().unwrap();
        assert!(
            said.contains(REASON) && said.contains("2023-11-17 22:13 UTC"),
            "{said}"
        );
    }
    // In Japanese the sentence is Japanese and the members are the same.
    let a = post(
        &c.w,
        &uri,
        &[
            token("propose", &c.contributor),
            tip(),
            ("cookie", "inklkunn_lang=ja".to_owned()),
        ],
        body(),
    )
    .await;
    assert_eq!(a.status, 403);
    assert!(
        a.json()
            .at("/explanation")
            .as_str()
            .unwrap()
            .contains("制限は 2023-11-17 22:13 UTC に終わります。"),
        "{}",
        a.text()
    );
    assert_eq!(a.json().at("/until"), &json!(NOW + 3 * 86_400));
}

#[tokio::test]
async fn a_refusal_that_is_not_the_restrictions_is_the_plain_forbidden_and_tells_nothing_of_anyones_restriction()
 {
    let c = cast();
    restrict(&c, &c.contributor, NOW + 86_400);
    let uri = format!("{API}/proposals");
    // An observer may not propose, restriction or none; a token that cannot propose; a visitor with no account.
    for headers in [
        vec![token("propose", &c.observer), tip()],
        vec![token("read", &c.other), tip()],
    ] {
        let a = post(
            &c.w,
            &uri,
            &headers,
            opening("Improve it", "Changed.", true),
        )
        .await;
        assert_eq!(a.status, 403, "{headers:?}: {}", a.text());
        let p = a.json();
        assert_eq!(problem_type(&a), urn("forbidden"));
        assert_eq!(p.at("/reason"), &json!(null), "{}", a.text());
        assert!(!a.text().contains(REASON), "{}", a.text());
        assert_eq!(p.at("/until"), &json!(null));
    }
    // The restricted account's own token that cannot propose is the token's refusal, not the restriction's.
    let a = post(
        &c.w,
        &uri,
        &[token("read", &c.contributor), tip()],
        opening("Improve it", "Changed.", true),
    )
    .await;
    assert_eq!(a.status, 403);
    assert_eq!(a.json().at("/reason"), &json!(null), "{}", a.text());
}

#[tokio::test]
async fn a_restriction_that_has_ended_by_the_clock_refuses_nothing() {
    let c = cast();
    restrict(&c, &c.contributor, NOW);
    let a = post(
        &c.w,
        &format!("{API}/proposals"),
        &[token("propose", &c.contributor), tip()],
        opening("Improve it", "Changed.", true),
    )
    .await;
    assert_eq!(
        a.status,
        201,
        "at its end it no longer applies: {}",
        a.text()
    );
}
