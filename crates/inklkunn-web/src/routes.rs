//! **The one place routes are registered** (RFC 002 D-2; internal design ENF-3; a workspace gate fails if
//! `.route(`, `.nest(`, `.merge(`, `.fallback(` or `Router::new` appears in any other web source). Every
//! entry has the check that guards it: an `Action` the handler consults before it touches wiki data, or a
//! stated reason it serves none. The routers are built **from** the table, so what is served is what is
//! declared, and a test holds the table to the URL scheme's one rule: nothing but a wiki's slug, then the
//! reserved prefix `-/`, may precede a page title, so no title can ever be shadowed by a route.

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{MethodRouter, get, post};
use inklkunn_core::{Action, Guard, RouteError, RouteTable};

use crate::config::{Scheme, WebConfig};
use crate::discover::Origins;
use crate::headers::{self, Surface};
use crate::origin::Dispatch;
use crate::pages::{self, LANGUAGE_PATH, PageState};
use crate::services::Services;
use crate::{
    account, api, changes, csrf, discover, health, history, inbox, knowledge, probe, proposals,
    raw, search, signin, style, views, watch, wiki,
};

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
        methods: endpoint.methods,
        guard: endpoint.guard,
        handler: endpoint.handler,
    }));
    specs
}

/// The web pages' entry points.
fn page_specs() -> Vec<Spec> {
    let mut specs = proposal_specs();
    specs.extend(account_specs());
    specs.extend(wiki_specs());
    specs.extend(view_specs());
    specs
}

/// A write form's entry point: the reading handler, the writing one and the bound on the body.
macro_rules! form_route {
    ($get:expr, $post:expr, $limit:expr) => {
        || get($get).post($post).layer(DefaultBodyLimit::max($limit))
    };
}

/// A write that is only a post, with the bound on its body.
macro_rules! post_route {
    ($post:expr, $limit:expr) => {
        || post($post).layer(DefaultBodyLimit::max($limit))
    };
}

/// The proposals' entry points (RFC 004 D-2, D-3; handoff 004 T3). Each carries the action the proposal
/// service authorizes it with; every handler begins with the wiki's read check, so a wiki the visitor may not
/// read answers as one that is not there.
fn proposal_specs() -> Vec<Spec> {
    vec![
        Spec {
            path: "/{slug}/-/proposals",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(proposals::list::list),
        },
        Spec {
            path: "/{slug}/-/proposals/new",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Propose),
            handler: form_route!(
                proposals::new::new_form,
                proposals::submit::new_post,
                proposals::PROPOSAL_FORM_LIMIT
            ),
        },
        Spec {
            path: "/{slug}/-/proposals/curate",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Propose),
            handler: form_route!(
                proposals::curate::curate_form,
                proposals::curate::curate_post,
                proposals::PROPOSAL_FORM_LIMIT
            ),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(proposals::show::proposal),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/edit",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Propose),
            handler: form_route!(
                proposals::submit::edit_form,
                proposals::submit::edit_post,
                proposals::PROPOSAL_FORM_LIMIT
            ),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/comment",
            methods: &["POST"],
            guard: Guard::Action(Action::Comment),
            handler: post_route!(proposals::review::comment, proposals::SMALL_FORM_LIMIT),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/decide",
            methods: &["POST"],
            guard: Guard::Action(Action::Review),
            handler: post_route!(proposals::review::decide, proposals::SMALL_FORM_LIMIT),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/accept",
            methods: &["POST"],
            guard: Guard::Action(Action::Accept),
            handler: post_route!(proposals::accept::accept, pages::FORM_LIMIT),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/reconfirm",
            methods: &["POST"],
            guard: Guard::Action(Action::Propose),
            handler: post_route!(proposals::touched::reconfirm, proposals::SMALL_FORM_LIMIT),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/decline",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Accept),
            handler: form_route!(
                proposals::review::decline_form,
                proposals::review::decline_post,
                proposals::SMALL_FORM_LIMIT
            ),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/ask",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Accept),
            handler: form_route!(
                proposals::review::ask_form,
                proposals::review::ask_post,
                proposals::SMALL_FORM_LIMIT
            ),
        },
        Spec {
            path: "/{slug}/-/proposals/{id}/withdraw",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Propose),
            handler: form_route!(
                proposals::review::withdraw_form,
                proposals::review::withdraw_post,
                proposals::SMALL_FORM_LIMIT
            ),
        },
        Spec {
            path: "/{slug}/-/discussion/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(proposals::list::discussion),
        },
    ]
}

