//! The write API through the real router (RFC 004 D-13; handoff 004 T7; API-03 … API-05): opening, reading,
//! revising and taking back proposals, comments, decisions, acceptances and declines; the base each write names
//! in `If-Match` and the conflict a stale one is; the scopes of a token intersected with the roles of its
//! account; the rate limits with `Retry-After`; the answers that are the same for a wiki that is not there; and
//! what is data. The repository is an in-memory double, so these run in both test runs. Accounts are the test
//! identity's: a session (`x-test-account`) or a token (`Authorization: Bearer <scope>.<account id>`).

use axum::body::Body;
use axum::http::{Method, Request, header};
use inklkunn_core::Principal;
use inklkunn_core::proposal::QuotaConfig;
use inklkunn_store::{AccountId, ProposalFilter, ProposalId, Role, Visibility};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use super::common::{self, World, add_person, block_id, page, person, state_id, with_accounts};
use super::{Answer, At, get, router};

const HOME: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n";
const API: &str = "/handbook/-/api/v1";

struct Cast {
    w: World,
    contributor: AccountId,
    other: AccountId,
    reviewer: AccountId,
    maintainer: AccountId,
    admin: AccountId,
    observer: AccountId,
}

fn cast() -> Cast {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Home.md", HOME),
            (
                "pages/Guide.md",
                &page(&["title: Guide"], "# Setup\n\nInstall it.\n"),
            ),
        ],
    ));
    let c = Cast {
        contributor: person(1),
        other: person(2),
        reviewer: person(3),
        maintainer: person(4),
        admin: person(5),
        observer: person(6),
        w,
    };
    add_person(&c.w, &c.contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&c.w, &c.other, "Ben Okoye", Some(Role::Contributor));
    add_person(&c.w, &c.reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(&c.w, &c.maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&c.w, &c.admin, "Ada Admin", Some(Role::Admin));
    add_person(&c.w, &c.observer, "Olu Reader", Some(Role::Observer));
    c
}

// ------------------------------------------------------------------------------------ requests

fn session(who: &AccountId) -> (&'static str, String) {
    ("x-test-account", who.as_str().to_owned())
}

fn token(scope: &str, who: &AccountId) -> (&'static str, String) {
    ("authorization", format!("Bearer {scope}.{}", who.as_str()))
}

fn quoted(tag: &str) -> (&'static str, String) {
    ("if-match", format!("\"{tag}\""))
}

fn tip() -> (&'static str, String) {
    quoted(&state_id(1))
}

/// A write: `body` as JSON with the headers given (a JSON content type unless one is given).
async fn post(w: &World, uri: &str, headers: &[(&'static str, String)], body: String) -> Answer {
    send(w, Method::POST, uri, headers, Body::from(body)).await
}

async fn send(
    w: &World,
    method: Method,
    uri: &str,
    headers: &[(&'static str, String)],
    body: Body,
) -> Answer {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "localhost:8080");
    if !headers.iter().any(|(k, _)| *k == "content-type") {
        req = req.header(header::CONTENT_TYPE, "application/json");
    }
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    let res = router(w).oneshot(req.body(body).unwrap()).await.unwrap();
    let (parts, body) = res.into_parts();
    Answer {
        status: parts.status,
        headers: parts.headers,
        bytes: axum::body::to_bytes(body, 1 << 26).await.unwrap().to_vec(),
    }
}

fn principal(who: &AccountId) -> Principal {
    Principal::session(&inklkunn_core::IdentityAuthority::for_tests(), who.clone())
}

fn files_of_home(text: &str) -> Value {
    json!([{"path": "pages/Home.md", "text": text}])
}

fn opening(summary: &str, text: &str, submit: bool) -> String {
    json!({"summary": summary, "submit": submit, "files": files_of_home(text)}).to_string()
}

fn better() -> String {
    HOME.replace("Second paragraph.", "Second paragraph, better.")
}

/// Open a proposal as `who` (a session) and give the answer's JSON; asserts the `201`.
async fn open(c: &Cast, who: &AccountId, submit: bool) -> Value {
    let a = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(who), tip()],
        opening("Improve the second paragraph", &better(), submit),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    a.json()
}

fn id_of(proposal: &Value) -> ProposalId {
    ProposalId::parse(proposal.at("/id").as_str().unwrap()).unwrap()
}

fn etag_of(proposal: &Value) -> String {
    proposal.at("/etag").as_str().unwrap().to_owned()
}

fn tag_header(proposal: &Value) -> (&'static str, String) {
    ("if-match", etag_of(proposal))
}

fn problem_type(a: &Answer) -> String {
    a.json().at("/type").as_str().unwrap().to_owned()
}

fn urn(code: &str) -> String {
    format!("urn:inklkunn:problem:{code}")
}

// ------------------------------------------------------------------------------------ the life of a proposal

#[tokio::test]
async fn a_proposal_is_opened_read_decided_revised_and_taken_back_each_write_naming_its_base() {
    let c = cast();
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    assert_eq!(p.at("/state"), "held-for-review");
    assert_eq!(p.at("/revision"), 1);
    assert_eq!(p.at("/yours"), true);
    assert_eq!(p.at("/view"), "main");
    assert_eq!(etag_of(&p), format!("\"{id}-r1\""));

    // The read gives the same base, and says whose it is.
    let read = super::send(
        &c.w,
        Method::GET,
        &format!("{API}/proposals/{id}"),
        &[("x-test-account", c.reviewer.as_str())],
    )
    .await;
    assert_eq!(read.status, 200, "{}", read.text());
    assert_eq!(read.header("etag"), Some(etag_of(&p).as_str()));
    assert_eq!(read.json().at("/etag"), p.at("/etag"));
    assert_eq!(read.json().at("/yours"), false);

    // A reviewer decides on the revision the tag names.
    let decided = post(
        &c.w,
        &format!("{API}/proposals/{id}/decisions"),
        &[token("review", &c.reviewer), tag_header(&p)],
        json!({"decision": "approve", "body": "Good. 良い。"}).to_string(),
    )
    .await;
    assert_eq!(decided.status, 201, "{}", decided.text());
    assert_eq!(decided.json().at("/revision"), 1);
    assert_eq!(decided.json().at("/decision"), "approve");
    // A decision is not changed.
    let again = post(
        &c.w,
        &format!("{API}/proposals/{id}/decisions"),
        &[token("review", &c.reviewer), tag_header(&p)],
        json!({"decision": "request-changes"}).to_string(),
    )
    .await;
    assert_eq!(again.status, 409);
    assert_eq!(problem_type(&again), urn("already-decided"));

    // The proposer revises; the revision has a new base, and the old one is stale.
    let text = HOME.replace("Second paragraph.", "Second paragraph, better still.");
    let revised = post(
        &c.w,
        &format!("{API}/proposals/{id}/revisions"),
        &[session(&c.contributor), tag_header(&p)],
        opening("Improve it again", &text, true),
    )
    .await;
    // (A revision has no `submit`: it is sent for review by being made.)
    assert_eq!(revised.status, 400, "{}", revised.text());
    assert_eq!(revised.json().at("/member"), "submit");
    let revised = post(
        &c.w,
        &format!("{API}/proposals/{id}/revisions"),
        &[session(&c.contributor), tag_header(&p)],
        json!({"summary": "Improve it again", "files": files_of_home(&text)}).to_string(),
    )
    .await;
    assert_eq!(revised.status, 201, "{}", revised.text());
    let r2 = revised.json();
    assert_eq!(r2.at("/revision"), 2);
    assert_eq!(etag_of(&r2), format!("\"{id}-r2\""));
    assert_eq!(
        revised.header("location"),
        Some(format!("{API}/proposals/{id}").as_str())
    );
    assert_eq!(revised.header("etag"), Some(etag_of(&r2).as_str()));

    let stale = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&p)],
        json!({"body": "Looks fine"}).to_string(),
    )
    .await;
    assert_eq!(stale.status, 409);
    assert_eq!(problem_type(&stale), urn("stale-base"));
    assert_eq!(stale.json().at("/current"), r2.at("/etag"));
    assert_eq!(stale.json().at("/current_revision"), 2);

    // A comment on the revision seen, with a reply and a part of the page.
    let comment = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&r2)],
        json!({"body": "Is *better* the word?", "anchor": {"page": "pages/Home.md", "ordinal": 1}})
            .to_string(),
    )
    .await;
    assert_eq!(comment.status, 201, "{}", comment.text());
    assert_eq!(comment.json().at("/revision"), 2);
    assert_eq!(comment.json().at("/anchor/page"), "pages/Home.md");
    let reply = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[session(&c.contributor), tag_header(&r2)],
        json!({"body": "Yes.", "parent": comment.json().at("/id")}).to_string(),
    )
    .await;
    assert_eq!(reply.status, 201, "{}", reply.text());
    assert_eq!(reply.json().at("/parent"), comment.json().at("/id"));

    // The proposer takes it back, with a reason.
    let none = post(
        &c.w,
        &format!("{API}/proposals/{id}/withdrawals"),
        &[session(&c.contributor), tag_header(&r2)],
        json!({"reason": "  "}).to_string(),
    )
    .await;
    assert_eq!(none.status, 422);
    assert_eq!(problem_type(&none), urn("unacceptable"));
    assert_eq!(none.json().at("/reason"), "reason-missing");
    let gone = post(
        &c.w,
        &format!("{API}/proposals/{id}/withdrawals"),
        &[session(&c.contributor), tag_header(&r2)],
        json!({"reason": "Found a better place for it"}).to_string(),
    )
    .await;
    assert_eq!(gone.status, 200, "{}", gone.text());
    assert_eq!(gone.json().at("/state"), "withdrawn");
    // Nothing more can be done to it.
    let late = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&r2)],
        json!({"body": "Too late"}).to_string(),
    )
    .await;
    assert_eq!(late.status, 409);
    assert_eq!(problem_type(&late), urn("not-in-this-state"));
    assert_eq!(late.json().at("/state"), "withdrawn");
}

