//! The request shapes of the write API (K58): what a body must look like, said twice (by the OpenAPI document's schema and
//! by the handler that reads it), and the two held to each other. Every write endpoint's request schema in the document
//! has `examples`; each is sent through the schema and through the handler, which must take it. Then a table of the shapes
//! the handler accepts and the ones it refuses is sent through both, and the test fails wherever the schema and the
//! handler disagree: a client that follows the document can make every request the server takes (a link that is let go
//! says nothing else), and sends none the server refuses for its shape. No JSON-schema crate is in the lock, so the
//! schema is walked by `shapes::Checker`, the walk the answers are held to.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::common::{claim_id, source_id};
use super::super::shapes::Checker;
use super::super::{Answer, At, get};
use super::{
    API, better, cast, id_of, open, post, problem_type, session, tag_header, tip, token, urn,
};

/// The request schemas the document's writes name, with the paths they are the body of.
fn request_schemas(document: &Value) -> BTreeMap<String, Vec<String>> {
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, item) in document.at("/paths").as_object().unwrap() {
        let reference = item.at("/post/requestBody/content/application~1json/schema/$ref");
        if let Some(name) = reference
            .as_str()
            .and_then(|r| r.strip_prefix("#/components/schemas/"))
        {
            found.entry(name.to_owned()).or_default().push(path.clone());
        }
    }
    found
}

/// Send `body` to the endpoint that takes the request schema `schema`, in a wiki of its own, as someone who may.
async fn through_handler(schema: &str, body: &Value) -> Answer {
    if schema == "NewCuration" {
        return super::knowledge::send_curation(body).await;
    }
    let c = cast();
    let text = body.to_string();
    match schema {
        "NewProposal" => {
            post(
                &c.w,
                &format!("{API}/proposals"),
                &[session(&c.contributor), tip()],
                text,
            )
            .await
        }
        "ProposalEdit" => {
            let p = open(&c, &c.contributor, false).await;
            let id = id_of(&p);
            post(
                &c.w,
                &format!("{API}/proposals/{id}/draft"),
                &[session(&c.contributor), tag_header(&p)],
                text,
            )
            .await
        }
        "Empty" => {
            let p = open(&c, &c.contributor, false).await;
            let id = id_of(&p);
            post(
                &c.w,
                &format!("{API}/proposals/{id}/submission"),
                &[session(&c.contributor), tag_header(&p)],
                text,
            )
            .await
        }
        "Reason" => {
            let p = open(&c, &c.contributor, true).await;
            let id = id_of(&p);
            post(
                &c.w,
                &format!("{API}/proposals/{id}/withdrawals"),
                &[session(&c.contributor), tag_header(&p)],
                text,
            )
            .await
        }
        "NewComment" => {
            let p = open(&c, &c.contributor, true).await;
            let id = id_of(&p);
            post(
                &c.w,
                &format!("{API}/proposals/{id}/comments"),
                &[token("propose", &c.other), tag_header(&p)],
                text,
            )
            .await
        }
        "NewDecision" => {
            let p = open(&c, &c.contributor, true).await;
            let id = id_of(&p);
            post(
                &c.w,
                &format!("{API}/proposals/{id}/decisions"),
                &[token("review", &c.reviewer), tag_header(&p)],
                text,
            )
            .await
        }
        other => panic!("a write body with no handler in this test: {other}"),
    }
}

/// The handler refused the shape of the body.
fn refused_for_its_shape(a: &Answer) -> bool {
    a.status == 400 && problem_type(a) == urn("invalid-body")
}

/// What the handler said, in a line for a failure message.
fn said(a: &Answer) -> String {
    format!("{}: {}", a.status, a.text())
}

#[tokio::test]
async fn every_write_body_has_examples_and_each_is_valid_for_its_schema_and_taken_by_its_handler() {
    let c = cast();
    let document = get(&c.w, "/-/api/v1/openapi.json").await.json();
    let schemas = request_schemas(&document);
    // Every write of the document is covered, and the ones that matter are among them.
    for name in [
        "NewProposal",
        "ProposalEdit",
        "Empty",
        "Reason",
        "NewComment",
        "NewDecision",
        "NewCuration",
    ] {
        assert!(schemas.contains_key(name), "{name}: {schemas:?}");
    }
    for (name, paths) in &schemas {
        let examples = document
            .at(&format!("/components/schemas/{name}/examples"))
            .as_array()
            .unwrap_or_else(|| panic!("{name} ({paths:?}) has no `examples`"));
        assert!(!examples.is_empty(), "{name} has no example");
        for (i, example) in examples.iter().enumerate() {
            let mut problems = Vec::new();
            Checker::for_request(&document).check(
                &json!({"$ref": format!("#/components/schemas/{name}")}),
                example,
                "$",
                &mut problems,
            );
            assert!(
                problems.is_empty(),
                "{name} example {i} is not valid for its schema: {problems:?}: {example}"
            );
            let answer = through_handler(name, example).await;
            assert!(
                (200..300).contains(&answer.status.as_u16()),
                "{name} example {i} is not taken by its handler ({}): {example}",
                said(&answer)
            );
        }
    }
}