/// The views' entry points (RFC 005 part 2 D-9; handoff 005 K-T8): the list, the form that creates one, the
/// comparison of two. Each begins with the wiki's read check, so a wiki the visitor may not read answers as one that
/// is not there; the form's handlers then ask `create-view` of the authorizer before they show a form or act.
fn view_specs() -> Vec<Spec> {
    vec![
        Spec {
            path: "/{slug}/-/views",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(views::list::list),
        },
        Spec {
            path: "/{slug}/-/views/new",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::CreateView),
            handler: form_route!(
                views::new::new_form,
                views::new::new_post,
                views::new::FORM_LIMIT
            ),
        },
        Spec {
            path: "/{slug}/-/views/compare",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(views::compare::compare),
        },
        Spec {
            path: "/{slug}/-/views/compare/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(views::compare::compare_page),
        },
    ]
}

/// Signing in and out, joining, an invitation and the account page (the sign-in handoff's U1 and U2). None
/// serves wiki data: they are the visitor's own sign-in, and the signed-in person's own account. Where sign-in
/// is not served by this build every one of them is the generic not-found page.
fn account_specs() -> Vec<Spec> {
    vec![
        Spec {
            path: signin::SIGN_IN_PATH,
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Public {
                reason: "sign-in: it asks for a name and a password and serves no wiki data; what it answers never says whether an account exists",
            },
            handler: || {
                get(signin::sign_in_page)
                    .post(signin::sign_in_post)
                    .layer(DefaultBodyLimit::max(signin::FORM_LIMIT))
            },
        },
        Spec {
            path: signin::SIGN_OUT_PATH,
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Public {
                reason: "sign-out: it ends this browser's own session and serves no wiki data; its form carries the session's token like every write",
            },
            handler: || {
                get(signin::sign_out_page)
                    .post(signin::sign_out_post)
                    .layer(DefaultBodyLimit::max(pages::FORM_LIMIT))
            },
        },
        Spec {
            path: signin::JOIN_PATH,
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Public {
                reason: "joining: where the operator opened it to anyone a person makes an account; it serves no wiki data, and says joining is not open otherwise",
            },
            handler: || {
                get(signin::join_page)
                    .post(signin::join_post)
                    .layer(DefaultBodyLimit::max(signin::FORM_LIMIT))
            },
        },
        Spec {
            path: signin::INVITE_ROUTE,
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Public {
                reason: "an invitation: the one-time link an operator made, opened to make an account; it serves no wiki data and answers the same for a link that was used, one that ended and one that never was",
            },
            handler: || {
                get(signin::invite_page)
                    .post(signin::invite_post)
                    .layer(DefaultBodyLimit::max(signin::FORM_LIMIT))
            },
        },
        Spec {
            path: signin::OIDC_PATH,
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "starts a sign-in with the configured provider: it sends the browser to the provider and serves no wiki data; where no provider is configured it is the generic not-found page",
            },
            handler: || get(signin::oidc::start),
        },
        Spec {
            path: signin::OIDC_CALLBACK_PATH,
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "where the provider sends the browser back: it finishes the sign-in only for the browser that started it (the login-state cookie and the state must agree, once) and serves no wiki data",
            },
            handler: || get(signin::oidc::callback),
        },
        Spec {
            path: signin::INVITE_OIDC_ROUTE,
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "starts an invitation with the configured provider instead of a password: it sends the browser to the provider and serves no wiki data, and answers the same for a link that was used, one that ended and one that never was",
            },
            handler: || get(signin::oidc::start_invitation),
        },
        Spec {
            path: account::PATH,
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "the signed-in person's own account (name, sessions, tokens); it serves no wiki data, and a visitor with no session is sent to sign in",
            },
            handler: || get(account::account_page),
        },
        Spec {
            path: account::NAME_PATH,
            methods: &["POST"],
            guard: Guard::Public {
                reason: "saves the signed-in person's own display name; it serves no wiki data and needs a session and its token",
            },
            handler: || post_route_for_account(account::save_name),
        },
        Spec {
            path: account::SESSIONS_END_PATH,
            methods: &["POST"],
            guard: Guard::Public {
                reason: "ends one of the signed-in person's own sessions; it serves no wiki data and needs a session and its token",
            },
            handler: || post_route_for_account(account::end_session),
        },
        Spec {
            path: account::TOKENS_PATH,
            methods: &["POST"],
            guard: Guard::Public {
                reason: "makes an API token for the signed-in person's own account and shows it once; it serves no wiki data and needs a session and its token",
            },
            handler: || post_route_for_account(account::make_token),
        },
        Spec {
            path: account::TOKENS_REVOKE_PATH,
            methods: &["POST"],
            guard: Guard::Public {
                reason: "revokes one of the signed-in person's own API tokens; it serves no wiki data and needs a session and its token",
            },
            handler: || post_route_for_account(account::revoke_token),
        },
        Spec {
            path: account::PASSWORD_PATH,
            methods: &["POST"],
            guard: Guard::Public {
                reason: "changes the signed-in person's own password after the current one is checked (throttled and constant-work like sign-in) and ends their other sessions and tokens; it serves no wiki data and needs a session and its token",
            },
            handler: || post_route_for_account(account::change_password),
        },
    ]
}

