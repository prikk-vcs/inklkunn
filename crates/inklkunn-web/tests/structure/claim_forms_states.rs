//! Every state of the pages that make claims and attach sources (handoff 005 K-T5; RFC 005 part 2 D-3): the part
//! menu with its two links on a paragraph, the form that attaches a source and the one that challenges, the form of
//! a paragraph that others rely on (its marker held), the confirmation of a removal, a source form with its problems,
//! the proposal's page that lists the claims and sources it records, the one that sets what only curation sets, and
//! the form that resumes a proposal with claims. Each is a page the structural rules run on, in both languages.

use axum::body::Body;
use axum::http::{Request, header};
use inklkunn_core::knowledge::compose::{Attachment, SourceDraft, attach};
use inklkunn_core::knowledge::{
    Bearing, Claim, ClaimId, Expression, Link, Source, SourceId, SourceKind, SourceLink,
    StatementKind, claim_path, source_path,
};
use inklkunn_core::proposal::{ProposalEdit, ProposalService};
use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_store::{AccountId, FileChange, ProposalRecord, Role};
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site_with;
use tower::ServiceExt as _;

use super::common::{World, add_person, block_id, person, state_id, with_accounts};
use super::{Out, State};

const BRIDGE: &str = "---\ntitle: Bridge\nid: bridge-page\n---\n\nIt opened in 1950. ^k1\n\nIt is long.\n\n# Notes\n\nA note.\n";

struct Env {
    w: World,
    writer: AccountId,
}

impl Env {
    fn new() -> Env {
        let claim_id = ClaimId::parse("01930000-0000-7000-8000-000000000001").unwrap();
        let source_id = SourceId::parse("01931111-0000-7000-8000-000000000001").unwrap();
        let mut claim = Claim::new(claim_id);
        claim.kind = StatementKind::Observation;
        claim.expressions = vec![Expression::new("bridge-page", "k1").unwrap()];
        let block = inklkunn_markup::blocks("It opened in 1950. ^k1");
        claim.sources = vec![SourceLink {
            source: source_id.clone(),
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            judged_on: inklkunn_core::knowledge::block_digest(block.first().unwrap()),
        }];
        let mut source = Source::new(source_id);
        source.identity.title = Some("A history of the bridge".to_owned());
        let (cp, sp) = (claim_path(&claim.id), source_path(&source.id));
        let w = with_accounts(World::new(
            "handbook",
            "The Handbook",
            &[
                ("pages/Bridge.md", BRIDGE),
                (cp.as_str(), &claim.to_yaml()),
                (sp.as_str(), &source.to_yaml()),
            ],
        ));
        let writer = person(1);
        add_person(&w, &writer, "Aiko Tanaka", Some(Role::Contributor));
        add_person(&w, &person(4), "Mika Ito", Some(Role::Maintainer));
        Env { w, writer }
    }

    fn who(id: &AccountId) -> Principal {
        Principal::session(&IdentityAuthority::for_tests(), id.clone())
    }

    fn service(&self) -> std::sync::Arc<ProposalService> {
        self.w.services.proposals.clone()
    }

    /// A proposal of the writer's that attaches a source to the second paragraph, held for review.
    fn attaching(&self) -> ProposalRecord {
        let start = BRIDGE.find("It is long.").unwrap();
        let attached = attach(
            BRIDGE,
            &(start..start + "It is long.".len()),
            None,
            &Attachment {
                source: SourceDraft {
                    kind: SourceKind::Secondary,
                    title: Some("A survey of bridges".to_owned()),
                    author: None,
                    date: None,
                    reference: None,
                },
                relation: Link::Challenges,
                bearing: Bearing::Partial,
                note: None,
                kind: StatementKind::Opinion,
            },
        )
        .unwrap();
        let [(cp, ct), (sp, st)] = attached.files;
        let edit = ProposalEdit {
            summary: "A source for the length".to_owned(),
            files: vec![
                FileChange {
                    path: "pages/Bridge.md".to_owned(),
                    text: Some(attached.page),
                },
                FileChange {
                    path: cp,
                    text: Some(ct),
                },
                FileChange {
                    path: sp,
                    text: Some(st),
                },
            ],
            ..ProposalEdit::default()
        };
        let who = Self::who(&self.writer);
        let draft = self
            .service()
            .create(&who, &self.w.wiki, None, edit)
            .unwrap();
        self.service()
            .submit(&who, &self.w.wiki, &draft.id)
            .unwrap()
    }

