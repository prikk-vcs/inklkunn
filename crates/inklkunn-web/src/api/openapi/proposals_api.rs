//! The schemas of the reads of the proposals (RFC 007 D-9; handoff 007 I-T5): a page of a wiki's proposals, and a page of a
//! proposal's comments. Each item is the schema the write API already has (`Proposal`, `Comment`), so a client reads a list
//! with the code that reads one; a test holds each answer to the members its schema names (`tests/api/proposals.rs`).

use serde_json::{Map, Value, json};

use super::shapes::{object, schema_ref};
use crate::api::describe::SIZES;

/// The schemas, by name.
pub(super) fn schemas() -> Map<String, Value> {
    let cursor = json!({"type": ["string", "null"], "description": "Where the next page of the list begins; `null` on the last. Opaque. The `Link` header of the answer names the same page (`rel=\"next\"`)."});
    let mut out = Map::new();
    out.insert(
        "ProposalList".to_owned(),
        object(
            "A page of the proposals of a wiki that were sent for review, newest sent first. **Never a draft.** Each is the proposal as `getProposal` tells it.",
            json!({
                "items": {"type": "array", "maxItems": SIZES.1, "items": schema_ref("Proposal")},
                "next_cursor": cursor.clone(),
            }),
            &[],
        ),
    );
    out.insert(
        "ProposalComments".to_owned(),
        object(
            "A page of a proposal's comments, in the order they were made. Each is the comment as `commentOnProposal` tells it; one that moderation hides has an empty `body` and says which parts are hidden, and why. No comment names its author.",
            json!({
                "items": {"type": "array", "maxItems": SIZES.1, "items": schema_ref("Comment")},
                "next_cursor": cursor,
            }),
            &[],
        ),
    );
    out
}