/// A write of the account page, with the bound on its body.
fn post_route_for_account<H, T>(handler: H) -> MethodRouter<PageState>
where
    H: axum::handler::Handler<T, PageState>,
    T: 'static,
{
    post(handler).layer(DefaultBodyLimit::max(account::FORM_LIMIT))
}

/// The other web pages' entry points.
fn wiki_specs() -> Vec<Spec> {
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
        // --- handoff 004 T6: the inbox (account-wide, so outside any wiki's address) and following a page.
        Spec {
            path: "/-/inbox",
            methods: &["GET", "HEAD", "POST"],
            guard: Guard::Action(Action::Read),
            handler: || {
                get(inbox::inbox_page)
                    .post(inbox::mark_read)
                    .layer(DefaultBodyLimit::max(pages::FORM_LIMIT))
            },
        },
        Spec {
            path: "/{slug}/-/watch/{*title}",
            methods: &["POST"],
            guard: Guard::Action(Action::Read),
            handler: || post(watch::watch).layer(DefaultBodyLimit::max(pages::FORM_LIMIT)),
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
            path: "/robots.txt",
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "robots.txt for crawlers: fixed rules and the sitemap addresses of the wikis anyone may read; it names no wiki that is not public",
            },
            handler: || get(discover::robots),
        },
        Spec {
            path: "/{slug}/-/changes",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(changes::changes_page),
        },
        Spec {
            path: "/{slug}/-/changes.atom",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(changes::changes_atom),
        },
        Spec {
            path: "/{slug}/-/search",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(search::search_page),
        },
        Spec {
            path: "/{slug}/-/opensearch.xml",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(discover::opensearch),
        },
        Spec {
            path: "/{slug}/-/health",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(health::health_index),
        },
        Spec {
            path: "/{slug}/-/health/{list}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(health::health_list),
        },
        // --- handoff 005 K-T6: what a reader sees of the knowledge layer (RFC 005 part 2 D-7).
        Spec {
            path: "/{slug}/-/claim/{id}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(knowledge::show::claim_page),
        },
        // --- handoff 005 K-T10: a claim's own discussion (RFC 005 part 2 D-11): a comment, by the action a comment on
        // a proposal is made with.
        Spec {
            path: "/{slug}/-/claim/{id}/discuss",
            methods: &["POST"],
            guard: Guard::Action(Action::Comment),
            handler: post_route!(knowledge::thread::comment_post, proposals::SMALL_FORM_LIMIT),
        },
        Spec {
            path: "/{slug}/-/why/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(knowledge::why::why_page),
        },
        Spec {
            path: "/{slug}/-/links/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(health::links_page),
        },
        Spec {
            path: "/{slug}/-/site.json",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(discover::site_description),
        },
        Spec {
            path: "/{slug}/-/describe/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(discover::describe_page),
        },
        Spec {
            path: "/{slug}/-/sitemap.xml",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(discover::sitemap),
        },
        Spec {
            path: "/{slug}/-/raw/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Public {
                reason: "refuses: a page's source is served only on the content origin, so this origin answers every request for it with the generic not-found page and reads nothing",
            },
            handler: || get(raw::raw_refused),
        },
        Spec {
            path: "/{slug}/{*title}",
            methods: &["GET", "HEAD"],
            guard: Guard::Action(Action::Read),
            handler: || get(wiki::view_page),
        },
    ]
}

