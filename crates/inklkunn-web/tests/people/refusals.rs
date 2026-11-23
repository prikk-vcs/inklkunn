//! What a restricted person is told, on every page that refuses them (RFC 006 D-7; threat model C-3g): that they are restricted,
//! the reason they were given and the moment it ends; and that no one else is told it, and no other refusal is put down to it.

use inklkunn_store::{Actor, FileChange, RestrictionEventId};

use super::*;

const REASON: &str = "posting the same link into every page";
const UNTIL: &str = "2023-11-17 22:13 UTC";

/// Restrict `who` for three days, as the admin does through the form.
async fn restrict(e: &Env, who: &AccountId) {
    let a = post_form(
        &e.w,
        "/handbook/-/people/restrict",
        Some(&e.admin),
        &restrict_form(&who.to_string(), "3", REASON),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
}

fn held(e: &Env) -> String {
    let record = e.w.hold_unjudged(
        &e.contributor,
        "Improve the second paragraph",
        vec![FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some(home_with("The bridge opened in 1951.")),
        }],
    );
    format!("/handbook/-/proposals/{}", record.id)
}

/// The three sentences a restricted person is told, in English.
fn told(main: &str) {
    assert!(
        main.contains("You are restricted in this wiki: you can read it, and you cannot comment, propose or review until the restriction ends."),
        "{main}"
    );
    assert!(
        main.contains(&format!("The reason given: {REASON}")),
        "{main}"
    );
    assert!(
        main.contains(&format!("The restriction ends on {UNTIL}.")),
        "{main}"
    );
}

