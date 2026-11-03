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

/// H2: a proxy the operator declared whose header cannot be read puts every client on the proxy's address, the
/// same failure; the server says so, once, and says nothing while the header is readable.
#[tokio::test]
async fn a_declared_proxy_whose_header_cannot_be_read_is_said_once_at_run_time() {
    let r = real(AuthConfig {
        proxies: ProxyPolicy::trusting(&["127.0.0.1"]).unwrap(),
        ..config()
    });
    let said = || {
        r.w.logged()
            .into_iter()
            .filter(|l| l.contains("X-Forwarded-For"))
            .collect::<Vec<_>>()
    };
    // A readable header from the proxy, and a request from a peer that is not the proxy: nothing to say.
    failed_sign_in(&r, "127.0.0.1", Some("198.51.100.1")).await;
    failed_sign_in(&r, "203.0.113.5", Some("unknown")).await;
    assert!(said().is_empty(), "{:?}", said());
    // The first unreadable one, then any number more (ports, `unknown`, none at all).
    failed_sign_in(&r, "127.0.0.1", Some("unknown")).await;
    failed_sign_in(&r, "127.0.0.1", Some("198.51.100.1:5555")).await;
    failed_sign_in(&r, "127.0.0.1", None).await;
    let lines = said();
    assert_eq!(lines.len(), 1, "once: {lines:?}");
    let line = lines.first().unwrap();
    assert!(
        line.contains("INKLKUNN_TRUSTED_PROXIES")
            && line.contains("every client behind it counts as one address"),
        "{line}"
    );
    // The loopback line of G1 is another matter and was not said: a proxy is declared.
    assert!(!line.contains("no trusted proxy is declared"), "{line}");
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
    /// Decide a write as this account whatever the credential says (the layer and the page disagreeing).
    impostor: Option<inklkunn_store::AccountId>,
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
        if let Some(account) = &self.impostor {
            return WriteIdentity::Principal(Principal::session(
                &inklkunn_core::IdentityAuthority::for_tests(),
                account.clone(),
            ));
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
        impostor: None,
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

// ------------------------------------------------------------------------------------ G6, G10

use crate::harness::Session;

async fn post_form(
    r: &Real,
    s: &Session,
    uri: &str,
    fields: &[(&str, &str)],
) -> crate::common::Answer {
    r.call("POST", uri, &s.headers(), &form(fields)).await
}

const NEW_PASSWORD: &str = "a different password altogether";

fn token_fields(password: &str) -> Vec<(&str, &str)> {
    vec![
        ("label", "bot"),
        ("scope", "admin"),
        ("days", "365"),
        ("password", password),
    ]
}

/// G6: a token outlives the session, so a stolen session alone must not be able to make one.
#[tokio::test]
async fn making_a_token_asks_for_the_password_again() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Admin));
    let s = r.sign_in("alice").await;
    // The form asks for it.
    let page = r.get("/-/account", &[("cookie", &s.cookie)]).await.body();
    assert!(
        page.contains("id=\"token-password\"")
            && page.contains("Asked again because a token keeps"),
        "{page}"
    );
    // A session alone: no password, a wrong one.
    for password in ["", "a wrong password entirely"] {
        let a = post_form(&r, &s, "/-/account/tokens", &token_fields(password)).await;
        assert_eq!(a.status, 401, "{password:?}: {}", a.body());
        assert!(
            a.body()
                .contains("The password was not right, so no token was made."),
            "{}",
            a.body()
        );
        assert!(!a.body().contains("Your new token"));
        assert!(r.auth.tokens(&alice).unwrap().is_empty());
    }
    // The right one.
    let a = post_form(&r, &s, "/-/account/tokens", &token_fields(PASSWORD)).await;
    assert_eq!(a.status, 200, "{}", a.body());
    assert!(a.body().contains("Your new token"));
    assert_eq!(r.auth.tokens(&alice).unwrap().len(), 1);
    // The password field is never put back in the page.
    assert!(!a.body().contains(PASSWORD));
}

