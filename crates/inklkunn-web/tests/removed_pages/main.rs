//! A removed page (RFC 006 D-10; handoff G-T6; acceptance criterion 10) through the real router, over an in-memory repository with a
//! scripted history: a page removed by an accepted proposal answers `410 Gone` with its page (when, by which proposal, why, and the
//! way back), a title that never was a page stays a `404`, a restricted wiki answers a stranger as an absent wiki does, and the way
//! back is a draft proposal that is a rollback when it merges cleanly and the page's last text when later changes block it. The
//! real prikk, end to end, is in `inklkunn-core/tests/soft_deletion.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use common::{
    APP, CONTENT, World, accept_by_hand, add_person, block_id, get, get_as, op, page, person,
    post_form, request, shown, with_accounts,
};
use inklkunn_store::{AccountId, FileChange, Role, Visibility};

const GUIDE: &str = "pages/Guide.md";
const HOME: &str = "pages/Home.md";

fn home(body: &str) -> String {
    page(&["title: Home"], body)
}

fn guide(body: &str) -> String {
    page(&["title: Guide"], body)
}

/// The message of an accepted proposal `id`, in the form the service writes.
fn message(summary: &str, id: &inklkunn_store::ProposalId) -> String {
    use inklkunn_core::message::{Actor, BlockId, Message, Summary};
    use inklkunn_store::DisplayName;
    let actor = |n: &str, name: &str| Actor::Account {
        id: AccountId::parse(&format!("019a0000-0000-7000-8000-0000000000{n}")).expect("id"),
        name: DisplayName::new(name).expect("name"),
    };
    Message::proposal(
        Summary::new(summary).expect("summary"),
        id.clone(),
        actor("a1", "Mika Tanaka"),
        actor("a2", "Ren Sato"),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a proposal message")
    .to_text()
}

/// A wiki whose history is: 1 the creation (`Home`, `Guide`), 2 an edit of `Home`, 3 the removal of `Guide` by an accepted proposal
/// that this wiki keeps. Returns the world and the proposal.
fn handbook(visibility: Visibility, slug: &str) -> (World, inklkunn_store::ProposalId) {
    scripted(visibility, slug, false)
}

/// [`handbook`]; with `also_edits`, the removing change also edits `Home` (so a later edit of the same place blocks its rollback).
fn scripted(
    visibility: Visibility,
    slug: &str,
    also_edits: bool,
) -> (World, inklkunn_store::ProposalId) {
    let w = World::with_visibility(
        slug,
        "The Handbook",
        visibility,
        &[(HOME, &home("Welcome.\n")), (GUIDE, &guide("The guide.\n"))],
    );
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![
            op("create-file", &[HOME]),
            op("create-file", &[GUIDE]),
        ]),
    );
    w.fake.set(HOME, &home("Welcome again.\n"));
    w.fake.say(2, "Improve the welcome");
    w.fake.show(2, shown(vec![op("edit-text", &[HOME])]));
    let author = person(1);
    add_person(&w, &author, "Mika Tanaka", Some(Role::Contributor));
    add_person(&w, &person(2), "Ren Sato", Some(Role::Maintainer));
    let record = w.hold_unjudged(
        &author,
        "Remove the guide: it is out of date",
        vec![FileChange {
            path: GUIDE.to_owned(),
            text: None,
        }],
    );
    accept_by_hand(&w, &record.id, &person(2), &author);
    if also_edits {
        w.fake
            .apply(&[(HOME, &home("Welcome again, tidied.\n"))], &[GUIDE]);
        w.fake.show(
            3,
            shown(vec![op("delete-node", &[GUIDE]), op("edit-text", &[HOME])]),
        );
    } else {
        w.fake.remove(GUIDE);
        w.fake.show(3, shown(vec![op("delete-node", &[GUIDE])]));
    }
    w.fake.say(
        3,
        &message("Remove the guide: it is out of date", &record.id),
    );
    (w, record.id)
}

fn removed_handbook() -> (World, inklkunn_store::ProposalId) {
    let (w, id) = handbook(Visibility::Public, "handbook");
    (with_accounts(w), id)
}

/// One member of a problem document.
fn member<'a>(json: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    json.get(name)
        .unwrap_or_else(|| panic!("no member `{name}` in {json}"))
}