#[tokio::test]
async fn a_draft_is_saved_and_sent_and_is_its_authors_alone() {
    let c = cast();
    let p = open(&c, &c.contributor, false).await;
    let id = id_of(&p);
    assert_eq!(p.at("/state"), "draft");
    assert_eq!(p.at("/revision"), 0);
    assert_eq!(etag_of(&p), format!("\"{id}-r0\""));

    // Nobody else can read it, write on it or tell it from a proposal that is not there.
    let missing = ProposalId::new();
    for (uri_id, who) in [(id.to_string(), &c.other), (missing.to_string(), &c.other)] {
        let a = super::send(
            &c.w,
            Method::GET,
            &format!("{API}/proposals/{uri_id}"),
            &[("x-test-account", who.as_str())],
        )
        .await;
        assert_eq!(a.status, 404, "{uri_id}");
        assert_eq!(problem_type(&a), urn("not-found"));
        for (word, body) in [
            ("draft", json!({"summary": "x"})),
            ("submission", json!({})),
            ("withdrawals", json!({"reason": "x"})),
            ("comments", json!({"body": "x"})),
        ] {
            let a = post(
                &c.w,
                &format!("{API}/proposals/{uri_id}/{word}"),
                &[token("accept", who), quoted(&format!("{uri_id}-r0"))],
                body.to_string(),
            )
            .await;
            assert_eq!(a.status, 404, "{word}: {}", a.text());
            assert_eq!(problem_type(&a), urn("not-found"), "{word}");
        }
    }

    // The author saves it, then sends it.
    let saved = post(
        &c.w,
        &format!("{API}/proposals/{id}/draft"),
        &[session(&c.contributor), tag_header(&p)],
        json!({"summary": "A better summary", "files": files_of_home(&better())}).to_string(),
    )
    .await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    assert_eq!(saved.json().at("/summary"), "A better summary");
    assert_eq!(saved.json().at("/state"), "draft");
    let sent = post(
        &c.w,
        &format!("{API}/proposals/{id}/submission"),
        &[session(&c.contributor), tag_header(&p)],
        "{}".to_owned(),
    )
    .await;
    assert_eq!(sent.status, 200, "{}", sent.text());
    assert_eq!(sent.json().at("/state"), "held-for-review");
    assert_eq!(sent.json().at("/revision"), 1);
    // The draft's base is now stale.
    let stale = post(
        &c.w,
        &format!("{API}/proposals/{id}/draft"),
        &[session(&c.contributor), tag_header(&p)],
        json!({"summary": "Again", "files": files_of_home(&better())}).to_string(),
    )
    .await;
    assert_eq!(stale.status, 409);
    assert_eq!(problem_type(&stale), urn("stale-base"));
    assert_eq!(stale.json().at("/current_revision"), 1);
}

