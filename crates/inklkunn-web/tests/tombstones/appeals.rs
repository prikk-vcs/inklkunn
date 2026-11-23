//! Appeals against a hiding (RFC 006 D-9; handoff G-T5; acceptance criteria 8 and 9; threat model C-14f, C-14g): the author is
//! offered *contest this* on the placeholder of what was hidden; an overturn **restores every surface exactly, byte for byte**, and
//! brings a proposal the hiding declined back to review; the moderation log says only that the hiding was appealed and how it
//! ended, never the words and never a name; and what the author wrote is read by them and by the rung that decides, and by no one
//! else.

use axum::http::StatusCode;
use inklkunn_store::{AccountId, AppealState, GovernanceStore, ProposalState, ProposalStore, Role};

use super::acts::pages_of_change_two;
use super::{AUTHOR, Env, borrowed, comment_item, env, hide, page_item, proposal_item, read};
use crate::common::{add_person, person, post_form};

const GRIEVANCE: &str = "ZEBRAGRIEVANCE I wrote this myself and it names nobody.";
const REASON: &str = "ZEBRAREASON The hiding was wrong.";

fn admin(e: &Env) -> AccountId {
    let admin = person(9);
    add_person(&e.w, &admin, "Ada Admin", Some(Role::Admin));
    admin
}

async fn file(e: &Env, who: &AccountId, hide_id: &str) -> axum::http::StatusCode {
    post_form(
        &e.w,
        "/handbook/-/appeals/new",
        Some(who),
        &[("kind", "tombstone"), ("id", hide_id), ("text", GRIEVANCE)],
    )
    .await
    .status
}

fn hides(e: &Env) -> Vec<inklkunn_store::TombstoneEvent> {
    e.w.store
        .tombstone_log(&e.w.wiki.id, None, 50)
        .expect("the log")
        .into_iter()
        .filter(|ev| ev.act == inklkunn_store::TombstoneAct::Hide)
        .collect()
}

fn open_appeal(e: &Env) -> inklkunn_store::AppealRecord {
    e.w.store
        .open_appeals(&e.w.wiki.id, 10)
        .unwrap()
        .into_iter()
        .next()
        .expect("an open appeal")
}

