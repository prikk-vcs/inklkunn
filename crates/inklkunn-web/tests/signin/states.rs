//! **The structural rules over every state the sign-in handoff adds**, in English and in Japanese (RFC 003 D-6):
//! each state is rendered through the real router and the real authenticator and parsed with the same HTML5
//! parser and held to the same rules as every other page (`tests/structure`), among them the rule this handoff
//! adds: every form a signed-in person can submit, and every form that makes a session, carries a token. The
//! retired words and the glossary are the catalog's own tests, which hold every message these pages say
//! (`tests/retired_words.rs`, `tests/glossary.rs`).

#[path = "../structure/dom.rs"]
pub(crate) mod dom;
#[path = "../structure/rules.rs"]
pub(crate) mod rules;

use inklkunn_auth::{AuthConfig, JoinPolicy, ThrottlePolicy};
use inklkunn_store::Role;

use crate::common::NOW;
use crate::harness::*;

/// The headers a request carries for its cookies: none, or one `cookie`.
type Cookies = Vec<(String, String)>;

struct State {
    id: &'static str,
    status: u16,
    html: String,
}

/// Every state, each on a world of its own, with `extra` request headers (the language cookie).
async fn states(extra: &[(&str, &str)]) -> Vec<State> {
    let mut out: Vec<State> = Vec::new();
    let mut add = |id: &'static str, a: crate::common::Answer| {
        out.push(State {
            id,
            status: a.status.as_u16(),
            html: a.body(),
        });
    };
    // The cookie header of the language and of the visitor's cookies, merged into one.
    let merge = |extra_cookie: Option<&str>| -> Cookies {
        let lang = extra
            .iter()
            .find(|(k, _)| *k == "cookie")
            .map(|(_, v)| (*v).to_owned());
        let joined = [lang, extra_cookie.map(str::to_owned)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ");
        if joined.is_empty() {
            Vec::new()
        } else {
            vec![("cookie".to_owned(), joined)]
        }
    };
    // Request through `r` with merged cookies.
    async fn go(
        r: &Real,
        method: &str,
        uri: &str,
        cookie: Option<&str>,
        more: &[(&str, &str)],
        body: &str,
        merge: &dyn Fn(Option<&str>) -> Cookies,
    ) -> crate::common::Answer {
        let cookies = merge(cookie);
        let mut headers: Vec<(&str, &str)> = cookies
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        headers.extend_from_slice(more);
        r.call(method, uri, &headers, body).await
    }

    // ---- signed out: the sign-in page and its refusals
    let r = real(AuthConfig {
        login_throttle: ThrottlePolicy {
            max_failures: 2,
            lock_secs: 600,
            forget_secs: 600,
            max_tracked: 100,
        },
        ..config()
    });
    r.person("alice", "Alice", Some(Role::Contributor));
    add(
        "sign-in",
        go(&r, "GET", "/-/sign-in", None, &[], "", &merge).await,
    );
    add(
        "sign-in-with-return",
        go(
            &r,
            "GET",
            "/-/sign-in?next=/garden/Home",
            None,
            &[],
            "",
            &merge,
        )
        .await,
    );
    let page = go(&r, "GET", "/-/sign-in", None, &[], "", &merge).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let csrf_cookie = format!("__Host-inklkunn-csrf={token}");
    let attempt = |password: &str| {
        form(&[
            ("csrf", &token),
            ("login", "alice"),
            ("password", password),
            ("next", "/"),
        ])
    };
    add(
        "sign-in-failed",
        go(
            &r,
            "POST",
            "/-/sign-in",
            Some(&csrf_cookie),
            &[SAME_SITE],
            &attempt("wrong"),
            &merge,
        )
        .await,
    );
    add("sign-in-locked", {
        let _ = go(
            &r,
            "POST",
            "/-/sign-in",
            Some(&csrf_cookie),
            &[SAME_SITE],
            &attempt("wrong"),
            &merge,
        )
        .await;
        go(
            &r,
            "POST",
            "/-/sign-in",
            Some(&csrf_cookie),
            &[SAME_SITE],
            &attempt("wrong"),
            &merge,
        )
        .await
    });
    add(
        "form-expired",
        go(
            &r,
            "POST",
            "/-/sign-in",
            None,
            &[SAME_SITE],
            &attempt("wrong"),
            &merge,
        )
        .await,
    );
    add(
        "sign-out-signed-out",
        go(&r, "GET", "/-/sign-out", None, &[], "", &merge).await,
    );
    add(
        "wiki-page-signed-out",
        go(&r, "GET", "/garden/Home", None, &[], "", &merge).await,
    );
    add(
        "join-closed",
        go(&r, "GET", "/-/join", None, &[], "", &merge).await,
    );
    let invitation = r.auth.invite(None, 3600, NOW).unwrap();
    let invite_uri = format!("/-/invite/{}", invitation.secret.as_str());
    add(
        "invite",
        go(&r, "GET", &invite_uri, None, &[], "", &merge).await,
    );
    let page = go(&r, "GET", &invite_uri, None, &[], "", &merge).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}");
    add(
        "invite-problem",
        go(
            &r,
            "POST",
            &invite_uri,
            Some(&cookie),
            &[SAME_SITE],
            &form(&[
                ("csrf", &token),
                ("login", "alice"),
                ("name", "A"),
                ("password", PASSWORD),
            ]),
            &merge,
        )
        .await,
    );
    add(
        "invite-unusable",
        go(
            &r,
            "GET",
            "/-/invite/inv_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
            &[],
            "",
            &merge,
        )
        .await,
    );

    // ---- open to anyone
    let r = real(joining(JoinPolicy::Anyone));
    add(
        "sign-in-join-open",
        go(&r, "GET", "/-/sign-in", None, &[], "", &merge).await,
    );
    add(
        "join",
        go(&r, "GET", "/-/join", None, &[], "", &merge).await,
    );
    let page = go(&r, "GET", "/-/join", None, &[], "", &merge).await;
    let token = cookie_value(&page, "__Host-inklkunn-csrf").unwrap();
    let cookie = format!("__Host-inklkunn-csrf={token}");
    add(
        "join-problem",
        go(
            &r,
            "POST",
            "/-/join",
            Some(&cookie),
            &[SAME_SITE],
            &form(&[
                ("csrf", &token),
                ("login", "x"),
                ("name", "X"),
                ("password", "short"),
            ]),
            &merge,
        )
        .await,
    );

    // ---- signed in
    let r = real(config());
    r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    let signed = Some(s.cookie.as_str());
    add(
        "sign-in-already",
        go(&r, "GET", "/-/sign-in", signed, &[], "", &merge).await,
    );
    add(
        "sign-out",
        go(&r, "GET", "/-/sign-out", signed, &[], "", &merge).await,
    );
    add(
        "wiki-page-signed-in",
        go(&r, "GET", "/garden/Home", signed, &[], "", &merge).await,
    );
    add(
        "verification-signed-in",
        go(&r, "GET", "/garden/-/verification", signed, &[], "", &merge).await,
    );
    add(
        "account",
        go(&r, "GET", "/-/account", signed, &[], "", &merge).await,
    );
    for (id, done) in [
        ("account-name-saved", "name"),
        ("account-session-ended", "session"),
        ("account-token-revoked", "token"),
    ] {
        add(
            id,
            go(
                &r,
                "GET",
                &format!("/-/account?done={done}"),
                signed,
                &[],
                "",
                &merge,
            )
            .await,
        );
    }
    let post = |uri: &'static str, fields: Vec<(&'static str, String)>| {
        let body = form(
            &fields
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect::<Vec<_>>(),
        );
        (uri, body)
    };
    let signed_post = [SAME_SITE, (inklkunn_web::CSRF_HEADER, s.csrf.as_str())];
    let (uri, body) = post("/-/account/name", vec![("name", String::new())]);
    add(
        "account-name-problem",
        go(&r, "POST", uri, signed, &signed_post, &body, &merge).await,
    );
    let (uri, body) = post(
        "/-/account/tokens",
        vec![
            ("label", "bot".into()),
            ("scope", "nonsense".into()),
            ("days", "7".into()),
        ],
    );
    add(
        "account-token-problem",
        go(&r, "POST", uri, signed, &signed_post, &body, &merge).await,
    );
    let (uri, body) = post(
        "/-/account/tokens",
        vec![
            ("label", "the <build> bot".into()),
            ("scope", "propose".into()),
            ("days", "30".into()),
        ],
    );
    add(
        "account-token-made",
        go(&r, "POST", uri, signed, &signed_post, &body, &merge).await,
    );
    // A list with every state of a token: live, revoked, ended.
    let alice = r.w.store.account_by_login_for_test("alice");
    let live = r
        .auth
        .issue_token(&alice, inklkunn_store::Scope::Review, "live", 3600, NOW)
        .unwrap();
    let gone = r
        .auth
        .issue_token(&alice, inklkunn_store::Scope::Read, "revoked", 3600, NOW)
        .unwrap();
    r.auth.revoke_token(&alice, &gone.id, NOW).unwrap();
    r.auth
        .issue_token(
            &alice,
            inklkunn_store::Scope::Accept,
            "ended",
            60,
            NOW - 3600,
        )
        .unwrap();
    let _ = live;
    add(
        "account-tokens-listed",
        go(&r, "GET", "/-/account", signed, &[], "", &merge).await,
    );
    out
}

