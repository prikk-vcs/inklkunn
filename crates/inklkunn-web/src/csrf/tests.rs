#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::http::Method;

use super::*;

#[test]
fn the_token_is_the_first_csrf_field_of_the_form() {
    assert_eq!(
        form_field(b"a=1&csrf=tok&b=2", "csrf").as_deref(),
        Some("tok")
    );
    assert_eq!(
        form_field(b"csrf=first&csrf=second", "csrf").as_deref(),
        Some("first")
    );
    assert_eq!(
        form_field(b"csrf=a%2Bb+c", "csrf").as_deref(),
        Some("a+b c")
    );
    assert_eq!(form_field(b"%63srf=tok", "csrf").as_deref(), Some("tok"));
    assert_eq!(form_field(b"csrf=", "csrf").as_deref(), Some(""));
    for none in [
        &b""[..],
        b"a=1",
        b"csrfx=1",
        b"xcsrf=1",
        b"csrf",
        b"csrf=%FF",
    ] {
        assert_eq!(
            form_field(none, "csrf").filter(|t| !t.is_empty()),
            None,
            "{}",
            String::from_utf8_lossy(none)
        );
    }
}

#[test]
fn only_a_request_that_changes_something_needs_the_token_and_only_three_posts_are_excused() {
    for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
        assert!(!needs_token(&method, "/anything"), "{method}");
    }
    for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        assert!(needs_token(&method, "/w/-/proposals/new"), "{method}");
        assert!(needs_token(&method, "/-/inbox"), "{method}");
        assert!(needs_token(&method, "/-/sign-out"), "{method}");
        assert!(needs_token(&method, "/-/account/tokens"), "{method}");
        assert!(!needs_token(&method, "/-/lang"), "{method}");
        assert!(!needs_token(&method, "/-/sign-in"), "{method}");
        assert!(!needs_token(&method, "/-/join"), "{method}");
        assert!(!needs_token(&method, "/-/invite/inv_x"), "{method}");
    }
    // A lookalike is not an excuse.
    for path in [
        "/-/sign-in/",
        "/-/sign-in/x",
        "/-/joined",
        "/-/invite",
        "/x/-/sign-in",
        "/-/langs",
    ] {
        assert!(needs_token(&Method::POST, path), "{path}");
    }
}

#[test]
fn only_a_urlencoded_body_is_a_form() {
    let with = |ct: &str| {
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, ct.parse().unwrap());
        h
    };
    assert!(is_form(&with("application/x-www-form-urlencoded")));
    assert!(is_form(&with(
        "Application/X-WWW-Form-Urlencoded; charset=UTF-8"
    )));
    assert!(!is_form(&with("application/json")));
    assert!(!is_form(&with("multipart/form-data; boundary=x")));
    assert!(!is_form(&HeaderMap::new()));
}

#[tokio::test]
async fn a_request_the_layer_decided_nothing_about_is_nobody_and_its_write_is_refused() {
    // H3: with no decision attached (a handler wired outside the layer, a route someone exempts later) the
    // request is anonymous and its write is a refusal, whatever cookie it carries; nothing is asked again.
    use axum::extract::FromRequestParts;
    let request = axum::http::Request::builder()
        .uri("/w/-/proposals/new")
        .header(header::COOKIE, "__Host-inklkunn-session=whatever")
        .body(())
        .unwrap();
    let (mut parts, ()) = request.into_parts();
    let Ok(decided) = Decided::from_request_parts(&mut parts, &()).await;
    assert!(matches!(decided.principal(), Principal::Anonymous));
    assert_eq!(decided.write_identity(), WriteIdentity::CsrfMismatch);
}

#[test]
fn a_refusal_of_the_layer_is_nobody_and_stays_a_refusal() {
    // (A decided principal is the layer's to make, from the authenticator's; the sign-in tests that write
    // through the real router are what shows it arrives.)
    for refused in [WriteIdentity::CsrfMismatch, WriteIdentity::InvalidToken] {
        let decided = Decided(Some(refused.clone()));
        assert!(matches!(decided.principal(), Principal::Anonymous));
        assert_eq!(decided.write_identity(), refused);
    }
    assert_eq!(Decided(None).write_identity(), WriteIdentity::CsrfMismatch);
}
