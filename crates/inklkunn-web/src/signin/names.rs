//! The names the sign-in pages, the account page and the CSRF layer agree on: the addresses, the form fields,
//! the query value that says what was just done, and the header and media-type names. Code a form and a handler
//! share; no sentence a reader sees.

use axum::http::{HeaderValue, header};
use axum::response::Response;
use inklkunn_auth::session::CSRF_COOKIE;

/// Where a person signs in.
pub(crate) const SIGN_IN_PATH: &str = "/-/sign-in";
/// Where a person signs out.
pub(crate) const SIGN_OUT_PATH: &str = "/-/sign-out";
/// Where a person makes an account where joining is open.
pub(crate) const JOIN_PATH: &str = "/-/join";
/// What an invitation link starts with; the secret follows.
pub(crate) const INVITE_PREFIX: &str = "/-/invite/";
/// The route pattern of an invitation link.
pub(crate) const INVITE_ROUTE: &str = "/-/invite/{secret}";

/// The account page.
pub(crate) const ACCOUNT_PATH: &str = "/-/account";
/// Where the display name is saved.
pub(crate) const NAME_PATH: &str = "/-/account/name";
/// Where a session is ended.
pub(crate) const SESSIONS_END_PATH: &str = "/-/account/sessions/end";
/// Where a token is made.
pub(crate) const TOKENS_PATH: &str = "/-/account/tokens";
/// Where a token is revoked.
pub(crate) const TOKENS_REVOKE_PATH: &str = "/-/account/tokens/revoke";

/// The hidden field every form of a signed-in person carries.
pub const CSRF_FIELD: &str = "csrf";
/// The header a client that is not a form sends the token in.
pub const CSRF_HEADER: &str = "x-csrf-token";
/// The media type of a form.
pub(crate) const FORM_MEDIA_TYPE: &str = "application/x-www-form-urlencoded";
/// What an address of the API has in it: `/{slug}/-/api/v1/…`.
pub(crate) const API_SEGMENT: &str = "/-/api/";
/// The `Authorization` scheme of an API token.
pub(crate) const BEARER: &str = "bearer";
/// The header a trusted proxy reports the client address in.
pub(crate) const FORWARDED_FOR: &str = "x-forwarded-for";

/// The form fields.
pub(crate) const F_LOGIN: &str = "login";
pub(crate) const F_PASSWORD: &str = "password";
pub(crate) const F_NAME: &str = "name";
pub(crate) const F_NEXT: &str = "next";
pub(crate) const F_HANDLE: &str = "handle";
pub(crate) const F_LABEL: &str = "label";
pub(crate) const F_SCOPE: &str = "scope";
pub(crate) const F_DAYS: &str = "days";
pub(crate) const F_ID: &str = "id";

/// The query parameter of the account page that says what was just done, and its values.
pub(crate) const Q_DONE: &str = "done";
pub(crate) const DONE_NAME: &str = "name";
pub(crate) const DONE_SESSION: &str = "session";
pub(crate) const DONE_TOKEN: &str = "token";

/// The address *Sign in* leads to from the page `back` (a local address): the sign-in page, which returns
/// there.
pub(crate) fn sign_in_href(back: &str) -> String {
    if back == "/" || back.is_empty() {
        SIGN_IN_PATH.to_owned()
    } else {
        format!(
            "{SIGN_IN_PATH}?{F_NEXT}={}",
            inklkunn_markup::percent_encode(back)
        )
    }
}

/// The address of the invitation whose secret is `secret`.
pub(crate) fn invite_action(secret: &str) -> String {
    format!("{INVITE_PREFIX}{secret}")
}

/// The account page after `what` was done.
pub(crate) fn account_done(what: &str) -> String {
    format!("{ACCOUNT_PATH}?{Q_DONE}={what}")
}

/// The `Set-Cookie` that removes the double-submit cookie.
pub(crate) fn clear_form_cookie() -> String {
    format!("{CSRF_COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0")
}

/// A page that is not kept: it holds a one-time secret, a token or a person's own records.
pub(crate) fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
