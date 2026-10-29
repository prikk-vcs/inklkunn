//! A wiki's pages through the real router (RFC 003 D-1, D-5; handoff 003 T4): found, redirected, missing,
//! private, unavailable, stale; the badge; the verification page; existence neutrality. The repository is an
//! in-memory double, so these run in both test runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{NOW, World, busy, clean, damaged, page};
use inklkunn_store::{Attempt, NotRun, Verification, VerificationState, Visibility, WikiStore};
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site_with;
use tower::ServiceExt as _;

fn router(w: &World) -> Router {
    site_with(
        &WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
        Some(w.services.clone()),
    )
    .unwrap()
}

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn send(w: &World, method: Method, uri: &str, extra: &[(&str, &str)]) -> Answer {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "localhost:8080");
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let res = router(w)
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 22).await.unwrap();
    Answer {
        status: parts.status,
        headers: parts.headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn get(w: &World, uri: &str) -> Answer {
    send(w, Method::GET, uri, &[]).await
}

async fn post_form(w: &World, uri: &str, same_site: bool) -> Answer {
    let site = if same_site {
        "same-origin"
    } else {
        "cross-site"
    };
    send(
        w,
        Method::POST,
        uri,
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("sec-fetch-site", site),
        ],
    )
    .await
}

fn handbook() -> World {
    World::new(
        "handbook",
        "The Handbook",
        &[
            (
                "pages/Home.md",
                &page(
                    &["title: Home", "description: Start here"],
                    "# Welcome\n\nSee [[Guide]] and [[Nowhere]].\n",
                ),
            ),
            (
                "pages/Guide.md",
                &page(&["title: Guide", "aliases: [How-to]"], "The guide.\n"),
            ),
            (
                "pages/Old Name.md",
                &page(
                    &["title: Old Name", "kind: redirect", "redirect: Guide"],
                    "",
                ),
            ),
            (
                "pages/Forged.md",
                &page(
                    &["title: Forged"],
                    "Go <a class=\"wikilink\" href=\"https://evil.example/\">Home</a> now.\n",
                ),
            ),
            (
                "pages/~odd.md",
                &page(&["title: ~odd"], "A title with a tilde.\n"),
            ),
        ],
    )
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[tokio::test]
async fn a_page_is_shown_with_one_main_heading_its_body_and_the_honest_badge() {
    let w = handbook();
    let a = get(&w, "/handbook/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(
        a.headers
            .get(header::CONTENT_LANGUAGE)
            .map(|v| v.to_str().unwrap()),
        Some("en")
    );
    assert!(a.headers.contains_key(header::CONTENT_SECURITY_POLICY));
    assert_eq!(count(&a.body, "<h1"), 1, "{}", a.body);
    assert!(a.body.contains("<h1>Home</h1>"), "{}", a.body);
    // The body's own `#` is moved down so the page keeps its one h1.
    assert!(
        a.body.contains("<h2 id=\"h-welcome\">Welcome</h2>"),
        "{}",
        a.body
    );
    assert!(
        a.body
            .contains("<title>Home · The Handbook · inklkunn</title>")
    );
    assert!(
        a.body
            .contains("class=\"wikilink\" href=\"/handbook/Guide\"")
    );
    assert!(a.body.contains("wikilink-missing"));
    assert!(a.body.contains("License: CC-BY-SA-4.0"));
    // Never verified: the badge says so, and says nothing more.
    assert!(
        a.body
            .contains("Verification: <a href=\"/handbook/-/verification\">Not checked yet</a>")
    );
    assert!(!a.body.contains("Verified"));
    // A proposal cannot be made yet: the affordance is there, disabled, and says why; no dead link.
    assert!(
        a.body
            .contains("<button type=\"button\" disabled>Propose a change</button>")
    );
    assert!(a.body.contains("Proposals arrive with the next release."));
    assert!(!a.body.contains("/-/create"));
}

#[tokio::test]
async fn the_badge_is_the_records_and_never_the_caches() {
    let w = handbook();
    let first = get(&w, "/handbook/Home").await;
    assert!(first.body.contains("Not checked yet"));
    for (state, label) in [
        (VerificationState::Verified, "Verified"),
        (VerificationState::Failed, "Verification failed"),
    ] {
        w.store
            .set_verification(
                &w.wiki.id,
                Verification {
                    state,
                    at: Some(NOW),
                },
            )
            .unwrap();
        // The page itself is a cache hit (the tip did not move); the badge is read afresh.
        let a = get(&w, "/handbook/Home").await;
        assert!(
            a.body.contains(&format!(">{label}</a>")),
            "{label}: {}",
            a.body
        );
        assert_eq!(count(&a.body, "Verification: "), 1);
    }
}

#[tokio::test]
async fn a_check_that_could_not_run_never_changes_what_the_badge_says_of_the_last_verdict() {
    let attempt = Attempt {
        at: NOW - 30,
        why: NotRun::Busy,
    };
    // No verdict yet: the attempt is what there is to say.
    let w = handbook();
    w.store.record_attempt(&w.wiki.id, attempt).unwrap();
    let a = get(&w, "/handbook/Home").await;
    assert!(
        a.body.contains(">Could not be checked now</a>"),
        "{}",
        a.body
    );

    // A verdict, then an attempt: the badge and the verification page still read the verdict.
    for (state, label) in [
        (VerificationState::Failed, "Verification failed"),
        (VerificationState::Verified, "Verified"),
    ] {
        let w = handbook();
        w.store
            .set_verification(
                &w.wiki.id,
                Verification {
                    state,
                    at: Some(NOW - 10_000),
                },
            )
            .unwrap();
        w.store.record_attempt(&w.wiki.id, attempt).unwrap();
        for uri in ["/handbook/Home", "/handbook/-/verification"] {
            let a = get(&w, uri).await;
            assert!(
                a.body.contains(&format!(">{label}</a>")),
                "{uri}: {}",
                a.body
            );
            assert!(!a.body.contains(">Could not be checked now</a>"), "{uri}");
        }
        let page = get(&w, "/handbook/-/verification").await;
        assert!(
            page.body.contains("A later check could not be completed")
                && page.body.contains("the result above still stands"),
            "the attempt is told apart on the verification page: {}",
            page.body
        );
    }
}

#[tokio::test]
async fn an_alias_a_redirect_and_a_near_title_say_how_the_reader_got_there() {
    let w = handbook();
    let a = get(&w, "/handbook/How-to").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("<h1>Guide</h1>"));
    assert!(
        a.body.contains("How-to is another title of this page."),
        "{}",
        a.body
    );
    let r = get(&w, "/handbook/Old%20Name").await;
    assert!(r.body.contains("<h1>Guide</h1>"));
    assert!(r.body.contains("Redirected from Old Name."), "{}", r.body);
    let c = get(&w, "/handbook/guide").await;
    assert!(
        c.body
            .contains("You asked for guide. This is the page with the closest title."),
        "{}",
        c.body
    );
}

