//! Signing in and out, joining, and opening an invitation (the sign-in handoff's U1 and U2; RFC 002 D-1). The
//! web layer renders and routes; `inklkunn_auth::Authenticator` does the work.
//!
//! * **`/-/sign-in`**: a sign-in name and a password. Constant-work and throttled as the authenticator is built
//!   (the client address is the peer's, or the one a *declared* trusted proxy forwards). **The refusal never says
//!   whether the account exists**: a wrong password, an unknown name, a disabled account and a name nobody could
//!   have are the same sentence.
//! * **`/-/sign-out`**: ends this browser's session. A form with the session's token, like every write.
//! * **`/-/join`**: where the operator opened joining to anyone, a person makes their own account. Otherwise it
//!   says joining is not open.
//! * **`/-/invite/{secret}`**: the one-time link the operator made. The person who opens it chooses a sign-in
//!   name and a password; the account is made and the link is used up in one step. A link that was used, one
//!   that ended and one that never existed are the same page.
//!
//! **These forms have no session yet to carry a synchronizer token**, so each carries a double-submit token: a
//! random value in a `__Host-` cookie and the same value in the form (`inklkunn_auth::session`), checked before
//! anything else; the same-site check stays beside it as the second wall. The session cookie that a success
//! sets is `__Host-`, `Secure`, `HttpOnly` and `SameSite=Strict`; the double-submit cookie is removed.
//!
//! * **`/-/sign-in/oidc`**, its callback and **`/-/invite/{secret}/oidc`** (`signin/oidc.rs`): the same two
//!   pages offer the configured provider, only when there is one. Joining through it is the server's
//!   `INKLKUNN_JOIN` setting and nothing else.
//!
//! Nothing here needs a script, and a page here is never cached or indexed.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use askama::Template;
use axum::extract::rejection::{FormRejection, PathRejection, QueryRejection};
use axum::extract::{ConnectInfo, Form, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use inklkunn_auth::hashing::PASSWORD_MIN_CHARS;
use inklkunn_auth::session::{CsrfToken, clear_cookie, form_token_matches};
use inklkunn_auth::{AccountError, AuthError, Authenticator, ClientAddress, SignedIn};
use inklkunn_store::{DisplayNameError, LoginNameError};

use crate::catalog::Key;
use crate::pages::{Page, PageState, answer, render_form_expired, render_form_refused};
use crate::services::Services;
use crate::wiki::{blocking, generic_not_found, redirect, render_failed};

pub(crate) mod names;
pub(crate) mod oidc;

use names::{F_LOGIN, F_NAME, F_NEXT, F_PASSWORD, FORWARDED_FOR, clear_form_cookie, invite_action};
pub(crate) use names::{
    INVITE_OIDC_ROUTE, INVITE_ROUTE, JOIN_PATH, OIDC_CALLBACK_PATH, OIDC_PATH, SIGN_IN_PATH,
    SIGN_OUT_PATH, no_store, sign_in_href,
};
pub(crate) use oidc::{ProviderLink, provider_name};

/// The most bytes one of these forms may carry: a name, a display name and a password, each percent-encoded.
pub(crate) const FORM_LIMIT: usize = 16 * 1024;

type Fields = Result<Form<HashMap<String, String>>, FormRejection>;

// ------------------------------------------------------------------------------------ what a request is

/// The peer that opened the connection, when the server knows it (`ConnectInfo`; a test that drives the router
/// directly has none).
pub(crate) struct Peer(pub(crate) Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for Peer {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Peer, Infallible> {
        Ok(Peer(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip()),
        ))
    }
}

/// The client address a throttle counts: the peer, unless it is a trusted proxy and says otherwise. A request
/// whose peer is unknown counts as one address (`0.0.0.0`): throttled together rather than not at all.
pub(crate) fn client_of(auth: &Authenticator, peer: &Peer, headers: &HeaderMap) -> ClientAddress {
    let forwarded: Vec<&str> = headers
        .get_all(FORWARDED_FOR)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    let forwarded = (!forwarded.is_empty()).then(|| forwarded.join(", "));
    auth.client_address(
        peer.0.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
        forwarded.as_deref(),
    )
}

/// The `Cookie` header(s) as one.
fn cookie_line(headers: &HeaderMap) -> Option<String> {
    let all: Vec<&str> = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    (!all.is_empty()).then(|| all.join("; "))
}

/// The services and the authenticator, if sign-in is served here.
fn served(state: &PageState) -> Option<(std::sync::Arc<Services>, std::sync::Arc<Authenticator>)> {
    let services = state.services.clone()?;
    let auth = services.auth.clone()?;
    Some((services, auth))
}

/// The field of a form, or "".
fn field<'a>(fields: &'a HashMap<String, String>, name: &str) -> &'a str {
    fields.get(name).map_or("", String::as_str)
}

