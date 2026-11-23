#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::{Action, Guard, RouteTable};

use super::super::describe::{Answer, Doc};
use super::*;

/// Look into a JSON value by pointer; absent is `null` (indexing is denied by the workspace's lints).
trait At {
    fn at(&self, pointer: &str) -> &Value;
}

static NULL: Value = Value::Null;

impl At for Value {
    fn at(&self, pointer: &str) -> &Value {
        self.pointer(pointer).unwrap_or(&NULL)
    }
}

fn docs() -> Vec<&'static Doc> {
    endpoints::all().iter().filter_map(|e| e.doc).collect()
}

fn real_table() -> RouteTable {
    crate::routes::application_table().unwrap()
}

#[test]
fn the_document_is_built_from_the_live_registry_and_names_every_api_route() {
    let table = real_table();
    let document = document(&table, &docs()).unwrap();
    let paths = document.at("/paths").as_object().unwrap();
    let from_registry: Vec<String> = table
        .entries()
        .iter()
        .filter(|e| is_api_route(e.path) && !endpoints::NOT_ENDPOINTS.contains(&e.path))
        .map(|e| openapi_path(e.path))
        .collect();
    assert!(from_registry.len() >= 12, "{from_registry:?}");
    for path in &from_registry {
        assert!(
            paths.contains_key(path),
            "{path} is served and not described"
        );
    }
    for path in paths.keys() {
        assert!(
            from_registry.contains(path),
            "{path} is described and not served"
        );
    }
}

#[test]
fn a_route_without_a_description_is_drift() {
    let mut table = real_table();
    table
        .register(
            "/{slug}/-/api/v1/planted",
            &["GET"],
            Guard::Action(Action::Read),
        )
        .unwrap();
    assert_eq!(
        document(&table, &docs()).unwrap_err(),
        Drift::Undocumented("/{slug}/-/api/v1/planted".to_owned())
    );
}

#[test]
fn a_description_without_a_route_is_drift() {
    static PHANTOM: Doc = Doc {
        path: "/{slug}/-/api/v1/phantom",
        operation_id: "phantom",
        tag: "pages",
        summary: "A description that no route serves",
        description: "x",
        query: &[],
        answer: Answer::Fragment,
        write: None,
        problems: &[],
    };
    let mut all = docs();
    all.push(&PHANTOM);
    assert_eq!(
        document(&real_table(), &all).unwrap_err(),
        Drift::NoRoute("/{slug}/-/api/v1/phantom".to_owned())
    );
}

#[test]
fn removing_a_description_is_drift_too() {
    let mut all = docs();
    let removed = all.remove(3);
    assert_eq!(
        document(&real_table(), &all).unwrap_err(),
        Drift::Undocumented(removed.path.to_owned())
    );
}

#[test]
fn an_api_route_guarded_by_another_action_is_refused() {
    let mut table = RouteTable::new();
    for entry in real_table().entries() {
        let guard = if entry.path == "/{slug}/-/api/v1/pages" {
            Guard::Action(Action::Propose)
        } else {
            entry.guard
        };
        table.register(entry.path, entry.methods, guard).unwrap();
    }
    assert_eq!(
        document(&table, &docs()).unwrap_err(),
        Drift::Unguarded("/{slug}/-/api/v1/pages".to_owned())
    );
}

#[test]
fn a_write_guarded_by_another_action_than_its_description_names_is_refused() {
    // A write that is registered as a read, or as another act of the proposal's life, is drift: what the
    // document says a token needs and what the router checks must be one thing.
    for wrong in [Action::Read, Action::Review, Action::Comment, Action::Admin] {
        let mut table = RouteTable::new();
        for entry in real_table().entries() {
            let guard = if entry.path == "/{slug}/-/api/v1/proposals" {
                Guard::Action(wrong)
            } else {
                entry.guard
            };
            table.register(entry.path, entry.methods, guard).unwrap();
        }
        assert_eq!(
            document(&table, &docs()).unwrap_err(),
            Drift::Unguarded("/{slug}/-/api/v1/proposals".to_owned()),
            "{wrong:?}"
        );
    }
}

