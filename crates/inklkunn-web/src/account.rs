//! The account page (`/-/account`; the sign-in handoff's U1): the signed-in person's display name, the sessions
//! they hold with *End this session* each, and their API tokens: made with a scope and an expiry, **shown
//! once**, revoked. No script.
//!
//! **A session is needed, and a token is not enough**: a request that presents a bearer token has no session
//! here (`Identify::session` is `None` for it), so a token can neither read this page nor make another token.
//! A visitor with no session is sent to the sign-in page, which brings them back here. Every form carries the
//! session's token like any write, and it was checked in front of the handler (`crate::csrf`).
//!
//! Nothing here names a wiki, a role or any wiki data: the page is the account's own records, and an account
//! sees only its own sessions and tokens (the authenticator takes the account id from the session, never from
//! the form).

use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::extract::rejection::{FormRejection, QueryRejection};
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use inklkunn_auth::hashing::PASSWORD_MIN_CHARS;
use inklkunn_auth::token::{IssueError, MAX_LIVE_PER_ACCOUNT};
use inklkunn_auth::{AccountError, AuthError, Authenticator, ConfirmError};
use inklkunn_store::{AccountId, Scope, TokenId};

use crate::catalog::Key;
use crate::csrf::Decided;
use crate::pages::{Page, PageState, answer, render_form_refused};
use crate::services::Services;
use crate::signin::{Peer, client_of};
use crate::time::utc_minutes;
use crate::wiki::{blocking, generic_not_found, redirect, render_failed};

pub(crate) use crate::signin::names::{
    ACCOUNT_PATH as PATH, NAME_PATH, PASSWORD_PATH, SESSIONS_END_PATH, TOKENS_PATH,
    TOKENS_REVOKE_PATH,
};
use crate::signin::names::{
    DONE_NAME, DONE_PASSWORD, DONE_SESSION, DONE_TOKEN, F_CURRENT, F_DAYS, F_HANDLE, F_ID, F_LABEL,
    F_NAME, F_NEW, F_PASSWORD, F_SCOPE, Q_DONE, account_done, no_store,
};

/// The most bytes one of these forms may carry.
pub(crate) const FORM_LIMIT: usize = 8 * 1024;

/// The longest note a token carries, in characters.
const LABEL_MAX: usize = 80;

/// The lifetimes a token may be made with, in days, and the words for them.
const LIFETIMES: [(u32, Key); 5] = [
    (1, Key::AccountLifetimeDay),
    (7, Key::AccountLifetimeWeek),
    (30, Key::AccountLifetimeMonth),
    (90, Key::AccountLifetimeQuarter),
    (365, Key::AccountLifetimeYear),
];

/// How many ended tokens are listed beside the live ones.
const ENDED_SHOWN: usize = 10;

type Fields = Result<Form<HashMap<String, String>>, FormRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

struct SessionRow {
    line: String,
    current: Option<String>,
    handle: String,
}

struct TokenRow {
    label: String,
    scope: String,
    state: String,
    /// Set for a token that can still be revoked.
    revoke_id: Option<String>,
}

struct OptionRow {
    value: String,
    label: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "account.html")]
struct AccountTpl<'a> {
    page: &'a Page,
    title: String,
    heading: String,
    note: Option<String>,
    made: Option<String>,
    made_heading: String,
    made_body: String,
    made_use: String,
    name_heading: String,
    name_problem: Option<String>,
    name_action: &'static str,
    name_label: String,
    name: String,
    name_hint: String,
    name_button: String,
    password_heading: String,
    /// Whether the account has a password: the form that changes it, and the password a token asks for, are
    /// for such an account only.
    has_password: bool,
    password_intro: String,
    password_none: String,
    password_problem: Option<String>,
    password_action: &'static str,
    password_current_label: String,
    password_new_label: String,
    password_new_hint: String,
    password_button: String,
    sessions_heading: String,
    sessions_intro: String,
    sessions: Vec<SessionRow>,
    sessions_action: &'static str,
    session_end: String,
    tokens_heading: String,
    tokens_intro: String,
    tokens_none: String,
    tokens: Vec<TokenRow>,
    tokens_revoke_action: &'static str,
    token_revoke: String,
    token_new_heading: String,
    token_problem: Option<String>,
    tokens_action: &'static str,
    token_label: String,
    typed_label: String,
    token_label_hint: String,
    token_scope_label: String,
    scopes: Vec<OptionRow>,
    token_lifetime_label: String,
    lifetimes: Vec<OptionRow>,
    token_password_label: String,
    token_password_hint: String,
    token_provider_hint: String,
    token_button: String,
}

