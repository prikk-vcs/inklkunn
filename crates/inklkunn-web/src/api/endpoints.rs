//! The handlers of the read API and the list of them the route registry reads. Each handler is a few lines:
//! [`serve`] does the shared start (language, existence-neutral authorization, the query checked against the
//! endpoint's [`Doc`]) and the shared finish (`ETag`, `304`, problem details, the `Link` to the description);
//! the closure given to it is the endpoint's own work.
//!
//! `pages` (list), `pages/{title}`, `raw/{title}`, `fragment/{title}` and `openapi.json` are here;
//! `history`, `revisions` and `diff` are in [`past`], `backlinks`, `health`, `search` and `changes` in
//! [`across`]. Every one calls the read service's own reads, the ones the web pages call, and shapes the result
//! (`shape.rs`).

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use axum::routing::get;
use inklkunn_core::read::{Cursor, DEFAULT_PAGE_SIZE, PageFilter, PageOutcome, PageView};
use inklkunn_core::{Action, Guard};
use inklkunn_store::{IndexEntry, WikiRecord};
use serde_json::{Value, json};

use super::describe::{self, Doc};
use super::problem::{Code, Problem};
use super::reply::{self, Reply};
use super::request::{Params, from_api, from_read, language, serve};
use super::{Endpoint, OPENAPI_MEDIA_TYPE, openapi, shape};
use crate::pages::PageState;
use crate::present::WikiLinks;
use crate::services::Services;
use crate::url::view_ref;

mod across;
mod past;

/// The addresses under the API's prefixes that are *not* endpoints: they answer the generic not-found problem,
/// so a mistyped endpoint is never taken for a page title. The OpenAPI document does not list them.
pub(crate) const NOT_ENDPOINTS: [&str; 2] = ["/{slug}/-/api/{*rest}", "/-/api/{*rest}"];

/// Every endpoint, in documentation order.
pub(super) fn all() -> Vec<Endpoint> {
    let read = Guard::Action(Action::Read);
    let documented = |doc: &'static Doc, guard, handler| Endpoint {
        path: doc.path,
        doc: Some(doc),
        guard,
        handler,
    };
    let unlisted = |path, handler| Endpoint {
        path,
        doc: None,
        guard: Guard::Public {
            reason: "an address under the API that is not an endpoint: it reads nothing and answers every caller the same not-found problem",
        },
        handler,
    };
    vec![
        documented(
            &describe::OPENAPI,
            Guard::Public {
                reason: "the description of the API: generated from the route table, the same for everyone, and it serves no wiki data",
            },
            || get(description),
        ),
        documented(&describe::PAGES, read, || get(list_pages)),
        documented(&describe::PAGE, read, || get(get_page)),
        documented(&describe::RAW, read, || get(get_raw)),
        documented(&describe::FRAGMENT, read, || get(get_fragment)),
        documented(&describe::HISTORY, read, || get(past::history)),
        documented(&describe::REVISION, read, || get(past::revision)),
        documented(&describe::DIFF, read, || get(past::diff)),
        documented(&describe::BACKLINKS, read, || get(across::backlinks)),
        documented(&describe::HEALTH, read, || get(across::health)),
        documented(&describe::SEARCH, read, || get(across::search)),
        documented(&describe::CHANGES, read, || get(across::changes)),
        unlisted(NOT_ENDPOINTS[0], || get(unknown)),
        unlisted(NOT_ENDPOINTS[1], || get(unknown)),
    ]
}

// ------------------------------------------------------------------------------------ small helpers

/// The view a request names: its short name (for the page links) and the repository name (for the read).
pub(super) struct View {
    pub(super) name: Option<String>,
    pub(super) full: Option<String>,
}

pub(super) fn view_of(params: &Params) -> Result<View, Problem> {
    match params.text("view") {
        None => Ok(View {
            name: None,
            full: None,
        }),
        Some(name) => match view_ref(name) {
            Some(full) => Ok(View {
                name: Some(name.to_owned()),
                full: Some(full),
            }),
            None => Err(Problem::new(Code::NoSuchView).named("view", name)),
        },
    }
}

pub(super) fn short(view: &str) -> &str {
    view.strip_prefix("heads/").unwrap_or(view)
}

pub(super) fn tip_json(tip: &inklkunn_core::read::Tip) -> Value {
    json!({
        "view": short(&tip.view),
        // A page served from what the server last had is not pinned to a block.
        "block_id": (!tip.block_id.is_empty()).then_some(&tip.block_id),
    })
}

