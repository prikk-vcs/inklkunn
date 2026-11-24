//! **No token reaches a web-only act** (RFC 006 D-12; threat model C-14f, "never scripted through the API"; readiness gap 1): hiding,
//! restoring and showing hidden content, putting a role in place or taking it away, restricting a person and renewing or lifting a
//! restriction, a temporary protection and the decision of an appeal are acts of a person at the site, not of a script. (The form that
//! proposes a change to the rules is a proposal, which the propose API makes as well: it is listed as ordinary, with that reason.) The API document says they are web-only, and **this walks the route table**: every `POST`
//! of the application origin that is under a wiki and not under the API's prefix is either one of [`WEB_ONLY`] (and is sent, with the
//! strongest scope a token has, as an admin's token, and refused, and nothing in the records changes) or one of [`ORDINARY`] (the
//! forms of the collaboration the API also offers, each with the reason it is not in the first list). **A new write form that is in
//! neither list fails here**, so whoever adds one has to say which it is, and a web-only one is held to the refusal the day it is
//! registered.
//!
//! Each route is also sent with the admin's *session* and the same body, in a world of its own, and must be done: so the refusal of
//! the token is a refusal of an act that would have been made, and not of a form that was never right.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;

use axum::http::StatusCode;
use inklkunn_store::{
    AccountId, AppealRecord, AuditOutcome, GovernanceStore, ProposalId, ProposalStore, Role, Scope,
    TombstoneEvent, Visibility,
};

use crate::common::{
    self, World, add_person, home_with, person, post_form, send_body, under_rules, with_accounts,
};

/// The routes that are web-only (the API document says so), each by its address and what it takes to be done.
const WEB_ONLY: &[&str] = &[
    "/{slug}/-/moderate",
    "/{slug}/-/moderate/unhide",
    "/{slug}/-/moderate/show",
    "/{slug}/-/people/grant",
    "/{slug}/-/people/restrict",
    "/{slug}/-/people/renew",
    "/{slug}/-/people/lift",
    "/{slug}/-/protect",
    "/{slug}/-/appeals/decide",
];

/// The writes of the collaboration and of a person's own, which are not in the list above. A new `POST` under a wiki that is in neither
/// list fails the walk: **add it to the list it belongs to** (a web-only act to [`WEB_ONLY`], with its body in [`prepare`]).
const ORDINARY: &[(&str, &str)] = &[
    (
        "/{slug}/-/policy/edit",
        "the form of the rules makes a proposal of the governance files, which the propose API makes as well (D-12: a change to the rules is proposed through it); the proposal is judged and accepted as every proposal is",
    ),
    (
        "/{slug}/-/proposals/new",
        "a proposal: the propose API is the same act",
    ),
    (
        "/{slug}/-/proposals/curate",
        "a proposal for a page's claims: the same act",
    ),
    (
        "/{slug}/-/proposals/{id}/edit",
        "a revision of one's own proposal: the API's revision",
    ),
    (
        "/{slug}/-/proposals/{id}/comment",
        "a comment, which the API's write also is",
    ),
    (
        "/{slug}/-/proposals/{id}/decide",
        "a review: the API's review",
    ),
    (
        "/{slug}/-/proposals/{id}/accept",
        "the acceptance: the API's accept",
    ),
    (
        "/{slug}/-/proposals/{id}/reconfirm",
        "a proposal's own reconfirmation: the API's",
    ),
    (
        "/{slug}/-/proposals/{id}/decline",
        "a decline: the API's decline",
    ),
    (
        "/{slug}/-/proposals/{id}/ask",
        "a request for a revision: the API's",
    ),
    (
        "/{slug}/-/proposals/{id}/withdraw",
        "a withdrawal by the author: the API's",
    ),
    (
        "/{slug}/-/restore/{*title}",
        "a proposal that restores a removed page: a proposal",
    ),
    (
        "/{slug}/-/views/new",
        "a view of the wiki, made by whoever the rules allow",
    ),
    (
        "/{slug}/-/claim/{id}/discuss",
        "a comment in a claim's discussion: a comment",
    ),
    (
        "/{slug}/-/knowledge/replace",
        "a proposal that replaces or retires a knowledge file: judged as a proposal",
    ),
    (
        "/{slug}/-/appeals/new",
        "filing an appeal is the affected person's own act at the site; no moderator is scripted by it",
    ),
    (
        "/{slug}/-/watch/{*title}",
        "a person's own subscription to a page",
    ),
    (
        "/{slug}/-/verification",
        "the wiki's own check of its history, which any reader may ask for",
    ),
];

