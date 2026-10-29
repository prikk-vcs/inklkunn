#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn landing(next: Option<&str>) -> Landing {
    Landing::new(next, vec!["a=1".to_owned(), "b=2".to_owned()])
}

#[test]
fn a_landing_is_a_200_page_with_a_meta_refresh_and_a_link_and_never_a_redirect() {
    let l = landing(Some("/handbook/Home"));
    assert_eq!(l.status(), 200);
    assert_eq!(l.target(), "/handbook/Home");
    assert_eq!(l.set_cookies(), ["a=1", "b=2"]);
    let body = l.body();
    assert!(body.starts_with("<!doctype html>"), "{body}");
    assert!(
        body.contains("<meta http-equiv=\"refresh\" content=\"0;url=/handbook/Home\">"),
        "{body}"
    );
    assert!(
        body.contains("<a href=\"/handbook/Home\">continue</a>"),
        "{body}"
    );
    assert!(!body.contains("<script"), "no script: {body}");
    assert!(
        !l.headers()
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("location"))
    );
}

#[test]
fn the_page_is_sent_uncached_without_a_referrer_and_under_a_policy_that_allows_nothing() {
    let h: std::collections::HashMap<_, _> = landing(None).headers().iter().copied().collect();
    let h = |name: &str| h.get(name).copied().unwrap();
    assert_eq!(h("Content-Type"), "text/html; charset=utf-8");
    assert_eq!(h("Cache-Control"), "no-store");
    assert_eq!(h("Referrer-Policy"), "no-referrer");
    assert_eq!(h("X-Content-Type-Options"), "nosniff");
    let csp = h("Content-Security-Policy");
    assert!(csp.starts_with("default-src 'none'"), "{csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
}

#[test]
fn only_a_path_on_this_site_is_a_target() {
    for ok in ["/", "/x", "/handbook/Home?x=1&y=2#top", "/日本語", "/a;b"] {
        assert_eq!(landing(Some(ok)).target(), ok, "{ok}");
    }
    let too_long = format!("/{}", "a".repeat(MAX_TARGET_BYTES));
    for bad in [
        "",
        "evil",
        "https://evil.example/",
        "http://evil.example/",
        "//evil.example/",
        "///evil.example/",
        "/\\evil.example",
        "/ x",
        "/x\ty",
        "/x\ny",
        "/x\ry",
        "/x\0y",
        "/x\u{2028}y",
        "javascript:alert(1)",
        too_long.as_str(),
    ] {
        assert_eq!(landing(Some(bad)).target(), "/", "{bad:?}");
    }
    assert_eq!(landing(None).target(), "/");
}

#[test]
fn the_target_is_escaped_where_it_lands_in_the_page() {
    let l = landing(Some("/x\"><script>alert('1')</script>&"));
    let body = l.body();
    assert!(!body.contains("<script"), "{body}");
    assert!(!body.contains("\"><"), "{body}");
    assert!(
        body.contains("/x&quot;&gt;&lt;script&gt;alert(&#39;1&#39;)&lt;/script&gt;&amp;"),
        "{body}"
    );
}

#[test]
fn debug_shows_the_target_and_never_a_cookie() {
    let l = Landing::new(Some("/p"), vec!["__Host-s=SECRETVALUE".to_owned()]);
    let shown = format!("{l:?}");
    assert!(
        shown.contains("/p") && !shown.contains("SECRETVALUE"),
        "{shown}"
    );
}
