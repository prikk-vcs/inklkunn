//! The shared start and finish of a write (RFC 004 D-13; handoff 004 T7; API-03 … API-05), the way
//! [`super::request::serve`] is for a read.
//!
//! **The order is the contract.** Each step may stop the request, and none depends on a fact a caller must not
//! learn:
//!
//! 1. *the credential, as a request*: an `Authorization` header that is not `Bearer <token>`, or whose token
//!    does not authenticate, is `invalid-token` (401); a request with a session cookie and no token must come
//!    from this site (`forbidden`). Neither looks at a wiki, so neither tells a wiki from another.
//! 2. *the wiki*: absent and not-readable are the same `not-found`, byte for byte.
//! 3. *an account*: a request with no credential at all is `authentication-required` (401) — after the wiki, so
//!    a restricted wiki is never told from an absent one by a sign-in prompt.
//! 4. *the action*: the endpoint's `Action`, asked of the authorizer for this principal — **where a token's
//!    scope and the account's role are intersected (AZ-3)** — before a byte of the body is read. A token that
//!    carries `propose` is `forbidden` at `accept` whatever its account is.
//! 5. *the base is named* (`If-Match`; 428 when it is not) and *the body is bounded*: `Content-Type` is JSON, a
//!    declared length over the endpoint's bound is refused unread, and a body is read to at most the bound.
//! 6. only then is the body parsed, strictly, and the service called.
//!
//! No cookie is needed and none is set. A bearer token is not sent by a browser by itself, so a request that
//! carries one is not a cross-site forgery; one that carries only a session cookie is held to the pages' own
//! same-site check, and to a JSON body, which a form cannot send.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Uri, header};
use axum::response::Response;
use inklkunn_core::{Principal, Resource};
use inklkunn_store::{AccountId, ProposalId, WikiRecord};

use super::describe::Doc;
use super::problem::{Code, Problem};
use super::reply::{self, Reply};
use super::request::{language, parse_path};
use crate::catalog::T;
use crate::pages::PageState;
use crate::services::Services;
use crate::url::decode;
use crate::wiki::{blocking, open};

pub(crate) mod base;
pub(crate) mod body;
pub(crate) mod refuse;

use body::Fields;

/// What a write's own work is given: who asks, of which wiki, the proposal its address names, and the base it
/// named in `If-Match` (still to be compared with the current state).
#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) services: Arc<Services>,
    pub(crate) wiki: WikiRecord,
    pub(crate) principal: Principal,
    pub(crate) t: T,
    /// The proposal the address names, for the endpoints that have one.
    pub(crate) id: Option<ProposalId>,
    /// The tag inside the quotes of `If-Match`.
    pub(crate) if_match: String,
}

impl Request {
    /// The account behind the request. A write is always an account's: the service refuses the rest.
    pub(crate) fn account(&self) -> Option<&AccountId> {
        self.principal.account_id()
    }

    /// The operator's log.
    pub(crate) fn log(&self, line: &str) {
        (self.services.log)(&format!("wiki `{}`: {line}", self.wiki.slug));
    }

    /// The address of the proposal in the API.
    pub(crate) fn proposal_path(&self, id: &ProposalId) -> String {
        format!("/{}/-/api/v1/proposals/{id}", self.wiki.slug)
    }
}

/// Run a write: the shared start, `work` on a blocking thread, the shared finish.
pub(crate) async fn serve_write<F>(
    state: PageState,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
    doc: &'static Doc,
    work: F,
) -> Response
where
    F: FnOnce(&Request, Fields) -> Result<Reply, Problem> + Send + 'static,
{
    let t = language(&headers);
    let outcome = run(state, headers, uri, body, doc, t, work).await;
    // A write is never a `304`: it has no `If-None-Match`.
    reply::finish(t, None, outcome)
}

async fn run<F>(
    state: PageState,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
    doc: &'static Doc,
    t: T,
    work: F,
) -> Result<Reply, Problem>
where
    F: FnOnce(&Request, Fields) -> Result<Reply, Problem> + Send + 'static,
{
    let not_found = || Problem::new(Code::NotFound);
    let write = doc.write.ok_or_else(|| Problem::new(Code::Failed))?;
    let services = state.services.ok_or_else(not_found)?;
    let (raw_slug, _word, raw_rest) = parse_path(uri.path()).ok_or_else(not_found)?;
    let slug = decode(raw_slug).ok_or_else(not_found)?;
    let raw_id = raw_rest.split('/').next().unwrap_or("").to_owned();
    let has_id = doc.path.contains("{id}");

    let head = headers.clone();
    let admitted = blocking(move || {
        admit(
            &services,
            &head,
            &slug,
            has_id.then_some(raw_id.as_str()),
            write.action,
        )
    })
    .await
    .ok_or_else(|| Problem::new(Code::Failed))??;

    // The base is named before the body is read.
    let if_match = base::if_match(&headers)?;
    let bytes = read_body(&headers, body, write.limit).await?;

    blocking(move || {
        let request = Request {
            services: admitted.services,
            wiki: admitted.wiki,
            principal: admitted.principal,
            t,
            id: admitted.id,
            if_match,
        };
        let fields = body::parse(&bytes)?;
        work(&request, fields)
    })
    .await
    .unwrap_or_else(|| Err(Problem::new(Code::Failed)))
}