/// Add a `Set-Cookie` to a response.
fn with_cookie(mut response: Response, cookie: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// A page nobody keeps or indexes: these addresses hold one-time secrets and tokens.
pub(crate) fn private(mut page: Page) -> Page {
    page.noindex = true;
    page
}

/// `Retry-After` for a lock that ends at `until`.
fn retry_after(response: Response, until: i64, now: i64) -> Response {
    let secs = until.saturating_sub(now).clamp(1, 24 * 3600);
    let mut response = response;
    if let Ok(value) = HeaderValue::from_str(&secs.to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

/// The form's double-submit token: right, or the page that says the form expired. Checked before anything
/// else is looked at.
fn double_submit(headers: &HeaderMap, fields: &HashMap<String, String>) -> bool {
    form_token_matches(
        cookie_line(headers).as_deref(),
        field(fields, crate::csrf::FIELD),
    )
}

fn expired(page: &Page) -> Response {
    let page = page.clone().without_switcher();
    answer(StatusCode::FORBIDDEN, &page, render_form_expired(&page))
}

fn not_from_this_site(page: &Page) -> Response {
    let page = page.clone().without_switcher();
    answer(StatusCode::FORBIDDEN, &page, render_form_refused(&page))
}

/// A session cookie and a removal of the double-submit cookie, on `response`.
fn signed_in_response(response: Response, signed_in: &SignedIn, now: i64) -> Response {
    let response = with_cookie(response, &signed_in.session.set_cookie(now));
    with_cookie(response, &clear_form_cookie())
}

// ------------------------------------------------------------------------------------ sign in

#[derive(Template)]
#[template(path = "sign_in.html")]
struct SignInTpl<'a> {
    page: &'a Page,
    title: String,
    heading: String,
    problem: Option<String>,
    /// Set when the visitor is signed in already.
    already: Option<String>,
    account_href: &'static str,
    to_account: String,
    action: &'static str,
    token: String,
    next: String,
    login: String,
    login_label: String,
    password_label: String,
    button: String,
    provider: Option<ProviderLink>,
    provider_joins: Option<String>,
    join_href: Option<&'static str>,
    join_label: String,
    invitation_only: String,
}

struct SignInView<'a> {
    problem: Option<String>,
    login: &'a str,
    next: &'a str,
}

fn sign_in_form(
    page: &Page,
    auth: &Authenticator,
    status: StatusCode,
    view: &SignInView<'_>,
) -> Response {
    let Ok(token) = CsrfToken::mint() else {
        return render_failed(page);
    };
    let open_to_all = auth.join_policy().lets_anyone_join_with_a_password();
    let provider = oidc::sign_in_link(page, view.next);
    let rendered = SignInTpl {
        page,
        title: page.title(Key::TitleSignIn),
        heading: page.get(Key::SignInHeading),
        problem: view.problem.clone(),
        already: page
            .signed_in_as()
            .map(|name| page.t().msg(Key::SignInAlready).with("name", name).text()),
        account_href: crate::account::PATH,
        to_account: page.get(Key::SignInToAccount),
        action: SIGN_IN_PATH,
        token: token.as_str().to_owned(),
        next: view.next.to_owned(),
        login: view.login.to_owned(),
        login_label: page.get(Key::SignInLoginLabel),
        password_label: page.get(Key::SignInPasswordLabel),
        button: page.get(Key::SignInButton),
        provider_joins: (provider.is_some() && auth.join_policy().lets_a_provider_sign_in_join())
            .then(|| oidc::provider_joins(page)),
        provider,
        join_href: open_to_all.then_some(JOIN_PATH),
        join_label: page.get(Key::SignInJoin),
        invitation_only: page.get(Key::SignInInvitationOnly),
    }
    .render();
    no_store(with_cookie(
        answer(status, page, rendered),
        &token.form_cookie(),
    ))
}

type Query1 = Result<Query<HashMap<String, String>>, QueryRejection>;

/// `GET /-/sign-in`.
pub(crate) async fn sign_in_page(
    State(state): State<PageState>,
    page: Page,
    query: Query1,
) -> Response {
    let Some((_, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    let page = private(page);
    let next = query
        .ok()
        .and_then(|Query(q)| q.get(F_NEXT).cloned())
        .and_then(|n| crate::lang::safe_back(&n).map(str::to_owned))
        .unwrap_or_else(|| "/".to_owned());
    sign_in_form(
        &page,
        &auth,
        StatusCode::OK,
        &SignInView {
            problem: None,
            login: "",
            next: &next,
        },
    )
}

/// `POST /-/sign-in`.
pub(crate) async fn sign_in_post(
    State(state): State<PageState>,
    page: Page,
    peer: Peer,
    headers: HeaderMap,
    form: Fields,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    let page = private(page);
    if !crate::lang::from_this_site(&headers) {
        return not_from_this_site(&page);
    }
    let Ok(Form(fields)) = form else {
        return expired(&page);
    };
    if !double_submit(&headers, &fields) {
        return expired(&page);
    }
    let next = crate::lang::safe_back(field(&fields, F_NEXT))
        .unwrap_or("/")
        .to_owned();
    let login = field(&fields, F_LOGIN).trim().to_owned();
    let password = field(&fields, F_PASSWORD).to_owned();
    let client = client_of(&auth, &peer, &headers);
    let now = (services.clock)();
    let (for_work, shown) = (auth.clone(), (login.clone(), next.clone()));
    let result = blocking(move || for_work.sign_in(&login, &password, &client, now)).await;
    let again = |status: StatusCode, problem: Key| {
        sign_in_form(
            &page,
            &auth,
            status,
            &SignInView {
                problem: Some(page.get(problem)),
                login: &shown.0,
                next: &shown.1,
            },
        )
    };
    match result {
        None => render_failed(&page),
        Some(Ok(signed_in)) => signed_in_response(redirect(&next), &signed_in, now),
        Some(Err(AuthError::BadCredentials)) => again(StatusCode::UNAUTHORIZED, Key::SignInFailed),
        Some(Err(AuthError::Locked { until })) => retry_after(
            again(StatusCode::TOO_MANY_REQUESTS, Key::SignInLocked),
            until,
            now,
        ),
        Some(Err(AuthError::Busy)) => retry_after(
            again(StatusCode::SERVICE_UNAVAILABLE, Key::SignInBusy),
            now + 5,
            now,
        ),
        Some(Err(other)) => {
            (services.log)(&format!("a sign-in could not be completed: {other}"));
            again(StatusCode::SERVICE_UNAVAILABLE, Key::ProblemUnavailable)
        }
    }
}

// ------------------------------------------------------------------------------------ sign out

#[derive(Template)]
#[template(path = "sign_out.html")]
struct SignOutTpl<'a> {
    page: &'a Page,
    title: String,
    heading: String,
    signed_in: bool,
    body: String,
    button: String,
    sign_in_href: Option<String>,
    sign_in_label: String,
}

/// `GET /-/sign-out`: the button, for someone who followed a link; changes nothing.
pub(crate) async fn sign_out_page(State(state): State<PageState>, page: Page) -> Response {
    if served(&state).is_none() {
        return generic_not_found(&page);
    }
    let page = private(page);
    let signed_in = page.who.is_some();
    let rendered = SignOutTpl {
        page: &page,
        title: page.title(Key::TitleSignOut),
        heading: page.get(Key::SignOutHeading),
        signed_in,
        body: page.get(if signed_in {
            Key::SignOutBody
        } else {
            Key::SignOutNotSignedIn
        }),
        button: page.get(Key::HeaderSignOut),
        sign_in_href: (!signed_in).then(|| SIGN_IN_PATH.to_owned()),
        sign_in_label: page.get(Key::HeaderSignIn),
    }
    .render();
    no_store(answer(StatusCode::OK, &page, rendered))
}

/// `POST /-/sign-out`: end this browser's session and remove its cookie. The session's token was checked in
/// front of the handler, as for every write; a browser with no session goes to the start page too.
pub(crate) async fn sign_out_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    if !crate::lang::from_this_site(&headers) {
        return not_from_this_site(&private(page));
    }
    let secret = crate::identity::session_secret(&headers);
    let ended = blocking(move || {
        if let Some(secret) = secret
            && let Err(e) = auth.sign_out(&secret)
        {
            (services.log)(&format!("a session could not be ended: {e}"));
        }
    })
    .await;
    if ended.is_none() {
        return render_failed(&page);
    }
    no_store(with_cookie(redirect("/"), &clear_cookie()))
}

// ------------------------------------------------------------------------------------ joining and invitations

#[derive(Template)]
#[template(path = "new_account.html")]
struct NewAccountTpl<'a> {
    page: &'a Page,
    title: String,
    heading: String,
    intro: String,
    problem: Option<String>,
    action: String,
    token: String,
    login: String,
    name: String,
    login_label: String,
    login_hint: String,
    name_label: String,
    name_hint: String,
    password_label: String,
    password_hint: String,
    button: String,
    provider: Option<ProviderLink>,
    provider_intro: String,
}

