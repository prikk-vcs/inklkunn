//! One test for each surface of the table (`inklkunn_core::tombstone::surfaces::SURFACES`; the gate checks the names): the
//! hidden parts are hidden from the HTML route and the API member, **for every viewer including the `moderate` rung**, in
//! every byte of the answer and its headers, and the tombstone stands in their place.

use axum::http::StatusCode;
use inklkunn_store::AccountId;

use super::{
    Env, HIDING, borrowed, change, comment_item, env, hide, nothing_hidden_in, page_item,
    proposal_item, read,
};
use crate::common::{self, post_form};

const ALL: &[&str] = &["part_content", "part_summary", "part_author"];

/// Every viewer: signed out, an observer, the author, the moderator.
fn viewers(e: &Env) -> Vec<Option<AccountId>> {
    vec![
        None,
        Some(e.reader.clone()),
        Some(e.author.clone()),
        Some(e.maintainer.clone()),
    ]
}

/// Ask `uri` as every viewer: it answers, says nothing hidden, and (`tombstone`) says why in its place.
async fn shows_the_tombstone(e: &Env, uri: &str, tombstone: &str) {
    for who in viewers(e) {
        let a = read(e, uri, who.as_ref()).await;
        assert_eq!(a.status, StatusCode::OK, "{uri} as {who:?}: {}", a.body());
        nothing_hidden_in(&a, uri);
        let body = a.body();
        assert!(
            body.contains(tombstone),
            "{uri} as {who:?} does not say `{tombstone}`: {body}"
        );
    }
}