#[tokio::test]
async fn a_new_page_is_named_by_its_title_and_a_title_that_is_taken_or_a_path_outside_pages_is_refused()
 {
    let c = cast();
    let fresh = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        json!({"summary": "A new page", "submit": true,
               "files": [{"title": "Notes/Today", "text": "---\ntitle: Notes/Today\n---\n\nHello.\n"}]})
        .to_string(),
    )
    .await;
    assert_eq!(fresh.status, 201, "{}", fresh.text());
    let who = principal(&c.contributor);
    let id = id_of(&fresh.json());
    let files =
        c.w.services
            .proposals
            .revisions(&who, &c.w.wiki, &id)
            .unwrap()
            .pop()
            .unwrap()
            .content
            .files;
    assert_eq!(files.len(), 1);
    assert_eq!(files.first().unwrap().path, "pages/Notes/Today.md");

    for (file, reason) in [
        (json!({"title": "Home", "text": "x"}), "title-taken"),
        (json!({"title": "", "text": "x"}), "title-invalid"),
        (
            json!({"path": "README.md", "text": "x"}),
            "path-outside-pages",
        ),
    ] {
        let a = post(
            &c.w,
            &format!("{API}/proposals"),
            &[session(&c.contributor), tip()],
            json!({"summary": "x", "submit": false, "files": [file]}).to_string(),
        )
        .await;
        assert_eq!(a.status, 422, "{reason}: {}", a.text());
        assert_eq!(problem_type(&a), urn("unacceptable"));
        assert_eq!(a.json().at("/reason"), reason);
        assert!(!a.json().at("/detail").as_str().unwrap().is_empty());
    }
    // A rename that leaves the repository is refused by the service's own rule.
    let a = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        json!({"summary": "x", "submit": false, "files": [],
               "renames": [{"from": "../x.md", "to": "pages/y.md"}]})
        .to_string(),
    )
    .await;
    assert_eq!(a.status, 422, "{}", a.text());
    assert_eq!(a.json().at("/reason"), "bad-path");
    // A page is addressed by a path or a title, and a removal has a path.
    for (file, member) in [
        (
            json!({"path": "pages/Home.md", "title": "Home", "text": "x"}),
            "files",
        ),
        (json!({"title": "New", "text": null}), "files"),
        (json!({"path": "pages/Home.md"}), "files[0].text"),
        (
            json!({"path": "pages/Home.md", "text": "x", "mode": 1}),
            "files[0].mode",
        ),
    ] {
        let a = post(
            &c.w,
            &format!("{API}/proposals"),
            &[session(&c.contributor), tip()],
            json!({"summary": "x", "submit": false, "files": [file]}).to_string(),
        )
        .await;
        assert_eq!(a.status, 400, "{member}: {}", a.text());
        assert_eq!(a.json().at("/member"), member);
    }
}

#[tokio::test]
async fn the_keys_the_service_owns_are_refused_to_a_contributor_as_they_are_on_the_form() {
    let c = cast();
    let owned = page(&["title: Home", "visibility: restricted"], "Body.\n");
    let body = |text: &str| {
        json!({"summary": "x", "submit": true, "files": [{"path": "pages/Home.md", "text": text}]})
            .to_string()
    };
    let refused = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        body(&owned),
    )
    .await;
    assert_eq!(refused.status, 422, "{}", refused.text());
    assert_eq!(refused.json().at("/reason"), "front-owned");
    assert!(
        refused
            .json()
            .at("/detail")
            .as_str()
            .unwrap()
            .contains("visibility")
    );
    let unreadable = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        body("---\ntitle: &anchor Home\n---\n\nBody.\n"),
    )
    .await;
    assert_eq!(unreadable.status, 422);
    assert_eq!(unreadable.json().at("/reason"), "front-unreadable");
    // An administrator may write them.
    let allowed = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.admin), tip()],
        body(&owned),
    )
    .await;
    assert_eq!(allowed.status, 201, "{}", allowed.text());
}

// ------------------------------------------------------------------------------------ the base

#[tokio::test]
async fn a_stale_if_match_is_a_409_problem_detail_with_a_stable_type_and_the_current_base() {
    let c = cast();
    // A new proposal names the view's state it was written against.
    let stale = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), quoted(&state_id(9))],
        opening("x", &better(), true),
    )
    .await;
    assert_eq!(stale.status, 409, "{}", stale.text());
    assert_eq!(
        stale.header("content-type"),
        Some("application/problem+json")
    );
    let body = stale.json();
    assert_eq!(body.at("/type"), "urn:inklkunn:problem:stale-base");
    assert_eq!(body.at("/status"), 409);
    assert_eq!(body.at("/current_state_id"), &json!(state_id(1)));
    assert_eq!(body.at("/current"), &json!(format!("\"{}\"", state_id(1))));
    // Nothing was recorded.
    let all =
        c.w.services
            .proposals
            .list(
                &principal(&c.contributor),
                &c.w.wiki,
                &ProposalFilter::default(),
            )
            .unwrap();
    assert!(all.is_empty());

    // The read API gives the state a write names.
    let read = get(&c.w, &format!("{API}/pages?limit=1")).await.json();
    assert_eq!(read.at("/tip/state_id"), &json!(state_id(1)));
    assert_eq!(read.at("/tip/block_id"), &json!(block_id(1)));

    // The same words in Japanese; the type and the status do not change.
    let ja = send_ja(&c, &state_id(9)).await;
    assert_eq!(ja.json().at("/type"), "urn:inklkunn:problem:stale-base");
    assert_eq!(ja.json().at("/status"), 409);
    assert_ne!(ja.json().at("/title"), body.at("/title"));
}

async fn send_ja(c: &Cast, state: &str) -> Answer {
    post(
        &c.w,
        &format!("{API}/proposals"),
        &[
            session(&c.contributor),
            quoted(state),
            ("cookie", "inklkunn_lang=ja".to_owned()),
        ],
        opening("x", &better(), true),
    )
    .await
}

#[tokio::test]
async fn a_write_that_names_no_base_or_a_wrong_one_is_told_so() {
    let c = cast();
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    let uri = format!("{API}/proposals/{id}/comments");
    let body = json!({"body": "x"}).to_string();
    let who = || token("propose", &c.other);

    let none = post(&c.w, &uri, &[who()], body.clone()).await;
    assert_eq!(none.status, 428, "{}", none.text());
    assert_eq!(problem_type(&none), urn("base-required"));

    for bad in ["*", "W/\"x\"", "\"a\", \"b\"", "garbage", "\"\"", "\"x y\""] {
        let a = post(
            &c.w,
            &uri,
            &[who(), ("if-match", bad.to_owned())],
            body.clone(),
        )
        .await;
        assert_eq!(a.status, 400, "{bad}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-parameter"));
        assert_eq!(a.json().at("/parameter"), "If-Match");
    }
    // The tag of another proposal, and the tag of the view's state, are not this proposal's base.
    let other = open(&c, &c.contributor, true).await;
    for tag in [etag_of(&other), format!("\"{}\"", state_id(1))] {
        let a = post(
            &c.w,
            &uri,
            &[who(), ("if-match", tag.clone())],
            body.clone(),
        )
        .await;
        assert_eq!(a.status, 400, "{tag}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-parameter"));
    }
    // A new proposal's base is a state id.
    let a = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), quoted("not-a-state")],
        opening("x", &better(), true),
    )
    .await;
    assert_eq!(a.status, 400, "{}", a.text());
    let a = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor)],
        opening("x", &better(), true),
    )
    .await;
    assert_eq!(a.status, 428);
    // The right one goes through.
    let a = post(&c.w, &uri, &[who(), tag_header(&p)], body).await;
    assert_eq!(a.status, 201, "{}", a.text());
}