/// What a page that makes an account is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Making {
    Join,
    Invitation,
}

/// What was typed, to put back after a problem (never the password).
#[derive(Default)]
struct Typed {
    login: String,
    name: String,
}

fn new_account_form(
    page: &Page,
    making: Making,
    action: String,
    status: StatusCode,
    problem: Option<String>,
    typed: &Typed,
) -> Response {
    let Ok(token) = CsrfToken::mint() else {
        return render_failed(page);
    };
    let (title, heading, intro, button) = match making {
        Making::Join => (
            Key::TitleJoin,
            Key::JoinHeading,
            Key::JoinIntro,
            Key::JoinButton,
        ),
        Making::Invitation => (
            Key::TitleInvite,
            Key::InviteHeading,
            Key::InviteIntro,
            Key::InviteButton,
        ),
    };
    // Only an invitation offers the provider here: a person who joins without one signs in with it from the
    // sign-in page, where joining through it is the server's own setting.
    let provider = (making == Making::Invitation)
        .then(|| oidc::invitation_link(page, &action))
        .flatten();
    let rendered = NewAccountTpl {
        page,
        title: page.title(title),
        heading: page.get(heading),
        intro: page.get(intro),
        problem,
        action,
        token: token.as_str().to_owned(),
        login: typed.login.clone(),
        name: typed.name.clone(),
        login_label: page.get(Key::SignInLoginLabel),
        login_hint: page.get(Key::AccountFormLoginHint),
        name_label: page.get(Key::AccountFormNameLabel),
        name_hint: page.get(Key::AccountFormNameHint),
        password_label: page.get(Key::AccountFormPasswordLabel),
        password_hint: page
            .t()
            .msg(Key::AccountFormPasswordHint)
            .with("min", PASSWORD_MIN_CHARS.to_string())
            .text(),
        button: page.get(button),
        provider,
        provider_intro: oidc::invitation_intro(page),
    }
    .render();
    no_store(with_cookie(
        answer(status, page, rendered),
        &token.form_cookie(),
    ))
}

