//! The read API through the real router (RFC 003 D-13; handoff 003 T8): the pages it serves, the problems it
//! answers, `ETag` and `304`, cursors and bounds, existence neutrality, the language of the words and the
//! stability of the codes, and the description generated from the router. The repository is an in-memory
//! double, so these run in both test runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod across;
#[path = "../common/mod.rs"]
mod common;
mod description;
mod past;
mod shapes;
mod write;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use common::{World, busy, page};
use inklkunn_store::Visibility;
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site_with;
use serde_json::Value;
use tower::ServiceExt as _;

/// Look into a JSON value by pointer (`/items/0/title`; `~1` is a `/` in a key). Absent is `null`. Indexing
/// (`value["key"]`) panics on a shape nobody expected and is denied by the workspace's lints.
pub(crate) trait At {
    fn at(&self, pointer: &str) -> &Value;
}

static NULL: Value = Value::Null;

impl At for Value {
    fn at(&self, pointer: &str) -> &Value {
        self.pointer(pointer).unwrap_or(&NULL)
    }
}

/// Set `parent`'s member `key` to `value` (a `null` value removes it); the parent must be an object.
pub(crate) fn put(document: &mut Value, parent: &str, key: &str, value: Value) {
    if let Some(Value::Object(map)) = document.pointer_mut(parent) {
        if value.is_null() {
            map.remove(key);
        } else {
            map.insert(key.to_owned(), value);
        }
    }
}

pub(crate) fn router(w: &World) -> Router {
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

pub(crate) struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub bytes: Vec<u8>,
}

impl Answer {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.text()))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// The headers as sorted `name: value` lines, for comparing two answers whole.
    pub fn header_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {}", v.to_str().unwrap_or("?")))
            .collect();
        lines.sort();
        lines
    }
}

pub(crate) async fn send(w: &World, method: Method, uri: &str, extra: &[(&str, &str)]) -> Answer {
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
        bytes: bytes.to_vec(),
    }
}

pub(crate) async fn get(w: &World, uri: &str) -> Answer {
    send(w, Method::GET, uri, &[]).await
}

const JA: (&str, &str) = ("cookie", "inklkunn_lang=ja");

fn handbook() -> World {
    World::new(
        "handbook",
        "The Handbook",
        &[
            (
                "pages/Home.md",
                &page(
                    &["title: Home", "description: Start here", "tags: [start]"],
                    "# Welcome\n\nSee [[Guide]] and [[Nowhere]].\n",
                ),
            ),
            (
                "pages/Guide.md",
                &page(
                    &["title: Guide", "aliases: [How-to]", "license: CC0-1.0"],
                    "## Steps\n\nThe guide.\n",
                ),
            ),
            (
                "pages/Docs/Intro.md",
                &page(&["title: Docs/Intro"], "Introduction.\n"),
            ),
            (
                "pages/Docs/Deep/Guide.md",
                &page(&["title: Docs/Deep/Guide"], "Deep.\n"),
            ),
            (
                "pages/Old Name.md",
                &page(
                    &["title: Old Name", "kind: redirect", "redirect: Guide"],
                    "",
                ),
            ),
            (
                "pages/Dangling.md",
                &page(
                    &["title: Dangling", "kind: redirect", "redirect: Nowhere"],
                    "",
                ),
            ),
            (
                "pages/Broken.md",
                "---\ntitle: &anchor Broken\n---\n\nBody.\n",
            ),
            (
                "pages/Forged.md",
                &page(
                    &["title: Forged"],
                    "Go <a class=\"wikilink\" href=\"https://evil.example/\">Home</a> now.\n<script>alert(1)</script>\n",
                ),
            ),
        ],
    )
}

pub(crate) fn handbook_world() -> World {
    handbook()
}

fn problem_type(a: &Answer) -> String {
    a.json().at("/type").as_str().unwrap().to_owned()
}

