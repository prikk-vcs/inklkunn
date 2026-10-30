#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use inklkunn_core::Guard;
use tower::ServiceExt as _;

use super::*;

fn config() -> WebConfig {
    WebConfig::new(
        "127.0.0.1:8080",
        "http://localhost:8080",
        "http://localhost:8081",
    )
    .unwrap()
}

async fn get_at(host: &str, path: &str) -> axum::response::Response {
    site(&config())
        .unwrap()
        .oneshot(
            Request::get(path)
                .header("host", host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[test]
fn every_entry_is_guarded_or_public_with_a_reason() {
    let table = application_table().unwrap();
    assert!(!table.entries().is_empty());
    for e in table.entries() {
        match e.guard {
            Guard::Action(_) => {}
            Guard::Public { reason } => assert!(reason.len() >= 16, "{}: {reason}", e.path),
        }
    }
}

/// The URL scheme's one rule (RFC 003 D-1): a path is `/`, `/-/…`, or `/{slug}/-/…` or `/{slug}/…` with a
/// parameter standing for a title. No other fixed word may stand where a slug or a title could, so no title
/// is ever shadowed.
const ROOT_FILES: &[&str] = &["robots.txt"];

fn path_is_reserved_or_parametric(path: &str) -> bool {
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match segments.as_slice() {
        [""] => true,
        ["-", ..] => true,
        [first, "-", ..] if first.starts_with('{') => true,
        // The files a crawler asks for at the root. No wiki can be named like one: a slug is lower-case
        // letters, digits and hyphens (`root_files_cannot_be_wiki_slugs`).
        [file] if ROOT_FILES.contains(file) => true,
        [first, rest @ ..] if first.starts_with('{') => rest.iter().all(|s| s.starts_with('{')),
        _ => false,
    }
}

#[test]
fn no_declared_route_can_shadow_a_title() {
    for e in application_table().unwrap().entries() {
        assert!(
            path_is_reserved_or_parametric(e.path),
            "{} would shadow a title",
            e.path
        );
    }
}

#[test]
fn root_files_cannot_be_wiki_slugs() {
    for file in ROOT_FILES {
        assert!(
            inklkunn_store::Slug::new(file).is_err(),
            "a wiki could be named {file}, which the root file would shadow"
        );
    }
}

#[test]
fn the_shadowing_check_catches_its_planted_violations() {
    for ok in [
        "/",
        "/-/health",
        "/{slug}/-/history/{*title}",
        "/{slug}/{*title}",
        "/-/wikis",
        "/robots.txt",
    ] {
        assert!(path_is_reserved_or_parametric(ok), "{ok}");
    }
    for planted in [
        "/about",
        "/health",
        "/{slug}/about",
        "/{slug}/history/{*title}",
        "/static/site.css",
        "/api/v1",
        "/sitemap.xml",
    ] {
        assert!(
            !path_is_reserved_or_parametric(planted),
            "{planted} must fail the check"
        );
    }
}

#[tokio::test]
async fn health_answers_on_the_application_origin_only() {
    let res = get_at("localhost:8080", "/-/health").await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = get_at("localhost:8081", "/-/health").await;
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "the content origin answers no application route"
    );
}

#[tokio::test]
async fn the_content_origin_is_inert_and_the_application_origin_serves_no_raw_bytes() {
    // No attachment exists yet; both ways the answer is a 404 with the origin's own headers.
    let raw = "/handbook/-/raw/Home";
    let on_content = get_at("localhost:8081", raw).await;
    assert_eq!(on_content.status(), StatusCode::NOT_FOUND);
    assert!(
        on_content.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    let on_app = get_at("localhost:8080", raw).await;
    assert_eq!(on_app.status(), StatusCode::NOT_FOUND);
    assert!(
        !on_app.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
}

#[tokio::test]
async fn a_host_that_names_neither_origin_is_misdirected() {
    for host in [
        "evil.example",
        "localhost",
        "localhost:9999",
        "localhost:8080.evil.example",
        "",
    ] {
        let res = get_at(host, "/-/health").await;
        assert_eq!(res.status(), StatusCode::MISDIRECTED_REQUEST, "{host:?}");
        assert!(
            res.headers().contains_key("content-security-policy"),
            "{host:?}"
        );
    }
    // No Host header at all.
    let res = site(&config())
        .unwrap()
        .oneshot(Request::get("/-/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::MISDIRECTED_REQUEST);
}

#[tokio::test]
async fn every_class_of_response_carries_the_headers() {
    let s = site(&config()).unwrap();
    let cases = [
        ("localhost:8080", "GET", "/-/health", StatusCode::OK),
        (
            "localhost:8080",
            "GET",
            "/nothing/here",
            StatusCode::NOT_FOUND,
        ),
        (
            "localhost:8080",
            "POST",
            "/-/health",
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        ("localhost:8081", "GET", "/anything", StatusCode::NOT_FOUND),
        (
            "elsewhere.example",
            "GET",
            "/-/health",
            StatusCode::MISDIRECTED_REQUEST,
        ),
    ];
    for (host, method, path, expected) in cases {
        let res = s
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), expected, "{method} {host}{path}");
        for name in [
            "content-security-policy",
            "x-content-type-options",
            "referrer-policy",
            "permissions-policy",
            "x-frame-options",
            "cross-origin-opener-policy",
            "cache-control",
            "vary",
        ] {
            assert!(
                res.headers().contains_key(name),
                "{method} {host}{path}: no {name}"
            );
        }
        assert_eq!(res.headers()["x-content-type-options"], "nosniff");
    }
}

// ---------------------------------------------------------------- handoff 003 T2: the words and the layout

/// A header of an answer, as text.
fn h<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .unwrap_or_else(|| panic!("no {name} header"))
        .to_str()
        .unwrap()
}

async fn send(
    host: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("host", host);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let res = site(&config())
        .unwrap()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 20).await.unwrap();
    (
        parts.status,
        parts.headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

#[test]
fn the_switcher_and_the_stylesheet_are_public_routes_with_a_reason_under_the_reserved_prefix() {
    let table = application_table().unwrap();
    for (path, methods) in [
        ("/-/lang", &["GET", "HEAD", "POST"][..]),
        ("/-/assets/{file}", &["GET", "HEAD"][..]),
    ] {
        let e = table
            .find(path)
            .unwrap_or_else(|| panic!("{path} is not declared"));
        assert_eq!(e.methods, methods, "{path}");
        match e.guard {
            Guard::Public { reason } => assert!(reason.len() >= 16, "{path}: {reason}"),
            Guard::Action(_) => panic!("{path}: the switcher and the sheet serve no wiki data"),
        }
    }
}

#[test]
fn every_route_that_can_show_wiki_data_is_guarded_by_read_and_the_rest_are_public() {
    let table = application_table().unwrap();
    // The read API's endpoints are held to the same rule by `tests/api` (description.rs); this list is the pages'.
    let guarded: Vec<&str> = table
        .guarded()
        .map(|(e, _)| e.path)
        .filter(|p| !p.contains("/-/api/"))
        .collect();
    assert_eq!(
        guarded,
        [
            "/-/wikis",
            "/-/inbox",
            "/{slug}/-/watch/{*title}",
            "/{slug}",
            "/{slug}/-/about",
            "/{slug}/-/verification",
            "/{slug}/-/history/{*title}",
            "/{slug}/-/change/{block}",
            "/{slug}/-/diff/{from}/{to}/{*title}",
            "/{slug}/-/at/{block}/{*title}",
            "/{slug}/-/changes",
            "/{slug}/-/changes.atom",
            "/{slug}/-/search",
            "/{slug}/-/opensearch.xml",
            "/{slug}/-/health",
            "/{slug}/-/health/{list}",
            "/{slug}/-/links/{*title}",
            "/{slug}/-/site.json",
            "/{slug}/-/describe/{*title}",
            "/{slug}/-/sitemap.xml",
            "/{slug}/{*title}"
        ]
    );
    for (e, action) in table.guarded() {
        assert_eq!(action, inklkunn_core::Action::Read, "{}", e.path);
    }
    let public: Vec<&str> = table
        .entries()
        .iter()
        .filter(|e| matches!(e.guard, Guard::Public { .. }))
        .map(|e| e.path)
        .filter(|p| !p.contains("/-/api/"))
        .collect();
    assert_eq!(
        public,
        [
            "/-/health",
            "/-/lang",
            "/-/assets/{file}",
            "/robots.txt",
            "/{slug}/-/raw/{*title}"
        ]
    );
}

#[tokio::test]
async fn an_unknown_address_is_a_page_in_the_visitors_words_with_every_header() {
    // Japanese is a preview: it is the reader's choice (the cookie), never the browser's setting.
    for (header, value, lang, heading) in [
        ("accept-language", "en", "en", "<h1>Not found</h1>"),
        ("accept-language", "ja", "en", "<h1>Not found</h1>"),
        (
            "cookie",
            "inklkunn_lang=ja",
            "ja",
            "<h1>見つかりません</h1>",
        ),
    ] {
        let (status, headers, body) = send(
            "localhost:8080",
            "GET",
            "/nothing/here?x=1",
            &[(header, value)],
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains(heading), "{body}");
        assert!(
            body.contains(&format!("<html lang=\"{lang}\" dir=\"ltr\">")),
            "{body}"
        );
        assert_eq!(h(&headers, "content-type"), "text/html; charset=utf-8");
        assert_eq!(h(&headers, "content-language"), lang);
        for name in [
            "content-security-policy",
            "x-content-type-options",
            "referrer-policy",
            "permissions-policy",
            "cache-control",
            "vary",
        ] {
            assert!(headers.contains_key(name), "{name}");
        }
        assert_eq!(h(&headers, "cache-control"), "private, no-cache");
        assert!(h(&headers, "vary").contains("Accept-Language"));
        // The switcher returns to the address that was asked for.
        assert!(
            body.contains("name=\"next\" value=\"/nothing/here?x=1\""),
            "{body}"
        );
    }
}

#[tokio::test]
async fn the_stylesheet_is_cached_for_a_year_under_a_name_that_changes_with_its_bytes() {
    let (status, headers, body) = send("localhost:8080", "GET", crate::style::path(), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, crate::style::CSS);
    assert_eq!(h(&headers, "content-type"), "text/css; charset=utf-8");
    assert_eq!(
        h(&headers, "cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        h(&headers, "vary"),
        "Accept-Encoding",
        "the sheet does not depend on a cookie or a language"
    );
    // The handler cannot loosen the page policy: the security headers are still the layer's.
    assert_eq!(h(&headers, "x-content-type-options"), "nosniff");
    assert!(headers.contains_key("content-security-policy"));
    // HEAD is answered too; another name is a 404 page, not the sheet.
    let (status, _, _) = send("localhost:8080", "HEAD", crate::style::path(), &[]).await;
    assert_eq!(status, StatusCode::OK);
    let (status, headers, body) = send(
        "localhost:8080",
        "GET",
        "/-/assets/site-0000000000000000.css",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("<h1>Not found</h1>"), "{body}");
    assert_ne!(
        h(&headers, "cache-control"),
        "public, max-age=31536000, immutable"
    );
    // …and never on the content origin.
    let (status, _, body) = send("localhost:8081", "GET", crate::style::path(), &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("body {"), "{body}");
}

#[tokio::test]
async fn the_content_origin_answers_in_plain_text_in_the_visitors_language() {
    for (header, value, want) in [
        (
            "accept-language",
            "en",
            "There is nothing to show at this address. Check the address for typing mistakes.",
        ),
        (
            "cookie",
            "inklkunn_lang=ja",
            "このアドレスには表示できるものがありません。アドレスに入力ミスがないか確認してください。",
        ),
    ] {
        let (status, headers, body) =
            send("localhost:8081", "GET", "/anything", &[(header, value)]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, want);
        assert_eq!(h(&headers, "content-type"), "text/plain; charset=utf-8");
        assert!(h(&headers, "content-security-policy").contains("sandbox"));
        assert!(!body.contains('<'), "the content origin answers no page");
    }
}

#[tokio::test]
async fn a_misdirected_request_gets_one_sentence_in_the_visitors_language() {
    let (status, headers, body) = send(
        "evil.example",
        "GET",
        "/-/health",
        &[("cookie", "inklkunn_lang=ja")],
    )
    .await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    assert!(body.starts_with("このサーバーは"), "{body}");
    assert_eq!(h(&headers, "content-type"), "text/plain; charset=utf-8");
    assert_eq!(h(&headers, "content-language"), "ja");
    assert!(headers.contains_key("content-security-policy"));
    let (_, _, body) = send("evil.example", "GET", "/-/health", &[]).await;
    assert!(
        body.starts_with("This server does not serve pages at that address."),
        "{body}"
    );
}

#[tokio::test]
async fn the_liveness_probe_is_still_the_fixed_word() {
    let (status, headers, body) = send("localhost:8080", "GET", "/-/health", &[]).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "ok\n"));
    assert!(headers.contains_key("content-security-policy"));
}
