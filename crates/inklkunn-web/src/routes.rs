//! **The one place routes are registered** (RFC 002 D-2; internal design ENF-3; a workspace gate fails if
//! `.route(`, `.nest(`, `.merge(`, `.fallback(` or `Router::new` appears in any other web source). Every
//! entry has the check that guards it: an `Action` the handler consults before it touches wiki data, or a
//! stated reason it serves none. The routers are built **from** the table, so what is served is what is
//! declared, and a test holds the table to the URL scheme's one rule: nothing but a wiki's slug, then the
//! reserved prefix `-/`, may precede a page title, so no title can ever be shadowed by a route.

use axum::Router;
use axum::http::StatusCode;
use axum::middleware::from_fn;
use axum::routing::{MethodRouter, get};
use inklkunn_core::{Guard, RouteError, RouteTable};

use crate::config::{Scheme, WebConfig};
use crate::headers::{self, Surface};
use crate::origin::Dispatch;

/// One declared entry point and the handler that serves it.
struct Spec {
    path: &'static str,
    methods: &'static [&'static str],
    guard: Guard,
    handler: fn() -> MethodRouter,
}

/// The application origin's entry points, in declaration order.
fn application_specs() -> Vec<Spec> {
    vec![Spec {
        path: "/-/health",
        methods: &["GET", "HEAD"],
        guard: Guard::Public {
            reason: "liveness for the proxy: a fixed word, no wiki data, the same for everyone",
        },
        handler: || get(health),
    }]
}

async fn health() -> &'static str {
    "ok\n"
}

async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "Not found.\n")
}

/// The application origin's declared entry points.
pub fn application_table() -> Result<RouteTable, RouteError> {
    let mut table = RouteTable::new();
    for spec in application_specs() {
        table.register(spec.path, spec.methods, spec.guard)?;
    }
    Ok(table)
}

/// The application origin's router, built from its table. Anything not declared is a 404 that carries the
/// same headers as every other response.
pub fn application(https: bool) -> Result<Router, RouteError> {
    application_table()?;
    let mut router = Router::new();
    for spec in application_specs() {
        router = router.route(spec.path, (spec.handler)());
    }
    Ok(router.fallback(not_found).layer(from_fn(move |req, next| {
        headers::layer(Surface::Application { https }, req, next)
    })))
}

/// The content origin's router. It serves wiki-authored bytes as inert attachments and nothing else; no
/// attachment is served yet, so everything is a 404 with the content origin's headers.
pub fn content(https: bool) -> Router {
    Router::new()
        .fallback(not_found)
        .layer(from_fn(move |req, next| {
            headers::layer(Surface::Content { https }, req, next)
        }))
}

/// Both origins behind one listener.
pub fn site(config: &WebConfig) -> Result<Router, RouteError> {
    let https = |scheme| scheme == Scheme::Https;
    let dispatch = Dispatch::new(
        application(https(config.app_origin.scheme()))?,
        content(https(config.content_origin.scheme())),
        config.app_origin.clone(),
        config.content_origin.clone(),
    );
    Ok(Router::new().fallback_service(dispatch))
}

#[cfg(test)]
mod tests;