// ------------------------------------------------------------------------------------ credentials and scopes

#[tokio::test]
async fn a_token_with_propose_cannot_accept_even_when_its_account_is_a_maintainer() {
    let c = cast();
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    let accept = format!("{API}/proposals/{id}/acceptances");
    let decline = format!("{API}/proposals/{id}/declines");
    let decide = format!("{API}/proposals/{id}/decisions");

    // The account is a maintainer: through a session, which carries every scope, the endpoint lets it go on
    // (this build has no acceptor, which it says after the checks).
    let session_accept = post(
        &c.w,
        &accept,
        &[session(&c.maintainer), tag_header(&p)],
        "{}".to_owned(),
    )
    .await;
    assert_eq!(session_accept.status, 501, "{}", session_accept.text());
    assert_eq!(problem_type(&session_accept), urn("not-available"));
    assert_eq!(session_accept.json().at("/feature"), "accept");

    // The same account through tokens that carry less is refused at the endpoint, before anything else.
    for scope in ["read", "propose", "review"] {
        let a = post(
            &c.w,
            &accept,
            &[token(scope, &c.maintainer), tag_header(&p)],
            "{}".to_owned(),
        )
        .await;
        assert_eq!(a.status, 403, "{scope}: {}", a.text());
        assert_eq!(problem_type(&a), urn("forbidden"), "{scope}");
        let d = post(
            &c.w,
            &decline,
            &[token(scope, &c.maintainer), tag_header(&p)],
            json!({"reason": "No"}).to_string(),
        )
        .await;
        assert_eq!(d.status, 403, "{scope}: {}", d.text());
    }
    // And a token with `propose` cannot review either, though its account is a maintainer.
    for scope in ["read", "propose"] {
        let a = post(
            &c.w,
            &decide,
            &[token(scope, &c.maintainer), tag_header(&p)],
            json!({"decision": "approve"}).to_string(),
        )
        .await;
        assert_eq!(a.status, 403, "{scope}: {}", a.text());
    }
    // `accept` is enough for the endpoint, and `review` decides.
    let a = post(
        &c.w,
        &accept,
        &[token("accept", &c.maintainer), tag_header(&p)],
        "{}".to_owned(),
    )
    .await;
    assert_eq!(a.status, 501, "{}", a.text());
    let a = post(
        &c.w,
        &decide,
        &[token("review", &c.maintainer), tag_header(&p)],
        json!({"decision": "approve"}).to_string(),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());

    // The role limits the token as the token limits the role: a contributor's token with the widest scope.
    for scope in ["accept", "admin"] {
        let a = post(
            &c.w,
            &accept,
            &[token(scope, &c.contributor), tag_header(&p)],
            "{}".to_owned(),
        )
        .await;
        assert_eq!(a.status, 403, "{scope}: {}", a.text());
    }
    // A declined proposal carries the acceptor's reason.
    let declined = post(
        &c.w,
        &decline,
        &[token("accept", &c.maintainer), tag_header(&p)],
        json!({"reason": "Not for this wiki"}).to_string(),
    )
    .await;
    assert_eq!(declined.status, 200, "{}", declined.text());
    assert_eq!(declined.json().at("/state"), "declined");
}

#[tokio::test]
async fn every_write_needs_an_account_and_a_token_that_does_not_authenticate_is_not_an_anonymous_request()
 {
    let c = cast();
    let uri = format!("{API}/proposals");
    // No credential at all, on a wiki anyone may read: sign in.
    let a = post(&c.w, &uri, &[tip()], opening("x", &better(), true)).await;
    assert_eq!(a.status, 401, "{}", a.text());
    assert_eq!(problem_type(&a), urn("authentication-required"));
    assert_eq!(a.header("www-authenticate"), Some("Bearer"));
    // A token that is not one: the request says what is wrong with it, wherever it goes.
    for bad in [
        "Bearer nonsense",
        "Bearer admin.not-an-account",
        "Basic dXNlcjpwYXNz",
        "Bearer",
        "Bearer a b",
    ] {
        let a = post(
            &c.w,
            &uri,
            &[("authorization", bad.to_owned()), tip()],
            opening("x", &better(), true),
        )
        .await;
        assert_eq!(a.status, 401, "{bad}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-token"), "{bad}");
        assert_eq!(
            a.header("www-authenticate"),
            Some("Bearer error=\"invalid_token\"")
        );
        // The token is never repeated.
        assert!(!a.text().contains("nonsense"));
    }
    // A token wins over a session cookie: the session's account does not widen the token.
    let a = post(
        &c.w,
        &uri,
        &[token("read", &c.contributor), session(&c.admin), tip()],
        opening("x", &better(), true),
    )
    .await;
    assert_eq!(a.status, 403, "{}", a.text());
    // A reader who may not propose is told so.
    let a = post(
        &c.w,
        &uri,
        &[session(&c.observer), tip()],
        opening("x", &better(), true),
    )
    .await;
    assert_eq!(a.status, 403);
    assert_eq!(problem_type(&a), urn("forbidden"));
}

#[tokio::test]
async fn a_session_without_a_token_must_come_from_this_site_and_a_token_need_not() {
    let c = cast();
    let uri = format!("{API}/proposals");
    for cross in [
        ("sec-fetch-site", "cross-site"),
        ("sec-fetch-site", "same-site"),
        ("origin", "http://evil.example"),
    ] {
        let a = post(
            &c.w,
            &uri,
            &[
                session(&c.contributor),
                tip(),
                (cross.0, cross.1.to_owned()),
            ],
            opening("x", &better(), true),
        )
        .await;
        assert_eq!(a.status, 403, "{cross:?}: {}", a.text());
        assert_eq!(problem_type(&a), urn("forbidden"));
        // A token is not sent by a browser by itself: the same request with one is not a forgery.
        let a = post(
            &c.w,
            &uri,
            &[
                token("propose", &c.contributor),
                tip(),
                (cross.0, cross.1.to_owned()),
            ],
            opening("x", &better(), false),
        )
        .await;
        assert_eq!(a.status, 201, "{cross:?}: {}", a.text());
    }
    for same in [
        ("sec-fetch-site", "same-origin"),
        ("origin", "http://localhost:8080"),
    ] {
        let a = post(
            &c.w,
            &uri,
            &[session(&c.contributor), tip(), (same.0, same.1.to_owned())],
            opening("x", &better(), false),
        )
        .await;
        assert_eq!(a.status, 201, "{same:?}: {}", a.text());
    }
    // A form cannot send JSON, so a cookie-borne request that is a form is not a write.
    let a = post(
        &c.w,
        &uri,
        &[
            session(&c.contributor),
            tip(),
            (
                "content-type",
                "application/x-www-form-urlencoded".to_owned(),
            ),
        ],
        "summary=x".to_owned(),
    )
    .await;
    assert_eq!(a.status, 415);
    assert_eq!(problem_type(&a), urn("unsupported-media-type"));
}