#[tokio::test]
async fn a_missing_page_is_a_404_that_invites_and_offers_no_dead_link() {
    let w = handbook();
    let a = get(&w, "/handbook/Not%20Written").await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert!(a.body.contains("This page does not exist yet"));
    assert!(
        a.body
            .contains("No page in The Handbook has the title Not Written.")
    );
    assert!(
        a.body
            .contains("<button type=\"button\" disabled>Propose a change</button>")
    );
    assert_eq!(count(&a.body, "<h1"), 1);
}

#[tokio::test]
async fn a_private_wiki_and_a_wiki_that_is_not_there_are_the_same_answer() {
    let w = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &page(&["title: Home"], "hidden\n"))],
    );
    for path in [
        "/{}",
        "/{}/Home",
        "/{}/~main/Home",
        "/{}/-/about",
        "/{}/-/verification",
        "/{}/No%20such%20page",
    ] {
        let real = get(&w, &path.replace("{}", "secret")).await;
        let absent = get(&w, &path.replace("{}", "no-such-wiki")).await;
        assert_eq!(real.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(real.status, absent.status, "{path}");
        // The only difference allowed is the reader's own address echoed in the language buttons.
        assert_eq!(
            real.body.replace("secret", "X"),
            absent.body.replace("no-such-wiki", "X"),
            "{path}: the answers differ"
        );
        assert_eq!(
            real.headers.get(header::CONTENT_LANGUAGE),
            absent.headers.get(header::CONTENT_LANGUAGE)
        );
        assert!(!real.body.contains("Secret"), "{path}");
    }
    // The verification form too: nothing is run for a wiki the caller may not read.
    let post = post_form(&w, "/secret/-/verification", true).await;
    assert_eq!(post.status, StatusCode::NOT_FOUND);
    assert_eq!(w.fake.verifies(), 0);
    // And it is not in the list.
    let list = get(&w, "/-/wikis").await;
    assert!(!list.body.contains("Secret"), "{}", list.body);
}

