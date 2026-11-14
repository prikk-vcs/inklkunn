//! The schemas of the knowledge layer's endpoints (RFC 005 part 2 D-12; handoff 005 K-T11): a claim, a source, a page's
//! provenance, the comparison of two views, and the body of a curation proposal. The enumerations are read from the
//! format's own words (`inklkunn_core::knowledge`), so the document cannot say a word the files do not. A test holds
//! each endpoint's real answer to its schema's members (`tests/api/knowledge.rs`).

use inklkunn_core::knowledge::{
    Availability, Bearing, Lifecycle, Link, ReferenceForm, RelationType, SourceKind, Standing,
    StatementKind,
};
use serde_json::{Map, Value, json};

use super::shapes::{closed, object, request, schema_ref};

/// The ids the examples of a curation body name (a claim and two sources; any ids of the format would do).
const EXAMPLE_CLAIM: &str = "01930000-0000-7000-8000-000000000001";
const EXAMPLE_FIRST: &str = "01931111-0000-7000-8000-000000000001";
const EXAMPLE_SECOND: &str = "01931111-0000-7000-8000-000000000002";

/// The shape of the id of a claim or a source: a version-7 UUID in lower case with hyphens, the only text the format
/// and the handlers take. (`format: uuid` is an annotation for a validator, and admits more.)
const ID_PATTERN: &str = "^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$";
/// The same with white space around it, which the handler of a source's replacement ignores.
const ID_PATTERN_PADDED: &str =
    "^\\s*[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\\s*$";
/// Nothing, or white space only: the handler takes it for no source.
const NO_ID_PATTERN: &str = "^\\s*$";

fn relation_enum() -> Value {
    json!({"type": "string", "enum": words(Link::ALL, Link::word), "description": "Whether the source supports the claim or challenges it."})
}

fn bearing_enum() -> Value {
    json!({"type": "string", "enum": words(Bearing::ALL, Bearing::word), "description": "How directly the source speaks to the claim, never how true either is."})
}