// ------------------------------------------------------------------------------------ accepting

/// A signer that seals whatever it is asked to, with the ids a real one would give.
struct Sealing;

impl inklkunn_core::Accept for Sealing {
    fn accept(
        &self,
        _: &inklkunn_store::WikiId,
        _: &str,
        _: &inklkunn_core::AcceptanceToken,
        _: &inklkunn_core::PreparedChange,
    ) -> Result<inklkunn_core::SealedChange, inklkunn_core::AcceptError> {
        Ok(inklkunn_core::SealedChange {
            block_id: "e".repeat(64),
            patch_ids: vec!["9".repeat(64)],
        })
    }

    fn create_view(
        &self,
        _: &inklkunn_store::WikiId,
        _: &str,
        _: &str,
        _: &inklkunn_core::ViewCreationAct,
    ) -> Result<(), inklkunn_core::AcceptError> {
        Err(inklkunn_core::AcceptError::NotPending)
    }
}

/// The world with the acceptance of RFC 004 D-4 over a signer that seals, and the directory it leans on.
fn with_acceptor(mut c: Cast) -> (Cast, common::TempDir) {
    use inklkunn_core::{AcceptanceParts, LeaseConfig, ProposalAcceptor, WikiLeases};
    let dir = common::TempDir::new("accept");
    let layout = inklkunn_store::RootLayout::new(dir.0.join("root"));
    layout.prepare(&c.w.wiki.id).unwrap();
    let store = c.w.store.clone();
    let leases = WikiLeases::new(
        inklkunn_core::WriteLeases::new(layout.clone()),
        store.clone(),
        "test-process",
        LeaseConfig::default(),
    );
    let acceptor = ProposalAcceptor::new(AcceptanceParts {
        proposals: c.w.services.proposals.clone(),
        acceptances: store.clone(),
        acts: store.clone(),
        roles: store.clone(),
        accounts: store.clone(),
        storage: c.w.fake.clone(),
        layout,
        signer: std::sync::Arc::new(Sealing),
        leases,
        reads: c.w.services.read.clone(),
    })
    .with_reconciliation(false);
    c.w.services = std::sync::Arc::new(inklkunn_web::Services {
        acceptor: Some(std::sync::Arc::new(acceptor)),
        ..(*c.w.services).clone()
    });
    (c, dir)
}

#[tokio::test]
async fn an_acceptor_with_the_scope_and_the_role_signs_the_revision_once_and_a_retry_writes_nothing()
 {
    let (c, _dir) = with_acceptor(cast());
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    let accept = format!("{API}/proposals/{id}/acceptances");
    let a = post(
        &c.w,
        &accept,
        &[token("accept", &c.maintainer), tag_header(&p)],
        "{}".to_owned(),
    )
    .await;
    assert_eq!(a.status, 200, "{}", a.text());
    assert_eq!(a.json().at("/state"), "accepted");
    assert_eq!(a.json().at("/acceptance/outcome"), "signed");
    assert_eq!(a.json().at("/acceptance/block_id"), &json!("e".repeat(64)));
    assert_eq!(a.header("etag"), Some(etag_of(&p).as_str()));
    // A retry of the same revision answers what the first did and seals nothing again.
    let again = post(
        &c.w,
        &accept,
        &[token("accept", &c.maintainer), tag_header(&p)],
        "{}".to_owned(),
    )
    .await;
    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(again.json().at("/acceptance/outcome"), "already-signed");
    assert_eq!(
        again.json().at("/acceptance/id"),
        a.json().at("/acceptance/id")
    );
    // A token that carries less is refused before the acceptor is asked.
    let q = open(&c, &c.contributor, true).await;
    let refused = post(
        &c.w,
        &format!("{API}/proposals/{}/acceptances", id_of(&q)),
        &[token("propose", &c.maintainer), tag_header(&q)],
        "{}".to_owned(),
    )
    .await;
    assert_eq!(refused.status, 403);
    let state =
        c.w.services
            .proposals
            .get(&principal(&c.maintainer), &c.w.wiki, &id_of(&q))
            .unwrap()
            .state;
    assert_eq!(state.as_str(), "held-for-review");
    // And a base that is not the latest revision is stale, not accepted.
    let r = post(
        &c.w,
        &format!("{API}/proposals/{}/revisions", id_of(&q)),
        &[session(&c.contributor), tag_header(&q)],
        json!({"summary": "again", "files": files_of_home(&better().replace("better", "best"))})
            .to_string(),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let stale = post(
        &c.w,
        &format!("{API}/proposals/{}/acceptances", id_of(&q)),
        &[token("accept", &c.maintainer), tag_header(&q)],
        "{}".to_owned(),
    )
    .await;
    assert_eq!(stale.status, 409, "{}", stale.text());
    assert_eq!(problem_type(&stale), urn("stale-base"));
}

// ------------------------------------------------------------------------------------ limits

fn with_quota(mut c: Cast, quota: QuotaConfig) -> Cast {
    let services = &c.w.services;
    let proposals = std::sync::Arc::new(
        inklkunn_core::proposal::ProposalService::new(
            c.w.store.clone(),
            c.w.store.clone(),
            c.w.store.clone(),
            (*services.authorizer).clone(),
            services.read.clone(),
            quota,
        )
        .with_clock(|| common::NOW),
    );
    c.w.services = std::sync::Arc::new(inklkunn_web::Services {
        proposals,
        ..(*c.w.services).clone()
    });
    c
}

#[tokio::test]
async fn a_refused_rate_carries_retry_after_and_names_its_limit_and_numbers() {
    let c = with_quota(
        cast(),
        QuotaConfig {
            proposals_per_hour_per_principal: 2,
            ..QuotaConfig::default()
        },
    );
    for _ in 0..2 {
        open(&c, &c.contributor, false).await;
    }
    let refused = post(
        &c.w,
        &format!("{API}/proposals"),
        &[token("propose", &c.contributor), tip()],
        opening("One too many", &better(), false),
    )
    .await;
    assert_eq!(refused.status, 429, "{}", refused.text());
    assert_eq!(problem_type(&refused), urn("refused-limit"));
    // The window is an hour from the oldest record, which was made now.
    assert_eq!(refused.header("retry-after"), Some("3600"));
    let b = refused.json();
    assert_eq!(b.at("/limit"), "proposals-per-hour-per-principal");
    assert_eq!(b.at("/allowed"), 2);
    assert_eq!(b.at("/current"), 2);
    assert_eq!(b.at("/asked"), 1);
    assert_eq!(b.at("/reset"), "at");
    assert_eq!(b.at("/reset_at"), common::NOW + 3600);
    // Another person is not held by it.
    open(&c, &c.other, false).await;
}

#[tokio::test]
async fn a_limit_that_waiting_cannot_lift_says_so_and_has_no_retry_after() {
    let c = with_quota(
        cast(),
        QuotaConfig {
            open_proposals: 1,
            ..QuotaConfig::default()
        },
    );
    open(&c, &c.contributor, false).await;
    let refused = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        opening("Another", &better(), false),
    )
    .await;
    assert_eq!(refused.status, 429);
    assert_eq!(refused.json().at("/limit"), "open-proposals");
    assert_eq!(refused.json().at("/reset"), "when-a-proposal-closes");
    assert_eq!(refused.header("retry-after"), None);
}

