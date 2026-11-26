//! How an answer of the API leaves: its media type, its `ETag`, the `304` for a matching `If-None-Match`, the
//! `Link` to the description, and a problem detail's headers. One place, so no endpoint can forget one.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use serde_json::Value;

use super::PROBLEM_MEDIA_TYPE;
use super::problem::Problem;
use crate::catalog::T;

/// A successful answer's content.
#[derive(Debug, Clone)]
pub(crate) struct Reply {
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
    /// The status of a success: 200, or 201 for a write that made something.
    pub(crate) status: StatusCode,
    /// A validator the endpoint names itself, instead of the body's hash: the base a write sends back as
    /// `If-Match`. It names a state of a proposal, not these bytes, so it never answers `304`.
    pub(crate) etag: Option<String>,
    /// Where what a write made lives.
    pub(crate) location: Option<String>,
    /// The `next_cursor` of the body, when the body is a page of a list that goes on: the one value the body's member
    /// and the `Link` header are both made from (RFC 007 D-9), read where the body is made so that no list can say one
    /// and not the other.
    pub(crate) next_cursor: Option<String>,
    /// The address of the page that follows, for the `Link` header's `rel="next"` (RFC 8288): made from the request's
    /// own address and `next_cursor` by the shared start and finish, never by an endpoint.
    pub(crate) next_link: Option<String>,
}

impl Reply {
    /// A JSON document, with `<`, `>` and `&` written as `<`, `>` and `&`: the same document to
    /// any JSON reader, and no markup of an author's in it for anything that is not one (a response sniffed as
    /// HTML, a body pasted into a page). They can only be inside a string, so no other byte changes.
    pub(crate) fn json(value: &Value) -> Result<Reply, Problem> {
        match serde_json::to_vec(value) {
            Ok(body) => {
                let mut reply = Reply::bytes("application/json", no_markup(&body));
                reply.next_cursor = value
                    .get("next_cursor")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Ok(reply)
            }
            Err(_) => Err(Problem::new(super::problem::Code::Failed)),
        }
    }

    /// An HTML fragment (the page body, already sanitized).
    pub(crate) fn html(fragment: &str) -> Reply {
        Reply::bytes("text/html; charset=utf-8", fragment.as_bytes().to_vec())
    }

    /// Bytes of a stated media type.
    pub(crate) fn bytes(content_type: &'static str, body: Vec<u8>) -> Reply {
        Reply {
            content_type,
            body,
            status: StatusCode::OK,
            etag: None,
            location: None,
            next_cursor: None,
            next_link: None,
        }
    }

    /// The same answer with this validator (a base) in place of the body's hash.
    #[must_use]
    pub(crate) fn with_etag(mut self, etag: String) -> Reply {
        self.etag = Some(etag);
        self
    }

    /// The same answer as a `201`, naming what it made.
    #[must_use]
    pub(crate) fn created(mut self, location: String) -> Reply {
        self.status = StatusCode::CREATED;
        self.location = Some(location);
        self
    }
}

/// The JSON escape of one of the three characters of markup (`<`, `>`, `&`), or `None` for any other. Every
/// JSON document the service writes (the API's, the discovery documents') escapes exactly these.
pub(crate) fn markup_escape(c: char) -> Option<&'static str> {
    match c {
        '<' => Some("\\u003c"),
        '>' => Some("\\u003e"),
        '&' => Some("\\u0026"),
        _ => None,
    }
}

/// `bytes` of a JSON document with the three characters of markup written as JSON escapes.
fn no_markup(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match markup_escape(char::from(*byte)) {
            // The three are ASCII, and a byte of a multi-byte character is never one of them.
            Some(escape) if byte.is_ascii() => out.extend_from_slice(escape.as_bytes()),
            _ => out.push(*byte),
        }
    }
    out
}

/// FNV-1a over the bytes: a short validator that changes when the body does. It is a cache validator, not a
/// security measure, so a fast hash with no secret is the right size (the stylesheet's fingerprint is the same).
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The strong `ETag` of a body: the hash and the length, so two bodies of different sizes never share one.
pub(crate) fn etag_of(body: &[u8]) -> String {
    format!("\"{:016x}-{:x}\"", fingerprint(body), body.len())
}

