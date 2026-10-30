//! The endpoints that look across a wiki: the pages that link to a page, the link health lists, search and the
//! recent changes. Each calls the read service's own reads (the ones the web pages call), checks nothing the
//! page does not, and shapes the result (`shape.rs`).

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::read::{
    BacklinkAnswer, BacklinkKind, ChangeFilter, Cursor, HealthList, HealthRows, RedirectIssueKind,
    Tri,
};
use serde_json::{Value, json};

use super::{ambiguous, cursor_of, limit_of, short, tip_json, view_of};
use crate::api::describe;
use crate::api::problem::{Code, Problem};
use crate::api::reply::Reply;
use crate::api::request::{from_api, from_read, serve};
use crate::api::shape;
use crate::changes::{people_lines, restrict_to_reader};
use crate::health::Resolve;
use crate::pages::PageState;

pub(super) async fn backlinks(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::BACKLINKS,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let title = ctx.title()?;
            let after = cursor_of(params)?;
            let page = match services
                .read
                .api_backlinks(
                    wiki,
                    view.full.as_deref(),
                    &title,
                    after.as_ref(),
                    limit_of(params),
                )
                .map_err(|e| from_api(services, wiki, e))?
            {
                BacklinkAnswer::Found(page) => page,
                BacklinkAnswer::NotFound => return Err(Problem::new(Code::PageNotFound)),
                BacklinkAnswer::Ambiguous { candidates } => return Err(ambiguous(&candidates)),
            };
            Reply::json(&json!({
                "title": page.title,
                "exists": page.exists,
                "tip": tip_json(&page.tip),
                "items": page.items.iter().map(|link| json!({
                    "title": link.from_title,
                    "kind": match link.kind {
                        BacklinkKind::Link => "link",
                        BacklinkKind::Include => "include",
                        BacklinkKind::Redirect => "redirect",
                    },
                    "context": link.context,
                })).collect::<Vec<_>>(),
                "total": page.total,
                "cut": page.cut,
                "next_cursor": page.next.as_ref().map(Cursor::as_str),
            }))
        },
    )
    .await
}

pub(super) async fn health(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::HEALTH,
        |services, wiki, params, _| {
            let view = view_of(params)?;
            let list = params
                .text("list")
                .and_then(HealthList::from_slug)
                .ok_or_else(|| Problem::new(Code::InvalidParameter).named("parameter", "list"))?;
            let after = cursor_of(params)?;
            let page = services
                .read
                .api_health(
                    wiki,
                    view.full.as_deref(),
                    list,
                    after.as_ref(),
                    limit_of(params),
                )
                .map_err(|e| from_api(services, wiki, e))?;
            // A row: the title, the code of what would resolve it, and what that list adds.
            let item = |title: &str, resolve: Resolve, more: Value| {
                let mut row = json!({"title": title, "action": resolve.code()});
                if let (Value::Object(row), Value::Object(more)) = (&mut row, more) {
                    row.extend(more);
                }
                row
            };
            let items: Vec<Value> = match &page.rows {
                HealthRows::Wanted(rows) => rows
                    .iter()
                    .map(|w| {
                        item(
                            &w.title,
                            Resolve::WantedAdd,
                            json!({"linked_from": w.linked_from}),
                        )
                    })
                    .collect(),
                HealthRows::Orphans(rows) => rows
                    .iter()
                    .map(|p| item(&p.entry.title, Resolve::Orphan, json!({})))
                    .collect(),
                HealthRows::DeadEnds(rows) => rows
                    .iter()
                    .map(|p| item(&p.entry.title, Resolve::DeadEnd, json!({})))
                    .collect(),
                HealthRows::Redirects(rows) => rows
                    .iter()
                    .map(|i| {
                        item(
                            &i.entry.title,
                            Resolve::of_redirect(i.kind.clone()),
                            json!({
                                "target": i.target,
                                "problem": match i.kind {
                                    RedirectIssueKind::Broken => "broken",
                                    RedirectIssueKind::Loop => "loop",
                                    RedirectIssueKind::TooLong => "too-long",
                                    RedirectIssueKind::Double => "double",
                                },
                            }),
                        )
                    })
                    .collect(),
                HealthRows::WantsHelp(rows) => rows
                    .iter()
                    .map(|p| {
                        let mark = match p.status.as_str() {
                            "wip" => "wip",
                            "draft" => "draft",
                            _ => "stub",
                        };
                        item(&p.entry.title, Resolve::Help, json!({"mark": mark}))
                    })
                    .collect(),
                // `last_touched` is when this server first saw the newest change that touched the page, in
                // Unix seconds; `null` when none of the changes looked through touched it.
                HealthRows::Stale(rows) => rows
                    .iter()
                    .map(|p| {
                        item(
                            &p.entry.title,
                            Resolve::Review,
                            json!({"last_touched": p.last_touched}),
                        )
                    })
                    .collect(),
            };
            Reply::json(&json!({
                "list": page.list.slug(),
                "tip": tip_json(&page.tip),
                "items": items,
                "total": page.total,
                "next_cursor": page.next.as_ref().map(Cursor::as_str),
                "scan": page.scan.map(|scan| json!({
                    "looked_through": scan.looked_through,
                    "complete": scan.complete,
                })),
            }))
        },
    )
    .await
}

