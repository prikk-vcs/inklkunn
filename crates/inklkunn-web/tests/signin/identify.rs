//! **Who a request is** (the sign-in handoff's U4) and **the one check every write carries** (handoff 004 P6),
//! through the real router and the real authenticator.
//!
//! The probe for a read is `/-/wikis`: the wiki here is restricted, so the list names it for an account that
//! holds a role in it (alice) and for nobody else (bob, an anonymous visitor). The probe for a write is
//! `POST /-/inbox` (mark everything read), which an account may do and a visitor is told needs one.

use inklkunn_auth::AuthConfig;
use inklkunn_core::Guard;
use inklkunn_store::{Role, Scope, Visibility, WikiStore};
use inklkunn_web::routes::application_table;

use crate::common::{Answer, NOW};
use crate::harness::*;

const GARDEN: &str = "The Garden";

struct Cast {
    r: Real,
    alice: inklkunn_store::AccountId,
    bob: inklkunn_store::AccountId,
}

fn cast() -> Cast {
    let r = real_with(AuthConfig { ..config() }, Visibility::Restricted);
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let bob = r.person("bob", "Bob", None);
    Cast { r, alice, bob }
}

fn bearer(secret: &str) -> String {
    format!("Bearer {secret}")
}

impl Cast {
    fn token(&self, who: &inklkunn_store::AccountId, scope: Scope) -> String {
        self.r
            .auth
            .issue_token(who, scope, "t", 3600, NOW)
            .unwrap()
            .secret
            .as_str()
            .to_owned()
    }

    async fn sees_the_garden(&self, headers: &[(&str, &str)]) -> bool {
        let a = self.r.get("/-/wikis", headers).await;
        assert_eq!(a.status, 200);
        a.body().contains(GARDEN)
    }

    /// The write probe: did it mark the inbox read (a redirect back to it)?
    async fn marks_read(&self, headers: &[(&str, &str)]) -> Answer {
        self.r.call("POST", "/-/inbox", headers, "").await
    }
}

#[tokio::test]
async fn a_session_cookie_alone_is_the_account_of_the_session() {
    let c = cast();
    let s = c.r.sign_in("alice").await;
    assert!(c.sees_the_garden(&[("cookie", &s.cookie)]).await);
    assert!(!c.sees_the_garden(&[]).await, "and nobody else does");
    let bobs = c.r.sign_in("bob").await;
    assert!(!c.sees_the_garden(&[("cookie", &bobs.cookie)]).await);
}

#[tokio::test]
async fn a_bearer_token_alone_is_the_account_of_the_token_with_its_scope() {
    let c = cast();
    let token = c.token(&c.alice, Scope::Read);
    assert!(
        c.sees_the_garden(&[("authorization", &bearer(&token))])
            .await
    );
    // A token is not sent by a browser by itself: a write needs no CSRF token, only the token.
    let a = c
        .marks_read(&[SAME_SITE, ("authorization", &bearer(&token))])
        .await;
    assert_eq!(a.status, 303, "{}", a.body());
}

#[tokio::test]
async fn when_both_are_presented_the_token_wins_and_the_cookie_is_ignored() {
    let c = cast();
    let alices = c.r.sign_in("alice").await;
    let bobs = c.r.sign_in("bob").await;
    let alice_token = c.token(&c.alice, Scope::Read);
    let bob_token = c.token(&c.bob, Scope::Read);
    // Alice's cookie with Bob's token is Bob; Bob's cookie with Alice's token is Alice.
    assert!(
        !c.sees_the_garden(&[
            ("cookie", &alices.cookie),
            ("authorization", &bearer(&bob_token))
        ])
        .await
    );
    assert!(
        c.sees_the_garden(&[
            ("cookie", &bobs.cookie),
            ("authorization", &bearer(&alice_token))
        ])
        .await
    );
    // A write the same way: Bob's token with Alice's cookie *and her token* is Bob's write, with no CSRF
    // token asked and none believed.
    let bobs_header = bearer(&bob_token);
    let mut headers = alices.headers();
    headers.push(("authorization", &bobs_header));
    let a = c.marks_read(&headers).await;
    assert_eq!(a.status, 303, "{}", a.body());
    // Alice's cookie, with a CSRF token, is not narrowed or widened by a read-scope token either.
    assert!(
        c.sees_the_garden(&[
            ("cookie", &alices.cookie),
            ("authorization", &bearer(&alice_token))
        ])
        .await
    );
}