/// The cursor a request sends back, if it sent one and it is one this server gave out.
pub(super) fn cursor_of(params: &Params) -> Result<Option<Cursor>, Problem> {
    match params.text("cursor") {
        Some(text) => Cursor::parse(text)
            .map(Some)
            .ok_or_else(|| Problem::new(Code::InvalidCursor)),
        None => Ok(None),
    }
}

/// The size of a page of a list a request asks for (already checked to be within the bounds), else the default.
pub(super) fn limit_of(params: &Params) -> usize {
    params
        .int("limit")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
}

/// Several pages match a title and none is nearer: the problem that lists them.
pub(super) fn ambiguous(candidates: &[IndexEntry]) -> Problem {
    Problem::new(Code::AmbiguousTitle).member(
        "candidates",
        Value::Array(candidates.iter().map(|c| json!(c.title)).collect()),
    )
}

/// The page a title leads to, or why not; a page the server could not ask for is the last one it has, labelled
/// (the same serve-stale rule as the page view, OP-03).
fn page_of(
    services: &Services,
    wiki: &WikiRecord,
    ctx: &super::request::Ctx,
    view: &View,
) -> Result<PageView, Problem> {
    let title = ctx.title()?;
    let links = WikiLinks::new(ctx.t, wiki.slug.as_str(), view.name.as_deref());
    match services
        .read
        .page(wiki, view.full.as_deref(), &title, &links)
    {
        Ok(PageOutcome::Found(found)) => Ok(*found),
        Ok(PageOutcome::Missing { .. }) => Err(Problem::new(Code::PageNotFound)),
        Ok(PageOutcome::Ambiguous { candidates }) => Err(ambiguous(&candidates)),
        Ok(PageOutcome::BrokenRedirect { .. }) => Err(Problem::new(Code::RedirectBroken)),
        Err(inklkunn_core::ReadError::CouldNotBeReadNow(cause)) => {
            (services.log)(&format!(
                "the API could not read a page of wiki `{}` now: {cause}",
                wiki.slug
            ));
            services
                .read
                .stale_page(wiki, view.full.as_deref(), &title, &links)
                .ok_or_else(|| Problem::new(Code::Unavailable))
        }
        Err(error) => Err(from_read(services, wiki, error)),
    }
}

// ------------------------------------------------------------------------------------ the real endpoints

async fn description(headers: HeaderMap) -> Response {
    let outcome = openapi::served().map(|bytes| Reply::bytes(OPENAPI_MEDIA_TYPE, bytes.to_vec()));
    reply::finish(
        language(&headers),
        reply::if_none_match(&headers).as_deref(),
        outcome,
    )
}

async fn list_pages(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::PAGES,
        |services, wiki, params, _| {
            let view = view_of(params)?;
            let filter = PageFilter {
                namespace: params.text("namespace").map(str::to_owned),
                kind: params.text("kind").map(str::to_owned),
                tag: params.text("tag").map(str::to_owned),
                changed_since: params.int("changed_since"),
            };
            let after = cursor_of(params)?;
            let list = services
                .read
                .list_pages(
                    wiki,
                    view.full.as_deref(),
                    &filter,
                    after.as_ref(),
                    limit_of(params),
                )
                .map_err(|e| from_api(services, wiki, e))?;
            Reply::json(&json!({
                "tip": tip_json(&list.tip),
                "items": list.items.iter().map(|item| json!({
                    "title": item.title,
                    "kind": item.kind,
                    "id": item.id,
                    "aliases": item.aliases,
                    "problem": item.problem,
                })).collect::<Vec<_>>(),
                "next_cursor": list.next.as_ref().map(Cursor::as_str),
            }))
        },
    )
    .await
}

async fn get_page(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::PAGE,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let page = page_of(services, wiki, ctx, &view)?;
            Reply::json(&shape::page(wiki, &page))
        },
    )
    .await
}

async fn get_raw(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::RAW,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let title = ctx.title()?;
            let info = services
                .read
                .raw_info(wiki, view.full.as_deref(), &title)
                .map_err(|e| from_api(services, wiki, e))?;
            // A title that leads to no single page: say which of the two it is, as the page view does.
            let Some(info) = info else {
                return Err(match page_of(services, wiki, ctx, &view) {
                    Err(problem) => problem,
                    Ok(_) => Problem::new(Code::PageNotFound),
                });
            };
            Reply::json(&json!({
                "title": info.title,
                "kind": info.kind,
                "storage_path": info.storage_path,
                "tip": tip_json(&info.tip),
            }))
        },
    )
    .await
}

async fn get_fragment(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::FRAGMENT,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let page = page_of(services, wiki, ctx, &view)?;
            Ok(Reply::html(page.html.as_str()))
        },
    )
    .await
}

async fn unknown(headers: HeaderMap) -> Response {
    super::not_found(&headers)
}