    /// A curation proposal of the writer's: a status, no page, held for review.
    fn curating(&self) -> ProposalRecord {
        let id = ClaimId::parse("01930000-0000-7000-8000-000000000001").unwrap();
        let mut claim = Claim::new(id.clone());
        claim.kind = StatementKind::Observation;
        claim.expressions = vec![Expression::new("bridge-page", "k1").unwrap()];
        claim.community_status.status = inklkunn_core::knowledge::Standing::Contested;
        let edit = ProposalEdit {
            summary: "Contested".to_owned(),
            files: vec![FileChange {
                path: claim_path(&id),
                text: Some(claim.to_yaml()),
            }],
            ..ProposalEdit::default()
        };
        let who = Self::who(&self.writer);
        let draft = self
            .service()
            .create(&who, &self.w.wiki, None, edit)
            .unwrap();
        self.service()
            .submit(&who, &self.w.wiki, &draft.id)
            .unwrap()
    }

    /// A proposal of the writer's that sets a community status beside a page edit, held for review.
    fn setting_a_status(&self) -> ProposalRecord {
        let claim_text = {
            let id = ClaimId::parse("01930000-0000-7000-8000-000000000001").unwrap();
            let mut claim = Claim::new(id);
            claim.expressions = vec![Expression::new("bridge-page", "k1").unwrap()];
            claim.community_status.status = inklkunn_core::knowledge::Standing::Accepted;
            claim.to_yaml()
        };
        let edit = ProposalEdit {
            summary: "A status".to_owned(),
            files: vec![
                FileChange {
                    path: claim_path(
                        &ClaimId::parse("01930000-0000-7000-8000-000000000001").unwrap(),
                    ),
                    text: Some(claim_text),
                },
                FileChange {
                    path: "pages/Bridge.md".to_owned(),
                    text: Some(BRIDGE.replace("A note.", "A longer note.")),
                },
            ],
            ..ProposalEdit::default()
        };
        let who = Self::who(&self.writer);
        let draft = self
            .service()
            .create(&who, &self.w.wiki, None, edit)
            .unwrap();
        self.service()
            .submit(&who, &self.w.wiki, &draft.id)
            .unwrap()
    }
}

struct Req {
    method: &'static str,
    uri: String,
    account: Option<AccountId>,
    form: Vec<(String, String)>,
}

fn get(uri: &str, account: &AccountId) -> Req {
    Req {
        method: "GET",
        uri: uri.to_owned(),
        account: Some(account.clone()),
        form: Vec::new(),
    }
}

fn post(uri: &str, account: &AccountId, form: Vec<(&str, String)>) -> Req {
    Req {
        method: "POST",
        uri: uri.to_owned(),
        account: Some(account.clone()),
        form: form.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    }
}

/// The form of the `ordinal`-th part of `Bridge` with `text`, and these more fields.
fn part_form(
    ordinal: usize,
    text: &str,
    more: &[(&'static str, &str)],
) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "block".to_owned()),
        ("ordinal", ordinal.to_string()),
        ("base_state", state_id(1)),
        ("base_block", block_id(1)),
        ("p0_path", "pages/Bridge.md".to_owned()),
        ("p0_title", "Bridge".to_owned()),
        ("p0_text", text.to_owned()),
        ("summary", "A source".to_owned()),
        ("sources", String::new()),
    ];
    for (k, v) in more {
        fields.push((*k, (*v).to_owned()));
    }
    fields
}

pub(super) struct Scenario {
    id: &'static str,
    identity: &'static str,
    status: u16,
    make: fn(&Env) -> Req,
}

const NEW: &str = "/handbook/-/proposals/new";
const CURATE: &str = "/handbook/-/proposals/curate";
const CLAIM: &str = "01930000-0000-7000-8000-000000000001";

/// What the curation form sends, with this button and this status.
fn curate_form(name: &'static str, button: &str, status: &str) -> Vec<(&'static str, String)> {
    vec![
        (name, button.to_owned()),
        ("base_state", state_id(1)),
        ("base_block", block_id(1)),
        ("claim", CLAIM.to_owned()),
        ("links", "1".to_owned()),
        ("status", status.to_owned()),
        (
            "c0_source",
            "01931111-0000-7000-8000-000000000001".to_owned(),
        ),
        ("c0_relation", "supports".to_owned()),
        ("c0_bearing", "direct".to_owned()),
        ("c0_reviewed", String::new()),
        ("c0_availability", "unknown".to_owned()),
        ("summary", "Curate".to_owned()),
    ]
}

