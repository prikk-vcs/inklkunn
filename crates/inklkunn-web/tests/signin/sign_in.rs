//! The sign-in page, the refusal, the throttles, the cookie, the header and sign-out.

use std::sync::atomic::Ordering;

use inklkunn_auth::{AuthConfig, JoinPolicy, ProxyPolicy, ThrottlePolicy};
use inklkunn_store::{AccountStore, Role};

use crate::common::NOW;
use crate::harness::*;

/// A sign-in as the page sends it: the double-submit token in the cookie and in the field.
async fn try_sign_in(
    r: &Real,
    login: &str,
    password: &str,
    extra: &[(&str, &str)],
) -> crate::common::Answer {
    let page = r.get("/-/sign-in", &[]).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}");
    let body = form(&[
        ("csrf", &token),
        ("login", login),
        ("password", password),
        ("next", "/"),
    ]);
    let mut headers = vec![SAME_SITE, ("cookie", cookie.as_str())];
    headers.extend_from_slice(extra);
    r.call("POST", "/-/sign-in", &headers, &body).await
}

#[tokio::test]
async fn the_sign_in_page_is_a_form_with_a_double_submit_token_and_no_script() {
    let r = real(config());
    let a = r.get("/-/sign-in", &[]).await;
    assert_eq!(a.status, 200);
    let body = a.body();
    assert!(body.contains("<h1>Sign in</h1>"), "{body}");
    for field in ["name=\"login\"", "name=\"password\"", "type=\"password\""] {
        assert!(body.contains(field), "{field}: {body}");
    }
    assert!(body.contains("autocomplete=\"current-password\""), "{body}");
    assert!(!body.contains("<script"), "{body}");
    // The token is in the form and, as a __Host- cookie, in the browser; never cached.
    let line = cookie_line(&a, "__Host-inklkunn-csrf").expect("the cookie");
    assert!(
        line.contains("; Path=/; Secure; HttpOnly; SameSite=Strict") && !line.contains("Domain"),
        "{line}"
    );
    assert_eq!(
        csrf_field(&body).as_deref(),
        cookie_value(&a, "__Host-inklkunn-csrf").as_deref()
    );
    assert_eq!(a.header("cache-control"), Some("no-store"));
    assert!(body.contains("noindex"), "not indexed: {body}");
    // Two visits do not share a token.
    let b = r.get("/-/sign-in", &[]).await;
    assert_ne!(
        cookie_value(&a, "__Host-inklkunn-csrf"),
        cookie_value(&b, "__Host-inklkunn-csrf")
    );
}

#[tokio::test]
async fn the_sign_in_page_shows_only_what_exists() {
    // Joining is off by default: one sentence, no Join.
    let r = real(config());
    let body = r.get("/-/sign-in", &[]).await.body();
    assert!(
        body.contains("Ask the person who runs this wiki for an invitation."),
        "{body}"
    );
    assert!(
        !body.contains("Join this wiki") && !body.contains("/-/join"),
        "{body}"
    );
    // A provider alone does not open a password door.
    let r = real(joining(JoinPolicy::Provider));
    let body = r.get("/-/sign-in", &[]).await.body();
    assert!(
        body.contains("Ask the person who runs this wiki for an invitation."),
        "{body}"
    );
    assert!(!body.contains("/-/join"), "{body}");
    // Open to anyone: Join, and not the sentence.
    let r = real(joining(JoinPolicy::Anyone));
    let body = r.get("/-/sign-in", &[]).await.body();
    assert!(
        body.contains("href=\"/-/join\">Join this wiki</a>"),
        "{body}"
    );
    assert!(
        !body.contains("Ask the person who runs this wiki"),
        "{body}"
    );
}

