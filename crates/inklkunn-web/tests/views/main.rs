//! Views: listing, creating and comparing them (RFC 005 part 2 D-9; handoff 005 K-T8), through the real router over
//! an in-memory repository, the real proposal service and the real acceptor and ledger, with a signer stand-in that
//! makes a branch only when the ledger's record says to. No prikk is needed, so these run in both test runs; the
//! same path against a real prikk is in the core's `tests/views.rs`. Every sentence is invented.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;
mod stubs;

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use common::{
    APP, Answer, World, add_person, claim, get, get_as, home_with, link, person, post_form, put,
    request, send_body, source, with_accounts,
};
use inklkunn_core::knowledge::{Bearing, Link, SourceKind, Standing};
use inklkunn_store::{AccountId, ActStore, Role, ViewActOutcome, ViewKind};
use stubs::{Brancher, attach, attach_stub};

const JA: &[(&str, &str)] = &[("cookie", "inklkunn_lang=ja")];
const SAID: &str = "The bridge opened in 1950.";

struct Env {
    w: World,
    contributor: AccountId,
    reviewer: AccountId,
    maintainer: AccountId,
    observer: AccountId,
    signer: Arc<Brancher>,
}

fn env() -> Env {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home_with(SAID))],
    );
    let w = with_accounts(w);
    let (contributor, reviewer, maintainer, observer) =
        (person(1), person(2), person(3), person(4));
    add_person(&w, &contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&w, &reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &observer, "Olu Reader", Some(Role::Observer));
    let (w, signer) = attach(w);
    Env {
        w,
        contributor,
        reviewer,
        maintainer,
        observer,
        signer,
    }
}

fn main_of(a: &Answer) -> String {
    let body = a.body();
    let start = body.find("<main").expect("a main landmark");
    let end = body.find("</main>").expect("the end of main");
    body.get(start..end).expect("main").to_owned()
}

/// The list item that holds `needle`.
fn entry_of(main: &str, needle: &str) -> String {
    let at = main.find(needle).expect(needle);
    let start = main
        .get(..at)
        .and_then(|head| head.rfind("<li>"))
        .expect("an item");
    let rest = main.get(start..).expect("the rest");
    let end = rest.find("</li>").expect("the end of the item");
    rest.get(..end).expect("the item").to_owned()
}

fn form<'a>(name: &'a str, kind: &'a str, from: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("name", name), ("kind", kind), ("from", from)]
}

const NEW: &str = "/handbook/-/views/new";

// ------------------------------------------------------------------------------------ the list

#[tokio::test]
async fn a_wiki_with_one_view_lists_the_default_view_by_that_name_and_nothing_else() {
    let e = env();
    let a = get(&e.w, "/handbook/-/views").await;
    assert_eq!(a.status, StatusCode::OK);
    let main = main_of(&a);
    assert!(main.contains("<h1>Views</h1>"), "{main}");
    assert!(main.contains("the default view"), "{main}");
    assert!(main.contains("Shown when no view is named."), "{main}");
    assert!(
        main.contains("This wiki has only its default view."),
        "{main}"
    );
    // An anonymous reader is offered no form and no comparison of one view with itself.
    assert!(!main.contains(NEW), "{main}");
    assert!(!main.contains("/-/views/compare"), "{main}");
    // The default view is read from the front door.
    assert!(
        main.contains("<a href=\"/handbook\">Read the default view</a>"),
        "{main}"
    );
}

#[tokio::test]
async fn the_list_names_the_kind_and_the_origin_of_a_view_the_service_made_and_says_so_of_one_it_did_not()
 {
    let e = env();
    // One made through the form; one made some other way.
    let r = post_form(
        &e.w,
        NEW,
        Some(&e.reviewer),
        &form("reading", "interpretation", "main"),
    )
    .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.body());
    e.w.fake.add_view("elsewhere");
    let main = main_of(&get(&e.w, "/handbook/-/views").await);
    assert!(main.contains("id=\"view-reading\""), "{main}");
    assert!(main.contains("Interpretation"), "{main}");
    assert!(
        main.contains("A competing understanding, kept beside the default view."),
        "{main}"
    );
    assert!(main.contains("Forked from the default view."), "{main}");
    assert!(
        main.contains("<a href=\"/handbook/~reading/Home\">Read the view “reading”</a>"),
        "{main}"
    );
    assert!(
        main.contains("Compare the view “reading” with the default view"),
        "{main}"
    );
    // The view nobody recorded is listed, with one plain sentence, not an invented origin.
    assert!(
        main.contains("The wiki did not record where this view came from."),
        "{main}"
    );
    // Nothing ranks them: no word of authority anywhere on the page.
    for word in ["canonical", "official", "adopted", "preferred", "best"] {
        assert!(!main.to_lowercase().contains(word), "{word}: {main}");
    }
    // In Japanese too.
    let ja = request(&e.w, APP, "GET", "/handbook/-/views", JA)
        .await
        .body();
    assert!(ja.contains("既定のビュー"), "{ja}");
    assert!(ja.contains("解釈"), "{ja}");
}

