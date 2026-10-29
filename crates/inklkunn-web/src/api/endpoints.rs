//! The handlers of the read API and the list of them the route registry reads. Each handler is a few lines:
//! [`serve`] does the shared start (language, existence-neutral authorization, the query checked against the
//! endpoint's [`Doc`]) and the shared finish (`ETag`, `304`, problem details, the `Link` to the description);
//! the closure given to it is the endpoint's own work.
//!
//! **Real now:** `pages` (list), `pages/{title}`, `raw/{title}`, `fragment/{title}`, and `openapi.json`.
//! **Stubs:** `history`, `revisions`, `diff`, `backlinks`, `health`, `search`, `changes` check their input and
//! answer the problem `not-available` until the read service has the read behind them (`inklkunn_core::read`'s
//! `api.rs` is where each stub is filled in).

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use axum::routing::get;
use inklkunn_core::read::{
    ApiError, Cursor, DEFAULT_PAGE_SIZE, HEADING_DEMOTION, PageFilter, PageOutcome, PageView,
    Pending,
};
use inklkunn_core::{Action, Guard};
use inklkunn_store::WikiRecord;
use serde_json::{Value, json};

use super::describe::{self, Doc};
use super::problem::{Code, Problem};
use super::reply::{self, Reply};
use super::request::{Params, from_api, from_read, hex_id, language, serve};
use super::{Endpoint, OPENAPI_MEDIA_TYPE, openapi};
use crate::pages::PageState;
use crate::present::WikiLinks;
use crate::services::Services;
use crate::url::view_ref;

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
        documented(&describe::HISTORY, read, || get(history)),
        documented(&describe::REVISION, read, || get(revision)),
        documented(&describe::DIFF, read, || get(diff)),
        documented(&describe::BACKLINKS, read, || get(backlinks)),
        documented(&describe::HEALTH, read, || get(health)),
        documented(&describe::SEARCH, read, || get(search)),
        documented(&describe::CHANGES, read, || get(changes)),
        unlisted(NOT_ENDPOINTS[0], || get(unknown)),
        unlisted(NOT_ENDPOINTS[1], || get(unknown)),
    ]
}

// ------------------------------------------------------------------------------------ small helpers

/// The view a request names: its short name (for the page links) and the repository name (for the read).
struct View {
    name: Option<String>,
    full: Option<String>,
}

fn view_of(params: &Params) -> Result<View, Problem> {
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

fn short(view: &str) -> &str {
    view.strip_prefix("heads/").unwrap_or(view)
}

fn tip_json(tip: &inklkunn_core::read::Tip) -> Value {
    json!({
        "view": short(&tip.view),
        // A page served from what the server last had is not pinned to a block.
        "block_id": (!tip.block_id.is_empty()).then_some(&tip.block_id),
    })
}

/// A stub's answer: the read is not there yet, so the only thing it can be is the problem.
fn pending(
    read: Result<Pending, ApiError>,
    services: &Services,
    wiki: &WikiRecord,
) -> Result<Reply, Problem> {
    match read {
        Err(error) => Err(from_api(services, wiki, error)),
        Ok(never) => match never {},
    }
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
        Ok(PageOutcome::Ambiguous { candidates }) => Err(Problem::new(Code::AmbiguousTitle)
            .member(
                "candidates",
                Value::Array(candidates.into_iter().map(|c| json!(c.title)).collect()),
            )),
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
            let limit = params
                .int("limit")
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(DEFAULT_PAGE_SIZE);
            let after = match params.text("cursor") {
                Some(text) => {
                    Some(Cursor::parse(text).ok_or_else(|| Problem::new(Code::InvalidCursor))?)
                }
                None => None,
            };
            let list = services
                .read
                .list_pages(wiki, view.full.as_deref(), &filter, after.as_ref(), limit)
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
            let license = page
                .meta
                .license
                .clone()
                .unwrap_or_else(|| wiki.default_license.clone());
            Reply::json(&json!({
                "title": page.meta.title,
                "id": page.meta.id,
                "kind": page.meta.kind,
                "status": page.meta.status,
                "lang": page.meta.lang,
                "license": license,
                "description": page.meta.description,
                "tags": page.meta.tags,
                "aliases": page.meta.aliases,
                "toc": page.toc.iter().map(|h| json!({
                    // The level the heading has in `html`, where the page's own title is the one `h1`.
                    "level": h.level.saturating_add(HEADING_DEMOTION).min(6),
                    "id": h.id,
                    "text": h.text,
                })).collect::<Vec<_>>(),
                "html": page.html.as_str(),
                "redirected_from": page.redirected_from.as_ref().map(|from| json!({
                    "title": from.title,
                    "by": match from.by {
                        inklkunn_core::read::RedirectedBy::Alias => "alias",
                        inklkunn_core::read::RedirectedBy::Redirect => "redirect",
                    },
                })),
                "matched_by": page.via.as_str(),
                "stale": page.stale,
                "tip": tip_json(&page.tip),
            }))
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

// ------------------------------------------------------------------------------------ the stubs

async fn history(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::HISTORY,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let title = ctx.title()?;
            pending(
                services
                    .read
                    .api_history(wiki, view.full.as_deref(), &title),
                services,
                wiki,
            )
        },
    )
    .await
}

async fn revision(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::REVISION,
        |services, wiki, _, ctx| {
            let id = hex_id(&ctx.rest, 64, "id")?;
            pending(services.read.api_revision(wiki, &id), services, wiki)
        },
    )
    .await
}

async fn diff(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::DIFF,
        |services, wiki, params, _| {
            // `path` is required by the parameter's own description, so a diff of nothing in particular is
            // refused before any read could be asked for it.
            let Some(path) = params.text("path") else {
                return Err(Problem::new(Code::DiffPathRequired));
            };
            pending(
                services
                    .read
                    .api_diff(wiki, params.text("from"), params.text("to"), path),
                services,
                wiki,
            )
        },
    )
    .await
}

async fn backlinks(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::BACKLINKS,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let title = ctx.title()?;
            pending(
                services
                    .read
                    .api_backlinks(wiki, view.full.as_deref(), &title),
                services,
                wiki,
            )
        },
    )
    .await
}

async fn health(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::HEALTH,
        |services, wiki, params, _| {
            let view = view_of(params)?;
            pending(
                services.read.api_health(wiki, view.full.as_deref()),
                services,
                wiki,
            )
        },
    )
    .await
}

async fn search(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::SEARCH,
        |services, wiki, params, _| {
            let view = view_of(params)?;
            let query = params.text("q").unwrap_or_default();
            pending(
                services.read.api_search(wiki, view.full.as_deref(), query),
                services,
                wiki,
            )
        },
    )
    .await
}

async fn changes(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::CHANGES,
        |services, wiki, params, _| {
            let view = view_of(params)?;
            pending(
                services.read.api_changes(wiki, view.full.as_deref()),
                services,
                wiki,
            )
        },
    )
    .await
}
