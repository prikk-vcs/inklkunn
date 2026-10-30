//! The inbox and following a page through the real router (handoff 004 T6; RFC 004 D-9): an account is needed
//! and said in words; each kind of event is a sentence in the reader's language with other people's text
//! escaped; an event about a wiki the reader can no longer read names nothing; reading is a `POST` from this
//! site; following a page is the same, answers a wiki the visitor may not read as one that is not there, and is
//! listed in the inbox with a way to stop.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::body::Body;
use axum::http::{Request, header};
use common::{Answer, NOW, World, held_proposal as held, page, tell, with_accounts};
use inklkunn_store::{
    Account, AccountId, AccountStore, DisplayName, InboxEventKind, InboxStore, Role, RoleGrant,
    RoleStore, Visibility, WatchRecord, WatchTarget,
};
use tower::ServiceExt as _;

const SAME_SITE: &[(&str, &str)] = &[("sec-fetch-site", "same-origin")];
const CROSS_SITE: &[(&str, &str)] = &[("sec-fetch-site", "cross-site")];

fn files() -> Vec<(&'static str, String)> {
    vec![
        ("pages/Home.md", page(&["title: Home"], "Welcome.\n")),
        (
            "pages/Guides/Setup.md",
            page(&["title: Guides/Setup"], "Set up.\n"),
        ),
    ]
}

fn world(visibility: Visibility, slug: &str, name: &str) -> World {
    let owned = files();
    let borrowed: Vec<(&str, &str)> = owned.iter().map(|(p, t)| (*p, t.as_str())).collect();
    with_accounts(World::with_visibility(slug, name, visibility, &borrowed))
}

fn handbook() -> World {
    world(Visibility::Public, "handbook", "The Handbook")
}

fn account(w: &World, name: &str, role: Option<Role>) -> AccountId {
    let id = AccountId::new();
    w.store
        .add_account(&Account {
            id: id.clone(),
            display_name: DisplayName::new(name).unwrap(),
            local: None,
            created_at: NOW,
            disabled: false,
        })
        .unwrap();
    if let Some(role) = role {
        w.store
            .grant(RoleGrant {
                account: id.clone(),
                wiki: w.wiki.id.clone(),
                role,
                granted_by: None,
                granted_at: NOW,
            })
            .unwrap();
    }
    id
}