/// Every API answer carries the link to the description and the page headers.
fn assert_api_headers(a: &Answer, what: &str) {
    assert_eq!(
        a.header("link"),
        Some(
            "</-/api/v1/openapi.json>; rel=\"service-desc\"; type=\"application/vnd.oai.openapi+json;version=3.1.0\""
        ),
        "{what}"
    );
    assert!(
        a.headers.contains_key(header::CONTENT_SECURITY_POLICY),
        "{what}"
    );
    assert_eq!(
        a.header("x-content-type-options"),
        Some("nosniff"),
        "{what}"
    );
    assert!(a.headers.contains_key(header::CONTENT_LANGUAGE), "{what}");
}

// ------------------------------------------------------------------------------------ the pages

#[tokio::test]
async fn a_list_is_walked_by_cursor_once_in_order_and_in_bounded_pages() {
    let w = handbook();
    let mut titles: Vec<String> = Vec::new();
    let mut uri = "/handbook/-/api/v1/pages?limit=3".to_owned();
    let mut walked = 0;
    loop {
        let a = get(&w, &uri).await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.text());
        assert_eq!(a.header("content-type"), Some("application/json"));
        assert_api_headers(&a, &uri);
        let body = a.json();
        let items = body.at("/items").as_array().unwrap();
        assert!(items.len() <= 3);
        titles.extend(
            items
                .iter()
                .map(|i| i.at("/title").as_str().unwrap().to_owned()),
        );
        assert!(body.at("/tip/block_id").is_string());
        assert_eq!(body.at("/tip/view"), "main");
        walked += 1;
        match body.at("/next_cursor").as_str() {
            Some(cursor) => uri = format!("/handbook/-/api/v1/pages?limit=3&cursor={cursor}"),
            None => break,
        }
        assert!(walked < 20);
    }
    assert_eq!(walked, 3);
    assert_eq!(
        titles,
        [
            "Broken",
            "Dangling",
            "Docs/Deep/Guide",
            "Docs/Intro",
            "Forged",
            "Guide",
            "Home",
            "Old Name"
        ]
        .map(String::from)
    );
}

#[tokio::test]
async fn a_list_carries_each_pages_kind_aliases_and_the_problem_of_an_unreadable_one() {
    let w = handbook();
    let a = get(&w, "/handbook/-/api/v1/pages").await;
    let body = a.json();
    let item = |title: &str| {
        body.at("/items")
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i.at("/title") == title)
            .unwrap_or_else(|| panic!("no {title} in {body}"))
            .clone()
    };
    assert_eq!(item("Guide").at("/aliases/0"), "How-to");
    assert_eq!(item("Old Name").at("/kind"), "redirect");
    assert_eq!(*item("Home").at("/problem"), Value::Null);
    assert!(item("Broken").at("/problem").is_string(), "{body}");
    assert_eq!(*body.at("/next_cursor"), Value::Null);
    // No page of the list leaks a storage path.
    assert!(!a.text().contains("pages/"), "{}", a.text());
}

#[tokio::test]
async fn the_filters_narrow_a_list_and_the_ones_not_offered_are_refused_not_ignored() {
    let w = handbook();
    let titles = |a: &Answer| -> Vec<String> {
        a.json()
            .at("/items")
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.at("/title").as_str().unwrap().to_owned())
            .collect()
    };
    let docs = get(&w, "/handbook/-/api/v1/pages?namespace=Docs").await;
    assert_eq!(titles(&docs), ["Docs/Deep/Guide", "Docs/Intro"]);
    let redirects = get(&w, "/handbook/-/api/v1/pages?kind=redirect").await;
    assert_eq!(titles(&redirects), ["Dangling", "Old Name"]);
    let encoded = get(&w, "/handbook/-/api/v1/pages?namespace=Docs%2FDeep").await;
    assert_eq!(titles(&encoded), ["Docs/Deep/Guide"]);
    for refused in [
        "/handbook/-/api/v1/pages?tag=start",
        "/handbook/-/api/v1/pages?changed_since=1700000000",
    ] {
        let a = get(&w, refused).await;
        assert_eq!(a.status, StatusCode::NOT_IMPLEMENTED, "{refused}");
        assert_eq!(problem_type(&a), "urn:inklkunn:problem:not-available");
    }
}