/// ... and guessing it there costs what guessing it at the sign-in page costs.
#[tokio::test]
async fn guessing_the_password_to_make_a_token_is_throttled_like_signing_in() {
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Admin));
    let s = r.sign_in("alice").await;
    let mut statuses = Vec::new();
    for _ in 0..7 {
        let a = post_form(
            &r,
            &s,
            "/-/account/tokens",
            &token_fields("wrong wrong wrong"),
        )
        .await;
        statuses.push(a.status.as_u16());
        if a.status == 429 {
            assert!(a.header("retry-after").is_some());
        }
    }
    assert_eq!(statuses, [401, 401, 401, 401, 401, 429, 429]);
    // The right password is refused while locked, here and at the sign-in page.
    let a = post_form(&r, &s, "/-/account/tokens", &token_fields(PASSWORD)).await;
    assert_eq!(a.status, 429);
    let page = r.get("/-/sign-in", &[]).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}");
    let a = r
        .call(
            "POST",
            "/-/sign-in",
            &[SAME_SITE, ("cookie", cookie.as_str())],
            &form(&[("csrf", &token), ("login", "alice"), ("password", PASSWORD)]),
        )
        .await;
    assert_eq!(a.status, 429);
}

async fn provider_only_session(r: &Real, started: i64) -> (inklkunn_store::AccountId, Session) {
    let account = r.auth.register_external("Sso Person", started).unwrap();
    let secret = r.auth.start_session_for_test(&account.id, started);
    let csrf = r
        .auth
        .principal_for_session(&secret, crate::common::NOW)
        .unwrap()
        .unwrap()
        .csrf
        .as_str()
        .to_owned();
    let session = Session {
        cookie: format!("__Host-inklkunn-session={secret}"),
        csrf,
    };
    (account.id, session)
}

/// The account page acts on the account of the session it draws; if the layer decided the request is someone
/// else's, it is not a request to act on.
#[tokio::test]
async fn the_account_page_does_not_act_when_the_layer_and_the_page_disagree_about_who_it_is() {
    let mut r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let bob = r.person("bob", "Bob", None);
    let s = r.sign_in("alice").await;
    let spy = Arc::new(Spy {
        inner: r.w.services.identity.clone(),
        principal: AtomicUsize::new(0),
        for_write: AtomicUsize::new(0),
        outage: false,
        impostor: Some(bob),
    });
    r.w.services = Arc::new(Services {
        identity: spy,
        ..(*r.w.services).clone()
    });
    let a = post_form(&r, &s, "/-/account/name", &[("name", "Mallory")]).await;
    assert_eq!(a.status, 403, "{}", a.body());
    let page = r.get("/-/account", &[("cookie", &s.cookie)]).await.body();
    assert!(page.contains("value=\"Alice\""), "{page}");
}

/// G6, for an account with no password: a fresh sign-in (ten minutes) stands in for it.
#[tokio::test]
async fn an_account_with_no_password_makes_a_token_only_within_ten_minutes_of_signing_in() {
    let r = real(config());
    let now = crate::common::NOW;
    // Signed in five minutes ago: fine, and the form says why there is no password field.
    let (account, fresh) = provider_only_session(&r, now - 300).await;
    let page = r
        .get("/-/account", &[("cookie", &fresh.cookie)])
        .await
        .body();
    assert!(
        !page.contains("id=\"token-password\"") && page.contains("has no password"),
        "{page}"
    );
    let a = post_form(&r, &fresh, "/-/account/tokens", &token_fields("")).await;
    assert_eq!(a.status, 200, "{}", a.body());
    assert_eq!(r.auth.tokens(&account).unwrap().len(), 1);
    // Signed in an hour ago: asked to sign in again, and no token is made.
    let (account, stale) = provider_only_session(&r, now - 3600).await;
    let a = post_form(&r, &stale, "/-/account/tokens", &token_fields("")).await;
    assert_eq!(a.status, 403, "{}", a.body());
    assert!(
        a.body()
            .contains("Sign out, sign in again, and make the token within ten minutes."),
        "{}",
        a.body()
    );
    assert!(r.auth.tokens(&account).unwrap().is_empty());
}