/// One body, with whether the shape is one the handler takes.
struct Shape {
    what: &'static str,
    body: Value,
    taken: bool,
}

fn yes(what: &'static str, body: Value) -> Shape {
    Shape {
        what,
        body,
        taken: true,
    }
}

fn no(what: &'static str, body: Value) -> Shape {
    Shape {
        what,
        body,
        taken: false,
    }
}

/// A curation body of the wiki `knowledge::cast_checked` makes, with these members besides.
fn curation(more: Value) -> Value {
    let mut body = json!({"claim": claim_id(1).to_string(), "summary": "x", "submit": false});
    if let (Some(into), Value::Object(from)) = (body.as_object_mut(), more) {
        into.extend(from);
    }
    body
}

/// An id of the same layout whose version is not 7.
fn other_version(id: &str) -> String {
    assert_eq!(id.get(14..15), Some("7"), "{id}");
    format!(
        "{}4{}",
        id.get(..14).unwrap_or(""),
        id.get(15..).unwrap_or("")
    )
}

/// The shapes of a curation body: what a link, a source and a reference may and may not say.
fn curation_shapes() -> Vec<Shape> {
    let (one, two) = (source_id(1).to_string(), source_id(2).to_string());
    let link = |l: Value| curation(json!({"links": [l]}));
    let source = |s: Value| curation(json!({"sources": [s]}));
    vec![
        yes("the bare body", curation(json!({}))),
        yes("a status", curation(json!({"status": "accepted"}))),
        yes("an empty summary", curation(json!({"summary": ""}))),
        yes(
            "a view and a confirmation",
            curation(json!({"view": "main", "confirm": true})),
        ),
        yes(
            "no link and no source",
            curation(json!({"links": [], "sources": []})),
        ),
        yes(
            "a link that is changed",
            link(json!({"source": two, "relation": "supports", "bearing": "indirect"})),
        ),
        yes(
            "a link that is changed, with `remove` false",
            link(
                json!({"source": two, "relation": "challenges", "bearing": "partial", "remove": false}),
            ),
        ),
        yes(
            "a link that goes",
            link(json!({"source": two, "remove": true})),
        ),
        yes("a source with only its id", source(json!({"source": one}))),
        yes(
            "a source with only a day",
            source(json!({"source": one, "reviewed": "2026-11-02"})),
        ),
        yes(
            "a source with only an answer",
            source(json!({"source": one, "availability": "unavailable"})),
        ),
        yes(
            "a source with only an author",
            source(json!({"source": one, "author": "Noor Aziz"})),
        ),
        yes(
            "a source with only a title",
            source(json!({"source": one, "title": "Another"})),
        ),
        yes(
            "a source with the day and the answer cleared",
            source(json!({"source": one, "reviewed": "", "availability": "unknown"})),
        ),
        yes(
            "a source with a reference that is cleared",
            source(json!({"source": one, "reference": {"type": "url", "value": ""}})),
        ),
        yes(
            "a source with a reference",
            source(json!({"source": one, "reference": {"type": "doi", "value": "10.1000/182"}})),
        ),
        yes(
            "a source that nothing replaces",
            source(json!({"source": one, "superseded_by": ""})),
        ),
        yes(
            "a source that another replaces",
            source(json!({"source": one, "superseded_by": two})),
        ),
        yes(
            "a source not yet checked, corrected",
            source(json!({"source": two, "title": "A title"})),
        ),
        no("no claim", json!({"summary": "x", "submit": false})),
        no(
            "no summary",
            json!({"claim": claim_id(1).to_string(), "submit": false}),
        ),
        no(
            "no choice",
            json!({"claim": claim_id(1).to_string(), "summary": "x"}),
        ),
        no("a claim that is no id", curation(json!({"claim": "nope"}))),
        no("a member nobody knows", curation(json!({"score": 5}))),
        no("a status nobody knows", curation(json!({"status": "true"}))),
        no(
            "a confirmation that is no flag",
            curation(json!({"confirm": "yes"})),
        ),
        no("a link with only its source", link(json!({"source": two}))),
        no(
            "a link with only a relation",
            link(json!({"source": two, "relation": "supports"})),
        ),
        no(
            "a link with only a bearing",
            link(json!({"source": two, "bearing": "direct"})),
        ),
        no(
            "a link that is not let go",
            link(json!({"source": two, "remove": false})),
        ),
        no(
            "a link that goes and bears",
            link(
                json!({"source": two, "remove": true, "relation": "supports", "bearing": "direct"}),
            ),
        ),
        no(
            "a link that goes and has a relation",
            link(json!({"source": two, "remove": true, "relation": "supports"})),
        ),
        no(
            "a link that goes and has a bearing",
            link(json!({"source": two, "remove": true, "bearing": "direct"})),
        ),
        no(
            "a link that goes, `remove` no flag",
            link(json!({"source": two, "remove": "yes"})),
        ),
        no(
            "a link with no source",
            link(json!({"relation": "supports", "bearing": "direct"})),
        ),
        no(
            "a link whose source is no id",
            link(json!({"source": "the second", "relation": "supports", "bearing": "direct"})),
        ),
        no(
            "a bearing nobody knows",
            link(json!({"source": two, "relation": "supports", "bearing": "very"})),
        ),
        no(
            "a link with a member nobody knows",
            link(json!({"source": two, "relation": "supports", "bearing": "direct", "note": "x"})),
        ),
        no(
            "more links than a body may hold",
            curation(
                json!({"links": (0..257).map(|_| json!({"source": two, "remove": true})).collect::<Vec<_>>()}),
            ),
        ),
        no(
            "a source with no id",
            source(json!({"reviewed": "2026-11-02"})),
        ),
        no("a source whose id is no id", source(json!({"source": "x"}))),
        no(
            "a day that is no text",
            source(json!({"source": one, "reviewed": 20261102})),
        ),
        no(
            "an answer nobody knows",
            source(json!({"source": one, "availability": "maybe"})),
        ),
        no(
            "a title that is no text",
            source(json!({"source": one, "title": 5})),
        ),
        no(
            "a source with a member nobody knows",
            source(json!({"source": one, "kind": "primary"})),
        ),
        no(
            "a reference with no value",
            source(json!({"source": one, "reference": {"type": "doi"}})),
        ),
        no(
            "a reference with no type",
            source(json!({"source": one, "reference": {"value": "x"}})),
        ),
        no(
            "a reference of a type nobody knows",
            source(json!({"source": one, "reference": {"type": "fax", "value": "1"}})),
        ),
        no(
            "a reference with a member nobody knows",
            source(json!({"source": one, "reference": {"type": "doi", "value": "1", "note": "x"}})),
        ),
        no(
            "a source replaced by no id",
            source(json!({"source": one, "superseded_by": "the later one"})),
        ),
        // The handler trims the replacement (K68): white space around an id is the id, and white space alone is none.
        yes(
            "a source replaced by an id with white space around it",
            source(json!({"source": one, "superseded_by": format!(" \t{two}\n")})),
        ),
        yes(
            "a source that white space says nothing replaces",
            source(json!({"source": one, "superseded_by": " \n"})),
        ),
        no(
            "a source replaced by an id with white space inside it",
            source(json!({"source": one, "superseded_by": two.to_string().replacen('-', " ", 1)})),
        ),
        // Ids are in lower case and of version 7, as the format has them (`format: uuid` is not enough).
        no(
            "a source replaced by an id in upper case",
            source(
                json!({"source": one, "superseded_by": source_id(0xab).to_string().to_uppercase()}),
            ),
        ),
        no(
            "a source replaced by an id of another version",
            source(json!({"source": one, "superseded_by": other_version(&two.to_string())})),
        ),
        no(
            "a source in upper case",
            source(json!({"source": source_id(0xab).to_string().to_uppercase()})),
        ),
        no(
            "a source of another version",
            source(json!({"source": other_version(&one.to_string())})),
        ),
        no(
            "a link's source in upper case",
            link(
                json!({"source": source_id(0xab).to_string().to_uppercase(), "relation": "supports", "bearing": "direct"}),
            ),
        ),
        no(
            "a link's source of another version",
            link(json!({"source": other_version(&two.to_string()), "remove": true})),
        ),
        no(
            "a claim in upper case",
            curation(json!({"claim": claim_id(0xab).to_string().to_uppercase()})),
        ),
        no(
            "a claim of another version",
            curation(json!({"claim": other_version(&claim_id(1).to_string())})),
        ),
    ]
}