#[tokio::test]
async fn a_right_name_and_password_sets_the_session_cookie_and_returns_to_the_page() {
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let page = r.get("/-/sign-in?next=/garden/Home", &[]).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    assert!(page.body().contains("name=\"next\" value=\"/garden/Home\""));
    let a = r
        .call(
            "POST",
            "/-/sign-in",
            &[
                SAME_SITE,
                ("cookie", &format!("__Host-inklkunn-csrf={token}")),
            ],
            &form(&[
                ("csrf", &token),
                ("login", " Alice "),
                ("password", PASSWORD),
                ("next", "/garden/Home"),
            ]),
        )
        .await;
    assert_eq!(a.status, 303, "{}", a.body());
    assert_eq!(a.header("location"), Some("/garden/Home"));
    let line = cookie_line(&a, "__Host-inklkunn-session").expect("a session cookie");
    // Exactly the attributes the design names: no Domain, no looser SameSite, an end.
    assert!(
        line.contains("; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=")
            && !line.contains("Domain"),
        "{line}"
    );
    let cleared = cookie_line(&a, "__Host-inklkunn-csrf").expect("the one-shot cookie is removed");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
}

#[tokio::test]
async fn the_header_names_who_is_signed_in_and_the_way_out_and_a_signed_out_visitor_is_offered_the_way_in()
 {
    let r = real(config());
    r.person("alice", "Alice <b>", Some(Role::Contributor));
    let signed_out = r.get("/garden/Home", &[]).await.body();
    assert!(
        signed_out.contains("<a href=\"/-/sign-in?next=/garden/Home\">Sign in</a>"),
        "{signed_out}"
    );
    assert!(!signed_out.contains("Sign out"), "{signed_out}");
    let s = r.sign_in("alice").await;
    let home = r.get("/garden/Home", &[("cookie", &s.cookie)]).await.body();
    assert!(
        home.contains("Signed in as Alice &#60;b&#62;"),
        "the name, escaped: {home}"
    );
    assert!(home.contains("href=\"/-/account\""), "{home}");
    assert!(
        home.contains(&format!(
            "<form method=\"post\" action=\"/-/sign-out\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button type=\"submit\">Sign out</button></form>",
            s.csrf
        )),
        "{home}"
    );
    assert!(!home.contains(">Sign in</a>"), "{home}");
    // No script anywhere in the page.
    assert!(!home.contains("<script"));
}

#[tokio::test]
async fn nothing_offers_sign_in_where_it_leads_nowhere() {
    // The same wiki served without sign-in: no link, and the sign-in address is the generic not-found page.
    let w = crate::common::with_accounts(crate::common::World::new(
        "garden",
        "The Garden",
        &[(
            "pages/Home.md",
            &crate::common::page(&["title: Home"], "Hi.\n"),
        )],
    ));
    let home = crate::common::get(&w, "/garden/Home").await.body();
    assert!(
        !home.contains("Sign in") && !home.contains("/-/sign-in"),
        "{home}"
    );
    for path in ["/-/sign-in", "/-/join", "/-/account", "/-/invite/inv_x"] {
        let a = crate::common::get(&w, path).await;
        assert_eq!(a.status, 404, "{path}");
    }
}

#[tokio::test]
async fn the_refusal_never_says_whether_the_account_exists() {
    let r = real(config());
    let alice = r.person("alice", "Alice", None);
    r.person("dana", "Dana", None);
    r.auth
        .disable(&r.person("gone", "Gone", None), NOW)
        .unwrap();
    let normal = |html: String| {
        // The typed name is echoed back and the token is fresh each time; nothing else may differ.
        let mut out = String::new();
        let mut rest = html.as_str();
        while let Some(at) = rest.find("value=\"") {
            out.push_str(&rest[..at]);
            out.push_str("value=\"…");
            let after = &rest[at + 7..];
            rest = after.split_once('"').map_or("", |(_, tail)| tail);
        }
        out.push_str(rest);
        out
    };
    let wrong = try_sign_in(&r, "alice", "not the password at all", &[]).await;
    let unknown = try_sign_in(&r, "nobody-here", PASSWORD, &[]).await;
    let disabled = try_sign_in(&r, "gone", PASSWORD, &[]).await;
    let impossible = try_sign_in(&r, "!!", PASSWORD, &[]).await;
    for a in [&wrong, &unknown, &disabled, &impossible] {
        assert_eq!(a.status, 401, "{}", a.body());
        assert!(
            a.body()
                .contains("That sign-in name and password did not match.")
        );
        assert!(cookie_line(a, "__Host-inklkunn-session").is_none());
    }
    let base = normal(wrong.body());
    for a in [&unknown, &disabled, &impossible] {
        assert_eq!(normal(a.body()), base, "the pages differ");
    }
    // The name typed is put back (not the password).
    assert!(wrong.body().contains("value=\"alice\"") && !wrong.body().contains("not the password"));
    let _ = alice;
}

