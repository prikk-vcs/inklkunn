//! The schemas of the write API: what a body holds and what an answer says (`propose/shape.rs`, `decide.rs`
//! produce them; `tests/api/write.rs` holds each answer to its schema's members).

use serde_json::{Map, Value, json};

use super::shapes::{closed, object, request, schema_ref};

/// The schemas, by name.
pub(super) fn schemas() -> Map<String, Value> {
    let state = json!({
        "type": "string",
        "enum": ["draft", "held-for-review", "needs-revision", "accepted", "declined", "withdrawn", "superseded"],
        "description": "Where the proposal is in its life.",
    });
    let proposal = object(
        "A proposal. Its `etag` is what the next write on it names in `If-Match` (the `ETag` header says the same): it is made from the proposal's id and its latest revision, so it changes with a new revision and not with a decision.",
        json!({
            "id": {"type": "string", "description": "The proposal's id."},
            "state": state,
            "view": {"type": "string", "description": "The view the proposal is addressed at."},
            "summary": {"type": "string", "description": "What and why: one line. Untrusted text."},
            "sources": {"type": "array", "items": {"type": "string"}, "description": "Where the change comes from. Untrusted text."},
            "revision": {"type": "integer", "minimum": 0, "description": "The latest revision; 0 while the proposal is a draft."},
            "etag": {"type": "string", "description": "The base of the next write, quotes included."},
            "yours": {"type": "boolean", "description": "Whether the caller is the proposer."},
            "superseded_by": {"type": ["string", "null"], "description": "The proposal that superseded this one."},
            "created_at": {"type": "integer", "description": "Unix seconds."},
            "updated_at": {"type": "integer", "description": "Unix seconds."},
            "submitted_at": {"type": ["integer", "null"], "description": "Unix seconds; `null` for a proposal never sent for review."},
            "closed_at": {"type": ["integer", "null"], "description": "Unix seconds; `null` while the proposal is open."},
            "acceptance": {
                "type": "object",
                "description": "Only in the answer to an acceptance.",
                "required": ["id", "outcome", "block_id", "patch_id", "reauthored"],
                "properties": {
                    "id": {"type": "string"},
                    "outcome": {"type": "string", "enum": ["signed", "already-signed"], "description": "`already-signed`: an earlier call put the change in the history, and this one wrote nothing."},
                    "block_id": {"type": "string", "description": "The change in the history."},
                    "patch_id": {"type": "string"},
                    "reauthored": {"type": "boolean", "description": "The view had moved on and nothing overlapped, so the change was written again against its tip."},
                },
            },
        }),
        &["acceptance"],
    );
    // Exactly one of `path` and `title`, and `text` always: the two shapes are the alternatives.
    let file = json!({
        "description": "One page of a proposal: a page that exists, named by its `path`, or a page that does not exist yet, named by its `title`. `text` is always there.",
        "oneOf": [
            closed(json!({
                "type": "object",
                "description": "A page that exists.",
                "required": ["path", "text"],
                "properties": {
                    "path": {"type": "string", "description": "The repository path of a page that exists (the `storage_path` of the raw endpoint), inside `pages/`."},
                    "text": {"type": ["string", "null"], "description": "The page's whole new text, carried as it is; `null` removes the page."},
                },
            })),
            closed(json!({
                "type": "object",
                "description": "A page that does not exist yet.",
                "required": ["title", "text"],
                "properties": {
                    "title": {"type": "string", "description": "The title of a page that does not exist yet; its path is made from it by the wiki's rule."},
                    "text": {"type": "string", "description": "The new page's whole text, carried as it is."},
                },
            })),
        ],
    });
    let rename = closed(object(
        "A declared rename.",
        json!({
            "from": {"type": "string"},
            "to": {"type": "string"},
        }),
        &[],
    ));
    let drop = closed(object(
        "A paragraph removed on purpose.",
        json!({
            "path": {"type": "string", "description": "The repository path of the page, as in `files`."},
            "marker": {"type": "string", "description": "The `^id` marker of the paragraph on that page, without the caret."},
        }),
        &[],
    ));
    let edit_members = json!({
        "summary": {"type": "string", "description": "What and why: one line of at most 120 characters. Required to send for review."},
        "sources": {"type": "array", "maxItems": 20, "items": {"type": "string"}},
        "files": {"type": "array", "maxItems": 20, "items": schema_ref("ProposalFile")},
        "renames": {"type": "array", "maxItems": 20, "items": schema_ref("ProposalRename")},
        "minor": {"type": "boolean", "description": "A minor change. Default `false`."},
        "automated": {"type": "boolean", "description": "Machine-assisted content. Default `false`."},
        "drops": {"type": "array", "maxItems": 50, "items": schema_ref("ProposalDrop"), "description": "The paragraphs that claims are said in and that these files leave out, each named with its page: the explicit act of removing them. A paragraph that is left out and not named is `unacceptable` (`claim-paragraph-removed`, naming the page and the marker); a drop that is not such a paragraph of that page is `unacceptable` (`drop-not-removed`, likewise). The claims said there stop being said there, in the same proposal, and the reviewer sees it; the claim stays, said nowhere. Default: none."},
    });
    let mut new_members = edit_members.clone();
    if let Some(map) = new_members.as_object_mut() {
        map.insert("view".to_owned(), json!({"type": "string", "description": "The view to propose to. Default: the wiki's default view."}));
        map.insert("submit".to_owned(), json!({"type": "boolean", "description": "`true` sends the proposal for review at once; `false` keeps a draft only the proposer sees."}));
    }
    let new_proposal = request(
        object(
            "The body that opens a proposal. A member not listed is `invalid-body`.",
            new_members,
            &[
                "view",
                "sources",
                "files",
                "renames",
                "minor",
                "automated",
                "drops",
            ],
        ),
        json!([
            {"summary": "Add a page about the notes", "submit": false, "files": [{"title": "Notes", "text": "The notes begin here.\n"}]},
            {"summary": "Add two pages", "submit": true, "view": "main", "files": [{"title": "Alpha", "text": "First.\n"}, {"title": "Beta", "text": "Second.\n"}], "sources": ["https://example.org/survey"], "renames": [], "minor": false, "automated": false, "drops": []},
        ]),
    );
    let proposal_edit = request(
        object(
            "The body that saves a draft or adds a revision.",
            edit_members,
            &["sources", "files", "renames", "minor", "automated", "drops"],
        ),
        json!([
            {"summary": "Add a page about the notes", "files": [{"title": "Notes", "text": "The notes begin here, revised.\n"}]},
            {"summary": "Say why", "sources": ["https://example.org/survey"], "minor": true},
        ]),
    );
    let reason = request(
        object(
            "The body of a decline or a withdrawal.",
            json!({"reason": {"type": "string", "maxLength": 4096, "description": "Required; at most 4096 bytes of UTF-8. `maxLength` counts characters, so it is only the first check: a text of fewer characters can still be longer than 4096 bytes, and is refused (`reason-too-long`)."}}),
            &[],
        ),
        json!([{"reason": "Another proposal says this better"}]),
    );
    let new_comment = request(
        object(
            "The body of a comment.",
            json!({
                "body": {"type": "string", "description": "The comment: required, at most 16384 bytes."},
                "parent": {"type": "string", "description": "The id of the comment this replies to."},
                "anchor": {
                    "type": "object",
                    "description": "The part of the revision the comment is about; the server builds the anchor from that revision's own text.",
                    "required": ["page", "ordinal"],
                    "properties": {
                        "page": {"type": "string", "description": "The page's repository path."},
                        "ordinal": {"type": "integer", "minimum": 0, "description": "The part's position among the page's top-level blocks (0 for the first)."},
                    },
                    "additionalProperties": false,
                },
            }),
            &["parent", "anchor"],
        ),
        json!([
            {"body": "This reads well."},
            {"body": "Is this the date the survey gives?", "anchor": {"page": "pages/Home.md", "ordinal": 0}},
        ]),
    );
    let new_decision = request(
        object(
            "The body of a decision on the revision `If-Match` names.",
            json!({
                "decision": {"type": "string", "enum": ["approve", "request-changes"]},
                "body": {"type": "string", "description": "What the reviewer says with it; may be left out."},
            }),
            &["body"],
        ),
        json!([
            {"decision": "approve"},
            {"decision": "request-changes", "body": "Please name the source."},
        ]),
    );
    let comment = object(
        "A comment as recorded.",
        json!({
            "id": {"type": "string"},
            "revision": {"type": "integer", "description": "The revision that was the latest."},
            "parent": {"type": ["string", "null"]},
            "body": {"type": "string", "description": "Untrusted text."},
            "anchor": {"type": ["object", "null"], "description": "`page`, `ordinal` and `block_id` (when the part has one)."},
            "created_at": {"type": "integer"},
        }),
        &[],
    );
    let decision = object(
        "A decision as recorded.",
        json!({
            "id": {"type": "string"},
            "revision": {"type": "integer"},
            "decision": {"type": "string", "enum": ["approve", "request-changes"]},
            "body": {"type": "string", "description": "Untrusted text."},
            "created_at": {"type": "integer"},
        }),
        &[],
    );
    let mut out = Map::new();
    for (name, schema) in [
        ("Proposal", proposal),
        ("ProposalFile", file),
        ("ProposalRename", rename),
        ("ProposalDrop", drop),
        ("NewProposal", new_proposal),
        ("ProposalEdit", proposal_edit),
        ("Reason", reason),
        ("NewComment", new_comment),
        ("NewDecision", new_decision),
        ("Comment", comment),
        ("Decision", decision),
        (
            "Empty",
            json!({"type": "object", "description": "A body with no member: `{}`.", "additionalProperties": false, "examples": [{}]}),
        ),
    ] {
        out.insert(name.to_owned(), schema);
    }
    out
}
