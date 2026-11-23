//! Protections on the web (RFC 006 D-7; handoff G-T3): the form of the `moderate` rung that puts one in place, renews it and
//! lifts it, and **the one display** of every protection on the page, in the proposal form and on the proposal's page, in both
//! languages. What a protection refuses is decided by `authorize()` and the acceptance, and is tested there with no template
//! in sight (`inklkunn-core`'s `acceptance::tests::protections`); this file holds the pages to say it.

use axum::http::StatusCode;
use inklkunn_store::{AccountId, GovernanceStore};

use super::{Cast, cast, location, propose};
use crate::common::{APP, Answer, get, get_as, page, post_form, request, send_body};

const PROTECT: &str = "/handbook/-/protect";
const JA: &str = "inklkunn_lang=ja";

async fn get_in(c: &Cast, uri: &str, account: Option<&AccountId>, cookie: Option<&str>) -> Answer {
    let mut headers: Vec<(&str, &str)> = Vec::new();
    if let Some(a) = account {
        headers.push(("x-test-account", a.as_str()));
    }
    if let Some(cookie) = cookie {
        headers.push(("cookie", cookie));
    }
    request(&c.w, APP, "GET", uri, &headers).await
}

/// The form's fields to put a protection of `title` in place.
fn fields<'a>(
    target: &'a str,
    title: &'a str,
    require: &'a str,
    role: &'a str,
    reason: &'a str,
    days: &'a str,
    act: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("target", target),
        ("title", title),
        ("require", require),
        ("role", role),
        ("reason", reason),
        ("days", days),
        ("act", act),
    ]
}

async fn protect(c: &Cast, who: &AccountId, f: &[(&str, &str)]) -> Answer {
    post_form(&c.w, PROTECT, Some(who), f).await
}

fn records(c: &Cast) -> usize {
    c.w.store.protection_events(&c.w.wiki.id, 50).unwrap().len()
}

fn main_of(a: &Answer) -> String {
    let body = a.body();
    let start = body.find("<main").expect("a main landmark");
    let end = body.find("</main>").expect("the end of main");
    body.get(start..end).expect("main").to_owned()
}

#[tokio::test]
async fn a_maintainer_protects_a_page_and_everyone_who_may_read_it_is_told_in_one_display() {
    let c = cast();
    // The form is the `moderate` rung's, and offers what it asks.
    let form = get_as(&c.w, PROTECT, Some(&c.maintainer)).await;
    assert_eq!(form.status, StatusCode::OK, "{}", form.body());
    let body = main_of(&form);
    assert!(body.contains("<h1>Protect a page</h1>"), "{body}");
    assert!(body.contains("name=\"title\""), "{body}");
    assert!(body.contains("Days it lasts, at most 90"), "{body}");
    // A page before it is protected says nothing of protections, and offers the maintainer the way to the form.
    let before = get_as(&c.w, "/handbook/Home", Some(&c.maintainer)).await;
    assert!(!before.body().contains("Protections"));
    assert!(
        before
            .body()
            .contains("<a href=\"/handbook/-/protect?page=Home\">Protect this page</a>")
    );
    assert!(
        !get(&c.w, "/handbook/Home")
            .await
            .body()
            .contains("Protect this page")
    );
    // Put it in place.
    let set = protect(
        &c,
        &c.maintainer,
        &fields(
            "page",
            "Home",
            "review",
            "",
            "Edit war this week.",
            "7",
            "set",
        ),
    )
    .await;
    assert_eq!(location(&set), "/handbook/-/protect?page=Home");
    assert_eq!(records(&c), 1);
    // The page tells every reader, once, with the kind, what it is on, what it asks and why.
    for reader in [None, Some(&c.contributor), Some(&c.observer)] {
        let shown = get_in(&c, "/handbook/Home", reader, None).await;
        assert_eq!(shown.status, StatusCode::OK);
        let body = shown.body();
        assert_eq!(
            body.matches("<li><strong>Temporarily, until ").count(),
            1,
            "{body}"
        );
        for says in [
            "<h2 id=\"protections-heading\">Protections</h2>",
            "On the page Home.",
            "A change here is accepted only after someone other than the proposer has approved it.",
            "Reason: Edit war this week.",
            "Anyone who may propose can still propose a change.",
        ] {
            assert!(body.contains(says), "{says}\n{body}");
        }
    }
    // Another page is as it was.
    assert!(
        !get(&c.w, "/handbook/Guide")
            .await
            .body()
            .contains("Protections")
    );
}

