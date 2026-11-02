//! Signing in with the configured provider through the real router and the real authenticator, against a
//! stand-in provider (the OpenID Connect handoff's U3; RFC 002 D-1; handoff 002 C9 and F4): the provider's button
//! exists only when a provider is configured; the login-state cookie the provider's cross-site redirect carries
//! is `Lax`, `HttpOnly`, `Secure`, short-lived and single-use while the session cookie stays `Strict`; a
//! callback this browser did not start, or one used twice, signs nobody in; a first sign-in makes an account
//! only where `INKLKUNN_JOIN` says; an invitation may be redeemed with the provider instead of a password; and
//! the page that moves the browser on is drawn here from the catalog, in the reader's language.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use inklkunn_auth::invitation::DEFAULT_LIFETIME_SECS;
use inklkunn_auth::oidc::{
    FetchError, FetchRequest, HttpsFetcher, OidcConfig, OidcError, OidcProvider, TokenVerifier,
    VerifiedIdentity,
};
use inklkunn_auth::{AuthConfig, JoinPolicy};
use inklkunn_store::{AccountStore, ExternalIdentity, Role, RoleStore, Visibility};

use crate::common::{Answer, NOW};
use crate::harness::*;

const ISSUER: &str = "https://idp.example";
const STATE_COOKIE: &str = "__Host-inklkunn-oidc";
const SESSION_COOKIE: &str = "__Host-inklkunn-session";

/// The stand-in provider's network: discovery and the token endpoint, answered from a map.
struct Idp(Arc<Mutex<HashMap<String, Vec<u8>>>>);

impl HttpsFetcher for Idp {
    fn fetch(&self, req: &FetchRequest) -> Result<Vec<u8>, FetchError> {
        self.0
            .lock()
            .unwrap()
            .get(&req.url)
            .cloned()
            .ok_or_else(|| FetchError::Transport(format!("no route for {}", req.url)))
    }
}

/// Vouches for whoever is at the provider just now (the ID token's signature is the auth crate's own tests).
struct Vouches(String, Arc<Mutex<String>>);

impl TokenVerifier for Vouches {
    fn verify(&self, _: &str, _: &str, _: i64) -> Result<VerifiedIdentity, OidcError> {
        let subject = self.1.lock().unwrap().clone();
        Ok(VerifiedIdentity::for_test(
            ExternalIdentity::new(&self.0, &subject).unwrap(),
            Some("alice@example.org".into()),
        ))
    }
}

/// What a test turns to change what the provider does.
pub(crate) struct Provider {
    issuer: String,
    routes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    at_the_provider: Arc<Mutex<String>>,
}

impl Provider {
    pub(crate) fn person(&self, subject: &str) {
        *self.at_the_provider.lock().unwrap() = subject.to_owned();
    }

    /// The provider stops answering the token endpoint.
    pub(crate) fn goes_down(&self) {
        self.routes
            .lock()
            .unwrap()
            .remove(&format!("{}/token", self.issuer));
    }
}

/// A world with a provider configured, and joining as `join` says.
pub(crate) fn with_provider(join: JoinPolicy) -> (Real, Provider) {
    provider_at(
        join,
        ISSUER,
        "http://localhost:8080/-/sign-in/oidc/callback",
    )
}

