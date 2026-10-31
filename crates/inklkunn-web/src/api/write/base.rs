//! What a write is made against (RFC 004 D-13; API-03): **the base is named, and a stale one is a conflict**.
//!
//! * A write on a proposal names the proposal's revision in `If-Match`: the `ETag` is made from the proposal's id
//!   and the number of its latest revision (`"<id>-r<n>"`; a draft has none yet, so `-r0`). The answer to every
//!   read and write of a proposal carries it.
//! * A write that opens a proposal names the state of the view's tip it was written against, the `state_id` the
//!   read endpoints give: `"<64 lower-case hexadecimal digits>"`.
//!
//! A base that is not the current one is `409 stale-base`, and the problem carries the current value, so the
//! client can read what changed, check its change against it and send again. Nothing is recorded by a write
//! with a stale base. `If-Match: *` and a list of tags are refused: "whatever it is now" is the one thing a
//! write must not say.

use axum::http::{HeaderMap, header};
use inklkunn_store::ProposalId;
use serde_json::json;

use super::Request;
use crate::api::problem::{Code, Problem};

/// The `ETag` of a proposal's revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProposalTag {
    pub(crate) id: ProposalId,
    /// The number of the latest revision; 0 for a draft.
    pub(crate) revision: u32,
}

impl ProposalTag {
    /// The header value, quotes included.
    pub(crate) fn render(&self) -> String {
        format!("\"{}-r{}\"", self.id, self.revision)
    }

    /// The tag inside the quotes of an `If-Match`.
    pub(crate) fn parse(inner: &str) -> Option<ProposalTag> {
        let (id, revision) = inner.rsplit_once("-r")?;
        let tag = ProposalTag {
            id: ProposalId::parse(id)?,
            revision: revision.parse().ok()?,
        };
        // Only the one spelling: `-r01` and `-r+1` name no revision this server wrote.
        (tag.render() == format!("\"{inner}\"")).then_some(tag)
    }
}

/// The problem for an `If-Match` that is there and is not a tag this endpoint takes.
pub(crate) fn bad_if_match() -> Problem {
    Problem::new(Code::InvalidParameter).named("parameter", "If-Match")
}

/// The one strong entity tag of the request's `If-Match`, without its quotes. No header is `base-required`;
/// a list, a weak tag, `*` or anything else that is not one quoted tag is `invalid-parameter`.
pub(crate) fn if_match(headers: &HeaderMap) -> Result<String, Problem> {
    let mut all = headers.get_all(header::IF_MATCH).iter();
    let (Some(first), None) = (all.next(), all.next()) else {
        return Err(if headers.contains_key(header::IF_MATCH) {
            bad_if_match()
        } else {
            Problem::new(Code::BaseRequired)
        });
    };
    let value = first.to_str().map_err(|_| bad_if_match())?.trim();
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .filter(|inner| !inner.is_empty() && !inner.contains(['"', ',', ' ']))
        .map(str::to_owned)
        .ok_or_else(bad_if_match)
}

/// `409 stale-base`, carrying the proposal's current tag.
pub(crate) fn stale(current: &ProposalTag) -> Problem {
    Problem::new(Code::StaleBase)
        .member("current", json!(current.render()))
        .member("current_revision", json!(current.revision))
}

impl Request {
    /// The proposal's latest revision now: 0 while it is a draft. `NotFound` for a proposal the principal may
    /// not see, as the service answers it.
    pub(crate) fn current_revision(
        &self,
        id: &ProposalId,
    ) -> Result<u32, inklkunn_core::proposal::ProposalError> {
        let revisions = self
            .services
            .proposals
            .revisions(&self.principal, &self.wiki, id)?;
        Ok(revisions.last().map_or(0, |r| r.seq))
    }

    /// The proposal's tag now.
    pub(crate) fn current_tag(&self, id: &ProposalId) -> Result<ProposalTag, Problem> {
        match self.current_revision(id) {
            Ok(revision) => Ok(ProposalTag {
                id: id.clone(),
                revision,
            }),
            Err(error) => Err(self.refusal(error)),
        }
    }

    /// The revision a write on a proposal was made against: the one `If-Match` names, if it is the proposal's
    /// latest. A tag of another proposal is `invalid-parameter`; an older revision is `stale-base`.
    pub(crate) fn based_on(&self, id: &ProposalId) -> Result<u32, Problem> {
        let named = ProposalTag::parse(&self.if_match)
            .filter(|tag| tag.id == *id)
            .ok_or_else(bad_if_match)?;
        let current = self.current_tag(id)?;
        if current.revision == named.revision {
            Ok(current.revision)
        } else {
            Err(stale(&current))
        }
    }
}
