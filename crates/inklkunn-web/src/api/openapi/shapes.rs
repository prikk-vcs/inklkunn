//! The schemas of the answers of the past, links, search and changes endpoints (`shape.rs` and `endpoints/` are
//! what produce them). Kept beside the others in the document's `components`; a test holds each endpoint's
//! real answer to its schema's members (`tests/api/shapes.rs`).

use serde_json::{Map, Value, json};

use crate::api::describe::SIZES;

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn array_of(name: &str) -> Value {
    json!({"type": "array", "items": schema_ref(name)})
}

fn or_null(name: &str) -> Value {
    json!({"oneOf": [schema_ref(name), {"type": "null"}]})
}

/// An object schema whose `required` members are all its properties but the ones in `optional`.
fn object(description: &str, properties: Value, optional: &[&str]) -> Value {
    let required: Vec<String> = properties
        .as_object()
        .map(|map| {
            map.keys()
                .filter(|k| !optional.contains(&k.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    json!({
        "type": "object",
        "description": description,
        "required": required,
        "properties": properties,
    })
}

/// A schema made of `base`'s properties and `extra`'s (the members every description of a change has, and the
/// members one kind of description adds).
fn with(base: &Value, description: &str, extra: Value) -> Value {
    let mut properties: Map<String, Value> = base
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Value::Object(more) = extra {
        properties.extend(more);
    }
    object(description, Value::Object(properties), &[])
}

/// The schemas, by name.
pub(super) fn schemas() -> Map<String, Value> {
    let tip = schema_ref("Tip");
    let cursor = json!({"type": ["string", "null"], "description": "Where the next page begins; `null` on the last. Opaque."});
    let people = json!({"type": "array", "items": {"type": "string"}, "description": "The sentences the pages say about the people of the change, in the language of `Content-Language`, from the change's message and no more. The wiki service signs a change into the history; no sentence says a person signed."});
    let change = object(
        "What every description of a change has, from the history's record of it. There is no time: the history carries none.",
        json!({
            "id": {"type": "string", "description": "The change's id: 64 lower-case hexadecimal digits. Give it to `revisions/{id}` and `diff`."},
            "number": {"type": "integer", "minimum": 1, "description": "The change's number in its view's history (1 for the first)."},
            "parent": {"type": ["string", "null"], "description": "The id of the change before this one, when the history the server reads holds it and this is not the first."},
            "first": {"type": "boolean", "description": "The first change of the view."},
            "undoes": {"type": "boolean", "description": "The version control says this change undoes earlier ones."},
            "summary": {"type": "string", "description": "The first line of the change's message: one line of plain text, at most 120 characters. Untrusted text."},
            "provenance": schema_ref("Provenance"),
            "people": people.clone(),
            "flags": {"type": "array", "items": {"type": "string", "enum": ["undo", "import", "service", "first"]}, "description": "What the change is marked as, beyond its people."},
        }),
        &[],
    );
    let mut out = Map::new();
    out.insert(
        "Person".to_owned(),
        object(
            "Who a message names. `name` is the text of a person as written (an account's display name then, an anonymous proposer's pseudonym, the service's label); it is plain text.",
            json!({
                "kind": {"type": "string", "enum": ["account", "anonymous", "service"]},
                "name": {"type": ["string", "null"]},
            }),
            &[],
        ),
    );
    out.insert(
        "Provenance".to_owned(),
        object(
            "What a change's message says about the people, and nothing it does not say. `proposed_by` and `accepted_by` are the people the message names; the wiki service signed the change into the history, whoever they were. `not-recorded` is a message that is not in the service's form: it names nobody, and nobody is assumed.",
            json!({
                "kind": {"type": "string", "enum": ["proposal", "rollback", "import", "system", "not-recorded"]},
                "proposed_by": or_null("Person"),
                "accepted_by": or_null("Person"),
            }),
            &[],
        ),
    );
    out.insert("Change".to_owned(), change.clone());
    out.insert(
        "PageEvent".to_owned(),
        object(
            "What a change did to the page a history is of. `title` is set for a move: the title the page had (`moved-from`) or has (`moved-to`). `not-inspected` is a change too large to list what it did, so it is not known whether it touched the page.",
            json!({
                "kind": {"type": "string", "enum": ["created", "edited", "deleted", "moved-from", "moved-to", "not-inspected"]},
                "title": {"type": "string"},
            }),
            &["title"],
        ),
    );
    out.insert(
        "HistoryRow".to_owned(),
        with(
            &change,
            "One change that touched the page.",
            json!({
                "page_title": {"type": "string", "description": "The title the page had after this change: before a move it is the old title."},
                "events": array_of("PageEvent"),
            }),
        ),
    );
    out.insert(
        "History".to_owned(),
        object(
            "One step of a page's history, newest change first.",
            json!({
                "title": {"type": "string", "description": "The page's title (an alias or a redirect led to it)."},
                "tip": tip.clone(),
                "items": {"type": "array", "maxItems": 20, "items": schema_ref("HistoryRow")},
                "next_cursor": cursor.clone(),
                "beyond_window": {"type": "boolean", "description": "The log is longer than the server reads, so the oldest changes are not listed."},
            }),
            &[],
        ),
    );
    out.insert(
        "Did".to_owned(),
        object(
            "One thing a change did. `subject` is set for `added`, `edited` and `removed`; `from` and `to` for `moved`. A page is named by title; any other file (an attachment, the manifest) is a `file` and is not named.",
            json!({
                "action": {"type": "string", "enum": ["added", "edited", "removed", "moved"]},
                "subject": schema_ref("Subject"),
                "from": schema_ref("Subject"),
                "to": schema_ref("Subject"),
            }),
            &["subject", "from", "to"],
        ),
    );
    out.insert(
        "Subject".to_owned(),
        object(
            "What a change did something to.",
            json!({
                "kind": {"type": "string", "enum": ["page", "file"]},
                "title": {"type": ["string", "null"]},
            }),
            &[],
        ),
    );
    out.insert(
        "Revision".to_owned(),
        with(
            &change,
            "One change of a view's history.",
            json!({
                "view": {"type": "string", "description": "The view the change was found in."},
                "did": {"type": "array", "maxItems": 200, "items": schema_ref("Did")},
                "omitted": {"type": "integer", "minimum": 0, "description": "How many more things it did than are listed."},
                "too_large": {"type": "boolean", "description": "The change is too large to list what it did."},
                "page": {"oneOf": [schema_ref("Page"), {"type": "null"}], "description": "The page titled by the `title` parameter as it stood after this change; `null` when no `title` was given."},
            }),
        ),
    );
    out.insert(
        "Segment".to_owned(),
        object(
            "A run of plain text. The runs of a line or a snippet join to the text, and a run never holds markup of its own: the client escapes it.",
            json!({
                "text": {"type": "string"},
                "marked": {"type": "boolean"},
            }),
            &[],
        ),
    );
    out.insert(
        "DiffLine".to_owned(),
        object(
            "One line of a hunk. `marked` segments are the wiki's word-level refinement; the line is the version control's own.",
            json!({
                "kind": {"type": "string", "enum": ["context", "removed", "added", "no-newline-marker"]},
                "segments": array_of("Segment"),
            }),
            &[],
        ),
    );
    out.insert(
        "Hunk".to_owned(),
        object(
            "One hunk. `lines` for a unified hunk, `text` for one that is not in that shape (shown as the text it is).",
            json!({
                "kind": {"type": "string", "enum": ["lines", "raw"]},
                "old_start": {"type": ["integer", "null"]},
                "new_start": {"type": ["integer", "null"]},
                "lines": array_of("DiffLine"),
                "text": {"type": "string"},
            }),
            &["old_start", "new_start", "lines", "text"],
        ),
    );
    out.insert(
        "Diff".to_owned(),
        object(
            "What changed in one page between two changes of a view's history.",
            json!({
                "title": {"type": "string"},
                "from": {"type": "string", "description": "The id of the older change."},
                "to": {"type": "string", "description": "The id of the newer change."},
                "from_number": {"type": "integer"},
                "to_number": {"type": "integer"},
                "status": {"type": "string", "enum": ["unchanged", "added", "removed", "modified", "other"]},
                "added": {"type": "integer", "minimum": 0, "description": "Lines added, over all hunks the version control gave."},
                "removed": {"type": "integer", "minimum": 0},
                "truncated": {"type": "boolean", "description": "More lines were given than the 4000 returned."},
                "no_lines": {"type": "boolean", "description": "The page differs and no lines were given: it is not text, or could not be compared."},
                "hunks": array_of("Hunk"),
            }),
            &[],
        ),
    );
    out.insert(
        "Backlink".to_owned(),
        object(
            "A page that links to the page.",
            json!({
                "title": {"type": "string", "description": "The linking page's title."},
                "kind": {"type": "string", "enum": ["link", "include", "redirect"], "description": "A link in the text, an inclusion, or a redirect page whose target this is."},
                "context": {"type": "string", "description": "The line of the linking page's source that makes the link, trimmed and shortened: plain text. Empty for a redirect."},
            }),
            &[],
        ),
    );
    out.insert(
        "Backlinks".to_owned(),
        object(
            "One page of the pages that link to a title.",
            json!({
                "title": {"type": "string"},
                "exists": {"type": "boolean", "description": "The title leads to a page. When it does not, these are the pages that link to it anyway (a wanted title)."},
                "tip": tip.clone(),
                "items": {"type": "array", "maxItems": SIZES.1, "items": schema_ref("Backlink")},
                "total": {"type": "integer", "minimum": 0},
                "cut": {"type": "boolean", "description": "The graph keeps at most 500 links for a title and holds that many: there may be others."},
                "next_cursor": cursor.clone(),
            }),
            &[],
        ),
    );
    out.insert(
        "HealthItem".to_owned(),
        object(
            "One row of a health list. `action` is what would resolve it, as a code: `add-page-or-fix-links` (wanted), `link-to-it` (orphans), `link-from-it` (dead-ends), `retarget-or-remove`, `break-the-loop`, `point-at-final-page` (redirects), `finish-and-unmark` (wants-help), `review` (stale). The other members depend on the list.",
            json!({
                "title": {"type": "string"},
                "action": {"type": "string", "enum": ["add-page-or-fix-links", "link-to-it", "link-from-it", "retarget-or-remove", "break-the-loop", "point-at-final-page", "finish-and-unmark", "review"]},
                "linked_from": {"type": "integer", "description": "`wanted`: how many different pages link to the title."},
                "target": {"type": "string", "description": "`redirects`: the target as the redirect page wrote it."},
                "problem": {"type": "string", "enum": ["broken", "loop", "too-long", "double"], "description": "`redirects`: what is wrong."},
                "mark": {"type": "string", "enum": ["wip", "draft", "stub"], "description": "`wants-help`: how the page is marked."},
                "last_touched": {"type": ["integer", "null"], "description": "`stale`: when this server first saw the newest change that touched the page, in Unix seconds; `null` when none of the changes looked through touched it."},
            }),
            &["linked_from", "target", "problem", "mark", "last_touched"],
        ),
    );
    out.insert(
        "Health".to_owned(),
        object(
            "One page of one of the link health lists.",
            json!({
                "list": {"type": "string", "enum": ["wanted", "orphans", "dead-ends", "redirects", "wants-help", "stale"]},
                "tip": tip.clone(),
                "items": {"type": "array", "maxItems": SIZES.1, "items": schema_ref("HealthItem")},
                "total": {"type": "integer", "minimum": 0, "description": "How many rows the whole list has."},
                "next_cursor": cursor.clone(),
                "scan": {
                    "type": ["object", "null"],
                    "description": "`stale` only: how far the list looked.",
                    "required": ["looked_through", "complete"],
                    "properties": {
                        "looked_through": {"type": "integer", "minimum": 0},
                        "complete": {"type": "boolean", "description": "The whole history was looked through: a page with no touch has none at all, rather than none lately."},
                    },
                },
            }),
            &[],
        ),
    );
    out.insert(
        "SearchHit".to_owned(),
        object(
            "A page the search found, by the text that matched.",
            json!({
                "title": {"type": "string", "description": "The page's title."},
                "kind": {"type": "string", "enum": ["title", "alias"], "description": "Did the page's title match, or one of its aliases?"},
                "snippet": {"type": "array", "items": schema_ref("Segment"), "description": "The text that matched (the title or the alias), cut into the part that matched (`marked`) and the rest. Plain text, never HTML."},
            }),
            &[],
        ),
    );
    out.insert(
        "Search".to_owned(),
        object(
            "What a search found, best first.",
            json!({
                "query": {"type": "string", "description": "The query as searched (trimmed, cut to a length)."},
                "tip": tip.clone(),
                "names_a_page": {"type": "boolean", "description": "The query is itself a title, an alias or a redirect."},
                "truncated": {"type": "boolean", "description": "More pages matched than are listed."},
                "items": {"type": "array", "maxItems": 50, "items": schema_ref("SearchHit")},
            }),
            &[],
        ),
    );
    out.insert(
        "ChangePage".to_owned(),
        object(
            "A page the change touched.",
            json!({
                "title": {"type": "string", "description": "Its title at the view's tip if the page is still there, else the one its path spells."},
                "exists": {"type": "boolean", "description": "The page is still there at the view's tip."},
            }),
            &[],
        ),
    );
    out.insert(
        "ChangeRow".to_owned(),
        object(
            "One recent change.",
            json!({
                "id": {"type": "string", "description": "The change's id (64 lower-case hexadecimal digits): give it to `revisions/{id}`."},
                "view": {"type": "string", "description": "The view it was listed from."},
                "summary": {"type": "string", "description": "The first line of the change's message: plain text. Untrusted text."},
                "provenance": schema_ref("Provenance"),
                "people": people,
                "rollback": {"type": "boolean", "description": "It is an undo."},
                "automated": {"type": "boolean", "description": "The wiki service made it itself (an import, creating the wiki), or it was proposed as machine-assisted (`machine_assisted`)."},
                "minor": {"type": "boolean", "description": "It was proposed as a minor change."},
                "machine_assisted": {"type": "boolean", "description": "It was proposed as machine-assisted content. The person who accepted it (`provenance.accepted_by`) answers for it."},
                "first_seen": {"type": ["integer", "null"], "description": "When this server first saw the change, in Unix seconds; not when it was made, which the history does not say. `null` when unknown."},
                "pages": {"type": ["array", "null"], "items": schema_ref("ChangePage"), "description": "What it touched; `null` when that could not be asked just now (`pages_missing`)."},
            }),
            &[],
        ),
    );
    out.insert(
        "Changes".to_owned(),
        object(
            "One page of the recent changes.",
            json!({
                "items": {"type": "array", "maxItems": SIZES.1, "items": schema_ref("ChangeRow")},
                "next_cursor": cursor,
                "views": {"type": "array", "items": {"type": "string"}, "description": "The wiki's open views by name, the default first: what `view` can name."},
                "left_out": {"type": "integer", "minimum": 0, "description": "How many changes of this page were left out because the caller may not read them. Which is not said."},
                "pages_missing": {"type": "boolean", "description": "Some changes are listed without their pages because the server could not ask just now."},
                "beyond_window": {"type": "boolean", "description": "The list is the newest part of the history: the server looks through the newest 100 changes of each view."},
            }),
            &[],
        ),
    );
    out
}