#[tokio::test]
async fn a_restricted_person_is_told_the_reason_and_the_end_by_the_form_that_would_have_let_them_propose()
 {
    let e = env();
    let new = "/handbook/-/proposals/new";
    // Before the restriction the form leads on (to the page to edit).
    assert_eq!(
        as_(&e, &e.contributor, new).await.status,
        StatusCode::SEE_OTHER,
        "before the restriction"
    );
    restrict(&e, &e.contributor).await;
    let a = as_(&e, &e.contributor, new).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    let main = main_of(&a);
    assert!(main.contains("<h1>Not allowed</h1>"), "{main}");
    told(&main);
    assert!(main.contains("Nothing was changed."), "{main}");
    // In Japanese.
    let ja = as_ja(&e, &e.contributor, new).await;
    let main = main_of(&ja);
    for want in [
        "このウィキでは制限を受けています。読むことはできますが、制限が終わるまで、コメント、提案、レビューはできません。",
        &format!("示された理由: {REASON}"),
        &format!("制限は {UNTIL} に終わります。"),
        "何も変更していません。",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    // Reading is never refused.
    assert_eq!(
        as_(&e, &e.contributor, "/handbook/Home").await.status,
        StatusCode::OK
    );
    assert_eq!(
        as_(&e, &e.contributor, "/handbook/-/proposals")
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_refusal_that_is_another_ones_is_never_put_down_to_a_restriction_and_no_one_is_told_anyone_elses()
 {
    let e = env();
    restrict(&e, &e.contributor).await;
    let new = "/handbook/-/proposals/new";
    // An observer is refused for their role; a visitor is asked to sign in; the restriction is not mentioned to either.
    for who in [Some(&e.observer), Some(&e.newcomer), None] {
        let a = get_as(&e.w, new, who).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{who:?}");
        let body = a.body();
        assert!(!body.contains(REASON), "{who:?}: {body}");
        assert!(!body.contains(UNTIL), "{who:?}: {body}");
        assert!(!body.contains("You are restricted"), "{who:?}: {body}");
    }
    let a = as_(&e, &e.observer, new).await;
    assert!(
        a.body()
            .contains("Your role in this wiki does not include this. Nothing was changed."),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_restricted_reviewer_is_told_on_a_comment_a_decision_and_the_form_that_creates_a_view() {
    let e = env();
    let p = held(&e);
    // Before: they comment and decide.
    let a = post_form(
        &e.w,
        &format!("{p}/comment"),
        Some(&e.reviewer),
        &[("body", "A question")],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    restrict(&e, &e.reviewer).await;
    let a = post_form(
        &e.w,
        &format!("{p}/comment"),
        Some(&e.reviewer),
        &[("body", "Another question")],
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    told(&main_of(&a));
    let a = post_form(
        &e.w,
        &format!("{p}/decide"),
        Some(&e.reviewer),
        &[("op", "approve"), ("revision", "1"), ("body", "Fine")],
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    told(&main_of(&a));
    let a = as_(&e, &e.reviewer, "/handbook/-/views/new").await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    told(&main_of(&a));
    // What they said before the restriction stands; what they tried after is not there.
    let page = as_(&e, &e.admin, &p).await.body();
    assert!(
        page.contains("A question") && !page.contains("Another question"),
        "{page}"
    );
}

#[tokio::test]
async fn a_restricted_maintainer_is_told_on_accepting_declining_and_asking_for_a_revision() {
    let e = env();
    let p = held(&e);
    restrict(&e, &e.other_maintainer).await;
    let who = &e.other_maintainer;
    let a = post_form(
        &e.w,
        &format!("{p}/accept"),
        Some(who),
        &[("revision", "1")],
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    told(&main_of(&a));
    for act in ["decline", "ask"] {
        let a = as_(&e, who, &format!("{p}/{act}")).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{act}: {}", a.body());
        told(&main_of(&a));
        let a = post_form(&e.w, &format!("{p}/{act}"), Some(who), &[("reason", "no")]).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{act}: {}", a.body());
        told(&main_of(&a));
    }
    // An unrestricted maintainer, asked the same, is not told of any restriction (nothing is served: no signer here).
    let a = post_form(
        &e.w,
        &format!("{p}/accept"),
        Some(&e.maintainer),
        &[("revision", "1")],
    )
    .await;
    assert!(!a.body().contains("You are restricted"), "{}", a.body());
}

#[tokio::test]
async fn a_restriction_that_has_ended_tells_no_one_anything_and_refuses_nothing() {
    let e = env();
    let by = Actor {
        account: e.admin.clone(),
        rung: Role::Admin,
    };
    // Recorded as it would have been three days ago, for a day: it ended two days ago.
    e.w.store
        .record_restriction_event(&RestrictionEvent {
            id: RestrictionEventId::new(),
            wiki: e.w.wiki.id.clone(),
            at: NOW - 3 * DAY,
            event: MeasureEvent::Set,
            account: e.contributor.clone(),
            by,
            policy_block: None,
            reason: Some(REASON.to_owned()),
            expires_at: Some(NOW - 2 * DAY),
            of_event: None,
            appeal: None,
        })
        .unwrap();
    let a = as_(&e, &e.contributor, "/handbook/-/proposals/new").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "it ended: {}", a.body());
    let own = main_of(&as_(&e, &e.contributor, PEOPLE).await);
    assert!(
        !own.contains("You are restricted") && !own.contains(REASON),
        "{own}"
    );
    let seen = main_of(&as_(&e, &e.admin, PEOPLE).await);
    assert!(!seen.contains(REASON), "{seen}");
    // And a restriction that ends a second from now still holds.
    let by = Actor {
        account: e.admin.clone(),
        rung: Role::Admin,
    };
    e.w.store
        .record_restriction_event(&RestrictionEvent {
            id: RestrictionEventId::new(),
            wiki: e.w.wiki.id.clone(),
            at: NOW - 100,
            event: MeasureEvent::Set,
            account: e.reviewer.clone(),
            by,
            policy_block: None,
            reason: Some("just about over".to_owned()),
            expires_at: Some(NOW + 1),
            of_event: None,
            appeal: None,
        })
        .unwrap();
    assert_eq!(
        as_(&e, &e.reviewer, "/handbook/-/views/new").await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn the_restricted_persons_own_refusals_say_the_same_in_a_restricted_wiki_where_they_remain_a_member()
 {
    let w = base(Visibility::Restricted);
    let e = cast(w);
    restrict(&e, &e.contributor).await;
    let a = as_(&e, &e.contributor, "/handbook/-/proposals/new").await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    told(&main_of(&a));
    assert_eq!(
        as_(&e, &e.contributor, "/handbook/Home").await.status,
        StatusCode::OK,
        "they still read"
    );
    // A stranger to the wiki is told nothing, as before.
    let a = as_(&e, &e.newcomer, "/handbook/-/proposals/new").await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert!(!a.body().contains(REASON), "{}", a.body());
}
