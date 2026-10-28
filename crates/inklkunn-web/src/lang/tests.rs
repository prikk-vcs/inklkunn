#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::http::HeaderValue;

use super::*;

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.append(
            axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v).unwrap(),
        );
    }
    h
}

#[test]
fn the_named_cookie_is_read_among_several_and_from_several_headers() {
    let h = headers(&[("cookie", "a=1; inklkunn_lang=ja; b=2")]);
    assert_eq!(get_cookie(&h, LANG_COOKIE), Some("ja"));
    assert_eq!(get_cookie(&h, "zzz"), None);
    let h = headers(&[("cookie", "a=1"), ("cookie", "inklkunn_lang=en")]);
    assert_eq!(get_cookie(&h, LANG_COOKIE), Some("en"));
    let h = headers(&[("cookie", "xinklkunn_lang=ja; inklkunn_lang_x=ja")]);
    assert_eq!(get_cookie(&h, LANG_COOKIE), None, "a name is matched whole");
    assert_eq!(get_cookie(&HeaderMap::new(), LANG_COOKIE), None);
}

#[test]
fn the_cookie_beats_the_header_and_a_value_that_names_no_language_is_ignored() {
    let tag = |pairs: &[(&str, &str)]| negotiate_from_headers(&headers(pairs)).tag();
    assert_eq!(tag(&[]), "en");
    assert_eq!(tag(&[("accept-language", "en-GB")]), "en");
    // Japanese is a preview: a browser that asks for it is not served it, a reader who chose it is.
    assert_eq!(
        tag(&[("accept-language", "ja")]),
        "en",
        "a preview is not chosen for the reader"
    );
    assert_eq!(
        tag(&[("accept-language", "en"), ("cookie", "inklkunn_lang=ja")]),
        "ja"
    );
    assert_eq!(tag(&[("cookie", "inklkunn_lang=ja")]), "ja");
    assert_eq!(
        tag(&[("accept-language", "ja"), ("cookie", "inklkunn_lang=en")]),
        "en"
    );
    assert_eq!(
        tag(&[("accept-language", "ja"), ("cookie", "inklkunn_lang=zz")]),
        "en"
    );
    assert_eq!(tag(&[("cookie", "inklkunn_lang=<script>")]), "en");
}

#[test]
fn the_cookie_holds_a_language_code_and_nothing_else() {
    let c = language_cookie("ja", true);
    assert_eq!(
        c,
        "inklkunn_lang=ja; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly; Secure"
    );
    let c = language_cookie("en", false);
    assert!(c.starts_with("inklkunn_lang=en;"), "{c}");
    assert!(!c.contains("Secure"), "{c}");
    assert!(c.contains("HttpOnly") && c.contains("SameSite=Lax"), "{c}");
}

#[test]
fn a_return_address_is_a_local_path_and_nothing_else() {
    for ok in [
        "/",
        "/-/lang",
        "/handbook/Home",
        "/handbook/%E3%83%9B%E3%83%BC%E3%83%A0",
        "/search?q=a+b&page=2",
        "/a/b#frag",
        "/~view/Title",
    ] {
        assert_eq!(safe_back(ok), Some(ok), "{ok}");
    }
    let long = format!("/{}", "a".repeat(MAX_BACK));
    for bad in [
        "",
        "x",
        "relative/path",
        "//evil.example/x",
        "https://evil.example/",
        "/\\evil.example",
        "javascript:alert(1)",
        "/a b",
        "/a\tb",
        "/a\nb",
        "/a\"b",
        "/a<b",
        "/a>b",
        "/ホーム",
        "/a{b}",
        "/a|b",
        "/a^b",
        "/a`b",
        long.as_str(),
    ] {
        assert_eq!(safe_back(bad), None, "{bad:?}");
    }
    let edge = format!("/{}", "a".repeat(MAX_BACK - 1));
    assert_eq!(safe_back(&edge), Some(edge.as_str()), "the limit itself");
}

fn back(method: &Method, uri: &str) -> String {
    back_for(method, &uri.parse::<Uri>().unwrap())
}

#[test]
fn a_switch_returns_to_the_page_it_was_made_on() {
    assert_eq!(back(&Method::GET, "/handbook/Home"), "/handbook/Home");
    assert_eq!(
        back(&Method::GET, "/handbook/-/search?q=cat&page=2"),
        "/handbook/-/search?q=cat&page=2"
    );
    assert_eq!(back(&Method::GET, "/"), "/");
    // A form answer has no address worth returning to.
    assert_eq!(back(&Method::POST, "/-/lang"), "/");
    assert_eq!(back(&Method::POST, "/handbook/Home"), "/");
    // The switcher's own page and the sign-in (whose query may hold a one-time code) are never copied.
    assert_eq!(back(&Method::GET, "/-/lang?next=/x"), "/");
    assert_eq!(back(&Method::GET, "/-/sign-in"), "/");
    assert_eq!(back(&Method::GET, "/-/sign-in/callback?code=SECRET"), "/");
    assert_eq!(
        back(&Method::GET, "/-/sign-ins"),
        "/-/sign-ins",
        "a longer name is another address"
    );
    // A request cannot carry a raw `<` in its address (it is not a URI), but a query the browser did send with
    // a character a return address may not hold is not copied.
    assert_eq!(back(&Method::GET, "/a?b=%3Cscript%3E"), "/a?b=%3Cscript%3E");
    assert_eq!(back(&Method::GET, "/a?b={x}"), "/");
    // A path that starts with two slashes would be read as a host by a redirect: it returns to the start.
    if let Ok(uri) = "//evil.example/x".parse::<Uri>() {
        assert_eq!(back_for(&Method::GET, &uri), "/");
    }
    assert_eq!(back(&Method::GET, "/a//b"), "/a//b");
}

#[test]
fn a_form_is_from_this_site_when_the_browser_says_so_or_says_nothing() {
    let ok = |pairs: &[(&str, &str)]| from_this_site(&headers(pairs));
    // Fetch metadata decides where the browser sends it.
    assert!(ok(&[("sec-fetch-site", "same-origin")]));
    assert!(ok(&[("sec-fetch-site", "none")]));
    assert!(!ok(&[("sec-fetch-site", "cross-site")]));
    assert!(
        !ok(&[("sec-fetch-site", "same-site")]),
        "a sibling subdomain, such as the content origin, is not this origin"
    );
    assert!(
        !ok(&[
            ("sec-fetch-site", "cross-site"),
            ("origin", "https://wiki.example"),
            ("host", "wiki.example"),
        ]),
        "fetch metadata wins over a matching Origin"
    );
    // Without it the Origin must name the Host.
    assert!(ok(&[
        ("origin", "https://wiki.example"),
        ("host", "wiki.example")
    ]));
    assert!(ok(&[
        ("origin", "http://localhost:8080"),
        ("host", "localhost:8080")
    ]));
    assert!(ok(&[
        ("origin", "https://WIKI.example"),
        ("host", "wiki.example")
    ]));
    assert!(!ok(&[
        ("origin", "https://evil.example"),
        ("host", "wiki.example")
    ]));
    assert!(!ok(&[
        ("origin", "http://localhost:9999"),
        ("host", "localhost:8080")
    ]));
    assert!(!ok(&[("origin", "null"), ("host", "wiki.example")]));
    assert!(
        !ok(&[("origin", "https://wiki.example")]),
        "no Host to compare"
    );
    // Neither: not a cross-site form post.
    assert!(ok(&[]));
    assert!(ok(&[("host", "wiki.example")]));
}
