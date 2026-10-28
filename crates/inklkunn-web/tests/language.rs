//! RFC 003 D-6 through the router: the language a visitor gets (the cookie, else `Accept-Language`, else
//! English), the switcher at `/-/lang` that works with no script and keeps its choice, what it refuses and
//! how it says so, the preview label wherever Japanese is offered, and the Japanese catalog's completeness.
//! No prikk is needed.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use inklkunn_web::WebConfig;
use inklkunn_web::catalog::{EN_DEF, JA_DEF, Key, Review, audit, languages};
use inklkunn_web::routes::site;
use tower::ServiceExt as _;

const APP: &str = "localhost:8080";
const FORM: &str = "application/x-www-form-urlencoded";

fn router() -> Router {
    site(
        &WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
    )
    .unwrap()
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    fn set_cookies(&self) -> Vec<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok().map(str::to_owned))
            .collect()
    }
}

async fn send(router: &Router, request: Request<Body>) -> Reply {
    let res = router.clone().oneshot(request).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 20).await.unwrap();
    Reply {
        status: parts.status,
        headers: parts.headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn get(uri: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut b = Request::get(uri).header("host", APP);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    b.body(Body::empty()).unwrap()
}

/// A form post as a browser sends it from a page of this site.
fn post(body: &str, extra: &[(&str, &str)]) -> Request<Body> {
    let mut b = Request::post("/-/lang")
        .header("host", APP)
        .header(header::CONTENT_TYPE, FORM)
        .header("origin", "http://localhost:8080")
        .header("sec-fetch-site", "same-origin");
    for (k, v) in extra {
        b = b.header(*k, *v);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| format!("%{b:02X}")).collect()
}

// ---------------------------------------------------------------- the Japanese catalog

#[test]
fn the_japanese_catalog_has_every_key_with_the_same_placeholders_and_no_markup() {
    assert_eq!(
        audit(&[&EN_DEF, &JA_DEF]),
        vec![],
        "the Japanese catalog has problems"
    );
    assert_eq!(JA_DEF.entries.len(), Key::ALL.len());
    for key in Key::ALL {
        let e = JA_DEF
            .entries
            .iter()
            .find(|e| e.key == key.id())
            .unwrap_or_else(|| panic!("the Japanese catalog lacks {}", key.id()));
        assert!(!e.other.trim().is_empty(), "{}", key.id());
        assert_eq!(e.one, e.other, "{}: Japanese has no plural", key.id());
    }
}

#[tokio::test]
async fn japanese_is_a_preview_until_a_sign_off_is_recorded_and_says_so_wherever_it_is_offered() {
    assert_eq!(JA_DEF.review, Review::Preview);
    assert_eq!(EN_DEF.review, Review::Source);
    let tags: Vec<&str> = languages().iter().map(|d| d.tag).collect();
    assert_eq!(tags, ["en", "ja"]);
    // In the footer of a page in English, in Japanese, and on the language page itself.
    let r = router();
    let in_english: &[(&str, &str)] = &[];
    let in_japanese: &[(&str, &str)] = &[("cookie", "inklkunn_lang=ja")];
    for (uri, headers, label) in [
        (
            "/anywhere",
            in_english,
            "<span lang=\"ja\">日本語</span> (preview)",
        ),
        (
            "/anywhere",
            in_japanese,
            "<span lang=\"ja\">日本語</span>（プレビュー版）",
        ),
        (
            "/-/lang",
            in_english,
            "<span lang=\"ja\">日本語</span> (preview)",
        ),
        (
            "/-/lang",
            in_japanese,
            "<span lang=\"ja\">日本語</span>（プレビュー版）",
        ),
    ] {
        let reply = send(&r, get(uri, headers)).await;
        assert!(
            reply.body.contains(label),
            "{uri} {headers:?}: {}",
            reply.body
        );
        assert!(
            !reply
                .body
                .contains("<span lang=\"en\">English</span> (preview)"),
            "English is not a preview"
        );
    }
    let page = send(&r, get("/-/lang", &[])).await;
    assert!(
        page.body.contains("This translation is a preview."),
        "{}",
        page.body
    );
    let page = send(&r, get("/-/lang", in_japanese)).await;
    assert!(
        page.body.contains("この翻訳はプレビュー版です。"),
        "{}",
        page.body
    );
}

// ---------------------------------------------------------------- negotiation

#[tokio::test]
async fn english_is_the_default_and_the_page_says_so() {
    let reply = send(&router(), get("/-/lang", &[])).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(
        reply.body.contains("<html lang=\"en\" dir=\"ltr\">"),
        "{}",
        reply.body
    );
    assert_eq!(reply.header("content-language"), "en");
    let vary = reply.header("vary");
    assert!(
        vary.contains("Accept-Language") && vary.contains("Cookie"),
        "{vary}"
    );
}

#[tokio::test]
async fn accept_language_and_the_cookie_choose_the_language_and_a_preview_only_by_the_cookie() {
    let r = router();
    for (accept, cookie, want) in [
        (Some("en-GB,fr;q=0.5"), None, "en"),
        (Some("fr"), None, "en"),
        (None, None, "en"),
        // Japanese is a preview: a browser that asks for it is served English, with the choice on offer…
        (Some("ja"), None, "en"),
        (Some("ja-JP,en;q=0.5"), None, "en"),
        (Some("en;q=0.4,ja;q=0.9"), None, "en"),
        // …and a reader who chose it is served Japanese, whatever the browser asks for.
        (None, Some("inklkunn_lang=ja"), "ja"),
        (Some("en"), Some("inklkunn_lang=ja"), "ja"),
        (Some("fr"), Some("other=1; inklkunn_lang=ja; more=2"), "ja"),
        (Some("ja"), Some("inklkunn_lang=en"), "en"),
        (Some("ja"), Some("inklkunn_lang=zz"), "en"),
        (None, Some("inklkunn_lang=<script>"), "en"),
        (Some("ja"), Some("other=1; inklkunn_lang=en; more=2"), "en"),
    ] {
        let mut h: Vec<(&str, &str)> = Vec::new();
        if let Some(a) = accept {
            h.push(("accept-language", a));
        }
        if let Some(c) = cookie {
            h.push(("cookie", c));
        }
        let reply = send(&r, get("/-/lang", &h)).await;
        assert!(
            reply.body.contains(&format!("<html lang=\"{want}\"")),
            "{accept:?} {cookie:?} → {want}: {}",
            reply.body
        );
        assert_eq!(reply.header("content-language"), want);
    }
}

// ---------------------------------------------------------------- the switcher, with no script

#[tokio::test]
async fn the_switcher_keeps_its_choice_without_a_script() {
    let r = router();
    // The page offers plain forms and nothing that needs a script; the policy forbids one anyway.
    let page = send(&r, get("/-/lang?next=/handbook/Home", &[])).await;
    assert_eq!(page.status, StatusCode::OK);
    for forbidden in [
        "<script",
        "onclick",
        "onsubmit",
        "javascript:",
        "<style",
        "style=",
    ] {
        assert!(!page.body.contains(forbidden), "{forbidden}: {}", page.body);
    }
    let csp = page.header("content-security-policy");
    assert!(
        csp.contains("default-src 'none'") && !csp.contains("script-src"),
        "{csp}"
    );
    assert_eq!(
        page.body
            .matches("<form method=\"post\" action=\"/-/lang\">")
            .count(),
        2
    );

    // The form a browser would send for the Japanese button, built from the page's own fields.
    assert!(
        page.body
            .contains("<input type=\"hidden\" name=\"lang\" value=\"ja\">")
    );
    assert!(
        page.body
            .contains("<input type=\"hidden\" name=\"next\" value=\"/handbook/Home\">")
    );
    let chosen = send(&r, post("lang=ja&next=%2Fhandbook%2FHome", &[])).await;
    assert_eq!(chosen.status, StatusCode::SEE_OTHER);
    assert_eq!(chosen.header("location"), "/handbook/Home");
    let cookies = chosen.set_cookies();
    assert_eq!(
        cookies,
        ["inklkunn_lang=ja; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly"],
        "a language code and nothing else; no Secure on a plain-http development origin"
    );

    // The next request carries the cookie the browser kept: Japanese, though the browser asks for English.
    let next = send(
        &r,
        get(
            "/handbook/Home",
            &[("accept-language", "en"), ("cookie", "inklkunn_lang=ja")],
        ),
    )
    .await;
    assert_eq!(next.status, StatusCode::NOT_FOUND);
    assert!(
        next.body.contains("<html lang=\"ja\" dir=\"ltr\">"),
        "{}",
        next.body
    );
    // Without the cookie it is the browser's own choice again.
    let plain = send(&r, get("/handbook/Home", &[("accept-language", "en")])).await;
    assert!(plain.body.contains("<html lang=\"en\""), "{}", plain.body);

    // And back to English, from a page that is itself Japanese.
    let page = send(&r, get("/handbook/Home", &[("cookie", "inklkunn_lang=ja")])).await;
    assert!(
        page.body
            .contains("<input type=\"hidden\" name=\"next\" value=\"/handbook/Home\">"),
        "the footer returns to the page it is on: {}",
        page.body
    );
    let back = send(
        &r,
        post(
            "lang=en&next=%2Fhandbook%2FHome",
            &[("cookie", "inklkunn_lang=ja")],
        ),
    )
    .await;
    assert_eq!(back.status, StatusCode::SEE_OTHER);
    assert_eq!(back.header("location"), "/handbook/Home");
    assert!(
        back.set_cookies()
            .iter()
            .any(|c| c.starts_with("inklkunn_lang=en;"))
    );
}

#[tokio::test]
async fn a_tag_with_a_region_is_normalised_and_an_https_site_marks_the_cookie_secure() {
    let r = router();
    let chosen = send(&r, post("lang=ja-JP&next=%2F", &[])).await;
    assert!(
        chosen
            .set_cookies()
            .iter()
            .any(|c| c.starts_with("inklkunn_lang=ja;"))
    );

    let https = site(
        &WebConfig::new(
            "0.0.0.0:8080",
            "https://wiki.example",
            "https://files.wiki.example",
        )
        .unwrap(),
    )
    .unwrap();
    let req = Request::post("/-/lang")
        .header("host", "wiki.example")
        .header(header::CONTENT_TYPE, FORM)
        .header("origin", "https://wiki.example")
        .body(Body::from("lang=ja&next=%2F"))
        .unwrap();
    let chosen = send(&https, req).await;
    assert_eq!(chosen.status, StatusCode::SEE_OTHER);
    assert!(
        chosen
            .set_cookies()
            .iter()
            .all(|c| c.ends_with("; HttpOnly; Secure")),
        "{:?}",
        chosen.set_cookies()
    );
}

#[tokio::test]
async fn a_return_address_that_is_not_local_goes_to_the_language_page() {
    let r = router();
    for next in [
        "//evil.example/x",
        "https://evil.example/",
        "/\\evil.example",
        "javascript:alert(1)",
        "",
        "/a b",
        "/a\"b",
    ] {
        let chosen = send(&r, post(&format!("lang=ja&next={}", encode(next)), &[])).await;
        assert_eq!(chosen.status, StatusCode::SEE_OTHER, "{next:?}");
        assert_eq!(chosen.header("location"), "/-/lang", "{next:?}");
    }
    // No `next` at all is the same.
    let chosen = send(&r, post("lang=ja", &[])).await;
    assert_eq!(chosen.header("location"), "/-/lang");
    // On the page itself, a return address that is not local is not offered as a link or as a field value.
    let page = send(&r, get("/-/lang?next=//evil.example/x", &[])).await;
    assert!(!page.body.contains("evil.example"), "{}", page.body);
    assert!(
        page.body.contains("name=\"next\" value=\"/-/lang\""),
        "{}",
        page.body
    );
}

#[tokio::test]
async fn a_form_the_switcher_cannot_accept_changes_nothing_and_says_why() {
    let r = router();
    // From another site: 403 and a page that says nothing was changed.
    for extra in [
        vec![("sec-fetch-site", "cross-site")],
        vec![("sec-fetch-site", "same-site")],
        vec![
            ("origin", "https://evil.example"),
            ("sec-fetch-site", "cross-site"),
        ],
    ] {
        let mut req = post("lang=ja&next=%2F", &[]);
        for (k, v) in &extra {
            req.headers_mut().insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v).unwrap(),
            );
        }
        let refused = send(&r, req).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{extra:?}");
        assert!(
            refused.body.contains("<h1>The form was not accepted</h1>"),
            "{}",
            refused.body
        );
        assert!(
            refused.set_cookies().is_empty(),
            "{extra:?}: a refused form sets nothing"
        );
    }
    // An Origin that is not this host, from a browser that sends no fetch metadata.
    let req = Request::post("/-/lang")
        .header("host", APP)
        .header(header::CONTENT_TYPE, FORM)
        .header("origin", "https://evil.example")
        .body(Body::from("lang=ja"))
        .unwrap();
    let refused = send(&r, req).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(refused.set_cookies().is_empty());

    // A language this build does not have is refused, not defaulted; so is a form that names none.
    for body in [
        "lang=xx".to_owned(),
        "lang=".to_owned(),
        "next=%2F".to_owned(),
        "".to_owned(),
        format!("lang={}", encode("ja;evil=1")),
        format!("lang={}", encode("en\r\nSet-Cookie: x=1")),
        format!("lang={}", encode("<script>")),
    ] {
        let refused = send(&r, post(&body, &[])).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{body:?}");
        assert!(
            refused
                .body
                .contains("<h1>The language was not changed</h1>"),
            "{}",
            refused.body
        );
        assert!(
            refused
                .body
                .contains("<a href=\"/-/lang\">Go to the language page</a>"),
            "{}",
            refused.body
        );
        assert!(refused.set_cookies().is_empty(), "{body:?}");
    }

    // A body that is not a form, or is too large to be this one, is refused with a page and a status.
    let req = Request::post("/-/lang")
        .header("host", APP)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("lang=ja"))
        .unwrap();
    let refused = send(&r, req).await;
    assert!(refused.status.is_client_error(), "{}", refused.status);
    assert!(
        refused
            .body
            .contains("<h1>The language was not changed</h1>"),
        "{}",
        refused.body
    );
    assert!(refused.set_cookies().is_empty());
    let big = format!("lang=ja&next=/{}", "a".repeat(8192));
    let refused = send(&r, post(&big, &[])).await;
    assert_eq!(refused.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(refused.body.contains("<html"), "{}", refused.body);
    assert!(refused.set_cookies().is_empty());

    // The words of a refusal follow the language too.
    let refused = send(&r, post("lang=xx", &[("cookie", "inklkunn_lang=ja")])).await;
    assert!(
        refused.body.contains("<h1>言語は変更されていません</h1>"),
        "{}",
        refused.body
    );
}