#[test]
fn a_write_with_no_description_is_drift() {
    let mut table = real_table();
    table
        .register(
            "/{slug}/-/api/v1/planted-write",
            &["POST"],
            Guard::Action(Action::Propose),
        )
        .unwrap();
    assert_eq!(
        document(&table, &docs()).unwrap_err(),
        Drift::Undocumented("/{slug}/-/api/v1/planted-write".to_owned())
    );
}

#[test]
fn every_operation_carries_the_guard_the_registry_declares() {
    let table = real_table();
    let document = document(&table, &docs()).unwrap();
    for (path, item) in document.at("/paths").as_object().unwrap() {
        // One method per path: a read is a `get` guarded by `read`, a write a `post` guarded by its action.
        let methods: Vec<&String> = item.as_object().unwrap().keys().collect();
        let [method] = methods.as_slice() else {
            panic!("{path}: one method a path, not {methods:?}");
        };
        let op = &item.at(&format!("/{method}"));
        let action = op.get("x-inklkunn-action").and_then(Value::as_str);
        let public = op.get("x-inklkunn-public").and_then(Value::as_str);
        match (method.as_str(), action, public) {
            ("get", Some("read"), None) => {}
            ("get", None, Some(reason)) => {
                assert_eq!(path, super::super::OPENAPI_PATH);
                assert!(reason.len() >= 16);
            }
            ("post", Some("propose" | "comment" | "review" | "accept"), None) => {
                assert!(
                    op.get("security").is_some(),
                    "{path}: a write names its security"
                );
                assert!(
                    op.get("requestBody").is_some(),
                    "{path}: a write has a body"
                );
            }
            other => panic!("{path}: {other:?}"),
        }
    }
}

/// Corrections G9: a client that authenticates a write with the session cookie must send the session's CSRF
/// token, and the document says so: the security scheme, the header, and every write's alternatives. The names
/// are the server's own constants, so a rename on either side is drift.
#[test]
fn a_write_describes_the_csrf_token_a_cookie_client_must_send() {
    let table = real_table();
    let document = document(&table, &docs()).unwrap();
    assert!(CSRF_HEADER_NAME.eq_ignore_ascii_case(crate::csrf::HEADER));
    assert_eq!(
        document.at(&format!("/components/securitySchemes/{CSRF_SCHEME}/name")),
        CSRF_HEADER_NAME
    );
    assert_eq!(
        document.at(&format!("/components/securitySchemes/{CSRF_SCHEME}/in")),
        "header"
    );
    assert_eq!(
        document.at(&format!(
            "/components/securitySchemes/{SESSION_SCHEME}/name"
        )),
        inklkunn_auth::session::SESSION_COOKIE
    );
    assert_eq!(
        document.at(&format!("/components/securitySchemes/{SESSION_SCHEME}/in")),
        "cookie"
    );
    let mut writes = 0;
    for (path, item) in document.at("/paths").as_object().unwrap() {
        let Some(post) = item.get("post") else {
            // A read has no CSRF token to send and no alternative to a bearer token's.
            let get = item.at("/get");
            let names: Vec<&str> = get
                .at("/parameters")
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p.get("name").and_then(Value::as_str))
                .collect();
            assert!(!names.contains(&CSRF_HEADER_NAME), "{path}");
            continue;
        };
        writes += 1;
        let names: Vec<&str> = post
            .at("/parameters")
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&CSRF_HEADER_NAME), "{path}: {names:?}");
        // H6: where a cookie client finds the token is said as it is: the hidden field of the forms it was
        // served; no page "shows" it as text.
        let described = post
            .at("/parameters")
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p.get("name").and_then(Value::as_str) == Some(CSRF_HEADER_NAME))
            .and_then(|p| p.get("description").and_then(Value::as_str))
            .unwrap();
        assert!(
            described.contains("hidden field `csrf`") && !described.contains("shows it"),
            "{path}: {described}"
        );
        assert_eq!(
            post.at("/security"),
            &json!([
                {BEARER_SCHEME: []},
                {SESSION_SCHEME: [], CSRF_SCHEME: []},
            ]),
            "{path}"
        );
    }
    assert!(writes >= 5, "{writes} writes");
}

#[test]
fn a_router_path_is_spelled_the_openapi_way() {
    assert_eq!(
        openapi_path("/{slug}/-/api/v1/pages/{*title}"),
        "/{slug}/-/api/v1/pages/{title}"
    );
    assert_eq!(
        path_params("/{slug}/-/api/v1/pages/{*title}"),
        ["slug", "title"]
    );
    assert_eq!(path_params("/-/api/v1/openapi.json"), Vec::<&str>::new());
}

