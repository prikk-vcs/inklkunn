//! **The structural rules over every state the OpenID Connect handoff adds**, in English and in Japanese
//! (RFC 003 D-6): each state is rendered through the real router and the real authenticator, with the stand-in
//! provider, parsed with the same HTML5 parser and held to the same rules as every other page. The page that
//! moves the browser on after a sign-in is not a page of the layout (it has no navigation and no style, by
//! design), so it has its own few rules here. The retired words and the glossary are the catalog's own tests.

use inklkunn_auth::JoinPolicy;
use inklkunn_store::{ExternalIdentity, Role};

use crate::provider::{invitation, start, with_provider};
use crate::states::{dom, rules};

struct State {
    id: &'static str,
    status: u16,
    html: String,
}

/// Every state, with `extra` request headers (the language cookie).
async fn states(lang_cookie: Option<&str>) -> Vec<State> {
    let mut out = Vec::new();
    let mut add = |id: &'static str, a: crate::common::Answer| {
        out.push(State {
            id,
            status: a.status.as_u16(),
            html: a.body(),
        });
    };
    let cookie_with = |more: &str| -> String {
        match lang_cookie {
            Some(l) => format!("{l}; {more}"),
            None => more.to_owned(),
        }
    };
    let lang: Vec<(&str, &str)> = lang_cookie.map(|l| ("cookie", l)).into_iter().collect();

    // ---- where nobody may join: the button, and the ways a sign-in with the provider ends
    let (r, p) = with_provider(JoinPolicy::Off);
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    r.auth
        .link_external(
            &alice,
            &ExternalIdentity::new("https://idp.example", "alice-at-idp").unwrap(),
            crate::common::NOW,
        )
        .unwrap();
    add("sign-in-with-provider", r.get("/-/sign-in", &lang).await);
    add(
        "sign-in-with-provider-and-return",
        r.get("/-/sign-in?next=/garden/Home", &lang).await,
    );
    p.person("a-stranger");
    let s = start(&r, "/-/sign-in/oidc", &lang).await;
    add(
        "provider-not-linked",
        r.get(
            &format!("/-/sign-in/oidc/callback?code=c&state={}", s.state),
            &[("cookie", &cookie_with(&s.cookie()))],
        )
        .await,
    );
    add(
        "provider-state",
        r.get("/-/sign-in/oidc/callback?code=c&state=s", &lang)
            .await,
    );
    let s = start(&r, "/-/sign-in/oidc", &lang).await;
    add(
        "provider-said-no",
        r.get(
            &format!(
                "/-/sign-in/oidc/callback?error=access_denied&state={}",
                s.state
            ),
            &[("cookie", &cookie_with(&s.cookie()))],
        )
        .await,
    );
    p.goes_down();
    let s = start(&r, "/-/sign-in/oidc", &lang).await;
    add(
        "provider-unreachable",
        r.get(
            &format!("/-/sign-in/oidc/callback?code=c&state={}", s.state),
            &[("cookie", &cookie_with(&s.cookie()))],
        )
        .await,
    );

    // ---- where a first sign-in makes an account: the sentence under the button
    let (r, _) = with_provider(JoinPolicy::Provider);
    add("sign-in-provider-joins", r.get("/-/sign-in", &lang).await);

    // ---- an invitation: the page, a refusal because the person has an account, a link that is no good
    let (r, p) = with_provider(JoinPolicy::Off);
    let secret = invitation(&r, Some(Role::Reviewer));
    add(
        "invite-with-provider",
        r.get(&format!("/-/invite/{secret}"), &lang).await,
    );
    let carol = r.person("carol", "Carol", None);
    r.auth
        .link_external(
            &carol,
            &ExternalIdentity::new("https://idp.example", "carol-at-idp").unwrap(),
            crate::common::NOW,
        )
        .unwrap();
    p.person("carol-at-idp");
    let s = start(&r, &format!("/-/invite/{secret}/oidc"), &lang).await;
    add(
        "invite-provider-taken",
        r.get(
            &format!("/-/sign-in/oidc/callback?code=c&state={}", s.state),
            &[("cookie", &cookie_with(&s.cookie()))],
        )
        .await,
    );
    add(
        "invite-provider-unusable",
        r.get(&format!("/-/invite/inv_{}/oidc", "A".repeat(43)), &lang)
            .await,
    );

    // ---- the page that moves the browser on
    let (r, p) = with_provider(JoinPolicy::Provider);
    p.person("newcomer");
    let s = start(&r, "/-/sign-in/oidc", &lang).await;
    add(
        "landing",
        r.get(
            &format!("/-/sign-in/oidc/callback?code=c&state={}", s.state),
            &[("cookie", &cookie_with(&s.cookie()))],
        )
        .await,
    );
    out
}

async fn every_state_passes(lang_cookie: Option<&str>, tag: &str) {
    let all = states(lang_cookie).await;
    assert_eq!(all.len(), 11);
    let mut problems = Vec::new();
    for s in &all {
        assert!(
            s.html
                .contains(&format!("<html lang=\"{tag}\" dir=\"ltr\">")),
            "{} is not rendered in {tag}: {}",
            s.id,
            s.html
        );
        assert!(!s.html.contains("<script"), "{}", s.id);
        let d = dom::parse(&s.html);
        if s.id == "landing" {
            continue;
        }
        for v in rules::check(&d) {
            problems.push(format!("{}: {} {}", s.id, v.rule, v.detail));
        }
    }
    assert_eq!(problems, Vec::<String>::new());
    // The statuses the pages promise.
    let status = |id: &str| all.iter().find(|s| s.id == id).map(|s| s.status);
    for (id, want) in [
        ("sign-in-with-provider", 200),
        ("sign-in-with-provider-and-return", 200),
        ("sign-in-provider-joins", 200),
        ("provider-not-linked", 403),
        ("provider-state", 403),
        ("provider-said-no", 401),
        ("provider-unreachable", 502),
        ("invite-with-provider", 200),
        ("invite-provider-taken", 409),
        ("invite-provider-unusable", 404),
        ("landing", 200),
    ] {
        assert_eq!(status(id), Some(want), "{id}");
    }
    // The landing: one title, one sentence with one link, the refresh, no layout.
    let landing = all.iter().find(|s| s.id == "landing").unwrap();
    let d = dom::parse(&landing.html);
    let title = rules::page_title(&d).expect("a title");
    assert!(title.ends_with("inklkunn"), "{title}");
    assert!(
        landing
            .html
            .contains("<meta http-equiv=\"refresh\" content=\"0;url=/-/account\">")
    );
    assert_eq!(landing.html.matches("<a ").count(), 1, "{}", landing.html);
    assert!(!landing.html.contains("<nav") && !landing.html.contains("<form"));
    // The refusals are the sign-in page, with its title; the others have their pages' own.
    let titles = |ids: &[&str]| -> std::collections::BTreeSet<String> {
        ids.iter()
            .map(|id| {
                let s = all.iter().find(|s| s.id == *id).unwrap();
                rules::page_title(&dom::parse(&s.html)).unwrap()
            })
            .collect()
    };
    assert_eq!(
        titles(&[
            "sign-in-with-provider",
            "provider-not-linked",
            "provider-state",
            "provider-said-no",
            "provider-unreachable",
            "invite-provider-taken",
        ])
        .len(),
        1
    );
}

#[tokio::test]
async fn every_provider_state_passes_every_rule_in_english() {
    every_state_passes(None, "en").await;
}

#[tokio::test]
async fn every_provider_state_passes_every_rule_in_japanese() {
    every_state_passes(Some("inklkunn_lang=ja"), "ja").await;
}