#[tokio::test]
async fn the_size_of_a_page_is_bounded_and_a_request_beyond_the_bound_is_refused() {
    let w = handbook();
    for (query, ok) in [
        ("limit=1", true),
        ("limit=100", true),
        ("limit=101", false),
        ("limit=0", false),
        ("limit=-1", false),
        ("limit=+5", false),
        ("limit=abc", false),
        ("limit=", false),
        ("limit=99999999999999999999", false),
    ] {
        let a = get(&w, &format!("/handbook/-/api/v1/pages?{query}")).await;
        if ok {
            assert_eq!(a.status, StatusCode::OK, "{query}");
        } else {
            assert_eq!(a.status, StatusCode::BAD_REQUEST, "{query}: {}", a.text());
            let body = a.json();
            assert_eq!(body.at("/type"), "urn:inklkunn:problem:invalid-parameter");
            assert_eq!(body.at("/parameter"), "limit");
        }
    }
}

#[tokio::test]
async fn an_unknown_repeated_or_malformed_parameter_is_refused_by_name() {
    let w = handbook();
    for (query, parameter) in [
        ("limt=5", "limt"),
        ("limit=5&limit=6", "limit"),
        ("view=main&view=main", "view"),
        ("%ff=1", "%ff"),
        ("cursor=%ff", "cursor"),
    ] {
        let a = get(&w, &format!("/handbook/-/api/v1/pages?{query}")).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{query}: {}", a.text());
        let body = a.json();
        assert_eq!(
            body.at("/type"),
            "urn:inklkunn:problem:invalid-parameter",
            "{query}"
        );
        assert_eq!(body.at("/parameter"), parameter, "{query}");
    }
    let long = "x".repeat(300);
    let a = get(&w, &format!("/handbook/-/api/v1/pages?{long}=1")).await;
    assert_eq!(a.json().at("/parameter").as_str().unwrap().len(), 64);
}

#[tokio::test]
async fn a_cursor_this_server_did_not_give_is_refused() {
    let w = handbook();
    for cursor in ["abc", "c1zz", "c2706167", "c1686f6d65", "c1ff"] {
        let a = get(&w, &format!("/handbook/-/api/v1/pages?cursor={cursor}")).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{cursor}");
        assert_eq!(
            problem_type(&a),
            "urn:inklkunn:problem:invalid-cursor",
            "{cursor}"
        );
    }
}

#[tokio::test]
async fn a_page_is_metadata_and_a_sanitized_fragment() {
    let w = handbook();
    let a = get(&w, "/handbook/-/api/v1/pages/Home").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    assert_api_headers(&a, "page");
    let body = a.json();
    assert_eq!(body.at("/title"), "Home");
    assert_eq!(body.at("/description"), "Start here");
    assert_eq!(body.at("/tags/0"), "start");
    assert_eq!(body.at("/kind"), "page");
    assert_eq!(body.at("/matched_by"), "exact");
    assert_eq!(body.at("/stale"), false);
    assert_eq!(*body.at("/redirected_from"), Value::Null);
    // The wiki's default license when the page names none.
    assert_eq!(body.at("/license"), "CC-BY-SA-4.0");
    let html = body.at("/html").as_str().unwrap();
    assert!(html.contains("<h2 id=\"h-welcome\">Welcome</h2>"), "{html}");
    assert!(html.contains("href=\"/handbook/Guide\""), "{html}");
    assert_eq!(body.at("/toc/0/text"), "Welcome");
    assert_eq!(body.at("/toc/0/level"), 2);
    assert!(body.at("/tip/block_id").is_string());
    // The page's license wins over the wiki's.
    let guide = get(&w, "/handbook/-/api/v1/pages/Guide").await.json();
    assert_eq!(guide.at("/license"), "CC0-1.0");
}