#[test]
fn the_document_is_served_the_same_each_time_and_is_the_one_built_here() {
    let served = served().unwrap();
    let again = served;
    assert_eq!(served, again);
    let built = serde_json::to_vec(&document(&real_table(), &docs()).unwrap()).unwrap();
    assert_eq!(served, built.as_slice());
}

#[test]
fn the_page_size_bound_is_in_the_document() {
    let document = document(&real_table(), &docs()).unwrap();
    let limit = document
        .at("/paths/~1{slug}~1-~1api~1v1~1pages/get/parameters")
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p.at("/name") == "limit")
        .unwrap();
    assert_eq!(
        limit.at("/schema/maximum"),
        inklkunn_core::read::MAX_PAGE_SIZE
    );
    assert_eq!(limit.at("/schema/minimum"), 1);
    assert_eq!(
        document.at("/components/schemas/PageList/properties/items/maxItems"),
        inklkunn_core::read::MAX_PAGE_SIZE
    );
}

#[test]
fn the_problem_registry_is_in_the_document() {
    let document = document(&real_table(), &docs()).unwrap();
    let listed = document.at("/x-inklkunn-problems").as_array().unwrap();
    assert_eq!(listed.len(), Code::ALL.len());
    for (listed, code) in listed.iter().zip(Code::ALL) {
        assert_eq!(listed.at("/code"), code.slug());
        assert_eq!(listed.at("/type"), &code.type_uri().as_str());
        assert_eq!(listed.at("/status"), code.status().as_u16());
    }
}

#[test]
fn the_document_calls_the_review_setting_the_operators_and_says_claims_cite_a_source() {
    // K36: `knowledge.review_after` is one value for every wiki of the instance until RFC 006's policy as data, and a
    // source is cited, not relied on, by the claims the list names.
    let text = document(&real_table(), &docs()).unwrap().to_string();
    assert!(
        text.contains("the operator's `knowledge.review_after`"),
        "the setting is the operator's"
    );
    assert!(!text.contains("wiki's `knowledge.review_after`"));
    assert!(text.contains("claims that cite"));
    assert!(!text.contains("claims that rely"));
    assert!(!text.contains("claims rely"));
}

/// The description of the operation with this id.
fn described(document: &Value, id: &str) -> String {
    for item in document.at("/paths").as_object().unwrap().values() {
        let op = item.at("/post");
        if op.at("/operationId") == id {
            return op.at("/description").as_str().unwrap().to_owned();
        }
    }
    panic!("no operation {id}");
}

