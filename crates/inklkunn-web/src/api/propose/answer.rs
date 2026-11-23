//! A proposal as the write API tells it (and the one read that goes with it): its state in the stable word,
//! its text, the latest revision, and the `etag` every write on it names in `If-Match`.

use inklkunn_core::Principal;
use inklkunn_core::tombstone::{Hidden, ProposalHidden};
use inklkunn_store::{AccountId, ProposalRecord, WikiRecord};
use serde_json::{Value, json};

use crate::api::problem::Problem;
use crate::api::reply::Reply;
use crate::api::write::base::ProposalTag;
use crate::services::Services;

/// The proposal as JSON. `viewer` is the account that asks: `yours` says whether it is the proposer, which is
/// all this says of the person (the people of a change are named by the pages' own words, not here).
pub(crate) fn proposal(
    record: &ProposalRecord,
    revision: u32,
    viewer: Option<&AccountId>,
    hidden: &ProposalHidden,
) -> Value {
    let tag = ProposalTag {
        id: record.id.clone(),
        revision,
    };
    let mut body = json!({
        "id": record.id.to_string(),
        "state": record.state.as_str(),
        "view": record.view.strip_prefix("heads/").unwrap_or(&record.view),
        "summary": record.summary,
        "sources": record.sources,
        "revision": revision,
        "etag": tag.render(),
        "yours": viewer == Some(&record.author),
        "superseded_by": record.superseded_by.as_ref().map(ToString::to_string),
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "submitted_at": record.submitted_at,
        "closed_at": record.closed_at,
    });
    // What moderation hides of the proposal is told in place of it (RFC 006 D-8): the summary is already empty, and the
    // members say which part, why, which role and when. The author is named nowhere in this member.
    if let Value::Object(members) = &mut body {
        crate::api::shape::tombstones(
            members,
            &Hidden::from_marks(hidden.summary.iter().chain(hidden.author.iter()).cloned()),
        );
    }
    body
}

/// The reply that carries a proposal: its `ETag` is the revision's tag, the base of the next write.
pub(crate) fn reply(
    record: &ProposalRecord,
    revision: u32,
    viewer: Option<&AccountId>,
    hidden: &ProposalHidden,
) -> Result<Reply, Problem> {
    let tag = ProposalTag {
        id: record.id.clone(),
        revision,
    };
    Ok(Reply::json(&proposal(record, revision, viewer, hidden))?.with_etag(tag.render()))
}

/// [`reply`] for a caller who is `principal` in `wiki`: what moderation hides of the proposal is asked of the service here, so
/// that no member that carries a proposal is made without asking.
pub(crate) fn reply_for(
    services: &Services,
    principal: &Principal,
    wiki: &WikiRecord,
    record: &ProposalRecord,
    revision: u32,
) -> Result<Reply, Problem> {
    let hidden = services
        .proposals
        .hidden_of(principal, wiki, &record.id)
        .map_err(|e| {
            (services.log)(&format!(
                "wiki `{}`: what is hidden of a proposal could not be read: {e}",
                wiki.slug
            ));
            Problem::new(crate::api::problem::Code::Unavailable)
        })?;
    reply(record, revision, principal.account_id(), &hidden)
}