#[tokio::test]
async fn the_list_of_wikis_names_the_ones_a_visitor_may_read() {
    let w = handbook();
    let a = get(&w, "/-/wikis").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("<a href=\"/handbook\">The Handbook</a>"));
    let empty = site_none_wikis().await;
    assert!(empty.contains("There are no wikis you can read here yet."));
}

async fn site_none_wikis() -> String {
    let res = inklkunn_web::routes::site(
        &WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
    )
    .unwrap()
    .oneshot(
        Request::builder()
            .uri("/-/wikis")
            .header(header::HOST, "localhost:8080")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn when_prikk_is_busy_a_page_with_no_copy_is_unavailable_and_one_with_a_copy_is_served_stale()
{
    let w = handbook();
    // Nothing cached yet: there is nothing to serve.
    w.fake.fail_views(Some(busy()));
    let a = get(&w, "/handbook/Home").await;
    assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        a.headers
            .get(header::RETRY_AFTER)
            .map(|v| v.to_str().unwrap()),
        Some("30")
    );
    assert!(a.body.contains("This page could not be read now"));
    assert!(!a.body.contains("Welcome"), "never rendered from nothing");
    assert!(
        w.logged().iter().any(|l| l.contains("handbook")),
        "the operator is told: {:?}",
        w.logged()
    );
    // Read it while prikk answers; then prikk goes busy.
    w.fake.fail_views(None);
    w.store
        .set_verification(
            &w.wiki.id,
            Verification {
                state: VerificationState::Verified,
                at: Some(NOW),
            },
        )
        .unwrap();
    let fresh = get(&w, "/handbook/Home").await;
    assert!(fresh.body.contains(">Verified</a>"));
    assert!(!fresh.body.contains("may be out of date"));
    w.fake.fail_views(Some(busy()));
    let stale = get(&w, "/handbook/Home").await;
    assert_eq!(stale.status, StatusCode::OK);
    assert!(
        stale.body.contains("This page may be out of date."),
        "{}",
        stale.body
    );
    assert!(stale.body.contains("<h2 id=\"h-welcome\">Welcome</h2>"));
    // The record still says Verified, and the page does not repeat it: it could not ask just now.
    assert!(
        stale.body.contains(">Could not be checked now</a>"),
        "{}",
        stale.body
    );
    assert!(!stale.body.contains(">Verified</a>"));
    // A page that was never cached is not made up, even though the index knows its title.
    let other = get(&w, "/handbook/Guide").await;
    assert_eq!(other.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn the_verification_page_states_the_record_and_runs_a_check_on_request() {
    let w = handbook();
    let a = get(&w, "/handbook/-/verification").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("This wiki has not been checked yet."));
    assert!(
        a.body
            .contains("<button type=\"submit\">Check now</button>")
    );
    assert!(a.body.contains("<details>"));
    assert_eq!(w.fake.verifies(), 0, "reading the page checks nothing");

    // A clean check: recorded, and the badge on every page follows.
    let b = post_form(&w, "/handbook/-/verification", true).await;
    assert_eq!(b.status, StatusCode::OK);
    assert!(
        b.body
            .contains("The check you asked for found nothing wrong.")
    );
    assert!(b.body.contains(">Verified</a>"));
    assert!(b.body.contains("Last checked: 2023-11-14 22:13 UTC."));
    assert_eq!(w.fake.verifies(), 1);
    assert_eq!(
        w.store.get(&w.wiki.id).unwrap().verification.state,
        VerificationState::Verified
    );
    let home = get(&w, "/handbook/Home").await;
    assert!(home.body.contains(">Verified</a>"));

    // Asked again at once: not repeated.
    let c = post_form(&w, "/handbook/-/verification", true).await;
    assert!(c.body.contains("A check ran a moment ago"), "{}", c.body);
    assert_eq!(w.fake.verifies(), 1);
}

#[tokio::test]
async fn a_failed_check_names_the_stages_and_a_check_that_could_not_run_is_not_a_failure() {
    let w = handbook();
    w.fake.verify_says(Ok(damaged()));
    let a = post_form(&w, "/handbook/-/verification", true).await;
    assert!(a.body.contains(">Verification failed</a>"), "{}", a.body);
    assert!(a.body.contains("<li><code>refs</code></li>"), "{}", a.body);
    assert!(
        a.body
            .contains("<li><code>ref-update-schema-trust</code></li>")
    );
    assert!(a.body.contains("What the check found"));
    assert!(
        !a.body.contains("integrity error"),
        "prikk's words are not shown"
    );

    // Another wiki, where the check cannot run: a limit is never a verdict (IS-7).
    let w = handbook();
    w.fake
        .verify_says(Err(inklkunn_prikk::StorageError::Unavailable {
            verb: "verify",
            reason: inklkunn_prikk::Unavailable::CeilingExceeded { bytes: 1 << 20 },
        }));
    let b = post_form(&w, "/handbook/-/verification", true).await;
    assert!(b.body.contains(">Could not be checked now</a>"));
    assert!(!b.body.contains("Verification failed"));
    assert!(b.body.contains("needs more time or memory"));
    let record = w.store.get(&w.wiki.id).unwrap();
    assert_eq!(record.verification.state, VerificationState::Unknown);
    assert_eq!(record.last_attempt.map(|a| a.why), Some(NotRun::Limit));

    // R1: where the last verdict was a failure, a check that could not run leaves it failed.
    let w = handbook();
    w.fake.verify_says(Ok(damaged()));
    post_form(&w, "/handbook/-/verification", true).await;
    w.store
        .set_verification(
            &w.wiki.id,
            Verification {
                state: VerificationState::Failed,
                at: Some(NOW - 3600),
            },
        )
        .unwrap();
    w.fake.verify_says(Err(busy()));
    let d = post_form(&w, "/handbook/-/verification", true).await;
    assert!(d.body.contains("The server is busy with other work"));
    assert!(d.body.contains(">Verification failed</a>"), "{}", d.body);
    assert!(!d.body.contains(">Could not be checked now</a>"));
    assert_eq!(
        w.store.get(&w.wiki.id).unwrap().verification.state,
        VerificationState::Failed
    );
    // Busy says so in its own words.
    let w = handbook();
    w.fake.verify_says(Err(busy()));
    let c = post_form(&w, "/handbook/-/verification", true).await;
    assert!(c.body.contains("The server is busy with other work"));
    let _ = clean();
}

#[tokio::test]
async fn a_form_from_another_site_runs_nothing() {
    let w = handbook();
    let a = post_form(&w, "/handbook/-/verification", false).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(a.body.contains("The form was not accepted"));
    assert_eq!(w.fake.verifies(), 0);
}

#[tokio::test]
async fn views_and_titles_follow_the_url_scheme() {
    let w = handbook();
    let v = get(&w, "/handbook/~main/Guide").await;
    assert_eq!(v.status, StatusCode::OK);
    assert!(
        v.body.contains("You are reading the view main."),
        "{}",
        v.body
    );
    assert!(
        v.body.contains("href=\"/handbook/~main/Guide\"") || v.body.contains("/handbook/~main/"),
        "links stay in the view: {}",
        v.body
    );
    let none = get(&w, "/handbook/~nope/Guide").await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    assert!(none.body.contains("This wiki has no view called nope."));
    let odd = get(&w, "/handbook/~bad%20name/Guide").await;
    assert_eq!(odd.status, StatusCode::NOT_FOUND);
    // A title that begins with a tilde is written %7E and is a title, never a view.
    let tilde = get(&w, "/handbook/%7Eodd").await;
    assert_eq!(tilde.status, StatusCode::OK, "{}", tilde.body);
    assert!(tilde.body.contains("<h1>~odd</h1>"));
    // Titles that cannot name a page, and addresses that cannot be decoded, are pages, not bare errors.
    let long = format!("/handbook/{}", "x".repeat(300));
    for uri in [long.as_str(), "/handbook/%ff", "/handbook/%"] {
        let a = get(&w, uri).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            a.body.contains("That title cannot name a page"),
            "{uri}: {}",
            a.body
        );
    }
}

#[tokio::test]
async fn the_front_door_leads_to_home_or_to_about() {
    let w = handbook();
    let a = get(&w, "/handbook").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER);
    assert_eq!(
        a.headers.get(header::LOCATION).map(|v| v.to_str().unwrap()),
        Some("/handbook/Home")
    );
    let bare = World::new(
        "bare",
        "Bare",
        &[("pages/Help.md", &page(&["title: Help"], "x\n"))],
    );
    let b = get(&bare, "/bare").await;
    assert_eq!(b.status, StatusCode::SEE_OTHER);
    assert_eq!(
        b.headers.get(header::LOCATION).map(|v| v.to_str().unwrap()),
        Some("/bare/-/about")
    );
}