/// The content origin's entry points: a page's source, byte for byte, and nothing else.
fn content_specs() -> Vec<Spec> {
    vec![Spec {
        path: "/{slug}/-/raw/{*title}",
        methods: &["GET", "HEAD"],
        guard: Guard::Action(Action::Read),
        handler: || get(raw::raw_content),
    }]
}

/// The content origin's declared entry points.
pub fn content_table() -> Result<RouteTable, RouteError> {
    let mut table = RouteTable::new();
    for spec in content_specs() {
        table.register(spec.path, spec.methods, spec.guard)?;
    }
    Ok(table)
}

/// The application origin's declared entry points.
pub fn application_table() -> Result<RouteTable, RouteError> {
    let mut table = RouteTable::new();
    for spec in application_specs() {
        table.register(spec.path, spec.methods, spec.guard)?;
    }
    Ok(table)
}

/// The application origin's router, built from its table, serving the wikis `services` reaches (none: every
/// wiki address is then the generic not-found page). Anything not declared is a 404 page in the visitor's
/// language that carries the same headers as every other response.
pub(crate) fn application_with(
    origins: Origins,
    services: Option<Arc<Services>>,
) -> Result<Router, RouteError> {
    application_table()?;
    let https = origins.app_origin().scheme() == Scheme::Https;
    let state = PageState {
        https,
        services,
        origins: Arc::new(origins),
    };
    let mut router = Router::new();
    for spec in application_specs() {
        router = router.route(spec.path, (spec.handler)());
    }
    Ok(router
        .fallback(api::fallback_not_found)
        .method_not_allowed_fallback(api::fallback_method_not_allowed)
        .with_state(state.clone())
        // Inside the headers: a refusal of a write carries them too. Before every handler: the session's CSRF
        // token is checked before any wiki is looked up (handoff 004 P6).
        .layer(from_fn_with_state(state, csrf::layer))
        .layer(from_fn(move |req, next| {
            headers::layer(Surface::Application { https }, req, next)
        })))
}

/// The content origin's router, built from its table. It serves wiki-authored bytes as inert files and nothing
/// else; everything it does not serve is a plain-text 404 with the content origin's headers (its policy allows
/// no stylesheet and no form, so it answers no page).
pub(crate) fn content_with(
    origins: Origins,
    services: Option<Arc<Services>>,
) -> Result<Router, RouteError> {
    content_table()?;
    let https = origins.content_origin().scheme() == Scheme::Https;
    let mut router = Router::new();
    for spec in content_specs() {
        router = router.route(spec.path, (spec.handler)());
    }
    Ok(router
        .fallback(pages::plain_not_found)
        .with_state(PageState {
            https,
            services,
            origins: Arc::new(origins),
        })
        .layer(from_fn(move |req, next| {
            headers::layer(Surface::Content { https }, req, next)
        })))
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
    let origins = Origins::new(config.app_origin.clone(), config.content_origin.clone());
    let dispatch = Dispatch::new(
        application_with(origins.clone(), services.clone())?,
        content_with(origins, services)?,
        config.app_origin.clone(),
        config.content_origin.clone(),
    );
    Ok(Router::new().fallback_service(dispatch))
}

#[cfg(test)]
mod tests;
