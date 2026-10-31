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
use inklkunn_auth::token::{IssueError, MAX_LIVE_PER_ACCOUNT};
use inklkunn_auth::{AccountError, AuthError, Authenticator};
use inklkunn_store::{AccountId, Scope, TokenId};

use crate::catalog::Key;
use crate::pages::{Page, PageState, answer, render_form_refused};
use crate::services::Services;
use crate::time::utc_minutes;
use crate::wiki::{blocking, generic_not_found, redirect, render_failed};

pub(crate) use crate::signin::names::{
    ACCOUNT_PATH as PATH, NAME_PATH, SESSIONS_END_PATH, TOKENS_PATH, TOKENS_REVOKE_PATH,
};
use crate::signin::names::{
    DONE_NAME, DONE_SESSION, DONE_TOKEN, F_DAYS, F_HANDLE, F_ID, F_LABEL, F_NAME, F_SCOPE, Q_DONE,
    account_done, no_store,
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
        Ok::<_, AuthError>((sessions, tokens))
    })
    .await;
    let (sessions, tokens) = match loaded {
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
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, form) {
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
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, form) {
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

/// `POST /-/account/tokens`: make a token, and show it once.
pub(crate) async fn make_token(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, form) {
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

/// `POST /-/account/tokens/revoke`.
pub(crate) async fn revoke_token(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    form: Fields,
) -> Response {
    let (acting, fields) = match begin(&state, &page, &headers, form) {
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
