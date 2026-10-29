//! Every state of a wiki's pages (handoff 003 T4), reached through the real router over an in-memory
//! repository: found (in each badge state), reached by an alias and by a redirect, missing, ambiguous, a
//! redirect that leads nowhere, unreadable, no such view, an invalid title, unavailable, served stale, the
//! about page, the verification page (before and after a check, each way a check can end), the list of wikis,
//! and an address that does not take the method. Each is a page the structural rules run on.

use axum::body::Body;
use axum::http::{Request, header};
use inklkunn_store::{Verification, VerificationState, WikiStore};
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site_with;
use tower::ServiceExt as _;

use super::common::{self, NOW, World, busy, damaged, page};
use super::{Out, State};

/// One state: how to get the world ready, and the request that shows it.
pub(super) struct Scenario {
    pub id: &'static str,
    pub identity: &'static str,
    pub status: u16,
    /// Addresses read first, so that a copy is cached.
    pub warm: &'static [&'static str],
    pub setup: fn(&World),
    pub method: &'static str,
    pub uri: &'static str,
    pub headers: &'static [(&'static str, &'static str)],
}

fn nothing(_: &World) {}

fn record(w: &World, state: VerificationState) {
    w.store
        .set_verification(
            &w.wiki.id,
            Verification {
                state,
                at: Some(NOW - 3600),
            },
        )
        .unwrap();
}

const SAME_SITE: &[(&str, &str)] = &[
    ("content-type", "application/x-www-form-urlencoded"),
    ("sec-fetch-site", "same-origin"),
];
const CROSS_SITE: &[(&str, &str)] = &[
    ("content-type", "application/x-www-form-urlencoded"),
    ("sec-fetch-site", "cross-site"),
];

pub(super) const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "wiki-page",
        identity: "wiki-page-home",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-verified",
        identity: "wiki-page-home",
        status: 200,
        warm: &[],
        setup: |w| record(w, VerificationState::Verified),
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-failed",
        identity: "wiki-page-home",
        status: 200,
        warm: &[],
        setup: |w| record(w, VerificationState::Failed),
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-could-not-check",
        identity: "wiki-page-home",
        status: 200,
        warm: &[],
        setup: |w| record(w, VerificationState::CouldNotCheck),
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-in-a-view",
        identity: "wiki-page-home",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/~main/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-by-alias",
        identity: "wiki-page-guide",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/How-to",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-by-redirect",
        identity: "wiki-page-guide",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/Old%20Name",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-near-title",
        identity: "wiki-page-guide",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/guide",
        headers: &[],
    },
    Scenario {
        id: "wiki-page-stale",
        identity: "wiki-page-home",
        status: 200,
        warm: &["/handbook/Home"],
        setup: |w| w.fake.fail_views(Some(busy())),
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-missing",
        identity: "wiki-missing",
        status: 404,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/Not%20Written",
        headers: &[],
    },
    Scenario {
        id: "wiki-ambiguous",
        identity: "wiki-ambiguous",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/Twin",
        headers: &[],
    },
    Scenario {
        id: "wiki-redirect-dangling",
        identity: "wiki-redirect-broken",
        status: 404,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/Dangling",
        headers: &[],
    },
    Scenario {
        id: "wiki-redirect-loop",
        identity: "wiki-redirect-broken",
        status: 404,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/Round",
        headers: &[],
    },
    Scenario {
        id: "wiki-unreadable",
        identity: "wiki-unreadable",
        status: 422,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/Broken",
        headers: &[],
    },
    Scenario {
        id: "wiki-no-such-view",
        identity: "wiki-no-such-view",
        status: 404,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/~nope/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-invalid-title",
        identity: "wiki-invalid-title",
        status: 404,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/%ff",
        headers: &[],
    },
    Scenario {
        id: "wiki-unavailable",
        identity: "wiki-unavailable",
        status: 503,
        warm: &[],
        setup: |w| w.fake.fail_views(Some(busy())),
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    Scenario {
        id: "wiki-about",
        identity: "wiki-about",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/-/about",
        headers: &[],
    },
    Scenario {
        id: "wiki-verification",
        identity: "wiki-verification",
        status: 200,
        warm: &[],
        setup: |w| record(w, VerificationState::Verified),
        method: "GET",
        uri: "/handbook/-/verification",
        headers: &[],
    },
    Scenario {
        id: "wiki-verification-never",
        identity: "wiki-verification",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/handbook/-/verification",
        headers: &[],
    },
    Scenario {
        id: "wiki-verification-clean",
        identity: "wiki-verification",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "POST",
        uri: "/handbook/-/verification",
        headers: SAME_SITE,
    },
    Scenario {
        id: "wiki-verification-damaged",
        identity: "wiki-verification",
        status: 200,
        warm: &[],
        setup: |w| w.fake.verify_says(Ok(damaged())),
        method: "POST",
        uri: "/handbook/-/verification",
        headers: SAME_SITE,
    },
    Scenario {
        id: "wiki-verification-busy",
        identity: "wiki-verification",
        status: 200,
        warm: &[],
        setup: |w| w.fake.verify_says(Err(busy())),
        method: "POST",
        uri: "/handbook/-/verification",
        headers: SAME_SITE,
    },
    Scenario {
        id: "wiki-verification-recent",
        identity: "wiki-verification",
        status: 200,
        warm: &[],
        setup: |w| {
            w.store
                .set_verification(
                    &w.wiki.id,
                    Verification {
                        state: VerificationState::Verified,
                        at: Some(NOW - 5),
                    },
                )
                .unwrap();
        },
        method: "POST",
        uri: "/handbook/-/verification",
        headers: SAME_SITE,
    },
    Scenario {
        id: "wiki-verification-refused",
        identity: "form-refused",
        status: 403,
        warm: &[],
        setup: nothing,
        method: "POST",
        uri: "/handbook/-/verification",
        headers: CROSS_SITE,
    },
    Scenario {
        id: "wikis",
        identity: "wikis",
        status: 200,
        warm: &[],
        setup: nothing,
        method: "GET",
        uri: "/-/wikis",
        headers: &[],
    },
    Scenario {
        id: "method-not-allowed",
        identity: "method-not-allowed",
        status: 405,
        warm: &[],
        setup: nothing,
        method: "POST",
        uri: "/handbook/Home",
        headers: &[],
    },
];