/// Who is acting: a session's account, and what is needed to read its records.
struct Acting {
    services: Arc<Services>,
    auth: Arc<Authenticator>,
    account: AccountId,
    secret: Option<String>,
    now: i64,
}

/// What differs between one drawing of the page and another.
#[derive(Default)]
struct Shown {
    status: Option<StatusCode>,
    note: Option<Key>,
    name_problem: Option<String>,
    password_problem: Option<String>,
    token_problem: Option<String>,
    /// A token just made, to show once.
    made: Option<String>,
    /// What was typed in the name field, when it was not saved.
    typed_name: Option<String>,
    typed_label: String,
    typed_scope: Option<Scope>,
    typed_days: Option<u32>,
}

fn scope_key(scope: Scope) -> Key {
    match scope {
        Scope::Read => Key::AccountScopeRead,
        Scope::Propose => Key::AccountScopePropose,
        Scope::Review => Key::AccountScopeReview,
        Scope::Accept => Key::AccountScopeAccept,
        Scope::Admin => Key::AccountScopeAdmin,
    }
}

/// Who is asking, or the answer to give: the sign-in page for a visitor with no session (which brings them
/// back), the generic not-found page where sign-in is not served.
fn acting(state: &PageState, page: &Page, headers: &HeaderMap) -> Result<Acting, Box<Response>> {
    let Some(services) = state.services.clone() else {
        return Err(Box::new(generic_not_found(page)));
    };
    let Some(auth) = services.auth.clone() else {
        return Err(Box::new(generic_not_found(page)));
    };
    let Some(who) = &page.who else {
        return Err(Box::new(redirect(&crate::signin::sign_in_href(PATH))));
    };
    Ok(Acting {
        now: (services.clock)(),
        account: who.account.clone(),
        secret: crate::identity::session_secret(headers),
        services,
        auth,
    })
}

