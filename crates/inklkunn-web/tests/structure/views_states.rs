//! Every state of the views' pages (handoff 005 K-T8; RFC 005 part 2 D-9), reached through the real router over an
//! in-memory repository, the real proposal service and the real acceptor: the list (the default view alone, with
//! views of each kind and one the service did not make, past its bound), the form that creates a view (who is let
//! in, with each problem, on a server that cannot create views), every way a creation can end, the comparison (the
//! chooser, the same view twice, two views at one change, two that differ, past the bounds, a view that is not
//! there, a diff that could not be read), one page's difference between two views, and a claim's page with
//! *Other views* (reading differently, alike everywhere, past the bound, a view that could not be read). Each is a page
//! the structural rules run on, in both languages.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, header};
use inklkunn_core::ViewFailure;
use inklkunn_core::knowledge::{Bearing, Link, SourceKind, Standing};
use inklkunn_prikk::storage::DiffSide;
use inklkunn_prikk::{Diff, DiffEntry};
use inklkunn_store::{
    AccountId, ActStore, Role, ViewActEnd, ViewActId, ViewActOutcome, ViewActRecord, ViewKind,
};
use inklkunn_web::WebConfig;
use inklkunn_web::catalog::{Lang, T};
use inklkunn_web::pages::Page;
use inklkunn_web::routes::site_with;
use inklkunn_web::views::{ViewOutcome, render_view_outcome};
use tower::ServiceExt as _;

use super::common::{
    World, add_person, claim, home_with, link, person, put, source, with_accounts,
};
use super::view_stubs::{attach, attach_stub};
use super::{Out, State};

const SAID: &str = "The bridge opened in 1950.";
const NOW_SAID: &str = "The bridge was opened in 1950.";

struct Env {
    w: World,
    contributor: AccountId,
    reviewer: AccountId,
}

impl Env {
    fn new() -> Env {
        let w = with_accounts(World::new(
            "handbook",
            "The Handbook",
            &[("pages/Home.md", &home_with(SAID))],
        ));
        let (contributor, reviewer) = (person(1), person(2));
        add_person(&w, &contributor, "Aiko Tanaka", Some(Role::Contributor));
        add_person(&w, &reviewer, "Rin Sato", Some(Role::Reviewer));
        let (w, _signer) = attach(w);
        Env {
            w,
            contributor,
            reviewer,
        }
    }
}

struct Req {
    method: &'static str,
    uri: String,
    account: Option<AccountId>,
    form: Vec<(String, String)>,
}

fn get(uri: &str, account: Option<&AccountId>) -> Req {
    Req {
        method: "GET",
        uri: uri.to_owned(),
        account: account.cloned(),
        form: Vec::new(),
    }
}

