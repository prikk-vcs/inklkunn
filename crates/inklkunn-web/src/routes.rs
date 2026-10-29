//! **The one place routes are registered** (RFC 002 D-2; internal design ENF-3; a workspace gate fails if
//! `.route(`, `.nest(`, `.merge(`, `.fallback(` or `Router::new` appears in any other web source). Every
//! entry has the check that guards it: an `Action` the handler consults before it touches wiki data, or a
//! stated reason it serves none. The routers are built **from** the table, so what is served is what is
//! declared, and a test holds the table to the URL scheme's one rule: nothing but a wiki's slug, then the
//! reserved prefix `-/`, may precede a page title, so no title can ever be shadowed by a route.

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::from_fn;
use axum::routing::{MethodRouter, get};
use inklkunn_core::{Action, Guard, RouteError, RouteTable};

use crate::config::{Scheme, WebConfig};
use crate::headers::{self, Surface};
use crate::origin::Dispatch;
use crate::pages::{self, LANGUAGE_PATH, PageState};
use crate::services::Services;
use crate::{api, history, probe, style, wiki};

/// One declared entry point and the handler that serves it.
struct Spec {
    path: &'static str,
    methods: &'static [&'static str],
    guard: Guard,
    handler: fn() -> MethodRouter<PageState>,
}

/// The application origin's entry points, in declaration order: the pages', then the read API's (which declare
/// themselves, with their descriptions, in `api::endpoints`).
fn application_specs() -> Vec<Spec> {
    let mut specs = page_specs();
    specs.extend(api::endpoints().into_iter().map(|endpoint| Spec {
        path: endpoint.path,
        methods: &["GET", "HEAD"],
        guard: endpoint.guard,
        handler: endpoint.handler,
    }));
    specs
}

/// The web pages' entry points.
fn page_specs() -> Vec<Spec> {
    vec![
        Spec {
            path: "/-/health",
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "liveness for the proxy: a fixed word, no wiki data, the same for everyone",
            },
            handler: || get(probe::health),
        },
        Spec {
            path: LANGUAGE_PATH,
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Public {
                reason: "the language switcher: it lists the languages and sets the visitor's own language cookie; it names no wiki and serves no wiki data",
            },
            handler: || {
                get(pages::language_page)
                    .post(pages::choose_language)
                    .layer(DefaultBodyLimit::max(pages::FORM_LIMIT))
            },
        },
        Spec {
            path: style::ROUTE,
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "the one stylesheet: static, the same for everyone, embedded in the binary; it serves no wiki data",
            },
            handler: || get(style::serve),
        },
        Spec {
            path: "/-/wikis",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(wiki::wikis),
        },
        Spec {
            path: "/{slug}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(wiki::front),
        },
        Spec {
            path: "/{slug}/-/about",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(wiki::about),
        },
        Spec {
            path: "/{slug}/-/verification",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Read),
            handler: || {
                get(wiki::verification)
                    .post(wiki::check_now)
                    .layer(DefaultBodyLimit::max(pages::FORM_LIMIT))
            },
        },
        // --- handoff 003 T5: a page's past. Each opens with the wiki's read check, as every handler does.
        Spec {
            path: "/{slug}/-/history/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(history::history),
        },
        Spec {
            path: "/{slug}/-/change/{block}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(history::change),
        },
        Spec {
            path: "/{slug}/-/diff/{from}/{to}/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(history::diff),
        },
        Spec {
            path: "/{slug}/-/at/{block}/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(history::at),
        },
        Spec {
            path: "/{slug}/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(wiki::view_page),
        },
    ]
}

/// The application origin's declared entry points.
pub fn application_table() -> Result<RouteTable, RouteError> {
    let mut table = RouteTable::new();
    for spec in application_specs() {
        table.register(spec.path, spec.methods, spec.guard)?;
    }
    Ok(table)
}

/// The application origin's router, built from its table. Anything not declared is a 404 page in the visitor's
/// language that carries the same headers as every other response.
pub fn application(https: bool) -> Result<Router, RouteError> {
    application_with(https, None)
}

/// [`application`], serving the wikis `services` reaches.
pub fn application_with(
    https: bool,
    services: Option<Arc<Services>>,
) -> Result<Router, RouteError> {
    application_table()?;
    let mut router = Router::new();
    for spec in application_specs() {
        router = router.route(spec.path, (spec.handler)());
    }
    Ok(router
        .fallback(api::fallback_not_found)
        .method_not_allowed_fallback(api::fallback_method_not_allowed)
        .with_state(PageState { https, services })
        .layer(from_fn(move |req, next| {
            headers::layer(Surface::Application { https }, req, next)
        })))
}

/// The content origin's router. It serves wiki-authored bytes as inert attachments and nothing else; no
/// attachment is served yet, so everything is a plain-text 404 with the content origin's headers (its policy
/// allows no stylesheet and no form, so it answers no page).
pub fn content(https: bool) -> Router {
    Router::new()
        .fallback(pages::plain_not_found)
        .layer(from_fn(move |req, next| {
            headers::layer(Surface::Content { https }, req, next)
        }))
}

/// Both origins behind one listener, serving no wiki (every wiki address is then the generic not-found page).
pub fn site(config: &WebConfig) -> Result<Router, RouteError> {
    site_with(config, None)
}

/// Both origins behind one listener, serving the wikis `services` reaches.
pub fn site_with(
    config: &WebConfig,
    services: Option<Arc<Services>>,
) -> Result<Router, RouteError> {
    let https = |scheme| scheme == Scheme::Https;
    let dispatch = Dispatch::new(
        application_with(https(config.app_origin.scheme()), services)?,
        content(https(config.content_origin.scheme())),
        config.app_origin.clone(),
        config.content_origin.clone(),
    );
    Ok(Router::new().fallback_service(dispatch))
}

#[cfg(test)]
mod tests;