/// What is inside `<main>`: the language buttons in the footer echo the address asked for.
fn main_of(body: &str) -> String {
    let from = body.find("<main").expect("a main");
    let to = body.find("</main>").expect("a main end");
    body.get(from..to).expect("main").to_owned()
}

#[tokio::test]
async fn a_removed_page_answers_410_with_when_which_proposal_and_why() {
    let (w, proposal) = removed_handbook();
    let a = get(&w, "/handbook/Guide").await;
    assert_eq!(a.status, 410, "{}", a.body());
    let body = a.body();
    assert!(body.contains("This page was removed"), "{body}");
    assert!(
        body.contains("The page Guide was removed from The Handbook."),
        "{body}"
    );
    // When: the change, and the moment its proposal was accepted.
    assert!(body.contains("It was removed by change 3."), "{body}");
    assert!(
        body.contains("The proposal that made the change was accepted on 2023-11-14 22:13 UTC."),
        "{body}"
    );
    // Why: the proposal's own summary, as the history shows it.
    assert!(
        body.contains("Reason given: Remove the guide: it is out of date"),
        "{body}"
    );
    // Which proposal, and the change.
    assert!(
        body.contains(&format!("/handbook/-/proposals/{proposal}")),
        "{body}"
    );
    assert!(
        body.contains(&format!("/handbook/-/change/{}", block_id(3))),
        "{body}"
    );
    assert!(body.contains("/handbook/-/history/Guide"), "{body}");
    // Never the page that invites a new page.
    assert!(!body.contains("This page does not exist yet"), "{body}");
}

#[tokio::test]
async fn a_title_that_never_was_a_page_is_a_404_and_a_moved_page_is_not_a_removed_one() {
    let (w, _) = removed_handbook();
    let never = get(&w, "/handbook/Nothing%20Here").await;
    assert_eq!(never.status, 404, "{}", never.body());
    assert!(never.body().contains("This page does not exist yet"));
    assert!(!never.body().contains("This page was removed"));
    // A page that was moved away (its last change is the move) is not a removed page.
    w.fake.set_files(&[("pages/Manual.md", &guide("Moved.\n"))]);
    w.fake.say(4, "Move the page");
    w.fake.show(
        4,
        shown(vec![op(
            "rename-path",
            &["pages/Draft.md", "pages/Manual.md"],
        )]),
    );
    let moved = get(&w, "/handbook/Draft").await;
    assert_eq!(moved.status, 404, "{}", moved.body());
}

#[tokio::test]
async fn the_api_member_is_a_410_problem_with_when_which_proposal_and_why() {
    let (w, proposal) = removed_handbook();
    let a = get(&w, "/handbook/-/api/v1/pages/Guide").await;
    assert_eq!(a.status, 410, "{}", a.body());
    let json: serde_json::Value = serde_json::from_slice(&a.bytes).unwrap();
    assert_eq!(member(&json, "type"), "urn:inklkunn:problem:page-gone");
    assert_eq!(member(&json, "status"), 410);
    assert_eq!(*member(&json, "change_id"), block_id(3));
    assert_eq!(*member(&json, "proposal"), proposal.to_string());
    assert_eq!(
        member(&json, "reason"),
        "Remove the guide: it is out of date"
    );
    assert_eq!(member(&json, "restore"), "rollback");
    assert!(
        member(&json, "detail")
            .as_str()
            .unwrap()
            .contains("The page Guide was removed by change 3."),
        "{json}"
    );
    let never = get(&w, "/handbook/-/api/v1/pages/Nothing%20Here").await;
    assert_eq!(never.status, 404);
    let json: serde_json::Value = serde_json::from_slice(&never.bytes).unwrap();
    assert_eq!(member(&json, "type"), "urn:inklkunn:problem:page-not-found");
}

#[tokio::test]
async fn the_raw_source_of_a_removed_page_is_a_410_and_a_never_page_a_404() {
    let (w, _) = removed_handbook();
    let gone = request(&w, CONTENT, "GET", "/handbook/-/raw/Guide", &[]).await;
    assert_eq!(gone.status, 410, "{}", gone.body());
    assert!(gone.body().contains("This page was removed."));
    assert!(
        gone.header("link")
            .is_some_and(|l| l.contains("/handbook/Guide")),
        "{:?}",
        gone.header("link")
    );
    let never = request(&w, CONTENT, "GET", "/handbook/-/raw/Nothing%20Here", &[]).await;
    assert_eq!(never.status, 404);
    // The application origin serves no source, whatever the page's state.
    let refused = request(&w, APP, "GET", "/handbook/-/raw/Guide", &[]).await;
    assert_eq!(refused.status, 404);
}

