#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

fn target(url: &str, host: &str, port: u16, addrs: &[&str]) -> FetchTarget {
    FetchTarget {
        url: url.to_owned(),
        host: host.to_owned(),
        port,
        addrs: addrs.iter().map(|a| a.parse().unwrap()).collect(),
    }
}

fn refused(t: &FetchTarget) -> String {
    match t.check() {
        Err(CurlError::InvalidRequest(e)) => e,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_name_is_pinned_with_every_address_and_the_ipv6_ones_bracketed() {
    let t = target(
        "https://idp.example.org/.well-known/openid-configuration",
        "idp.example.org",
        443,
        &["203.0.113.7", "2001:db8::1"],
    );
    let c = t.check().expect("checks");
    assert_eq!(c.url, t.url);
    assert_eq!(
        c.resolve.as_deref(),
        Some("idp.example.org:443:203.0.113.7,[2001:db8::1]")
    );
}

#[test]
fn a_port_in_the_url_must_be_the_pinned_port_and_a_missing_one_is_443() {
    let t = target(
        "https://idp.example.org:8443/x",
        "idp.example.org",
        8443,
        &["198.51.100.1"],
    );
    assert_eq!(
        t.check().unwrap().resolve.as_deref(),
        Some("idp.example.org:8443:198.51.100.1")
    );
    let t = target(
        "https://idp.example.org:8443/x",
        "idp.example.org",
        443,
        &["198.51.100.1"],
    );
    assert!(refused(&t).contains("port"));
    let t = target(
        "https://idp.example.org/x",
        "idp.example.org",
        8443,
        &["198.51.100.1"],
    );
    assert!(refused(&t).contains("port"));
}

#[test]
fn the_url_must_be_https_and_name_the_pinned_host() {
    for url in [
        "http://idp.example.org/x",
        "ftp://idp.example.org/x",
        "idp.example.org/x",
        "//idp.example.org/x",
        "https:/idp.example.org/x",
    ] {
        let t = target(url, "idp.example.org", 443, &["198.51.100.1"]);
        assert!(refused(&t).contains("https"), "{url}");
    }
    let t = target(
        "https://evil.example.net/x",
        "idp.example.org",
        443,
        &["198.51.100.1"],
    );
    assert!(refused(&t).contains("host"));
    // The scheme and the host compare case-insensitively.
    let t = target(
        "HTTPS://IDP.Example.org/x",
        "idp.example.org",
        443,
        &["198.51.100.1"],
    );
    assert!(t.check().is_ok());
}

#[test]
fn userinfo_controls_and_malformed_authorities_are_refused() {
    for url in [
        "https://trusted@evil.example.net/x",
        "https://idp.example.org\n/x",
        "https://idp.example.org /x",
        "https://idp.example.org:0/x",
        "https://idp.example.org:99999/x",
        "https://idp.example.org:abc/x",
        "https://idp.example.org:/x",
        "https://[::1/x",
        "https:///x",
    ] {
        let t = target(url, "idp.example.org", 443, &["198.51.100.1"]);
        assert!(t.check().is_err(), "{url}");
    }
    let long = format!("https://idp.example.org/{}", "a".repeat(5000));
    let t = target(&long, "idp.example.org", 443, &["198.51.100.1"]);
    assert!(refused(&t).contains("long"));
}

#[test]
fn a_target_without_an_address_is_refused() {
    let t = target("https://idp.example.org/x", "idp.example.org", 443, &[]);
    assert!(refused(&t).contains("no address"));
}

#[test]
fn a_literal_address_needs_no_resolution_and_must_be_pinned() {
    let t = target(
        "https://203.0.113.7/x",
        "203.0.113.7",
        443,
        &["203.0.113.7"],
    );
    let c = t.check().unwrap();
    assert_eq!(c.resolve, None);
    let t = target(
        "https://[2001:db8::1]:8443/x",
        "2001:db8::1",
        8443,
        &["2001:db8::1"],
    );
    assert_eq!(t.check().unwrap().resolve, None);
    // The URL's literal must be among the pinned addresses: the guard checked those, not this one.
    let t = target(
        "https://203.0.113.7/x",
        "203.0.113.7",
        443,
        &["198.51.100.1"],
    );
    assert!(refused(&t).contains("pinned"));
}

#[test]
fn a_host_that_could_change_a_resolve_entry_is_refused() {
    // The URL parser here splits the authority at the first colon, so a colon cannot reach the entry; the
    // other separator characters are refused by name.
    for host in [
        "idp.example.org,evil",
        "idp example",
        "idp*.example.org",
        "idp\u{e9}.example.org",
    ] {
        let t = FetchTarget {
            url: format!("https://{host}/x"),
            host: host.to_owned(),
            port: 443,
            addrs: vec!["198.51.100.1".parse().unwrap()],
        };
        assert!(t.check().is_err(), "{host}");
    }
}

#[test]
fn debug_shows_the_url_and_nothing_secret() {
    let r = CurlRequest {
        target: target(
            "https://idp.example.org/token",
            "idp.example.org",
            443,
            &["198.51.100.1"],
        ),
        form: Some("code=SECRETCODE&code_verifier=SECRETVERIFIER".to_owned()),
        authorization: Some("Basic SECRETBASIC".to_owned()),
    };
    let shown = format!("{r:?} {r:#?}");
    assert!(shown.contains("https://idp.example.org/token"));
    for secret in ["SECRETCODE", "SECRETVERIFIER", "SECRETBASIC"] {
        assert!(!shown.contains(secret), "{shown}");
    }
}
