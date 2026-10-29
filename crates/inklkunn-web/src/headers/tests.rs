#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};

use super::*;

fn blank(status: StatusCode) -> Response {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap()
}

fn value(h: &HeaderMap, name: &str) -> String {
    h.get(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn an_application_response_has_every_header() {
    let mut r = blank(StatusCode::OK);
    apply(&mut r, Surface::Application { https: true });
    let h = r.headers();
    assert_eq!(value(h, "content-security-policy"), APPLICATION_CSP);
    assert!(APPLICATION_CSP.contains("default-src 'none'"));
    assert!(
        !APPLICATION_CSP.contains("script-src"),
        "no script runs on any page"
    );
    assert!(!APPLICATION_CSP.contains("unsafe"), "{APPLICATION_CSP}");
    assert!(APPLICATION_CSP.contains("frame-ancestors 'none'"));
    assert_eq!(value(h, "x-content-type-options"), "nosniff");
    assert_eq!(value(h, "referrer-policy"), "same-origin");
    assert_eq!(value(h, "x-frame-options"), "DENY");
    assert_eq!(value(h, "cross-origin-opener-policy"), "same-origin");
    assert_eq!(value(h, "cross-origin-resource-policy"), "same-origin");
    assert!(value(h, "permissions-policy").contains("geolocation=()"));
    assert_eq!(value(h, "strict-transport-security"), "max-age=31536000");
    assert_eq!(value(h, "cache-control"), "private, no-cache");
    assert_eq!(value(h, "vary"), "Accept-Language, Cookie");
}

#[test]
fn the_content_origin_is_sandboxed_and_allows_nothing() {
    let mut r = blank(StatusCode::OK);
    apply(&mut r, Surface::Content { https: false });
    let h = r.headers();
    assert_eq!(value(h, "content-security-policy"), CONTENT_CSP);
    assert!(CONTENT_CSP.contains("sandbox") && CONTENT_CSP.contains("default-src 'none'"));
    assert_eq!(value(h, "x-content-type-options"), "nosniff");
    assert_eq!(
        value(h, "cross-origin-resource-policy"),
        "same-site",
        "a third party's page may not embed a wiki's bytes"
    );
    assert!(
        h.get("strict-transport-security").is_none(),
        "no HSTS over plain http"
    );
}

#[test]
fn a_handler_may_set_the_cache_headers_but_never_the_security_headers() {
    let mut r = blank(StatusCode::OK);
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    r.headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    r.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src *"),
    );
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("sniff"),
    );
    apply(&mut r, Surface::Application { https: false });
    let h = r.headers();
    assert_eq!(
        value(h, "cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(value(h, "vary"), "Accept-Encoding");
    assert_eq!(value(h, "content-security-policy"), APPLICATION_CSP);
    assert_eq!(value(h, "x-content-type-options"), "nosniff");
    assert_eq!(h.get_all("content-security-policy").iter().count(), 1);
}

#[test]
fn a_304_and_a_redirect_and_an_error_carry_them_too() {
    for status in [
        StatusCode::NOT_MODIFIED,
        StatusCode::SEE_OTHER,
        StatusCode::NOT_FOUND,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        let mut r = blank(status);
        apply(&mut r, Surface::Application { https: false });
        for name in [
            "content-security-policy",
            "x-content-type-options",
            "referrer-policy",
            "cache-control",
        ] {
            assert!(r.headers().contains_key(name), "{status}: no {name}");
        }
    }
}
