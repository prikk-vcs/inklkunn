//! The endpoints about a page's past: its history, one change, and the difference between two changes of a
//! page. Each calls the read service's own reads (the ones the history pages call) and shapes the result
//! (`shape.rs`); the shapes are described in `describe.rs` and `openapi.rs`.

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::read::{
    AtOutcome, ChangeOutcome, Cursor, DiffOutcome, DiffStatus, HistoryAnswer,
};
use inklkunn_markup::path::path_to_title;
use serde_json::{Value, json};

use super::{ambiguous, cursor_of, short, tip_json, view_of};
use crate::api::describe;
use crate::api::problem::{Code, Problem};
use crate::api::reply::Reply;
use crate::api::request::{from_api, from_read, hex_id, serve};
use crate::api::shape;
use crate::pages::PageState;
use crate::present::WikiLinks;

pub(super) async fn history(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::HISTORY,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let title = ctx.title()?;
            let after = cursor_of(params)?;
            let answer = services
                .read
                .api_history(wiki, view.full.as_deref(), &title, after.as_ref())
                .map_err(|e| from_api(services, wiki, e))?;
            let (history, next) = match answer {
                HistoryAnswer::Page { history, next } => (history, next),
                HistoryAnswer::Ambiguous { candidates } => return Err(ambiguous(&candidates)),
            };
            let items: Vec<Value> = history
                .rows
                .iter()
                .map(|row| {
                    let mut item = shape::change_head(ctx.t, &row.change);
                    // The title the page had after this change: before a move it is the old title.
                    item.insert(
                        "page_title".to_owned(),
                        json!(path_to_title(&row.path).unwrap_or_else(|| history.title.clone())),
                    );
                    item.insert(
                        "events".to_owned(),
                        Value::Array(row.events.iter().map(shape::event).collect()),
                    );
                    Value::Object(item)
                })
                .collect();
            Reply::json(&json!({
                "title": history.title,
                "tip": tip_json(&history.tip),
                "items": items,
                "next_cursor": next.as_ref().map(Cursor::as_str),
                "beyond_window": history.beyond_window,
            }))
        },
    )
    .await
}

pub(super) async fn revision(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::REVISION,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let id = hex_id(&ctx.rest, 64, "id")?;
            let detail = match services
                .read
                .change(wiki, view.full.as_deref(), &id)
                .map_err(|e| from_read(services, wiki, e))?
            {
                ChangeOutcome::Found(detail) => detail,
                ChangeOutcome::NoSuchChange => return Err(Problem::new(Code::ChangeNotFound)),
            };
            // The page as it stood after the change, when a title was given: the permalink of that version.
            let page = match params.text("title") {
                None => Value::Null,
                Some(title) => {
                    let links = WikiLinks::new(ctx.t, wiki.slug.as_str(), view.name.as_deref());
                    match services
                        .read
                        .page_at(wiki, view.full.as_deref(), &id, title, &links)
                        .map_err(|e| from_read(services, wiki, e))?
                    {
                        AtOutcome::Found { view: page, .. } => shape::page(wiki, &page),
                        AtOutcome::PageMissing { .. } => {
                            return Err(Problem::new(Code::PageNotFound));
                        }
                        AtOutcome::Ambiguous { candidates } => {
                            return Err(ambiguous(&candidates));
                        }
                        AtOutcome::NoSuchChange => {
                            return Err(Problem::new(Code::ChangeNotFound));
                        }
                    }
                }
            };
            let mut body = shape::change_head(ctx.t, &detail.info);
            body.insert(
                "view".to_owned(),
                json!(short(view.full.as_deref().unwrap_or(&wiki.default_view))),
            );
            body.insert(
                "did".to_owned(),
                Value::Array(detail.did.iter().map(shape::did).collect()),
            );
            body.insert("omitted".to_owned(), json!(detail.omitted));
            body.insert("too_large".to_owned(), json!(detail.too_large));
            body.insert("page".to_owned(), page);
            Reply::json(&Value::Object(body))
        },
    )
    .await
}

const fn status_code(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Unchanged => "unchanged",
        DiffStatus::Added => "added",
        DiffStatus::Removed => "removed",
        DiffStatus::Modified => "modified",
        DiffStatus::Other => "other",
    }
}

pub(super) async fn diff(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::DIFF,
        |services, wiki, params, _| {
            // `path` is required by the parameter's own description, so a diff of nothing in particular is
            // refused before any read could be asked for it.
            let Some(title) = params.text("path") else {
                return Err(Problem::new(Code::DiffPathRequired));
            };
            let view = view_of(params)?;
            let diff = match services
                .read
                .api_diff(
                    wiki,
                    view.full.as_deref(),
                    params.text("from"),
                    params.text("to"),
                    title,
                )
                .map_err(|e| from_api(services, wiki, e))?
            {
                DiffOutcome::Found(diff) => diff,
                DiffOutcome::NoSuchChange => return Err(Problem::new(Code::ChangeNotFound)),
                DiffOutcome::Ambiguous { candidates } => return Err(ambiguous(&candidates)),
            };
            Reply::json(&json!({
                "title": diff.title,
                "from": diff.from,
                "to": diff.to,
                "from_number": diff.from_number,
                "to_number": diff.to_number,
                "status": status_code(diff.status),
                "added": diff.added,
                "removed": diff.removed,
                "truncated": diff.truncated,
                "no_lines": diff.no_lines,
                "hunks": diff.hunks.iter().map(shape::hunk).collect::<Vec<_>>(),
            }))
        },
    )
    .await
}