#[tokio::test]
async fn the_display_is_in_both_languages_and_the_reason_is_text_never_markup() {
    let c = cast();
    let set = protect(
        &c,
        &c.maintainer,
        &fields(
            "page",
            "Home",
            "rung",
            "admin",
            "Only <b>the admin</b> & co.",
            "3",
            "set",
        ),
    )
    .await;
    assert_eq!(set.status, StatusCode::SEE_OTHER, "{}", set.body());
    let en = get(&c.w, "/handbook/Home").await.body();
    assert!(
        en.contains("A change here is accepted only by admin or a higher role."),
        "{en}"
    );
    // The reason is the words of whoever wrote it, printed as text.
    assert!(
        en.contains("Reason: Only &#60;b&#62;the admin&#60;/b&#62; &#38; co."),
        "{en}"
    );
    assert!(!en.contains("<b>the admin</b>"));
    let ja = get_in(&c, "/handbook/Home", None, Some(JA)).await.body();
    for says in [
        "<h2 id=\"protections-heading\">保護</h2>",
        "までの一時的な保護",
        "ページ Home が対象です。",
        "ここへの変更を受け入れられるのは、管理者 の役割か、それより上の役割の人だけです。",
        "理由: Only &#60;b&#62;the admin&#60;/b&#62; &#38; co.",
        "提案できる人は、これまでどおり変更を提案できます。",
    ] {
        assert!(ja.contains(says), "{says}\n{ja}");
    }
}

#[tokio::test]
async fn the_proposal_form_and_the_proposals_page_show_the_same_protections_and_a_page_that_has_none_shows_none()
 {
    let c = cast();
    protect(
        &c,
        &c.maintainer,
        &fields("page", "Home", "review", "", "Edit war.", "7", "set"),
    )
    .await;
    // The form of a change to the protected page says what its acceptance will ask, before the person writes.
    let form = get_as(
        &c.w,
        "/handbook/-/proposals/new?page=Home",
        Some(&c.contributor),
    )
    .await;
    assert_eq!(form.status, StatusCode::OK, "{}", form.body());
    let body = main_of(&form);
    assert_eq!(body.matches("Reason: Edit war.").count(), 1, "{body}");
    assert!(body.contains("On the page Home."), "{body}");
    // The form of another page says nothing.
    let other = get_as(
        &c.w,
        "/handbook/-/proposals/new?page=Guide",
        Some(&c.contributor),
    )
    .await;
    assert!(!main_of(&other).contains("Protections"));
    // The proposal's page, for a proposal that touches the protected page.
    let uri = propose(&c, &c.contributor, "Say it better").await;
    let page_of = get_as(&c.w, &uri, Some(&c.maintainer)).await;
    let body = main_of(&page_of);
    assert_eq!(body.matches("Reason: Edit war.").count(), 1, "{body}");
    // In Japanese too.
    let ja = get_in(&c, &uri, Some(&c.maintainer), Some(JA)).await;
    assert!(main_of(&ja).contains("理由: Edit war."), "{}", main_of(&ja));
}

#[tokio::test]
async fn a_namespace_protection_is_shown_on_the_pages_below_it_and_not_on_the_page_of_that_name() {
    let c = cast();
    c.w.fake.set(
        "pages/Help/Start.md",
        &page(&["title: Start"], "How to begin.\n"),
    );
    c.w.fake
        .set("pages/Help.md", &page(&["title: Help"], "The help.\n"));
    let set = protect(
        &c,
        &c.maintainer,
        &fields(
            "namespace",
            "Help",
            "review",
            "",
            "The help is read by everyone.",
            "10",
            "set",
        ),
    )
    .await;
    assert_eq!(location(&set), "/handbook/-/protect?namespace=Help");
    let below = get(&c.w, "/handbook/Help/Start").await.body();
    assert!(below.contains("On every page below Help."), "{below}");
    assert!(
        below.contains("Reason: The help is read by everyone."),
        "{below}"
    );
    assert!(
        !get(&c.w, "/handbook/Help")
            .await
            .body()
            .contains("Protections")
    );
}

#[tokio::test]
async fn only_the_role_that_moderates_may_use_the_form_and_a_refusal_changes_nothing() {
    let c = cast();
    let ask = fields("page", "Home", "review", "", "Why.", "7", "set");
    for who in [&c.contributor, &c.reviewer, &c.observer] {
        let form = get_as(&c.w, PROTECT, Some(who)).await;
        assert_eq!(form.status, StatusCode::FORBIDDEN);
        assert!(
            form.body()
                .contains("Only people who hold the role that moderates")
        );
        let post = protect(&c, who, &ask).await;
        assert_eq!(post.status, StatusCode::FORBIDDEN, "{}", post.body());
    }
    // Someone who is not signed in is asked to sign in, and nothing is offered.
    let anonymous = get(&c.w, PROTECT).await;
    assert_eq!(anonymous.status, StatusCode::FORBIDDEN);
    assert!(!anonymous.body().contains("name=\"title\""));
    assert_eq!(records(&c), 0);
    // The admin holds the rung too.
    assert_eq!(
        protect(&c, &c.admin, &ask).await.status,
        StatusCode::SEE_OTHER
    );
}