#[tokio::test]
async fn a_get_never_changes_the_language_and_other_methods_are_not_offered() {
    let r = router();
    let reply = send(&r, get("/-/lang?lang=ja&next=/", &[])).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.set_cookies().is_empty());
    assert!(reply.body.contains("<html lang=\"en\""));
    for method in ["PUT", "DELETE", "PATCH"] {
        let req = Request::builder()
            .method(method)
            .uri("/-/lang")
            .header("host", APP)
            .body(Body::empty())
            .unwrap();
        let reply = send(&r, req).await;
        assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
        assert!(
            reply.headers.contains_key("content-security-policy"),
            "{method}"
        );
    }
    // HEAD is answered like GET, without a body.
    let req = Request::builder()
        .method("HEAD")
        .uri("/-/lang")
        .header("host", APP)
        .body(Body::empty())
        .unwrap();
    let reply = send(&r, req).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.body.is_empty());
}

#[tokio::test]
async fn a_refusal_and_the_language_page_have_no_footer_switcher_of_their_own() {
    let r = router();
    let page = send(&r, get("/-/lang", &[])).await;
    assert!(!page.body.contains("<nav"), "{}", page.body);
    let refused = send(&r, post("lang=xx", &[])).await;
    assert!(!refused.body.contains("<nav"), "{}", refused.body);
    let missing = send(&r, get("/nothing", &[])).await;
    assert!(
        missing
            .body
            .contains("<nav class=\"lang\" aria-label=\"Language\">"),
        "{}",
        missing.body
    );
}
