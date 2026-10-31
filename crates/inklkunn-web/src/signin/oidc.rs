//! Signing in with the configured provider (the OpenID Connect handoff's U3; RFC 002 D-1; handoff 002 C9).
//! The web layer renders and routes; `inklkunn_auth::Authenticator` does the protocol.
//!
//! * **`GET /-/sign-in/oidc`** starts a sign-in: the browser is sent to the provider with the login-state
//!   cookie (`SameSite=Lax`: the provider's redirect back is a cross-site navigation, which a `Strict` cookie
//!   would not accompany). **`GET /-/invite/{secret}/oidc`** starts an invitation the same way, so the person
//!   who opens the link may use the provider instead of choosing a password. They are links, not forms: the
//!   page policy's `form-action 'self'` is enforced by browsers on the redirects that follow a form
//!   submission too, so a form that ends at the provider would be refused; a link is a navigation, which it
//!   does not govern.
//! * **`GET /-/sign-in/oidc/callback`** is where the provider sends the browser back. The state in the query and
//!   the state in the cookie must be the same one and must be a sign-in this server started a moment ago (a
//!   callback this browser did not start, or one used twice, is refused before any account is looked at). A
//!   success is answered with **a `200` page that moves the browser on** ([`landing`]), never a redirect: the
//!   session cookie is `SameSite=Strict`, so a redirect that follows a cross-site navigation would reach the
//!   next page without it (`inklkunn_auth::Landing` says why). The page's words are catalog messages in the
//!   reader's language, drawn here (handoff 002's F4); `inklkunn-auth` hands over only the target, the
//!   cookies and the headers.
//!
//! What the sign-in is *for* is kept on the server with the login in flight, not in a cookie or in the
//! callback's address, so an invitation's secret never reaches the provider's logs or the browser's history.
//! A refusal at the callback is the sign-in page with one sentence, and removes the login-state cookie.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::Response;
use inklkunn_auth::oidc::{OidcError, clear_state_cookie};
use inklkunn_auth::{AuthError, Authenticator, OidcPurpose, OidcSignInError, OidcSignedIn};

use super::names::{
    F_NEXT, Q_CODE, Q_ERROR, Q_STATE, invite_action, invite_oidc_action, no_store, oidc_start_href,
};
use super::{
    Making, Peer, SignInView, client_of, cookie_line, invitation_unusable, new_account_form,
    private, retry_after, served, sign_in_form, with_cookie,
};
use crate::catalog::{Key, Rich};
use crate::pages::{Page, PageState, answer};
use crate::services::Services;
use crate::wiki::{blocking, generic_not_found, render_failed};

type Query1 = Result<Query<HashMap<String, String>>, QueryRejection>;
type Secret = Result<Path<String>, PathRejection>;

/// The provider's name on a page: the host of its issuer (`https://accounts.example.org/realms/x` is
/// `accounts.example.org`), which is what a person recognizes and the only part of the address worth reading.
pub(crate) fn provider_name(issuer: &str) -> String {
    let rest = issuer.strip_prefix("https://").unwrap_or(issuer);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or(rest)
        .to_owned()
}

/// *Sign in with …*: where it leads and what it says.
pub(crate) struct ProviderLink {
    pub(crate) href: String,
    pub(crate) label: String,
}

/// The link that starts a sign-in with the provider, returning to `next` (a local path), when one is configured.
pub(crate) fn sign_in_link(page: &Page, next: &str) -> Option<ProviderLink> {
    let name = page.provider.as_deref()?;
    Some(ProviderLink {
        href: oidc_start_href(next),
        label: page
            .t()
            .msg(Key::SignInProvider)
            .with("provider", name)
            .text(),
    })
}

/// The link that makes the account an invitation is for with the provider instead of a password, when one is
/// configured. `action` is the invitation's own address.
pub(crate) fn invitation_link(page: &Page, action: &str) -> Option<ProviderLink> {
    let name = page.provider.as_deref()?;
    Some(ProviderLink {
        href: invite_oidc_action(action),
        label: page
            .t()
            .msg(Key::InviteProviderLink)
            .with("provider", name)
            .text(),
    })
}

/// A sentence of the catalog that names the provider.
fn about_provider(page: &Page, key: Key) -> String {
    page.t()
        .msg(key)
        .with("provider", page.provider.as_deref().unwrap_or_default())
        .text()
}

/// The sentence under *Sign in with …* where a first sign-in makes an account.
pub(crate) fn provider_joins(page: &Page) -> String {
    about_provider(page, Key::SignInProviderJoins)
}

/// The sentence above the provider link of an invitation.
pub(crate) fn invitation_intro(page: &Page) -> String {
    page.get(Key::InviteProviderIntro)
}

// ------------------------------------------------------------------------------------ starting

