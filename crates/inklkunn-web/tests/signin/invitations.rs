//! Invitations and joining through the pages.

use inklkunn_auth::JoinPolicy;
use inklkunn_auth::invitation::DEFAULT_LIFETIME_SECS;
use inklkunn_store::{AccountStore, LoginName, Role, RoleStore};

use crate::common::{Answer, NOW};
use crate::harness::*;

/// What the form of a page that makes an account sends: the token from the page and the cookie, and the fields.
async fn submit(
    r: &Real,
    action: &str,
    page_uri: &str,
    fields: &[(&str, &str)],
    extra: &[(&str, &str)],
) -> Answer {
    let page = r.get(page_uri, &[]).await;
    assert_eq!(page.status, 200, "{}", page.body());
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}");
    let mut all = vec![("csrf", token.as_str())];
    all.extend_from_slice(fields);
    let mut headers = vec![SAME_SITE, ("cookie", cookie.as_str())];
    headers.extend_from_slice(extra);
    r.call("POST", action, &headers, &form(&all)).await
}

fn link(r: &Real, wiki_role: Option<Role>, lifetime: i64) -> (String, String) {
    let grant = wiki_role.map(|role| (r.w.wiki.id.clone(), role));
    let issued = r.auth.invite(grant, lifetime, NOW).unwrap();
    let secret = issued.secret.as_str().to_owned();
    (format!("/-/invite/{secret}"), secret)
}

#[tokio::test]
async fn an_invitation_makes_an_account_once_signs_the_person_in_and_grants_the_role_it_named() {
    let r = real(config());
    let (uri, secret) = link(&r, Some(Role::Admin), DEFAULT_LIFETIME_SECS);
    let page = r.get(&uri, &[]).await;
    assert_eq!(page.status, 200);
    let body = page.body();
    // The secret is in the form's address and nowhere else: not in the language buttons' return address.
    assert_eq!(body.matches(secret.as_str()).count(), 1, "{body}");
    for want in [
        "<h1>You are invited</h1>",
        "name=\"login\"",
        "name=\"name\"",
        "name=\"password\"",
        "autocomplete=\"new-password\"",
        "At least 10 characters.",
        "Make my account",
    ] {
        assert!(body.contains(want), "{want}: {body}");
    }
    // The role is not told to whoever opens the link.
    assert!(!body.to_lowercase().contains("admin"), "{body}");
    assert!(!body.contains("<script"));
    assert_eq!(page.header("cache-control"), Some("no-store"));
    let a = submit(
        &r,
        &uri,
        &uri,
        &[
            ("login", "Carol"),
            ("name", "Carol Q."),
            ("password", PASSWORD),
        ],
        &[],
    )
    .await;
    assert_eq!(a.status, 303, "{}", a.body());
    assert_eq!(a.header("location"), Some("/-/account"));
    let secret = cookie_value(&a, "__Host-inklkunn-session").expect("signed in");
    let carol =
        r.w.store
            .account_by_login(&LoginName::parse("carol").unwrap())
            .unwrap()
            .unwrap();
    assert_eq!(carol.display_name.as_str(), "Carol Q.");
    assert_eq!(
        r.w.store.role(&carol.id, &r.w.wiki.id).unwrap(),
        Some(Role::Admin)
    );
    // The session is live: the account page answers.
    let cookie = format!("__Host-inklkunn-session={secret}");
    assert_eq!(
        r.get("/-/account", &[("cookie", &cookie)]).await.status,
        200
    );
}