fn post(account: Option<&AccountId>, form: &[(&str, &str)]) -> Req {
    Req {
        method: "POST",
        uri: NEW.to_owned(),
        account: account.cloned(),
        form: form
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

pub(super) struct Scenario {
    id: &'static str,
    identity: &'static str,
    status: u16,
    make: fn(&mut Env) -> Req,
}

const NEW: &str = "/handbook/-/views/new";
const LIST: &str = "/handbook/-/views";
const COMPARE: &str = "/handbook/-/views/compare?a=main&b=alt";
const CLAIM: &str = "/handbook/-/claim/01930000-0000-7000-8000-000000000001";

fn report(entries: Vec<DiffEntry>) -> Diff {
    let side = |p: &str| DiffSide {
        point: p.to_owned(),
        target_block_id: "b".repeat(64),
    };
    Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: side("a"),
        to: side("b"),
        entries,
        unsupported_paths: Vec::new(),
    }
}

fn entry(path: &str, status: &str, hunk: &str) -> DiffEntry {
    DiffEntry {
        path: path.to_owned(),
        status: status.to_owned(),
        from: None,
        to: None,
        hunks: if hunk.is_empty() {
            Vec::new()
        } else {
            vec![hunk.to_owned()]
        },
    }
}

/// `alt` is `main` as it was; `main` has moved on in the wording of `Home`, the status and the sources of its
/// claim, and has a claim of its own.
fn forked(e: &Env) {
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
    let mut now = claim(1, Standing::Accepted, None, NOW_SAID);
    now.sources = vec![
        link(1, Link::Supports, Bearing::Direct, NOW_SAID),
        link(2, Link::Challenges, Bearing::Partial, NOW_SAID),
    ];
    put(
        &e.w,
        &home_with(NOW_SAID),
        &[now, claim(2, Standing::Unresolved, None, NOW_SAID)],
        &[
            source(1, SourceKind::Primary),
            source(2, SourceKind::Secondary),
        ],
    );
    // A page only `main` has.
    e.w.fake
        .set("pages/Extra.md", "---\ntitle: Extra\n---\n\nHello.\n");
    e.w.fake.diff_says(Ok(report(vec![
        entry(
            "pages/Home.md",
            "modified",
            "@@ -6,1 +6,1 @@\n-The bridge opened in 1950. ^b1\n+The bridge was opened in 1950. ^b1\n",
        ),
        entry("pages/Extra.md", "removed", "@@ -1,1 +0,0 @@\n-Hello.\n"),
        entry("pages/Old.md", "added", "@@ -0,0 +1,1 @@\n+Bye.\n"),
    ])));
}

fn alike(e: &Env) {
    put(
        &e.w,
        &home_with(SAID),
        &[claim(1, Standing::Accepted, None, SAID)],
        &[],
    );
}

pub(super) const SCENARIOS: &[Scenario] = &[
    // --- the list
    Scenario {
        id: "views-list-only-default",
        identity: "views-list",
        status: 200,
        make: |_| get(LIST, None),
    },
    Scenario {
        id: "views-list-full",
        identity: "views-list",
        status: 200,
        make: |e| {
            e.w.fake.add_view("elsewhere");
            Req {
                method: "GET",
                uri: LIST.to_owned(),
                account: Some(e.reviewer.clone()),
                form: Vec::new(),
            }
        },
    },
    Scenario {
        id: "views-list-made",
        identity: "views-list",
        status: 200,
        make: |e| {
            // One of each kind, as the service records a view it made.
            for (name, kind) in [
                ("reading", ViewKind::Interpretation),
                ("scratch", ViewKind::Experimental),
            ] {
                e.w.fake.add_view(name);
                let act = ViewActRecord {
                    id: ViewActId::new(),
                    wiki: e.w.wiki.id.clone(),
                    name: name.to_owned(),
                    forked_from: "main".to_owned(),
                    from_block: "e".repeat(64),
                    kind,
                    created_by: e.reviewer.clone(),
                    created_at: 1,
                };
                e.w.store.begin_view_act(&act).unwrap();
                e.w.store
                    .finish_view_act(&ViewActEnd {
                        act: act.id,
                        outcome: ViewActOutcome::Created,
                        at: 2,
                    })
                    .unwrap();
            }
            get(LIST, None)
        },
    },
    Scenario {
        id: "views-list-omitted",
        identity: "views-list",
        status: 200,
        make: |e| {
            for n in 0..105 {
                e.w.fake.add_view(&format!("v{n:03}"));
            }
            get(LIST, None)
        },
    },
    // --- the form
    Scenario {
        id: "views-new-form",
        identity: "views-new",
        status: 200,
        make: |e| get(NEW, Some(&e.reviewer.clone())),
    },
    Scenario {
        id: "views-new-needs-account",
        identity: "propose-needs-account",
        status: 403,
        make: |_| get(NEW, None),
    },
    Scenario {
        id: "views-new-not-allowed",
        identity: "propose-not-allowed",
        status: 403,
        make: |e| get(NEW, Some(&e.contributor.clone())),
    },
    Scenario {
        id: "views-new-problems",
        identity: "views-new",
        status: 422,
        make: |e| {
            post(
                Some(&e.reviewer.clone()),
                &[("name", "Not A Name"), ("kind", ""), ("from", "nowhere")],
            )
        },
    },
    Scenario {
        id: "views-new-name-empty",
        identity: "views-new",
        status: 422,
        make: |e| {
            post(
                Some(&e.reviewer.clone()),
                &[("name", ""), ("kind", "experimental"), ("from", "main")],
            )
        },
    },
    Scenario {
        id: "views-new-name-start",
        identity: "views-new",
        status: 422,
        make: |e| {
            post(
                Some(&e.reviewer.clone()),
                &[("name", "-x"), ("kind", "experimental"), ("from", "main")],
            )
        },
    },
    Scenario {
        id: "views-new-name-long",
        identity: "views-new",
        status: 422,
        make: |e| {
            post(
                Some(&e.reviewer.clone()),
                &[
                    (
                        "name",
                        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                    ),
                    ("kind", "experimental"),
                    ("from", "main"),
                ],
            )
        },
    },
    Scenario {
        id: "views-new-taken",
        identity: "views-new",
        status: 409,
        make: |e| {
            e.w.fake.add_view("draft");
            post(
                Some(&e.reviewer.clone()),
                &[
                    ("name", "draft"),
                    ("kind", "experimental"),
                    ("from", "main"),
                ],
            )
        },
    },
    Scenario {
        id: "views-new-source-gone",
        identity: "views-new",
        status: 422,
        make: |e| {
            attach_stub(&mut e.w, ViewFailure::NoSuchSource);
            post(
                Some(&e.reviewer.clone()),
                &[("name", "fine"), ("kind", "experimental"), ("from", "main")],
            )
        },
    },
    Scenario {
        id: "views-new-not-kept",
        identity: "views-new-not-kept",
        status: 501,
        make: |e| {
            e.w.services = Arc::new(inklkunn_web::Services {
                acceptor: None,
                ..(*e.w.services).clone()
            });
            get(NEW, Some(&e.reviewer.clone()))
        },
    },
    Scenario {
        id: "views-new-unconfigured",
        identity: "views-new-unconfigured",
        status: 503,
        make: |e| {
            e.w.services = Arc::new(inklkunn_web::Services {
                acceptor: Some(Arc::new(inklkunn_web::NoSigner)),
                ..(*e.w.services).clone()
            });
            get(NEW, Some(&e.reviewer.clone()))
        },
    },
    // --- the comparison
    Scenario {
        id: "views-compare-chooser",
        identity: "views-compare",
        status: 200,
        make: |e| {
            forked(e);
            get("/handbook/-/views/compare", None)
        },
    },
    Scenario {
        id: "views-compare-same-view",
        identity: "views-compare",
        status: 200,
        make: |e| {
            forked(e);
            get("/handbook/-/views/compare?a=alt&b=alt", None)
        },
    },
    Scenario {
        id: "views-compare-same-change",
        identity: "views-compare",
        status: 200,
        make: |e| {
            e.w.fake.add_view("twin");
            get("/handbook/-/views/compare?a=main&b=twin", None)
        },
    },
    Scenario {
        id: "views-compare-differences",
        identity: "views-compare",
        status: 200,
        make: |e| {
            forked(e);
            get(COMPARE, None)
        },
    },
    Scenario {
        id: "views-compare-nothing-differs",
        identity: "views-compare",
        status: 200,
        make: |e| {
            alike(e);
            e.w.fake.add_view_at("alt", 2);
            e.w.fake
                .set("pages/Other.md", "---\ntitle: Other\n---\n\nOther.\n");
            e.w.fake.diff_says(Ok(report(Vec::new())));
            get(COMPARE, None)
        },
    },
    Scenario {
        id: "views-compare-pages-omitted",
        identity: "views-compare",
        status: 200,
        make: |e| {
            forked(e);
            e.w.fake.diff_says(Ok(report(
                (0..205)
                    .map(|n| entry(&format!("pages/P{n:04}.md"), "modified", ""))
                    .collect(),
            )));
            get(COMPARE, None)
        },
    },
    Scenario {
        id: "views-compare-claims-omitted",
        identity: "views-compare",
        status: 200,
        make: |e| {
            alike(e);
            e.w.fake.add_view_at("alt", 2);
            let many: Vec<_> = (1..=205_u8)
                .map(|n| claim(n, Standing::Contested, None, SAID))
                .collect();
            put(&e.w, &home_with(SAID), &many, &[]);
            e.w.fake.diff_says(Ok(report(Vec::new())));
            get(COMPARE, None)
        },
    },
    Scenario {
        id: "views-compare-no-such-view",
        identity: "wiki-no-such-view",
        status: 404,
        make: |_| get("/handbook/-/views/compare?a=main&b=nope", None),
    },
    Scenario {
        id: "views-compare-unavailable",
        identity: "wiki-unavailable",
        status: 503,
        make: |e| {
            forked(e);
            e.w.fake.diff_says(Err(super::common::busy()));
            get(COMPARE, None)
        },
    },
    Scenario {
        id: "views-compare-page",
        identity: "views-compare-page-home",
        status: 200,
        make: |e| {
            forked(e);
            get("/handbook/-/views/compare/Home?a=main&b=alt", None)
        },
    },
    Scenario {
        id: "views-compare-page-only-here",
        identity: "views-compare-page-extra",
        status: 200,
        make: |e| {
            forked(e);
            get("/handbook/-/views/compare/Extra?a=main&b=alt", None)
        },
    },
    Scenario {
        id: "views-compare-page-absent",
        identity: "views-compare-page-absent",
        status: 404,
        make: |e| {
            forked(e);
            get("/handbook/-/views/compare/Nowhere?a=main&b=alt", None)
        },
    },
    // --- a claim's page: other views
    Scenario {
        id: "claim-others-differ",
        identity: "claim-page",
        status: 200,
        make: |e| {
            forked(e);
            get(CLAIM, None)
        },
    },
    Scenario {
        id: "claim-others-alike",
        identity: "claim-page",
        status: 200,
        make: |e| {
            alike(e);
            e.w.fake.add_view("twin");
            get(CLAIM, None)
        },
    },
    Scenario {
        id: "claim-others-past-the-bound",
        identity: "claim-page",
        status: 200,
        make: |e| {
            alike(e);
            for n in 0..12 {
                e.w.fake.add_view(&format!("v{n:02}"));
            }
            get(CLAIM, None)
        },
    },
    Scenario {
        id: "claim-others-unreadable",
        identity: "claim-page",
        status: 200,
        make: |e| {
            alike(e);
            // A view whose tip the repository cannot give.
            e.w.fake.add_view_at("ghost", 99);
            get(CLAIM, None)
        },
    },
];

/// The outcomes of a creation, each a page of its own, with the identity of the page whose title they share.
const OUTCOMES: &[(&str, &str, &str)] = &[
    (
        "views-outcome-not-allowed",
        "not-allowed",
        "propose-not-allowed",
    ),
    ("views-outcome-busy", "busy", "accept-busy"),
    ("views-outcome-recovery", "recovery", "accept-recovery"),
    (
        "views-outcome-too-many",
        "too-many",
        "views-outcome-too-many",
    ),
    (
        "views-outcome-retirement",
        "retirement",
        "views-outcome-retirement",
    ),
    ("views-outcome-failed", "failed", "views-outcome-failed"),
    (
        "views-outcome-unavailable",
        "unavailable",
        "accept-unavailable",
    ),
    ("views-outcome-uncertain", "uncertain", "accept-uncertain"),
    ("views-outcome-not-kept", "not-kept", "views-new-not-kept"),
    (
        "views-outcome-unconfigured",
        "unconfigured",
        "views-new-unconfigured",
    ),
];

fn outcome(name: &str) -> ViewOutcome {
    match name {
        "not-allowed" => ViewOutcome::NotAllowed,
        "busy" => ViewOutcome::Busy,
        "recovery" => ViewOutcome::NeedsRecovery,
        "too-many" => ViewOutcome::TooMany { bound: 100 },
        "retirement" => ViewOutcome::RetirementUnfinished,
        "failed" => ViewOutcome::Failed,
        "unavailable" => ViewOutcome::Unavailable,
        "uncertain" => ViewOutcome::Uncertain,
        "not-kept" => ViewOutcome::NotKept,
        _ => ViewOutcome::SigningNotConfigured,
    }
}

/// How many states there are.
pub(super) const fn count() -> usize {
    SCENARIOS.len() + OUTCOMES.len()
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
        let mut env = Env::new();
        let req = (sc.make)(&mut env);
        let router = site_with(&config, Some(env.w.services.clone())).unwrap();
        let mut b = Request::builder()
            .method(req.method)
            .uri(&req.uri)
            .header(header::HOST, "localhost:8080");
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        if let Some(a) = &req.account {
            b = b.header("x-test-account", a.as_str());
            if req.method == "POST" {
                b = b.header(inklkunn_web::CSRF_HEADER, super::common::TEST_CSRF);
            }
        }
        let body = if req.method == "POST" {
            b = b
                .header("content-type", "application/x-www-form-urlencoded")
                .header("sec-fetch-site", "same-origin");
            req.form
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        inklkunn_markup::percent_encode(k),
                        inklkunn_markup::percent_encode(v)
                    )
                })
                .collect::<Vec<_>>()
                .join("&")
        } else {
            String::new()
        };
        let res = router
            .oneshot(b.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = res.status().as_u16();
        let bytes = axum::body::to_bytes(res.into_body(), 16 << 20)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes).into_owned();
        assert_eq!(status, sc.status, "{}: {html}", sc.id);
        assert!(html.contains("<html"), "{}: not a page", sc.id);
        out.states.push(State {
            id: sc.id.to_owned(),
            identity: sc.identity,
            status,
            html,
        });
    }
    // The outcomes of a creation, drawn by the function the creation will call.
    let env = Env::new();
    let lang = if extra.iter().any(|(_, v)| v.contains("inklkunn_lang=ja")) {
        Lang::from_tag("ja").unwrap()
    } else {
        Lang::from_tag("en").unwrap()
    };
    let page = Page::new(T::new(lang), "/");
    for (id, name, identity) in OUTCOMES {
        let res = render_view_outcome(&page, &env.w.wiki, &outcome(name));
        let status = res.status().as_u16();
        let bytes = axum::body::to_bytes(res.into_body(), 16 << 20)
            .await
            .unwrap();
        out.states.push(State {
            id: (*id).to_owned(),
            identity,
            status,
            html: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
}
