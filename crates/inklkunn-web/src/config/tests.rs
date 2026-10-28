#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn origins_parse_and_print_back() {
    for (text, scheme, host, port) in [
        ("https://wiki.example", Scheme::Https, "wiki.example", None),
        (
            "https://Wiki.Example:8443",
            Scheme::Https,
            "wiki.example",
            Some(8443),
        ),
        (
            "http://localhost:8080",
            Scheme::Http,
            "localhost",
            Some(8080),
        ),
        (
            "http://127.0.0.1:3000",
            Scheme::Http,
            "127.0.0.1",
            Some(3000),
        ),
        ("http://[::1]:3000", Scheme::Http, "[::1]", Some(3000)),
        (
            "https://xn--wiki-9ra.example",
            Scheme::Https,
            "xn--wiki-9ra.example",
            None,
        ),
    ] {
        let o = Origin::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(
            (o.scheme(), o.host(), o.port()),
            (scheme, host, port),
            "{text}"
        );
        assert_eq!(Origin::parse(&o.to_string()).unwrap(), o, "{text}");
    }
}

#[test]
fn what_is_not_an_origin_is_refused_with_the_reason() {
    for (bad, why) in [
        ("", "starts with https://"),
        ("wiki.example", "starts with https://"),
        ("ftp://wiki.example", "starts with https://"),
        ("https://", "no path, query or user"),
        ("https://wiki.example/", "no path, query or user"),
        ("https://wiki.example/path", "no path, query or user"),
        ("https://user@wiki.example", "no path, query or user"),
        ("https://wiki.example?x=1", "no path, query or user"),
        ("https://wiki.example:0", "port"),
        ("https://wiki.example:99999", "port"),
        ("https://wiki.example:http", "port"),
        ("https://-bad.example", "host"),
        ("https://bad_host.example", "host"),
        ("https://wiki example", "no path, query or user"),
        ("http://wiki.example", "loopback"),
        ("http://192.0.2.1:80", "loopback"),
    ] {
        let e = Origin::parse(bad).unwrap_err();
        assert!(e.0.contains(why), "{bad:?}: {e}");
    }
}

#[test]
fn a_host_header_matches_its_origin_with_the_default_port_left_out() {
    let https = Origin::parse("https://wiki.example").unwrap();
    assert!(https.matches_host_header("wiki.example"));
    assert!(https.matches_host_header("WIKI.example:443"));
    assert!(!https.matches_host_header("wiki.example:8443"));
    assert!(!https.matches_host_header("evil.example"));
    assert!(!https.matches_host_header("wiki.example.evil.example"));
    assert!(!https.matches_host_header(""));
    assert!(!https.matches_host_header("wiki.example:abc"));
    let dev = Origin::parse("http://localhost:8080").unwrap();
    assert!(dev.matches_host_header("localhost:8080"));
    assert!(!dev.matches_host_header("localhost"));
    assert!(!dev.matches_host_header("localhost:8081"));
}

#[test]
fn the_three_settings_are_checked_together() {
    let ok = WebConfig::new(
        "127.0.0.1:8080",
        "http://localhost:8080",
        "http://localhost:8081",
    )
    .unwrap();
    assert_eq!(ok.bind.port(), 8080);
    let public = WebConfig::new(
        "0.0.0.0:8080",
        "https://wiki.example",
        "https://files.wiki.example",
    )
    .unwrap();
    assert_eq!(public.content_origin.host(), "files.wiki.example");

    let e = |bind, app, content| WebConfig::new(bind, app, content).unwrap_err().0;
    assert!(e("nonsense", "https://a.example", "https://b.example").contains("INKLKUNN_BIND"));
    assert!(e("0.0.0.0:80", "wiki.example", "https://b.example").contains("INKLKUNN_APP_ORIGIN"));
    assert!(
        e("0.0.0.0:80", "https://a.example", "http://b.example")
            .contains("INKLKUNN_CONTENT_ORIGIN")
    );
    // The same origin twice, wherever the server listens.
    for bind in ["127.0.0.1:8080", "0.0.0.0:8080"] {
        assert!(
            e(bind, "https://a.example", "https://a.example").contains("same origin"),
            "{bind}"
        );
    }
    // The same host on another port: allowed on loopback, refused on anything reachable.
    assert!(
        WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:9090"
        )
        .is_ok()
    );
    for bind in ["0.0.0.0:8080", "192.0.2.10:443", "[::]:443"] {
        let m = e(bind, "https://a.example", "https://a.example:8443");
        assert!(
            m.contains("own host name") && m.contains("a.example"),
            "{bind}: {m}"
        );
    }
}
