//! A proposal as the write API tells it (and the one read that goes with it): its state in the stable word,
//! its text, the latest revision, and the `etag` every write on it names in `If-Match`.

use inklkunn_store::{AccountId, ProposalRecord};
use serde_json::{Value, json};

use crate::api::problem::Problem;
use crate::api::reply::Reply;
use crate::api::write::base::ProposalTag;

/// The proposal as JSON. `viewer` is the account that asks: `yours` says whether it is the proposer, which is
/// all this says of the person (the people of a change are named by the pages' own words, not here).
pub(crate) fn proposal(
    record: &ProposalRecord,
    revision: u32,
    viewer: Option<&AccountId>,
) -> Value {
    let tag = ProposalTag {
        id: record.id.clone(),
        revision,
    };
    json!({
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
    })
}

/// The reply that carries a proposal: its `ETag` is the revision's tag, the base of the next write.
pub(crate) fn reply(
    record: &ProposalRecord,
    revision: u32,
    viewer: Option<&AccountId>,
) -> Result<Reply, Problem> {
    let tag = ProposalTag {
        id: record.id.clone(),
        revision,
    };
    Ok(Reply::json(&proposal(record, revision, viewer))?.with_etag(tag.render()))
}