pub(super) async fn search(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::SEARCH,
        |services, wiki, params, _| {
            let view = view_of(params)?;
            let query = params.text("q").unwrap_or_default();
            let outcome = services
                .read
                .search(wiki, view.full.as_deref(), query)
                .map_err(|e| from_read(services, wiki, e))?;
            let limit = limit_of(params);
            Reply::json(&json!({
                "query": outcome.query,
                "tip": tip_json(&outcome.tip),
                "names_a_page": outcome.names_a_page,
                "truncated": outcome.truncated || outcome.hits.len() > limit,
                "items": outcome.hits.iter().take(limit).map(|hit| json!({
                    "title": hit.entry.title,
                    "kind": match hit.kind {
                        inklkunn_core::read::SearchKind::Title => "title",
                        inklkunn_core::read::SearchKind::Alias => "alias",
                    },
                    "snippet": shape::snippet(&hit.segments),
                })).collect::<Vec<_>>(),
            }))
        },
    )
    .await
}

pub(super) async fn changes(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::CHANGES,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let after = cursor_of(params)?;
            let filter = ChangeFilter {
                view: view.name.clone(),
                namespace: params.text("namespace").map(str::to_owned),
                page: None,
                automated: match params.text("automated") {
                    Some("hide") => Tri::Hide,
                    Some("only") => Tri::Only,
                    _ => Tri::Any,
                },
                minor: match params.text("minor") {
                    Some("hide") => Tri::Hide,
                    Some("only") => Tri::Only,
                    _ => Tri::Any,
                },
            };
            let page = services
                .read
                .api_changes(wiki, &filter, after.as_ref(), limit_of(params))
                .map_err(|e| from_api(services, wiki, e))?;
            // What the caller may not read is left out of the page and counted, as the changes page does.
            let (rows, left_out) = restrict_to_reader(services, &ctx.principal, wiki, page.rows);
            let items: Vec<Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "id": row.block_id,
                        "view": short(&row.view),
                        "summary": row.summary,
                        "provenance": shape::row_provenance(row),
                        "people": people_lines(ctx.t, row),
                        "rollback": row.rollback,
                        "automated": row.automated(),
                        "minor": row.minor,
                        "machine_assisted": row.machine_assisted,
                        "first_seen": (row.first_seen > 0).then_some(row.first_seen),
                        "pages": row.pages.as_ref().map(|pages| pages.iter().map(|p| json!({
                            "title": p.title,
                            "exists": p.exists,
                        })).collect::<Vec<_>>()),
                    })
                })
                .collect();
            Reply::json(&json!({
                "items": items,
                "next_cursor": page.next.as_ref().map(Cursor::as_str),
                "views": page.views,
                "left_out": left_out,
                "pages_missing": page.pages_missing,
                "beyond_window": page.beyond_window,
            }))
        },
    )
    .await
}