#[tokio::test]
async fn a_fragment_never_carries_a_script_an_event_handler_or_a_forged_link() {
    let w = handbook();
    let a = get(&w, "/handbook/-/api/v1/pages/Forged").await;
    assert_eq!(a.status, StatusCode::OK);
    let html = a.json().at("/html").as_str().unwrap().to_owned();
    assert!(!html.contains("<script"), "{html}");
    assert!(
        !html.contains("evil.example\""),
        "no live forged link: {html}"
    );
    let fragment = get(&w, "/handbook/-/api/v1/fragment/Forged").await;
    assert_eq!(
        fragment.text(),
        html,
        "the two endpoints give the same HTML"
    );
}

#[tokio::test]
async fn the_fragment_endpoint_answers_html_and_nothing_around_it() {
    let w = handbook();
    let a = get(&w, "/handbook/-/api/v1/fragment/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.header("content-type"), Some("text/html; charset=utf-8"));
    assert_api_headers(&a, "fragment");
    let text = a.text();
    assert!(text.contains("<h2 id=\"h-welcome\">Welcome</h2>"), "{text}");
    for wrapper in ["<html", "<body", "<head", "<!DOCTYPE", "<!doctype"] {
        assert!(!text.contains(wrapper), "{wrapper}: {text}");
    }
    // The page policy still forbids anything to run.
    assert!(
        a.header("content-security-policy")
            .unwrap()
            .contains("default-src 'none'")
    );
}

#[tokio::test]
async fn an_alias_and_a_redirect_lead_to_their_page_and_say_so() {
    let w = handbook();
    let alias = get(&w, "/handbook/-/api/v1/pages/How-to").await.json();
    assert_eq!(alias.at("/title"), "Guide");
    assert_eq!(alias.at("/redirected_from/title"), "How-to");
    assert_eq!(alias.at("/redirected_from/by"), "alias");
    let redirect = get(&w, "/handbook/-/api/v1/pages/Old%20Name").await.json();
    assert_eq!(redirect.at("/title"), "Guide");
    assert_eq!(redirect.at("/redirected_from/by"), "redirect");
}

#[tokio::test]
async fn the_raw_endpoint_says_where_the_source_is_and_never_carries_it() {
    let w = handbook();
    let a = get(&w, "/handbook/-/api/v1/raw/Guide").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    let body = a.json();
    assert_eq!(body.at("/title"), "Guide");
    assert_eq!(body.at("/kind"), "page");
    assert_eq!(body.at("/storage_path"), "pages/Guide.md");
    assert!(body.at("/tip/block_id").is_string());
    assert!(
        !a.text().contains("The guide."),
        "the source is not served here"
    );
    let nested = get(&w, "/handbook/-/api/v1/raw/Docs/Deep/Guide")
        .await
        .json();
    assert_eq!(nested.at("/storage_path"), "pages/Docs/Deep/Guide.md");
    // A page whose frontmatter cannot be read still has a place its source can be found.
    let broken = get(&w, "/handbook/-/api/v1/raw/Broken").await;
    assert_eq!(broken.status, StatusCode::OK, "{}", broken.text());
}

#[tokio::test]
async fn a_title_that_names_no_page_is_a_problem_that_says_which() {
    let w = handbook();
    let cases = [
        ("pages/Nowhere", StatusCode::NOT_FOUND, "page-not-found"),
        ("pages/Dangling", StatusCode::NOT_FOUND, "redirect-broken"),
        (
            "pages/Broken",
            StatusCode::UNPROCESSABLE_ENTITY,
            "page-unreadable",
        ),
        ("pages/..", StatusCode::BAD_REQUEST, "invalid-title"),
        ("pages/%ff", StatusCode::BAD_REQUEST, "invalid-title"),
        ("raw/Nowhere", StatusCode::NOT_FOUND, "page-not-found"),
        ("raw/Dangling", StatusCode::NOT_FOUND, "redirect-broken"),
        ("fragment/Nowhere", StatusCode::NOT_FOUND, "page-not-found"),
        (
            "pages/Home?view=nope",
            StatusCode::NOT_FOUND,
            "no-such-view",
        ),
        ("pages/Home?view=a/b", StatusCode::NOT_FOUND, "no-such-view"),
    ];
    for (tail, status, code) in cases {
        let a = get(&w, &format!("/handbook/-/api/v1/{tail}")).await;
        assert_eq!(a.status, status, "{tail}: {}", a.text());
        assert_eq!(
            a.header("content-type"),
            Some("application/problem+json"),
            "{tail}"
        );
        assert_eq!(
            problem_type(&a),
            format!("urn:inklkunn:problem:{code}"),
            "{tail}"
        );
        assert_api_headers(&a, tail);
        let body = a.json();
        assert_eq!(body.at("/status"), status.as_u16());
        for member in ["title", "detail", "hint"] {
            assert!(
                !body.at(&format!("/{member}")).as_str().unwrap().is_empty(),
                "{tail} {member}"
            );
        }
    }
}

#[tokio::test]
async fn several_matches_are_listed_as_candidates() {
    let w = World::new(
        "twins",
        "Twins",
        &[
            ("pages/A/Same.md", &page(&["title: A/Same"], "x\n")),
            ("pages/B/Same.md", &page(&["title: B/Same"], "x\n")),
        ],
    );
    let a = get(&w, "/twins/-/api/v1/pages/Same").await;
    assert_eq!(a.status, StatusCode::MULTIPLE_CHOICES, "{}", a.text());
    let body = a.json();
    assert_eq!(body.at("/type"), "urn:inklkunn:problem:ambiguous-title");
    assert_eq!(
        *body.at("/candidates"),
        serde_json::json!(["A/Same", "B/Same"])
    );
}

// ------------------------------------------------------------------------------------ validators

#[tokio::test]
async fn a_conditional_request_for_an_unchanged_answer_is_304_with_no_body() {
    let w = handbook();
    for uri in [
        "/handbook/-/api/v1/pages",
        "/handbook/-/api/v1/pages/Home",
        "/handbook/-/api/v1/raw/Home",
        "/handbook/-/api/v1/fragment/Home",
        "/-/api/v1/openapi.json",
    ] {
        let first = get(&w, uri).await;
        assert_eq!(first.status, StatusCode::OK, "{uri}");
        let etag = first
            .header("etag")
            .unwrap_or_else(|| panic!("{uri}: no ETag"))
            .to_owned();
        assert!(etag.starts_with('"') && etag.ends_with('"'), "{etag}");
        for presented in [
            etag.clone(),
            format!("W/{etag}"),
            format!("\"nope\", {etag}"),
            "*".to_owned(),
        ] {
            let again = send(&w, Method::GET, uri, &[("if-none-match", &presented)]).await;
            assert_eq!(
                again.status,
                StatusCode::NOT_MODIFIED,
                "{uri} with {presented}"
            );
            assert!(again.bytes.is_empty(), "{uri}");
            assert_eq!(again.header("etag"), Some(etag.as_str()), "{uri}");
            assert_api_headers(&again, uri);
        }
        let other = send(&w, Method::GET, uri, &[("if-none-match", "\"nope\"")]).await;
        assert_eq!(other.status, StatusCode::OK, "{uri}");
        assert_eq!(other.bytes, first.bytes);
    }
}

#[tokio::test]
async fn a_changed_answer_has_a_new_validator_and_the_old_one_no_longer_matches() {
    let w = handbook();
    let uri = "/handbook/-/api/v1/pages/Home";
    let before = get(&w, uri).await;
    let old = before.header("etag").unwrap().to_owned();
    w.fake.set(
        "pages/Home.md",
        &page(&["title: Home"], "# Welcome\n\nChanged.\n"),
    );
    let after = send(&w, Method::GET, uri, &[("if-none-match", &old)]).await;
    assert_eq!(after.status, StatusCode::OK);
    assert_ne!(after.header("etag").unwrap(), old);
    assert!(
        after
            .json()
            .at("/html")
            .as_str()
            .unwrap()
            .contains("Changed.")
    );
}

#[tokio::test]
async fn the_validator_depends_on_the_language_of_the_words() {
    let w = handbook();
    let en = get(&w, "/handbook/-/api/v1/pages/Nowhere").await;
    let ja = send(&w, Method::GET, "/handbook/-/api/v1/pages/Nowhere", &[JA]).await;
    // A problem has no validator (it is not a cacheable success), so the words cannot be served from a
    // cached English answer to a Japanese reader.
    assert!(en.header("etag").is_none() && ja.header("etag").is_none());
    let list_en = get(&w, "/handbook/-/api/v1/pages/Home").await;
    let list_ja = send(&w, Method::GET, "/handbook/-/api/v1/pages/Home", &[JA]).await;
    assert_eq!(list_en.status, StatusCode::OK);
    assert_eq!(list_ja.status, StatusCode::OK);
    assert!(list_en.header("vary").unwrap().contains("Accept-Language"));
}

#[tokio::test]
async fn head_is_answered_like_get_without_a_body() {
    let w = handbook();
    let get_it = get(&w, "/handbook/-/api/v1/pages").await;
    let head = send(&w, Method::HEAD, "/handbook/-/api/v1/pages", &[]).await;
    assert_eq!(head.status, StatusCode::OK);
    assert!(head.bytes.is_empty());
    assert_eq!(head.header("etag"), get_it.header("etag"));
    assert_eq!(head.header("content-type"), get_it.header("content-type"));
}

// ------------------------------------------------------------------------------------ existence

/// Every address of the API a reader of a wiki can ask, with its queries and bad inputs.
fn every_address(slug: &str) -> Vec<String> {
    [
        "pages",
        "pages?limit=2",
        "pages?limit=1000",
        "pages?bogus=1",
        "pages?cursor=abc",
        "pages?tag=x",
        "pages/Home",
        "pages/Nowhere",
        "pages/..",
        "pages/Home?view=nope",
        "raw/Guide",
        "raw/Nowhere",
        "fragment/Home",
        "fragment/Nowhere",
        "history/Home",
        "history/Home?cursor=n13",
        "history/Home?cursor=bogus",
        "history/Nowhere",
        "revisions/abc123",
        "revisions/NOT-HEX",
        "revisions/abc123?title=Home",
        "diff",
        "diff?path=Home",
        "diff?path=Home&from=abc&to=def",
        "diff?bogus=1",
        "backlinks/Home",
        "backlinks/Nowhere",
        "backlinks/Home?limit=1",
        "health",
        "health?list=orphans",
        "health?list=stale",
        "health?list=nope",
        "search?q=x",
        "search?q=x&limit=1",
        "search",
        "changes",
        "changes?automated=hide",
        "changes?view=nope",
        "changes?cursor=k10123456789abcdef",
        "nonsense",
        "pages/Home/..%2f..",
    ]
    .iter()
    .map(|tail| format!("/{slug}/-/api/v1/{tail}"))
    .collect()
}

#[tokio::test]
async fn a_wiki_you_may_not_read_answers_every_endpoint_exactly_as_a_wiki_that_is_not_there() {
    let secret = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &page(&["title: Home"], "Hidden.\n"))],
    );
    let elsewhere = handbook();
    // The same addresses against a wiki that exists and may not be read, and one that does not exist.
    for (address, other) in every_address("secret")
        .into_iter()
        .zip(every_address("never-created"))
    {
        for extra in [&[][..], &[JA][..], &[("if-none-match", "*")][..]] {
            let hidden = send(&secret, Method::GET, &address, extra).await;
            let absent = send(&elsewhere, Method::GET, &other, extra).await;
            assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{address}");
            assert_eq!(hidden.status, absent.status, "{address}");
            assert_eq!(hidden.bytes, absent.bytes, "{address}: the bodies differ");
            assert_eq!(
                hidden.header_lines(),
                absent.header_lines(),
                "{address}: the headers differ"
            );
            assert_eq!(problem_type(&hidden), "urn:inklkunn:problem:not-found");
        }
    }
}