async fn hide_change_two(e: &Env) {
    let item = page_item(2);
    let a = hide(e, &borrowed(&item), ALL, "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
}

#[tokio::test]
async fn surface_history_row() {
    let e = env().await;
    hide_change_two(&e).await;
    shows_the_tombstone(&e, "/handbook/-/history/Home", HIDING).await;
    shows_the_tombstone(&e, "/handbook/-/api/v1/history/Home", "\"tombstones\"").await;
}

#[tokio::test]
async fn surface_change_view() {
    let e = env().await;
    hide_change_two(&e).await;
    let b = change(2);
    shows_the_tombstone(&e, &format!("/handbook/-/change/{b}"), HIDING).await;
    shows_the_tombstone(
        &e,
        &format!("/handbook/-/api/v1/revisions/{b}"),
        "\"tombstones\"",
    )
    .await;
    // The page as of the change, given as a member of the change.
    shows_the_tombstone(
        &e,
        &format!("/handbook/-/api/v1/revisions/{b}?title=Home"),
        "\"hidden\"",
    )
    .await;
}

#[tokio::test]
async fn surface_diff() {
    let e = env().await;
    hide_change_two(&e).await;
    let (a, b) = (change(2), change(3));
    shows_the_tombstone(&e, &format!("/handbook/-/diff/{a}/{b}/Home"), HIDING).await;
    shows_the_tombstone(
        &e,
        &format!("/handbook/-/api/v1/diff?from={a}&to={b}&path=Home"),
        "\"hidden\"",
    )
    .await;
}

#[tokio::test]
async fn surface_page_as_of() {
    let e = env().await;
    // Before: the permalink shows the text it had.
    let before = read(&e, &format!("/handbook/-/at/{}/Home", change(2)), None).await;
    assert!(before.body().contains("ZEBRATEXT"), "{}", before.body());
    hide_change_two(&e).await;
    shows_the_tombstone(&e, &format!("/handbook/-/at/{}/Home", change(2)), HIDING).await;
}

#[tokio::test]
async fn surface_why_is_this_here() {
    let e = env().await;
    hide_change_two(&e).await;
    // *Why is this here?* and the API's provenance never quote what is hidden, whatever they can say of the page.
    for uri in ["/handbook/-/why/Home", "/handbook/-/api/v1/provenance/Home"] {
        for who in viewers(&e) {
            let a = read(&e, uri, who.as_ref()).await;
            nothing_hidden_in(&a, uri);
        }
    }
}

#[tokio::test]
async fn surface_compare_views() {
    let e = env().await;
    // A view whose tip is change 2: the text of change 2 is a tip's, so it cannot be hidden, in any view.
    e.w.fake.add_view_at("alt", 2);
    let item = page_item(2);
    let refused = hide(&e, &borrowed(&item), &["part_content"], "").await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body());
    assert!(
        refused.body().contains("current text"),
        "{}",
        refused.body()
    );
    // Its summary and author can be hidden, and the comparison names neither.
    let a = hide(&e, &borrowed(&item), &["part_summary", "part_author"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    for uri in [
        "/handbook/-/views/compare?a=main&b=alt",
        "/handbook/-/api/v1/views/compare?b=alt",
    ] {
        for who in viewers(&e) {
            let a = read(&e, uri, who.as_ref()).await;
            let all = format!("{:?}{}", a.headers, a.body());
            for word in ["ZEBRASUMMARY", "ZEBRA\"", "Zed"] {
                assert!(!all.contains(word), "{uri}: `{word}` is in the comparison");
            }
        }
    }
}

#[tokio::test]
async fn surface_recent_changes() {
    let e = env().await;
    hide_change_two(&e).await;
    shows_the_tombstone(&e, "/handbook/-/changes", HIDING).await;
    shows_the_tombstone(&e, "/handbook/-/changes.atom", HIDING).await;
    shows_the_tombstone(&e, "/handbook/-/api/v1/changes", "\"tombstones\"").await;
}

#[tokio::test]
async fn surface_proposal_page() {
    let e = env().await;
    // The accepted proposal's revision and the page's change are one content: one act hides both.
    let item = proposal_item(&e.accepted, 1);
    let a = hide(&e, &borrowed(&item), ALL, "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let id = &e.accepted;
    shows_the_tombstone(&e, &format!("/handbook/-/proposals/{id}"), HIDING).await;
    shows_the_tombstone(
        &e,
        &format!("/handbook/-/api/v1/proposals/{id}"),
        "\"tombstones\"",
    )
    .await;
    // The same act hid the page's own change, in the history.
    shows_the_tombstone(&e, "/handbook/-/history/Home", HIDING).await;
    shows_the_tombstone(&e, &format!("/handbook/-/at/{}/Home", change(2)), HIDING).await;
}

#[tokio::test]
async fn surface_proposal_comments() {
    let e = env().await;
    let item = comment_item(&e.open, &e.comment);
    let a = hide(&e, &borrowed(&item), &["part_content", "part_author"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    shows_the_tombstone(&e, &format!("/handbook/-/proposals/{}", e.open), HIDING).await;
    // The author's name is not in the thread of the proposal, though the proposal itself still names them.
    let body = read(&e, &format!("/handbook/-/proposals/{}", e.open), None)
        .await
        .body();
    assert!(body.contains("This comment is hidden."), "{body}");
}

#[tokio::test]
async fn surface_proposal_list() {
    let e = env().await;
    let item = proposal_item(&e.accepted, 1);
    let a = hide(&e, &borrowed(&item), &["part_summary", "part_author"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    // The discussion of the page carries the comments of the proposals that touch it: the hidden one is hidden there too.
    let item = comment_item(&e.open, &e.comment);
    let a = hide(&e, &borrowed(&item), &["part_content", "part_author"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    for uri in ["/handbook/-/proposals", "/handbook/-/discussion/Home"] {
        for who in viewers(&e) {
            let a = read(&e, uri, who.as_ref()).await;
            assert_eq!(a.status, StatusCode::OK, "{uri}");
            nothing_hidden_in(&a, uri);
        }
    }
    let list = read(&e, "/handbook/-/proposals", None).await.body();
    assert!(list.contains("(summary hidden)"), "{list}");
    assert!(list.contains("(hidden)"), "{list}");
}

#[tokio::test]
async fn surface_inbox() {
    let e = env().await;
    // The maintainer answers the author's open proposal: the author is told, naming the proposal by its summary.
    let said = common_comment(&e).await;
    assert_eq!(said, StatusCode::SEE_OTHER);
    let before = read(&e, "/-/inbox", Some(&e.other)).await.body();
    assert!(before.contains("An open proposal"), "{before}");
    let item = proposal_item(&e.open, 1);
    let a = hide(&e, &borrowed(&item), &["part_summary"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let after = read(&e, "/-/inbox", Some(&e.other)).await;
    assert_eq!(after.status, StatusCode::OK);
    assert!(
        !after.body().contains("An open proposal"),
        "{}",
        after.body()
    );
    assert!(
        after.body().contains("(summary hidden)"),
        "{}",
        after.body()
    );
}

async fn common_comment(e: &Env) -> StatusCode {
    crate::common::post_form(
        &e.w,
        &format!("/handbook/-/proposals/{}/comment", e.open),
        Some(&e.maintainer),
        &[("body", "Thanks, looking.")],
    )
    .await
    .status
}

#[tokio::test]
async fn surface_claim_thread() {
    use common_claim::*;
    let c = claim_cast();
    let id = say(&c, "ZEBRACLAIM said in the thread").await;
    let uri = format!("/handbook/-/claim/{}", common::claim_id(1));
    let before = get_as(&c.w, &uri, None).await.body();
    assert!(before.contains("ZEBRACLAIM"), "{before}");
    let a = post_form(
        &c.w,
        "/handbook/-/moderate",
        Some(&c.maintainer),
        &[
            ("kind", "claim-comment"),
            ("claim", &common::claim_id(1).to_string()),
            ("comment", &id),
            ("part_content", "1"),
            ("part_author", "1"),
            ("category", "spam"),
            ("note", ""),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    for who in [None, Some(&c.maintainer), Some(&c.questioner)] {
        let a = get_as(&c.w, &uri, who).await;
        assert_eq!(a.status, StatusCode::OK);
        let all = format!("{:?}{}", a.headers, a.body());
        assert!(!all.contains("ZEBRACLAIM"), "{all}");
        assert!(!all.contains("Zed Zebra"), "{all}");
        assert!(a.body().contains("This comment is hidden."), "{}", a.body());
        assert!(a.body().contains(HIDING), "{}", a.body());
    }
}

mod common_claim {
    pub(super) use crate::common::get_as;
    use crate::common::{
        self, Answer, World, add_person, claim, home_with, person, post_form, put, with_accounts,
    };
    use inklkunn_core::knowledge::Standing;
    use inklkunn_store::{AccountId, Role};

    const SAID: &str = "The bridge opened in 1950.";

    pub(super) struct Cast {
        pub(super) w: World,
        pub(super) questioner: AccountId,
        pub(super) maintainer: AccountId,
    }

    pub(super) fn claim_cast() -> Cast {
        let w = World::new(
            "handbook",
            "The Handbook",
            &[("pages/Home.md", &home_with(SAID))],
        );
        put(
            &w,
            &home_with(SAID),
            &[claim(1, Standing::Contested, None, SAID)],
            &[],
        );
        let w = with_accounts(w);
        let c = Cast {
            questioner: person(1),
            maintainer: person(4),
            w,
        };
        add_person(&c.w, &c.questioner, "Zed Zebra", Some(Role::Questioner));
        add_person(&c.w, &c.maintainer, "Mika Ito", Some(Role::Maintainer));
        c
    }

    /// The questioner writes a comment in the thread of claim 1; its id.
    pub(super) async fn say(c: &Cast, text: &str) -> String {
        let a: Answer = post_form(
            &c.w,
            &format!("/handbook/-/claim/{}/discuss", common::claim_id(1)),
            Some(&c.questioner),
            &[("body", text)],
        )
        .await;
        assert_eq!(a.status, axum::http::StatusCode::SEE_OTHER, "{}", a.body());
        use inklkunn_store::ClaimThreadStore;
        c.w.store
            .claim_comments(&c.w.wiki.id, &common::claim_id(1).to_string(), None, 10)
            .unwrap()
            .rows
            .first()
            .expect("a comment")
            .comment
            .id
            .to_string()
    }
}

#[tokio::test]
async fn surface_removed_page() {
    super::removed::surface_removed_page().await;
}

#[tokio::test]
async fn surface_restore_offer() {
    super::removed::surface_restore_offer().await;
}

#[tokio::test]
async fn surface_rules_history() {
    let e = env().await;
    // A change of the rules, made by the accepted proposal: the page of the rules names the proposal by its summary.
    e.w.fake
        .set_files(&[("governance/policy.yaml", "entry: observer\n")]);
    e.w.fake
        .say(4, &super::message(&e.accepted, &e.author, &e.maintainer));
    e.w.fake.touches(&change(4), &["governance/policy.yaml"]);
    let before = read(&e, "/handbook/-/policy", None).await;
    assert_eq!(before.status, StatusCode::OK, "{}", before.body());
    assert!(before.body().contains(super::SUMMARY), "{}", before.body());
    // Hide the proposal's summary and author: the page says the summary is hidden, to every viewer, the moderator included.
    let item = proposal_item(&e.accepted, 1);
    let a = hide(&e, &borrowed(&item), &["part_summary", "part_author"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    for who in viewers(&e) {
        let a = read(&e, "/handbook/-/policy", who.as_ref()).await;
        assert_eq!(a.status, StatusCode::OK, "{who:?}: {}", a.body());
        nothing_hidden_in(&a, "/handbook/-/policy");
        assert!(
            a.body().contains("(summary hidden)"),
            "{who:?}: {}",
            a.body()
        );
    }
}