#[tokio::test]
async fn the_about_page_states_the_license_and_what_the_accessibility_tests_do_not_cover() {
    let w = handbook();
    let a = get(&w, "/handbook/-/about").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("About The Handbook"));
    assert!(a.body.contains("CC-BY-SA-4.0"));
    assert!(
        a.body
            .contains("The view shown when none is named is main.")
    );
    assert!(
        a.body
            .contains("No person who uses assistive technology has tested this site yet.")
    );
    assert!(
        a.body.contains("The Japanese translation")
            || a.body.contains("This translation is a preview")
    );
}

#[tokio::test]
async fn an_authors_html_never_becomes_the_renderers_in_a_served_page() {
    let w = handbook();
    let a = get(&w, "/handbook/Forged").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(
        a.body
            .contains("&lt;a class=\"wikilink\" href=\"https://evil.example/\">Home&lt;/a&gt;")
            || a.body.contains(
                "&lt;a class=\"wikilink\" href=\"https://evil.example/\"&gt;Home&lt;/a&gt;"
            ),
        "{}",
        a.body
    );
    assert!(
        !a.body
            .contains("<a class=\"wikilink\" href=\"https://evil.example/\"")
    );
}

#[tokio::test]
async fn an_address_that_does_not_take_the_method_answers_with_a_page_and_says_what_it_takes() {
    let w = handbook();
    for (method, uri) in [
        (Method::POST, "/handbook/Home"),
        (Method::PUT, "/-/wikis"),
        (Method::DELETE, "/handbook/-/about"),
    ] {
        let a = send(&w, method.clone(), uri, &[]).await;
        assert_eq!(a.status, StatusCode::METHOD_NOT_ALLOWED, "{method} {uri}");
        assert!(
            a.body.contains("That is not possible at this address"),
            "{}",
            a.body
        );
        assert!(a.body.contains("<html lang=\"en\""));
        assert!(a.headers.contains_key(header::CONTENT_SECURITY_POLICY));
        assert!(a.headers.contains_key(header::ALLOW), "{method} {uri}");
    }
}

#[tokio::test]
async fn a_heading_called_main_cannot_take_the_skip_links_target() {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[(
            "pages/Home.md",
            &page(&["title: Home"], "# Main\n\nBack to [[#Main]].\n"),
        )],
    );
    let a = get(&w, "/handbook/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    // One element answers to `main`: the template's. The heading has its own prefixed id, and the contents
    // list and the page's own link point at that.
    assert_eq!(count(&a.body, "id=\"main\""), 1, "{}", a.body);
    assert!(a.body.contains("<h2 id=\"h-main\">Main</h2>"), "{}", a.body);
    assert!(
        a.body.contains("<a href=\"#h-main\">Main</a>"),
        "{}",
        a.body
    );
    assert!(
        a.body.contains("wikilink-self\" href=\"#h-main\""),
        "{}",
        a.body
    );
    assert_eq!(
        count(&a.body, "href=\"#main\""),
        1,
        "only the skip link: {}",
        a.body
    );
}