#[tokio::test]
async fn an_invalid_token_with_a_valid_cookie_is_refused_and_never_falls_back_to_the_cookie() {
    let c = cast();
    let alices = c.r.sign_in("alice").await;
    for bad in [
        bearer(&format!("ink_{}", "A".repeat(43))),
        bearer("ink_short"),
        "Bearer".to_owned(),
        "Bearer a b".to_owned(),
        "bearer  ".to_owned(),
    ] {
        // A read is an anonymous visitor's: not alice's.
        assert!(
            !c.sees_the_garden(&[("cookie", &alices.cookie), ("authorization", &bad)])
                .await,
            "{bad:?}"
        );
        // A form write is the visitor's, who is told an account is needed: nothing was marked.
        let mut headers = alices.headers();
        headers.push(("authorization", &bad));
        let a = c.marks_read(&headers).await;
        assert_eq!(a.status, 403, "{bad:?}: {}", a.body());
        assert!(
            a.body().contains("An account is needed"),
            "{bad:?}: {}",
            a.body()
        );
        // The write API says invalid token, and not "forbidden" or "authentication required".
        let mut headers = alices.headers();
        headers.push(("authorization", &bad));
        headers.push(("content-type", "application/json"));
        headers.push(("if-match", "\"x\""));
        let a =
            c.r.call("POST", "/garden/-/api/v1/proposals", &headers, "{}")
                .await;
        assert_eq!(a.status, 401, "{bad:?}: {}", a.body());
        assert!(a.body().contains("invalid-token"), "{bad:?}: {}", a.body());
    }
    // Another scheme in Authorization (a proxy's Basic) is not a token of ours: the cookie is believed.
    assert!(
        c.sees_the_garden(&[
            ("cookie", &alices.cookie),
            ("authorization", "Basic dXNlcjpwdw==")
        ])
        .await
    );
}

#[tokio::test]
async fn an_expired_session_a_revoked_token_an_expired_token_and_a_disabled_account_are_nobody() {
    let c = cast();
    // A session made eight days ago has ended.
    let old =
        c.r.auth
            .sign_in(
                "alice",
                PASSWORD,
                &c.r.auth
                    .client_address("203.0.113.1".parse().unwrap(), None),
                NOW - 8 * 86_400,
            )
            .unwrap();
    let cookie = format!("__Host-inklkunn-session={}", old.session.secret().as_str());
    assert!(!c.sees_the_garden(&[("cookie", &cookie)]).await);
    let a = c.marks_read(&[SAME_SITE, ("cookie", &cookie)]).await;
    assert_eq!(
        a.status, 403,
        "no session: the visitor is told an account is needed"
    );
    assert!(a.body().contains("An account is needed"), "{}", a.body());
    // A token.
    let issued =
        c.r.auth
            .issue_token(&c.alice, Scope::Admin, "t", 3600, NOW)
            .unwrap();
    let live = bearer(issued.secret.as_str());
    assert!(c.sees_the_garden(&[("authorization", &live)]).await);
    c.r.auth.revoke_token(&c.alice, &issued.id, NOW).unwrap();
    assert!(
        !c.sees_the_garden(&[("authorization", &live)]).await,
        "revoked"
    );
    // One that ended an hour ago.
    let ended =
        c.r.auth
            .issue_token(&c.alice, Scope::Admin, "t", 1800, NOW - 3600)
            .unwrap();
    assert!(
        !c.sees_the_garden(&[("authorization", &bearer(ended.secret.as_str()))])
            .await,
        "expired"
    );
    // The write API says so for each.
    for secret in [issued.secret.as_str(), ended.secret.as_str()] {
        let a =
            c.r.call(
                "POST",
                "/garden/-/api/v1/proposals",
                &[
                    ("authorization", &bearer(secret)),
                    ("content-type", "application/json"),
                    ("if-match", "\"x\""),
                ],
                "{}",
            )
            .await;
        assert_eq!(a.status, 401);
        assert!(a.body().contains("invalid-token"), "{}", a.body());
    }
    // A disabled account's token and session.
    let tok = c.token(&c.alice, Scope::Read);
    let s = c.r.sign_in("alice").await;
    assert!(c.sees_the_garden(&[("authorization", &bearer(&tok))]).await);
    c.r.auth.disable(&c.alice, NOW).unwrap();
    assert!(!c.sees_the_garden(&[("authorization", &bearer(&tok))]).await);
    assert!(!c.sees_the_garden(&[("cookie", &s.cookie)]).await);
}

