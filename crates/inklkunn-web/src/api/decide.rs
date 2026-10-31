//! The write API's review (RFC 004 D-3, D-4, D-8, D-13; handoff 004 T7): a comment, a decision bound to the
//! revision reviewed, an acceptance, a decline. Each names the proposal's revision in `If-Match` (the shared
//! start has checked the action — where a token's scope and the account's role are intersected — before this
//! runs; the service and the acceptance authorize again, on the proposal itself).
//!
//! Accepting is the acceptance of RFC 004 D-4 and nothing else: the same steps under the wiki's write lease, the
//! same typed outcomes, here told as problem details.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::acceptance::Accepted;
use inklkunn_core::proposal::{Invalid, NewComment, ProposalError};
use inklkunn_store::{CommentId, CommentRecord, DecisionKind, DecisionRecord, ProposalId};
use serde_json::{Value, json};

use super::describe;
use super::problem::{Code, Problem};
use super::propose::answer;
use super::reply::Reply;
use super::write::base::ProposalTag;
use super::write::{Request, serve_write};
use crate::pages::PageState;
use crate::proposals::anchor;

/// The name of the member that holds the page an anchor is on.
const ANCHOR_PAGE: &str = "page";

fn tag(id: &ProposalId, revision: u32) -> String {
    ProposalTag {
        id: id.clone(),
        revision,
    }
    .render()
}

/// A comment as JSON.
fn comment_json(comment: &CommentRecord) -> Value {
    json!({
        "id": comment.id.to_string(),
        "revision": comment.revision,
        "parent": comment.parent.as_ref().map(ToString::to_string),
        "body": comment.body,
        "anchor": comment.anchor.as_ref().map(|a| json!({
            "page": a.page,
            "ordinal": a.ordinal,
            "block_id": a.block_id,
        })),
        "created_at": comment.created_at,
    })
}

/// A decision as JSON.
fn decision_json(decision: &DecisionRecord) -> Value {
    json!({
        "id": decision.id.to_string(),
        "revision": decision.revision,
        "decision": decision.kind.as_str(),
        "body": decision.body,
        "created_at": decision.created_at,
    })
}

/// `POST /{slug}/-/api/v1/proposals/{id}/comments`.
pub(crate) async fn comment(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
) -> Response {
    serve_write(
        state,
        headers,
        uri,
        body,
        &describe::COMMENT,
        |req, mut fields| {
            let text = fields.text("body")?;
            let parent = fields.opt_text("parent")?;
            let anchor_fields = fields.object("anchor")?;
            fields.end()?;
            let id = req.proposal()?;
            req.based_on(&id)?;
            let parent = parent
                .map(|p| CommentId::parse(&p).ok_or_else(|| req.unacceptable(Invalid::BadAnchor)))
                .transpose()?;
            let anchor = match anchor_fields {
                None => None,
                Some(mut a) => {
                    let page = a.text(ANCHOR_PAGE)?;
                    let ordinal = a.count("ordinal")?;
                    a.end()?;
                    Some(anchor_of(req, &id, &page, ordinal)?)
                }
            };
            let comment = req
                .services
                .proposals
                .comment(
                    &req.principal,
                    &req.wiki,
                    &id,
                    NewComment {
                        body: text,
                        parent,
                        anchor,
                    },
                )
                .map_err(|error| req.refusal(error))?;
            Ok(Reply::json(&comment_json(&comment))?
                .with_etag(tag(&id, comment.revision))
                .created(req.proposal_path(&id)))
        },
    )
    .await
}

