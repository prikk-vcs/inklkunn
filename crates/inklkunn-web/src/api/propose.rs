//! The write API's proposals (RFC 004 D-1, D-2, D-13; handoff 004 T7): open one (a draft, or sent for review at
//! once), read one, save a draft, send a draft, add a revision, take one back. Each is a few lines: the shared
//! start has authenticated, authorized and bounded the request ([`super::write`]); this checks the base, makes
//! the pages whole ([`pages`]) and calls the proposal service, which authorizes again and holds the quotas.
//!
//! The service is the one write path: nothing here records a thing itself, and a rule the form has (the sizes
//! before the parser, the keys the service owns, a title that is taken) is applied in [`pages`] to a body as it
//! is to a form.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_store::{Base, ProposalId};

use super::describe;
use super::problem::{Code, Problem};
use super::reply::Reply;
use super::request::serve;
use super::write::base::bad_if_match;
use super::write::body::Fields;
use super::write::{Request, serve_write};
use crate::pages::PageState;
use crate::url::{decode, view_ref};

pub(crate) mod answer;
mod files;

impl Request {
    /// The proposal the address names (every endpoint that calls this has one).
    pub(crate) fn proposal(&self) -> Result<ProposalId, Problem> {
        self.id.clone().ok_or_else(|| Problem::new(Code::Failed))
    }
}

/// `POST /{slug}/-/api/v1/proposals`.
pub(crate) async fn create(
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
        &describe::CREATE_PROPOSAL,
        opened,
    )
    .await
}

/// Is `text` the state id of a view: 64 lower-case hexadecimal digits?
fn is_state_id(text: &str) -> bool {
    Base::new(text, text).is_some()
}

fn opened(req: &Request, mut fields: Fields) -> Result<Reply, Problem> {
    let view = fields.opt_text("view")?;
    let submit = fields.flag("submit")?;
    let sent = files::read(&mut fields)?;
    fields.end()?;

    let full = match view.as_deref() {
        None => None,
        Some(name) => {
            Some(view_ref(name).ok_or_else(|| Problem::new(Code::NoSuchView).named("view", name))?)
        }
    };
    // The base: the tip the caller wrote against, if it is still the tip.
    if !is_state_id(&req.if_match) {
        return Err(bad_if_match());
    }
    let tip = req
        .services
        .proposals
        .capture_base(&req.wiki, full.as_deref())
        .map_err(|error| req.refusal(error))?;
    if tip.state_id != req.if_match {
        return Err(Problem::new(Code::StaleBase)
            .member(
                "current",
                serde_json::json!(format!("\"{}\"", tip.state_id)),
            )
            .member("current_state_id", serde_json::json!(tip.state_id)));
    }

    let mut edit = files::prepare(req, sent, &tip, !submit)?;
    if submit {
        // Checked before anything is made, so a refused submission leaves no draft behind.
        if edit.summary.trim().is_empty() {
            return Err(req.unacceptable(inklkunn_core::proposal::Invalid::SummaryMissing));
        }
        if edit.files.is_empty() && edit.renames.is_empty() {
            return Err(req.unacceptable(inklkunn_core::proposal::Invalid::NothingProposed));
        }
    }
    edit.base = Some(tip);
    let service = &req.services.proposals;
    let draft = service
        .create(&req.principal, &req.wiki, full.as_deref(), edit)
        .map_err(|error| req.refusal(error))?;
    let (record, revision) = if submit {
        // The draft was made; if it cannot be sent, say so and name it, so the caller can go on with it.
        let sent = service
            .submit(&req.principal, &req.wiki, &draft.id)
            .map_err(|error| {
                req.refusal(error)
                    .member("draft", serde_json::json!(draft.id.to_string()))
            })?;
        (sent, 1)
    } else {
        (draft, 0)
    };
    Ok(answer::reply(&record, revision, req.account())?.created(req.proposal_path(&record.id)))
}

