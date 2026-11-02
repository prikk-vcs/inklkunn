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

// ------------------------------------------------------------------------------------ G2

/// The start of a provider sign-in is a plain GET that a page of another site can trigger. A browser that says
/// what a request is gets a start only for a person's own navigation; one that says nothing is served as before.
#[tokio::test]
async fn a_provider_start_is_for_a_persons_own_navigation_when_the_browser_says_what_it_is() {
    let (r, _) = crate::provider::with_provider(inklkunn_auth::JoinPolicy::Off);
    let secret = crate::provider::invitation(&r, None);
    let invite_start = format!("/-/invite/{secret}/oidc");
    let refused: [&[(&str, &str)]; 5] = [
        &[("sec-fetch-mode", "no-cors"), ("sec-fetch-dest", "image")],
        &[("sec-fetch-mode", "cors"), ("sec-fetch-dest", "empty")],
        &[("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "iframe")],
        &[("sec-fetch-mode", "no-cors")],
        &[("sec-fetch-dest", "script")],
    ];
    // Far more than the thirty starts an address has: none of them spends one.
    for _ in 0..10 {
        for headers in refused {
            for uri in ["/-/sign-in/oidc", invite_start.as_str()] {
                let a = r.get(uri, headers).await;
                assert_eq!(a.status, 403, "{uri} {headers:?}: {}", a.body());
                assert!(a.header("location").is_none(), "{uri} {headers:?}");
                assert!(
                    cookie_value(&a, "__Host-inklkunn-oidc").is_none(),
                    "{uri} {headers:?}: no login was started"
                );
                assert!(
                    a.body().contains("only when you open its address yourself"),
                    "{uri}: {}",
                    a.body()
                );
            }
        }
    }
    // A navigation, with or without the headers, is served.
    for headers in [
        &[][..],
        &[
            ("sec-fetch-mode", "navigate"),
            ("sec-fetch-dest", "document"),
        ][..],
        &[("sec-fetch-mode", "navigate")][..],
    ] {
        crate::provider::start(&r, "/-/sign-in/oidc", headers).await;
        crate::provider::start(&r, &invite_start, headers).await;
    }
}

// ------------------------------------------------------------------------------------ G4, G5, G8

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::http::HeaderMap;
use inklkunn_core::Principal;
use inklkunn_store::Role;
use inklkunn_web::{Identify, Services, SessionView, WriteIdentity};

/// An identity that counts what is asked of it, and may be made to fail the way a store outage does.
struct Spy {
    inner: Arc<dyn Identify>,
    principal: AtomicUsize,
    for_write: AtomicUsize,
    /// Pretend the session store cannot be read: no session is seen, and a write is refused (closed).
    outage: bool,
}

impl Identify for Spy {
    fn principal(&self, headers: &HeaderMap) -> Principal {
        self.principal.fetch_add(1, Ordering::SeqCst);
        if self.outage {
            return Principal::Anonymous;
        }
        self.inner.principal(headers)
    }

    fn may_be_signed_in(&self, headers: &HeaderMap) -> bool {
        self.inner.may_be_signed_in(headers)
    }

    fn session(&self, headers: &HeaderMap) -> Option<SessionView> {
        if self.outage {
            return None;
        }
        self.inner.session(headers)
    }

    fn for_write(&self, headers: &HeaderMap, csrf: Option<&str>) -> WriteIdentity {
        self.for_write.fetch_add(1, Ordering::SeqCst);
        if self.outage {
            return WriteIdentity::CsrfMismatch;
        }
        self.inner.for_write(headers, csrf)
    }
}

fn spied(r: &mut Real, outage: bool) -> Arc<Spy> {
    let spy = Arc::new(Spy {
        inner: r.w.services.identity.clone(),
        principal: AtomicUsize::new(0),
        for_write: AtomicUsize::new(0),
        outage,
    });
    r.w.services = Arc::new(Services {
        identity: spy.clone(),
        ..(*r.w.services).clone()
    });
    spy
}

/// G4: a session the layer could not read is not "signed out". The layer asked "is there a live session?" with a
/// call whose failure read as "no", and passed the request on; the handler then asked again on its own.
#[tokio::test]
async fn a_session_that_could_not_be_read_is_a_refusal_not_a_visitor() {
    let mut r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    let spy = spied(&mut r, true);
    let a = r
        .call(
            "POST",
            "/-/inbox",
            &[("cookie", s.cookie.as_str()), SAME_SITE],
            "",
        )
        .await;
    assert_eq!(a.status, 403, "{}", a.body());
    assert!(
        a.body().contains("expired"),
        "the form expired: {}",
        a.body()
    );
    assert_eq!(
        spy.principal.load(Ordering::SeqCst),
        0,
        "no handler ran to ask again"
    );
}

/// G4: the layer decides once and the handler is told. Nothing resolves the credential a second time.
#[tokio::test]
async fn the_layer_decides_once_and_a_write_handler_asks_nothing_more() {
    let mut r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    let token = r
        .auth
        .issue_token(
            &alice,
            inklkunn_store::Scope::Admin,
            "t",
            3600,
            crate::common::NOW,
        )
        .unwrap();
    let spy = spied(&mut r, false);
    // A form with the session's token: the layer asks with no token (the session is live), then with it.
    let a = r.call("POST", "/-/inbox", &s.headers(), "").await;
    assert_eq!(a.status, 303, "{}", a.body());
    assert_eq!(
        spy.principal.load(Ordering::SeqCst),
        0,
        "the handler asked nothing"
    );
    assert_eq!(spy.for_write.load(Ordering::SeqCst), 2);
    // A bearer token: judged once, by the layer.
    spy.for_write.store(0, Ordering::SeqCst);
    let bearer = format!("Bearer {}", token.secret.as_str());
    let a = r
        .call(
            "POST",
            "/garden/-/api/v1/proposals",
            &[
                ("authorization", bearer.as_str()),
                ("content-type", "application/json"),
            ],
            "{}",
        )
        .await;
    assert_ne!(a.status, 401, "the token was good: {}", a.body());
    assert_eq!(spy.principal.load(Ordering::SeqCst), 0);
    assert_eq!(
        spy.for_write.load(Ordering::SeqCst),
        1,
        "the endpoint did not ask again"
    );
    // A read asks once (it used to ask twice: once to find the wiki, once for the page).
    let a = r
        .get("/garden/-/proposals", &[("cookie", s.cookie.as_str())])
        .await;
    assert_eq!(a.status, 200, "{}", a.body());
    assert_eq!(spy.principal.load(Ordering::SeqCst), 1);
}

/// G5: a request that presents a bearer token is judged by the token alone, and signing out is no exception.
#[tokio::test]
async fn signing_out_with_a_bearer_token_does_nothing_to_a_cookie_session() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    let token = r
        .auth
        .issue_token(
            &alice,
            inklkunn_store::Scope::Read,
            "t",
            3600,
            crate::common::NOW,
        )
        .unwrap();
    let good = format!("Bearer {}", token.secret.as_str());
    for bearer in [good.as_str(), "Bearer not-a-token"] {
        let mut headers = s.headers();
        headers.push(("authorization", bearer));
        let a = r.call("POST", "/-/sign-out", &headers, "").await;
        assert_eq!(a.status, 303, "{bearer}: {}", a.body());
        assert!(
            cookie_line(&a, "__Host-inklkunn-session").is_none(),
            "{bearer}: the cookie was not touched"
        );
        assert_eq!(
            r.get("/-/account", &[("cookie", &s.cookie)]).await.status,
            200,
            "{bearer}: the session lives on"
        );
    }
    // With no token it still signs out.
    let a = r.call("POST", "/-/sign-out", &s.headers(), "").await;
    assert!(cookie_line(&a, "__Host-inklkunn-session").is_some());
    assert_eq!(
        r.get("/-/account", &[("cookie", &s.cookie)]).await.status,
        303
    );
}

/// A post of a page that makes a session, from a browser that holds `session_cookie`.
async fn post_over_a_session(
    r: &Real,
    action: &str,
    page_uri: &str,
    session_cookie: &str,
    fields: &[(&str, &str)],
) -> crate::common::Answer {
    let page = r.get(page_uri, &[]).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}; {session_cookie}");
    let mut all = vec![("csrf", token.as_str())];
    all.extend_from_slice(fields);
    r.call(
        "POST",
        action,
        &[SAME_SITE, ("cookie", cookie.as_str())],
        &form(&all),
    )
    .await
}