/// Send the browser to the provider, with the login-state cookie. The response is never kept.
fn to_the_provider(page: &Page, begun: &inklkunn_auth::oidc::BeginSignIn) -> Response {
    let (Ok(location), Ok(cookie)) = (
        HeaderValue::from_str(&begun.authorization_url),
        HeaderValue::from_str(&begun.state_cookie()),
    ) else {
        return render_failed(page);
    };
    let mut response = Response::new(axum::body::Body::empty());
    *response.status_mut() = StatusCode::SEE_OTHER;
    let headers = response.headers_mut();
    headers.insert(header::LOCATION, location);
    headers.append(header::SET_COOKIE, cookie);
    no_store(response)
}

/// What a sign-in that could not even start says.
fn not_started(
    page: &Page,
    services: &Services,
    auth: &Authenticator,
    error: &OidcSignInError,
    invitation: Option<&str>,
    now: i64,
) -> Response {
    let again = |status, problem: String| match invitation {
        Some(action) => new_account_form(
            page,
            Making::Invitation,
            action.to_owned(),
            status,
            Some(problem),
            &super::Typed::default(),
        ),
        None => sign_in_form(
            page,
            auth,
            status,
            &SignInView {
                problem: Some(problem),
                login: "",
                next: "/",
            },
        ),
    };
    match error {
        OidcSignInError::Oidc(OidcError::NotConfigured) => generic_not_found(page),
        OidcSignInError::InvitationUnusable => invitation_unusable(page),
        OidcSignInError::Auth(AuthError::Locked { until }) => retry_after(
            again(StatusCode::TOO_MANY_REQUESTS, page.get(Key::SignInLocked)),
            *until,
            now,
        ),
        OidcSignInError::Oidc(e) => {
            (services.log)(&format!(
                "a sign-in through the provider could not start: {e}"
            ));
            again(
                StatusCode::BAD_GATEWAY,
                about_provider(page, Key::SignInProviderUnreachable),
            )
        }
        other => {
            (services.log)(&format!(
                "a sign-in through the provider could not start: {other}"
            ));
            again(
                StatusCode::SERVICE_UNAVAILABLE,
                page.get(Key::ProblemUnavailable),
            )
        }
    }
}

/// `GET /-/sign-in/oidc`: start a sign-in. `next` is where the person lands (a local path; anything else is
/// the start page).
pub(crate) async fn start(
    State(state): State<PageState>,
    page: Page,
    peer: Peer,
    headers: HeaderMap,
    query: Query1,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    if !auth.oidc_configured() {
        return generic_not_found(&page);
    }
    let page = private(page);
    let next = query
        .ok()
        .and_then(|Query(q)| q.get(F_NEXT).cloned())
        .and_then(|n| crate::lang::safe_back(&n).map(str::to_owned))
        .unwrap_or_else(|| "/".to_owned());
    let client = client_of(&auth, &peer, &headers);
    let now = (services.clock)();
    let for_work = auth.clone();
    let result = blocking(move || {
        for_work.begin_oidc(
            OidcPurpose::SignIn {
                next: Some(&next),
                after_joining: Some(crate::account::PATH),
            },
            &client,
            now,
        )
    })
    .await;
    match result {
        None => render_failed(&page),
        Some(Ok(begun)) => to_the_provider(&page, &begun),
        Some(Err(e)) => not_started(&page, &services, &auth, &e, None, now),
    }
}

/// `GET /-/invite/{secret}/oidc`: start the invitation with the provider. The secret is in this address (as in
/// the invitation's own) and from here on only on the server.
pub(crate) async fn start_invitation(
    State(state): State<PageState>,
    page: Page,
    peer: Peer,
    headers: HeaderMap,
    secret: Secret,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    if !auth.oidc_configured() {
        return generic_not_found(&page);
    }
    let page = private(page);
    let Ok(Path(secret)) = secret else {
        return invitation_unusable(&page);
    };
    let action = invite_action(&secret);
    let client = client_of(&auth, &peer, &headers);
    let now = (services.clock)();
    let for_work = auth.clone();
    let result = blocking(move || {
        for_work.begin_oidc(
            OidcPurpose::Invitation {
                secret: &secret,
                next: Some(crate::account::PATH),
            },
            &client,
            now,
        )
    })
    .await;
    match result {
        None => render_failed(&page),
        Some(Ok(begun)) => to_the_provider(&page, &begun),
        Some(Err(e)) => not_started(&page, &services, &auth, &e, Some(&action), now),
    }
}

// ------------------------------------------------------------------------------------ the callback

/// The page that moves the browser on after a sign-in: standalone (no layout, no style, no script, no
/// sub-resource), so the policy that allows nothing is enough, and it says in a `<meta>` that the address it
/// was reached at (whose query held the provider's code) is not to be sent on as a referrer.
#[derive(Template)]
#[template(path = "landing.html")]
struct LandingTpl {
    lang: &'static str,
    dir: &'static str,
    title: String,
    target: String,
    body: Rich,
}