/// Draw the account page.
async fn draw(page: Page, acting: Acting, shown: Shown) -> Response {
    let page = crate::signin::private(page);
    let for_work = (
        acting.auth.clone(),
        acting.account.clone(),
        acting.secret.clone(),
    );
    let now = acting.now;
    let loaded = blocking(move || {
        let sessions = for_work
            .0
            .sessions(&for_work.1, for_work.2.as_deref(), now)?;
        let tokens = for_work.0.tokens(&for_work.1)?;
        let has_password = for_work.0.has_password(&for_work.1)?;
        Ok::<_, AuthError>((sessions, tokens, has_password))
    })
    .await;
    let (sessions, tokens, has_password) = match loaded {
        Some(Ok(found)) => found,
        Some(Err(e)) => {
            (acting.services.log)(&format!("an account's records could not be read: {e}"));
            return render_failed(&page);
        }
        None => return render_failed(&page),
    };
    let t = page.t();
    let session_rows = sessions
        .iter()
        .map(|s| SessionRow {
            line: t
                .msg(Key::AccountSessionLine)
                .with("started", utc_minutes(s.created_at))
                .with("ends", utc_minutes(s.expires_at))
                .text(),
            current: s.current.then(|| page.get(Key::AccountSessionCurrent)),
            handle: s.handle.clone(),
        })
        .collect();
    let live = |expires: Option<i64>, revoked: Option<i64>| {
        revoked.is_none() && expires.is_none_or(|e| e > now)
    };
    let mut rows: Vec<TokenRow> = Vec::new();
    let (live_tokens, ended): (Vec<_>, Vec<_>) = tokens
        .iter()
        .rev()
        .partition(|k| live(k.expires_at, k.revoked_at));
    for token in live_tokens
        .into_iter()
        .chain(ended.into_iter().take(ENDED_SHOWN))
    {
        let state = match (token.revoked_at, token.expires_at) {
            (Some(at), _) => t
                .msg(Key::AccountTokenRevoked)
                .with("when", utc_minutes(at))
                .text(),
            (None, Some(at)) if at <= now => t
                .msg(Key::AccountTokenExpired)
                .with("when", utc_minutes(at))
                .text(),
            (None, Some(at)) => t
                .msg(Key::AccountTokenLive)
                .with("when", utc_minutes(at))
                .text(),
            (None, None) => String::new(),
        };
        rows.push(TokenRow {
            label: if token.label.is_empty() {
                page.get(Key::AccountTokenUnlabelled)
            } else {
                token.label.clone()
            },
            scope: t
                .msg(Key::AccountTokenScope)
                .with("scope", page.get(scope_key(token.scope)))
                .text(),
            state,
            revoke_id: live(token.expires_at, token.revoked_at).then(|| token.id.to_string()),
        });
    }
    let name = shown.typed_name.clone().unwrap_or_else(|| {
        match acting.services.accounts.account(&acting.account) {
            Ok(Some(account)) => account.display_name.as_str().to_owned(),
            _ => String::new(),
        }
    });
    let chosen_scope = shown.typed_scope.unwrap_or(Scope::Propose);
    let chosen_days = shown.typed_days.unwrap_or(30);
    let rendered = AccountTpl {
        page: &page,
        title: page.title(Key::TitleAccount),
        heading: page.get(Key::AccountHeading),
        note: shown.note.map(|k| page.get(k)),
        made: shown.made.clone(),
        made_heading: page.get(Key::AccountTokenMadeHeading),
        made_body: page.get(Key::AccountTokenMadeBody),
        made_use: page.get(Key::AccountTokenMadeUse),
        name_heading: page.get(Key::AccountNameHeading),
        name_problem: shown.name_problem.clone(),
        name_action: NAME_PATH,
        name_label: page.get(Key::AccountNameLabel),
        name,
        name_hint: page.get(Key::AccountNameHint),
        name_button: page.get(Key::AccountNameButton),
        password_heading: page.get(Key::AccountPasswordHeading),
        has_password,
        password_intro: page.get(Key::AccountPasswordIntro),
        password_none: page.get(Key::AccountPasswordNone),
        password_problem: shown.password_problem.clone(),
        password_action: PASSWORD_PATH,
        password_current_label: page.get(Key::AccountPasswordCurrentLabel),
        password_new_label: page.get(Key::AccountPasswordNewLabel),
        password_new_hint: page
            .t()
            .msg(Key::AccountFormPasswordHint)
            .with("min", PASSWORD_MIN_CHARS.to_string())
            .text(),
        password_button: page.get(Key::AccountPasswordButton),
        sessions_heading: page.get(Key::AccountSessionsHeading),
        sessions_intro: page.get(Key::AccountSessionsIntro),
        sessions: session_rows,
        sessions_action: SESSIONS_END_PATH,
        session_end: page.get(Key::AccountSessionEnd),
        tokens_heading: page.get(Key::AccountTokensHeading),
        tokens_intro: page.get(Key::AccountTokensIntro),
        tokens_none: page.get(Key::AccountTokensNone),
        tokens: rows,
        tokens_revoke_action: TOKENS_REVOKE_PATH,
        token_revoke: page.get(Key::AccountTokenRevoke),
        token_new_heading: page.get(Key::AccountTokenNewHeading),
        token_problem: shown.token_problem.clone(),
        tokens_action: TOKENS_PATH,
        token_label: page.get(Key::AccountTokenLabel),
        typed_label: shown.typed_label.clone(),
        token_label_hint: page.get(Key::AccountTokenLabelHint),
        token_scope_label: page.get(Key::AccountTokenScopeLabel),
        scopes: Scope::ALL
            .iter()
            .map(|s| OptionRow {
                value: s.as_str().to_owned(),
                label: page.get(scope_key(*s)),
                selected: *s == chosen_scope,
            })
            .collect(),
        token_lifetime_label: page.get(Key::AccountTokenLifetimeLabel),
        lifetimes: LIFETIMES
            .iter()
            .map(|(days, key)| OptionRow {
                value: days.to_string(),
                label: page.get(*key),
                selected: *days == chosen_days,
            })
            .collect(),
        token_password_label: page.get(Key::AccountTokenPasswordLabel),
        token_password_hint: page.get(Key::AccountTokenPasswordHint),
        token_provider_hint: page.get(Key::AccountTokenProviderHint),
        token_button: page.get(Key::AccountTokenButton),
    }
    .render();
    no_store(answer(
        shown.status.unwrap_or(StatusCode::OK),
        &page,
        rendered,
    ))
}

