//! The read API (RFC 003 D-13; handoff 003 T8; external design API-01, API-02, API-04): the GET half of IX-4, a
//! JSON description of what the pages show, under a **versioned path**, described by an OpenAPI 3.1 document
//! that is **generated from the router's route table** and never kept in parallel.
//!
//! # Where it lives
//!
//! * `/{slug}/-/api/v1/…` — a wiki's endpoints. The prefix sits under the reserved `-/` of the URL scheme
//!   (RFC 003 D-1), so no page title can collide with it, and the version is in the path, so a v2 is a new
//!   prefix beside this one. Everything under `/{slug}/-/api/` that is not a documented endpoint answers the
//!   generic not-found problem, so a mistyped endpoint is never mistaken for a page.
//! * `/-/api/v1/openapi.json` — the description, one document for the whole site (the slug is a parameter of
//!   every path in it). Every response of the API carries `Link: </-/api/v1/openapi.json>; rel="service-desc"`.
//!
//! # What an endpoint is
//!
//! Each endpoint is one `Doc` (what it is, its parameters, its answer, its problems — in `describe`) and one
//! handler (in `endpoints`). `endpoints`'s list is the only thing the route registry (`routes.rs`) reads, so
//! the endpoints are in the same table as the web pages and carry the same check: `Guard::Action(Action::Read)`,
//! consulted by the handler's first step before it touches a wiki. The OpenAPI document (`openapi`) is built
//! from that table and from the `Doc`s together; a path in one and not the other fails the build of the
//! document, and a test holds it both ways.
//!
//! # What an answer is
//!
//! * A success is JSON (`application/json`), or for the fragment endpoint the page's sanitized HTML fragment;
//!   every success has an `ETag` (a hash of the body) and answers `304` to a matching `If-None-Match`, after
//!   the caller was authorized, so a `304` tells a caller nothing it could not read.
//! * A failure is a **problem detail** (RFC 9457, `application/problem+json`; `problem`). Its `type` is
//!   `urn:inklkunn:problem:<code>`: a stable name that does not depend on the host the server is reached by and
//!   is never a link that might not resolve. The codes are the registry `problem::Code::ALL`, listed in the
//!   OpenAPI document. `title`, `detail` and `hint` are catalog messages in the caller's language (the same
//!   negotiation as the pages: the language cookie, else `Accept-Language`, else English); the `type`, the
//!   `status` and the extension members (`parameter`, `view`, `feature`, `candidates`) do not change with it.
//! * **Existence-neutral.** A wiki that does not exist and a wiki the caller may not read answer the same
//!   problem (`not-found`, 404), byte for byte.
//! * **Bounded.** A list holds at most [`MAX_PAGE_SIZE`] items; a request for more is refused with
//!   `invalid-parameter`, never silently trimmed. Lists use cursors (a position, not an offset).
//! * **Strict.** An unknown or repeated query parameter is `invalid-parameter`: a mistyped `limt=5` is told so.
//!
//! The browser's same-origin policy keeps other sites from reading these answers (`Cross-Origin-Resource-Policy:
//! same-origin`); there is no CORS. Machine credentials (API-05) arrive with RFC 004.

use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use axum::routing::MethodRouter;
use inklkunn_core::Guard;

use crate::pages::{self, Page, PageState};

pub(crate) mod describe;
pub(crate) mod endpoints;
pub(crate) mod openapi;
pub(crate) mod problem;
mod reply;
mod request;

pub use inklkunn_core::read::MAX_PAGE_SIZE;

/// The prefix of a wiki's endpoints, as the route registry writes it.
pub const WIKI_PREFIX: &str = "/{slug}/-/api/v1";

/// The prefix of the site-wide endpoints.
pub const SITE_PREFIX: &str = "/-/api/v1";

/// Where the OpenAPI document is served.
pub const OPENAPI_PATH: &str = "/-/api/v1/openapi.json";

/// The media type of the OpenAPI document.
pub const OPENAPI_MEDIA_TYPE: &str = "application/vnd.oai.openapi+json;version=3.1.0";

/// The media type of a problem detail.
pub const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";

/// One entry of the route registry that belongs to the API: the description, the check that guards it, and the
/// handler. `routes.rs` turns each into a registered route; nothing else registers one.
pub(crate) struct Endpoint {
    /// The registry key.
    pub path: &'static str,
    /// What the endpoint is, for the description. `None` for the two catch-alls that answer the not-found
    /// problem for an address under the API that is not an endpoint.
    pub doc: Option<&'static describe::Doc>,
    /// The check that guards it.
    pub guard: Guard,
    /// The handler, as the router takes it.
    pub handler: fn() -> MethodRouter<PageState>,
}

/// Every endpoint of the API, in documentation order.
pub(crate) fn endpoints() -> Vec<Endpoint> {
    endpoints::all()
}

/// Is `path` an address of the API (documented or not)?
pub(crate) fn is_api_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix('/') else {
        return false;
    };
    let mut segments = rest.split('/');
    match (segments.next(), segments.next(), segments.next()) {
        (Some("-"), Some("api"), _) => true,
        (Some(slug), Some("-"), Some("api")) => !slug.is_empty(),
        _ => false,
    }
}

/// The answer to a method an address of the API does not take: a problem detail in the caller's language.
/// (The `Allow` header is the router's own.)
pub(crate) fn method_not_allowed(headers: &HeaderMap) -> Response {
    request::answer_plain(headers, problem::Code::MethodNotAllowed)
}

/// The answer to an address of the API that is not an endpoint.
pub(crate) fn not_found(headers: &HeaderMap) -> Response {
    request::answer_plain(headers, problem::Code::NotFound)
}

/// The router's fallback for an address nothing serves: a problem detail under the API's prefixes, the page
/// that says *not found* everywhere else.
pub(crate) async fn fallback_not_found(page: Page, uri: Uri, headers: HeaderMap) -> Response {
    if is_api_path(uri.path()) {
        return not_found(&headers);
    }
    pages::not_found(page).await
}

/// The router's fallback for a method an address does not take: a problem detail under the API's prefixes, the
/// page that says so everywhere else.
pub(crate) async fn fallback_method_not_allowed(
    page: Page,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if is_api_path(uri.path()) {
        return method_not_allowed(&headers);
    }
    pages::method_not_allowed(page).await
}

#[cfg(test)]
mod tests;