#[tokio::test]
async fn a_body_is_bounded_before_it_is_parsed_and_the_texts_by_the_forms_own_bounds() {
    let c = cast();
    let uri = format!("{API}/proposals");
    let headers = || vec![session(&c.contributor), tip()];

    // Over the bound: refused at the declared length, without reading a byte.
    let mut declared = headers();
    declared.push(("content-length", (1usize << 30).to_string()));
    let a = post(&c.w, &uri, &declared, "{}".to_owned()).await;
    assert_eq!(a.status, 413, "{}", a.text());
    assert_eq!(problem_type(&a), urn("body-too-large"));
    assert!(
        a.json()
            .at("/limit")
            .as_str()
            .unwrap()
            .parse::<usize>()
            .unwrap()
            > 2 << 20
    );
    // And refused when it is longer than it said, or said nothing.
    let huge = format!("{{\"summary\":\"{}\"}}", "x".repeat(13 << 20));
    let a = post(&c.w, &uri, &headers(), huge).await;
    assert_eq!(a.status, 413);
    assert_eq!(problem_type(&a), urn("body-too-large"));

    // A page over a mebibyte is refused by the form's bound, not by the parser.
    let big = "a".repeat((1 << 20) + 1);
    let a = post(&c.w, &uri, &headers(), opening("x", &big, true)).await;
    assert_eq!(a.status, 422, "{}", &a.text()[..a.text().len().min(300)]);
    assert_eq!(a.json().at("/reason"), "page-too-large");
    // Three pages of 800 KiB are each fine and together over two mebibytes.
    let part = "b".repeat(800 * 1024);
    let three = json!({"summary": "x", "submit": true, "files": [
        {"title": "A", "text": part}, {"title": "B", "text": part}, {"title": "C", "text": part}]});
    let a = post(&c.w, &uri, &headers(), three.to_string()).await;
    assert_eq!(a.status, 422);
    assert_eq!(a.json().at("/reason"), "too-large");

    // The comment's body is bounded by its own endpoint.
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    let a = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&p)],
        json!({"body": "x".repeat(200 * 1024)}).to_string(),
    )
    .await;
    assert_eq!(a.status, 413);
    let a = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&p)],
        json!({"body": "x".repeat(20 * 1024)}).to_string(),
    )
    .await;
    assert_eq!(a.status, 422);
    assert_eq!(a.json().at("/reason"), "comment-too-long");
}

#[tokio::test]
async fn a_body_is_one_json_object_with_the_members_the_endpoint_takes_and_no_other() {
    let c = cast();
    let uri = format!("{API}/proposals");
    let ok = opening("x", &better(), false);
    for (what, headers, body, status, kind, member) in [
        (
            "not json",
            vec![session(&c.contributor), tip()],
            "summary=x".to_owned(),
            400,
            "invalid-body",
            Some("$"),
        ),
        (
            "an array",
            vec![session(&c.contributor), tip()],
            "[]".to_owned(),
            400,
            "invalid-body",
            Some("$"),
        ),
        (
            "empty",
            vec![session(&c.contributor), tip()],
            String::new(),
            400,
            "invalid-body",
            Some("$"),
        ),
        (
            "unknown member",
            vec![session(&c.contributor), tip()],
            ok.replace("\"submit\"", "\"sumbit\""),
            400,
            "invalid-body",
            Some("submit"),
        ),
        (
            "an unknown member",
            vec![session(&c.contributor), tip()],
            ok.replacen('{', "{\"extra\":1,", 1),
            400,
            "invalid-body",
            Some("extra"),
        ),
        (
            "missing member",
            vec![session(&c.contributor), tip()],
            json!({"submit": true, "files": []}).to_string(),
            400,
            "invalid-body",
            Some("summary"),
        ),
        (
            "wrong type",
            vec![session(&c.contributor), tip()],
            json!({"summary": 7, "submit": true}).to_string(),
            400,
            "invalid-body",
            Some("summary"),
        ),
        (
            "repeated member",
            vec![session(&c.contributor), tip()],
            r#"{"summary":"a","summary":"b","submit":false}"#.to_owned(),
            400,
            "invalid-body",
            Some("summary"),
        ),
        (
            "the wrong media type",
            vec![
                session(&c.contributor),
                tip(),
                ("content-type", "text/plain".to_owned()),
            ],
            ok.clone(),
            415,
            "unsupported-media-type",
            None,
        ),
        (
            "no media type",
            vec![
                session(&c.contributor),
                tip(),
                ("content-type", String::new()),
            ],
            ok.clone(),
            415,
            "unsupported-media-type",
            None,
        ),
    ] {
        let a = post(&c.w, &uri, &headers, body).await;
        assert_eq!(a.status, status, "{what}: {}", a.text());
        assert_eq!(problem_type(&a), urn(kind), "{what}");
        if let Some(member) = member {
            assert_eq!(a.json().at("/member"), member, "{what}");
        }
    }
    // A body with no member is `{}`, and nothing else, where an endpoint takes none.
    let p = open(&c, &c.contributor, false).await;
    let id = id_of(&p);
    let a = post(
        &c.w,
        &format!("{API}/proposals/{id}/submission"),
        &[session(&c.contributor), tag_header(&p)],
        r#"{"x":1}"#.to_owned(),
    )
    .await;
    assert_eq!(a.status, 400);
    assert_eq!(a.json().at("/member"), "x");
}

// ------------------------------------------------------------------------------------ not there, not yours