fn shapes_of(schema: &str) -> Vec<Shape> {
    let page = |text: &str| json!({"title": "Notes", "text": text});
    match schema {
        "NewCuration" => curation_shapes(),
        "NewProposal" => vec![
            yes(
                "a summary and a choice",
                json!({"summary": "x", "submit": false}),
            ),
            yes(
                "a page that exists, with its new text",
                json!({"summary": "x", "submit": false, "files": [{"path": "pages/Home.md", "text": better()}]}),
            ),
            yes(
                "a page that exists, removed",
                json!({"summary": "x", "submit": false, "files": [{"path": "pages/Guide.md", "text": null}]}),
            ),
            yes(
                "a page that does not exist yet",
                json!({"summary": "x", "submit": false, "files": [page("Words.\n")]}),
            ),
            yes(
                "a rename and a drop",
                json!({"summary": "x", "submit": false, "renames": [{"from": "pages/Guide.md", "to": "pages/Manual.md"}], "drops": [{"path": "pages/Home.md", "marker": "b1"}]}),
            ),
            no(
                "a rename with a member nobody knows",
                json!({"summary": "x", "submit": false, "renames": [{"from": "pages/Guide.md", "to": "pages/Manual.md", "keep": true}]}),
            ),
            no(
                "a drop with a member nobody knows",
                json!({"summary": "x", "submit": false, "drops": [{"path": "pages/Home.md", "marker": "b1", "why": "x"}]}),
            ),
            no("no summary", json!({"submit": false})),
            no("no choice", json!({"summary": "x"})),
            no(
                "a summary that is no text",
                json!({"summary": 7, "submit": false}),
            ),
            no(
                "a choice that is no flag",
                json!({"summary": "x", "submit": "no"}),
            ),
            no(
                "a view that is no text",
                json!({"summary": "x", "submit": false, "view": 1}),
            ),
            no(
                "a member nobody knows",
                json!({"summary": "x", "submit": false, "score": 5}),
            ),
            no(
                "a page with a path and a title",
                json!({"summary": "x", "submit": false, "files": [{"path": "pages/Home.md", "title": "Home", "text": "x"}]}),
            ),
            no(
                "a page with neither",
                json!({"summary": "x", "submit": false, "files": [{"text": "x"}]}),
            ),
            no(
                "a page with no text",
                json!({"summary": "x", "submit": false, "files": [{"path": "pages/Home.md"}]}),
            ),
            no(
                "a page that does not exist yet and is removed",
                json!({"summary": "x", "submit": false, "files": [{"title": "Notes", "text": null}]}),
            ),
            no(
                "a page with a member nobody knows",
                json!({"summary": "x", "submit": false, "files": [{"path": "pages/Home.md", "text": "x", "mode": "raw"}]}),
            ),
            no(
                "a rename with no destination",
                json!({"summary": "x", "submit": false, "renames": [{"from": "pages/Guide.md"}]}),
            ),
            no(
                "a drop with no marker",
                json!({"summary": "x", "submit": false, "drops": [{"path": "pages/Home.md"}]}),
            ),
            no(
                "more pages than a proposal may hold",
                json!({"summary": "x", "submit": false, "files": (0..21).map(|i| json!({"title": format!("Page {i}"), "text": "x"})).collect::<Vec<_>>()}),
            ),
        ],
        "ProposalEdit" => vec![
            yes("a summary", json!({"summary": "x"})),
            yes(
                "a summary and pages",
                json!({"summary": "x", "files": [page("Words.\n")], "minor": true, "automated": false}),
            ),
            no("nothing", json!({})),
            no(
                "a choice, which only an opening has",
                json!({"summary": "x", "submit": true}),
            ),
            no(
                "a view, which only an opening has",
                json!({"summary": "x", "view": "main"}),
            ),
            no("a summary that is no text", json!({"summary": ["x"]})),
        ],
        "Empty" => vec![yes("no member", json!({})), no("a member", json!({"x": 1}))],
        "Reason" => vec![
            yes("a reason", json!({"reason": "x"})),
            no("no reason", json!({})),
            no("a reason that is no text", json!({"reason": 7})),
            no("a member nobody knows", json!({"reason": "x", "why": 1})),
        ],
        "NewComment" => vec![
            yes("a body", json!({"body": "x"})),
            yes(
                "a body and the part it is about",
                json!({"body": "x", "anchor": {"page": "pages/Home.md", "ordinal": 0}}),
            ),
            yes(
                "a body and a comment it answers",
                json!({"body": "x", "parent": "not-a-comment"}),
            ),
            no("no body", json!({})),
            no("a member nobody knows", json!({"body": "x", "score": 5})),
            no("a body that is no text", json!({"body": 1})),
            no(
                "an anchor with no position",
                json!({"body": "x", "anchor": {"page": "pages/Home.md"}}),
            ),
            no(
                "an anchor with no page",
                json!({"body": "x", "anchor": {"ordinal": 0}}),
            ),
            no(
                "a position below the first",
                json!({"body": "x", "anchor": {"page": "pages/Home.md", "ordinal": -1}}),
            ),
            no(
                "a position that is no whole number",
                json!({"body": "x", "anchor": {"page": "pages/Home.md", "ordinal": 1.5}}),
            ),
            no(
                "an anchor with a member nobody knows",
                json!({"body": "x", "anchor": {"page": "pages/Home.md", "ordinal": 0, "block": "b"}}),
            ),
            no(
                "an anchor that is no object",
                json!({"body": "x", "anchor": null}),
            ),
        ],
        "NewDecision" => vec![
            yes("a decision", json!({"decision": "approve"})),
            yes(
                "a decision and words",
                json!({"decision": "request-changes", "body": "x"}),
            ),
            no("no decision", json!({})),
            no(
                "a member nobody knows",
                json!({"decision": "approve", "score": 5}),
            ),
            no("a decision nobody knows", json!({"decision": "maybe"})),
            no(
                "words that are no text",
                json!({"decision": "approve", "body": 5}),
            ),
        ],
        other => panic!("no shapes written for {other}"),
    }
}