#[tokio::test]
async fn every_sign_in_that_gets_past_the_form_checks_does_one_verification() {
    let r = real(config());
    r.person("alice", "Alice", None);
    let before = r.hasher.verifies.load(Ordering::SeqCst);
    for (login, password) in [
        ("alice", "wrong"),
        ("nobody", PASSWORD),
        ("alice", PASSWORD),
    ] {
        try_sign_in(&r, login, password, &[]).await;
    }
    assert_eq!(r.hasher.verifies.load(Ordering::SeqCst) - before, 3);
}

#[tokio::test]
async fn a_form_without_the_sign_in_token_or_from_another_site_is_refused_before_any_password_is_checked()
 {
    let r = real(config());
    r.person("alice", "Alice", None);
    let before = r.hasher.verifies.load(Ordering::SeqCst);
    let page = r.get("/-/sign-in", &[]).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}");
    let good = |t: &str| form(&[("csrf", t), ("login", "alice"), ("password", PASSWORD)]);
    // No cookie, a cookie that is another token, a field that is another token, no field, an empty one.
    type Case<'a> = (&'a str, Vec<(&'a str, &'a str)>, String);
    let cases: Vec<Case<'_>> = vec![
        ("no cookie", vec![SAME_SITE], good(&token)),
        (
            "another cookie",
            vec![SAME_SITE, ("cookie", "__Host-inklkunn-csrf=other")],
            good(&token),
        ),
        (
            "another field",
            vec![SAME_SITE, ("cookie", &cookie)],
            good("other"),
        ),
        (
            "no field",
            vec![SAME_SITE, ("cookie", &cookie)],
            form(&[("login", "alice"), ("password", PASSWORD)]),
        ),
        (
            "empty both",
            vec![SAME_SITE, ("cookie", "__Host-inklkunn-csrf=")],
            good(""),
        ),
    ];
    for (what, headers, body) in cases {
        let a = r.call("POST", "/-/sign-in", &headers, &body).await;
        assert_eq!(a.status, 403, "{what}: {}", a.body());
        assert!(a.body().contains("The form has expired"), "{what}");
        assert!(
            cookie_line(&a, "__Host-inklkunn-session").is_none(),
            "{what}"
        );
    }
    // The right token from another site.
    let a = r
        .call(
            "POST",
            "/-/sign-in",
            &[("sec-fetch-site", "cross-site"), ("cookie", &cookie)],
            &good(&token),
        )
        .await;
    assert_eq!(a.status, 403);
    assert!(
        a.body().contains("coming from a page of this site"),
        "{}",
        a.body()
    );
    assert_eq!(
        r.hasher.verifies.load(Ordering::SeqCst),
        before,
        "no password was checked"
    );
    // And the right one works.
    let a = r
        .call(
            "POST",
            "/-/sign-in",
            &[SAME_SITE, ("cookie", &cookie)],
            &good(&token),
        )
        .await;
    assert_eq!(a.status, 303);
}

#[tokio::test]
async fn too_many_failures_for_a_name_lock_it_even_for_the_right_password_and_say_when_to_come_back()
 {
    let r = real(AuthConfig {
        login_throttle: ThrottlePolicy {
            max_failures: 3,
            lock_secs: 600,
            forget_secs: 600,
            max_tracked: 100,
        },
        ..config()
    });
    r.person("alice", "Alice", None);
    for _ in 0..3 {
        assert_eq!(try_sign_in(&r, "alice", "wrong", &[]).await.status, 401);
    }
    let before = r.hasher.verifies.load(Ordering::SeqCst);
    let locked = try_sign_in(&r, "alice", PASSWORD, &[]).await;
    assert_eq!(locked.status, 429);
    assert!(
        locked.body().contains("too many attempts"),
        "{}",
        locked.body()
    );
    assert!(
        locked
            .header("retry-after")
            .is_some_and(|s| s.parse::<i64>().is_ok_and(|n| n > 0))
    );
    assert!(cookie_line(&locked, "__Host-inklkunn-session").is_none());
    assert_eq!(
        r.hasher.verifies.load(Ordering::SeqCst),
        before,
        "a lock costs no verification"
    );
    // A name that does not exist locks the same way, so a lock is no oracle.
    for _ in 0..3 {
        try_sign_in(&r, "nobody-here", "wrong", &[]).await;
    }
    assert_eq!(
        try_sign_in(&r, "nobody-here", "wrong", &[]).await.status,
        429
    );
}