fn writes_for(id: &str) -> Vec<(String, String, String)> {
    let r = format!("{id}-r1");
    let tag = format!("\"{r}\"");
    let body = |v: Value| v.to_string();
    vec![
        (
            format!("{API}/proposals"),
            format!("\"{}\"", state_id(1)),
            body(json!({"summary": "x", "submit": true, "files": files_of_home(HOME)})),
        ),
        (
            format!("{API}/proposals/{id}/draft"),
            tag.clone(),
            body(json!({"summary": "x"})),
        ),
        (
            format!("{API}/proposals/{id}/submission"),
            tag.clone(),
            "{}".to_owned(),
        ),
        (
            format!("{API}/proposals/{id}/revisions"),
            tag.clone(),
            body(json!({"summary": "x"})),
        ),
        (
            format!("{API}/proposals/{id}/comments"),
            tag.clone(),
            body(json!({"body": "x"})),
        ),
        (
            format!("{API}/proposals/{id}/decisions"),
            tag.clone(),
            body(json!({"decision": "approve"})),
        ),
        (
            format!("{API}/proposals/{id}/acceptances"),
            tag.clone(),
            "{}".to_owned(),
        ),
        (
            format!("{API}/proposals/{id}/declines"),
            tag.clone(),
            body(json!({"reason": "x"})),
        ),
        (
            format!("{API}/proposals/{id}/withdrawals"),
            tag,
            body(json!({"reason": "x"})),
        ),
    ]
}

#[tokio::test]
async fn a_private_wikis_write_endpoints_answer_404_identically_whether_or_not_it_exists() {
    let w = with_accounts(World::with_visibility(
        "secret",
        "Secret Wiki",
        Visibility::Restricted,
        &[(
            "pages/Home.md",
            &page(&["title: Home"], "the hidden text\n"),
        )],
    ));
    let outsider = person(7);
    add_person(&w, &outsider, "Out Sider", None);
    let insider = person(8);
    add_person(&w, &insider, "In Sider", Some(Role::Admin));
    let id = ProposalId::new().to_string();
    let credentials: Vec<(&str, Vec<(&'static str, String)>)> = vec![
        ("an outsider's token", vec![token("admin", &outsider)]),
        ("an outsider's session", vec![session(&outsider)]),
        ("no credential", vec![]),
        (
            "a token that does not authenticate",
            vec![("authorization", "Bearer nope".to_owned())],
        ),
    ];
    for (who, creds) in credentials {
        for (uri, base, body) in writes_for(&id) {
            let mut headers = creds.clone();
            headers.push(("if-match", base.clone()));
            let real = post(
                &w,
                &uri.replace("handbook", "secret"),
                &headers,
                body.clone(),
            )
            .await;
            let absent = post(&w, &uri.replace("handbook", "no-such-wiki"), &headers, body).await;
            let norm = |a: &Answer, slug: &str| {
                let mut lines = a.header_lines();
                lines.retain(|l| !l.starts_with("content-length"));
                (a.status, lines, a.text().replace(slug, "X"))
            };
            assert_eq!(
                norm(&real, "secret"),
                norm(&absent, "no-such-wiki"),
                "{who} {uri}: a restricted wiki is told from an absent one"
            );
            if who == "a token that does not authenticate" {
                // That is about the request, and it is the same for every wiki.
                assert_eq!(real.status, 401, "{who} {uri}");
            } else {
                assert_eq!(real.status, 404, "{who} {uri}: {}", real.text());
                assert_eq!(problem_type(&real), urn("not-found"));
            }
            for secret in ["hidden text", "Secret Wiki"] {
                assert!(!real.text().contains(secret), "{who} {uri}: leaked");
            }
        }
    }
    // The wiki's own administrator reaches it: the endpoint is there for those who may read.
    let a = post(
        &w,
        &format!("{API}/proposals").replace("handbook", "secret"),
        &[token("admin", &insider), tip()],
        opening("x", "hidden text\n", false),
    )
    .await;
    assert_ne!(a.status, 404, "{}", a.text());
}

// ------------------------------------------------------------------------------------ what is data

#[tokio::test]
async fn multibyte_json_round_trips_exactly_in_the_records_and_in_the_answers() {
    let c = cast();
    let text = "---\ntitle: Home\n---\n\n日本語の段落。🙂 é ü — ½\n\n\u{1F9D1}\u{200D}\u{1F4BB} and \u{0}\u{1}\u{7f} controls\n";
    let summary = "要約 — summary 🙂";
    let a = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        // Written the way a client may: some characters as escapes, some raw.
        json!({"summary": summary, "sources": ["https://例え.jp/ページ"], "submit": true,
               "files": [{"path": "pages/Home.md", "text": text}]})
        .to_string()
        .replace('日', "\\u65e5"),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    let id = id_of(&a.json());
    assert_eq!(a.json().at("/summary"), summary);
    assert_eq!(a.json().at("/sources/0"), "https://例え.jp/ページ");
    let who = principal(&c.contributor);
    let stored =
        c.w.services
            .proposals
            .revisions(&who, &c.w.wiki, &id)
            .unwrap()
            .pop()
            .unwrap();
    assert_eq!(
        stored.content.files.first().unwrap().text.as_deref(),
        Some(text)
    );
    // Raw multibyte text is on the wire as it is, not as escapes.
    assert!(a.text().contains("要約"), "{}", a.text());

    let comment = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&a.json())],
        json!({"body": "賛成です 👍 — ok"}).to_string(),
    )
    .await;
    assert_eq!(comment.status, 201, "{}", comment.text());
    let comments =
        c.w.services
            .proposals
            .comments(&who, &c.w.wiki, &id)
            .unwrap();
    assert_eq!(comments.first().unwrap().body, "賛成です 👍 — ok");
    assert_eq!(comment.json().at("/body"), "賛成です 👍 — ok");
}