pub(super) const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "parts-with-sources",
        identity: "propose-bridge-parts",
        status: 200,
        make: |e| get("/handbook/-/proposals/new?page=Bridge&parts=1", &e.writer),
    },
    Scenario {
        id: "attach-form",
        identity: "attach-bridge",
        status: 200,
        make: |e| {
            get(
                "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=1&attach=supports",
                &e.writer,
            )
        },
    },
    Scenario {
        id: "challenge-form",
        identity: "challenge-bridge",
        status: 200,
        make: |e| {
            get(
                "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=1&attach=challenges",
                &e.writer,
            )
        },
    },
    Scenario {
        id: "attach-form-for-a-claim",
        identity: "attach-bridge",
        status: 200,
        make: |e| {
            get(
                "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=0&attach=supports",
                &e.writer,
            )
        },
    },
    Scenario {
        id: "attach-problems",
        identity: "attach-bridge",
        status: 422,
        make: |e| {
            post(
                NEW,
                &e.writer,
                part_form(
                    1,
                    "It is long, really.",
                    &[
                        ("attach", "supports"),
                        ("a_kind", "other"),
                        ("a_ref_form", "url"),
                        ("a_ref", "javascript:alert(1)"),
                        ("a_bearing", "direct"),
                        ("a_claim_kind", "unknown"),
                    ],
                ),
            )
        },
    },
    Scenario {
        id: "propose-claim-paragraph",
        identity: "propose-bridge",
        status: 200,
        make: |e| {
            get(
                "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=0",
                &e.writer,
            )
        },
    },
    Scenario {
        id: "propose-claims-on-the-page",
        identity: "propose-bridge",
        status: 200,
        make: |e| get("/handbook/-/proposals/new?page=Bridge", &e.writer),
    },
    Scenario {
        id: "propose-removal-to-confirm",
        identity: "propose-bridge",
        status: 422,
        make: |e| post(NEW, &e.writer, part_form(0, "", &[])),
    },
    Scenario {
        id: "propose-removal-confirmed-again",
        identity: "propose-bridge",
        status: 422,
        // Confirmed, but no summary: the box stays ticked.
        make: |e| {
            let mut fields = part_form(0, "", &[("drop_k1", "1")]);
            fields.retain(|(k, _)| *k != "summary");
            fields.push(("summary", String::new()));
            post(NEW, &e.writer, fields)
        },
    },
    Scenario {
        id: "proposal-with-claims",
        identity: "proposal-with-claims",
        status: 200,
        make: |e| {
            let p = e.attaching();
            get(&format!("/handbook/-/proposals/{}", p.id), &e.writer)
        },
    },
    Scenario {
        id: "proposal-sets-a-status",
        identity: "proposal-sets-a-status",
        status: 200,
        make: |e| {
            let p = e.setting_a_status();
            get(&format!("/handbook/-/proposals/{}", p.id), &e.writer)
        },
    },
    Scenario {
        id: "curate-form",
        identity: "curate",
        status: 200,
        make: |e| get(&format!("{CURATE}?claim={CLAIM}"), &e.writer),
    },
    Scenario {
        id: "curate-problems",
        identity: "curate",
        status: 422,
        make: |e| post(CURATE, &e.writer, curate_form("op", "submit", "unknown")),
    },
    Scenario {
        id: "curate-changes",
        identity: "curate",
        status: 200,
        make: |e| post(CURATE, &e.writer, curate_form("op", "changes", "contested")),
    },
    Scenario {
        id: "proposal-curation",
        identity: "proposal-curation",
        status: 200,
        make: |e| {
            let p = e.curating();
            get(&format!("/handbook/-/proposals/{}", p.id), &e.writer)
        },
    },
    Scenario {
        id: "revise-with-claims",
        identity: "propose-revise",
        status: 200,
        make: |e| {
            let p = e.attaching();
            get(&format!("/handbook/-/proposals/{}/edit", p.id), &e.writer)
        },
    },
];

/// How many states these are.
pub(super) const fn count() -> usize {
    SCENARIOS.len()
}

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
            if req.method == "POST" {
                b = b.header(inklkunn_web::CSRF_HEADER, crate::common::TEST_CSRF);
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
}