#[tokio::test]
async fn the_address_that_is_throttled_is_the_peers_unless_a_declared_proxy_says_otherwise() {
    let policy = ThrottlePolicy {
        max_failures: 3,
        lock_secs: 600,
        forget_secs: 600,
        max_tracked: 100,
    };
    let r = real(AuthConfig {
        address_throttle: policy,
        login_throttle: ThrottlePolicy {
            max_failures: 1000,
            ..policy
        },
        proxies: ProxyPolicy::trusting(&["10.0.0.0/8"]).unwrap(),
        ..config()
    });
    r.person("alice", "Alice", None);
    let ask = |peer: &'static str, forwarded: &'static str, login: String| {
        let r = &r;
        async move {
            let page = r.get("/-/sign-in", &[]).await;
            let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
            let cookie = format!("__Host-inklkunn-csrf={token}");
            let mut headers = vec![SAME_SITE, ("cookie", cookie.as_str())];
            if !forwarded.is_empty() {
                headers.push(("x-forwarded-for", forwarded));
            }
            r.call_from(
                Some(format!("{peer}:50000").parse().unwrap()),
                "POST",
                "/-/sign-in",
                &headers,
                &form(&[("csrf", &token), ("login", &login), ("password", "wrong")]),
            )
            .await
            .status
        }
    };
    // A stranger who forges X-Forwarded-For to look like many clients is still one address.
    let statuses = [
        ask("203.0.113.5", "198.51.100.1", "a1".into()).await,
        ask("203.0.113.5", "198.51.100.2", "a2".into()).await,
        ask("203.0.113.5", "198.51.100.3", "a3".into()).await,
        ask("203.0.113.5", "198.51.100.4", "a4".into()).await,
    ];
    assert_eq!(
        statuses,
        [401, 401, 401, 429],
        "the forged header bought nothing"
    );
    // Behind the declared proxy, each client the proxy reports is its own address.
    let behind = [
        ask("10.0.0.1", "198.51.100.10", "b1".into()).await,
        ask("10.0.0.1", "198.51.100.11", "b2".into()).await,
        ask("10.0.0.1", "198.51.100.12", "b3".into()).await,
        ask("10.0.0.1", "198.51.100.13", "b4".into()).await,
        ask("10.0.0.1", "198.51.100.10", "b5".into()).await,
    ];
    assert_eq!(
        behind, [401; 5],
        "four clients behind the proxy, one failure each"
    );
}

#[tokio::test]
async fn a_return_address_is_a_local_path_or_the_start_page() {
    let r = real(config());
    r.person("alice", "Alice", None);
    for (next, want) in [
        ("//evil.example/x", "/"),
        ("https://evil.example/", "/"),
        ("/garden/Home", "/garden/Home"),
        ("/\\evil.example", "/"),
        ("", "/"),
    ] {
        let page = r.get("/-/sign-in", &[]).await;
        let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
        let a = r
            .call(
                "POST",
                "/-/sign-in",
                &[
                    SAME_SITE,
                    ("cookie", &format!("__Host-inklkunn-csrf={token}")),
                ],
                &form(&[
                    ("csrf", &token),
                    ("login", "alice"),
                    ("password", PASSWORD),
                    ("next", next),
                ]),
            )
            .await;
        assert_eq!(a.status, 303, "{next}");
        assert_eq!(a.header("location"), Some(want), "{next}");
    }
}