/// As [`with_provider`], for a provider at `issuer` (an https address, with a port if it has one) that sends the
/// browser back to `redirect_uri`: the real-browser run serves the site on a real port.
pub(crate) fn provider_at(join: JoinPolicy, issuer: &str, redirect_uri: &str) -> (Real, Provider) {
    let routes = Arc::new(Mutex::new(HashMap::from([
        (
            format!("{issuer}/.well-known/openid-configuration"),
            format!(
                r#"{{"issuer":"{issuer}","authorization_endpoint":"{issuer}/authorize","token_endpoint":"{issuer}/token","jwks_uri":"{issuer}/jwks"}}"#
            )
            .into_bytes(),
        ),
        (format!("{issuer}/token"), br#"{"id_token":"t"}"#.to_vec()),
    ])));
    let at_the_provider = Arc::new(Mutex::new("nobody".to_owned()));
    let provider = OidcProvider::with_verifier(
        OidcConfig {
            issuer: issuer.into(),
            client_id: "inklkunn-wiki".into(),
            client_secret: None,
            redirect_uri: redirect_uri.into(),
            scopes: Vec::new(),
        },
        Arc::new(Idp(routes.clone())),
        Arc::new(Vouches(issuer.to_owned(), at_the_provider.clone())),
    );
    let r = real_built(
        AuthConfig { join, ..config() },
        Visibility::Public,
        |auth| auth.with_oidc(provider),
    );
    (
        r,
        Provider {
            issuer: issuer.to_owned(),
            routes,
            at_the_provider,
        },
    )
}

/// A browser that started a sign-in: the provider's address it was sent to and the state it holds.
pub(crate) struct Started {
    pub(crate) state: String,
    pub(crate) answer: Answer,
}

impl Started {
    /// The `Cookie` header a browser sends back with the callback.
    pub(crate) fn cookie(&self) -> String {
        format!("{STATE_COOKIE}={}", self.state)
    }
}

pub(crate) async fn start(r: &Real, uri: &str, headers: &[(&str, &str)]) -> Started {
    let a = r.get(uri, headers).await;
    assert_eq!(a.status, 303, "{uri}: {}", a.body());
    let state = cookie_value(&a, STATE_COOKIE).expect("the login-state cookie");
    Started { state, answer: a }
}

/// The provider sends the browser back: the query holds the `state` it was given, and `cookie` is what the
/// browser sends with it.
async fn come_back(r: &Real, state: &str, cookie: Option<&str>, extra: &[(&str, &str)]) -> Answer {
    let uri = format!("/-/sign-in/oidc/callback?code=the-code&state={state}");
    let mut headers: Vec<(&str, &str)> = extra.to_vec();
    if let Some(c) = cookie {
        headers.push(("cookie", c));
    }
    r.get(&uri, &headers).await
}

/// Start and come back with the right cookie.
pub(crate) async fn sign_in_with_the_provider(r: &Real, uri: &str) -> Answer {
    let s = start(r, uri, &[]).await;
    come_back(r, &s.state, Some(&s.cookie()), &[]).await
}

pub(crate) fn invitation(r: &Real, role: Option<Role>) -> String {
    let grant = role.map(|role| (r.w.wiki.id.clone(), role));
    r.auth
        .invite(grant, DEFAULT_LIFETIME_SECS, NOW)
        .unwrap()
        .secret
        .as_str()
        .to_owned()
}

fn account_of(r: &Real, subject: &str) -> Option<inklkunn_store::Account> {
    r.w.store
        .account_by_identity(&ExternalIdentity::new(ISSUER, subject).unwrap())
        .unwrap()
}

// ------------------------------------------------------------------------------------ the button

#[tokio::test]
async fn the_provider_button_is_offered_only_where_a_provider_is_configured() {
    // No provider: no link, anywhere a person signs in or is invited, and the addresses are the generic page.
    let r = real(joining(JoinPolicy::Anyone));
    let secret = invitation(&r, None);
    let invite = format!("/-/invite/{secret}");
    for uri in [
        "/-/sign-in",
        "/-/sign-in?next=/garden/Home",
        "/-/join",
        &invite,
    ] {
        let page = r.get(uri, &[]).await;
        assert_eq!(page.status, 200, "{uri}");
        let body = page.body();
        assert!(
            !body.contains("/oidc") && !body.contains("Sign in with"),
            "{uri}: {body}"
        );
        assert!(!body.contains("instead of a password"), "{uri}: {body}");
    }
    for uri in [
        "/-/sign-in/oidc".to_owned(),
        "/-/sign-in/oidc/callback?code=c&state=s".to_owned(),
        format!("/-/invite/{secret}/oidc"),
    ] {
        let a = r.get(&uri, &[]).await;
        assert_eq!(a.status, 404, "{uri}");
        assert!(a.header("set-cookie").is_none(), "{uri}");
    }
    assert!(!r.auth.oidc_configured());

    // A provider: the sign-in page and the invitation offer it, by the provider's host name; the join page does not.
    let (r, _) = with_provider(JoinPolicy::Anyone);
    let secret = invitation(&r, None);
    let page = r.get("/-/sign-in", &[]).await.body();
    assert!(
        page.contains("<a href=\"/-/sign-in/oidc\">Sign in with idp.example</a>"),
        "{page}"
    );
    let page = r.get("/-/sign-in?next=/garden/Home", &[]).await.body();
    assert!(
        page.contains("<a href=\"/-/sign-in/oidc?next=/garden/Home\">Sign in with idp.example</a>"),
        "{page}"
    );
    let invite = r.get(&format!("/-/invite/{secret}"), &[]).await.body();
    assert!(
        invite.contains(&format!(
            "<a href=\"/-/invite/{secret}/oidc\">Use idp.example instead of a password</a>"
        )),
        "{invite}"
    );
    assert!(
        invite.contains("let your provider vouch for you"),
        "{invite}"
    );
    // The invitation's secret is in its two own addresses and nowhere else (not in a language button's return).
    assert_eq!(invite.matches(secret.as_str()).count(), 2, "{invite}");
    assert!(!r.get("/-/join", &[]).await.body().contains("/oidc"));
    // In Japanese, the same links in the reader's words.
    let ja = [("cookie", "inklkunn_lang=ja")];
    let page = r.get("/-/sign-in", &ja).await.body();
    assert!(
        page.contains("<a href=\"/-/sign-in/oidc\">idp.example でサインイン</a>"),
        "{page}"
    );
    let invite = r.get(&format!("/-/invite/{secret}"), &ja).await.body();
    assert!(
        invite.contains("パスワードの代わりに idp.example を使う</a>"),
        "{invite}"
    );
}

#[tokio::test]
async fn where_a_first_sign_in_makes_an_account_the_sign_in_page_says_so_and_nowhere_else() {
    for (join, says) in [
        (JoinPolicy::Off, false),
        (JoinPolicy::Provider, true),
        (JoinPolicy::Anyone, true),
    ] {
        let (r, _) = with_provider(join);
        let page = r.get("/-/sign-in", &[]).await.body();
        assert_eq!(
            page.contains(
                "The first time you sign in with idp.example, an account is made for you."
            ),
            says,
            "{join:?}: {page}"
        );
        // Where it does not, the page still tells a newcomer how accounts come into being.
        if join == JoinPolicy::Off {
            assert!(
                page.contains("Ask the person who runs this wiki for an invitation."),
                "{page}"
            );
        }
    }
}

// ------------------------------------------------------------------------------------ the round trip

#[tokio::test]
async fn the_login_state_cookie_is_lax_http_only_secure_and_short_lived_while_the_session_stays_strict()
 {
    let (r, p) = with_provider(JoinPolicy::Off);
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    r.auth
        .link_external(
            &alice,
            &ExternalIdentity::new(ISSUER, "alice-at-idp").unwrap(),
            NOW,
        )
        .unwrap();
    p.person("alice-at-idp");
    let s = start(&r, "/-/sign-in/oidc", &[]).await;
    // The browser is sent to the provider, with the state in the address and the proof key.
    let location = s.answer.header("location").unwrap().to_owned();
    assert!(
        location.starts_with(
            "https://idp.example/authorize?response_type=code&client_id=inklkunn-wiki"
        ),
        "{location}"
    );
    assert!(
        location.contains(&format!("&state={}&", s.state)),
        "{location}"
    );
    assert!(
        location.contains("code_challenge_method=S256"),
        "{location}"
    );
    assert!(location.contains("&scope=openid&"), "{location}");
    // The cookie the cross-site redirect must carry back.
    let line = cookie_line(&s.answer, STATE_COOKIE).unwrap();
    assert_eq!(
        line,
        format!(
            "{STATE_COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600",
            s.state
        )
    );
    assert!(
        !line.contains("Strict") && !line.contains("Domain"),
        "{line}"
    );
    // The start sets no session and is never kept.
    assert_eq!(set_cookies(&s.answer).len(), 1);
    assert_eq!(s.answer.header("cache-control"), Some("no-store"));

    // The way back signs in; its answer ends the state cookie and carries the Strict session.
    let done = come_back(&r, &s.state, Some(&s.cookie()), &[]).await;
    assert_eq!(done.status, 200, "{}", done.body());
    let cookies = set_cookies(&done);
    assert_eq!(cookies.len(), 2, "{cookies:?}");
    let session = cookie_line(&done, SESSION_COOKIE).unwrap();
    assert!(
        session.contains("; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=")
            && !session.contains("Domain"),
        "{session}"
    );
    let ended = cookie_line(&done, STATE_COOKIE).unwrap();
    assert_eq!(
        ended,
        format!("{STATE_COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0")
    );
}

#[tokio::test]
async fn a_success_is_a_200_page_in_the_readers_language_that_moves_the_browser_on_and_is_never_a_redirect()
 {
    let (r, p) = with_provider(JoinPolicy::Off);
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    r.auth
        .link_external(
            &alice,
            &ExternalIdentity::new(ISSUER, "alice-at-idp").unwrap(),
            NOW,
        )
        .unwrap();
    p.person("alice-at-idp");
    let done = sign_in_with_the_provider(&r, "/-/sign-in/oidc?next=/garden/Home").await;
    assert_eq!(done.status, 200);
    assert!(
        done.header("location").is_none(),
        "a redirect would lose the Strict cookie"
    );
    let body = done.body();
    assert!(body.starts_with("<!doctype html>"), "{body}");
    assert!(body.contains("<html lang=\"en\" dir=\"ltr\">"), "{body}");
    assert!(
        body.contains("<meta http-equiv=\"refresh\" content=\"0;url=/garden/Home\">"),
        "{body}"
    );
    // The address this page is reached at holds the provider's code; the page asks that it not be sent on.
    assert!(
        body.contains("<meta name=\"referrer\" content=\"no-referrer\">"),
        "{body}"
    );
    assert!(
        body.contains("<meta name=\"robots\" content=\"noindex\">"),
        "{body}"
    );
    assert!(
        body.contains("<title>Signing in · inklkunn</title>"),
        "{body}"
    );
    assert!(
        body.contains(
            "Signing you in. If this page does not move on by itself, <a href=\"/garden/Home\">continue to the page you asked for</a>."
        ),
        "{body}"
    );
    assert!(
        !body.contains("<script") && !body.contains("<link") && !body.contains("<style"),
        "{body}"
    );
    assert_eq!(done.header("cache-control"), Some("no-store"));
    assert_eq!(done.header("content-language"), Some("en"));
    assert!(
        done.header("x-robots-tag")
            .is_some_and(|v| v.contains("noindex"))
    );
    assert!(
        done.header("content-security-policy").is_some_and(
            |v| v.starts_with("default-src 'none'") && v.contains("form-action 'self'")
        ),
        "the site's own policy, which allows nothing the page uses"
    );
    // The page it moves on to, with the session it set, knows the person.
    let secret = cookie_value(&done, SESSION_COOKIE).unwrap();
    let cookie = format!("{SESSION_COOKIE}={secret}");
    let next = r.get("/garden/Home", &[("cookie", &cookie)]).await.body();
    assert!(next.contains("Signed in as Alice"), "{next}");

    // In Japanese, from the language cookie: the words are the catalog's.
    let (r, p) = with_provider(JoinPolicy::Off);
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    r.auth
        .link_external(
            &alice,
            &ExternalIdentity::new(ISSUER, "alice-at-idp").unwrap(),
            NOW,
        )
        .unwrap();
    p.person("alice-at-idp");
    let ja = "inklkunn_lang=ja";
    let s = start(&r, "/-/sign-in/oidc", &[("cookie", ja)]).await;
    let done = come_back(&r, &s.state, Some(&format!("{ja}; {}", s.cookie())), &[]).await;
    let body = done.body();
    assert_eq!(done.status, 200, "{body}");
    assert!(body.contains("<html lang=\"ja\" dir=\"ltr\">"), "{body}");
    assert!(
        body.contains("<title>サインインしています · inklkunn</title>"),
        "{body}"
    );
    assert!(
        body.contains("サインインしています。このページが自動で切り替わらない場合は、<a href=\"/\">続きのページへ進んでください</a>。"),
        "{body}"
    );
    assert_eq!(done.header("content-language"), Some("ja"));
}

#[tokio::test]
async fn a_callback_this_browser_did_not_start_signs_nobody_in_and_makes_no_account() {
    // Even where joining through the provider is open: a login-CSRF callback must not make or open anything.
    let (r, p) = with_provider(JoinPolicy::Provider);
    p.person("attacker-chosen");
    let mine = start(&r, "/-/sign-in/oidc", &[]).await;
    for (what, cookie) in [
        ("no cookie at all", None),
        (
            "someone else's state",
            Some(format!("{STATE_COOKIE}=not-this-state")),
        ),
        (
            "a cookie of another name",
            Some(format!("other={}", mine.state)),
        ),
        ("an empty state cookie", Some(format!("{STATE_COOKIE}="))),
    ] {
        let a = come_back(
            &r,
            &mine.state,
            cookie.as_deref(),
            &[("sec-fetch-site", "cross-site")],
        )
        .await;
        assert_eq!(a.status, 403, "{what}: {}", a.body());
        let body = a.body();
        assert!(
            body.contains("This sign-in did not start in this browser"),
            "{what}: {body}"
        );
        assert!(cookie_line(&a, SESSION_COOKIE).is_none(), "{what}");
        // The refusal ends the state cookie, so nothing is left behind.
        let ended = cookie_line(&a, STATE_COOKIE).expect("the state cookie ends");
        assert!(ended.contains("Max-Age=0"), "{what}: {ended}");
        assert!(account_of(&r, "attacker-chosen").is_none(), "{what}");
    }
    // The state is also missing from the address, or the code is.
    for uri in [
        "/-/sign-in/oidc/callback",
        "/-/sign-in/oidc/callback?code=c",
        "/-/sign-in/oidc/callback?state=",
    ] {
        let a = r.get(uri, &[("cookie", &mine.cookie())]).await;
        assert_eq!(a.status, 403, "{uri}");
        assert!(account_of(&r, "attacker-chosen").is_none(), "{uri}");
    }
    // None of those spent the real login: the browser that started it can still finish it.
    let ok = come_back(&r, &mine.state, Some(&mine.cookie()), &[]).await;
    assert_eq!(ok.status, 200, "{}", ok.body());
    assert!(account_of(&r, "attacker-chosen").is_some());
}

#[tokio::test]
async fn a_state_used_once_is_used_up_and_a_replayed_callback_is_refused() {
    let (r, p) = with_provider(JoinPolicy::Off);
    let alice = r.person("alice", "Alice", None);
    r.auth
        .link_external(
            &alice,
            &ExternalIdentity::new(ISSUER, "alice-at-idp").unwrap(),
            NOW,
        )
        .unwrap();
    p.person("alice-at-idp");
    let s = start(&r, "/-/sign-in/oidc", &[]).await;
    let first = come_back(&r, &s.state, Some(&s.cookie()), &[]).await;
    assert_eq!(first.status, 200);
    // The same address with the same cookie again (a back button, a copied link, a thief with the address):
    // the cookie was removed by the first answer and the server forgot the state.
    let again = come_back(&r, &s.state, Some(&s.cookie()), &[]).await;
    assert_eq!(again.status, 403, "{}", again.body());
    assert!(cookie_line(&again, SESSION_COOKIE).is_none());
    assert!(
        again.body().contains("it has been used already"),
        "{}",
        again.body()
    );
}

#[tokio::test]
async fn a_login_that_took_too_long_is_refused_even_with_the_right_cookie() {
    let (r, p) = with_provider(JoinPolicy::Off);
    let alice = r.person("alice", "Alice", None);
    r.auth
        .link_external(
            &alice,
            &ExternalIdentity::new(ISSUER, "alice-at-idp").unwrap(),
            NOW,
        )
        .unwrap();
    p.person("alice-at-idp");
    // The clock the pages run on stands at NOW; a login begun long before it has expired by then. (The
    // authenticator keeps time as passed in: begin it in the past through the library.)
    let client = inklkunn_auth::ProxyPolicy::none().client("203.0.113.5".parse().unwrap(), None);
    let begun = r
        .auth
        .begin_oidc(
            inklkunn_auth::OidcPurpose::SignIn {
                next: None,
                after_joining: None,
            },
            &client,
            NOW - 3600,
        )
        .unwrap();
    let a = come_back(
        &r,
        &begun.state,
        Some(&format!("{STATE_COOKIE}={}", begun.state)),
        &[],
    )
    .await;
    assert_eq!(a.status, 403, "{}", a.body());
    assert!(cookie_line(&a, SESSION_COOKIE).is_none());
}

// ------------------------------------------------------------------------------------ who is let in

#[tokio::test]
async fn an_identity_is_linked_to_an_account_only_by_the_operator_or_where_the_server_says() {
    // Joining off: an identity nobody linked is told so, and nothing is made.
    let (r, p) = with_provider(JoinPolicy::Off);
    p.person("newcomer");
    let a = sign_in_with_the_provider(&r, "/-/sign-in/oidc").await;
    assert_eq!(a.status, 403, "{}", a.body());
    let body = a.body();
    assert!(
        body.contains("no account here is linked to your sign-in there")
            && body.contains("Ask the person who runs this wiki for an invitation."),
        "{body}"
    );
    assert!(account_of(&r, "newcomer").is_none());
    assert!(cookie_line(&a, SESSION_COOKIE).is_none());
    // The operator links it (as `inklkunn account link` does), and the same sign-in lets the person in.
    let carol = r.person("carol", "Carol", Some(Role::Reviewer));
    r.auth
        .link_external(
            &carol,
            &ExternalIdentity::new(ISSUER, "newcomer").unwrap(),
            NOW,
        )
        .unwrap();
    let a = sign_in_with_the_provider(&r, "/-/sign-in/oidc").await;
    assert_eq!(a.status, 200, "{}", a.body());
    let secret = cookie_value(&a, SESSION_COOKIE).unwrap();
    let account = r
        .get(
            "/-/account",
            &[("cookie", &format!("{SESSION_COOKIE}={secret}"))],
        )
        .await;
    assert_eq!(account.status, 200);
    assert!(account.body().contains("Carol"));

    // A disabled account's identity is let in no more.
    r.auth.disable(&carol, NOW).unwrap();
    let a = sign_in_with_the_provider(&r, "/-/sign-in/oidc").await;
    assert_eq!(a.status, 401, "{}", a.body());
    assert!(cookie_line(&a, SESSION_COOKIE).is_none());
}

#[tokio::test]
async fn where_the_server_lets_a_first_provider_sign_in_make_an_account_it_has_no_role_and_the_new_quotas()
 {
    for join in [JoinPolicy::Provider, JoinPolicy::Anyone] {
        let (r, p) = with_provider(join);
        p.person("newcomer");
        let done = sign_in_with_the_provider(&r, "/-/sign-in/oidc?next=/garden/Home").await;
        assert_eq!(done.status, 200, "{join:?}: {}", done.body());
        // A new account lands on its account page, where the placeholder name can be changed.
        assert!(
            done.body().contains("content=\"0;url=/-/account\""),
            "{join:?}: {}",
            done.body()
        );
        let account = account_of(&r, "newcomer").expect("the account");
        assert!(account.local.is_none(), "no password");
        assert_eq!(
            account.created_at, NOW,
            "so the new-account quotas apply from now"
        );
        assert_eq!(r.w.store.role(&account.id, &r.w.wiki.id).unwrap(), None);
        // The next sign-in is the same account, and lands where the person was going.
        let again = sign_in_with_the_provider(&r, "/-/sign-in/oidc?next=/garden/Home").await;
        assert!(
            again.body().contains("content=\"0;url=/garden/Home\""),
            "{}",
            again.body()
        );
        assert_eq!(
            account_of(&r, "newcomer").map(|a| a.id),
            Some(account.id),
            "one person, one account"
        );
    }
}

#[tokio::test]
async fn an_account_is_never_matched_or_linked_by_email() {
    let (r, p) = with_provider(JoinPolicy::Provider);
    // An account whose name looks like the e-mail address the provider reports for a stranger.
    let alice = r.person("alice", "alice@example.org", Some(Role::Admin));
    p.person("a-stranger");
    let done = sign_in_with_the_provider(&r, "/-/sign-in/oidc").await;
    assert_eq!(done.status, 200);
    let stranger = account_of(&r, "a-stranger").unwrap();
    assert_ne!(
        stranger.id, alice,
        "a new account, not the one the e-mail names"
    );
    assert_eq!(r.w.store.role(&stranger.id, &r.w.wiki.id).unwrap(), None);
    assert_eq!(
        r.w.store.role(&alice, &r.w.wiki.id).unwrap(),
        Some(Role::Admin)
    );
    assert!(account_of(&r, "alice").is_none());
}

// ------------------------------------------------------------------------------------ an invitation

#[tokio::test]
async fn an_invitation_may_be_redeemed_with_the_provider_instead_of_a_password() {
    // Joining is off: an invitation is how this server's accounts come into being.
    let (r, p) = with_provider(JoinPolicy::Off);
    let secret = invitation(&r, Some(Role::Reviewer));
    let uri = format!("/-/invite/{secret}/oidc");
    p.person("invited");
    let s = start(&r, &uri, &[]).await;
    // The secret goes to the provider in no form: not in the address, not in the cookie.
    let location = s.answer.header("location").unwrap();
    assert!(!location.contains(&secret), "{location}");
    assert!(
        !cookie_line(&s.answer, STATE_COOKIE)
            .unwrap()
            .contains(&secret)
    );
    let done = come_back(&r, &s.state, Some(&s.cookie()), &[]).await;
    assert_eq!(done.status, 200, "{}", done.body());
    assert!(
        done.body().contains("content=\"0;url=/-/account\""),
        "{}",
        done.body()
    );
    assert!(!done.body().contains(&secret));
    for line in set_cookies(&done) {
        assert!(!line.contains(&secret), "{line}");
    }
    // The account is bound to the provider's identity, has no password, and has the role the invitation named.
    let account = account_of(&r, "invited").expect("the account");
    assert!(account.local.is_none());
    assert_eq!(
        r.w.store.role(&account.id, &r.w.wiki.id).unwrap(),
        Some(Role::Reviewer)
    );
    // It is signed in.
    let session = cookie_value(&done, SESSION_COOKIE).unwrap();
    let page = r
        .get(
            "/-/account",
            &[("cookie", &format!("{SESSION_COOKIE}={session}"))],
        )
        .await;
    assert_eq!(page.status, 200);
    // The link is used up: the same page as for any link that was used, at its address and its provider start.
    for uri in [format!("/-/invite/{secret}"), uri.clone()] {
        let a = r.get(&uri, &[]).await;
        assert_eq!(a.status, 404, "{uri}");
        assert!(a.body().contains("This invitation cannot be used"), "{uri}");
    }
    // And the account signs in with the provider as itself afterwards.
    let again = sign_in_with_the_provider(&r, "/-/sign-in/oidc").await;
    assert_eq!(again.status, 200);
    assert_eq!(
        account_of(&r, "invited").map(|a| a.id),
        Some(account.id),
        "no second account"
    );
}

#[tokio::test]
async fn an_invitation_that_is_no_good_does_not_start_a_sign_in_and_one_used_meanwhile_is_not_redeemed_twice()
 {
    let (r, p) = with_provider(JoinPolicy::Off);
    // Never was, malformed, ended: the one page.
    let ended = r.auth.invite(None, 10, NOW - 1000).unwrap();
    for uri in [
        format!("/-/invite/inv_{}/oidc", "A".repeat(43)),
        "/-/invite/not-an-invitation/oidc".to_owned(),
        format!("/-/invite/{}/oidc", ended.secret.as_str()),
    ] {
        let a = r.get(&uri, &[]).await;
        assert_eq!(a.status, 404, "{uri}");
        assert!(a.body().contains("This invitation cannot be used"), "{uri}");
        assert!(cookie_line(&a, STATE_COOKIE).is_none(), "{uri}");
    }
    // Started, and the link is used by someone else with a password before the person comes back.
    let secret = invitation(&r, None);
    p.person("slow");
    let s = start(&r, &format!("/-/invite/{secret}/oidc"), &[]).await;
    r.auth
        .redeem_with_password(
            &secret,
            "quick",
            "Quick",
            PASSWORD,
            &r.auth.client_address("203.0.113.7".parse().unwrap(), None),
            NOW,
        )
        .unwrap();
    let a = come_back(&r, &s.state, Some(&s.cookie()), &[]).await;
    assert_eq!(a.status, 404, "{}", a.body());
    assert!(a.body().contains("This invitation cannot be used"));
    assert!(account_of(&r, "slow").is_none());
    assert!(cookie_line(&a, SESSION_COOKIE).is_none());
    assert!(
        cookie_line(&a, STATE_COOKIE).is_some_and(|c| c.contains("Max-Age=0")),
        "the state cookie ends here too"
    );
}

#[tokio::test]
async fn a_provider_identity_that_has_an_account_spends_no_invitation_and_is_told_to_sign_in() {
    let (r, p) = with_provider(JoinPolicy::Off);
    let carol = r.person("carol", "Carol", None);
    r.auth
        .link_external(
            &carol,
            &ExternalIdentity::new(ISSUER, "carol-at-idp").unwrap(),
            NOW,
        )
        .unwrap();
    let secret = invitation(&r, Some(Role::Admin));
    p.person("carol-at-idp");
    let a = sign_in_with_the_provider(&r, &format!("/-/invite/{secret}/oidc")).await;
    assert_eq!(a.status, 409, "{}", a.body());
    let body = a.body();
    assert!(
        body.contains("Your sign-in with idp.example already has an account here, so this invitation was not used."),
        "{body}"
    );
    assert!(
        body.contains("Sign in with idp.example</a>"),
        "the way to sign in is offered: {body}"
    );
    assert!(cookie_line(&a, SESSION_COOKIE).is_none());
    // Still good for someone else; nobody got the role.
    assert!(r.auth.invitation_usable(&secret, NOW).unwrap());
    assert_eq!(r.w.store.role(&carol, &r.w.wiki.id).unwrap(), None);
}

// ------------------------------------------------------------------------------------ when it goes wrong

#[tokio::test]
async fn a_provider_that_says_no_or_cannot_be_reached_ends_on_the_sign_in_page_with_one_sentence() {
    let (r, p) = with_provider(JoinPolicy::Provider);
    p.person("newcomer");
    // The person declined at the provider, or the provider could not sign them in.
    let s = start(&r, "/-/sign-in/oidc", &[]).await;
    let a = r
        .get(
            &format!(
                "/-/sign-in/oidc/callback?error=access_denied&state={}",
                s.state
            ),
            &[("cookie", &s.cookie())],
        )
        .await;
    assert_eq!(a.status, 401, "{}", a.body());
    assert!(
        a.body()
            .contains("The sign-in with idp.example did not work, and nothing was changed."),
        "{}",
        a.body()
    );
    assert!(cookie_line(&a, STATE_COOKIE).is_some_and(|c| c.contains("Max-Age=0")));
    assert!(account_of(&r, "newcomer").is_none());
    // The provider is down when the token is asked for.
    p.goes_down();
    let a = sign_in_with_the_provider(&r, "/-/sign-in/oidc").await;
    assert_eq!(a.status, 502, "{}", a.body());
    assert!(
        a.body()
            .contains("The provider could not be reached right now"),
        "{}",
        a.body()
    );
    assert!(account_of(&r, "newcomer").is_none());
    // And a start that cannot even get the provider's document.
    let (r, p) = with_provider(JoinPolicy::Off);
    p.routes.lock().unwrap().clear();
    let a = r.get("/-/sign-in/oidc", &[]).await;
    assert_eq!(a.status, 502, "{}", a.body());
    assert!(
        a.body()
            .contains("The provider could not be reached right now")
    );
    assert!(cookie_line(&a, STATE_COOKIE).is_none());
}

#[tokio::test]
async fn starting_sign_ins_is_throttled_per_address_and_says_so() {
    let (r, _) = with_provider(JoinPolicy::Off);
    let peer: std::net::SocketAddr = "198.51.100.9:4000".parse().unwrap();
    let mut last = 0;
    for _ in 0..40 {
        last = r
            .call_from(Some(peer), "GET", "/-/sign-in/oidc", &[], "")
            .await
            .status
            .as_u16();
        if last == 429 {
            break;
        }
        assert_eq!(last, 303);
    }
    assert_eq!(last, 429, "thirty starts, then the address is held");
    let a = r
        .call_from(Some(peer), "GET", "/-/sign-in/oidc", &[], "")
        .await;
    assert_eq!(a.status, 429);
    assert!(a.header("retry-after").is_some());
    // Another address is not.
    let other: std::net::SocketAddr = "198.51.100.10:4000".parse().unwrap();
    assert_eq!(
        r.call_from(Some(other), "GET", "/-/sign-in/oidc", &[], "")
            .await
            .status,
        303
    );
}

#[tokio::test]
async fn the_callbacks_address_is_never_copied_into_the_language_buttons_return() {
    let (r, p) = with_provider(JoinPolicy::Off);
    p.person("newcomer");
    let s = start(&r, "/-/sign-in/oidc", &[]).await;
    let a = come_back(&r, &s.state, None, &[]).await;
    assert_eq!(a.status, 403);
    let body = a.body();
    // The refusal page has language buttons; none returns to an address that holds the code or the state.
    assert!(
        !body.contains("the-code") && !body.contains(&s.state),
        "{body}"
    );
}