#[tokio::test]
async fn the_schema_and_the_handler_agree_on_the_shapes_of_every_write_body() {
    let c = cast();
    let document = get(&c.w, "/-/api/v1/openapi.json").await.json();
    let mut disagreements = Vec::new();
    for name in request_schemas(&document).keys() {
        for shape in shapes_of(name) {
            let mut problems = Vec::new();
            Checker::for_request(&document).check(
                &json!({"$ref": format!("#/components/schemas/{name}")}),
                &shape.body,
                "$",
                &mut problems,
            );
            let answer = through_handler(name, &shape.body).await;
            let by_schema = problems.is_empty();
            let by_handler = !refused_for_its_shape(&answer);
            if by_schema != shape.taken || by_handler != shape.taken {
                disagreements.push(format!(
                    "{name}, {}: meant to be {}; the schema says {} {problems:?}; the handler says {}",
                    shape.what,
                    if shape.taken { "taken" } else { "refused" },
                    if by_schema { "valid" } else { "invalid" },
                    said(&answer),
                ));
            }
            if shape.taken {
                // Taken is not a refusal by someone other than the body's reader: it got to the content.
                assert!(
                    ![401, 403, 404, 415].contains(&answer.status.as_u16()),
                    "{name}, {}: not read: {}",
                    shape.what,
                    said(&answer)
                );
            }
        }
    }
    assert!(disagreements.is_empty(), "{disagreements:#?}");
}