trait ByLogin {
    fn account_by_login_for_test(&self, login: &str) -> inklkunn_store::AccountId;
}

impl ByLogin for std::sync::Arc<inklkunn_store::MemoryStore> {
    fn account_by_login_for_test(&self, login: &str) -> inklkunn_store::AccountId {
        use inklkunn_store::AccountStore;
        self.account_by_login(&inklkunn_store::LoginName::parse(login).unwrap())
            .unwrap()
            .unwrap()
            .id
    }
}

async fn every_state_passes(extra: &[(&str, &str)], tag: &str) {
    let all = states(extra).await;
    assert!(all.len() >= 25, "{} states", all.len());
    let mut problems = Vec::new();
    for s in &all {
        assert!(
            s.html
                .contains(&format!("<html lang=\"{tag}\" dir=\"ltr\">")),
            "{} is not rendered in {tag}",
            s.id
        );
        let d = dom::parse(&s.html);
        for v in rules::check(&d) {
            problems.push(format!("{}: {} {}", s.id, v.rule, v.detail));
        }
        // A page of a person's own never offers a form that would change something without a button.
        assert!(!s.html.contains("<script"), "{}", s.id);
    }
    assert_eq!(problems, Vec::<String>::new());
    // The statuses the pages promise.
    let status = |id: &str| all.iter().find(|s| s.id == id).map(|s| s.status);
    for (id, want) in [
        ("sign-in", 200),
        ("sign-in-failed", 401),
        ("sign-in-locked", 429),
        ("form-expired", 403),
        ("sign-out", 200),
        ("sign-out-signed-out", 200),
        ("join-closed", 404),
        ("join", 200),
        ("join-problem", 422),
        ("invite", 200),
        ("invite-problem", 409),
        ("invite-unusable", 404),
        ("account", 200),
        ("account-name-problem", 422),
        ("account-token-problem", 422),
        ("account-token-made", 200),
        ("wiki-page-signed-in", 200),
        ("verification-signed-in", 200),
        ("wiki-page-signed-out", 200),
    ] {
        assert_eq!(status(id), Some(want), "{id}");
    }
    // Titles: one per page, and the pages are told apart.
    let title = |id: &str| {
        let s = all.iter().find(|s| s.id == id).unwrap();
        rules::page_title(&dom::parse(&s.html)).unwrap()
    };
    let distinct: std::collections::BTreeSet<String> = [
        "sign-in",
        "sign-out",
        "join",
        "join-closed",
        "invite",
        "invite-unusable",
        "account",
        "form-expired",
    ]
    .iter()
    .map(|id| title(id))
    .collect();
    // Eight states, six pages: the closed join and the unusable invitation are those pages saying no.
    assert_eq!(distinct.len(), 6, "{distinct:?}");
    assert_eq!(title("join"), title("join-closed"));
    assert_eq!(title("invite"), title("invite-unusable"));
    assert_eq!(title("sign-in"), title("sign-in-failed"));
    assert_eq!(title("account"), title("account-token-made"));
}

#[tokio::test]
async fn every_sign_in_state_passes_every_rule_in_english() {
    every_state_passes(&[], "en").await;
}

#[tokio::test]
async fn every_sign_in_state_passes_every_rule_in_japanese() {
    every_state_passes(&[("cookie", "inklkunn_lang=ja")], "ja").await;
}