#[tokio::test]
async fn nothing_of_a_hidden_wiki_is_in_its_answer_not_its_name_its_pages_or_its_validators() {
    let secret = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &page(&["title: Home"], "Hidden.\n"))],
    );
    for address in every_address("secret") {
        let a = get(&secret, &address).await;
        let text = a.text();
        assert!(
            !text.contains("secret") && !text.contains("Secret"),
            "{address}"
        );
        assert!(!text.contains("Hidden"), "{address}");
        assert!(a.header("etag").is_none(), "{address}");
    }
    // A validator from a wiki you may read does not unlock one you may not.
    let public = handbook();
    let etag = get(&public, "/handbook/-/api/v1/pages").await;
    let etag = etag.header("etag").unwrap().to_owned();
    let a = send(
        &secret,
        Method::GET,
        "/secret/-/api/v1/pages",
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_bad_parameter_to_a_hidden_wiki_is_not_found_not_bad_request() {
    let secret = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &page(&["title: Home"], "Hidden.\n"))],
    );
    // The check of the query comes after the right to read, so a refusal of a parameter tells a stranger
    // nothing about the wiki.
    for address in [
        "/secret/-/api/v1/pages?limit=1000",
        "/secret/-/api/v1/diff",
        "/secret/-/api/v1/search",
    ] {
        let a = get(&secret, address).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{address}");
    }
}