#[tokio::test]
async fn a_write_with_a_session_cookie_needs_that_sessions_token() {
    let c = cast();
    let alices = c.r.sign_in("alice").await;
    let other = c.r.sign_in("alice").await;
    let bobs = c.r.sign_in("bob").await;
    let cookie = alices.cookie.as_str();
    let wrong: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("no token", vec![("cookie", cookie), SAME_SITE]),
        (
            "another session of the same account",
            vec![
                ("cookie", cookie),
                SAME_SITE,
                (inklkunn_web::CSRF_HEADER, &other.csrf),
            ],
        ),
        (
            "another account's session",
            vec![
                ("cookie", cookie),
                SAME_SITE,
                (inklkunn_web::CSRF_HEADER, &bobs.csrf),
            ],
        ),
        (
            "an empty token",
            vec![
                ("cookie", cookie),
                SAME_SITE,
                (inklkunn_web::CSRF_HEADER, ""),
            ],
        ),
        (
            "a token that is a prefix of the right one",
            vec![
                ("cookie", cookie),
                SAME_SITE,
                (inklkunn_web::CSRF_HEADER, alices.csrf.get(..10).unwrap()),
            ],
        ),
    ];
    for (what, headers) in wrong {
        let a = c.marks_read(&headers).await;
        assert_eq!(a.status, 403, "{what}");
        assert!(
            a.body().contains("The form has expired"),
            "{what}: {}",
            a.body()
        );
    }
    // In the form's field, the right token passes, and so does the header.
    let a =
        c.r.call(
            "POST",
            "/-/inbox",
            &[("cookie", cookie), SAME_SITE],
            &form(&[("csrf", &alices.csrf)]),
        )
        .await;
    assert_eq!(a.status, 303, "{}", a.body());
    let a = c.marks_read(&alices.headers()).await;
    assert_eq!(a.status, 303);
    // The header beats a body that says something else, as the form field is the fallback.
    let a =
        c.r.call(
            "POST",
            "/-/inbox",
            &alices.headers(),
            &form(&[("csrf", "nonsense")]),
        )
        .await;
    assert_eq!(a.status, 303);
    // The token is checked even when the form is from another site: either wall refuses.
    let a =
        c.r.call(
            "POST",
            "/-/inbox",
            &[("cookie", cookie), ("sec-fetch-site", "cross-site")],
            "",
        )
        .await;
    assert_eq!(a.status, 403);
}

#[tokio::test]
async fn the_token_is_checked_before_any_wiki_is_looked_up_so_a_refusal_tells_wikis_apart_not_at_all()
 {
    let c = cast();
    c.r.w
        .store
        .create(inklkunn_store::NewWiki {
            visibility: Visibility::Public,
            ..inklkunn_store::NewWiki::new(
                inklkunn_store::Slug::new("open").unwrap(),
                inklkunn_store::DisplayName::new("Open").unwrap(),
            )
        })
        .unwrap();
    let alices = c.r.sign_in("alice").await;
    let proposal = "019a0000-0000-7000-8000-00000000dddd";
    let mut answers = Vec::new();
    for wiki in ["garden", "open", "nowhere"] {
        for path in [
            format!("/{wiki}/-/proposals/new"),
            format!("/{wiki}/-/proposals/{proposal}/comment"),
            format!("/{wiki}/-/proposals/{proposal}/decide"),
            format!("/{wiki}/-/proposals/{proposal}/accept"),
            format!("/{wiki}/-/proposals/{proposal}/decline"),
            format!("/{wiki}/-/proposals/{proposal}/ask"),
            format!("/{wiki}/-/proposals/{proposal}/withdraw"),
            format!("/{wiki}/-/proposals/{proposal}/edit"),
            format!("/{wiki}/-/watch/Home"),
            format!("/{wiki}/-/verification"),
        ] {
            let a =
                c.r.call(
                    "POST",
                    &path,
                    &[("cookie", &alices.cookie), SAME_SITE],
                    "op=submit",
                )
                .await;
            assert_eq!(a.status, 403, "{path}");
            assert!(a.body().contains("The form has expired"), "{path}");
            answers.push((path, a.body()));
        }
    }
    let first = answers.first().unwrap().1.clone();
    for (path, body) in &answers {
        assert_eq!(*body, first, "{path}: the refusal depends on the wiki");
    }
    // And the API: a cookie write with no token is `forbidden`, for a wiki that is there and one that is not.
    let mut api = Vec::new();
    for wiki in ["garden", "nowhere"] {
        let a =
            c.r.call(
                "POST",
                &format!("/{wiki}/-/api/v1/proposals"),
                &[
                    ("cookie", &alices.cookie),
                    SAME_SITE,
                    ("content-type", "application/json"),
                ],
                "{}",
            )
            .await;
        assert_eq!(a.status, 403, "{wiki}");
        assert!(a.body().contains("forbidden"), "{}", a.body());
        api.push(a.body());
    }
    assert_eq!(api.first(), api.get(1));
}