/// The answer to a callback that signed somebody in: the page, with exactly the status, headers and cookies
/// the authenticator handed over.
fn landing(page: &Page, done: &OidcSignedIn) -> Response {
    let landing = &done.landing;
    let link_label = page.get(Key::LandingContinue);
    let rendered = LandingTpl {
        lang: page.lang_tag(),
        dir: page.dir(),
        title: page.title(Key::TitleLanding),
        target: landing.target().to_owned(),
        body: page
            .t()
            .msg(Key::LandingBody)
            .link("link", link_label, landing.target())
            .html(),
    }
    .render();
    let status = StatusCode::from_u16(landing.status()).unwrap_or(StatusCode::OK);
    let mut response = answer(status, page, rendered);
    let headers = response.headers_mut();
    for (name, value) in landing.headers() {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(name, value);
        }
    }
    for cookie in landing.set_cookies() {
        if let Ok(value) = HeaderValue::from_str(cookie) {
            headers.append(header::SET_COOKIE, value);
        }
    }
    response
}

/// What a callback that did not sign anybody in says, with the status that goes with it.
fn refusal(page: &Page, error: &OidcSignInError) -> (StatusCode, String) {
    let about = |key| about_provider(page, key);
    match error {
        OidcSignInError::Oidc(OidcError::State) => {
            (StatusCode::FORBIDDEN, page.get(Key::SignInProviderState))
        }
        OidcSignInError::Oidc(OidcError::Provider(_)) => (
            StatusCode::BAD_GATEWAY,
            about(Key::SignInProviderUnreachable),
        ),
        OidcSignInError::NotLinked => (
            StatusCode::FORBIDDEN,
            page.get(Key::SignInProviderNotLinked),
        ),
        OidcSignInError::IdentityTaken => (StatusCode::CONFLICT, about(Key::InviteProviderTaken)),
        OidcSignInError::Auth(AuthError::Locked { .. }) => {
            (StatusCode::TOO_MANY_REQUESTS, page.get(Key::SignInLocked))
        }
        OidcSignInError::Auth(AuthError::Busy) => {
            (StatusCode::SERVICE_UNAVAILABLE, page.get(Key::SignInBusy))
        }
        OidcSignInError::Auth(AuthError::BadCredentials) | OidcSignInError::Oidc(_) => {
            (StatusCode::UNAUTHORIZED, about(Key::SignInProviderFailed))
        }
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            page.get(Key::ProblemUnavailable),
        ),
    }
}

/// `GET /-/sign-in/oidc/callback`.
pub(crate) async fn callback(
    State(state): State<PageState>,
    page: Page,
    peer: Peer,
    headers: HeaderMap,
    query: Query1,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    if !auth.oidc_configured() {
        return generic_not_found(&page);
    }
    let page = private(page);
    let q = query.map(|Query(q)| q).unwrap_or_default();
    let field = |name: &str| q.get(name).cloned().unwrap_or_default();
    let (state_param, code) = (field(Q_STATE), field(Q_CODE));
    let client = client_of(&auth, &peer, &headers);
    let now = (services.clock)();
    // The provider says no (the person declined, or it could not sign them in): nothing to finish, and the
    // login in flight is left to end by itself. The reason is the provider's word, for the log.
    let refused_by_provider = q.contains_key(Q_ERROR);
    let cookies = cookie_line(&headers);
    let for_work = auth.clone();
    let result = if refused_by_provider {
        Some(Err(OidcSignInError::Oidc(OidcError::Provider(
            "the provider answered with an error".to_owned(),
        ))))
    } else {
        blocking(move || {
            for_work.finish_oidc(&state_param, cookies.as_deref(), &code, &client, now)
        })
        .await
    };
    match result {
        None => render_failed(&page),
        Some(Ok(done)) => landing(&page, &done),
        Some(Err(OidcSignInError::InvitationUnusable)) => {
            with_cookie(invitation_unusable(&page), &clear_state_cookie())
        }
        Some(Err(error)) => {
            (services.log)(&format!(
                "a sign-in through the provider did not complete: {error}"
            ));
            let (status, problem) = if refused_by_provider {
                (
                    StatusCode::UNAUTHORIZED,
                    about_provider(&page, Key::SignInProviderFailed),
                )
            } else {
                refusal(&page, &error)
            };
            let response = sign_in_form(
                &page,
                &auth,
                status,
                &SignInView {
                    problem: Some(problem),
                    login: "",
                    next: "/",
                },
            );
            let response = match &error {
                OidcSignInError::Auth(AuthError::Locked { until }) => {
                    retry_after(response, *until, now)
                }
                _ => response,
            };
            with_cookie(response, &clear_state_cookie())
        }
    }
}