#[tokio::test]
async fn the_list_is_bounded_and_says_how_many_it_leaves_out() {
    let e = env();
    for n in 0..105 {
        e.w.fake.add_view(&format!("v{n:03}"));
    }
    let main = main_of(&get(&e.w, "/handbook/-/views").await);
    assert!(
        main.contains("The first 100 views are listed here; the other 6 are not shown."),
        "{main}"
    );
    assert_eq!(main.matches("<li id=\"view-").count(), 100, "{main}");
}

#[tokio::test]
async fn only_whoever_may_create_a_view_is_offered_the_form() {
    let e = env();
    let offered = |a: &Answer| main_of(a).contains(&format!("href=\"{NEW}\""));
    let who = [
        (None, false),
        (Some(&e.observer), false),
        (Some(&e.contributor), false),
        (Some(&e.reviewer), true),
        (Some(&e.maintainer), true),
    ];
    for (account, expected) in who {
        let a = get_as(&e.w, "/handbook/-/views", account).await;
        assert_eq!(a.status, StatusCode::OK);
        assert_eq!(offered(&a), expected, "{account:?}");
    }
}

#[tokio::test]
async fn the_about_page_leads_to_the_list() {
    let e = env();
    let body = get(&e.w, "/handbook/-/about").await.body();
    assert!(
        body.contains("<a href=\"/handbook/-/views\">All views of this wiki</a>"),
        "{body}"
    );
}

// ------------------------------------------------------------------------------------ who may create

#[tokio::test]
async fn a_reviewer_creates_a_view_through_the_form_and_the_act_is_recorded_and_signed_for() {
    let e = env();
    let form_page = get_as(&e.w, NEW, Some(&e.reviewer)).await;
    assert_eq!(form_page.status, StatusCode::OK);
    let main = main_of(&form_page);
    assert!(main.contains("<h1>Create a view</h1>"), "{main}");
    for field in [
        "name=\"name\"",
        "name=\"kind\"",
        "name=\"from\"",
        "name=\"csrf\"",
    ] {
        assert!(form_page.body().contains(field), "{field}: {main}");
    }
    assert!(main.contains("value=\"experimental\""), "{main}");
    assert!(main.contains("value=\"interpretation\""), "{main}");
    // No kind is preselected: choosing what a view is for is deliberate.
    assert!(!main.contains(" checked"), "{main}");
    assert!(
        main.contains("<option value=\"main\" selected>the default view</option>"),
        "{main}"
    );

    let done = post_form(
        &e.w,
        NEW,
        Some(&e.reviewer),
        &form("reading", "interpretation", "main"),
    )
    .await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body());
    assert_eq!(
        done.header("location"),
        Some("/handbook/-/views#view-reading")
    );
    // The record: whole, ended as created, by this account.
    let acts = e.w.store.view_acts(&e.w.wiki.id).unwrap();
    assert_eq!(acts.len(), 1);
    let entry = acts.first().unwrap();
    assert_eq!(entry.act.name, "reading");
    assert_eq!(entry.act.kind, ViewKind::Interpretation);
    assert_eq!(entry.act.forked_from, "main");
    assert_eq!(entry.act.created_by, e.reviewer);
    assert_eq!(
        entry.end.as_ref().map(|e| &e.outcome),
        Some(&ViewActOutcome::Created)
    );
    // The signer was asked once, for the record's facts, and its guard found the act open.
    let calls = e.signer.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls
            .first()
            .map(|c| (c.name.as_str(), c.from.as_str(), c.guarded)),
        Some(("heads/reading", "heads/main", true))
    );
    // A maintainer may as well.
    let again = post_form(
        &e.w,
        NEW,
        Some(&e.maintainer),
        &form("second", "experimental", "reading"),
    )
    .await;
    assert_eq!(again.status, StatusCode::SEE_OTHER, "{}", again.body());
    assert_eq!(e.w.store.view_acts(&e.w.wiki.id).unwrap().len(), 2);
}

#[tokio::test]
async fn nobody_below_a_reviewer_and_nobody_unknown_is_given_the_form_or_makes_a_view() {
    let e = env();
    for (who, status) in [
        (Some(&e.contributor), StatusCode::FORBIDDEN),
        (Some(&e.observer), StatusCode::FORBIDDEN),
        (None, StatusCode::FORBIDDEN),
    ] {
        let page = get_as(&e.w, NEW, who).await;
        assert_eq!(page.status, status, "{who:?}");
        assert!(!page.body().contains("name=\"kind\""), "{who:?}: no form");
        let posted = post_form(&e.w, NEW, who, &form("x", "experimental", "main")).await;
        assert_eq!(posted.status, status, "{who:?}");
    }
    // A contributor is told it is not allowed; a visitor who is not signed in is told to sign in.
    let contributor = main_of(&get_as(&e.w, NEW, Some(&e.contributor)).await);
    assert!(contributor.contains("Not allowed"), "{contributor}");
    assert!(
        contributor.contains("does not include creating views"),
        "{contributor}"
    );
    let anonymous = main_of(&get_as(&e.w, NEW, None).await);
    assert!(anonymous.to_lowercase().contains("account"), "{anonymous}");
    // The refusal comes before the form is read: a form with every field wrong is not told what is wrong with it by
    // someone who may not create a view, and a visitor who is nobody is told to sign in, not that it is not allowed.
    let bad = form("Bad Name", "", "nowhere");
    let refused = post_form(&e.w, NEW, Some(&e.contributor), &bad).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(
        main_of(&refused).contains("Not allowed"),
        "{}",
        main_of(&refused)
    );
    assert!(!refused.body().contains("role=\"alert\""));
    let nobody = post_form(&e.w, NEW, None, &bad).await;
    assert_eq!(nobody.status, StatusCode::FORBIDDEN);
    assert!(
        main_of(&nobody).to_lowercase().contains("account"),
        "{}",
        main_of(&nobody)
    );
    assert!(
        !main_of(&nobody).contains("Not allowed"),
        "{}",
        main_of(&nobody)
    );
    assert!(e.w.store.view_acts(&e.w.wiki.id).unwrap().is_empty());
    assert!(e.signer.calls().is_empty());
    assert!(e.w.logged().is_empty(), "{:?}", e.w.logged());
}