/// The pointers of every schema object under `value` that says `additionalProperties: false`.
fn closed_places(value: &Value, at: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if map.get("additionalProperties") == Some(&Value::Bool(false)) {
                out.push(at.to_owned());
            }
            for (key, inner) in map {
                closed_places(
                    inner,
                    &format!("{at}/{}", key.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        Value::Array(items) => {
            for (i, inner) in items.iter().enumerate() {
                closed_places(inner, &format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// The names of the schemas a request schema is made of: itself, and every schema it refers to, and so on.
fn made_of(document: &Value, name: &str, found: &mut std::collections::BTreeSet<String>) {
    if !found.insert(name.to_owned()) {
        return;
    }
    let mut refs = Vec::new();
    fn collect(value: &Value, refs: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(r) = map.get("$ref").and_then(Value::as_str) {
                    refs.push(r.rsplit('/').next().unwrap_or("").to_owned());
                }
                map.values().for_each(|v| collect(v, refs));
            }
            Value::Array(items) => items.iter().for_each(|v| collect(v, refs)),
            _ => {}
        }
    }
    collect(
        document.at(&format!("/components/schemas/{name}")),
        &mut refs,
    );
    for r in refs {
        made_of(document, &r, found);
    }
}

#[tokio::test]
async fn a_request_schema_that_stops_being_closed_is_seen_by_a_row_the_schema_then_takes() {
    // K68: the walk treats an object as open unless `additionalProperties` is `false`, so every place a request schema
    // is closed (`closed()`) must be one a row refuses a member at. Each place is opened in turn, in a copy of the
    // document, and some row that is refused must then be valid for the schema: a place no row sees would be a
    // closedness nobody tests.
    let c = cast();
    let document = get(&c.w, "/-/api/v1/openapi.json").await.json();
    let requests = request_schemas(&document);
    let mut names = std::collections::BTreeSet::new();
    for name in requests.keys() {
        made_of(&document, name, &mut names);
    }
    let mut places = Vec::new();
    for name in &names {
        closed_places(
            document.at(&format!("/components/schemas/{name}")),
            &format!("/components/schemas/{name}"),
            &mut places,
        );
    }
    assert!(
        places.len() >= 12,
        "the request schemas are closed in {places:?}"
    );
    // The rows that are refused, with the schema they are the body of.
    let refused: Vec<(String, Shape)> = requests
        .keys()
        .flat_map(|name| {
            shapes_of(name)
                .into_iter()
                .filter(|s| !s.taken)
                .map(|s| (name.clone(), s))
        })
        .collect();
    let valid_for = |document: &Value, name: &str, body: &Value| {
        let mut problems = Vec::new();
        Checker::for_request(document).check(
            &json!({"$ref": format!("#/components/schemas/{name}")}),
            body,
            "$",
            &mut problems,
        );
        problems.is_empty()
    };
    let mut unseen = Vec::new();
    for place in &places {
        let mut opened = document.clone();
        let target = opened
            .pointer_mut(place)
            .and_then(Value::as_object_mut)
            .expect("a schema object");
        target.remove("additionalProperties");
        let seen = refused
            .iter()
            .any(|(name, shape)| valid_for(&opened, name, &shape.body));
        if !seen {
            unseen.push(place.clone());
        }
    }
    assert!(
        unseen.is_empty(),
        "closed, and no row would notice if it were not: {unseen:#?}"
    );
}

#[tokio::test]
async fn a_reason_is_counted_in_bytes_as_the_document_says_and_the_handler_does() {
    // K68: JSON Schema's `maxLength` counts characters and the handler counts bytes (at most 4096). The schema says bytes
    // in words, and keeps `maxLength` as the first check; the rows show where they part.
    let c = cast();
    let document = get(&c.w, "/-/api/v1/openapi.json").await.json();
    let description = document.at("/components/schemas/Reason/properties/reason/description");
    assert!(
        description
            .as_str()
            .is_some_and(|d| d.contains("4096 bytes")),
        "{description}"
    );
    assert_eq!(
        document.at("/components/schemas/Reason/properties/reason/maxLength"),
        4096
    );
    let by_schema = |reason: &str| {
        let mut problems = Vec::new();
        Checker::for_request(&document).check(
            &json!({"$ref": "#/components/schemas/Reason"}),
            &json!({"reason": reason}),
            "$",
            &mut problems,
        );
        problems.is_empty()
    };
    for (what, reason, schema_takes, handler_takes) in [
        ("4096 one-byte characters", "x".repeat(4096), true, true),
        ("4097 one-byte characters", "x".repeat(4097), false, false),
        ("2048 two-byte characters", "é".repeat(2048), true, true),
        // Fewer characters than the bound, more bytes than it: the schema's first check passes, the handler refuses.
        ("2049 two-byte characters", "é".repeat(2049), true, false),
        ("1366 three-byte characters", "あ".repeat(1366), true, false),
    ] {
        assert_eq!(by_schema(&reason), schema_takes, "{what}");
        let answer = through_handler("Reason", &json!({"reason": reason})).await;
        assert_eq!(
            (200..300).contains(&answer.status.as_u16()),
            handler_takes,
            "{what}: {}",
            said(&answer)
        );
        if !handler_takes {
            assert_eq!(answer.json().at("/reason"), "reason-too-long", "{what}");
        }
    }
}