/// Does an `If-None-Match` value match `etag`? `*` matches any existing answer; otherwise a list of entity
/// tags, compared weakly (a `W/` prefix is ignored: RFC 9110, 13.1.2).
pub(crate) fn matches(if_none_match: &str, etag: &str) -> bool {
    if_none_match.trim() == "*"
        || if_none_match
            .split(',')
            .map(|candidate| {
                let candidate = candidate.trim();
                candidate.strip_prefix("W/").unwrap_or(candidate)
            })
            .any(|candidate| candidate == etag)
}

/// The `Link` header every answer of the API carries: where the description is. It is written out because
/// `concat!` cannot take a constant; a test holds it to `OPENAPI_PATH` and `OPENAPI_MEDIA_TYPE`.
pub(crate) const SERVICE_DESC: &str = "</-/api/v1/openapi.json>; rel=\"service-desc\"; type=\"application/vnd.oai.openapi+json;version=3.1.0\"";

/// The seconds a caller is told to wait when the wiki could not be read just now.
const UNAVAILABLE_RETRY: u64 = 30;

fn plain_failure() -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    response
}

/// Turn what an endpoint came to into the response. `if_none_match` is the request's header, if any; it is
/// consulted only for a success, which is only ever reached by a caller who was allowed to read it.
pub(crate) fn finish(
    t: T,
    if_none_match: Option<&str>,
    outcome: Result<Reply, Problem>,
) -> Response {
    let language = HeaderValue::from_static(t.lang().tag());
    let next = outcome
        .as_ref()
        .ok()
        .and_then(|reply| reply.next_link.clone());
    let mut response = match outcome {
        Ok(reply) => {
            let named = reply.etag.is_some();
            let etag = reply.etag.clone().unwrap_or_else(|| etag_of(&reply.body));
            let validator = HeaderValue::from_str(&etag);
            let unchanged = !named && if_none_match.is_some_and(|value| matches(value, &etag));
            let mut response = if unchanged {
                let mut response = Response::new(Body::empty());
                *response.status_mut() = StatusCode::NOT_MODIFIED;
                response
            } else {
                let mut response = Response::new(Body::from(reply.body));
                *response.status_mut() = reply.status;
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(reply.content_type),
                );
                response
            };
            if let Some(Ok(location)) = reply.location.as_deref().map(HeaderValue::from_str) {
                response.headers_mut().insert(header::LOCATION, location);
            }
            if let Ok(validator) = validator {
                response.headers_mut().insert(header::ETAG, validator);
            }
            response
        }
        Err(problem) => {
            let body = no_markup(&serde_json::to_vec(&problem.body(t)).unwrap_or_default());
            if body.is_empty() {
                return plain_failure();
            }
            let mut response = Response::new(Body::from(body));
            *response.status_mut() = problem.code.status();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(PROBLEM_MEDIA_TYPE),
            );
            let waiting = problem.retry_after_secs().or_else(|| {
                (problem.code == super::problem::Code::Unavailable).then_some(UNAVAILABLE_RETRY)
            });
            if let Some(seconds) = waiting {
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
            }
            if let Some(challenge) = problem.code.challenge() {
                response.headers_mut().insert(
                    header::WWW_AUTHENTICATE,
                    HeaderValue::from_static(challenge),
                );
            }
            response
        }
    };
    response
        .headers_mut()
        .insert(header::CONTENT_LANGUAGE, language);
    response
        .headers_mut()
        .insert(header::LINK, HeaderValue::from_static(SERVICE_DESC));
    // A page of a list that goes on says where the next one is, beside the body's `next_cursor` (RFC 8288; RFC 007 D-9). It is
    // a second `Link` header after the description's, so a client that reads the first one reads what it always read.
    if let Some(Ok(next)) = next
        .as_deref()
        .map(|address| HeaderValue::from_str(&format!("<{address}>; rel=\"next\"")))
    {
        response.headers_mut().append(header::LINK, next);
    }
    response
}

/// The `If-None-Match` of a request, if it is text.
pub(crate) fn if_none_match(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}
