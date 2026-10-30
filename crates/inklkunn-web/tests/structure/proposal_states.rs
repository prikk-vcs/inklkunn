//! Every state of the proposal pages (handoff 004 T3), reached through the real router over an in-memory
//! repository and the real proposal service: the list (empty, full), the form (a page, a section, a new page,
//! with problems, previewed, with its changes, resumed as a draft, revising), the chooser, the refusals (not
//! signed in, not allowed, a quota), the proposal's page in each of the seven states (and with comments, an
//! orphaned comment, an older decision), the reason forms, Accept, the discussion, and the page that says how an
//! acceptance ended, for each outcome of RFC 004 D-12. Each is a page the structural rules run on, in both
//! languages.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, header};
use inklkunn_core::proposal::{ProposalEdit, ProposalService};
use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_store::{AccountId, Anchor, DecisionKind, FileChange, ProposalRecord, Quote, Role};
use inklkunn_web::WebConfig;
use inklkunn_web::catalog::{Lang, T};
use inklkunn_web::pages::Page;
use inklkunn_web::proposals::{AcceptOutcome, render_accept_outcome};
use inklkunn_web::routes::site_with;
use tower::ServiceExt as _;

use super::common::{NOW, World, add_person, block_id, page, person, state_id, with_accounts};
use super::{Out, State};

const HOME: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n";

/// The wiki and its people.
struct Env {
    w: World,
    contributor: AccountId,
    other: AccountId,
    reviewer: AccountId,
    maintainer: AccountId,
    observer: AccountId,
}

impl Env {
    fn new() -> Env {
        let w = World::new(
            "handbook",
            "The Handbook",
            &[
                ("pages/Home.md", HOME),
                (
                    "pages/Guide.md",
                    &page(
                        &["title: Guide"],
                        "# Setup\n\nInstall it.\n\n## Details\n\nMore.\n\n# Use\n\nUse it.\n",
                    ),
                ),
            ],
        );
        let env = Env {
            w: with_accounts(w),
            contributor: person(1),
            other: person(2),
            reviewer: person(3),
            maintainer: person(4),
            observer: person(6),
        };
        add_person(
            &env.w,
            &env.contributor,
            "Aiko Tanaka",
            Some(Role::Contributor),
        );
        add_person(&env.w, &env.other, "Ben Okoye", Some(Role::Contributor));
        add_person(&env.w, &env.reviewer, "Rin Sato", Some(Role::Reviewer));
        add_person(&env.w, &env.maintainer, "Mika Ito", Some(Role::Maintainer));
        add_person(&env.w, &env.observer, "Olu Reader", Some(Role::Observer));
        env
    }

    fn who(id: &AccountId) -> Principal {
        Principal::session(&IdentityAuthority::for_tests(), id.clone())
    }

    fn service(&self) -> Arc<ProposalService> {
        self.w.services.proposals.clone()
    }

    fn edit(summary: &str, second: &str) -> ProposalEdit {
        ProposalEdit {
            summary: summary.to_owned(),
            sources: vec!["A book, page 12".to_owned()],
            files: vec![FileChange {
                path: "pages/Home.md".to_owned(),
                text: Some(HOME.replace("Second paragraph.", second)),
            }],
            ..ProposalEdit::default()
        }
    }

    /// A draft of `who`.
    fn draft(&self, who: &AccountId, summary: &str) -> ProposalRecord {
        self.service()
            .create(
                &Self::who(who),
                &self.w.wiki,
                None,
                Self::edit(summary, "Second, drafted."),
            )
            .unwrap()
    }

    /// A proposal of `who`, held for review.
    fn held(&self, who: &AccountId, summary: &str) -> ProposalRecord {
        let d = self.draft(who, summary);
        self.service()
            .submit(&Self::who(who), &self.w.wiki, &d.id)
            .unwrap()
    }

    fn address(&self, p: &ProposalRecord, rest: &str) -> String {
        format!("/handbook/-/proposals/{}{rest}", p.id)
    }
}