/// Who may go on: the services, the wiki, the principal and the proposal's id.
struct Admitted {
    services: Arc<Services>,
    wiki: WikiRecord,
    principal: Principal,
    id: Option<ProposalId>,
}

/// Steps 1 to 4 of the order above.
fn admit(
    services: &Arc<Services>,
    headers: &HeaderMap,
    slug: &str,
    raw_id: Option<&str>,
    action: inklkunn_core::Action,
) -> Result<Admitted, Problem> {
    let presented = presented_token(headers)?;
    if !presented && !crate::lang::from_this_site(headers) {
        return Err(Problem::new(Code::Forbidden));
    }
    if presented && matches!(services.identity.principal(headers), Principal::Anonymous) {
        return Err(Problem::new(Code::InvalidToken));
    }
    let wiki = open(services, headers, slug).ok_or_else(|| Problem::new(Code::NotFound))?;
    let principal = services.identity.principal(headers);
    if matches!(principal, Principal::Anonymous) {
        return Err(Problem::new(Code::AuthenticationRequired));
    }
    let id = match raw_id {
        Some(raw) => Some(
            ProposalId::parse(&decode(raw).unwrap_or_default())
                .ok_or_else(|| Problem::new(Code::NotFound))?,
        ),
        None => None,
    };
    let resource = match &id {
        Some(id) => Resource::Proposal {
            wiki: wiki.id.clone(),
            id: id.to_string(),
        },
        None => Resource::Wiki {
            wiki: wiki.id.clone(),
        },
    };
    if !services
        .authorizer
        .authorize(&principal, action, &resource)
        .is_allowed()
    {
        return Err(Problem::new(Code::Forbidden));
    }
    Ok(Admitted {
        services: services.clone(),
        wiki,
        principal,
        id,
    })
}

/// Does the request present an API token? `Err` when it presents something in the `Authorization` header that
/// cannot be one: a scheme other than `Bearer`, no token, a token with a space in it, or two headers.
fn presented_token(headers: &HeaderMap) -> Result<bool, Problem> {
    let mut all = headers.get_all(header::AUTHORIZATION).iter();
    let Some(first) = all.next() else {
        return Ok(false);
    };
    let bad = || Problem::new(Code::InvalidToken);
    if all.next().is_some() {
        return Err(bad());
    }
    let value = first.to_str().map_err(|_| bad())?;
    match value.split_once(' ') {
        Some((scheme, token))
            if scheme.eq_ignore_ascii_case("bearer")
                && !token.is_empty()
                && !token.contains(' ') =>
        {
            Ok(true)
        }
        _ => Err(bad()),
    }
}

/// Is the body declared JSON? `application/json`, with no parameter but a UTF-8 `charset`.
fn is_json(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let mut parts = value.split(';').map(str::trim);
    parts
        .next()
        .is_some_and(|media| media.eq_ignore_ascii_case("application/json"))
        && parts.all(|param| {
            param.split_once('=').is_some_and(|(k, v)| {
                k.trim().eq_ignore_ascii_case("charset")
                    && v.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
            })
        })
}

/// The body, read to at most `limit` bytes. A length declared over the bound is refused without reading.
async fn read_body(headers: &HeaderMap, body: Body, limit: usize) -> Result<Vec<u8>, Problem> {
    if !is_json(headers) {
        return Err(Problem::new(Code::UnsupportedMediaType));
    }
    let too_large = || Problem::new(Code::BodyTooLarge).named("limit", limit.to_string());
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|n| n > limit) {
        return Err(too_large());
    }
    match axum::body::to_bytes(body, limit).await {
        Ok(bytes) => Ok(bytes.to_vec()),
        // It fails for a body over the bound (the one a caller can act on) or for one the client broke off,
        // which nobody is left to read an answer to.
        Err(_) => Err(too_large()),
    }
}

#[cfg(test)]
mod tests;