#[tokio::test]
async fn the_history_of_a_removed_page_shows_the_removal() {
    let (w, _) = removed_handbook();
    let a = get(&w, "/handbook/-/history/Guide").await;
    assert_eq!(a.status, 200, "{}", a.body());
    let body = a.body();
    assert!(body.contains("The page was removed"), "{body}");
    assert!(body.contains("Change 3"), "{body}");
    assert!(body.contains("Change 1"), "{body}");
}

#[tokio::test]
async fn a_restricted_wiki_answers_a_stranger_as_an_absent_wiki_and_a_member_with_the_410_page() {
    let (w, _) = handbook(Visibility::Restricted, "handbook");
    let w = with_accounts(w);
    let stranger = get(&w, "/handbook/Guide").await;
    let absent = get(&w, "/no-such-wiki/Guide").await;
    assert_eq!(stranger.status, 404);
    // Byte for byte, but for the wiki's own address that the language buttons echo.
    assert_eq!(
        stranger.body().replace("handbook", "X"),
        absent.body().replace("no-such-wiki", "X")
    );
    assert!(!stranger.body().contains("removed"), "{}", stranger.body());
    // The raw source and the API's page answer the same way.
    for uri in [
        "/handbook/-/api/v1/pages/Guide",
        "/handbook/-/restore/Guide",
    ] {
        let a = get(&w, uri).await;
        let b = get(&w, &uri.replace("handbook", "no-such-wiki")).await;
        assert_eq!(a.status, b.status, "{uri}");
        assert_eq!(
            a.body().replace("handbook", "X"),
            b.body().replace("no-such-wiki", "X"),
            "{uri}"
        );
    }
    // A member who may read it is told.
    let member = person(3);
    add_person(&w, &member, "A reader", Some(Role::Observer));
    let a = get_as(&w, "/handbook/Guide", Some(&member)).await;
    assert_eq!(a.status, 410, "{}", a.body());
    assert!(a.body().contains("This page was removed"));
    // A never-existing title is still the page that invites, for the member.
    let never = get_as(&w, "/handbook/Nothing", Some(&member)).await;
    assert_eq!(never.status, 404);
}

#[tokio::test]
async fn whoever_may_propose_is_offered_the_restoring_proposal_and_whoever_may_not_is_told_why() {
    let (w, _) = removed_handbook();
    let contributor = person(1);
    let a = get_as(&w, "/handbook/Guide", Some(&contributor)).await;
    assert_eq!(a.status, 410);
    assert!(
        a.body().contains("Propose restoring this page"),
        "{}",
        a.body()
    );
    assert!(
        a.body().contains("/handbook/-/restore/Guide"),
        "{}",
        a.body()
    );
    assert!(
        a.body()
            .contains("Restoring it is a proposal that takes that change back."),
        "{}",
        a.body()
    );
    // An observer may read and may not propose: no link, and the reason in the refusal's own words.
    let observer = person(9);
    add_person(&w, &observer, "A reader", Some(Role::Observer));
    let b = get_as(&w, "/handbook/Guide", Some(&observer)).await;
    assert_eq!(b.status, 410);
    assert!(
        !b.body().contains("Propose restoring this page"),
        "{}",
        b.body()
    );
    assert!(!b.body().contains("/handbook/-/restore/"), "{}", b.body());
    assert!(
        b.body().contains("you cannot propose changes in this wiki"),
        "{}",
        b.body()
    );
    // The form and the act refuse them too.
    let form = get_as(&w, "/handbook/-/restore/Guide", Some(&observer)).await;
    assert_eq!(form.status, 403, "{}", form.body());
    let post = post_form(
        &w,
        "/handbook/-/restore/Guide",
        Some(&observer),
        &[("mode", "rollback")],
    )
    .await;
    assert_eq!(post.status, 403, "{}", post.body());
}