/// G10, H5: the account page changes the password.
#[tokio::test]
async fn the_account_page_changes_the_password_ends_every_session_and_every_token_and_sets_a_new_cookie()
 {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let here = r.sign_in("alice").await;
    let there = r.sign_in("alice").await;
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
    let page = r
        .get("/-/account", &[("cookie", &here.cookie)])
        .await
        .body();
    for want in [
        "<h2 id=\"password-heading\">Password</h2>",
        "action=\"/-/account/password\"",
        "name=\"current\" type=\"password\" autocomplete=\"current-password\"",
        "name=\"new\" type=\"password\" autocomplete=\"new-password\"",
        "Change password",
    ] {
        assert!(page.contains(want), "{want}: {page}");
    }
    let a = post_form(
        &r,
        &here,
        "/-/account/password",
        &[("current", PASSWORD), ("new", NEW_PASSWORD)],
    )
    .await;
    assert_eq!(a.status, 303, "{}", a.body());
    assert_eq!(a.header("location"), Some("/-/account?done=password"));
    // The answer sets a new `__Host-` session cookie: this browser continues with it.
    let set = a.header("set-cookie").unwrap_or_default().to_owned();
    assert!(
        set.starts_with("__Host-inklkunn-session=")
            && set.contains("Secure")
            && set.contains("HttpOnly")
            && set.contains("SameSite=Strict")
            && set.contains("Path=/"),
        "{set}"
    );
    let fresh = cookie_value(&a, "__Host-inklkunn-session").expect("a new session cookie");
    let fresh_cookie = format!("__Host-inklkunn-session={fresh}");
    assert_ne!(
        fresh_cookie, here.cookie,
        "a session id never survives the change"
    );
    assert!(
        a.header("cache-control")
            .is_some_and(|c| c.contains("no-store")),
        "{:?}",
        a.header("cache-control")
    );
    // The old cookie, the one that made the change, is over, as is the other; so the token.
    for old in [&here, &there] {
        assert_eq!(
            r.get("/-/account", &[("cookie", &old.cookie)]).await.status,
            303,
            "an ended session is signed out"
        );
    }
    let by_token = r
        .get(
            "/-/inbox",
            &[(
                "authorization",
                &format!("Bearer {}", token.secret.as_str()),
            )],
        )
        .await;
    assert_ne!(by_token.status, 200, "the token no longer works");
    // The new cookie is signed in, is told, and the forms of the following page carry the new CSRF token.
    let after = r
        .get("/-/account?done=password", &[("cookie", &fresh_cookie)])
        .await;
    assert_eq!(after.status, 200);
    assert!(
        after.body().contains("Your password was changed."),
        "{}",
        after.body()
    );
    let csrf = csrf_field(&after.body()).expect("the new session's token is in the page");
    assert_ne!(
        csrf, here.csrf,
        "the old session's token is not carried over"
    );
    let continued = Session {
        cookie: fresh_cookie.clone(),
        csrf,
    };
    // A write with the new session and its token is accepted; with the old token it is not.
    let name = post_form(&r, &continued, "/-/account/name", &[("name", "Alice B.")]).await;
    assert_eq!(name.status, 303, "{}", name.body());
    let stale = Session {
        cookie: fresh_cookie,
        csrf: here.csrf.clone(),
    };
    let refused = post_form(&r, &stale, "/-/account/name", &[("name", "Mallory")]).await;
    assert_eq!(refused.status, 403, "{}", refused.body());
    // The new password signs in and the old does not.
    let page = r.get("/-/sign-in", &[]).await;
    let csrf = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={csrf}");
    let sign_in = |password: &'static str| {
        let (r, csrf, cookie) = (&r, csrf.clone(), cookie.clone());
        async move {
            r.call(
                "POST",
                "/-/sign-in",
                &[SAME_SITE, ("cookie", cookie.as_str())],
                &form(&[("csrf", &csrf), ("login", "alice"), ("password", password)]),
            )
            .await
            .status
        }
    };
    assert_eq!(sign_in(PASSWORD).await, 401);
    assert_eq!(sign_in(NEW_PASSWORD).await, 303);
}