/// `GET /-/account`.
pub(crate) async fn account_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    params: Params,
) -> Response {
    let acting = match acting(&state, &page, &headers) {
        Ok(a) => a,
        Err(response) => return *response,
    };
    let note = params
        .ok()
        .and_then(|Query(q)| q.get(Q_DONE).cloned())
        .and_then(|d| match d.as_str() {
            DONE_NAME => Some(Key::AccountNameSaved),
            DONE_SESSION => Some(Key::AccountSessionEnded),
            DONE_TOKEN => Some(Key::AccountTokenRevokedNote),
            DONE_PASSWORD => Some(Key::AccountPasswordChanged),
            _ => None,
        });
    draw(
        page,
        acting,
        Shown {
            note,
            ..Shown::default()
        },
    )
    .await
}

/// Every write of this page begins the same way: from this site, a session, the form.
fn begin(
    state: &PageState,
    page: &Page,
    headers: &HeaderMap,
    decided: &Decided,
    form: Fields,
) -> Result<(Acting, HashMap<String, String>), Box<Response>> {
    if !crate::lang::from_this_site(headers) {
        let page = page.clone().without_switcher();
        return Err(Box::new(answer(
            StatusCode::FORBIDDEN,
            &page,
            render_form_refused(&page),
        )));
    }
    let acting = acting(state, page, headers)?;
    // The CSRF layer decided who this request is; the page's session view must be the same account, or this is
    // not a request to act on (a token is not a session, and the two cannot disagree).
    if decided.principal(&acting.services, headers).account_id() != Some(&acting.account) {
        return Err(Box::new(crate::csrf::expired(headers)));
    }
    let Ok(Form(fields)) = form else {
        return Err(Box::new(crate::csrf::expired(headers)));
    };
    Ok((acting, fields))
}

fn field<'a>(fields: &'a HashMap<String, String>, name: &str) -> &'a str {
    fields.get(name).map_or("", String::as_str)
}

fn account_problem(page: &Page, error: &AccountError) -> Option<String> {
    use inklkunn_store::DisplayNameError;
    let key = match error {
        AccountError::DisplayName(DisplayNameError::Length) => Key::AccountFormNameLength,
        AccountError::DisplayName(DisplayNameError::Control) => Key::AccountFormNameControl,
        AccountError::DisplayName(_) => Key::AccountFormNameSpace,
        _ => return None,
    };
    Some(page.get(key))
}

/// `POST /-/account/name`.
pub(crate) async fn save_name(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, &decided, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    let typed = field(&fields, F_NAME).trim().to_owned();
    let for_work = (acting.auth.clone(), acting.account.clone(), typed.clone());
    let done = blocking(move || for_work.0.set_display_name(&for_work.1, &for_work.2)).await;
    match done {
        None => render_failed(&page),
        Some(Ok(())) => redirect(&account_done(DONE_NAME)),
        Some(Err(error)) => {
            let problem = account_problem(&page, &error).unwrap_or_else(|| {
                (acting.services.log)(&format!("a name could not be saved: {error}"));
                page.get(Key::ProblemUnavailable)
            });
            draw(
                page,
                acting,
                Shown {
                    status: Some(StatusCode::UNPROCESSABLE_ENTITY),
                    name_problem: Some(problem),
                    typed_name: Some(typed),
                    ..Shown::default()
                },
            )
            .await
        }
    }
}