/// **A refusal's reason that is not described fails here** (handoff G-T1 correction C-7): the reasons are taken from what the
/// server emits (the problems themselves, built from the real refusals), each must be said by the operation that can refuse
/// with it, and each member of such a problem must be a member of the `Problem` schema.
#[test]
fn every_reason_the_rules_of_a_wiki_refuse_with_is_described_by_the_operation_and_its_members_by_the_problem()
 {
    use crate::api::write::refuse::{acceptance_problem, invalid_word};
    use inklkunn_core::AcceptFailure;
    use inklkunn_core::governance::{Fault, GovernanceFile, ReadFault, Refusal, Require, Target};
    use inklkunn_core::proposal::Invalid;
    use inklkunn_core::protection::{Lack, Notice};
    use inklkunn_store::Role;

    let document = document(&real_table(), &docs()).unwrap();
    let problem = |failure: AcceptFailure| {
        let problem = acceptance_problem(failure, &|_| {});
        let body = problem.body(crate::catalog::T::new(crate::catalog::EN));
        body.as_object().unwrap().clone()
    };
    let fault = Fault {
        line: Some(4),
        ..Fault::at(GovernanceFile::Policy, Refusal::UnknownKey, "rungs")
    };
    // With what stands at its place, so that the members `found` and `found_more` are tied to the schema too (E-1).
    let unreadable = ReadFault::from_stored(GovernanceFile::Policy, Some(2), Refusal::NotAFile)
        .with_found(vec!["governance/policy.yaml/a.txt".to_owned()], 3);
    let at_acceptance = [
        problem(AcceptFailure::GovernanceInvalid(vec![fault])),
        problem(AcceptFailure::GovernanceRung(Role::Admin)),
        problem(AcceptFailure::ApprovalsMissing { needed: 2, have: 1 }),
        problem(AcceptFailure::Protected {
            notices: vec![Notice::temporary(
                Target::Path("Home".to_owned()),
                Require::Rung(Role::Admin),
                "Why.",
                1_700_086_400,
            )],
            lack: Lack::Rung(Role::Admin),
        }),
        problem(AcceptFailure::RulesUnreadable(unreadable)),
    ];
    // What the acceptance refuses with, from the problems, and what a person proposing is refused with, from the words.
    let accepting: Vec<String> = at_acceptance
        .iter()
        .map(|body| body["reason"].as_str().unwrap().to_owned())
        .chain([invalid_word(Invalid::GovernanceMixed).to_owned()])
        .collect();
    assert_eq!(
        accepting,
        [
            "governance-invalid",
            "governance-rung",
            "approvals-missing",
            "protected",
            "rules-unreadable",
            "governance-mixed"
        ]
    );
    let accept = described(&document, "acceptProposal");
    for reason in &accepting {
        assert!(
            accept.contains(&format!("`{reason}`")),
            "acceptProposal does not describe `{reason}`"
        );
    }
    let proposing = [
        invalid_word(Invalid::GovernancePath),
        invalid_word(Invalid::GovernanceMixed),
    ];
    for id in [
        "createProposal",
        "saveDraft",
        "submitDraft",
        "reviseProposal",
    ] {
        let text = described(&document, id);
        for reason in proposing {
            assert!(
                text.contains(&format!("`{reason}`")),
                "{id} does not describe `{reason}`"
            );
        }
    }
    // The writes that the unreadable rules refuse say so, and so does the introduction of the document.
    assert!(described(&document, "declineProposal").contains("`rules-unreadable`"));
    assert!(
        document
            .at("/info/description")
            .as_str()
            .unwrap()
            .contains("`rules-unreadable`")
    );
    // Every member of such a problem is a member the schema names.
    let members = document.at("/components/schemas/Problem/properties");
    for body in &at_acceptance {
        for member in body.keys() {
            assert!(
                members.get(member).is_some(),
                "the problem member `{member}` is not described by the Problem schema"
            );
        }
    }
    // The entries of `refusals` are described as they are emitted.
    let entry = at_acceptance
        .first()
        .and_then(|body| body.get("refusals"))
        .and_then(|refusals| refusals.get(0))
        .and_then(Value::as_object)
        .unwrap();
    let listed = document.at("/components/schemas/Problem/properties/refusals/items/properties");
    for member in entry.keys() {
        assert!(listed.get(member).is_some(), "refusals[].{member}");
    }
}

#[test]
fn a_restricted_account_refusal_is_described_with_every_member_the_server_emits() {
    // RFC 006 D-7 (handoff G-T2): the problem a restricted account gets names its reason, its end and a sentence; the
    // document says so, and the Problem schema names every member the server emits for it.
    use inklkunn_core::RestrictionNotice;
    use inklkunn_store::RestrictionEventId;
    let document = document(&real_table(), &docs()).unwrap();
    let intro = document.at("/info/description").as_str().unwrap();
    assert!(
        intro.contains("`restricted`") && intro.contains("`until`"),
        "{intro}"
    );
    let reason = document
        .at("/components/schemas/Problem/properties/reason/description")
        .as_str()
        .unwrap();
    assert!(reason.contains("`restricted`"), "{reason}");
    let notice = RestrictionNotice {
        id: RestrictionEventId::new(),
        reason: "spam".to_owned(),
        until: 1_700_000_000,
    };
    let t = crate::api::request::language(&axum::http::HeaderMap::new());
    let body = crate::api::write::refuse::restricted_problem(t, &notice).body(t);
    let members = document.at("/components/schemas/Problem/properties");
    for member in body.as_object().unwrap().keys() {
        if ["type", "title", "status", "detail", "hint"].contains(&member.as_str()) {
            continue;
        }
        assert!(
            members.get(member).is_some(),
            "the problem member `{member}` of a restricted account is not described"
        );
    }
}
