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
fn path_is_reserved_or_parametric(path: &str) -> bool {
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match segments.as_slice() {
        [""] => true,
        ["-", ..] => true,
        [first, "-", ..] if first.starts_with('{') => true,
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
fn the_shadowing_check_catches_its_planted_violations() {
    for ok in [
        "/",
        "/-/health",
        "/{slug}/-/history/{*title}",
        "/{slug}/{*title}",
        "/-/wikis",
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