/// G8: signing in while signed in ends the session presented.
#[tokio::test]
async fn signing_in_while_signed_in_ends_the_session_presented() {
    let r = real(joining(inklkunn_auth::JoinPolicy::Anyone));
    r.person("alice", "Alice", Some(Role::Contributor));
    r.person("bob", "Bob", None);
    let alices = r.sign_in("alice").await;
    assert_eq!(
        r.get("/-/account", &[("cookie", &alices.cookie)])
            .await
            .status,
        200
    );
    // Bob signs in on the same browser: alice's session is over, on the server.
    let done = post_over_a_session(
        &r,
        "/-/sign-in",
        "/-/sign-in",
        &alices.cookie,
        &[("login", "bob"), ("password", PASSWORD), ("next", "/")],
    )
    .await;
    assert_eq!(done.status, 303, "{}", done.body());
    assert_eq!(
        r.get("/-/account", &[("cookie", &alices.cookie)])
            .await
            .status,
        303,
        "alice's session did not outlive bob's sign-in"
    );
    let bobs = format!(
        "__Host-inklkunn-session={}",
        cookie_value(&done, "__Host-inklkunn-session").unwrap()
    );
    assert_eq!(r.get("/-/account", &[("cookie", &bobs)]).await.status, 200);
    // A refused sign-in leaves the session it was made over alone.
    let refused = post_over_a_session(
        &r,
        "/-/sign-in",
        "/-/sign-in",
        &bobs,
        &[("login", "bob"), ("password", "wrong"), ("next", "/")],
    )
    .await;
    assert_eq!(refused.status, 401);
    assert_eq!(r.get("/-/account", &[("cookie", &bobs)]).await.status, 200);
    // So does making an account (joining) while signed in.
    let joined = post_over_a_session(
        &r,
        "/-/join",
        "/-/join",
        &bobs,
        &[
            ("login", "carol"),
            ("name", "Carol"),
            ("password", PASSWORD),
        ],
    )
    .await;
    assert_eq!(joined.status, 303, "{}", joined.body());
    assert_eq!(
        r.get("/-/account", &[("cookie", &bobs)]).await.status,
        303,
        "bob's session did not outlive carol's joining"
    );
}
