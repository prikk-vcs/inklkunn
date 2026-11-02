//! The account page: the name, the sessions, the tokens.

use inklkunn_store::Role;

use crate::common::NOW;
use crate::harness::*;

async fn post(r: &Real, s: &Session, uri: &str, fields: &[(&str, &str)]) -> crate::common::Answer {
    r.call("POST", uri, &s.headers(), &form(fields)).await
}

#[tokio::test]
async fn a_visitor_with_no_session_is_sent_to_sign_in_and_back() {
    let r = real(config());
    let a = r.get("/-/account", &[]).await;
    assert_eq!(a.status, 303);
    assert_eq!(a.header("location"), Some("/-/sign-in?next=/-/account"));
    // Every write of the page does the same, and changes nothing.
    for uri in [
        "/-/account/name",
        "/-/account/sessions/end",
        "/-/account/tokens",
        "/-/account/tokens/revoke",
        "/-/account/password",
    ] {
        let a = r
            .call("POST", uri, &[SAME_SITE], &form(&[("name", "X")]))
            .await;
        assert_eq!(a.status, 303, "{uri}");
        assert_eq!(
            a.header("location"),
            Some("/-/sign-in?next=/-/account"),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn the_account_page_shows_the_name_the_sessions_and_the_tokens_with_a_form_for_each_act() {
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    let a = r.get("/-/account", &[("cookie", &s.cookie)]).await;
    assert_eq!(a.status, 200);
    let body = a.body();
    assert_eq!(a.header("cache-control"), Some("no-store"));
    for want in [
        "<h1>Your account</h1>",
        "value=\"Alice\"",
        "Where you are signed in",
        "This is the browser you are using.",
        "End this session",
        "API tokens",
        "You have no tokens.",
        "Make a token",
    ] {
        assert!(body.contains(want), "{want}: {body}");
    }
    assert!(!body.contains("<script"), "{body}");
    // Every form of the page carries the session's token (and the page has no other kind of form but the
    // footer's language buttons).
    let forms = body.matches("<form method=\"post\"").count();
    let carrying = body
        .matches(&format!(
            "<input type=\"hidden\" name=\"csrf\" value=\"{}\">",
            s.csrf
        ))
        .count();
    let language_buttons = body.matches("action=\"/-/lang\"").count();
    assert_eq!(forms, carrying + language_buttons, "{body}");
}

#[tokio::test]
async fn the_name_is_saved_shown_in_the_header_and_checked() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    let a = post(&r, &s, "/-/account/name", &[("name", "  Alice Ångström  ")]).await;
    assert_eq!(a.status, 303);
    assert_eq!(a.header("location"), Some("/-/account?done=name"));
    assert_eq!(r.account(&alice).display_name.as_str(), "Alice Ångström");
    let page = r
        .get("/-/account?done=name", &[("cookie", &s.cookie)])
        .await
        .body();
    assert!(page.contains("Your name was saved."), "{page}");
    assert!(page.contains("Signed in as Alice Ångström"), "{page}");
    // Any script is a name; a line break, an empty name and a name too long are not.
    for (bad, said) in [
        (
            "Line\nbreak",
            "A name cannot contain line breaks or control characters.",
        ),
        ("", "A name has 1 to 120 characters."),
        (&"x".repeat(121), "A name has 1 to 120 characters."),
    ] {
        let a = post(&r, &s, "/-/account/name", &[("name", bad)]).await;
        assert_eq!(a.status, 422, "{bad:?}");
        assert!(a.body().contains(said), "{bad:?}: {}", a.body());
        assert_eq!(r.account(&alice).display_name.as_str(), "Alice Ångström");
    }
    assert_eq!(
        post(&r, &s, "/-/account/name", &[("name", "アイコ")])
            .await
            .status,
        303
    );
    assert_eq!(r.account(&alice).display_name.as_str(), "アイコ");
}

#[tokio::test]
async fn a_session_is_listed_and_ended_by_its_owner_alone() {
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    r.person("bob", "Bob", Some(Role::Contributor));
    let phone = r.sign_in("alice").await;
    let laptop = r.sign_in("alice").await;
    let bobs = r.sign_in("bob").await;
    let page = r
        .get("/-/account", &[("cookie", &laptop.cookie)])
        .await
        .body();
    assert_eq!(
        page.matches("End this session").count(),
        2,
        "Bob's is not Alice's to see: {page}"
    );
    assert_eq!(
        page.matches("This is the browser you are using.").count(),
        1
    );
    // The handles of Alice's two sessions, in the order listed: the newest (the laptop) first.
    let handles: Vec<String> = page
        .match_indices("name=\"handle\" value=\"")
        .map(|(at, m)| page[at + m.len()..].split('"').next().unwrap().to_owned())
        .collect();
    assert_eq!(handles.len(), 2);
    let bobs_page = r
        .get("/-/account", &[("cookie", &bobs.cookie)])
        .await
        .body();
    let bobs_handle = bobs_page
        .split("name=\"handle\" value=\"")
        .nth(1)
        .and_then(|t| t.split('"').next())
        .unwrap()
        .to_owned();
    // Bob's handle in Alice's hand ends nothing of Bob's.
    let a = post(
        &r,
        &laptop,
        "/-/account/sessions/end",
        &[("handle", &bobs_handle)],
    )
    .await;
    assert_eq!(a.status, 303);
    assert_eq!(
        r.get("/-/account", &[("cookie", &bobs.cookie)])
            .await
            .status,
        200
    );
    // The phone (the other of hers): ended, and the laptop goes on.
    let a = post(
        &r,
        &laptop,
        "/-/account/sessions/end",
        &[("handle", handles.get(1).unwrap())],
    )
    .await;
    assert_eq!(a.header("location"), Some("/-/account?done=session"));
    assert_eq!(
        r.get("/-/account", &[("cookie", &phone.cookie)])
            .await
            .status,
        303
    );
    assert_eq!(
        r.get("/-/account", &[("cookie", &laptop.cookie)])
            .await
            .status,
        200
    );
    // The one in use: this browser is signed out, with its cookie removed.
    let a = post(
        &r,
        &laptop,
        "/-/account/sessions/end",
        &[("handle", handles.first().unwrap())],
    )
    .await;
    assert_eq!(a.status, 303);
    assert_eq!(a.header("location"), Some("/"));
    assert!(
        cookie_line(&a, "__Host-inklkunn-session")
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        r.get("/-/account", &[("cookie", &laptop.cookie)])
            .await
            .status,
        303
    );
}

#[tokio::test]
async fn a_token_is_made_with_a_scope_and_an_expiry_shown_once_listed_and_revoked() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    let a = post(
        &r,
        &s,
        "/-/account/tokens",
        &[
            ("label", "the build bot"),
            ("scope", "propose"),
            ("days", "7"),
            ("password", PASSWORD),
        ],
    )
    .await;
    assert_eq!(a.status, 200, "{}", a.body());
    assert_eq!(a.header("cache-control"), Some("no-store"));
    let body = a.body();
    assert!(body.contains("Your new token"), "{body}");
    let secret = body
        .split("<code>")
        .nth(1)
        .and_then(|t| t.split("</code>").next())
        .unwrap()
        .to_owned();
    assert!(secret.starts_with("ink_") && secret.len() == 47, "{secret}");
    // Once: the page afterwards lists the token by its note and never shows it again.
    let again = r.get("/-/account", &[("cookie", &s.cookie)]).await.body();
    assert!(
        !again.contains(&secret) && !again.contains("Your new token"),
        "{again}"
    );
    assert!(again.contains("the build bot"), "{again}");
    assert!(
        again.contains("Scope: Propose: also comment and propose changes"),
        "{again}"
    );
    assert!(
        again.contains("Works until 2023-11-21"),
        "seven days from NOW: {again}"
    );
    // It works as a credential, and as no more than it was made as: a probe that reads for an account.
    let inbox = r
        .get(
            "/-/inbox",
            &[("authorization", &format!("Bearer {secret}"))],
        )
        .await;
    assert_eq!(inbox.status, 200, "{}", inbox.body());
    // Revoked: gone at once, and listed as revoked.
    let id = again
        .split("name=\"id\" value=\"")
        .nth(1)
        .and_then(|t| t.split('"').next())
        .unwrap()
        .to_owned();
    let a = post(&r, &s, "/-/account/tokens/revoke", &[("id", &id)]).await;
    assert_eq!(a.header("location"), Some("/-/account?done=token"));
    let inbox = r
        .get(
            "/-/inbox",
            &[("authorization", &format!("Bearer {secret}"))],
        )
        .await;
    assert_eq!(inbox.status, 403, "a revoked token is anonymous");
    let after = r.get("/-/account", &[("cookie", &s.cookie)]).await.body();
    assert!(after.contains("Revoked 2023-11-14"), "{after}");
    assert!(
        !after.contains("name=\"id\""),
        "nothing left to revoke: {after}"
    );
    // Another account's token id revokes nothing.
    let bob = r.person("bob", "Bob", Some(Role::Contributor));
    let theirs = r
        .auth
        .issue_token(&bob, inklkunn_store::Scope::Read, "bobs", 3600, NOW)
        .unwrap();
    let a = post(
        &r,
        &s,
        "/-/account/tokens/revoke",
        &[("id", &theirs.id.to_string())],
    )
    .await;
    assert_eq!(a.status, 303);
    assert!(
        r.auth
            .principal_for_token(theirs.secret.as_str(), NOW)
            .is_ok(),
        "still Bob's and live"
    );
    let _ = alice;
}

#[tokio::test]
async fn a_token_form_with_a_scope_or_an_expiry_that_is_not_offered_makes_nothing() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    for fields in [
        vec![("label", "x"), ("scope", "root"), ("days", "7")],
        vec![("label", "x"), ("scope", "propose"), ("days", "3")],
        vec![("label", "x"), ("scope", "propose"), ("days", "-1")],
        vec![("label", "x"), ("scope", "propose")],
        vec![
            ("label", &"n".repeat(81)),
            ("scope", "propose"),
            ("days", "7"),
        ],
    ] {
        let mut fields = fields;
        fields.push(("password", PASSWORD));
        let a = post(&r, &s, "/-/account/tokens", &fields).await;
        assert_eq!(a.status, 422, "{fields:?}");
        assert!(!a.body().contains("Your new token"));
    }
    assert!(r.auth.tokens(&alice).unwrap().is_empty());
    // The scope the person chose is put back for another try.
    let a = post(
        &r,
        &s,
        "/-/account/tokens",
        &[
            ("label", "keep"),
            ("scope", "review"),
            ("days", "99"),
            ("password", PASSWORD),
        ],
    )
    .await;
    assert!(
        a.body().contains("value=\"review\" selected"),
        "{}",
        a.body()
    );
    assert!(a.body().contains("value=\"keep\""), "{}", a.body());
    // A note is optional.
    let a = post(
        &r,
        &s,
        "/-/account/tokens",
        &[
            ("label", ""),
            ("scope", "read"),
            ("days", "1"),
            ("password", PASSWORD),
        ],
    )
    .await;
    assert_eq!(a.status, 200);
    assert!(
        r.get("/-/account", &[("cookie", &s.cookie)])
            .await
            .body()
            .contains("A token with no note")
    );
}