/// `POST /-/account/sessions/end`: end one of the account's own sessions. Ending the one in use signs this
/// browser out.
pub(crate) async fn end_session(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, &decided, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    let handle = field(&fields, F_HANDLE).to_owned();
    let for_work = (
        acting.auth.clone(),
        acting.account.clone(),
        acting.secret.clone(),
        acting.now,
    );
    let done = blocking(move || {
        let (auth, account, secret, now) = for_work;
        // Whether the one named is the one in use, before it is gone.
        let current = auth
            .sessions(&account, secret.as_deref(), now)?
            .iter()
            .any(|s| s.handle == handle && s.current);
        let ended = auth.end_session_of(&account, &handle, now)?;
        Ok::<_, AuthError>((ended, current))
    })
    .await;
    match done {
        None => render_failed(&page),
        Some(Err(e)) => {
            (acting.services.log)(&format!("a session could not be ended: {e}"));
            render_failed(&page)
        }
        Some(Ok((true, true))) => {
            let mut response = redirect("/");
            if let Ok(value) = HeaderValue::from_str(&inklkunn_auth::session::clear_cookie()) {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
            response
        }
        Some(Ok((true, false))) => redirect(&account_done(DONE_SESSION)),
        // Not one of theirs, or already over: nothing to do, and the page shows what is true now.
        Some(Ok((false, _))) => redirect(PATH),
    }
}

/// `POST /-/account/tokens`: make a token, and show it once. **The password is asked again** (or, for an account
/// with none, a sign-in of the last ten minutes is needed): a token outlives the session, so a stolen session
/// alone must not be able to make one (corrections G6).
pub(crate) async fn make_token(
    State(state): State<PageState>,
    page: Page,
    peer: Peer,
    headers: HeaderMap,
    decided: Decided,
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, &decided, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    let label = field(&fields, F_LABEL).trim().to_owned();
    let scope = Scope::parse(field(&fields, F_SCOPE));
    let days = field(&fields, F_DAYS)
        .parse::<u32>()
        .ok()
        .filter(|d| LIFETIMES.iter().any(|(allowed, _)| allowed == d));
    let again = |problem: Key| Shown {
        status: Some(StatusCode::UNPROCESSABLE_ENTITY),
        token_problem: Some(page.get(problem)),
        typed_label: label.clone(),
        typed_scope: scope,
        typed_days: days,
        ..Shown::default()
    };
    if label.chars().count() > LABEL_MAX {
        let shown = again(Key::AccountTokenLabelLong);
        return draw(page, acting, shown).await;
    }
    let (Some(scope_wanted), Some(days_wanted)) = (scope, days) else {
        let shown = again(Key::ProblemUnavailable);
        return draw(page, acting, shown).await;
    };
    // Only now, with everything else in order, is the password judged (a form that was never going to be
    // accepted does not spend a guess).
    let client = client_of(&acting.services, &acting.auth, &peer, &headers);
    let for_work = (
        acting.auth.clone(),
        acting.account.clone(),
        acting.secret.clone().unwrap_or_default(),
        field(&fields, F_PASSWORD).to_owned(),
    );
    let (now, asked) = (acting.now, client.clone());
    let confirmed = blocking(move || {
        for_work
            .0
            .confirm_for_sensitive_act(&for_work.1, &for_work.2, &for_work.3, &asked, now)
    })
    .await;
    let refuse = |status: StatusCode, text: String| Shown {
        status: Some(status),
        token_problem: Some(text),
        typed_label: label.clone(),
        typed_scope: scope,
        typed_days: days,
        ..Shown::default()
    };
    let refused = match confirmed {
        None => return render_failed(&page),
        Some(Ok(())) => None,
        Some(Err(ConfirmError::SignInAgain)) => Some((
            refuse(
                StatusCode::FORBIDDEN,
                page.get(Key::AccountTokenSignInAgain),
            ),
            None,
        )),
        Some(Err(ConfirmError::Auth(AuthError::BadCredentials))) => Some((
            refuse(
                StatusCode::UNAUTHORIZED,
                page.get(Key::AccountTokenPasswordWrong),
            ),
            None,
        )),
        Some(Err(ConfirmError::Auth(AuthError::Locked { until }))) => Some((
            refuse(StatusCode::TOO_MANY_REQUESTS, page.get(Key::SignInLocked)),
            Some(until),
        )),
        Some(Err(ConfirmError::Auth(AuthError::Busy))) => Some((
            refuse(StatusCode::SERVICE_UNAVAILABLE, page.get(Key::SignInBusy)),
            Some(now + 5),
        )),
        Some(Err(other)) => {
            (acting.services.log)(&format!("a token could not be confirmed: {other}"));
            Some((
                refuse(
                    StatusCode::SERVICE_UNAVAILABLE,
                    page.get(Key::ProblemUnavailable),
                ),
                None,
            ))
        }
    };
    if let Some((shown, retry)) = refused {
        let response = draw(page, acting, shown).await;
        return match retry {
            Some(until) => crate::signin::retry_after(response, until, now),
            None => response,
        };
    }
    let for_work = (
        acting.auth.clone(),
        acting.account.clone(),
        label.clone(),
        acting.now,
    );
    let made = blocking(move || {
        for_work.0.issue_token(
            &for_work.1,
            scope_wanted,
            &for_work.2,
            i64::from(days_wanted) * 86_400,
            for_work.3,
        )
    })
    .await;
    match made {
        None => render_failed(&page),
        Some(Ok(issued)) => {
            draw(
                page,
                acting,
                Shown {
                    made: Some(issued.secret.as_str().to_owned()),
                    ..Shown::default()
                },
            )
            .await
        }
        Some(Err(IssueError::TooMany)) => {
            let mut shown = again(Key::AccountTokenTooMany);
            shown.token_problem = Some(
                page.t()
                    .msg(Key::AccountTokenTooMany)
                    .with("max", MAX_LIVE_PER_ACCOUNT.to_string())
                    .text(),
            );
            draw(page, acting, shown).await
        }
        Some(Err(error)) => {
            (acting.services.log)(&format!("a token could not be made: {error}"));
            let shown = again(Key::ProblemUnavailable);
            draw(page, acting, shown).await
        }
    }
}

/// `POST /-/account/password`: change one's own password (corrections G10). The current password is asked and
/// judged first, throttled and constant-work like a sign-in; every other session of the account ends and every
/// API token is revoked, and this browser stays signed in.
pub(crate) async fn change_password(
    State(state): State<PageState>,
    page: Page,
    peer: Peer,
    headers: HeaderMap,
    decided: Decided,
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, &decided, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    let client = client_of(&acting.services, &acting.auth, &peer, &headers);
    let for_work = (
        acting.auth.clone(),
        acting.account.clone(),
        acting.secret.clone().unwrap_or_default(),
        field(&fields, F_CURRENT).to_owned(),
        field(&fields, F_NEW).to_owned(),
    );
    let now = acting.now;
    let changed = blocking(move || {
        for_work.0.change_own_password(
            &for_work.1,
            &for_work.2,
            &for_work.3,
            &for_work.4,
            &client,
            now,
        )
    })
    .await;
    let problem = |status: StatusCode, text: String| Shown {
        status: Some(status),
        password_problem: Some(text),
        ..Shown::default()
    };
    let (shown, retry) = match changed {
        None => return render_failed(&page),
        Some(Ok(())) => return redirect(&account_done(DONE_PASSWORD)),
        Some(Err(AccountError::Auth(AuthError::BadCredentials))) => (
            problem(
                StatusCode::UNAUTHORIZED,
                page.get(Key::AccountPasswordWrong),
            ),
            None,
        ),
        Some(Err(AccountError::Auth(AuthError::Locked { until }))) => (
            problem(StatusCode::TOO_MANY_REQUESTS, page.get(Key::SignInLocked)),
            Some(until),
        ),
        Some(Err(AccountError::Auth(AuthError::Busy))) => (
            problem(StatusCode::SERVICE_UNAVAILABLE, page.get(Key::SignInBusy)),
            Some(now + 5),
        ),
        Some(Err(AccountError::Password(refusal))) => {
            let key = match refusal {
                inklkunn_auth::PasswordRefusal::TooShort => None,
                _ => Some(Key::AccountFormPasswordLong),
            };
            let text = match key {
                Some(key) => page.get(key),
                None => page
                    .t()
                    .msg(Key::AccountFormPasswordShort)
                    .with("min", PASSWORD_MIN_CHARS.to_string())
                    .text(),
            };
            (problem(StatusCode::UNPROCESSABLE_ENTITY, text), None)
        }
        Some(Err(other)) => {
            (acting.services.log)(&format!("a password could not be changed: {other}"));
            (
                problem(
                    StatusCode::SERVICE_UNAVAILABLE,
                    page.get(Key::ProblemUnavailable),
                ),
                None,
            )
        }
    };
    let response = draw(page, acting, shown).await;
    match retry {
        Some(until) => crate::signin::retry_after(response, until, now),
        None => response,
    }
}

/// `POST /-/account/tokens/revoke`.
pub(crate) async fn revoke_token(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, &decided, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    let Some(id) = TokenId::parse(field(&fields, F_ID)) else {
        return redirect(PATH);
    };
    let for_work = (acting.auth.clone(), acting.account.clone(), acting.now);
    let done = blocking(move || for_work.0.revoke_token(&for_work.1, &id, for_work.2)).await;
    match done {
        None => render_failed(&page),
        Some(Err(e)) => {
            (acting.services.log)(&format!("a token could not be revoked: {e}"));
            render_failed(&page)
        }
        Some(Ok(true)) => redirect(&account_done(DONE_TOKEN)),
        Some(Ok(false)) => redirect(PATH),
    }
}