// ------------------------------------------------------------------------------------ language

#[tokio::test]
async fn the_code_is_the_same_in_every_language_and_the_words_are_not() {
    let w = handbook();
    for address in [
        "/handbook/-/api/v1/pages/Nowhere",
        "/handbook/-/api/v1/pages?limit=0",
        "/handbook/-/api/v1/pages?cursor=zz",
        "/handbook/-/api/v1/revisions/abc",
        "/handbook/-/api/v1/diff",
        "/handbook/-/api/v1/pages/Home?view=nope",
        "/nobody/-/api/v1/pages",
    ] {
        let en = get(&w, address).await;
        let ja = send(&w, Method::GET, address, &[JA]).await;
        assert_eq!(en.status, ja.status, "{address}");
        let (en_body, ja_body) = (en.json(), ja.json());
        assert_eq!(en_body.at("/type"), ja_body.at("/type"), "{address}");
        assert_eq!(en_body.at("/status"), ja_body.at("/status"), "{address}");
        for member in ["title", "detail", "hint"] {
            assert_ne!(
                en_body.at(&format!("/{member}")),
                ja_body.at(&format!("/{member}")),
                "{address} {member}"
            );
        }
        assert_eq!(en.header("content-language"), Some("en"));
        assert_eq!(ja.header("content-language"), Some("ja"));
        assert_eq!(
            en.header("content-type"),
            Some("application/problem+json"),
            "{address}"
        );
    }
}