/// The wiki every scenario starts from.
fn handbook() -> World {
    World::new(
        "handbook",
        "The Handbook",
        &[
            (
                "pages/Home.md",
                &page(
                    &["title: Home", "description: Start here"],
                    "# Welcome\n\nSee [[Guide]] and [[Nowhere]].\n\n## Background\n\nMore.\n",
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
            ("pages/a/Twin.md", &page(&["title: Twin"], "One.\n")),
            ("pages/b/Twin.md", &page(&["title: Twin"], "Two.\n")),
            (
                "pages/Dangling.md",
                &page(
                    &[
                        "title: Dangling",
                        "kind: redirect",
                        "redirect: Nowhere At All",
                    ],
                    "",
                ),
            ),
            (
                "pages/Round.md",
                &page(
                    &["title: Round", "kind: redirect", "redirect: Round Two"],
                    "",
                ),
            ),
            (
                "pages/Round Two.md",
                &page(
                    &["title: Round Two", "kind: redirect", "redirect: Round"],
                    "",
                ),
            ),
            (
                "pages/Broken.md",
                "---\ntitle: &anchor Broken\n---\n\nBody.\n",
            ),
        ],
    )
}

/// Build every scenario's state, with `extra` request headers (the language cookie).
pub(super) async fn build(extra: &[(&str, &str)], out: &mut Out) {
    let config = WebConfig::new(
        "127.0.0.1:8080",
        "http://localhost:8080",
        "http://localhost:8081",
    )
    .unwrap();
    for sc in SCENARIOS {
        let w = handbook();
        let router = site_with(&config, Some(w.services.clone())).unwrap();
        let request = |method: &str, uri: &str, headers: &[(&str, &str)]| {
            let mut b = Request::builder()
                .method(method)
                .uri(uri)
                .header(header::HOST, "localhost:8080");
            for (k, v) in extra.iter().chain(headers) {
                b = b.header(*k, *v);
            }
            b.body(Body::empty()).unwrap()
        };
        for uri in sc.warm {
            let _ = router
                .clone()
                .oneshot(request("GET", uri, &[]))
                .await
                .unwrap();
        }
        (sc.setup)(&w);
        let res = router
            .clone()
            .oneshot(request(sc.method, sc.uri, sc.headers))
            .await
            .unwrap();
        let status = res.status().as_u16();
        let bytes = axum::body::to_bytes(res.into_body(), 16 << 20)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes).into_owned();
        assert_eq!(status, sc.status, "{}: {html}", sc.id);
        if html.contains("<html") {
            out.states.push(State {
                id: sc.id.to_owned(),
                identity: sc.identity,
                status,
                html,
            });
        } else {
            out.skipped
                .0
                .push(format!("{}: not an HTML document", sc.id));
        }
    }
    let _ = common::clean();
}