/// What a refusal to make an account says, and with which status.
fn refusal_of(page: &Page, error: &AccountError) -> Option<(StatusCode, String)> {
    let t = page.t();
    let say = |key: Key| Some((StatusCode::UNPROCESSABLE_ENTITY, page.get(key)));
    match error {
        AccountError::LoginTaken => {
            Some((StatusCode::CONFLICT, page.get(Key::AccountFormLoginTaken)))
        }
        AccountError::Login(LoginNameError::Length) => say(Key::AccountFormLoginLength),
        AccountError::Login(_) => say(Key::AccountFormLoginCharacters),
        AccountError::DisplayName(DisplayNameError::Length) => say(Key::AccountFormNameLength),
        AccountError::DisplayName(DisplayNameError::Control) => say(Key::AccountFormNameControl),
        AccountError::DisplayName(_) => say(Key::AccountFormNameSpace),
        AccountError::Password(inklkunn_auth::PasswordRefusal::TooShort) => Some((
            StatusCode::UNPROCESSABLE_ENTITY,
            t.msg(Key::AccountFormPasswordShort)
                .with("min", PASSWORD_MIN_CHARS.to_string())
                .text(),
        )),
        AccountError::Password(_) => say(Key::AccountFormPasswordLong),
        _ => None,
    }
}