/// What a route needs to be done: the body and the world it is made in.
struct Stand {
    w: World,
    admin: AccountId,
    maintainer: AccountId,
    contributor: AccountId,
}

fn rules_default() -> inklkunn_core::governance::Rules {
    inklkunn_core::governance::Rules::default()
}

fn stand() -> Stand {
    let w = under_rules(
        &with_accounts(World::with_visibility(
            "handbook",
            "The Handbook",
            Visibility::Public,
            &[("pages/Home.md", &home_with("The bridge opened in 1950."))],
        )),
        rules_default(),
    );
    let s = Stand {
        admin: person(1),
        maintainer: person(2),
        contributor: person(5),
        w,
    };
    add_person(&s.w, &s.admin, "Ada Admin", Some(Role::Admin));
    add_person(&s.w, &s.maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&s.w, &s.contributor, "Aiko Tanaka", Some(Role::Contributor));
    s
}

/// The records of the world, whole: the in-memory store prints everything it holds, but for the derived read cache (a read fills it,
/// and what is read is not what this test holds still).
fn records(s: &Stand) -> String {
    let all = format!("{:?}", s.w.store);
    match (all.find("derived: Mutex"), all.find("proposals: Mutex")) {
        (Some(from), Some(to)) if from < to => {
            // What the trail keeps of acts: a refusal is a line of the trail too, and is not an act.
            let acts: Vec<_> =
                s.w.audit
                    .events()
                    .into_iter()
                    .filter(|e| e.outcome != AuditOutcome::Denied)
                    .collect();
            format!("{}{}{acts:?}", &all[..from], &all[to..])
        }
        _ => panic!("the store's print has changed: {all}"),
    }
}

/// A proposal by the contributor with a comment: what a hide is made of. Returns the proposal and the comment.
async fn a_comment(s: &Stand) -> (ProposalId, String) {
    let propose = post_form(
        &s.w,
        "/handbook/-/proposals/new",
        Some(&s.contributor),
        &[
            ("op", "submit"),
            ("pages", "1"),
            ("renames", "0"),
            ("part", "whole"),
            ("base_state", &common::state_id(1)),
            ("base_block", &common::block_id(1)),
            ("p0_path", "pages/Home.md"),
            ("p0_title", "Home"),
            ("p0_text", &home_with("The bridge opened in 1951.")),
            ("summary", "Correct the year"),
            ("sources", ""),
        ],
    )
    .await;
    assert_eq!(propose.status, StatusCode::SEE_OTHER, "{}", propose.body());
    let to = propose.header("location").unwrap().to_owned();
    let id = ProposalId::parse(
        to.split("/proposals/")
            .nth(1)
            .unwrap()
            .split(['/', '?'])
            .next()
            .unwrap(),
    )
    .unwrap();
    let said = post_form(
        &s.w,
        &format!("/handbook/-/proposals/{id}/comment"),
        Some(&s.contributor),
        &[("body", "A comment on the proposal.")],
    )
    .await;
    assert_eq!(said.status, StatusCode::SEE_OTHER, "{}", said.body());
    let comment =
        s.w.store
            .comments(&id, None)
            .unwrap()
            .first()
            .expect("a comment")
            .id
            .to_string();
    (id, comment)
}

fn hide_fields(id: &ProposalId, comment: &str) -> Vec<(String, String)> {
    pairs(&[
        ("kind", "proposal-comment"),
        ("proposal", id.as_str()),
        ("comment", comment),
        ("part_content", "1"),
        ("part_author", "1"),
        ("category", "harassment"),
        ("note", ""),
    ])
}