/// The anchor of the `ordinal`-th part of `page` in the proposal's latest revision, built from that
/// revision's own text and never from the body: the body names the page and the position only.
fn anchor_of(
    req: &Request,
    id: &ProposalId,
    page: &str,
    ordinal: u32,
) -> Result<inklkunn_store::Anchor, Problem> {
    let latest = req
        .services
        .proposals
        .revisions(&req.principal, &req.wiki, id)
        .map_err(|error| req.refusal(error))?
        .pop()
        .ok_or_else(|| req.refusal(ProposalError::Changed))?;
    let bad = || req.unacceptable(Invalid::BadAnchor);
    let text = latest
        .content
        .files
        .iter()
        .find(|f| f.path == page)
        .and_then(|f| f.text.as_deref())
        .ok_or_else(bad)?;
    let ordinal = usize::try_from(ordinal).map_err(|_| bad())?;
    anchor::make(page, text, ordinal).ok_or_else(bad)
}

/// `POST /{slug}/-/api/v1/proposals/{id}/decisions`.
pub(crate) async fn decide(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
) -> Response {
    serve_write(
        state,
        headers,
        uri,
        body,
        &describe::DECIDE,
        |req, mut fields| {
            let kind = fields.text("decision")?;
            let text = fields.opt_text("body")?.unwrap_or_default();
            fields.end()?;
            let kind = DecisionKind::parse(&kind)
                .ok_or_else(|| super::write::body::invalid("decision"))?;
            let id = req.proposal()?;
            let revision = req.based_on(&id)?;
            let decision = req
                .services
                .proposals
                .decide(&req.principal, &req.wiki, &id, revision, kind, &text)
                .map_err(|error| req.refusal(error))?;
            Ok(Reply::json(&decision_json(&decision))?
                .with_etag(tag(&id, revision))
                .created(req.proposal_path(&id)))
        },
    )
    .await
}

/// `POST /{slug}/-/api/v1/proposals/{id}/acceptances`.
pub(crate) async fn accept(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
) -> Response {
    serve_write(
        state,
        headers,
        uri,
        body,
        &describe::ACCEPT,
        |req, fields| {
            fields.end()?;
            let id = req.proposal()?;
            let revision = req.based_on(&id)?;
            // Not served by this build: said after the checks an acceptance makes first, so a caller who may
            // not accept hears *forbidden*, not what the build lacks.
            let acceptor = req.services.acceptor.as_ref().ok_or_else(|| {
                Problem::new(Code::NotAvailable).member("feature", json!("accept"))
            })?;
            let accepted = acceptor
                .accept(&req.principal, &req.wiki, &id, revision)
                .map_err(|failure| req.refused_acceptance(failure))?;
            let (signed, acceptance, block_id, patch_id, reauthored) = match accepted {
                Accepted::Signed(s) => (true, s.acceptance, s.block_id, s.patch_id, s.reauthored),
                Accepted::AlreadySigned {
                    acceptance,
                    block_id,
                    patch_id,
                } => (false, acceptance, block_id, patch_id, false),
            };
            let record = req
                .services
                .proposals
                .get(&req.principal, &req.wiki, &id)
                .map_err(|error| req.refusal(error))?;
            let mut body = answer::proposal(&record, revision, req.account());
            if let Some(object) = body.as_object_mut() {
                object.insert(
                    "acceptance".to_owned(),
                    json!({
                        "id": acceptance.to_string(),
                        "outcome": if signed { "signed" } else { "already-signed" },
                        "block_id": block_id,
                        "patch_id": patch_id,
                        "reauthored": reauthored,
                    }),
                );
            }
            Ok(Reply::json(&body)?.with_etag(tag(&id, revision)))
        },
    )
    .await
}

/// `POST /{slug}/-/api/v1/proposals/{id}/declines`.
pub(crate) async fn decline(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
) -> Response {
    serve_write(
        state,
        headers,
        uri,
        body,
        &describe::DECLINE,
        |req, mut fields| {
            let reason = fields.text("reason")?;
            fields.end()?;
            let id = req.proposal()?;
            let revision = req.based_on(&id)?;
            let record = req
                .services
                .proposals
                .decline(&req.principal, &req.wiki, &id, &reason)
                .map_err(|error| req.refusal(error))?;
            answer::reply(&record, revision, req.account())
        },
    )
    .await
}