/// `GET /-/join`.
pub(crate) async fn join_page(State(state): State<PageState>, page: Page) -> Response {
    let Some((_, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    let page = private(page);
    if !auth.join_policy().lets_anyone_join_with_a_password() {
        return join_closed(&page);
    }
    new_account_form(
        &page,
        Making::Join,
        JOIN_PATH.to_owned(),
        StatusCode::OK,
        None,
        &Typed::default(),
    )
}

fn closed_notice(page: &Page, title: Key, heading: Key, body: Key, link: bool) -> Response {
    let link = (link && page.sign_in).then(|| (SIGN_IN_PATH, page.get(Key::HeaderSignIn)));
    let rendered = crate::pages::render_notice(page, title, heading, body, link);
    no_store(answer(StatusCode::NOT_FOUND, page, rendered))
}

fn join_closed(page: &Page) -> Response {
    closed_notice(
        page,
        Key::TitleJoin,
        Key::JoinClosedHeading,
        Key::JoinClosedBody,
        true,
    )
}

fn invitation_unusable(page: &Page) -> Response {
    closed_notice(
        page,
        Key::TitleInvite,
        Key::InviteUnusableHeading,
        Key::InviteUnusableBody,
        false,
    )
}

/// What a request to make an account carries once its token is accepted.
struct Submitted {
    login: String,
    name: String,
    password: String,
}

fn submitted(fields: &HashMap<String, String>) -> Submitted {
    Submitted {
        login: field(fields, F_LOGIN).trim().to_owned(),
        name: field(fields, F_NAME).trim().to_owned(),
        password: field(fields, F_PASSWORD).to_owned(),
    }
}

/// The page after a problem with what was typed, or `None` for a failure of the server.
fn problem_page(
    page: &Page,
    making: Making,
    action: String,
    error: &AccountError,
    typed: &Submitted,
    log: &dyn Fn(&str),
    now: i64,
) -> Response {
    let typed_back = Typed {
        login: typed.login.clone(),
        name: typed.name.clone(),
    };
    match error {
        AccountError::Auth(AuthError::Locked { until }) => retry_after(
            new_account_form(
                page,
                making,
                action,
                StatusCode::TOO_MANY_REQUESTS,
                Some(page.get(Key::SignInLocked)),
                &typed_back,
            ),
            *until,
            now,
        ),
        AccountError::Auth(AuthError::Busy) => retry_after(
            new_account_form(
                page,
                making,
                action,
                StatusCode::SERVICE_UNAVAILABLE,
                Some(page.get(Key::SignInBusy)),
                &typed_back,
            ),
            now + 5,
            now,
        ),
        other => match refusal_of(page, other) {
            Some((status, text)) => {
                new_account_form(page, making, action, status, Some(text), &typed_back)
            }
            None => {
                log(&format!("an account could not be made: {other}"));
                new_account_form(
                    page,
                    making,
                    action,
                    StatusCode::SERVICE_UNAVAILABLE,
                    Some(page.get(Key::ProblemUnavailable)),
                    &typed_back,
                )
            }
        },
    }
}

/// `POST /-/join`.
pub(crate) async fn join_post(
    State(state): State<PageState>,
    page: Page,
    peer: Peer,
    headers: HeaderMap,
    form: Fields,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    let page = private(page);
    if !crate::lang::from_this_site(&headers) {
        return not_from_this_site(&page);
    }
    let Ok(Form(fields)) = form else {
        return expired(&page);
    };
    if !double_submit(&headers, &fields) {
        return expired(&page);
    }
    if !auth.join_policy().lets_anyone_join_with_a_password() {
        return join_closed(&page);
    }
    let typed = submitted(&fields);
    let client = client_of(&auth, &peer, &headers);
    let now = (services.clock)();
    let (for_work, input) = (
        auth.clone(),
        (
            typed.login.clone(),
            typed.name.clone(),
            typed.password.clone(),
        ),
    );
    let result =
        blocking(move || for_work.join_with_password(&input.0, &input.1, &input.2, &client, now))
            .await;
    match result {
        None => render_failed(&page),
        Some(Ok(signed_in)) => signed_in_response(redirect(crate::account::PATH), &signed_in, now),
        Some(Err(AccountError::JoinClosed)) => join_closed(&page),
        Some(Err(error)) => problem_page(
            &page,
            Making::Join,
            JOIN_PATH.to_owned(),
            &error,
            &typed,
            &*services.log,
            now,
        ),
    }
}

type Secret = Result<Path<String>, PathRejection>;

/// `GET /-/invite/{secret}`.
pub(crate) async fn invite_page(
    State(state): State<PageState>,
    page: Page,
    secret: Secret,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    let page = private(page);
    let Ok(Path(secret)) = secret else {
        return invitation_unusable(&page);
    };
    let now = (services.clock)();
    let usable = {
        let (auth, secret) = (auth.clone(), secret.clone());
        blocking(move || auth.invitation_usable(&secret, now)).await
    };
    match usable {
        None => render_failed(&page),
        Some(Ok(true)) => {
            let action = invite_action(&secret);
            new_account_form(
                &page,
                Making::Invitation,
                action,
                StatusCode::OK,
                None,
                &Typed::default(),
            )
        }
        Some(Ok(false)) => invitation_unusable(&page),
        Some(Err(e)) => {
            (services.log)(&format!("an invitation could not be checked: {e}"));
            render_failed(&page)
        }
    }
}

/// `POST /-/invite/{secret}`.
pub(crate) async fn invite_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    secret: Secret,
    form: Fields,
) -> Response {
    let Some((services, auth)) = served(&state) else {
        return generic_not_found(&page);
    };
    let page = private(page);
    if !crate::lang::from_this_site(&headers) {
        return not_from_this_site(&page);
    }
    let Ok(Path(secret)) = secret else {
        return invitation_unusable(&page);
    };
    let Ok(Form(fields)) = form else {
        return expired(&page);
    };
    if !double_submit(&headers, &fields) {
        return expired(&page);
    }
    let typed = submitted(&fields);
    let now = (services.clock)();
    let action = invite_action(&secret);
    let (for_work, input) = (
        auth.clone(),
        (
            secret,
            typed.login.clone(),
            typed.name.clone(),
            typed.password.clone(),
        ),
    );
    let result = blocking(move || {
        for_work.redeem_with_password(&input.0, &input.1, &input.2, &input.3, now)
    })
    .await;
    match result {
        None => render_failed(&page),
        Some(Ok(signed_in)) => signed_in_response(redirect(crate::account::PATH), &signed_in, now),
        Some(Err(AccountError::InvitationUnusable)) => invitation_unusable(&page),
        Some(Err(error)) => problem_page(
            &page,
            Making::Invitation,
            action,
            &error,
            &typed,
            &*services.log,
            now,
        ),
    }
}