#[tokio::test]
async fn a_password_change_that_is_refused_changes_nothing() {
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let here = r.sign_in("alice").await;
    let there = r.sign_in("alice").await;
    for (current, new, status, says) in [
        (
            "wrong",
            NEW_PASSWORD,
            401,
            "The current password was not right.",
        ),
        (PASSWORD, "short", 422, "At least 10 characters."),
        (PASSWORD, &"x".repeat(2000), 422, "too long"),
    ] {
        let a = post_form(
            &r,
            &here,
            "/-/account/password",
            &[("current", current), ("new", new)],
        )
        .await;
        assert_eq!(a.status, status, "{}", a.body());
        assert!(a.body().contains(says), "{says}: {}", a.body());
        assert!(!a.body().contains(NEW_PASSWORD));
        assert_eq!(
            r.get("/-/account", &[("cookie", &there.cookie)])
                .await
                .status,
            200,
            "the other session was not ended"
        );
    }
    // The old password still works.
    let again = r.sign_in("alice").await;
    assert!(!again.cookie.is_empty());
    // Nothing without the session's token, or from another site.
    for headers in [
        vec![("cookie", here.cookie.as_str()), SAME_SITE],
        vec![
            ("cookie", here.cookie.as_str()),
            ("sec-fetch-site", "cross-site"),
            (inklkunn_web::CSRF_HEADER, here.csrf.as_str()),
        ],
    ] {
        let a = r
            .call(
                "POST",
                "/-/account/password",
                &headers,
                &form(&[("current", PASSWORD), ("new", NEW_PASSWORD)]),
            )
            .await;
        assert_eq!(a.status, 403);
    }
    assert!(r.sign_in_still_works_with(PASSWORD).await);
}

#[tokio::test]
async fn guessing_the_current_password_is_throttled_and_a_provider_account_has_no_form() {
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let here = r.sign_in("alice").await;
    let mut statuses = Vec::new();
    for _ in 0..7 {
        let a = post_form(
            &r,
            &here,
            "/-/account/password",
            &[("current", "wrong wrong wrong"), ("new", NEW_PASSWORD)],
        )
        .await;
        statuses.push(a.status.as_u16());
    }
    assert_eq!(statuses, [401, 401, 401, 401, 401, 429, 429]);
    let (_, sso) = provider_only_session(&r, crate::common::NOW - 60).await;
    let page = r.get("/-/account", &[("cookie", &sso.cookie)]).await.body();
    assert!(
        page.contains("This account has no password: it signs in through a provider.")
            && !page.contains("action=\"/-/account/password\""),
        "{page}"
    );
    // A post anyway changes nothing.
    let a = post_form(
        &r,
        &sso,
        "/-/account/password",
        &[("current", ""), ("new", NEW_PASSWORD)],
    )
    .await;
    assert_eq!(a.status, 401);
}

impl Real {
    /// Can alice still sign in with `password`?
    async fn sign_in_still_works_with(&self, password: &str) -> bool {
        let page = self.get("/-/sign-in", &[]).await;
        let csrf = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
        let cookie = format!("__Host-inklkunn-csrf={csrf}");
        self.call(
            "POST",
            "/-/sign-in",
            &[SAME_SITE, ("cookie", cookie.as_str())],
            &form(&[("csrf", &csrf), ("login", "alice"), ("password", password)]),
        )
        .await
        .status
            == 303
    }
}