#[tokio::test]
async fn a_token_creates_a_view_only_with_the_review_scope() {
    let e = env();
    let body = |name: &str| format!("name={name}&kind=experimental&from=main");
    let with_token = |scope: &str| format!("Bearer {scope}.{}", e.reviewer);
    for (scope, status) in [
        ("read", StatusCode::FORBIDDEN),
        ("propose", StatusCode::FORBIDDEN),
        ("review", StatusCode::SEE_OTHER),
        ("accept", StatusCode::SEE_OTHER),
    ] {
        let token = with_token(scope);
        let a = send_body(
            &e.w,
            "POST",
            NEW,
            None,
            &[
                ("sec-fetch-site", "same-origin"),
                ("authorization", token.as_str()),
            ],
            body(&format!("t-{scope}")),
        )
        .await;
        assert_eq!(a.status, status, "{scope}: {}", a.body());
    }
    assert_eq!(e.w.store.view_acts(&e.w.wiki.id).unwrap().len(), 2);
}

#[tokio::test]
async fn a_write_without_the_sessions_token_or_from_another_site_makes_no_view() {
    let e = env();
    let no_token = send_body(
        &e.w,
        "POST",
        NEW,
        Some(&e.reviewer),
        &[("sec-fetch-site", "same-origin"), (common::NO_TOKEN, "")],
        "name=x&kind=experimental&from=main".to_owned(),
    )
    .await;
    assert_eq!(
        no_token.status,
        StatusCode::FORBIDDEN,
        "{}",
        no_token.body()
    );
    let cross = send_body(
        &e.w,
        "POST",
        NEW,
        Some(&e.reviewer),
        &[("sec-fetch-site", "cross-site")],
        "name=x&kind=experimental&from=main".to_owned(),
    )
    .await;
    assert_eq!(cross.status, StatusCode::FORBIDDEN, "{}", cross.body());
    assert!(e.w.store.view_acts(&e.w.wiki.id).unwrap().is_empty());
    assert!(e.signer.calls().is_empty());
}

#[tokio::test]
async fn a_restricted_wiki_and_an_absent_one_answer_the_form_alike() {
    let w = World::with_visibility(
        "secret",
        "Secret",
        inklkunn_store::Visibility::Restricted,
        &[("pages/Home.md", &home_with(SAID))],
    );
    let w = with_accounts(w);
    let stranger = person(9);
    add_person(&w, &stranger, "A stranger", None);
    let real = get_as(&w, "/secret/-/views/new", Some(&stranger)).await;
    let absent = get_as(&w, "/nowhere/-/views/new", Some(&stranger)).await;
    assert_eq!(real.status, absent.status);
    assert_eq!(real.status, StatusCode::NOT_FOUND);
    let list = get_as(&w, "/secret/-/views", Some(&stranger)).await;
    assert_eq!(list.status, StatusCode::NOT_FOUND);
    let compare = get_as(&w, "/secret/-/views/compare?a=main&b=main", Some(&stranger)).await;
    assert_eq!(compare.status, StatusCode::NOT_FOUND);
}

// ------------------------------------------------------------------------------------ problems with the form

#[tokio::test]
async fn a_name_a_kind_or_a_source_that_is_wrong_shows_the_form_again_with_what_was_typed() {
    let e = env();
    let long = "x".repeat(65);
    let cases = [
        (form("", "experimental", "main"), "Give the view a name."),
        (
            form("Draft", "experimental", "main"),
            "A name starts with a lower-case letter or a digit.",
        ),
        (
            form("a b", "experimental", "main"),
            "A name has only lower-case letters",
        ),
        (
            form("a/b", "experimental", "main"),
            "A name has only lower-case letters",
        ),
        (
            form(&long, "experimental", "main"),
            "A name has at most 64 characters.",
        ),
        (form("fine", "", "main"), "Choose what the view is for."),
        (
            form("fine", "canonical", "main"),
            "Choose what the view is for.",
        ),
        (
            form("fine", "experimental", "nowhere"),
            "Choose a view that is there to fork it from.",
        ),
    ];
    for (fields, problem) in cases {
        let a = post_form(&e.w, NEW, Some(&e.reviewer), &fields).await;
        assert_eq!(
            a.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{fields:?}: {}",
            a.body()
        );
        let main = main_of(&a);
        assert!(main.contains(problem), "{fields:?}: {main}");
        assert!(main.contains("role=\"alert\""), "{main}");
        // What was typed is kept.
        let typed = fields.first().map_or("", |(_, v)| *v);
        if !typed.is_empty() && typed.len() < 20 && !typed.contains('/') {
            assert!(
                main.contains(&format!("value=\"{typed}\"")),
                "{typed}: {main}"
            );
        }
    }
    assert!(e.w.store.view_acts(&e.w.wiki.id).unwrap().is_empty());
    assert!(e.signer.calls().is_empty());
}