/// One request.
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

fn post(uri: &str, account: Option<&AccountId>, form: &[(&str, &str)]) -> Req {
    Req {
        method: "POST",
        uri: uri.to_owned(),
        account: account.cloned(),
        form: form
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

/// A form of the page `Home` as the form posts it.
fn home_form<'a>(text: &'a str, summary: &'a str, op: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("op", op),
        ("pages", "1"),
        ("renames", "0"),
        ("part", "whole"),
        ("base_state", Box::leak(state_id(1).into_boxed_str())),
        ("base_block", Box::leak(block_id(1).into_boxed_str())),
        ("p0_path", "pages/Home.md"),
        ("p0_title", "Home"),
        ("p0_text", text),
        ("summary", summary),
        ("sources", "A book, page 12"),
    ]
}

/// One state: its identity (the page it is), the status it answers, and how to reach it.
pub(super) struct Scenario {
    id: &'static str,
    identity: &'static str,
    status: u16,
    make: fn(&Env) -> Req,
}

pub(super) const SCENARIOS: &[Scenario] = &[
    // --- the list
    Scenario {
        id: "proposals-empty",
        identity: "proposals-list",
        status: 200,
        make: |_| get("/handbook/-/proposals", None),
    },
    Scenario {
        id: "proposals-full",
        identity: "proposals-list",
        status: 200,
        make: |e| {
            e.draft(&e.contributor, "A draft of mine");
            e.held(&e.contributor, "Waiting for review");
            let done = e.held(&e.other, "Already declined");
            e.service()
                .decline(
                    &Env::who(&e.maintainer),
                    &e.w.wiki,
                    &done.id,
                    "Thank you; not now.",
                )
                .unwrap();
            get("/handbook/-/proposals", Some(&e.contributor))
        },
    },
    // --- who is let in
    Scenario {
        id: "propose-needs-account",
        identity: "propose-needs-account",
        status: 403,
        make: |_| get("/handbook/-/proposals/new?page=Home", None),
    },
    Scenario {
        id: "propose-not-allowed",
        identity: "propose-not-allowed",
        status: 403,
        make: |e| get("/handbook/-/proposals/new?page=Home", Some(&e.observer)),
    },
    // --- the form
    Scenario {
        id: "propose-page",
        identity: "propose-home",
        status: 200,
        make: |e| get("/handbook/-/proposals/new?page=Home", Some(&e.contributor)),
    },
    Scenario {
        id: "propose-section",
        identity: "propose-guide",
        status: 200,
        make: |e| {
            get(
                "/handbook/-/proposals/new?page=Guide&part=section&ordinal=1",
                Some(&e.contributor),
            )
        },
    },
    Scenario {
        id: "propose-new-page",
        identity: "propose-new",
        status: 200,
        make: |e| {
            get(
                "/handbook/-/proposals/new?new=1&title=Setup",
                Some(&e.contributor),
            )
        },
    },
    Scenario {
        id: "propose-chooser",
        identity: "propose-parts",
        status: 200,
        make: |e| {
            get(
                "/handbook/-/proposals/new?page=Guide&parts=1",
                Some(&e.contributor),
            )
        },
    },
    Scenario {
        id: "propose-problems",
        identity: "propose-home",
        status: 422,
        make: |e| {
            let text = HOME.replace("Second paragraph.", "Second, <b>changed</b>.");
            let form = Box::leak(Box::new(home_form(&text, "", "submit")));
            post("/handbook/-/proposals/new", Some(&e.contributor), form)
        },
    },
    Scenario {
        id: "propose-preview",
        identity: "propose-home",
        status: 200,
        make: |e| {
            let text = HOME.replace(
                "Second paragraph.",
                "Second, **changed**.\n\n[[Guide]] and [[Nowhere]].",
            );
            let form = Box::leak(Box::new(home_form(&text, "Change", "preview")));
            post("/handbook/-/proposals/new", Some(&e.contributor), form)
        },
    },
    Scenario {
        id: "propose-changes",
        identity: "propose-home",
        status: 200,
        make: |e| {
            let text = HOME
                .replace("Second paragraph.", "Second, changed.")
                .replace("Third paragraph.", "");
            let form = Box::leak(Box::new(home_form(&text, "Change", "changes")));
            post("/handbook/-/proposals/new", Some(&e.contributor), form)
        },
    },
    Scenario {
        id: "propose-nothing-changed",
        identity: "propose-home",
        status: 200,
        make: |e| {
            let form = Box::leak(Box::new(home_form(HOME, "Change", "changes")));
            post("/handbook/-/proposals/new", Some(&e.contributor), form)
        },
    },
    Scenario {
        id: "propose-quota",
        identity: "quota-refused",
        status: 429,
        make: |e| {
            for _ in 0..20 {
                e.draft(&e.contributor, "Another");
            }
            let text = HOME.replace("Second paragraph.", "Second, again.");
            let form = Box::leak(Box::new(home_form(&text, "One more", "draft")));
            post("/handbook/-/proposals/new", Some(&e.contributor), form)
        },
    },
    Scenario {
        id: "draft-resumed",
        identity: "propose-revise",
        status: 200,
        make: |e| {
            let d = e.draft(&e.contributor, "My draft");
            get(&e.address(&d, "/edit?saved=1"), Some(&e.contributor))
        },
    },
    Scenario {
        id: "revise-held",
        identity: "propose-revise",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "To be revised");
            get(&e.address(&p, "/edit"), Some(&e.contributor))
        },
    },
    // --- the proposal's page in each state
    Scenario {
        id: "proposal-draft",
        identity: "proposal-draft",
        status: 200,
        make: |e| {
            let d = e.draft(&e.contributor, "A private draft");
            get(&e.address(&d, ""), Some(&e.contributor))
        },
    },
    Scenario {
        id: "proposal-held-reviewer",
        identity: "proposal-held",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Held with a conversation");
            let s = e.service();
            let anchor = quote_anchor("Second, drafted.", 1);
            let c = s
                .comment(
                    &Env::who(&e.reviewer),
                    &e.w.wiki,
                    &p.id,
                    inklkunn_core::proposal::NewComment {
                        body: "Is this the better word?".to_owned(),
                        parent: None,
                        anchor: Some(anchor),
                    },
                )
                .unwrap();
            s.comment(
                &Env::who(&e.contributor),
                &e.w.wiki,
                &p.id,
                inklkunn_core::proposal::NewComment {
                    body: "Yes, it is the usual one.".to_owned(),
                    parent: Some(c.id),
                    anchor: None,
                },
            )
            .unwrap();
            s.comment(
                &Env::who(&e.other),
                &e.w.wiki,
                &p.id,
                inklkunn_core::proposal::NewComment {
                    body: "A general remark.".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap();
            s.decide(
                &Env::who(&e.reviewer),
                &e.w.wiki,
                &p.id,
                1,
                DecisionKind::Approve,
                "Fine.",
            )
            .unwrap();
            get(&e.address(&p, "?on=0-1#comment"), Some(&e.reviewer))
        },
    },
    Scenario {
        id: "proposal-held-maintainer",
        identity: "proposal-held-m",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Waiting for a maintainer");
            get(&e.address(&p, ""), Some(&e.maintainer))
        },
    },
    Scenario {
        id: "proposal-held-author",
        identity: "proposal-held-a",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Seen by its author");
            get(&e.address(&p, ""), Some(&e.contributor))
        },
    },
    Scenario {
        id: "proposal-held-anonymous",
        identity: "proposal-held-n",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Read by anyone");
            get(&e.address(&p, ""), None)
        },
    },
    Scenario {
        id: "proposal-older-decision-and-orphan",
        identity: "proposal-revised",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Revised after review");
            let s = e.service();
            s.comment(
                &Env::who(&e.reviewer),
                &e.w.wiki,
                &p.id,
                inklkunn_core::proposal::NewComment {
                    body: "About the old wording.".to_owned(),
                    parent: None,
                    anchor: Some(quote_anchor("Wording that is gone", 1)),
                },
            )
            .unwrap();
            s.decide(
                &Env::who(&e.reviewer),
                &e.w.wiki,
                &p.id,
                1,
                DecisionKind::RequestChanges,
                "",
            )
            .unwrap();
            s.revise(
                &Env::who(&e.contributor),
                &e.w.wiki,
                &p.id,
                Env::edit("Revised after review", "Second, revised."),
            )
            .unwrap();
            get(&e.address(&p, ""), Some(&e.maintainer))
        },
    },
    Scenario {
        id: "proposal-needs-revision",
        identity: "proposal-needs-revision",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Sent back");
            e.service()
                .ask_for_revision(
                    &Env::who(&e.maintainer),
                    &e.w.wiki,
                    &p.id,
                    "Please keep the old heading.",
                )
                .unwrap();
            get(&e.address(&p, ""), Some(&e.contributor))
        },
    },
    Scenario {
        id: "proposal-accepted",
        identity: "proposal-accepted",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Taken in");
            super::common::accept_by_hand(&e.w, &p.id, &e.maintainer, &e.contributor);
            get(&e.address(&p, ""), None)
        },
    },
    Scenario {
        id: "proposal-declined",
        identity: "proposal-declined",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Not taken");
            e.service()
                .decline(
                    &Env::who(&e.maintainer),
                    &e.w.wiki,
                    &p.id,
                    "Thank you. The wording is used elsewhere.",
                )
                .unwrap();
            get(&e.address(&p, ""), None)
        },
    },
    Scenario {
        id: "proposal-withdrawn",
        identity: "proposal-withdrawn",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Taken back");
            e.service()
                .withdraw(
                    &Env::who(&e.contributor),
                    &e.w.wiki,
                    &p.id,
                    "I found a better way.",
                )
                .unwrap();
            get(&e.address(&p, ""), None)
        },
    },
    Scenario {
        id: "proposal-superseded",
        identity: "proposal-superseded",
        status: 200,
        make: |e| {
            let old = e.held(&e.contributor, "The first try");
            let new = e.held(&e.contributor, "The second try");
            e.service()
                .supersede(&Env::who(&e.contributor), &e.w.wiki, &old.id, &new.id)
                .unwrap();
            get(&e.address(&old, ""), None)
        },
    },
    Scenario {
        id: "proposal-not-found",
        identity: "not-found",
        status: 404,
        make: |e| {
            get(
                "/handbook/-/proposals/019a0000-0000-7000-8000-00000000dddd",
                Some(&e.contributor),
            )
        },
    },
    // --- the thin forms
    Scenario {
        id: "decline-form",
        identity: "decline-form",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "To decline");
            get(&e.address(&p, "/decline"), Some(&e.maintainer))
        },
    },
    Scenario {
        id: "decline-without-reason",
        identity: "decline-form",
        status: 422,
        make: |e| {
            let p = e.held(&e.contributor, "To decline");
            post(
                &e.address(&p, "/decline"),
                Some(&e.maintainer),
                &[("reason", "")],
            )
        },
    },
    Scenario {
        id: "withdraw-form",
        identity: "withdraw-form",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "To withdraw");
            get(&e.address(&p, "/withdraw"), Some(&e.contributor))
        },
    },
    Scenario {
        id: "ask-form",
        identity: "ask-form",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "To send back");
            get(&e.address(&p, "/ask"), Some(&e.maintainer))
        },
    },
    Scenario {
        id: "refused-not-in-this-state",
        identity: "refused-not-now",
        status: 409,
        make: |e| {
            let p = e.held(&e.contributor, "Already declined");
            e.service()
                .decline(&Env::who(&e.maintainer), &e.w.wiki, &p.id, "No.")
                .unwrap();
            get(&e.address(&p, "/decline"), Some(&e.maintainer))
        },
    },
    Scenario {
        id: "refused-not-allowed",
        identity: "propose-not-allowed",
        status: 403,
        make: |e| {
            let p = e.held(&e.contributor, "Not for you");
            get(&e.address(&p, "/decline"), Some(&e.contributor))
        },
    },
    Scenario {
        id: "accept-not-yet",
        identity: "accept-not-yet",
        status: 501,
        make: |e| {
            let p = e.held(&e.contributor, "To accept");
            post(&e.address(&p, "/accept"), Some(&e.maintainer), &[])
        },
    },
    // --- a page's discussion
    Scenario {
        id: "discussion",
        identity: "discussion-home",
        status: 200,
        make: |e| {
            let p = e.held(&e.contributor, "Discussed");
            e.service()
                .comment(
                    &Env::who(&e.reviewer),
                    &e.w.wiki,
                    &p.id,
                    inklkunn_core::proposal::NewComment {
                        body: "Looks fine to me.".to_owned(),
                        ..Default::default()
                    },
                )
                .unwrap();
            get("/handbook/-/discussion/Home", None)
        },
    },
    Scenario {
        id: "discussion-empty",
        identity: "discussion-guide",
        status: 200,
        make: |_| get("/handbook/-/discussion/Guide", None),
    },
];

