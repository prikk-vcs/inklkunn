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
const PROPOSAL: &str = "019a0000-0000-7000-8000-00000000cccc";

/// A concrete address for a registered pattern, for the wiki named `slug`.
fn address(pattern: &str, slug: &str) -> String {
    pattern
        .replace("{slug}", slug)
        .replace("{*title}", "Home")
        .replace("{*rest}", "pages")
        .replace("{id}", PROPOSAL)
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
            // The pages read; the proposals' writes carry the action their handler authorizes with (RFC 004).
            // (The write API's endpoints are the same acts under the API's prefix, and take `POST`.)
            let writes = (entry.path.contains("/-/proposals/")
                && entry.path != "/{slug}/-/proposals/{id}")
                || (entry.path.starts_with("/{slug}/-/api/v1/proposals")
                    && entry.methods == ["POST"]);
            // A view's creation is its own action (RFC 005 part 2 D-9; handoff 005 K-T8).
            let creates_a_view = entry.path == "/{slug}/-/views/new";
            // A comment in a claim's own discussion is made with the action a comment on a proposal is (RFC 005 part 2
            // D-11; handoff 005 K-T10).
            let comments_on_a_claim = entry.path == "/{slug}/-/claim/{id}/discuss";
            // Putting a temporary protection in place is the `moderate` rung's (RFC 006 D-7; handoff G-T3).
            let moderates = entry.path == "/{slug}/-/protect";
            let expected = if moderates {
                vec![Action::Moderate]
            } else if creates_a_view {
                vec![Action::CreateView]
            } else if comments_on_a_claim {
                vec![Action::Comment]
            } else if writes {
                [
                    Action::Propose,
                    Action::Comment,
                    Action::Review,
                    Action::Accept,
                ]
                .to_vec()
            } else {
                vec![Action::Read]
            };
            assert!(
                expected.contains(&action),
                "{}: {action:?} is not an action this route may carry",
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

/// Every address of the knowledge layer that names a claim, a source or a page's provenance, for the wiki `{}`: the claim
/// page, *Why is this here?*, the list of views and *Compare views* (each on the main view and on another), and the API's
/// `claims/{id}`, `sources/{id}`, `provenance/{title}` and view comparison.
fn knowledge_addresses() -> Vec<String> {
    let claim = common::claim_id(1);
    let source = common::source_id(1);
    [
        format!("/{{}}/-/claim/{claim}"),
        format!("/{{}}/-/claim/{claim}?view=alt"),
        "/{}/-/why/Home".to_owned(),
        "/{}/-/why/Home?view=alt".to_owned(),
        "/{}/-/views".to_owned(),
        "/{}/-/views/compare?a=main&b=alt".to_owned(),
        format!("/{{}}/-/api/v1/claims/{claim}"),
        format!("/{{}}/-/api/v1/sources/{source}"),
        "/{}/-/api/v1/provenance/Home".to_owned(),
        "/{}/-/api/v1/views/compare?b=alt".to_owned(),
    ]
    .into()
}

/// A wiki whose `Home` says a claim (with a source) in its first paragraph, and has a second view.
fn holding_a_claim(slug: &str, name: &str, visibility: Visibility) -> World {
    let said = "The hidden bridge opened in 1950.";
    let w = common::with_accounts(World::with_visibility(
        slug,
        name,
        visibility,
        // The paragraph is worded differently when the wiki is created, so that the change that adds the claim is the
        // one that last changed it.
        &[(
            "pages/Home.md",
            &common::home_with("The hidden bridge opened in 1949."),
        )],
    ));
    w.fake.add_view("alt");
    let mut c = common::claim(1, inklkunn_core::knowledge::Standing::Contested, None, said);
    c.sources = vec![common::link(
        1,
        inklkunn_core::knowledge::Link::Supports,
        inklkunn_core::knowledge::Bearing::Direct,
        said,
    )];
    let mut source = common::source(1, inklkunn_core::knowledge::SourceKind::Primary);
    source.reference = Some(inklkunn_core::knowledge::Reference {
        form: inklkunn_core::knowledge::ReferenceForm::Citation,
        value: HIDDEN_REFERENCE.to_owned(),
    });
    common::put(&w, &common::home_with(said), &[c], &[source]);
    // The change that wrote the claim and the source, so that *Why is this here?* and the provenance list the source
    // the change attached (maintenance M56): its reference is the wiki's own, and a visitor who may not read the wiki
    // is told no more of it than of the claim.
    w.fake.say(1, "Create the wiki");
    w.fake.show(
        1,
        common::shown(vec![common::op("create-file", &["pages/Home.md"])]),
    );
    w.fake.say(2, "Add a claim and its source");
    let source_file = inklkunn_core::knowledge::source_path(&common::source_id(1));
    w.fake.show(
        2,
        common::shown(vec![
            common::op("edit-text", &["pages/Home.md"]),
            common::op("create-file", &[source_file.as_str()]),
        ]),
    );
    w
}

/// A reference only the wiki's own readers may see.
const HIDDEN_REFERENCE: &str = "Hidden harbour ledger, folio 7";

async fn ask_as(
    world: &World,
    method: &str,
    uri: &str,
    account: Option<&inklkunn_store::AccountId>,
) -> Answer {
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
        .header(header::HOST, "localhost:8080");
    if let Some(account) = account {
        req = req.header("x-test-account", account.as_str());
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

#[tokio::test]
async fn a_restricted_wiki_that_holds_a_claim_answers_every_knowledge_address_as_an_absent_wiki_does()
 {
    // RFC 005 part 2 and threat model INV-4 (existence neutrality): a claim, a source, a page's provenance and the
    // comparison of views are the wiki's own, so a visitor who may not read the wiki is told nothing a wiki that is not
    // there would not tell: not by status, header or byte (but the visitor's own address echoed in the language buttons).
    let secret = holding_a_claim("secret", "Secret Wiki", Visibility::Restricted);
    // A public wiki with the same claim, to show the addresses are real and answer when they may.
    let open = holding_a_claim("open", "Open Wiki", Visibility::Public);
    let stranger = common::person(9);
    common::add_person(&secret, &stranger, "A stranger", None);
    let member = common::person(1);
    common::let_read(&secret, &member);

    let mut compared = 0;
    for pattern in knowledge_addresses() {
        // The addresses are real: on a wiki that may be read, with the same claim, each answers a page or a document.
        let read = ask_as(&open, "GET", &pattern.replace("{}", "open"), None).await;
        assert_eq!(read.status, StatusCode::OK, "{pattern} on a public wiki");
        assert!(
            read.body.contains("hidden bridge")
                || read.body.contains(common::claim_id(1).as_str())
                || read.body.contains(common::source_id(1).as_str())
                || pattern.contains("/views"),
            "{pattern}: the control page does not carry the claim: {}",
            read.body
        );
        // The pages that list what a change attached carry the source's reference for a reader, as text.
        if pattern.contains("/why/Home") || pattern.contains("/provenance/Home") {
            assert!(
                read.body.contains(HIDDEN_REFERENCE),
                "{pattern}: the reference of the source a change attached is listed: {}",
                read.body
            );
        }
        // The person the wiki is open to reads it all the same.
        let member_read = ask_as(
            &secret,
            "GET",
            &pattern.replace("{}", "secret"),
            Some(&member),
        )
        .await;
        assert_eq!(member_read.status, StatusCode::OK, "{pattern} for a reader");

        for (who, account) in [
            ("anonymous", None),
            ("a signed-in non-member", Some(&stranger)),
        ] {
            for method in ["GET", "HEAD"] {
                let real = ask_as(&secret, method, &pattern.replace("{}", "secret"), account).await;
                let absent = ask_as(
                    &secret,
                    method,
                    &pattern.replace("{}", "no-such-wiki"),
                    account,
                )
                .await;
                let which = format!("{method} {pattern} as {who}");
                assert_eq!(real.status, StatusCode::NOT_FOUND, "{which}");
                let norm = |a: &Answer, slug: &str| {
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
                    "{which}: the refusal tells a restricted wiki that holds a claim from an absent one"
                );
                for hidden in [
                    "hidden bridge",
                    "Secret Wiki",
                    HIDDEN_REFERENCE,
                    &common::claim_id(1).to_string(),
                    &common::source_id(1).to_string(),
                ] {
                    assert!(
                        !real.body.contains(hidden) || pattern.contains(hidden),
                        "{which}: leaked `{hidden}`"
                    );
                }
                compared += 1;
            }
        }
    }
    assert_eq!(compared, knowledge_addresses().len() * 4);
}

#[tokio::test]
async fn the_list_of_wikis_never_names_a_wiki_the_visitor_may_not_read() {
    let world = restricted();
    let a = ask(&world, "localhost:8080", "GET", "/-/wikis").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(!a.body.contains("Secret Wiki") && !a.body.contains("/secret"));
}

#[tokio::test]
async fn the_inbox_names_no_wiki_to_a_visitor_with_no_account() {
    // It is an account-wide page: for a visitor with none it is the same words whatever wikis exist, and it
    // never names the restricted one.
    let world = restricted();
    for method in ["GET", "POST"] {
        let a = ask(&world, "localhost:8080", method, "/-/inbox").await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{method}");
        for secret in ["Secret Wiki", "/secret", "hidden text"] {
            assert!(!a.body.contains(secret), "{method}: leaked `{secret}`");
        }
        assert!(a.body.contains("An account is needed"), "{method}");
    }
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
