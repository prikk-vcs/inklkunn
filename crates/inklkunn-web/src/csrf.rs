//! **One mechanism for every write a signed-in person can make** (handoff 004 P6; RFC 002 D-1; the sign-in
//! handoff's U1 and U4). A session cookie is sent by the browser on its own, so a request that carries one and
//! changes something is believed only if it also presents the session's CSRF synchronizer token: in the form's
//! hidden field `csrf`, or in the header `X-CSRF-Token` for a client that is not a form. The check is this
//! layer, in front of every route of the application origin, so it is **built once and cannot be forgotten by a
//! handler**, and it runs **before the wiki is looked up** (before any handler runs at all): a refusal is the
//! same page for a wiki that is there and one that is not.
//!
//! * The token and the cookie are checked **together** by `Identify::for_write` (the authenticator's
//!   `principal_for_write`); a token of another session, an empty one and a missing one are all the same
//!   refusal (403, *the form expired*).
//! * The same-site check (`crate::lang::from_this_site`) stays in every handler as the second wall.
//! * **A request that presents a bearer token is judged by the token alone**: it needs no CSRF token (a browser
//!   does not send one by itself) and a cookie it also carries is not looked at.
//! * A request with no live session has nothing to forge: it passes, and the handler says what an anonymous
//!   visitor may do.
//! * The pages that make a session (sign-in, joining, an invitation) are protected by a double-submit token in
//!   a `__Host-` cookie instead (they have no session yet); the language form changes a cookie of the visitor's
//!   own and nothing else. Those are the only exceptions, listed here.
//!
//! The body is read here only for a request that carries a **live** session (so a stranger's flood costs a
//! lookup, not a buffer), to the largest bound any form has.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::catalog::T;
use crate::pages::{PageState, answer, render_form_expired};
use crate::services::{Services, WriteIdentity};
use crate::signin::names::{FORM_MEDIA_TYPE, INVITE_PREFIX, JOIN_PATH, SIGN_IN_PATH};

pub use crate::signin::names::{CSRF_FIELD as FIELD, CSRF_HEADER as HEADER};

/// The POSTs that need no session token, each for its reason: the language form sets the visitor's own language
/// cookie and nothing else; the three below make a session, and have no session to forge a write with (they are
/// held by the double-submit token of `signin`).
const EXEMPT: &[&str] = &[crate::pages::LANGUAGE_PATH, SIGN_IN_PATH, JOIN_PATH];
/// The invitation page's prefix (its address carries the one-time secret).
const EXEMPT_PREFIX: &str = INVITE_PREFIX;

/// What an address of the API has in it: `/{slug}/-/api/v1/…`.
const API_SEGMENT: &str = "/-/api/";

/// The most a body is read to: the largest form there is.
const BODY_LIMIT: usize = crate::proposals::PROPOSAL_FORM_LIMIT;

/// Does a request to `path` with `method` change something that a session's CSRF token must guard?
fn needs_token(method: &Method, path: &str) -> bool {
    let safe = matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    );
    !safe && !EXEMPT.contains(&path) && !path.starts_with(EXEMPT_PREFIX)
}

/// The value of `name` in an `application/x-www-form-urlencoded` body: the first one.
fn form_field(body: &[u8], name: &str) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    text.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (form_decode(key)? == name).then(|| form_decode(value))?
    })
}

/// `+` is a space and `%XX` is a byte.
fn form_decode(raw: &str) -> Option<String> {
    crate::url::decode(&raw.replace('+', " "))
}

fn is_form(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|m| m.trim().eq_ignore_ascii_case(FORM_MEDIA_TYPE))
        })
}

/// The token a request presents: the header, else the form's field. A body that could not be read within the
/// bound presented none (the handler would refuse it as too large anyway).
async fn presented(req: Request) -> (Request, Option<String>) {
    let (parts, body) = req.into_parts();
    if let Some(value) = parts
        .headers
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
    {
        return (Request::from_parts(parts, body), Some(value));
    }
    if !is_form(&parts.headers) {
        return (Request::from_parts(parts, body), None);
    }
    match axum::body::to_bytes(body, BODY_LIMIT).await {
        Ok(bytes) => {
            let token = form_field(&bytes, FIELD);
            (Request::from_parts(parts, Body::from(bytes)), token)
        }
        // Too large, or broken: it presented no token we could read, and the handler would refuse it anyway.
        Err(_) => (Request::from_parts(parts, Body::empty()), None),
    }
}

/// The layer.
pub(crate) async fn layer(State(state): State<PageState>, req: Request, next: Next) -> Response {
    let Some(services) = state.services.clone() else {
        return next.run(req).await;
    };
    if !needs_token(req.method(), req.uri().path())
        || !services.identity.may_be_signed_in(req.headers())
    {
        return next.run(req).await;
    }
    // Only a live session has anything to forge; look before reading a body.
    let path = req.uri().path().to_owned();
    let (probe, headers) = (services.clone(), req.headers().clone());
    let live = tokio::task::spawn_blocking(move || probe.identity.session(&headers))
        .await
        .map_err(|_| ())
        .map(|session| session.is_some());
    match live {
        Ok(false) => return next.run(req).await,
        Ok(true) => {}
        // The check could not run: closed.
        Err(()) => return expired_for(req.headers(), &path),
    }
    let (req, token) = presented(req).await;
    let decision = check(&services, req.headers().clone(), token).await;
    match decision {
        Some(WriteIdentity::Principal(_) | WriteIdentity::InvalidToken) => next.run(req).await,
        Some(WriteIdentity::CsrfMismatch) | None => expired_for(req.headers(), &path),
    }
}

/// Ask the identity, on a blocking thread, who the request is with the token it presented.
async fn check(
    services: &std::sync::Arc<Services>,
    headers: HeaderMap,
    token: Option<String>,
) -> Option<WriteIdentity> {
    let services = services.clone();
    tokio::task::spawn_blocking(move || services.identity.for_write(&headers, token.as_deref()))
        .await
        .ok()
}

/// The refusal of a request to `path`: for the API a problem detail (`forbidden`, as its cookie-only writes from
/// another site are), else the page.
fn expired_for(headers: &HeaderMap, path: &str) -> Response {
    if path.contains(API_SEGMENT) {
        return crate::api::reply::finish(
            T::new(crate::lang::negotiate_from_headers(headers)),
            None,
            Err(crate::api::problem::Problem::new(
                crate::api::problem::Code::Forbidden,
            )),
        );
    }
    expired(headers)
}

/// The refusal: 403, the same page for every wiki and for none, in the visitor's language.
pub(crate) fn expired(headers: &HeaderMap) -> Response {
    let page = crate::pages::Page::new(T::new(crate::lang::negotiate_from_headers(headers)), "/")
        .without_switcher();
    answer(StatusCode::FORBIDDEN, &page, render_form_expired(&page))
}

#[cfg(test)]
mod tests;
