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
}

impl Reply {
    /// A JSON document.
    pub(crate) fn json(value: &Value) -> Result<Reply, Problem> {
        match serde_json::to_vec(value) {
            Ok(body) => Ok(Reply {
                content_type: "application/json",
                body,
            }),
            Err(_) => Err(Problem::new(super::problem::Code::Failed)),
        }
    }

    /// An HTML fragment (the page body, already sanitized).
    pub(crate) fn html(fragment: &str) -> Reply {
        Reply {
            content_type: "text/html; charset=utf-8",
            body: fragment.as_bytes().to_vec(),
        }
    }

    /// Bytes of a stated media type.
    pub(crate) fn bytes(content_type: &'static str, body: Vec<u8>) -> Reply {
        Reply { content_type, body }
    }
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
    let mut response = match outcome {
        Ok(reply) => {
            let etag = etag_of(&reply.body);
            let validator = HeaderValue::from_str(&etag);
            let unchanged = if_none_match.is_some_and(|value| matches(value, &etag));
            let mut response = if unchanged {
                let mut response = Response::new(Body::empty());
                *response.status_mut() = StatusCode::NOT_MODIFIED;
                response
            } else {
                let mut response = Response::new(Body::from(reply.body));
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(reply.content_type),
                );
                response
            };
            if let Ok(validator) = validator {
                response.headers_mut().insert(header::ETAG, validator);
            }
            response
        }
        Err(problem) => {
            let body = serde_json::to_vec(&problem.body(t)).unwrap_or_default();
            if body.is_empty() {
                return plain_failure();
            }
            let mut response = Response::new(Body::from(body));
            *response.status_mut() = problem.code.status();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(PROBLEM_MEDIA_TYPE),
            );
            if problem.code == super::problem::Code::Unavailable {
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("30"));
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
    response
}

/// The `If-None-Match` of a request, if it is text.
pub(crate) fn if_none_match(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}