#[tokio::test]
async fn an_overturned_hiding_restores_every_surface_byte_for_byte_and_the_log_says_only_that_it_was_appealed_and_how_it_ended()
 {
    let e = env().await;
    let ada = admin(&e);
    let before = pages_of_change_two(&e, None).await;
    let item = page_item(2);
    let all = ["part_content", "part_summary", "part_author"];
    let a = hide(&e, &borrowed(&item), &all, "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert_ne!(before, pages_of_change_two(&e, None).await);
    let made = hides(&e);
    assert!(
        made.len() >= 2,
        "a change's text and its author are two records"
    );
    let first = made.first().unwrap();

    // The author files: a form with no script, in their own words.
    let form = read(
        &e,
        &format!("/handbook/-/appeals/new?kind=tombstone&id={}", first.id),
        Some(&e.author),
    )
    .await;
    assert_eq!(form.status, StatusCode::OK, "{}", form.body());
    assert!(form.body().contains("<textarea"), "{}", form.body());
    assert!(form.body().contains("name=\"csrf\""), "{}", form.body());
    assert!(!form.body().contains("<script"), "{}", form.body());
    assert_eq!(
        file(&e, &e.author, first.id.as_str()).await,
        StatusCode::SEE_OTHER
    );
    let appeal = open_appeal(&e);
    assert_eq!(appeal.text, GRIEVANCE);

    // The log, to anyone who reads the wiki: that it was appealed, and nothing else.
    let log = read(&e, "/handbook/-/moderation", None).await.body();
    assert!(
        log.contains("This hiding was appealed. The appeal is waiting for a decision."),
        "{log}"
    );
    assert!(!log.contains("ZEBRAGRIEVANCE"), "{log}");
    assert!(!log.contains(AUTHOR), "{log}");

    // The deciders decide: an admin who did not hide it overturns it, with a reason the author is told.
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/decide",
        Some(&ada),
        &[
            ("appeal", appeal.id.as_str()),
            ("decision", "overturned"),
            ("reason", REASON),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/appeals?done=overturned")
    );
    assert_eq!(
        before,
        pages_of_change_two(&e, None).await,
        "every surface of the change is as it was, byte for byte"
    );
    assert!(matches!(
        e.w.store.appeal(&appeal.id).unwrap().unwrap().state,
        AppealState::Overturned(..)
    ));
    let log = read(&e, "/handbook/-/moderation", None).await.body();
    assert!(
        log.contains("This hiding was appealed. The appeal was decided: the hiding was reversed."),
        "{log}"
    );
    assert!(
        log.contains("Restored because an appeal was decided."),
        "{log}"
    );
    assert!(
        !log.contains("ZEBRAGRIEVANCE") && !log.contains("ZEBRAREASON"),
        "{log}"
    );
    assert!(!log.contains(AUTHOR), "{log}");
}

#[tokio::test]
async fn an_appeals_words_are_read_by_the_author_and_the_deciders_page_and_nowhere_else() {
    let e = env().await;
    let ada = admin(&e);
    let item = comment_item(&e.open, &e.comment);
    let a = hide(&e, &borrowed(&item), &["part_content"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let hide_id = hides(&e).first().unwrap().id.clone();
    // The comment was the author's (Zed), and `e.author` wrote it.
    assert_eq!(
        file(&e, &e.author, hide_id.as_str()).await,
        StatusCode::SEE_OTHER
    );

    // Read: the author's own page and the deciders' page.
    let mine = read(&e, "/handbook/-/appeals/mine", Some(&e.author)).await;
    assert_eq!(mine.status, StatusCode::OK);
    assert!(mine.body().contains("ZEBRAGRIEVANCE"), "{}", mine.body());
    let queue = read(&e, "/handbook/-/appeals", Some(&ada)).await;
    assert_eq!(queue.status, StatusCode::OK);
    assert!(queue.body().contains("ZEBRAGRIEVANCE"), "{}", queue.body());
    assert!(
        queue.body().contains(AUTHOR),
        "the decider sees who appealed"
    );

    // Everyone else reads nothing: another contributor, the maintainer who hid it, a reader, and a visitor.
    let all_else = [Some(&e.other), Some(&e.maintainer), Some(&e.reader), None];
    for who in all_else {
        for uri in [
            "/handbook/-/appeals",
            "/handbook/-/appeals/mine",
            "/handbook/-/moderation",
            "/handbook/-/people",
            &format!("/handbook/-/proposals/{}", e.open),
        ] {
            let a = read(&e, uri, who).await;
            assert!(
                !a.body().contains("ZEBRAGRIEVANCE"),
                "{who:?} reads the words at {uri}: {}",
                a.body()
            );
        }
        let queue = read(&e, "/handbook/-/appeals", who).await;
        assert!(
            queue.status.is_client_error(),
            "{who:?}: the deciders' page is not theirs: {}",
            queue.status
        );
    }
    // And they cannot appeal what is not theirs: a bystander posts the hiding's id and is told what is told of nothing.
    assert_eq!(
        file(&e, &e.other, hide_id.as_str()).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        file(&e, &e.reader, hide_id.as_str()).await,
        StatusCode::NOT_FOUND
    );
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/new",
        None,
        &[
            ("kind", "tombstone"),
            ("id", hide_id.as_str()),
            ("text", GRIEVANCE),
        ],
    )
    .await;
    assert!(a.status.is_client_error(), "{}", a.status);
    assert_eq!(e.w.store.open_appeals(&e.w.wiki.id, 10).unwrap().len(), 1);
}

#[tokio::test]
async fn the_author_is_offered_contest_this_beside_the_tombstone_of_their_own_comment_and_nobody_else_is()
 {
    let e = env().await;
    let item = comment_item(&e.open, &e.comment);
    let a = hide(&e, &borrowed(&item), &["part_content"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let hide_id = hides(&e).first().unwrap().id.clone();
    let link = format!("/handbook/-/appeals/new?kind=tombstone&id={hide_id}");
    let uri = format!("/handbook/-/proposals/{}", e.open);
    for (who, sees) in [
        (Some(&e.author), true),
        (Some(&e.other), false),
        (Some(&e.maintainer), false),
        (Some(&e.reader), false),
        (None, false),
    ] {
        let body = read(&e, &uri, who).await.body().replace("&#38;", "&");
        assert_eq!(body.contains(&link), sees, "{who:?}: {body}");
        assert!(body.contains("Contest this") == sees, "{who:?}: {body}");
    }
    // Once appealed, the link is gone: one open appeal per action per person.
    assert_eq!(
        file(&e, &e.author, hide_id.as_str()).await,
        StatusCode::SEE_OTHER
    );
    let body = read(&e, &uri, Some(&e.author)).await.body();
    assert!(!body.contains("Contest this"), "{body}");
    let again = file(&e, &e.author, hide_id.as_str()).await;
    assert_eq!(again, StatusCode::CONFLICT);
}

#[tokio::test]
async fn an_overturned_hiding_that_declined_an_open_proposal_brings_it_back_to_review() {
    let e = env().await;
    let ada = admin(&e);
    let item = proposal_item(&e.open, 1);
    let a = hide(&e, &borrowed(&item), &["part_content"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert_eq!(
        e.w.store.proposal(&e.open, None).unwrap().unwrap().state,
        ProposalState::Declined
    );
    // The proposal's author is `other`: the hiding is theirs to contest.
    let hide_id = hides(&e).first().unwrap().id.clone();
    assert_eq!(
        file(&e, &e.other, hide_id.as_str()).await,
        StatusCode::SEE_OTHER
    );
    let appeal = open_appeal(&e);
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/decide",
        Some(&ada),
        &[
            ("appeal", appeal.id.as_str()),
            ("decision", "overturned"),
            ("reason", REASON),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/appeals?done=reopened")
    );
    assert_eq!(
        e.w.store.proposal(&e.open, None).unwrap().unwrap().state,
        ProposalState::HeldForReview,
        "the hiding's decline is part of the hiding"
    );
    let page = read(&e, "/handbook/-/appeals?done=reopened", Some(&ada))
        .await
        .body();
    assert!(
        page.contains("The proposal is held for review again."),
        "{page}"
    );
}