#[tokio::test]
async fn hostile_content_is_stored_as_text_and_written_with_markup_characters_escaped() {
    let c = cast();
    let hostile = "<script>alert(1)</script> & <img src=x onerror=alert(2)> \"quoted\" ]]> </body>";
    let text = format!("---\ntitle: Home\n---\n\n{hostile}\n\n[[Home|<b>x</b>]]\n");
    let a = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        json!({"summary": hostile.replace('\n', " "), "sources": [hostile], "submit": true,
               "files": [{"path": "pages/Home.md", "text": text}]})
        .to_string(),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    // The answer carries no raw markup character, and reads back to the same text.
    let raw = a.text();
    for ch in ['<', '>', '&'] {
        assert!(!raw.contains(ch), "a raw {ch:?} in {raw}");
    }
    assert!(raw.contains("\\u003cscript\\u003e"));
    assert_eq!(a.json().at("/summary"), hostile);
    assert_eq!(a.json().at("/sources/0"), hostile);
    let id = id_of(&a.json());
    let who = principal(&c.contributor);
    let stored =
        c.w.services
            .proposals
            .revisions(&who, &c.w.wiki, &id)
            .unwrap()
            .pop()
            .unwrap();
    assert_eq!(
        stored.content.files.first().unwrap().text.as_deref(),
        Some(text.as_str())
    );

    // So does a comment and a reason; none is interpreted.
    let comment = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&a.json())],
        json!({"body": hostile}).to_string(),
    )
    .await;
    assert_eq!(comment.status, 201, "{}", comment.text());
    for ch in ['<', '>', '&'] {
        assert!(!comment.text().contains(ch));
    }
    assert_eq!(
        c.w.services
            .proposals
            .comments(&who, &c.w.wiki, &id)
            .unwrap()
            .first()
            .unwrap()
            .body,
        hostile
    );
    // What a problem repeats of a name a client sent is escaped as well.
    let bad = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        json!({"summary": "x", "submit": false, "<img src=x onerror=alert(1)>&": 1}).to_string(),
    )
    .await;
    assert_eq!(bad.status, 400);
    assert!(bad.text().contains("\\u003cimg"), "{}", bad.text());
    for ch in ['<', '>', '&'] {
        assert!(!bad.text().contains(ch), "{}", bad.text());
    }
    // And nothing the page shows of it is a script: the proposal's page escapes it.
    let shown = get(&c.w, &format!("/handbook/-/proposals/{id}")).await;
    assert!(!shown.text().contains("<script>alert(1)"));
}

// ------------------------------------------------------------------------------------ the description

#[tokio::test]
async fn the_description_names_every_write_with_its_base_its_scope_and_the_problems_it_can_answer()
{
    let c = cast();
    let document = get(&c.w, "/-/api/v1/openapi.json").await.json();
    let writes = [
        ("/{slug}/-/api/v1/proposals", "propose", "NewProposal", 201),
        (
            "/{slug}/-/api/v1/proposals/{id}/draft",
            "propose",
            "ProposalEdit",
            200,
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/submission",
            "propose",
            "Empty",
            200,
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/revisions",
            "propose",
            "ProposalEdit",
            201,
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/comments",
            "propose",
            "NewComment",
            201,
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/decisions",
            "review",
            "NewDecision",
            201,
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/acceptances",
            "accept",
            "Empty",
            200,
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/declines",
            "accept",
            "Reason",
            200,
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/withdrawals",
            "propose",
            "Reason",
            200,
        ),
    ];
    for (path, scope, request, success) in writes {
        let key = path.replace('/', "~1");
        let op = document.at(&format!("/paths/{key}/post"));
        assert!(!op.is_null(), "{path} is not described");
        assert_eq!(op.at("/x-inklkunn-scope"), scope, "{path}");
        assert_eq!(
            op.at("/requestBody/content/application~1json/schema/$ref"),
            &json!(format!("#/components/schemas/{request}")),
            "{path}"
        );
        let names: Vec<&str> = op
            .at("/parameters")
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p.at("/in") == "header")
            .map(|p| p.at("/name").as_str().unwrap())
            .collect();
        assert_eq!(names, ["If-Match"], "{path}");
        let statuses: Vec<String> = op
            .at("/responses")
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let expected = std::iter::once(success.to_string())
            .chain(["401", "403", "404", "409", "413", "415", "428"].map(String::from));
        for status in expected {
            assert!(
                statuses.contains(&status),
                "{path} lacks {status}: {statuses:?}"
            );
        }
        assert!(
            !statuses.contains(&"304".to_owned()),
            "{path}: a write has no 304"
        );
        assert_eq!(op.at("/security/0/bearerAuth"), &json!([]));
    }
    // The registry of the types is the one a client matches on, and every code a write answers is in it.
    let listed: Vec<&str> = document
        .at("/x-inklkunn-problems")
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.at("/type").as_str().unwrap())
        .collect();
    for code in [
        "authentication-required",
        "invalid-token",
        "forbidden",
        "base-required",
        "stale-base",
        "invalid-body",
        "body-too-large",
        "unsupported-media-type",
        "unacceptable",
        "not-in-this-state",
        "already-decided",
        "refused-limit",
        "busy",
        "needs-recovery",
        "overlap",
        "nothing-to-change",
        "acceptance-in-progress",
        "acceptance-failed",
    ] {
        assert!(listed.contains(&urn(code).as_str()), "{code}");
    }
    // The shapes of the answers.
    for schema in ["Proposal", "Comment", "Decision", "NewProposal", "Reason"] {
        assert!(
            !document
                .at(&format!("/components/schemas/{schema}"))
                .is_null(),
            "{schema}"
        );
    }
    assert_eq!(
        document.at("/components/securitySchemes/bearerAuth/scheme"),
        "bearer"
    );
}

#[tokio::test]
async fn every_answer_of_the_write_api_is_of_the_shape_the_document_promises() {
    let c = cast();
    let document = get(&c.w, "/-/api/v1/openapi.json").await.json();
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    let comment = post(
        &c.w,
        &format!("{API}/proposals/{id}/comments"),
        &[token("propose", &c.other), tag_header(&p)],
        json!({"body": "x", "anchor": {"page": "pages/Home.md", "ordinal": 0}}).to_string(),
    )
    .await
    .json();
    let decision = post(
        &c.w,
        &format!("{API}/proposals/{id}/decisions"),
        &[token("review", &c.reviewer), tag_header(&p)],
        json!({"decision": "approve", "body": "ok"}).to_string(),
    )
    .await
    .json();
    for (schema, value) in [
        ("Proposal", &p),
        ("Comment", &comment),
        ("Decision", &decision),
    ] {
        let declared = document
            .at(&format!("/components/schemas/{schema}/properties"))
            .as_object()
            .unwrap();
        let required: Vec<&str> = document
            .at(&format!("/components/schemas/{schema}/required"))
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let object = value.as_object().unwrap();
        for name in object.keys() {
            assert!(
                declared.contains_key(name),
                "{schema}.{name} is not declared"
            );
        }
        for name in required {
            assert!(
                object.contains_key(name),
                "{schema}.{name} is not in the answer"
            );
        }
    }
}

#[tokio::test]
async fn a_write_endpoint_answers_a_method_it_does_not_take_with_a_problem_and_allow() {
    let c = cast();
    for uri in [
        format!("{API}/proposals/{}/comments", ProposalId::new()),
        format!("{API}/proposals/{}/acceptances", ProposalId::new()),
    ] {
        let a = super::send(&c.w, Method::GET, &uri, &[]).await;
        assert_eq!(a.status, 405, "{uri}");
        assert_eq!(problem_type(&a), urn("method-not-allowed"));
        assert_eq!(a.header("allow"), Some("POST"));
    }
    let a = super::send(&c.w, Method::DELETE, &format!("{API}/proposals"), &[]).await;
    assert_eq!(a.status, 405);
}