#[tokio::test]
async fn no_write_route_of_the_registry_escapes_the_token() {
    let c = cast();
    let alices = c.r.sign_in("alice").await;
    // The posts that need no session token, each for its reason (csrf.rs), and no others.
    let excused = ["/-/lang", "/-/sign-in", "/-/join", "/-/invite/{secret}"];
    let mut walked = Vec::new();
    for e in application_table().unwrap().entries() {
        if !e.methods.contains(&"POST") {
            continue;
        }
        if excused.contains(&e.path) {
            assert!(
                matches!(e.guard, Guard::Public { .. }),
                "{}: an excused post is public",
                e.path
            );
            continue;
        }
        let uri = e
            .path
            .replace("{slug}", "garden")
            .replace("{*title}", "Home")
            .replace("{id}", "019a0000-0000-7000-8000-00000000dddd")
            .replace("{secret}", "x");
        let a =
            c.r.call(
                "POST",
                &uri,
                &[
                    ("cookie", &alices.cookie),
                    SAME_SITE,
                    ("content-type", "application/json"),
                ],
                "{}",
            )
            .await;
        assert_eq!(
            a.status,
            403,
            "{} accepted a cookie with no token: {}",
            e.path,
            a.body()
        );
        assert!(
            a.body().contains("The form has expired") || a.body().contains("forbidden"),
            "{}: {}",
            e.path,
            a.body()
        );
        walked.push(e.path);
    }
    // The pages' writes and the API's, and the account page's own.
    for expected in [
        "/{slug}/-/proposals/new",
        "/{slug}/-/proposals/{id}/accept",
        "/-/inbox",
        "/{slug}/-/watch/{*title}",
        "/{slug}/-/verification",
        "/-/sign-out",
        "/-/account/tokens",
    ] {
        assert!(
            walked.contains(&expected),
            "{expected} was not walked: {walked:?}"
        );
    }
    assert!(walked.len() >= 20, "{}: {walked:?}", walked.len());
    // And with the token every one of them gets past the layer (it is then the handler's to answer).
    for path in &walked {
        let uri = path
            .replace("{slug}", "garden")
            .replace("{*title}", "Home")
            .replace("{id}", "019a0000-0000-7000-8000-00000000dddd");
        let mut headers = alices.headers();
        headers.push(("content-type", "application/json"));
        let a = c.r.call("POST", &uri, &headers, "{}").await;
        assert!(
            !(a.status == 403 && a.body().contains("The form has expired")),
            "{path} refused a request that carried the token"
        );
    }
}

#[tokio::test]
async fn a_visitor_with_no_session_has_nothing_to_forge_and_the_language_form_needs_no_token() {
    let c = cast();
    // Anonymous: the handler says an account is needed; it is not an expired form.
    let a = c.marks_read(&[SAME_SITE]).await;
    assert_eq!(a.status, 403);
    assert!(a.body().contains("An account is needed"), "{}", a.body());
    // A signed-in person's language choice is a cookie of their own: no token.
    let s = c.r.sign_in("alice").await;
    let a =
        c.r.call(
            "POST",
            "/-/lang",
            &[("cookie", &s.cookie), SAME_SITE],
            &form(&[("lang", "ja"), ("next", "/")]),
        )
        .await;
    assert_eq!(a.status, 303, "{}", a.body());
}

fn headers_of(pairs: &[(&str, &str)]) -> axum::http::HeaderMap {
    let mut h = axum::http::HeaderMap::new();
    for (k, v) in pairs {
        h.append(
            axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
            axum::http::HeaderValue::from_str(v).unwrap(),
        );
    }
    h
}