#[tokio::test]
async fn a_protection_lasts_at_most_the_platforms_limit_and_the_refusal_names_it() {
    let c = cast();
    let too_long = protect(
        &c,
        &c.maintainer,
        &fields("page", "Home", "review", "", "Why.", "91", "set"),
    )
    .await;
    assert_eq!(too_long.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = main_of(&too_long);
    assert!(
        body.contains("A protection lasts at most 90 days. The person who runs this server sets that limit (INKLKUNN_GOVERNANCE_LIMIT_DAYS)."),
        "{body}"
    );
    // What was typed is still in the form.
    assert!(body.contains("value=\"Home\""), "{body}");
    let body = "target=page&title=Home&require=review&role=&reason=Why.&days=91&act=set".to_owned();
    let japanese = send_body(
        &c.w,
        "POST",
        PROTECT,
        Some(&c.maintainer),
        &[("sec-fetch-site", "same-origin"), ("cookie", JA)],
        body,
    )
    .await;
    assert_eq!(japanese.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = main_of(&japanese);
    assert!(
        body.contains("保護の期間は 90 日までです。この上限は、このサーバーの管理者が設定しています（INKLKUNN_GOVERNANCE_LIMIT_DAYS）。"),
        "{body}"
    );
    // Every other thing that is wrong is said at once.
    let bad = protect(
        &c,
        &c.maintainer,
        &fields("page", "Help/", "rung", "", "", "0", "set"),
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = main_of(&bad);
    for says in [
        "Give a title as the wiki writes it, with no slash at either end.",
        "Choose a role.",
        "Give a reason of one line, at most 300 characters.",
        "Give a whole number of days from 1 to 90.",
    ] {
        assert!(body.contains(says), "{says}\n{body}");
    }
    assert_eq!(records(&c), 0);
    // The limit itself is allowed.
    assert_eq!(
        protect(
            &c,
            &c.maintainer,
            &fields("page", "Home", "review", "", "Why.", "90", "set")
        )
        .await
        .status,
        StatusCode::SEE_OTHER
    );
}

#[tokio::test]
async fn the_form_renews_and_lifts_and_says_when_there_is_nothing_to_do() {
    let c = cast();
    let set = fields("page", "Home", "review", "", "First.", "2", "set");
    assert_eq!(
        protect(&c, &c.maintainer, &set).await.status,
        StatusCode::SEE_OTHER
    );
    // Setting what is in force is refused and says to renew it or lift it.
    let again = protect(&c, &c.maintainer, &set).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert!(
        main_of(&again)
            .contains("A temporary protection is in force here already. Renew it or lift it.")
    );
    // The form now offers to renew and to lift, and says a protection is in force.
    let form = get_as(&c.w, "/handbook/-/protect?page=Home", Some(&c.maintainer)).await;
    let body = main_of(&form);
    assert!(body.contains("value=\"renew\""), "{body}");
    assert!(body.contains("value=\"lift\""), "{body}");
    assert!(!body.contains("value=\"set\""), "{body}");
    assert!(
        body.contains("A temporary protection is in force here."),
        "{body}"
    );
    // Renewing replaces what it asks, its reason and its end; the page shows the one in force.
    let renew = fields(
        "page",
        "Home",
        "rung",
        "maintainer",
        "Second.",
        "30",
        "renew",
    );
    assert_eq!(
        protect(&c, &c.maintainer, &renew).await.status,
        StatusCode::SEE_OTHER
    );
    let shown = get(&c.w, "/handbook/Home").await.body();
    assert_eq!(shown.matches("<li><strong>").count(), 1, "{shown}");
    assert!(shown.contains("Reason: Second."), "{shown}");
    assert!(!shown.contains("Reason: First."), "{shown}");
    assert!(
        shown.contains("accepted only by maintainer or a higher role."),
        "{shown}"
    );
    // Lifting ends it now.
    let lift = fields("page", "Home", "review", "", "", "1", "lift");
    assert_eq!(
        protect(&c, &c.maintainer, &lift).await.status,
        StatusCode::SEE_OTHER
    );
    assert!(
        !get(&c.w, "/handbook/Home")
            .await
            .body()
            .contains("Protections")
    );
    // And there is nothing left to renew or lift.
    for f in [&renew, &lift] {
        let nothing = protect(&c, &c.maintainer, f).await;
        assert_eq!(nothing.status, StatusCode::CONFLICT, "{}", nothing.body());
        assert!(main_of(&nothing).contains("No temporary protection is in force here"));
    }
    // Three records, in order, each following the last.
    let kinds: Vec<_> =
        c.w.store
            .protection_events(&c.w.wiki.id, 50)
            .unwrap()
            .iter()
            .rev()
            .map(|e| e.event.as_str())
            .collect();
    assert_eq!(kinds, ["set", "renewed", "lifted"]);
}

#[tokio::test]
async fn a_form_that_is_not_from_this_site_does_nothing() {
    let c = cast();
    let foreign = request(
        &c.w,
        APP,
        "POST",
        PROTECT,
        &[
            ("x-test-account", c.maintainer.as_str()),
            ("sec-fetch-site", "cross-site"),
        ],
    )
    .await;
    assert_eq!(foreign.status, StatusCode::FORBIDDEN);
    assert_eq!(records(&c), 0);
}
