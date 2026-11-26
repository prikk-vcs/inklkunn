//! The reads of the proposals (RFC 007 D-9; handoff 007 I-T5): a wiki's proposals, and a proposal's comments.
//!
//! **They show what the same person's pages show**, by reading what the pages read:
//! * the start is every endpoint's ([`serve`]): a wiki that is absent and one the caller may not read are one `not-found`,
//!   before anything else is asked;
//! * a proposal is read through the proposal service's own reads (`list_sent`, `get`, `revisions`, `hidden_of`, `comments_page`), the ones the proposal's page and the list call, so the rule of whom a proposal is shown to
//!   (`authorize()`: *read*; a draft to its author alone) is theirs and none is written here a second time;
//! * **no draft is listed, not even the caller's own** (the list holds what was sent for review), and a comment of a draft is
//!   its author's alone, as the proposal is;
//! * what moderation hides is told as the proposal's page tells it, from the same marks: a hidden summary is empty and the
//!   proposal carries its tombstone, a hidden comment has no body and carries its.
//!
//! A cursor is a place of its own kind (a proposal's first sending and its id; a comment's id) and is refused on any other
//! list, as every list's is.

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::proposal::ProposalError;
use inklkunn_store::{CommentId, ProposalId, ProposalState, WikiRecord};
use serde_json::{Map, Value, json};

use crate::api::decide::comment_json;
use crate::api::describe;
use crate::api::problem::{Code, Problem};
use crate::api::propose::answer;
use crate::api::reply::Reply;
use crate::api::request::serve;
use crate::api::shape;
use crate::pages::PageState;
use crate::services::Services;
use crate::url::decode;

use super::limit_of;

/// The tag of a cursor of the list of proposals: the moment the last proposal of the page was first sent, and its id.
const SENT_TAG: &str = "p1";

/// The tag of a cursor of the list of a proposal's comments: the id of the last comment of the page.
const COMMENT_TAG: &str = "m1";

/// A cursor of the list of proposals, as the text a caller sends back.
fn sent_cursor(at: i64, id: &ProposalId) -> String {
    format!("{SENT_TAG}{at}.{id}")
}

/// What a cursor of the list of proposals says, if it is one this server gave out: the whole number in its one spelling, a
/// point, and an id in its one spelling. Nothing else is a cursor.
fn parse_sent_cursor(text: &str) -> Option<(i64, ProposalId)> {
    let (at, id) = text.strip_prefix(SENT_TAG)?.split_once('.')?;
    let parsed: i64 = at.parse().ok()?;
    (parsed >= 0 && parsed.to_string() == at).then_some(())?;
    Some((parsed, ProposalId::parse(id)?))
}

/// The problem a refused read of a proposal comes to: a proposal that is absent and one that may not be seen are the same
/// answer; anything else is the server's, logged and never shown (as `getProposal` says it).
fn refused(services: &Services, wiki: &WikiRecord, error: &ProposalError) -> Problem {
    match error {
        ProposalError::NotFound | ProposalError::NotAllowed => Problem::new(Code::NotFound),
        other => {
            (services.log)(&format!(
                "wiki `{}`: a proposal could not be read: {other}",
                wiki.slug
            ));
            Problem::new(Code::Unavailable)
        }
    }
}

/// `GET /{slug}/-/api/v1/proposals`.
pub(crate) async fn list(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::LIST_PROPOSALS,
        |services, wiki, params, ctx| {
            let only = params.text("state").and_then(ProposalState::parse);
            let after = match params.text("cursor") {
                None => None,
                Some(text) => {
                    Some(parse_sent_cursor(text).ok_or_else(|| Problem::new(Code::InvalidCursor))?)
                }
            };
            let limit = limit_of(params);
            let service = &services.proposals;
            // One more than a page, to know whether the list goes on.
            let mut found = service
                .list_sent(&ctx.principal, wiki, only, after.as_ref(), limit + 1)
                .map_err(|error| refused(services, wiki, &error))?;
            let more = found.len() > limit;
            found.truncate(limit);
            let mut items = Vec::with_capacity(found.len());
            for record in &found {
                let id = &record.id;
                let latest = service
                    .revisions(&ctx.principal, wiki, id)
                    .map(|revisions| revisions.last().map_or(0, |r| r.seq))
                    .map_err(|error| refused(services, wiki, &error))?;
                let hidden = service
                    .hidden_of(&ctx.principal, wiki, id)
                    .map_err(|error| refused(services, wiki, &error))?;
                items.push(answer::proposal(
                    record,
                    latest,
                    ctx.principal.account_id(),
                    &hidden,
                ));
            }
            let next = found
                .last()
                .filter(|_| more)
                .and_then(|last| Some(sent_cursor(last.submitted_at?, &last.id)));
            Reply::json(&json!({"items": items, "next_cursor": next}))
        },
    )
    .await
}

/// `GET /{slug}/-/api/v1/proposals/{id}/comments`.
pub(crate) async fn comments(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::LIST_PROPOSAL_COMMENTS,
        |services, wiki, params, ctx| {
            let id = ctx
                .rest
                .strip_suffix("/comments")
                .and_then(decode)
                .and_then(|raw| ProposalId::parse(&raw))
                .ok_or_else(|| Problem::new(Code::NotFound))?;
            let after = match params.text("cursor") {
                None => None,
                Some(text) => Some(
                    text.strip_prefix(COMMENT_TAG)
                        .and_then(CommentId::parse)
                        .ok_or_else(|| Problem::new(Code::InvalidCursor))?,
                ),
            };
            let limit = limit_of(params);
            let service = &services.proposals;
            // The window and its marks are what the proposal's page draws the thread from (a hidden comment's body is already
            // gone, and the marks say why), read for the window's comments alone; one more than a page, to know whether the
            // thread goes on.
            let found = service
                .comments_page(&ctx.principal, wiki, &id, after.as_ref(), limit + 1)
                .map_err(|error| refused(services, wiki, &error))?
                // A place that is no comment of this thread is not a cursor of this list (comments are never removed).
                .ok_or_else(|| Problem::new(Code::InvalidCursor))?;
            let marks = found.marks;
            let mut page = found.comments;
            let more = page.len() > limit;
            page.truncate(limit);
            let items: Vec<Value> = page
                .iter()
                .map(|comment| {
                    let mut item = comment_json(comment);
                    if let (Value::Object(members), Some(hidden)) =
                        (&mut item, marks.get(&comment.id))
                    {
                        shape::tombstones(members, hidden);
                    }
                    item
                })
                .collect();
            let next = page
                .last()
                .filter(|_| more)
                .map(|last| format!("{COMMENT_TAG}{}", last.id));
            let mut body = Map::new();
            body.insert("items".to_owned(), Value::Array(items));
            body.insert("next_cursor".to_owned(), json!(next));
            Reply::json(&Value::Object(body))
        },
    )
    .await
}