#[tokio::test]
async fn signing_out_ends_the_session_and_removes_the_cookie_and_needs_the_sessions_token() {
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    // The page of the link: a button, nothing changed.
    let page = r.get("/-/sign-out", &[("cookie", &s.cookie)]).await;
    assert_eq!(page.status, 200);
    assert!(page.body().contains("Sign out"), "{}", page.body());
    assert_eq!(
        r.get("/-/account", &[("cookie", &s.cookie)]).await.status,
        200
    );
    // Without the token, and with another session's, it is refused and the session lives on.
    let other = r.sign_in("alice").await;
    for headers in [
        vec![("cookie", s.cookie.as_str()), SAME_SITE],
        vec![
            ("cookie", s.cookie.as_str()),
            SAME_SITE,
            (inklkunn_web::CSRF_HEADER, other.csrf.as_str()),
        ],
    ] {
        let a = r.call("POST", "/-/sign-out", &headers, "").await;
        assert_eq!(a.status, 403, "{}", a.body());
        assert_eq!(
            r.get("/-/account", &[("cookie", &s.cookie)]).await.status,
            200
        );
    }
    // From another site, with the token: the second wall.
    let a = r
        .call(
            "POST",
            "/-/sign-out",
            &[
                ("cookie", &s.cookie),
                ("sec-fetch-site", "cross-site"),
                (inklkunn_web::CSRF_HEADER, &s.csrf),
            ],
            "",
        )
        .await;
    assert_eq!(a.status, 403);
    assert_eq!(
        r.get("/-/account", &[("cookie", &s.cookie)]).await.status,
        200
    );
    // With it: out.
    let a = r.call("POST", "/-/sign-out", &s.headers(), "").await;
    assert_eq!(a.status, 303);
    assert_eq!(a.header("location"), Some("/"));
    let cleared = cookie_line(&a, "__Host-inklkunn-session").unwrap();
    assert!(
        cleared.starts_with("__Host-inklkunn-session=;") && cleared.contains("Max-Age=0"),
        "{cleared}"
    );
    let after = r.get("/-/account", &[("cookie", &s.cookie)]).await;
    assert_eq!(
        after.status, 303,
        "the session is over on the server, not only in the browser"
    );
    assert_eq!(after.header("location"), Some("/-/sign-in?next=/-/account"));
    // The other browser is still signed in.
    assert_eq!(
        r.get("/-/account", &[("cookie", &other.cookie)])
            .await
            .status,
        200
    );
    // Signing out with no session is a trip to the start page, not an error.
    let a = r.call("POST", "/-/sign-out", &[SAME_SITE], "").await;
    assert_eq!(a.status, 303);
}

#[tokio::test]
async fn an_expired_session_is_no_session() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    // Made eight days ago: over a day ago, since a session lasts seven.
    let then = r
        .auth
        .sign_in(
            "alice",
            PASSWORD,
            &r.auth.client_address("203.0.113.1".parse().unwrap(), None),
            NOW - 8 * 86_400,
        )
        .unwrap();
    let cookie = format!("__Host-inklkunn-session={}", then.session.secret().as_str());
    let _ = alice;
    let home = r.get("/garden/Home", &[("cookie", &cookie)]).await.body();
    assert!(
        home.contains("Sign in") && !home.contains("Signed in as"),
        "{home}"
    );
    assert_eq!(
        r.get("/-/account", &[("cookie", &cookie)]).await.status,
        303
    );
    // A cookie that is not even shaped like one costs a lookup of nothing and is the same.
    let junk = r
        .get("/-/account", &[("cookie", "__Host-inklkunn-session=short")])
        .await;
    assert_eq!(junk.status, 303);
    // A name the session cookie is listed twice under is believed under neither.
    let s = r.sign_in("alice").await;
    let doubled = format!("{}; {}", s.cookie, s.cookie);
    assert_eq!(
        r.get("/-/account", &[("cookie", &doubled)]).await.status,
        303
    );
}

#[tokio::test]
async fn an_account_that_is_disabled_is_signed_out_everywhere_at_once() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    r.auth.disable(&alice, NOW).unwrap();
    assert_eq!(
        r.get("/-/account", &[("cookie", &s.cookie)]).await.status,
        303
    );
    assert!(r.account(&alice).disabled);
    let _ = r.w.store.account(&alice);
}
