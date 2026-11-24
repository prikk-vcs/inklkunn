//! A removed page (RFC 006 D-10; handoff G-T6; acceptance criterion 10) through the real router, over an in-memory repository with a
//! scripted history: a page removed by an accepted proposal answers `410 Gone` with its page (when, by which proposal, why, and the
//! way back), a title that never was a page stays a `404`, a restricted wiki answers a stranger as an absent wiki does, and the way
//! back is a draft proposal that is a rollback when it merges cleanly and the page's last text when later changes block it. The
//! real prikk, end to end, is in `inklkunn-core/tests/soft_deletion.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use common::{
    APP, Asked, CONTENT, World, accept_by_hand, add_person, block_id, get, get_as, op, page,
    person, post_form, request, shown, with_accounts,
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
    // Anonymous: nobody who may not propose is offered the restoring (by the rung alone, no way back is worked out).
    assert_eq!(member(&json, "restore"), "none");
    let proposer = get_as(&w, "/handbook/-/api/v1/pages/Guide", Some(&person(1))).await;
    assert_eq!(proposer.status, 410, "{}", proposer.body());
    let json: serde_json::Value = serde_json::from_slice(&proposer.bytes).unwrap();
    assert_eq!(member(&json, "restore"), "offered", "{json}");
    let observer = person(9);
    add_person(&w, &observer, "A reader", Some(Role::Observer));
    let seen = get_as(&w, "/handbook/-/api/v1/pages/Guide", Some(&observer)).await;
    let json: serde_json::Value = serde_json::from_slice(&seen.bytes).unwrap();
    assert_eq!(member(&json, "restore"), "none", "{json}");
    let json: serde_json::Value = serde_json::from_slice(&a.bytes).unwrap();
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
    assert!(shown.body().contains("Undo: change 3"), "{}", shown.body());
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
    // The page of the removal says nothing of which way back there is: the link, and the form works it out.
    assert!(
        a.body().contains("/handbook/-/restore/Guide"),
        "{}",
        a.body()
    );
    assert!(
        !a.body().contains("Later changes stand in the way"),
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

/// A wiki whose second change removed `count` pages at once (`P0` ... ), with no proposal behind it.
fn bulk(count: usize) -> World {
    let paths: Vec<String> = (0..count).map(|i| format!("pages/P{i}.md")).collect();
    let texts: Vec<String> = (0..count)
        .map(|i| page(&[&format!("title: P{i}")], "Text.\n"))
        .collect();
    let mut files: Vec<(&str, &str)> = paths
        .iter()
        .map(String::as_str)
        .zip(texts.iter().map(String::as_str))
        .collect();
    let home_text = home("Welcome.\n");
    files.push((HOME, &home_text));
    let w = World::new("bulk", "Bulk", &files);
    let creates: Vec<_> = paths.iter().map(|p| op("create-file", &[p])).collect();
    w.fake.say(1, "Create the wiki");
    w.fake.show(1, shown(creates));
    let gone: Vec<&str> = paths.iter().map(String::as_str).collect();
    w.fake.apply(&[], &gone);
    w.fake.say(2, "Remove them all");
    let deletes: Vec<_> = paths.iter().map(|p| op("delete-node", &[p])).collect();
    w.fake.show(2, shown(deletes));
    let w = with_accounts(w);
    add_person(&w, &person(1), "Mika Tanaka", Some(Role::Contributor));
    w
}

/// The calls that cost a process for each page, asked for by the first visit (of a fresh wiki, nothing held) to `P0` of a wiki
/// whose change removed `count` pages, on `host` at `uri`, as an anonymous visitor or as `who`; and by the same visit again.
async fn visit(
    count: usize,
    host: &str,
    uri: &str,
    who: Option<&inklkunn_store::AccountId>,
) -> [Asked; 2] {
    let extra: Vec<(&str, &str)> = who
        .map(|a| ("x-test-account", a.as_str()))
        .into_iter()
        .collect();
    let w = bulk(count);
    let mut out = Vec::new();
    for _ in 0..2 {
        let before = w.fake.asked();
        let a = request(&w, host, "GET", uri, &extra).await;
        assert_eq!(a.status, 410, "{uri}: {}", a.body());
        out.push(w.fake.asked().since(before));
    }
    out.try_into().expect("two visits")
}

/// **S-1: a visit to a removed page costs what a visit costs.** The 410 reads the removal and nothing else: the calls it makes
/// are the same for a change that removed 200 pages as for one that removed one page, on the page, the raw 410 and the API,
/// and they are these (no `cat` at all: the way back, a rollback plan and the history behind each page, is the form that
/// restores's, behind its `propose` guard).
#[tokio::test]
async fn a_visit_to_a_removed_page_costs_the_same_whatever_the_change_removed() {
    /// The first visit: the view's history twice and the changes it names.
    const FIRST: Asked = Asked {
        shows: 2,
        cats: 0,
        logs: 2,
    };
    /// A repeated visit: held by id.
    const AGAIN: Asked = Asked {
        shows: 0,
        cats: 0,
        logs: 0,
    };
    for (host, uri) in [
        (APP, "/bulk/P0"),
        (CONTENT, "/bulk/-/raw/P0"),
        (APP, "/bulk/-/api/v1/pages/P0"),
    ] {
        // As a visitor who is not signed in, and as a person who may propose (who is only offered the link).
        for who in [None, Some(person(1))] {
            let one = visit(1, host, uri, who.as_ref()).await;
            let many = visit(200, host, uri, who.as_ref()).await;
            assert_eq!(one, many, "{uri}: the cost followed the pages removed");
            assert_eq!(many, [FIRST, AGAIN], "{uri} as {who:?}");
        }
    }
}

// ------------------------------------------------------------------------------------ S-3: the summary and the two sentences

/// A POST of the restore form in the language `lang` (`en` or `ja`) as `who`, with these fields.
async fn restore_as(
    w: &World,
    who: &inklkunn_store::AccountId,
    lang: &str,
    fields: &[(&str, &str)],
) -> common::Answer {
    let body = fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                inklkunn_markup::percent_encode(k),
                inklkunn_markup::percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    common::send_body(
        w,
        "POST",
        "/handbook/-/restore/Guide",
        Some(who),
        &[
            ("sec-fetch-site", "same-origin"),
            ("cookie", &format!("inklkunn_lang={lang}")),
        ],
        body,
    )
    .await
}

/// The restore form in the language `lang`.
async fn restore_form_in(w: &World, who: &inklkunn_store::AccountId, lang: &str) -> common::Answer {
    request(
        w,
        APP,
        "GET",
        "/handbook/-/restore/Guide",
        &[
            ("x-test-account", who.as_str()),
            ("cookie", &format!("inklkunn_lang={lang}")),
        ],
    )
    .await
}

/// The summary of the draft at `location`, as its page shows it (in the language asked for).
async fn draft_page(w: &World, who: &inklkunn_store::AccountId, location: &str) -> String {
    let shown = get_as(w, location, Some(who)).await;
    assert_eq!(shown.status, 200, "{}", shown.body());
    shown.body()
}

#[tokio::test]
async fn the_restore_drafts_summary_is_a_catalog_message_in_the_persons_language_and_the_person_may_change_it()
 {
    // The removal also edited `Home` and a later change edited the same place: the way back is the last text.
    let (w, _) = scripted(Visibility::Public, "handbook", true);
    let w = with_accounts(w);
    w.fake.set(HOME, &home("Welcome again, edited later.\n"));
    w.fake.say(4, "Edit the welcome");
    w.fake.show(4, shown(vec![op("edit-text", &[HOME])]));
    let who = person(1);
    // The form carries the default, in the person's language, in a field they can edit.
    let en = restore_form_in(&w, &who, "en").await;
    assert_eq!(en.status, 200, "{}", en.body());
    assert!(en.body().contains("name=\"summary\""), "{}", en.body());
    assert!(
        en.body().contains("value=\"Restore: Guide\""),
        "{}",
        en.body()
    );
    let ja = restore_form_in(&w, &who, "ja").await;
    assert!(ja.body().contains("name=\"summary\""), "{}", ja.body());
    assert!(ja.body().contains("value=\"復元: Guide\""), "{}", ja.body());
    assert!(!ja.body().contains("Restore: Guide"), "{}", ja.body());
    // Sent as it came, the draft says it in that language; sent with no field at all (an older form), the default too.
    for (lang, fields, expected) in [
        (
            "en",
            vec![("mode", "last-text"), ("summary", "Restore: Guide")],
            "Restore: Guide",
        ),
        (
            "ja",
            vec![("mode", "last-text"), ("summary", "復元: Guide")],
            "復元: Guide",
        ),
        ("ja", vec![("mode", "last-text")], "復元: Guide"),
        (
            "en",
            vec![("mode", "last-text"), ("summary", "  ")],
            "Restore: Guide",
        ),
        (
            "en",
            vec![
                ("mode", "last-text"),
                ("summary", "The guide was removed by mistake"),
            ],
            "The guide was removed by mistake",
        ),
    ] {
        let made = restore_as(&w, &who, lang, &fields).await;
        assert_eq!(made.status, 303, "{lang} {fields:?}: {}", made.body());
        let at = made.header("location").expect("a location").to_owned();
        let page = draft_page(&w, &who, &at).await;
        assert!(page.contains(expected), "{lang} {fields:?}: {page}");
    }
    // A rollback's summary is the plan's: its form has no field for one.
    let (w, _) = removed_handbook();
    let form = restore_form_in(&w, &person(1), "en").await;
    assert!(
        form.body().contains("value=\"rollback\""),
        "{}",
        form.body()
    );
    assert!(!form.body().contains("name=\"summary\""), "{}", form.body());
}

/// A wiki whose page `Guide` was removed and cannot be restored by a proposal, in `way` (the last text is not text, or the change
/// that wrote it is far back, which is no obstacle), and the person who may propose.
fn unrestorable(way: &str) -> World {
    let w = World::new("handbook", "The Handbook", &[(HOME, &home("Welcome.\n"))]);
    w.fake.say(1, "Create the handbook");
    w.fake.show(1, shown(vec![op("create-file", &[HOME])]));
    match way {
        "not text" => {
            w.fake.set_bytes(GUIDE, &[0xff, 0xfe, 0x00, b'x']);
            w.fake.say(2, "Add the guide");
            w.fake.show(2, shown(vec![op("create-file", &[GUIDE])]));
            w.fake.apply(&[], &[GUIDE]);
            w.fake.say(3, "Remove the guide");
            w.fake.show(3, shown(vec![op("delete-node", &[GUIDE])]));
        }
        _ => {
            w.fake.set(GUIDE, &guide("The guide.\n"));
            w.fake.say(2, "Add the guide");
            w.fake.show(2, shown(vec![op("create-file", &[GUIDE])]));
            // More changes of other pages than the history behind a removal is read through.
            for n in 0..100 {
                w.fake.set(HOME, &home(&format!("Welcome {n}.\n")));
            }
            w.fake.apply(&[], &[GUIDE]);
            w.fake.say(103, "Remove the guide");
            w.fake.show(103, shown(vec![op("delete-node", &[GUIDE])]));
        }
    }
    let w = with_accounts(w);
    add_person(&w, &person(1), "Mika Tanaka", Some(Role::Contributor));
    w
}

#[tokio::test]
async fn a_page_that_cannot_be_restored_says_why_in_its_own_sentence_for_each_cause_in_both_languages()
 {
    let not_text = ["cannot be read as text", "テキストとして読めない"];
    // The text that comes back is read where the removal's parent holds it, through the one reader of a text at a block (G-T4
    // correction H-1), so a page whose last text is far behind the history this wiki reads is **not** a page that cannot be
    // restored: the text is there, readable and not hidden. (There is no sentence for *further back than the history*: a removal
    // with no parent to read the text at is shown by no history, and is a failed read, not an answer.)
    {
        let who = person(1);
        let w = unrestorable("beyond");
        let form = restore_form_in(&w, &who, "en").await;
        assert_eq!(form.status, 200, "{}", form.body());
        assert!(form.body().contains("name=\"mode\""), "{}", form.body());
    }
    for (way, mine) in [("not text", not_text)] {
        let who = person(1);
        for (lang, mine_here) in ["en", "ja"].into_iter().zip(mine) {
            // A world of its own for each request: how far back a request reads is bounded for it, and the blocks it has read are
            // held by id, so that a second request of the same wiki reads further than the first (RFC 003 D-2).
            let w = unrestorable(way);
            let form = restore_form_in(&w, &who, lang).await;
            assert_eq!(form.status, 200, "{way} {lang}: {}", form.body());
            let body = form.body();
            assert!(!body.contains("name=\"mode\""), "{way} {lang}: {body}");
            assert!(body.contains(mine_here), "{way} {lang}: {body}");
            // The way on is the empty new page; a request for a restoring draft is refused and makes none.
            assert!(
                body.contains("/handbook/-/proposals/new?"),
                "{way} {lang}: {body}"
            );
            let w = unrestorable(way);
            let post = restore_as(&w, &who, lang, &[("mode", "last-text")]).await;
            assert_eq!(post.status, 409, "{way} {lang}: {}", post.body());
            assert!(
                post.body().contains(mine_here),
                "{way} {lang}: {}",
                post.body()
            );
            assert_eq!(post.header("location"), None);
        }
        // The page of the removal says neither: it works nothing out.
        let w = unrestorable(way);
        let gone = get_as(&w, "/handbook/Guide", Some(&who)).await;
        assert_eq!(gone.status, 410, "{way}: {}", gone.body());
        assert!(!gone.body().contains(mine[0]), "{way}");
    }
}