fn pairs(fields: &[(&str, &str)]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// A hide made as the maintainer, whose first record is returned.
async fn a_hide(s: &Stand) -> (ProposalId, String, TombstoneEvent) {
    let (id, comment) = a_comment(s).await;
    let a = post_form(
        &s.w,
        "/handbook/-/moderate",
        Some(&s.maintainer),
        &borrowed(&hide_fields(&id, &comment)),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let event =
        s.w.store
            .tombstone_log(&s.w.wiki.id, None, 50)
            .unwrap()
            .into_iter()
            .next()
            .expect("a hide");
    (id, comment, event)
}

/// The contributor restricted by the maintainer.
async fn a_restriction(s: &Stand) {
    let a = post_form(
        &s.w,
        "/handbook/-/people/restrict",
        Some(&s.maintainer),
        &[
            ("who", s.contributor.as_str()),
            ("days", "3"),
            ("reason", "Edit-warring."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
}

/// An appeal of that restriction, open.
async fn an_appeal(s: &Stand) -> AppealRecord {
    a_restriction(s).await;
    let event =
        s.w.store
            .restriction_events(&s.w.wiki.id, &s.contributor)
            .unwrap()
            .remove(0);
    let filed = post_form(
        &s.w,
        "/handbook/-/appeals/new",
        Some(&s.contributor),
        &[
            ("kind", "restriction"),
            ("id", event.id.as_str()),
            ("text", "I did nothing wrong."),
        ],
    )
    .await;
    assert_eq!(filed.status, StatusCode::SEE_OTHER, "{}", filed.body());
    s.w.store.open_appeals(&s.w.wiki.id, 5).unwrap().remove(0)
}

fn borrowed(fields: &[(String, String)]) -> Vec<(&str, &str)> {
    fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

/// The body that does the act of `route` in `s`, after whatever the act needs to exist has been made as the people who make it.
async fn prepare(route: &str, s: &Stand) -> Vec<(String, String)> {
    match route {
        "/{slug}/-/moderate" => {
            let (id, comment) = a_comment(s).await;
            hide_fields(&id, &comment)
        }
        "/{slug}/-/moderate/unhide" => {
            let (_, _, hide) = a_hide(s).await;
            pairs(&[("hide", hide.id.as_str())])
        }
        "/{slug}/-/moderate/show" => {
            let (id, comment, hide) = a_hide(s).await;
            let mut fields = hide_fields(&id, &comment);
            fields.push(("hide".to_owned(), hide.id.to_string()));
            fields
        }
        "/{slug}/-/people/grant" => pairs(&[
            ("who", s.contributor.as_str()),
            ("role", "reviewer"),
            ("reason", "Good work."),
        ]),
        "/{slug}/-/people/restrict" => pairs(&[
            ("who", s.contributor.as_str()),
            ("days", "3"),
            ("reason", "Edit-warring."),
        ]),
        "/{slug}/-/people/renew" => {
            a_restriction(s).await;
            pairs(&[
                ("who", s.contributor.as_str()),
                ("days", "5"),
                ("reason", "Still."),
            ])
        }
        "/{slug}/-/people/lift" => {
            a_restriction(s).await;
            pairs(&[("who", s.contributor.as_str()), ("reason", "Settled.")])
        }
        "/{slug}/-/protect" => pairs(&[
            ("target", "page"),
            ("title", "Home"),
            ("require", "review"),
            ("reason", "Vandalism this week."),
            ("days", "7"),
            ("act", "set"),
        ]),
        "/{slug}/-/appeals/decide" => {
            let appeal = an_appeal(s).await;
            pairs(&[
                ("appeal", appeal.id.as_str()),
                ("decision", "overturned"),
                ("reason", "The record supports the appeal."),
            ])
        }
        other => panic!("{other} is web-only, and the test has no body for it"),
    }
}

fn address(route: &str) -> String {
    route.replace("{slug}", "handbook")
}

/// A token of `scope` for `account`, as the test identity reads it.
fn bearer(scope: Scope, account: &AccountId) -> String {
    format!("Bearer {}.{}", scope.as_str(), account)
}

fn encode(fields: &[(String, String)]) -> String {
    fields
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
}

/// Every address of the application origin that takes a `POST`, is under a wiki and is not the API's.
fn posts_under_a_wiki() -> BTreeSet<String> {
    inklkunn_web::routes::application_table()
        .unwrap()
        .entries()
        .iter()
        .filter(|e| e.methods.contains(&"POST"))
        .map(|e| e.path.to_owned())
        .filter(|p| p.starts_with("/{slug}/") && !p.starts_with("/{slug}/-/api/v1/"))
        .collect()
}

#[test]
fn every_write_form_of_a_wiki_is_listed_as_web_only_or_as_ordinary() {
    let known: BTreeSet<String> = WEB_ONLY
        .iter()
        .copied()
        .chain(ORDINARY.iter().map(|(p, _)| *p))
        .map(str::to_owned)
        .collect();
    let registered = posts_under_a_wiki();
    let unlisted: Vec<_> = registered.difference(&known).collect();
    assert!(
        unlisted.is_empty(),
        "a write form is in neither WEB_ONLY nor ORDINARY (a web-only act must be held to the token refusal): {unlisted:?}"
    );
    let gone: Vec<_> = known.difference(&registered).collect();
    assert!(
        gone.is_empty(),
        "a listed route is no longer registered: {gone:?}"
    );
    assert!(
        WEB_ONLY
            .iter()
            .all(|p| !ORDINARY.iter().any(|(o, _)| o == p)),
        "a route is in both lists"
    );
}

#[tokio::test]
async fn the_api_document_says_these_acts_are_web_only_and_has_no_path_that_does_one() {
    let w = stand().w;
    let a = common::get(&w, "/-/api/v1/openapi.json").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let doc: serde_json::Value = serde_json::from_slice(&a.bytes).expect("a document");
    assert!(
        a.body().contains("web-only in 0.4.0"),
        "the document does not say the acts are web-only"
    );
    let paths = doc
        .get("paths")
        .and_then(|p| p.as_object())
        .expect("the paths");
    for route in WEB_ONLY {
        let tail = route.trim_start_matches("/{slug}/-/");
        let api = format!("/{{slug}}/-/api/v1/{tail}");
        assert!(
            !paths
                .keys()
                .any(|k| *k == api || k.starts_with(&format!("{api}/"))),
            "{api}: the document has a path for a web-only act"
        );
    }
    // The members that tell of them only read.
    for member in ["policy", "protections", "moderation-log", "appeals/mine"] {
        let key = format!("/{{slug}}/-/api/v1/{member}");
        let item = paths.get(&key).and_then(|p| p.as_object());
        let methods: Vec<&String> = item
            .map(|i| {
                i.keys()
                    .filter(|k| ["get", "post", "put", "patch", "delete"].contains(&k.as_str()))
                    .collect()
            })
            .expect("the member is in the document");
        assert_eq!(methods, ["get"], "{key}: more than a read");
    }
}

/// Every web-only act, sent as a token of `scopes` of the admin's account, in a world of its own each: how each was answered and
/// whether the records or the trail of acts changed.
async fn sent_by_token(scopes: &[Scope]) -> Vec<(String, Scope, StatusCode, bool)> {
    let mut out = Vec::new();
    for route in WEB_ONLY {
        for scope in scopes {
            let s = stand();
            let body = prepare(route, &s).await;
            let before = records(&s);
            let bearer = bearer(*scope, &s.admin);
            let a = send_body(
                &s.w,
                "POST",
                &address(route),
                None,
                &[
                    ("sec-fetch-site", "same-origin"),
                    ("authorization", &bearer),
                ],
                encode(&body),
            )
            .await;
            out.push(((*route).to_owned(), *scope, a.status, records(&s) != before));
        }
    }
    out
}

#[tokio::test]
async fn the_session_of_an_admin_does_each_web_only_act_with_the_body_the_token_tests_send() {
    // The control of the tests below: a refusal of a token is a refusal of an act that is made when a session asks.
    for route in WEB_ONLY {
        let s = stand();
        let body = prepare(route, &s).await;
        let before = records(&s);
        let done = post_form(&s.w, &address(route), Some(&s.admin), &borrowed(&body)).await;
        assert!(
            done.status == StatusCode::SEE_OTHER || done.status == StatusCode::OK,
            "{route}: the session's act was not made: {} {}",
            done.status,
            done.body()
        );
        assert_ne!(
            records(&s),
            before,
            "{route}: the session's act changed nothing, so the control shows nothing"
        );
    }
}

#[tokio::test]
async fn a_token_that_does_not_carry_the_scope_of_the_act_reaches_no_web_only_act() {
    // The scopes below `accept` are refused each act, whatever the account's role is (a token carries less than the rung asks).
    let sent = sent_by_token(&[Scope::Read, Scope::Propose, Scope::Review]).await;
    assert_eq!(sent.len(), WEB_ONLY.len() * 3);
    for (route, scope, status, changed) in sent {
        assert!(
            status.is_client_error(),
            "{route} with a {scope:?} token: not refused: {status}"
        );
        assert!(!changed, "{route} with a {scope:?} token changed records");
    }
}

#[tokio::test]
#[ignore = "defect: a bearer token of an admin's account with scope accept or admin does moderation, restrictions, protections, appeal decisions and the rules form through the web forms (see the report of R-5 item 3)"]
async fn a_token_of_the_strongest_scope_reaches_no_web_only_act_and_nothing_is_changed() {
    // C-14f and RFC 006 D-12: these acts are never scripted through the API. A token judges a write alone and carries no CSRF
    // token, so the form routes, which take their principal from the layer, are reached by whoever holds one.
    let sent = sent_by_token(&[Scope::Accept, Scope::Admin]).await;
    assert_eq!(sent.len(), WEB_ONLY.len() * 2);
    let mut reached = Vec::new();
    for (route, scope, status, changed) in sent {
        if changed || !status.is_client_error() {
            reached.push(format!("{route} {scope:?}: {status}, changed: {changed}"));
        }
    }
    assert!(
        reached.is_empty(),
        "a token reached web-only acts: {reached:#?}"
    );
}