async fn send(
    w: &World,
    method: &str,
    uri: &str,
    who: Option<&AccountId>,
    headers: &[(&str, &str)],
    body: &str,
) -> Answer {
    let config = inklkunn_web::WebConfig::new(
        "127.0.0.1:8080",
        "http://localhost:8080",
        "http://localhost:8081",
    )
    .unwrap();
    let site = inklkunn_web::routes::site_with(&config, Some(w.services.clone())).unwrap();
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "localhost:8080");
    if method == "POST" {
        req = req.header("content-type", "application/x-www-form-urlencoded");
    }
    if let Some(who) = who {
        req = req.header("x-test-account", who.as_str());
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = site
        .oneshot(req.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    Answer {
        status: parts.status,
        headers: parts.headers,
        bytes: axum::body::to_bytes(body, 1 << 22).await.unwrap().to_vec(),
    }
}

async fn inbox(w: &World, who: Option<&AccountId>, headers: &[(&str, &str)]) -> Answer {
    send(w, "GET", "/-/inbox", who, headers, "").await
}

#[tokio::test]
async fn a_visitor_with_no_account_is_told_in_words_that_an_account_is_needed() {
    let w = handbook();
    let author = account(&w, "Aiko", Some(Role::Contributor));
    let p = held(&w, &author, "A summary no stranger may see");
    tell(&w, &author, InboxEventKind::Declined, &p, None, NOW);
    for method in ["GET", "POST"] {
        let a = send(&w, method, "/-/inbox", None, SAME_SITE, "").await;
        assert_eq!(a.status, 403, "{method}");
        let body = a.body();
        assert!(body.contains("An account is needed"), "{body}");
        assert!(body.contains("Sign in"), "{body}");
        assert!(!body.contains("no stranger"), "{body}");
        assert!(!body.contains("The Handbook"), "{body}");
    }
    // In Japanese too.
    let a = inbox(&w, None, &[("cookie", "inklkunn_lang=ja")]).await;
    assert_eq!(a.status, 403);
    assert!(a.body().contains("アカウントが必要です"), "{}", a.body());
}

#[tokio::test]
async fn each_kind_of_event_is_a_sentence_with_other_peoples_text_escaped() {
    let w = handbook();
    let me = account(&w, "Aiko", Some(Role::Contributor));
    let them = account(&w, "Ren <b>Sato</b>", Some(Role::Maintainer));
    let p = held(&w, &me, "Fix <script>alert(1)</script> in the intro");
    for (n, kind) in InboxEventKind::ALL.into_iter().enumerate() {
        tell(&w, &me, kind, &p, Some(&them), NOW + n as i64);
    }
    let a = inbox(&w, Some(&me), &[]).await;
    assert_eq!(a.status, 200, "{}", a.body());
    let body = a.body();
    for sentence in [
        "reviewed your proposal",
        "declined your proposal",
        "asked for a revision of your proposal",
        "withdrew the proposal",
        "replaced your proposal",
        "commented in the discussion of",
        "mentioned you in the discussion of",
        "was accepted and signed into the history",
        "A page you follow in",
    ] {
        assert!(body.contains(sentence), "{sentence}: {body}");
    }
    // Other people's words are text: the name and the summary are escaped, never markup.
    assert!(!body.contains("<script>alert"), "{body}");
    assert!(!body.contains("<b>Sato"), "{body}");
    assert!(body.contains("Ren &lt;b&gt;Sato&lt;/b&gt;"), "{body}");
    // The proposal is a link to its own page, and so is the wiki.
    assert!(
        body.contains(&format!("href=\"/handbook/-/proposals/{}\"", p.id)),
        "{body}"
    );
    assert!(
        body.contains("<a href=\"/handbook\">The Handbook</a>"),
        "{body}"
    );
    // Newest first; every one is new.
    assert!(body.matches("<strong>New</strong>").count() >= 9, "{body}");
    assert!(body.contains("Mark everything as read"), "{body}");
    // The Japanese preview, when the reader chose it.
    let ja = inbox(&w, Some(&me), &[("cookie", "inklkunn_lang=ja")])
        .await
        .body();
    assert!(ja.contains("受信箱") && ja.contains("見送りました"), "{ja}");
    // Someone else's inbox is empty, and says so.
    let empty = inbox(&w, Some(&them), &[]).await.body();
    assert!(
        empty.contains("Nothing has happened yet that involves you."),
        "{empty}"
    );
    assert!(!empty.contains("reviewed your proposal"), "{empty}");
}

#[tokio::test]
async fn an_event_about_a_wiki_the_reader_can_no_longer_read_names_nothing() {
    let w = world(Visibility::Restricted, "vault", "The Vault");
    let me = account(&w, "Aiko", Some(Role::Contributor));
    let boss = account(&w, "Ben", Some(Role::Maintainer));
    let p = held(&w, &me, "The vault's secret summary");
    tell(&w, &me, InboxEventKind::Declined, &p, Some(&boss), NOW);
    let before = inbox(&w, Some(&me), &[]).await.body();
    assert!(before.contains("The vault&#39;s secret summary") || before.contains("secret summary"));
    assert!(before.contains("The Vault"), "{before}");

    // Their role is taken away: the event is still theirs, as the bare fact, and nothing of the wiki is said.
    w.store.revoke(&me, &w.wiki.id).unwrap();
    let after = inbox(&w, Some(&me), &[]).await;
    assert_eq!(after.status, 200);
    let body = after.body();
    assert!(body.contains("in a wiki you can no longer read"), "{body}");
    for secret in ["secret summary", "The Vault", "vault", "Ben", "declined"] {
        assert!(!body.contains(secret), "{secret}: {body}");
    }
}

#[tokio::test]
async fn reading_is_a_post_from_this_site_and_goes_back_to_the_inbox() {
    let w = handbook();
    let me = account(&w, "Aiko", Some(Role::Contributor));
    let other = account(&w, "Ben", Some(Role::Contributor));
    let p = held(&w, &me, "Fix the intro");
    tell(&w, &me, InboxEventKind::Declined, &p, None, NOW);
    tell(&w, &other, InboxEventKind::Declined, &p, None, NOW);

    // Opening the page reads nothing.
    assert!(
        inbox(&w, Some(&me), &[])
            .await
            .body()
            .contains("<strong>New</strong>")
    );
    assert!(
        inbox(&w, Some(&me), &[])
            .await
            .body()
            .contains("<strong>New</strong>")
    );
    // A form from another site is refused, and nothing changes.
    let refused = send(&w, "POST", "/-/inbox", Some(&me), CROSS_SITE, "").await;
    assert_eq!(refused.status, 403);
    assert!(
        inbox(&w, Some(&me), &[])
            .await
            .body()
            .contains("<strong>New</strong>")
    );
    // From this site: back to the inbox, and read. Somebody else's events are not read.
    let done = send(&w, "POST", "/-/inbox", Some(&me), SAME_SITE, "").await;
    assert_eq!(done.status, 303);
    assert_eq!(done.header("location"), Some("/-/inbox"));
    let body = inbox(&w, Some(&me), &[]).await.body();
    assert!(!body.contains("<strong>New</strong>"), "{body}");
    assert!(!body.contains("Mark everything as read"), "{body}");
    assert!(
        body.contains("declined your proposal"),
        "still listed once read: {body}"
    );
    assert!(
        inbox(&w, Some(&other), &[])
            .await
            .body()
            .contains("<strong>New</strong>")
    );
}

#[tokio::test]
async fn following_a_page_is_listed_in_the_inbox_and_can_be_stopped() {
    let w = handbook();
    let me = account(&w, "Aiko", Some(Role::Contributor));
    let follow = "scope=page";

    let a = send(
        &w,
        "POST",
        "/handbook/-/watch/Home",
        Some(&me),
        SAME_SITE,
        follow,
    )
    .await;
    assert_eq!(a.status, 200, "{}", a.body());
    let body = a.body();
    assert!(body.contains("Following"), "{body}");
    assert!(body.contains("You follow the page Home."), "{body}");
    assert!(body.contains("href=\"/handbook/Home\""), "{body}");
    assert!(body.contains("href=\"/-/inbox\""), "{body}");
    assert_eq!(
        w.store.watches_of(&me).unwrap(),
        vec![WatchRecord {
            account: me.clone(),
            wiki: w.wiki.id.clone(),
            target: WatchTarget::Page("pages/Home.md".to_owned()),
            created_at: NOW,
        }]
    );

    // The namespace under a title, a hierarchy.
    let n = send(
        &w,
        "POST",
        "/handbook/-/watch/Guides",
        Some(&me),
        SAME_SITE,
        "scope=namespace&do=watch",
    )
    .await;
    assert_eq!(n.status, 200, "{}", n.body());
    assert!(n.body().contains("You follow the pages under Guides."));

    // The inbox lists both, each with a way to stop that names what it stops.
    let listed = inbox(&w, Some(&me), &[]).await.body();
    assert!(listed.contains("Pages you follow"), "{listed}");
    assert!(
        listed.contains("Home</a> in <a href=\"/handbook\">The Handbook</a>"),
        "{listed}"
    );
    assert!(
        listed.contains("The pages under <a href=\"/handbook/Guides\">Guides</a>"),
        "{listed}"
    );
    assert!(
        listed.contains("Stop following Home in The Handbook"),
        "{listed}"
    );
    assert!(
        listed.contains("action=\"/handbook/-/watch/Home\"")
            && listed.contains("value=\"unwatch\"")
    );

    // Stopping, as the form in the inbox posts it.
    let stop = send(
        &w,
        "POST",
        "/handbook/-/watch/Home",
        Some(&me),
        SAME_SITE,
        "scope=page&do=unwatch",
    )
    .await;
    assert_eq!(stop.status, 200);
    assert!(
        stop.body().contains("No longer following"),
        "{}",
        stop.body()
    );
    assert!(stop.body().contains("You no longer follow the page Home."));
    assert_eq!(w.store.watches_of(&me).unwrap().len(), 1);
    let listed = inbox(&w, Some(&me), &[]).await.body();
    assert!(!listed.contains("Stop following Home in"), "{listed}");
    // Again is no change and says the same.
    let again = send(
        &w,
        "POST",
        "/handbook/-/watch/Home",
        Some(&me),
        SAME_SITE,
        "do=unwatch",
    )
    .await;
    assert_eq!(again.status, 200);
    // Following twice is one watch.
    for _ in 0..2 {
        let a = send(
            &w,
            "POST",
            "/handbook/-/watch/Home",
            Some(&me),
            SAME_SITE,
            "",
        )
        .await;
        assert_eq!(a.status, 200);
    }
    assert_eq!(w.store.watches_of(&me).unwrap().len(), 2);
}

#[tokio::test]
async fn following_needs_an_account_a_form_from_this_site_and_a_page_the_account_may_read() {
    let w = handbook();
    let me = account(&w, "Aiko", Some(Role::Contributor));

    // No account: told so in words, in a wiki the visitor may read.
    let anon = send(&w, "POST", "/handbook/-/watch/Home", None, SAME_SITE, "").await;
    assert_eq!(anon.status, 403);
    assert!(
        anon.body().contains("An account is needed"),
        "{}",
        anon.body()
    );
    assert!(
        anon.body()
            .contains("Following a page belongs to an account.")
    );
    // A form from another site is refused whoever sends it, before anything is looked at.
    let cross = send(
        &w,
        "POST",
        "/handbook/-/watch/Home",
        Some(&me),
        CROSS_SITE,
        "",
    )
    .await;
    assert_eq!(cross.status, 403);
    assert!(
        cross.body().contains("form") || cross.body().contains("site"),
        "{}",
        cross.body()
    );
    assert!(w.store.watches_of(&me).unwrap().is_empty());
    // A title that cannot name a page is the not-found page.
    let bad = send(
        &w,
        "POST",
        "/handbook/-/watch/%ff",
        Some(&me),
        SAME_SITE,
        "",
    )
    .await;
    assert_eq!(bad.status, 404);
    // A wiki that is not there.
    let none = send(
        &w,
        "POST",
        "/nowhere/-/watch/Home",
        Some(&me),
        SAME_SITE,
        "",
    )
    .await;
    assert_eq!(none.status, 404);
    // `GET` is not a way to follow.
    let get = send(&w, "GET", "/handbook/-/watch/Home", Some(&me), &[], "").await;
    assert_eq!(get.status, 405);
    assert!(w.store.watches_of(&me).unwrap().is_empty());
}

#[tokio::test]
async fn a_restricted_wiki_is_followed_only_by_those_who_may_read_it_and_looks_absent_to_the_rest()
{
    let w = world(Visibility::Restricted, "vault", "The Vault");
    let member = account(&w, "Aiko", Some(Role::Observer));
    let outsider = account(&w, "Ben", None);
    let refused = send(
        &w,
        "POST",
        "/vault/-/watch/Home",
        Some(&outsider),
        SAME_SITE,
        "",
    )
    .await;
    let absent = send(
        &w,
        "POST",
        "/no-such-wiki/-/watch/Home",
        Some(&outsider),
        SAME_SITE,
        "",
    )
    .await;
    assert_eq!(refused.status, 404);
    assert_eq!(refused.status, absent.status);
    assert_eq!(
        refused.body().replace("vault", "X"),
        absent.body().replace("no-such-wiki", "X")
    );
    assert!(w.store.watches_of(&outsider).unwrap().is_empty());
    // A member follows.
    let ok = send(
        &w,
        "POST",
        "/vault/-/watch/Home",
        Some(&member),
        SAME_SITE,
        "",
    )
    .await;
    assert_eq!(ok.status, 200, "{}", ok.body());
    assert_eq!(w.store.watches_of(&member).unwrap().len(), 1);
    // And an anonymous visitor to the restricted wiki gets the same not-found as for one that is not there.
    let anon = send(&w, "POST", "/vault/-/watch/Home", None, SAME_SITE, "").await;
    assert_eq!(anon.status, 404);
}

#[tokio::test]
async fn an_account_that_follows_as_many_as_it_may_is_told_to_stop_one_first() {
    let w = handbook();
    let me = account(&w, "Aiko", Some(Role::Contributor));
    for i in 0..inklkunn_core::inbox::MAX_WATCHES {
        w.store
            .watch(&WatchRecord {
                account: me.clone(),
                wiki: w.wiki.id.clone(),
                target: WatchTarget::Page(format!("pages/P{i}.md")),
                created_at: NOW,
            })
            .unwrap();
    }
    let full = send(
        &w,
        "POST",
        "/handbook/-/watch/Home",
        Some(&me),
        SAME_SITE,
        "",
    )
    .await;
    assert_eq!(full.status, 409);
    assert!(
        full.body().contains("You follow as many as you can"),
        "{}",
        full.body()
    );
    assert_eq!(
        w.store.watches_of(&me).unwrap().len(),
        inklkunn_core::inbox::MAX_WATCHES
    );
}