#[tokio::test]
async fn accept_language_alone_never_selects_the_preview_language_as_for_pages() {
    let w = handbook();
    let a = send(
        &w,
        Method::GET,
        "/handbook/-/api/v1/pages/Nowhere",
        &[("accept-language", "ja")],
    )
    .await;
    assert_eq!(a.header("content-language"), Some("en"));
    let b = send(
        &w,
        Method::GET,
        "/handbook/-/api/v1/pages/Nowhere",
        &[("accept-language", "fr;q=0.9, en;q=0.5")],
    )
    .await;
    assert_eq!(b.header("content-language"), Some("en"));
}

// ------------------------------------------------------------------------------------ the diff

#[tokio::test]
async fn a_diff_needs_a_path_there_is_no_diff_of_a_whole_tree() {
    let w = handbook();
    for address in [
        "/handbook/-/api/v1/diff",
        "/handbook/-/api/v1/diff?path=",
        "/handbook/-/api/v1/diff?from=abc&to=def",
    ] {
        let a = get(&w, address).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{address}");
        assert_eq!(
            problem_type(&a),
            "urn:inklkunn:problem:diff-path-required",
            "{address}"
        );
    }
    // With the path it is a diff of that page: this wiki has one change, and nothing before it to compare to.
    let given = get(&w, "/handbook/-/api/v1/diff?path=Home").await;
    assert_eq!(given.status, StatusCode::NOT_FOUND);
    assert_eq!(
        problem_type(&given),
        "urn:inklkunn:problem:change-not-found"
    );
    // The same parameter may not be given twice, and none other is known.
    let twice = get(&w, "/handbook/-/api/v1/diff?path=a&path=b").await;
    assert_eq!(
        problem_type(&twice),
        "urn:inklkunn:problem:invalid-parameter"
    );
    let tree = get(&w, "/handbook/-/api/v1/diff?tree=1").await;
    assert_eq!(
        problem_type(&tree),
        "urn:inklkunn:problem:invalid-parameter"
    );
}