#[tokio::test]
async fn the_restore_form_makes_a_draft_rollback_when_the_change_merges_cleanly() {
    let (w, _) = removed_handbook();
    let contributor = person(1);
    let form = get_as(&w, "/handbook/-/restore/Guide", Some(&contributor)).await;
    assert_eq!(form.status, 200, "{}", form.body());
    let body = form.body();
    assert!(body.contains("Propose restoring Guide"), "{body}");
    assert!(
        body.contains("takes back change 3, which removed the page"),
        "{body}"
    );
    assert!(body.contains("value=\"rollback\""), "{body}");
    let made = post_form(
        &w,
        "/handbook/-/restore/Guide",
        Some(&contributor),
        &[("mode", "rollback")],
    )
    .await;
    assert_eq!(made.status, 303, "{}", made.body());
    let at = made.header("location").expect("a location").to_owned();
    assert!(at.starts_with("/handbook/-/proposals/"), "{at}");
    // A draft of the author's own, a rollback of the removal that writes the page again.
    let shown = get_as(&w, &at, Some(&contributor)).await;
    assert_eq!(shown.status, 200, "{}", shown.body());
    assert!(shown.body().contains("Draft"), "{}", shown.body());
    assert!(
        shown
            .body()
            .contains("Undo: Remove the guide: it is out of date"),
        "{}",
        shown.body()
    );
    assert!(shown.body().contains("Guide"), "{}", shown.body());
}

#[tokio::test]
async fn the_restore_form_offers_the_last_text_when_later_changes_block_the_rollback() {
    // The removal also edited `Home`, and a later change edited the same place: taking the change back would overlap.
    let (w, _) = scripted(Visibility::Public, "handbook", true);
    let w = with_accounts(w);
    w.fake.set(HOME, &home("Welcome again, edited later.\n"));
    w.fake.say(4, "Edit the welcome");
    w.fake.show(4, shown(vec![op("edit-text", &[HOME])]));
    let contributor = person(1);
    let a = get_as(&w, "/handbook/Guide", Some(&contributor)).await;
    assert_eq!(a.status, 410, "{}", a.body());
    assert!(
        a.body()
            .contains("Later changes stand in the way of taking that change back"),
        "{}",
        a.body()
    );
    let form = get_as(&w, "/handbook/-/restore/Guide", Some(&contributor)).await;
    assert_eq!(form.status, 200, "{}", form.body());
    assert!(
        form.body()
            .contains("writes the page again with the text it had before change 3 removed it"),
        "{}",
        form.body()
    );
    assert!(
        form.body().contains("value=\"last-text\""),
        "{}",
        form.body()
    );
    // A click on the way that is no longer the one offered is refused and shown again.
    let stale = post_form(
        &w,
        "/handbook/-/restore/Guide",
        Some(&contributor),
        &[("mode", "rollback")],
    )
    .await;
    assert_eq!(stale.status, 409, "{}", stale.body());
    assert!(stale.body().contains("changed after you opened this form"));
    let made = post_form(
        &w,
        "/handbook/-/restore/Guide",
        Some(&contributor),
        &[("mode", "last-text")],
    )
    .await;
    assert_eq!(made.status, 303, "{}", made.body());
    let at = made.header("location").expect("a location").to_owned();
    let shown = get_as(&w, &at, Some(&contributor)).await;
    assert_eq!(shown.status, 200, "{}", shown.body());
    assert!(shown.body().contains("Restore: Guide"), "{}", shown.body());
    assert!(shown.body().contains("The guide."), "{}", shown.body());
}

#[tokio::test]
async fn the_restore_form_of_a_page_that_is_not_removed_is_what_the_page_is() {
    let (w, _) = removed_handbook();
    let contributor = person(1);
    // A page that exists: the way to it.
    let there = get_as(&w, "/handbook/-/restore/Home", Some(&contributor)).await;
    assert_eq!(there.status, 303);
    assert_eq!(there.header("location"), Some("/handbook/Home"));
    // A title that never was a page: the page that invites it, as everywhere else.
    let never = get_as(&w, "/handbook/-/restore/Nothing", Some(&contributor)).await;
    assert_eq!(never.status, 404);
    assert!(main_of(&never.body()).contains("This page does not exist yet"));
    // Signed out: an account is needed, said in words.
    let anonymous = get(&w, "/handbook/-/restore/Guide").await;
    assert_eq!(anonymous.status, 403, "{}", anonymous.body());
}