fn words<T: Copy>(all: &[T], word: impl Fn(T) -> &'static str) -> Vec<&'static str> {
    all.iter().map(|w| word(*w)).collect()
}

/// The schemas, by name.
pub(super) fn schemas() -> Map<String, Value> {
    let tip = schema_ref("Tip");
    let judgement = json!({"type": "string", "enum": ["current", "earlier", "none"], "description": "`current`: judged on the wording the claim has now. `earlier`: judged on an earlier wording, which nobody has confirmed since. `none`: nothing was judged."});
    let relation = json!({"type": "string", "enum": words(Link::ALL, Link::word), "description": "Whether the source supports the claim or challenges it."});
    let bearing = json!({"type": "string", "enum": words(Bearing::ALL, Bearing::word), "description": "How directly the source speaks to the claim, never how true either is."});
    let mut out = Map::new();
    out.insert(
        "ClaimSaid".to_owned(),
        object(
            "A place a claim is said: a paragraph of a page.",
            json!({
                "title": {"type": ["string", "null"], "description": "The page's title at the tip, when exactly one page has the id the claim names."},
                "page_id": {"type": "string", "description": "The page's `id`, as the claim names it."},
                "block": {"type": "string", "description": "The paragraph's `^id`."},
                "state": {"type": "string", "enum": ["present", "page-missing", "page-repeated", "block-missing", "block-repeated"], "description": "`present`: the page has the paragraph once. The others say why there is no paragraph to point at."},
            }),
            &[],
        ),
    );
    out.insert(
        "ClaimSource".to_owned(),
        object(
            "A source a claim cites and how it bears on the claim.",
            json!({
                "source": {"type": "string", "description": "The source's id: give it to `sources/{id}`."},
                "relation": relation.clone(),
                "bearing": bearing.clone(),
                "judged_on": judgement.clone(),
                "readable": {"type": "boolean", "description": "The source's file can be read. When it cannot, nothing more is known of it."},
            }),
            &[],
        ),
    );
    out.insert(
        "Claim".to_owned(),
        object(
            "A claim, from the derived knowledge index of the view's tip. Nothing in it is a score, a rank or a verdict.",
            json!({
                "id": {"type": "string"},
                "kind": {"type": "string", "enum": words(StatementKind::ALL, StatementKind::word), "description": "What kind of statement it is."},
                "lifecycle": {"type": "string", "enum": words(Lifecycle::ALL, Lifecycle::word)},
                "status": {
                    "type": "object",
                    "description": "What the community has decided, by an accepted curation proposal and never by a count.",
                    "required": ["standing", "proposal", "proposal_unlisted", "judged_on"],
                    "properties": {
                        "standing": {"type": "string", "enum": words(Standing::ALL, Standing::word)},
                        "proposal": {"type": ["string", "null"], "description": "The proposal that decided it, when one did and the caller may read it."},
                        "proposal_unlisted": {"type": "boolean", "description": "A proposal decided it and the caller may not read it."},
                        "judged_on": judgement,
                    },
                },
                "said": {"type": "array", "items": schema_ref("ClaimSaid"), "description": "Where the claim is said."},
                "sources": {"type": "array", "items": schema_ref("ClaimSource"), "description": "The sources it cites."},
                "relations": {
                    "type": "array",
                    "description": "How it relates to other claims.",
                    "items": {
                        "type": "object",
                        "required": ["type", "claim"],
                        "properties": {
                            "type": {"type": "string", "enum": words(RelationType::ALL, RelationType::word)},
                            "claim": {"type": "string", "description": "The other claim's id."},
                        },
                    },
                },
                "tip": tip.clone(),
            }),
            &[],
        ),
    );
    out.insert(
        "SourceCitedBy".to_owned(),
        object(
            "A claim that cites a source.",
            json!({
                "claim": {"type": "string", "description": "The claim's id: give it to `claims/{id}`."},
                "relation": relation,
                "bearing": bearing,
                "judged_on": judgement,
            }),
            &[],
        ),
    );
    out.insert(
        "Source".to_owned(),
        object(
            "A source, from the derived knowledge index of the view's tip. Its texts are as written, cut at 300 characters, and untrusted.",
            json!({
                "id": {"type": "string"},
                "kind": {"type": "string", "enum": words(SourceKind::ALL, SourceKind::word)},
                "availability": {"type": "string", "enum": words(Availability::ALL, Availability::word), "description": "Whether it can be had, as a person who checked says."},
                "last_reviewed": {"type": ["string", "null"], "description": "The day a person last reviewed it, as written (`YYYY-MM-DD`)."},
                "superseded_by": {"type": ["string", "null"], "description": "The source that replaces it."},
                "title": {"type": ["string", "null"]},
                "author": {"type": ["string", "null"], "description": "An author or a publisher."},
                "date": {"type": ["string", "null"], "description": "A year, a season or a day, as written."},
                "reference": {"type": ["string", "null"], "description": "Where it is found: a web address, a citation, a place. Never fetched by the wiki."},
                "claims": {"type": "array", "maxItems": 50, "items": schema_ref("SourceCitedBy"), "description": "The claims that cite it, by claim id, at most 50."},
                "claims_total": {"type": "integer", "minimum": 0, "description": "How many claims cite it in all."},
                "tip": tip.clone(),
            }),
            &[],
        ),
    );
    out.insert(
        "ProvenanceParagraph".to_owned(),
        object(
            "A paragraph of the page and its past, as numbers of `changes`.",
            json!({
                "ordinal": {"type": "integer", "minimum": 1, "description": "Its place among the page's blocks, from 1."},
                "id": {"type": ["string", "null"], "description": "Its `^id`, when it has one."},
                "excerpt": {"type": "string", "description": "How it begins, on one line. Untrusted text."},
                "claims": {"type": "array", "items": {"type": "string"}, "description": "The claims said in it, by id."},
                "basis": {"type": "string", "enum": ["anchor", "text"], "description": "`anchor`: followed through its `^id`, which is exact. `text`: traced by matching its text, so what was found is the earliest wording found and not what happened."},
                "last_changed": {"type": ["integer", "null"], "description": "The `number` of the change that last changed it; `null` when no change read changed it."},
                "first_written": {"type": ["integer", "null"], "description": "The `number` of the change that first wrote it; `null` when the reading ended while it still had an earlier counterpart."},
            }),
            &[],
        ),
    );
    let change = super::shapes::schemas()
        .remove("Change")
        .unwrap_or_default();
    let mut change_properties: Map<String, Value> = change
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    change_properties.insert(
        "proposal".to_owned(),
        json!({
            "type": ["object", "null"],
            "description": "The proposal the change was made from: set only when the message names one, the acceptance records tie it to this change and the caller may read it.",
            "required": ["id", "summary", "sources"],
            "properties": {
                "id": {"type": "string"},
                "summary": {"type": "string", "description": "Untrusted text."},
                "sources": {"type": "array", "items": {"type": "string"}, "description": "The sources the proposer gave with it, each cut at 300 characters. Untrusted text."},
            },
        }),
    );
    out.insert(
        "ProvenanceChange".to_owned(),
        object(
            "A change a paragraph's past names: the members of a change, and the proposal it was made from.",
            Value::Object(change_properties),
            &[],
        ),
    );
    out.insert(
        "PageProvenance".to_owned(),
        object(
            "*Why is this here?* for a page, as data.",
            json!({
                "title": {"type": "string"},
                "tip": tip.clone(),
                "reach": {"type": "string", "enum": ["whole", "bounded", "spent", "unfollowed"], "description": "How far back the history was read. `whole`: every paragraph was followed to the change that wrote it. `bounded`: the history page's bounds ended the reading. `spent`: the work bound did. `unfollowed`: an earlier version could not be read."},
                "left_out": {"type": "integer", "minimum": 0, "description": "How many more paragraphs the page has than the 500 followed."},
                "paragraphs": {"type": "array", "items": schema_ref("ProvenanceParagraph")},
                "changes": {"type": "array", "items": schema_ref("ProvenanceChange"), "description": "The changes the paragraphs name, newest first."},
            }),
            &[],
        ),
    );
    out.insert(
        "ComparedPage".to_owned(),
        object(
            "A page the two views have differently.",
            json!({
                "title": {"type": "string"},
                "path": {"type": "string"},
                "status": {"type": "string", "enum": ["unchanged", "added", "removed", "modified", "other"], "description": "`added`: only `b` has it. `removed`: only `a`."},
                "added": {"type": "integer", "minimum": 0, "description": "Lines `b` has that `a` does not."},
                "removed": {"type": "integer", "minimum": 0, "description": "Lines `a` has that `b` does not."},
            }),
            &[],
        ),
    );
    out.insert(
        "ComparedClaim".to_owned(),
        object(
            "A claim that differs between the two views. A claim whose three facts agree is not listed.",
            json!({
                "id": {"type": "string"},
                "presence": {"type": "string", "enum": ["both", "only-a", "only-b", "unreadable-in-a", "unreadable-in-b"], "description": "Which views have it. `unreadable-in-b`: `a` has it and `b` has a file of that name that cannot be read, which is not the same as having none."},
                "wording": {"type": "boolean", "description": "Both have it and its paragraphs read differently."},
                "status": {
                    "type": ["object", "null"],
                    "description": "Both have it and the community's status differs.",
                    "required": ["a", "b"],
                    "properties": {"a": {"type": "string"}, "b": {"type": "string"}},
                },
                "sources": {"type": "boolean", "description": "Both have it and the sources that bear on it, or how, or what a source is, differ."},
                "said_in": {"type": ["string", "null"], "description": "The title of the first page that says it, so that it can be told from another."},
            }),
            &[],
        ),
    );
    out.insert(
        "ViewComparison".to_owned(),
        object(
            "Two views, compared: where they differ, claim by claim and page by page, and nothing about which is right.",
            json!({
                "a": tip.clone(),
                "b": tip,
                "same": {"type": "boolean", "description": "The two tips are one and the same change: nothing differs."},
                "pages": {"type": "array", "maxItems": 200, "items": schema_ref("ComparedPage")},
                "pages_omitted": {"type": "integer", "minimum": 0},
                "claims": {"type": "array", "maxItems": 200, "items": schema_ref("ComparedClaim")},
                "claims_omitted": {"type": "integer", "minimum": 0},
            }),
            &[],
        ),
    );
    // A link is changed or let go, and the two shapes are the alternatives: a schema that marked `relation` and `bearing`
    // required would forbid letting a link go, and one that marked them optional would allow a body the handler refuses.
    let source_id = json!({"type": "string", "format": "uuid", "pattern": ID_PATTERN, "description": "A source the claim cites: a source id, in lower case."});
    out.insert(
        "CurationLink".to_owned(),
        json!({
            "description": "How a source the claim cites should bear on it, or that the claim no longer cites it. A link is changed (`relation` and `bearing`, and `remove` is `false` or left out) or removed (`remove: true`, and nothing else), not both.",
            "oneOf": [
                closed(json!({
                    "type": "object",
                    "description": "A link that stays, and bears on the claim as these say.",
                    "required": ["source", "relation", "bearing"],
                    "properties": {
                        "source": source_id.clone(),
                        "relation": relation_enum(),
                        "bearing": bearing_enum(),
                        "remove": {"const": false, "description": "`false`, or left out."},
                    },
                })),
                closed(json!({
                    "type": "object",
                    "description": "A link that goes.",
                    "required": ["source", "remove"],
                    "properties": {
                        "source": source_id.clone(),
                        "remove": {"const": true, "description": "`true`: the claim no longer cites the source. The source's own file stays, and other claims keep their links to it. Nothing else is given with it."},
                    },
                })),
            ],
        }),
    );
    out.insert(
        "CurationSource".to_owned(),
        closed(object(
            "What a person checked about a source the claim cites and who and what it is. Every member but `source` may be left out, and a member left out is left as the base has it; an empty text is nothing.",
            json!({
                "source": source_id,
                "reviewed": {"type": "string", "description": "The day a person last checked it, `YYYY-MM-DD`. Left out: the base's day stays. The empty text clears it."},
                "availability": {"type": "string", "enum": words(Availability::ALL, Availability::word), "description": "Whether it can be had, as the person who checked says. Left out: the base's stays. `unknown` clears it."},
                "title": {"type": "string", "description": "Its title. Left out, or the same as the base's: unchanged (it is not cleaned or compared again). The empty text for none."},
                "author": {"type": "string", "description": "Its author or publisher. Left out: unchanged. The empty text for none."},
                "date": {"type": "string", "description": "Its date, as written. Left out: unchanged. The empty text for none."},
                "reference": {"type": "object", "description": "Where it is found. Left out: unchanged. A source needs a title or a reference.", "properties": {
                    "type": {"type": "string", "enum": words(ReferenceForm::ALL, ReferenceForm::word)},
                    "value": {"type": "string", "description": "The reference; the empty text for none."},
                }, "required": ["type", "value"], "additionalProperties": false},
                "superseded_by": {"description": "The id of the source that replaces it, a source of this wiki (`unknown-successor`) that does not lead back to this one or to a source it passed (`successor-cycle`). White space around it is ignored. Left out: unchanged. The empty text, or white space only, for none.", "anyOf": [
                    {"type": "string", "pattern": ID_PATTERN_PADDED},
                    {"type": "string", "pattern": NO_ID_PATTERN},
                ]},
            }),
            &["reviewed", "availability", "title", "author", "date", "reference", "superseded_by"],
        )),
    );
    out.insert(
        "NewCuration".to_owned(),
        request(
            object(
                "The body of a curation proposal. A member not listed is `invalid-body`; a member left out is left as the base has it.",
                json!({
                    "claim": {"type": "string", "format": "uuid", "pattern": ID_PATTERN, "description": "The claim's id, in lower case."},
                    "summary": {"type": "string", "description": "What and why: one line of at most 120 characters. Required to send for review."},
                    "view": {"type": "string", "description": "The view to propose to. Default: the wiki's default view."},
                    "submit": {"type": "boolean", "description": "`true` sends the proposal for review at once; `false` keeps a draft only the proposer sees."},
                    "status": {"type": "string", "enum": words(Standing::ALL, Standing::word), "description": "The community status to propose."},
                    "links": {"type": "array", "maxItems": 256, "items": schema_ref("CurationLink"), "description": "Links to change or remove, each naming a source the claim cites."},
                    "sources": {"type": "array", "maxItems": 256, "items": schema_ref("CurationSource"), "description": "Sources to record a check of, or to correct (checked or not), each naming a source the claim cites."},
                    "confirm": {"type": "boolean", "description": "Judge the sources and the status again on the wording the claim has in the base. Default `false`."},
                }),
                &["view", "status", "links", "sources", "confirm"],
            ),
            // The ids are those of the examples of the claim and its sources; a wiki that has others answers `not-found`.
            json!([
                {"claim": EXAMPLE_CLAIM, "summary": "Accept what the survey says", "submit": true, "status": "accepted"},
                {"claim": EXAMPLE_CLAIM, "summary": "The second source bears less directly; the first was checked", "submit": false, "view": "main",
                 "links": [{"source": EXAMPLE_SECOND, "relation": "supports", "bearing": "indirect", "remove": false}],
                 "sources": [{"source": EXAMPLE_FIRST, "reviewed": "2026-11-01", "availability": "available"}], "confirm": false},
                {"claim": EXAMPLE_CLAIM, "summary": "The claim no longer cites the second source", "submit": true,
                 "links": [{"source": EXAMPLE_SECOND, "remove": true}]},
                {"claim": EXAMPLE_CLAIM, "summary": "Correct the survey's record", "submit": true,
                 "sources": [{"source": EXAMPLE_FIRST, "title": "The bridge survey, second edition", "author": "Noor Aziz", "date": "2024",
                              "reference": {"type": "doi", "value": "10.1000/182"}, "superseded_by": EXAMPLE_SECOND}]},
                {"claim": EXAMPLE_CLAIM, "summary": "Nobody checked the survey after all", "submit": false,
                 "sources": [{"source": EXAMPLE_FIRST, "reviewed": "", "availability": "unknown", "superseded_by": ""}]},
            ]),
        ),
    );
    out
}