#[tokio::test]
async fn a_link_that_was_used_one_that_ended_and_one_that_never_was_are_one_page() {
    let r = real(config());
    let (used, _) = link(&r, None, DEFAULT_LIFETIME_SECS);
    let (ended, _) = {
        let issued = r.auth.invite(None, 10, NOW - 1000).unwrap();
        (format!("/-/invite/{}", issued.secret.as_str()), ())
    };
    let never = format!("/-/invite/inv_{}", "A".repeat(43));
    let junk = "/-/invite/not-an-invitation".to_owned();
    // Use one up.
    let a = submit(
        &r,
        &used,
        &used,
        &[
            ("login", "first"),
            ("name", "First"),
            ("password", PASSWORD),
        ],
        &[],
    )
    .await;
    assert_eq!(a.status, 303);
    let mut pages = Vec::new();
    for uri in [&used, &ended, &never, &junk] {
        let a = r.get(uri, &[]).await;
        assert_eq!(a.status, 404, "{uri}");
        assert!(a.body().contains("This invitation cannot be used"), "{uri}");
        assert!(
            a.body()
                .contains("Ask the person who runs this wiki for a new one."),
            "{uri}"
        );
        assert!(!a.body().contains("name=\"password\""), "{uri}: no form");
        pages.push(a.body());
    }
    // Byte for byte the same page, but for the language buttons' echo of the address.
    let normal = |html: &str, uri: &str| html.replace(uri, "ADDRESS");
    let first = normal(pages.first().unwrap(), &used);
    for (page, uri) in pages.iter().zip([&used, &ended, &never, &junk]) {
        assert_eq!(normal(page, uri), first, "{uri}");
    }
    // Posting to a used link makes nothing either.
    let a = r
        .call(
            "POST",
            &used,
            &[SAME_SITE],
            &form(&[
                ("csrf", "x"),
                ("login", "second"),
                ("name", "Second"),
                ("password", PASSWORD),
            ]),
        )
        .await;
    assert_eq!(a.status, 403, "the token is checked first");
    assert!(
        r.w.store
            .account_by_login(&LoginName::parse("second").unwrap())
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_refusal_spends_nothing_and_says_what_to_change() {
    let r = real(config());
    r.person("alice", "Alice", None);
    let (uri, _) = link(&r, None, DEFAULT_LIFETIME_SECS);
    for (fields, status, said) in [
        (
            vec![
                ("login", "alice"),
                ("name", "Imposter"),
                ("password", PASSWORD),
            ],
            409,
            "That sign-in name is taken.",
        ),
        (
            vec![("login", "a"), ("name", "Short"), ("password", PASSWORD)],
            422,
            "A sign-in name has 3 to 64 characters.",
        ),
        (
            vec![
                ("login", "new person"),
                ("name", "Space"),
                ("password", PASSWORD),
            ],
            422,
            "A sign-in name uses letters a to z",
        ),
        (
            vec![("login", "dana"), ("name", ""), ("password", PASSWORD)],
            422,
            "A name has 1 to 120 characters.",
        ),
        (
            vec![("login", "dana"), ("name", "Dana"), ("password", "short")],
            422,
            "A password needs at least 10 characters.",
        ),
    ] {
        let a = submit(&r, &uri, &uri, &fields, &[]).await;
        assert_eq!(a.status, status, "{fields:?}: {}", a.body());
        assert!(a.body().contains(said), "{fields:?}: {}", a.body());
        // What was typed is put back, except the password.
        assert!(!a.body().contains(PASSWORD));
        assert!(a.header("cache-control") == Some("no-store"));
    }
    // The same link still works, and is a fresh form with a fresh token each time.
    let a = submit(
        &r,
        &uri,
        &uri,
        &[("login", "dana"), ("name", "Dana"), ("password", PASSWORD)],
        &[],
    )
    .await;
    assert_eq!(a.status, 303, "{}", a.body());
}

#[tokio::test]
async fn an_invitation_form_without_its_token_or_from_another_site_makes_nothing() {
    let r = real(config());
    let (uri, _) = link(&r, None, DEFAULT_LIFETIME_SECS);
    let fields = [("login", "dana"), ("name", "Dana"), ("password", PASSWORD)];
    let a = r.call("POST", &uri, &[SAME_SITE], &form(&fields)).await;
    assert_eq!(a.status, 403);
    assert!(a.body().contains("The form has expired"));
    let page = r.get(&uri, &[]).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let mut all = vec![("csrf", token.as_str())];
    all.extend_from_slice(&fields);
    let a = r
        .call(
            "POST",
            &uri,
            &[
                ("sec-fetch-site", "cross-site"),
                ("cookie", &format!("__Host-inklkunn-csrf={token}")),
            ],
            &form(&all),
        )
        .await;
    assert_eq!(a.status, 403);
    assert!(
        r.w.store
            .account_by_login(&LoginName::parse("dana").unwrap())
            .unwrap()
            .is_none()
    );
    assert!(
        r.auth
            .invitation_usable(uri.trim_start_matches("/-/invite/"), NOW)
            .unwrap(),
        "the link is untouched"
    );
}

#[tokio::test]
async fn joining_is_closed_by_default_and_the_page_says_so_without_a_form() {
    for policy in [JoinPolicy::Off, JoinPolicy::Provider] {
        let r = real(joining(policy));
        let a = r.get("/-/join", &[]).await;
        assert_eq!(a.status, 404, "{policy:?}");
        let body = a.body();
        assert!(body.contains("Joining is not open"), "{body}");
        assert!(
            body.contains("Ask the person who runs this wiki for an invitation."),
            "{body}"
        );
        assert!(!body.contains("name=\"password\""), "{body}");
        // A post makes nothing, even with a token: there is none to get from a closed page, and the
        // refusal comes first either way.
        let a = r
            .call(
                "POST",
                "/-/join",
                &[SAME_SITE, ("cookie", "__Host-inklkunn-csrf=t")],
                &form(&[
                    ("csrf", "t"),
                    ("login", "dana"),
                    ("name", "Dana"),
                    ("password", PASSWORD),
                ]),
            )
            .await;
        assert_eq!(a.status, 404, "{policy:?}");
        assert!(
            r.w.store
                .account_by_login(&LoginName::parse("dana").unwrap())
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn where_anyone_may_join_a_person_makes_an_account_with_no_role_and_is_signed_in() {
    let r = real(joining(JoinPolicy::Anyone));
    let page = r.get("/-/join", &[]).await;
    assert_eq!(page.status, 200);
    assert!(
        page.body().contains("<h1>Join this wiki</h1>"),
        "{}",
        page.body()
    );
    let a = submit(
        &r,
        "/-/join",
        "/-/join",
        &[("login", "erin"), ("name", "Erin"), ("password", PASSWORD)],
        &[],
    )
    .await;
    assert_eq!(a.status, 303, "{}", a.body());
    assert_eq!(a.header("location"), Some("/-/account"));
    assert!(cookie_value(&a, "__Host-inklkunn-session").is_some());
    let erin =
        r.w.store
            .account_by_login(&LoginName::parse("erin").unwrap())
            .unwrap()
            .unwrap();
    assert_eq!(
        erin.created_at, NOW,
        "the new-account quotas count from here"
    );
    assert_eq!(r.w.store.role(&erin.id, &r.w.wiki.id).unwrap(), None);
    // The name is taken now.
    let a = submit(
        &r,
        "/-/join",
        "/-/join",
        &[("login", "erin"), ("name", "Other"), ("password", PASSWORD)],
        &[],
    )
    .await;
    assert_eq!(a.status, 409);
}
