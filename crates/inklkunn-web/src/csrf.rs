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
//!   visitor may do. **Only a request that is signed out passes unchecked**: the layer asks
//!   `Identify::for_write`, which fails closed (a session it could not read is a refusal, never "signed out"),
//!   and never a separate "is there a session" question whose failure would read as "no".
//! * **The layer decides once and says what it decided** (corrections G4): the identity it settled on goes into
//!   the request's extensions ([`Decided`]), and a write handler takes its principal from there. No handler
//!   resolves the credential a second time, so nothing can come out differently the second time.
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

use inklkunn_core::Principal;

use crate::catalog::T;
use crate::pages::{PageState, answer, render_form_expired};
use crate::services::{Services, WriteIdentity};
use crate::signin::names::{API_SEGMENT, FORM_MEDIA_TYPE, INVITE_PREFIX, JOIN_PATH, SIGN_IN_PATH};

pub use crate::signin::names::{CSRF_FIELD as FIELD, CSRF_HEADER as HEADER};

/// The POSTs that need no session token, each for its reason: the language form sets the visitor's own language
/// cookie and nothing else; the three below make a session, and have no session to forge a write with (they are
/// held by the double-submit token of `signin`).
const EXEMPT: &[&str] = &[crate::pages::LANGUAGE_PATH, SIGN_IN_PATH, JOIN_PATH];
/// The invitation page's prefix (its address carries the one-time secret).
const EXEMPT_PREFIX: &str = INVITE_PREFIX;

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

/// What the layer decided about a request that changes something, kept in the request's extensions for the
/// handler (corrections G4). A request the layer let through carries it; a handler that takes its principal
/// from here asks the credential nothing more. (A request the layer did not look at, a read, has none, and the
/// handler resolves the principal itself, once.)
#[derive(Debug, Clone)]
pub(crate) struct Decided(Option<WriteIdentity>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Decided {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Decided, Self::Rejection> {
        Ok(Decided(
            parts.extensions.get::<Settled>().map(|s| s.0.clone()),
        ))
    }
}

/// The extension the layer sets.
#[derive(Debug, Clone)]
struct Settled(WriteIdentity);

impl Decided {
    /// Who the request is, as the layer decided; else (it decided nothing) the identity is asked, once. A
    /// request that presented a bad token or a session without its token is nobody.
    pub(crate) fn principal(&self, services: &Services, headers: &HeaderMap) -> Principal {
        match &self.0 {
            Some(WriteIdentity::Principal(p)) => p.clone(),
            Some(WriteIdentity::InvalidToken | WriteIdentity::CsrfMismatch) => Principal::Anonymous,
            None => services.identity.principal(headers),
        }
    }

    /// The identity of a write as the layer decided it, else (it decided nothing) as the identity says now.
    pub(crate) fn write_identity(
        &self,
        services: &Services,
        headers: &HeaderMap,
        csrf: Option<&str>,
    ) -> WriteIdentity {
        match &self.0 {
            Some(decided) => decided.clone(),
            None => services.identity.for_write(headers, csrf),
        }
    }
}

/// The layer.
pub(crate) async fn layer(State(state): State<PageState>, req: Request, next: Next) -> Response {
    let Some(services) = state.services.clone() else {
        return next.run(req).await;
    };
    if !needs_token(req.method(), req.uri().path()) {
        return next.run(req).await;
    }
    let path = req.uri().path().to_owned();
    // First without a token: a request with no session cookie, or with a bearer token, is decided by this alone,
    // and nothing is buffered for a stranger. A request that is signed out passes; only that does.
    let first = check(&services, req.headers().clone(), None).await;
    let (req, decision) = match first {
        Some(decided @ (WriteIdentity::Principal(_) | WriteIdentity::InvalidToken)) => {
            (req, decided)
        }
        // A live session that has not shown its token, or a check that could not run (closed): the token is
        // read now (this request carries a session cookie, so it is worth the buffer) and decides.
        Some(WriteIdentity::CsrfMismatch) => {
            let (req, token) = presented(req).await;
            let decided = check(&services, req.headers().clone(), token).await;
            match decided {
                Some(decided @ (WriteIdentity::Principal(_) | WriteIdentity::InvalidToken)) => {
                    (req, decided)
                }
                Some(WriteIdentity::CsrfMismatch) | None => {
                    return expired_for(req.headers(), &path);
                }
            }
        }
        None => return expired_for(req.headers(), &path),
    };
    let mut req = req;
    req.extensions_mut().insert(Settled(decision));
    next.run(req).await
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