#[tokio::test]
async fn an_account_holds_a_bounded_number_of_live_tokens_and_is_told() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    for n in 0..100 {
        r.auth
            .issue_token(
                &alice,
                inklkunn_store::Scope::Read,
                &format!("t{n}"),
                3600,
                NOW,
            )
            .unwrap();
    }
    let a = post(
        &r,
        &s,
        "/-/account/tokens",
        &[
            ("label", "one more"),
            ("scope", "read"),
            ("days", "1"),
            ("password", PASSWORD),
        ],
    )
    .await;
    assert_eq!(a.status, 422);
    assert!(
        a.body()
            .contains("You already have 100 live tokens. Revoke one first."),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_token_alone_has_no_account_page_and_makes_no_token() {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Admin));
    let token = r
        .auth
        .issue_token(&alice, inklkunn_store::Scope::Admin, "all", 3600, NOW)
        .unwrap();
    let bearer = format!("Bearer {}", token.secret.as_str());
    let a = r.get("/-/account", &[("authorization", &bearer)]).await;
    assert_eq!(a.status, 303, "a token is not a session");
    let a = r
        .call(
            "POST",
            "/-/account/tokens",
            &[SAME_SITE, ("authorization", &bearer)],
            &form(&[("label", "child"), ("scope", "admin"), ("days", "30")]),
        )
        .await;
    assert_eq!(a.status, 303);
    assert_eq!(
        r.auth.tokens(&alice).unwrap().len(),
        1,
        "no second token was made"
    );
}
