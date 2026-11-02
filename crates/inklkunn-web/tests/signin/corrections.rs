//! The corrections from the review of 2026-11-01 (G1 – G10), through the real router and the real authenticator.

use inklkunn_auth::{AuthConfig, ProxyPolicy};

use crate::harness::*;

/// A failed sign-in from the connection of `peer`, which carried `forwarded` as `X-Forwarded-For` if given.
async fn failed_sign_in(r: &Real, peer: &str, forwarded: Option<&str>) -> u16 {
    let page = r.get("/-/sign-in", &[]).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}");
    let mut headers = vec![SAME_SITE, ("cookie", cookie.as_str())];
    if let Some(f) = forwarded {
        headers.push(("x-forwarded-for", f));
    }
    r.call_from(
        Some(format!("{peer}:50000").parse().unwrap()),
        "POST",
        "/-/sign-in",
        &headers,
        &form(&[("csrf", &token), ("login", "nobody"), ("password", "wrong")]),
    )
    .await
    .status
    .as_u16()
}

/// G1: a reverse proxy on the same host that nobody declared is the likeliest deployment, and every client is
/// then 127.0.0.1. The server says so, once, when it sees it.
#[tokio::test]
async fn a_loopback_proxy_nobody_declared_is_said_once_at_run_time() {
    let r = real(config());
    let ask = |peer: &'static str, forwarded: Option<&'static str>| {
        let r = &r;
        async move { failed_sign_in(r, peer, forwarded).await }
    };
    let said = || {
        r.w.logged()
            .into_iter()
            .filter(|l| l.contains("X-Forwarded-For"))
            .collect::<Vec<_>>()
    };
    // Nothing yet: no header, and a header from a peer that is not loopback.
    ask("127.0.0.1", None).await;
    ask("203.0.113.5", Some("198.51.100.1")).await;
    assert!(said().is_empty(), "{:?}", said());
    // The first loopback request that carries the header; then any number more.
    ask("127.0.0.1", Some("198.51.100.1")).await;
    ask("127.0.0.1", Some("198.51.100.2")).await;
    ask("[::1]", Some("198.51.100.3")).await;
    let lines = said();
    assert_eq!(lines.len(), 1, "once: {lines:?}");
    let line = lines.first().unwrap();
    assert!(
        line.contains("INKLKUNN_TRUSTED_PROXIES")
            && line.contains("every client counts as one address"),
        "{line}"
    );
}

/// ... and not when the operator declared a proxy: then the header is read and there is nothing to say.
#[tokio::test]
async fn a_declared_proxy_is_not_said_anything_about() {
    let r = real(AuthConfig {
        proxies: ProxyPolicy::trusting(&["127.0.0.0/8"]).unwrap(),
        ..config()
    });
    failed_sign_in(&r, "127.0.0.1", Some("198.51.100.1")).await;
    assert!(
        r.w.logged().iter().all(|l| !l.contains("X-Forwarded-For")),
        "{:?}",
        r.w.logged()
    );
}