/// The anchor of a comment about the part that reads `exact`, which is the `ordinal`-th of its page.
fn quote_anchor(exact: &str, ordinal: u32) -> Anchor {
    Anchor {
        page: "pages/Home.md".to_owned(),
        block_id: None,
        quote: Some(Quote {
            exact: exact.to_owned(),
            prefix: String::new(),
            suffix: String::new(),
        }),
        ordinal,
    }
}

/// How many states there are.
pub(super) const fn count() -> usize {
    SCENARIOS.len() + OUTCOMES.len()
}

/// The outcomes of an acceptance, each a page of its own.
const OUTCOMES: &[(&str, &str)] = &[
    ("accept-signed", "signed"),
    ("accept-overlap", "overlap"),
    ("accept-not-allowed", "not-allowed"),
    ("accept-busy", "busy"),
    ("accept-recovery", "recovery"),
    ("accept-refused", "refused"),
];

fn outcome(name: &str) -> AcceptOutcome {
    use inklkunn_core::proposal::{QuotaLimit, QuotaRefusal, Reset};
    match name {
        "signed" => AcceptOutcome::Signed,
        "overlap" => AcceptOutcome::Overlap,
        "not-allowed" => AcceptOutcome::NotAllowed,
        "busy" => AcceptOutcome::Busy { waiting: 3 },
        "recovery" => AcceptOutcome::NeedsRecovery,
        _ => AcceptOutcome::Refused(QuotaRefusal {
            limit: QuotaLimit::AcceptedBytesPerDay,
            allowed: 8 * 1024 * 1024,
            current: 8 * 1024 * 1024 - 100,
            asked: 4096,
            reset: Reset::At(NOW + 3600),
        }),
    }
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
        let env = Env::new();
        let req = (sc.make)(&env);
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
    // The outcomes of an acceptance, drawn by the function the acceptance will call.
    let env = Env::new();
    let lang = if extra.iter().any(|(_, v)| v.contains("inklkunn_lang=ja")) {
        Lang::from_tag("ja").unwrap()
    } else {
        Lang::from_tag("en").unwrap()
    };
    let page = Page::new(T::new(lang), "/");
    let p = env.held(&env.contributor, "Any");
    for (id, name) in OUTCOMES {
        let res = render_accept_outcome(&page, &env.w.wiki, &p.id, &outcome(name));
        let status = res.status().as_u16();
        let bytes = axum::body::to_bytes(res.into_body(), 16 << 20)
            .await
            .unwrap();
        out.states.push(State {
            id: (*id).to_owned(),
            identity: match *name {
                "not-allowed" => "propose-not-allowed",
                "refused" => "quota-refused",
                _ => id,
            },
            status,
            html: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
}