// ------------------------------------------------------------------------------------ addresses

#[tokio::test]
async fn an_address_under_the_api_that_is_not_an_endpoint_is_a_problem_never_a_page() {
    let w = handbook();
    for address in [
        "/handbook/-/api/v1/nonsense",
        "/handbook/-/api/v1/nonsense/more",
        "/handbook/-/api/v2/pages",
        "/handbook/-/api/",
        "/-/api/v1/pages",
        "/-/api/v1/nonsense",
        "/-/api/v7/openapi.json",
    ] {
        let a = get(&w, address).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{address}");
        assert_eq!(
            a.header("content-type"),
            Some("application/problem+json"),
            "{address}: {}",
            a.text()
        );
        assert_eq!(
            problem_type(&a),
            "urn:inklkunn:problem:not-found",
            "{address}"
        );
    }
    // The page titled like an API address is not shadowed by anything else: a title never starts with `-/`.
    let page = get(&w, "/handbook/Home").await;
    assert!(
        page.header("content-type")
            .unwrap()
            .starts_with("text/html")
    );
}

#[tokio::test]
async fn a_method_the_api_does_not_take_is_a_problem_in_the_callers_language() {
    let w = handbook();
    for address in [
        "/handbook/-/api/v1/pages",
        "/handbook/-/api/v1/pages/Home",
        "/-/api/v1/openapi.json",
        "/handbook/-/api/v1/unknown",
    ] {
        for method in [Method::POST, Method::PUT, Method::DELETE, Method::PATCH] {
            let a = send(&w, method.clone(), address, &[JA]).await;
            assert_eq!(
                a.status,
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} {address}"
            );
            assert_eq!(
                a.header("content-type"),
                Some("application/problem+json"),
                "{method} {address}"
            );
            assert_eq!(problem_type(&a), "urn:inklkunn:problem:method-not-allowed");
            assert_eq!(a.header("content-language"), Some("ja"));
            assert_api_headers(&a, address);
        }
    }
}

// ------------------------------------------------------------------------------------ busy

#[tokio::test]
async fn when_prikk_is_busy_the_api_serves_the_last_copy_labelled_or_says_it_cannot_read_now() {
    let w = handbook();
    w.fake.fail_views(Some(busy()));
    let none = get(&w, "/handbook/-/api/v1/pages/Home").await;
    assert_eq!(none.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(none.header("retry-after"), Some("30"));
    assert_eq!(problem_type(&none), "urn:inklkunn:problem:unavailable");
    assert!(
        !none.text().contains("Welcome"),
        "never rendered from nothing"
    );
    assert!(
        w.logged().iter().any(|l| l.contains("handbook")),
        "the operator is told: {:?}",
        w.logged()
    );
    let list = get(&w, "/handbook/-/api/v1/pages").await;
    assert_eq!(list.status, StatusCode::SERVICE_UNAVAILABLE);

    w.fake.fail_views(None);
    let fresh = get(&w, "/handbook/-/api/v1/pages/Home").await;
    assert_eq!(fresh.json().at("/stale"), false);
    w.fake.fail_views(Some(busy()));
    let stale = get(&w, "/handbook/-/api/v1/pages/Home").await;
    assert_eq!(stale.status, StatusCode::OK);
    let body = stale.json();
    assert_eq!(body.at("/stale"), true);
    assert_eq!(*body.at("/tip/block_id"), Value::Null);
    assert!(body.at("/html").as_str().unwrap().contains("Welcome"));
    // A page never cached is not made up.
    let other = get(&w, "/handbook/-/api/v1/pages/Guide").await;
    assert_eq!(other.status, StatusCode::SERVICE_UNAVAILABLE);
}