/// The question a write asks, answered for every combination of credentials the design names.
#[tokio::test]
async fn the_identity_of_a_write_is_the_token_alone_or_the_session_with_its_token_or_nobody() {
    use inklkunn_web::WriteIdentity as W;
    let c = cast();
    let who = |w: &W| match w {
        W::Principal(p) => p.account_id().cloned(),
        _ => None,
    };
    let identity = c.r.w.services.identity.clone();
    let alices = c.r.sign_in("alice").await;
    let bobs = c.r.sign_in("bob").await;
    let alice_token = bearer(&c.token(&c.alice, Scope::Read));
    let bob_token = bearer(&c.token(&c.bob, Scope::Read));
    let junk = bearer(&format!("ink_{}", "A".repeat(43)));
    let ask =
        |pairs: &[(&str, &str)], csrf: Option<&str>| identity.for_write(&headers_of(pairs), csrf);

    // Nothing: a visitor, who has nothing to forge.
    assert_eq!(
        ask(&[], None),
        W::Principal(inklkunn_core::Principal::Anonymous)
    );
    // A session needs its token: the cookie and the token together, or neither is believed.
    let cookie = ("cookie", alices.cookie.as_str());
    assert_eq!(
        who(&ask(&[cookie], Some(&alices.csrf))),
        Some(c.alice.clone())
    );
    for (what, csrf) in [
        ("none", None),
        ("empty", Some("")),
        ("another account's", Some(bobs.csrf.as_str())),
        ("nonsense", Some("nonsense")),
    ] {
        assert_eq!(ask(&[cookie], csrf), W::CsrfMismatch, "{what}");
    }
    // A cookie for a session that is over is no session: nothing to forge.
    let old =
        c.r.auth
            .sign_in(
                "alice",
                PASSWORD,
                &c.r.auth
                    .client_address("203.0.113.1".parse().unwrap(), None),
                NOW - 8 * 86_400,
            )
            .unwrap();
    let old_cookie = format!("__Host-inklkunn-session={}", old.session.secret().as_str());
    assert_eq!(
        ask(&[("cookie", &old_cookie)], None),
        W::Principal(inklkunn_core::Principal::Anonymous)
    );
    // A token judges the request alone: no token of the CSRF kind is asked, and a cookie is not looked at.
    assert_eq!(
        who(&ask(&[("authorization", &alice_token)], None)),
        Some(c.alice.clone())
    );
    assert_eq!(
        who(&ask(
            &[cookie, ("authorization", &bob_token)],
            Some(&alices.csrf)
        )),
        Some(c.bob.clone()),
        "Bob's token with Alice's cookie and Alice's token is Bob"
    );
    // A token that does not authenticate is `invalid token`, never the cookie's account, with or without the
    // right CSRF token beside it.
    for csrf in [None, Some(alices.csrf.as_str())] {
        assert_eq!(
            ask(&[cookie, ("authorization", &junk)], csrf),
            W::InvalidToken
        );
        assert_eq!(
            ask(&[cookie, ("authorization", "Bearer")], csrf),
            W::InvalidToken
        );
        assert_eq!(
            ask(&[cookie, ("authorization", "Bearer a b")], csrf),
            W::InvalidToken
        );
    }
    // Another scheme is not a token of ours: the session and its token are believed as without it.
    assert_eq!(
        who(&ask(
            &[cookie, ("authorization", "Basic dXNlcjpwdw==")],
            Some(&alices.csrf)
        )),
        Some(c.alice.clone())
    );
    assert_eq!(
        ask(&[cookie, ("authorization", "Basic dXNlcjpwdw==")], None),
        W::CsrfMismatch
    );
    // The page's session: only for a request a token does not judge.
    assert!(
        identity
            .session(&headers_of(&[cookie]))
            .is_some_and(|s| s.account == c.alice && s.csrf == alices.csrf)
    );
    assert!(
        identity
            .session(&headers_of(&[cookie, ("authorization", &junk)]))
            .is_none()
    );
    assert!(
        identity
            .session(&headers_of(&[("authorization", &alice_token)]))
            .is_none()
    );
    assert!(identity.session(&headers_of(&[])).is_none());
    assert!(!identity.may_be_signed_in(&headers_of(&[])));
    assert!(!identity.may_be_signed_in(&headers_of(&[cookie, ("authorization", &junk)])));
    assert!(identity.may_be_signed_in(&headers_of(&[cookie])));
}
