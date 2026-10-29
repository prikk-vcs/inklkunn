//! **The route-enumeration harness** (threat model INV-4; RFC 002 D-2; handoff 003 T4 … T8): every entry point the
//! route registry declares as guarded by an `Action` is requested, with each of its methods, for a wiki the
//! visitor may not read and for a wiki that is not there, and the two answers must be the same refusal — no
//! status, header or byte tells them apart (but for the visitor's own address, echoed in the language buttons).
//! The harness walks the registry itself, so a route added tomorrow is covered the day it is registered, and a
//! route whose handler forgets to ask `authorize()` fails here. It also holds each route to what it promises: a
//! guarded route is a `Read`, a route that is public gives a reason, and nothing of the restricted wiki is
//! in any answer.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::{World, page};
use inklkunn_core::{Action, Guard};
use inklkunn_store::Visibility;
use inklkunn_web::WebConfig;
use inklkunn_web::routes::{application_table, content_table, site_with};
use tower::ServiceExt as _;

const BLOCK: &str = "00000000000000000000000000000000000000000000000000000000000000b1";

/// A concrete address for a registered pattern, for the wiki named `slug`.
fn address(pattern: &str, slug: &str) -> String {
    pattern
        .replace("{slug}", slug)
        .replace("{*title}", "Home")
        .replace("{*rest}", "pages")
        .replace("{block}", BLOCK)
        .replace("{from}", BLOCK)
        .replace("{to}", BLOCK)
        .replace("{list}", "wanted")
        .replace("{file}", "x")
}

struct Answer {
    status: StatusCode,
    headers: Vec<(String, String)>,
    body: String,
}

async fn ask(world: &World, host: &str, method: &str, uri: &str) -> Answer {
    let config = WebConfig::new(
        "127.0.0.1:8080",
        "http://localhost:8080",
        "http://localhost:8081",
    )
    .unwrap();
    let site = site_with(&config, Some(world.services.clone())).unwrap();
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, host);
    if method == "POST" {
        req = req
            .header("content-type", "application/x-www-form-urlencoded")
            .header("sec-fetch-site", "same-origin");
    }
    let res = site
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 22).await.unwrap();
    let mut headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_owned()))
        .collect();
    headers.sort();
    Answer {
        status: parts.status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn restricted() -> World {
    World::with_visibility(
        "secret",
        "Secret Wiki",
        Visibility::Restricted,
        &[(
            "pages/Home.md",
            &page(&["title: Home"], "the hidden text\n"),
        )],
    )
}

#[tokio::test]
async fn every_guarded_route_answers_a_restricted_wiki_exactly_as_one_that_is_not_there() {
    let world = restricted();
    let mut walked = 0;
    for (table, host) in [
        (application_table().unwrap(), "localhost:8080"),
        (content_table().unwrap(), "localhost:8081"),
    ] {
        for (entry, action) in table.guarded() {
            assert_eq!(
                action,
                Action::Read,
                "{}: the routes of this release only read",
                entry.path
            );
            if !entry.path.contains("{slug}") {
                // The list of wikis names no single wiki; it is checked for leaking below.
                continue;
            }
            for method in entry.methods.iter().filter(|m| **m != "HEAD") {
                let real = ask(&world, host, method, &address(entry.path, "secret")).await;
                let absent = ask(&world, host, method, &address(entry.path, "no-such-wiki")).await;
                let which = format!("{method} {}", entry.path);
                assert_eq!(real.status, absent.status, "{which}");
                assert!(
                    real.status.is_client_error(),
                    "{which}: a restricted wiki must refuse, got {}",
                    real.status
                );
                // The visitor's own address is echoed in the language buttons; nothing else may differ.
                let norm = |a: &Answer, slug: &str| {
                    // The length differs only by the echoed address.
                    let headers: Vec<_> = a
                        .headers
                        .iter()
                        .filter(|(k, _)| k != "content-length")
                        .cloned()
                        .collect();
                    (a.status, headers, a.body.replace(slug, "X"))
                };
                assert_eq!(
                    norm(&real, "secret"),
                    norm(&absent, "no-such-wiki"),
                    "{which}: the refusal tells a restricted wiki from an absent one"
                );
                for secret in ["hidden text", "Secret Wiki"] {
                    assert!(!real.body.contains(secret), "{which}: leaked `{secret}`");
                }
                walked += 1;
            }
        }
    }
    assert!(
        walked >= 20,
        "the harness walked only {walked} routes: is the registry being read?"
    );
}

#[tokio::test]
async fn the_list_of_wikis_never_names_a_wiki_the_visitor_may_not_read() {
    let world = restricted();
    let a = ask(&world, "localhost:8080", "GET", "/-/wikis").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(!a.body.contains("Secret Wiki") && !a.body.contains("/secret"));
}

#[test]
fn every_public_route_gives_a_reason_and_is_not_under_a_wiki_slug_unless_it_refuses() {
    for table in [application_table().unwrap(), content_table().unwrap()] {
        for entry in table.entries() {
            if let Guard::Public { reason } = entry.guard {
                assert!(reason.len() >= 16, "{}: {reason}", entry.path);
                if entry.path.contains("{slug}") {
                    assert!(
                        reason.starts_with("refuses")
                            || reason.contains("no wiki data")
                            || reason.contains("reads nothing"),
                        "{}: a public route under a wiki must refuse or serve no wiki data: {reason}",
                        entry.path
                    );
                }
            }
        }
    }
}