#[tokio::test]
async fn what_was_chosen_stays_chosen_when_the_form_is_shown_again() {
    let e = env();
    e.w.fake.add_view("other");
    let a = post_form(
        &e.w,
        NEW,
        Some(&e.reviewer),
        &form("Bad Name", "interpretation", "other"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let main = main_of(&a);
    assert!(
        main.contains(
            "value=\"interpretation\" aria-describedby=\"view-kind-interpretation-note\" checked"
        ),
        "{main}"
    );
    assert!(
        !main.contains(
            "value=\"experimental\" aria-describedby=\"view-kind-experimental-note\" checked"
        ),
        "{main}"
    );
    assert!(
        main.contains("<option value=\"other\" selected>other</option>"),
        "{main}"
    );
    assert!(main.contains("value=\"Bad Name\""), "{main}");
}

#[tokio::test]
async fn a_name_that_is_taken_is_a_conflict_that_shows_the_form_and_changes_nothing() {
    let e = env();
    e.w.fake.add_view("draft");
    let a = post_form(
        &e.w,
        NEW,
        Some(&e.reviewer),
        &form("draft", "experimental", "main"),
    )
    .await;
    assert_eq!(a.status, StatusCode::CONFLICT, "{}", a.body());
    assert!(
        main_of(&a).contains("A view with that name is already there. Choose another name."),
        "{}",
        main_of(&a)
    );
    assert!(e.signer.calls().is_empty());
    // A name that differs only by case is the same name for this purpose; a created one is taken after.
    let b = post_form(
        &e.w,
        NEW,
        Some(&e.reviewer),
        &form("fresh", "experimental", "main"),
    )
    .await;
    assert_eq!(b.status, StatusCode::SEE_OTHER);
    let c = post_form(
        &e.w,
        NEW,
        Some(&e.reviewer),
        &form("fresh", "interpretation", "main"),
    )
    .await;
    assert_eq!(c.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_form_that_is_not_a_form_is_told_so_and_changes_nothing() {
    let e = env();
    let a = send_body(
        &e.w,
        "POST",
        NEW,
        Some(&e.reviewer),
        &[
            ("sec-fetch-site", "same-origin"),
            ("content-type", "text/plain"),
        ],
        "name=x".to_owned(),
    )
    .await;
    assert!(a.status.is_client_error(), "{}", a.status);
    assert!(e.w.store.view_acts(&e.w.wiki.id).unwrap().is_empty());
}

// ------------------------------------------------------------------------------------ each way a creation can end

#[tokio::test]
async fn each_way_a_creation_can_end_is_a_page_that_says_whether_anything_changed() {
    use inklkunn_core::{Busy, BusyWhy, ViewFailure};
    use inklkunn_store::{RecoveryFlag, RecoveryReason, StoreError};
    let busy = Busy {
        depth: 2,
        estimate: std::time::Duration::from_secs(4),
        why: BusyWhy::Waited,
    };
    let flag = RecoveryFlag {
        wiki: inklkunn_store::WikiId::new(),
        reason: RecoveryReason::Queued,
        act: None,
        since: 1,
    };
    let id = inklkunn_store::ViewActId::new();
    let cases: Vec<(ViewFailure, StatusCode, &str, bool)> = vec![
        (
            ViewFailure::NotAllowed,
            StatusCode::FORBIDDEN,
            "does not include creating views",
            false,
        ),
        (
            ViewFailure::NotKept,
            StatusCode::NOT_IMPLEMENTED,
            "no view can be created here yet",
            false,
        ),
        (
            ViewFailure::TooMany { bound: 100 },
            StatusCode::CONFLICT,
            "already has 100 open views",
            false,
        ),
        (
            ViewFailure::Busy(busy),
            StatusCode::SERVICE_UNAVAILABLE,
            "Another change is being made to the wiki.",
            true,
        ),
        (
            ViewFailure::NeedsRecovery(flag),
            StatusCode::SERVICE_UNAVAILABLE,
            "checked after an interruption",
            true,
        ),
        (
            ViewFailure::RetirementUnfinished("finish it".to_owned()),
            StatusCode::SERVICE_UNAVAILABLE,
            "so nothing new can be made in its history",
            false,
        ),
        (
            ViewFailure::Failed {
                act: id.clone(),
                word: "refused",
                detail: "x".to_owned(),
            },
            StatusCode::UNPROCESSABLE_ENTITY,
            "The history did not accept the new view.",
            false,
        ),
        (
            ViewFailure::Failed {
                act: id,
                word: "unavailable",
                detail: "x".to_owned(),
            },
            StatusCode::SERVICE_UNAVAILABLE,
            "could not be used just now",
            true,
        ),
        (
            ViewFailure::Unavailable("x".to_owned()),
            StatusCode::SERVICE_UNAVAILABLE,
            "could not be used just now",
            true,
        ),
        (
            ViewFailure::Uncertain("x".to_owned()),
            StatusCode::INTERNAL_SERVER_ERROR,
            "may or may not be there",
            false,
        ),
        (
            ViewFailure::Store(StoreError::NotFound),
            StatusCode::SERVICE_UNAVAILABLE,
            "could not be used just now",
            true,
        ),
    ];
    for (failure, status, words, retry) in cases {
        let mut e = env();
        attach_stub(&mut e.w, failure.clone());
        let a = post_form(
            &e.w,
            NEW,
            Some(&e.reviewer),
            &form("x", "experimental", "main"),
        )
        .await;
        assert_eq!(a.status, status, "{failure:?}: {}", a.body());
        assert!(main_of(&a).contains(words), "{failure:?}: {}", main_of(&a));
        assert_eq!(a.header("retry-after").is_some(), retry, "{failure:?}");
        // The cause is the operator's: it is in the log for what failed, and never on the page.
        assert!(!a.body().contains("finish it"), "{failure:?}: {}", a.body());
    }
}

#[tokio::test]
async fn a_server_that_cannot_create_views_says_so_before_it_shows_a_form() {
    let e = env();
    // A build without an acceptor.
    let w = common::fresh(&e.w);
    let a = get_as(&w, NEW, Some(&e.reviewer)).await;
    assert_eq!(a.status, StatusCode::NOT_IMPLEMENTED);
    assert!(
        main_of(&a).contains("Creating views is not available on this server"),
        "{}",
        main_of(&a)
    );
    assert!(!a.body().contains("name=\"kind\""));
    // A server started without the means to sign.
    let (mut w, _) = attach(common::fresh(&e.w));
    w.services = Arc::new(inklkunn_web::Services {
        acceptor: Some(Arc::new(inklkunn_web::services::NoSigner)),
        ..(*w.services).clone()
    });
    let a = get_as(&w, NEW, Some(&e.reviewer)).await;
    assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        main_of(&a).contains("Creating views is not set up on this server"),
        "{}",
        main_of(&a)
    );
    let posted = post_form(
        &w,
        NEW,
        Some(&e.reviewer),
        &form("x", "experimental", "main"),
    )
    .await;
    assert_eq!(posted.status, StatusCode::SERVICE_UNAVAILABLE);
    // A store that keeps no record of a view's creation (a stub: neither real store is one since schema 16).
    let (mut w, _) = attach(common::fresh(&e.w));
    w.services = Arc::new(inklkunn_web::Services {
        acts: Arc::new(stubs::KeepsNoViewActs(Arc::new(
            inklkunn_store::MemoryStore::new(),
        ))),
        ..(*w.services).clone()
    });
    let a = get_as(&w, NEW, Some(&e.reviewer)).await;
    assert_eq!(a.status, StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn a_server_over_the_sqlite_store_no_longer_says_that_creating_views_is_not_available() {
    // Schema 16 (K3): the SQLite store keeps the record of a view's creation, so the form is offered.
    let e = env();
    let (mut w, _) = attach(common::fresh(&e.w));
    w.services = Arc::new(inklkunn_web::Services {
        acts: Arc::new(inklkunn_store::SqliteStore::open_in_memory().unwrap()),
        ..(*w.services).clone()
    });
    let a = get_as(&w, NEW, Some(&e.reviewer)).await;
    assert_eq!(a.status, StatusCode::OK, "{}", main_of(&a));
    assert!(a.body().contains("name=\"kind\""), "{}", main_of(&a));
    assert!(!a.body().contains("Creating views is not available"));
}

// ------------------------------------------------------------------------------------ comparing

/// `main` has moved on from `alt`: its `Home` is worded differently, its claim is accepted where `alt` contests it, it
/// cites a second source, and it has a claim `alt` does not. The diff prikk would give is scripted.
fn forked(e: &Env) {
    let said_now = "The bridge was opened in 1950.";
    // Version 2 (what `alt` is): the claim as the paragraph first had it, contested, with one source.
    let mut old = claim(1, Standing::Contested, None, SAID);
    old.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
    put(
        &e.w,
        &home_with(SAID),
        &[old],
        &[
            source(1, SourceKind::Primary),
            source(2, SourceKind::Secondary),
        ],
    );
    e.w.fake.add_view_at("alt", 2);
    // Version 3 (what `main` is now).
    let mut now = claim(1, Standing::Accepted, None, said_now);
    now.sources = vec![
        link(1, Link::Supports, Bearing::Direct, said_now),
        link(2, Link::Challenges, Bearing::Partial, said_now),
    ];
    let own = claim(2, Standing::Unresolved, None, said_now);
    put(
        &e.w,
        &home_with(said_now),
        &[now, own],
        &[
            source(1, SourceKind::Primary),
            source(2, SourceKind::Secondary),
        ],
    );
    // A page only `main` has.
    e.w.fake.set(
        "pages/Extra.md",
        "---\ntitle: Extra\n---\n\nOnly in the default view.\n",
    );
    e.w.fake.diff_says(Ok(inklkunn_prikk::Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: inklkunn_prikk::storage::DiffSide {
            point: "a".to_owned(),
            target_block_id: "a".repeat(64),
        },
        to: inklkunn_prikk::storage::DiffSide {
            point: "b".to_owned(),
            target_block_id: "b".repeat(64),
        },
        entries: vec![
            inklkunn_prikk::DiffEntry {
                path: "pages/Home.md".to_owned(),
                status: "modified".to_owned(),
                from: None,
                to: None,
                hunks: vec![
                    "@@ -6,1 +6,1 @@\n-The bridge opened in 1950. ^b1\n+The bridge was opened in 1950. ^b1\n".to_owned(),
                ],
            },
            inklkunn_prikk::DiffEntry {
                path: "pages/Extra.md".to_owned(),
                status: "removed".to_owned(),
                from: None,
                to: None,
                hunks: vec!["@@ -1,1 +0,0 @@\n-Hello.\n".to_owned()],
            },
            inklkunn_prikk::DiffEntry {
                path: "pages/Old.md".to_owned(),
                status: "added".to_owned(),
                from: None,
                to: None,
                hunks: vec!["@@ -0,0 +1,1 @@\n+Bye.\n".to_owned()],
            },
        ],
        unsupported_paths: Vec::new(),
    }));
}

const COMPARE: &str = "/handbook/-/views/compare?a=main&b=alt";

#[tokio::test]
async fn two_views_that_differ_are_compared_claim_by_claim_and_page_by_page_without_a_verdict() {
    let e = env();
    forked(&e);
    let a = get(&e.w, COMPARE).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    assert!(
        main.contains("<h1>Comparing the default view and the view “alt”</h1>"),
        "{main}"
    );
    // The claims: the first differs in all three ways, the second is only in `alt`.
    assert!(
        main.contains("<h2 id=\"claims\">Claims that differ</h2>"),
        "{main}"
    );
    assert!(main.contains("A claim said in Home"), "{main}");
    assert!(main.contains("It is worded differently."), "{main}");
    assert!(
        main.contains("Its status: accepted in the default view, contested in the view “alt”."),
        "{main}"
    );
    assert!(main.contains("Its sources differ."), "{main}");
    assert!(main.contains("Only in the default view."), "{main}");
    // Each claim leads to its page in a view that has it.
    assert!(
        main.contains(&format!(
            "href=\"/handbook/-/claim/{}\"",
            common::claim_id(1)
        )),
        "{main}"
    );
    assert!(
        main.contains(&format!(
            "href=\"/handbook/-/claim/{}\"",
            common::claim_id(2)
        )),
        "{main}"
    );
    // The pages: how each differs, and a link to the page's own diff between the two views.
    assert!(
        main.contains("<h2 id=\"pages\">Pages that differ</h2>"),
        "{main}"
    );
    assert!(main.contains("The page differs."), "{main}");
    assert!(main.contains("1 line added"), "{main}");
    // A page only one view has says which: `Extra` is main's alone, `Old` is alt's alone.
    assert!(
        entry_of(&main, "How Extra differs").contains("Only in the default view."),
        "{main}"
    );
    assert!(
        entry_of(&main, "How Old differs").contains("Only in the view “alt”."),
        "{main}"
    );
    assert!(
        entry_of(&main, "How Home differs").contains("The page differs."),
        "{main}"
    );
    assert!(
        main.contains("href=\"/handbook/-/views/compare/Home?a=main&#38;b=alt\""),
        "{main}"
    );
    // No word says which is right.
    for word in [
        "canonical",
        "official",
        "adopted",
        "correct",
        "wrong",
        "true",
        "false",
        "verified",
    ] {
        let lowered = main.to_lowercase();
        assert!(!lowered.contains(&format!(" {word}")), "{word}: {main}");
    }
    // One diff of the two tips, whole tree.
    let asked = e.w.fake.diffs_asked();
    assert_eq!(asked.len(), 1);
    assert!(asked.first().unwrap().2.is_empty());
    // In Japanese.
    let ja = request(&e.w, APP, "GET", COMPARE, JA).await.body();
    assert!(ja.contains("違いのある主張"), "{ja}");
    assert!(ja.contains("違いのあるページ"), "{ja}");
}

#[tokio::test]
async fn a_pages_difference_between_two_views_is_the_history_pages_diff_with_its_marks() {
    let e = env();
    forked(&e);
    let a = get(&e.w, "/handbook/-/views/compare/Home?a=main&b=alt").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    assert!(main.contains("Lines marked removed are in the default view only; lines marked added are in the view “alt” only."), "{main}");
    assert!(main.contains("<ins>"), "{main}");
    assert!(main.contains("<del>"), "{main}");
    assert!(main.contains("diff-hunk"), "{main}");
    assert!(main.contains("Back to the comparison"), "{main}");
    assert!(
        main.contains("href=\"/handbook/-/views/compare?a=main&#38;b=alt\""),
        "{main}"
    );
    // The page is read in each view that has it.
    assert!(main.contains("Read Home in the default view"), "{main}");
    assert!(main.contains("Read Home in the view “alt”"), "{main}");
    assert!(main.contains("href=\"/handbook/~alt/Home\""), "{main}");
    // One diff, with the page's path.
    let asked = e.w.fake.diffs_asked();
    assert_eq!(asked.last().unwrap().2, vec!["pages/Home.md".to_owned()]);
    // A page neither view has; a view that is not there; an address without its views.
    let absent = get(&e.w, "/handbook/-/views/compare/Nowhere?a=main&b=alt").await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
    assert!(
        main_of(&absent).contains("Neither view has this page."),
        "{}",
        main_of(&absent)
    );
    let none = get(&e.w, "/handbook/-/views/compare/Home?a=main&b=nope").await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    let bare = get(&e.w, "/handbook/-/views/compare/Home").await;
    assert_eq!(bare.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_page_only_one_view_has_is_read_only_in_that_view() {
    let e = env();
    forked(&e);
    // `Extra` is the default view's alone: the page is offered where it is, and nowhere it is not.
    let a = get(&e.w, "/handbook/-/views/compare/Extra?a=main&b=alt").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    assert!(main.contains("Only in the default view."), "{main}");
    assert!(main.contains("Read Extra in the default view"), "{main}");
    assert!(!main.contains("Read Extra in the view “alt”"), "{main}");
    assert!(!main.contains("~alt/Extra"), "{main}");
}

#[tokio::test]
async fn a_claim_only_the_second_view_has_leads_to_its_page_in_the_second_view() {
    let e = env();
    forked(&e);
    // Reversed, the claim only `main` has is the second view's: its page is read in the default view (no `?view=`),
    // not in `alt`, which has no such claim.
    let main = main_of(&get(&e.w, "/handbook/-/views/compare?a=alt&b=main").await);
    assert!(
        main.contains(&format!(
            "href=\"/handbook/-/claim/{}\"",
            common::claim_id(2)
        )),
        "{main}"
    );
    assert!(
        !main.contains(&format!(
            "/handbook/-/claim/{}?view=alt",
            common::claim_id(2)
        )),
        "{main}"
    );
    assert!(main.contains("Only in the default view."), "{main}");
}

#[tokio::test]
async fn the_chooser_the_same_view_twice_and_a_view_that_is_not_there_are_each_a_page() {
    let e = env();
    forked(&e);
    // No second view chosen yet: the chooser, with both views offered and nothing compared.
    let chooser = get(&e.w, "/handbook/-/views/compare").await;
    assert_eq!(chooser.status, StatusCode::OK);
    let main = main_of(&chooser);
    assert!(main.contains("<h1>Compare views</h1>"), "{main}");
    assert!(
        main.contains("This lists where two views differ. It does not say which one is right."),
        "{main}"
    );
    assert!(
        main.contains("<select id=\"compare-a\" name=\"a\">"),
        "{main}"
    );
    assert!(
        main.contains("<option value=\"alt\">alt</option>"),
        "{main}"
    );
    assert!(!main.contains("Claims that differ"), "{main}");
    // The same view twice.
    let twice = main_of(&get(&e.w, "/handbook/-/views/compare?a=alt&b=alt").await);
    assert!(
        twice.contains("Choose two different views to compare."),
        "{twice}"
    );
    assert!(!twice.contains("Claims that differ"), "{twice}");
    // A view that is not there, on either side, and one the address cannot spell.
    for uri in [
        "/handbook/-/views/compare?a=main&b=nope",
        "/handbook/-/views/compare?a=nope&b=main",
        "/handbook/-/views/compare?a=..%2Fx&b=main",
    ] {
        let a = get(&e.w, uri).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            a.body().contains("There is no such view"),
            "{uri}: {}",
            a.body()
        );
    }
    // Views at the same change have nothing to compare.
    e.w.fake.add_view("twin");
    e.w.fake.add_view_at("twin2", 3);
    let same = main_of(&get(&e.w, "/handbook/-/views/compare?a=main&b=twin").await);
    assert!(
        same.contains("The two views hold the same content."),
        "{same}"
    );
}

#[tokio::test]
async fn the_lists_of_a_comparison_are_bounded_and_say_how_many_they_leave_out() {
    let e = env();
    forked(&e);
    let entries: Vec<inklkunn_prikk::DiffEntry> = (0..205)
        .map(|n| inklkunn_prikk::DiffEntry {
            path: format!("pages/P{n:04}.md"),
            status: "modified".to_owned(),
            from: None,
            to: None,
            hunks: Vec::new(),
        })
        .collect();
    e.w.fake.diff_says(Ok(inklkunn_prikk::Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: inklkunn_prikk::storage::DiffSide {
            point: "a".to_owned(),
            target_block_id: "a".repeat(64),
        },
        to: inklkunn_prikk::storage::DiffSide {
            point: "b".to_owned(),
            target_block_id: "b".repeat(64),
        },
        entries,
        unsupported_paths: Vec::new(),
    }));
    let main = main_of(&get(&e.w, COMPARE).await);
    assert!(
        main.contains("200 pages are listed; 5 more also differ and are not shown."),
        "{main}"
    );
    assert_eq!(main.matches("How P").count(), 200, "{main}");
}

#[tokio::test]
async fn a_diff_prikk_cannot_give_is_could_not_be_read_now_and_not_an_empty_comparison() {
    let e = env();
    forked(&e);
    e.w.fake.diff_says(Err(common::busy()));
    let a = get(&e.w, COMPARE).await;
    assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!a.body().contains("No page differs"), "{}", a.body());
}

// ------------------------------------------------------------------------------------ the claim page's other views

#[tokio::test]
async fn a_claims_page_names_each_other_view_where_it_reads_differently_with_a_link_to_compare() {
    let e = env();
    forked(&e);
    let claim_page = format!("/handbook/-/claim/{}", common::claim_id(1));
    let a = get(&e.w, &claim_page).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    assert!(main.contains("<h2>Other views</h2>"), "{main}");
    assert!(main.contains("the view “alt”"), "{main}");
    assert!(main.contains("It is worded differently."), "{main}");
    assert!(
        main.contains("Its status: accepted in the default view, contested in the view “alt”."),
        "{main}"
    );
    assert!(
        main.contains(
            "<a href=\"/handbook/-/views/compare?a=main&#38;b=alt\">Compare with the view “alt”</a>"
        ),
        "{main}"
    );
    assert!(
        !main.contains("No other view reads this claim differently."),
        "{main}"
    );
    // Read in the other view, the default view is the other.
    let there = main_of(&get(&e.w, &format!("{claim_page}?view=alt")).await);
    assert!(there.contains("<h2>Other views</h2>"), "{there}");
    assert!(
        there.contains(
            "href=\"/handbook/-/views/compare?a=alt&#38;b=main\">Compare with the default view</a>"
        ),
        "{there}"
    );
    // A claim only this view has: the other view is named, and the claim is said to be only here.
    let own = main_of(&get(&e.w, &format!("/handbook/-/claim/{}", common::claim_id(2))).await);
    assert!(own.contains("Only in the default view."), "{own}");
    assert!(own.contains("Compare with the view “alt”"), "{own}");
    // In the view that has not the claim there is no such claim.
    let none = get(
        &e.w,
        &format!("/handbook/-/claim/{}?view=alt", common::claim_id(2)),
    )
    .await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_claim_that_reads_alike_everywhere_says_so_and_a_wiki_with_one_view_says_nothing() {
    let e = env();
    put(
        &e.w,
        &home_with(SAID),
        &[claim(1, Standing::Accepted, None, SAID)],
        &[],
    );
    let page = format!("/handbook/-/claim/{}", common::claim_id(1));
    // Only the default view: no heading over an empty section.
    let alone = main_of(&get(&e.w, &page).await);
    assert!(!alone.contains("Other views"), "{alone}");
    // A second view at the same change reads it alike.
    e.w.fake.add_view("twin");
    let alike = main_of(&get(&e.w, &page).await);
    assert!(alike.contains("<h2>Other views</h2>"), "{alike}");
    assert!(
        alike.contains("No other view reads this claim differently."),
        "{alike}"
    );
}

#[tokio::test]
async fn a_view_that_could_not_be_read_is_said_to_be_unchecked_and_not_to_agree() {
    let e = env();
    put(
        &e.w,
        &home_with(SAID),
        &[claim(1, Standing::Accepted, None, SAID)],
        &[],
    );
    // A view whose tip the repository cannot give just now.
    e.w.fake.add_view_at("ghost", 99);
    let page = format!("/handbook/-/claim/{}", common::claim_id(1));
    let a = get(&e.w, &page).await;
    assert_eq!(a.status, StatusCode::OK);
    let main = main_of(&a);
    assert!(main.contains("<h2>Other views</h2>"), "{main}");
    assert!(
        main.contains("Some views could not be read just now, so they are not listed here."),
        "{main}"
    );
    assert!(
        !main.contains("No other view reads this claim differently."),
        "{main}"
    );
}

#[tokio::test]
async fn views_that_could_not_be_read_or_are_past_the_bound_are_said_to_be_unchecked_and_never_agreeing()
 {
    let e = env();
    put(
        &e.w,
        &home_with(SAID),
        &[claim(1, Standing::Accepted, None, SAID)],
        &[],
    );
    for n in 0..12 {
        e.w.fake.add_view(&format!("v{n:02}"));
    }
    let page = format!("/handbook/-/claim/{}", common::claim_id(1));
    let main = main_of(&get(&e.w, &page).await);
    assert!(
        main.contains(
            "Only 10 other views are checked on this page; compare views to see the others."
        ),
        "{main}"
    );
    // Not "no other view reads it differently": it did not look at them all.
    assert!(
        !main.contains("No other view reads this claim differently."),
        "{main}"
    );
}

// ------------------------------------------------------------------------------------ headers kept

#[tokio::test]
async fn the_headers_of_a_refusal_carry_nothing_of_the_wiki() {
    let e = env();
    let a = get_as(&e.w, NEW, Some(&e.contributor)).await;
    let h: &HeaderMap = &a.headers;
    assert!(h.get("location").is_none());
    assert!(h.get("set-cookie").is_none());
}