/// `GET /{slug}/-/api/v1/proposals/{id}`.
pub(crate) async fn get(State(state): State<PageState>, headers: HeaderMap, uri: Uri) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::GET_PROPOSAL,
        |services, wiki, _, ctx| {
            let id = decode(&ctx.rest)
                .and_then(|raw| ProposalId::parse(&raw))
                .ok_or_else(|| Problem::new(Code::NotFound))?;
            let service = &services.proposals;
            let looked = service.get(&ctx.principal, wiki, &id).and_then(|record| {
                let revisions = service.revisions(&ctx.principal, wiki, &id)?;
                Ok((record, revisions.last().map_or(0, |r| r.seq)))
            });
            match looked {
                Ok((record, revision)) => {
                    answer::reply(&record, revision, ctx.principal.account_id())
                }
                // A proposal that is absent and one that may not be seen are the same answer.
                Err(
                    inklkunn_core::proposal::ProposalError::NotFound
                    | inklkunn_core::proposal::ProposalError::NotAllowed,
                ) => Err(Problem::new(Code::NotFound)),
                Err(error) => {
                    (services.log)(&format!(
                        "wiki `{}`: a proposal could not be read: {error}",
                        wiki.slug
                    ));
                    Err(Problem::new(Code::Unavailable))
                }
            }
        },
    )
    .await
}

/// `POST /{slug}/-/api/v1/proposals/{id}/draft`.
pub(crate) async fn save_draft(
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
        &describe::SAVE_DRAFT,
        |req, mut fields| {
            let id = req.proposal()?;
            req.based_on(&id)?;
            let sent = files::read(&mut fields)?;
            fields.end()?;
            let held = req
                .services
                .proposals
                .draft(&req.principal, &req.wiki, &id)
                .map_err(|error| req.refusal(error))?;
            let edit = files::prepare(req, sent, &held.base, true)?;
            let record = req
                .services
                .proposals
                .save_draft(&req.principal, &req.wiki, &id, edit)
                .map_err(|error| req.refusal(error))?;
            answer::reply(&record, 0, req.account())
        },
    )
    .await
}

/// `POST /{slug}/-/api/v1/proposals/{id}/submission`.
pub(crate) async fn submit_draft(
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
        &describe::SUBMIT_DRAFT,
        |req, fields| {
            fields.end()?;
            let id = req.proposal()?;
            req.based_on(&id)?;
            let record = req
                .services
                .proposals
                .submit(&req.principal, &req.wiki, &id)
                .map_err(|error| req.refusal(error))?;
            answer::reply(&record, 1, req.account())
        },
    )
    .await
}

/// `POST /{slug}/-/api/v1/proposals/{id}/revisions`.
pub(crate) async fn revise(
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
        &describe::REVISE,
        |req, mut fields| {
            let id = req.proposal()?;
            let latest = req.based_on(&id)?;
            let sent = files::read(&mut fields)?;
            fields.end()?;
            let service = &req.services.proposals;
            let base = service
                .revisions(&req.principal, &req.wiki, &id)
                .map_err(|error| req.refusal(error))?
                .pop()
                .map(|revision| revision.content.base)
                // A draft has no revision to revise: the state machine says so, in its own words.
                .ok_or_else(|| {
                    req.refusal(inklkunn_core::proposal::ProposalError::NotInThisState {
                        op: inklkunn_store::ProposalOp::Revise,
                        state: inklkunn_store::ProposalState::Draft,
                    })
                })?;
            let edit = files::prepare(req, sent, &base, false)?;
            let record = service
                .revise(&req.principal, &req.wiki, &id, edit)
                .map_err(|error| req.refusal(error))?;
            Ok(answer::reply(&record, latest + 1, req.account())?
                .created(req.proposal_path(&record.id)))
        },
    )
    .await
}

/// `POST /{slug}/-/api/v1/proposals/{id}/withdrawals`.
pub(crate) async fn withdraw(
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
        &describe::WITHDRAW,
        |req, mut fields| {
            let reason = fields.text("reason")?;
            fields.end()?;
            let id = req.proposal()?;
            let revision = req.based_on(&id)?;
            let record = req
                .services
                .proposals
                .withdraw(&req.principal, &req.wiki, &id, &reason)
                .map_err(|error| req.refusal(error))?;
            answer::reply(&record, revision, req.account())
        },
    )
    .await
}
