//! The OpenAPI document (RFC 003 D-13; API-01): that it is a valid OpenAPI 3.1 document in the shape this
//! project relies on, that it is generated from the router (a drift test both ways), and that every endpoint
//! is in the route registry with its action.

use std::collections::BTreeSet;

use axum::http::StatusCode;
use inklkunn_core::{Action, Guard};
use inklkunn_web::api::{OPENAPI_MEDIA_TYPE, OPENAPI_PATH};
use inklkunn_web::routes::application_table;
use serde_json::{Value, json};

use super::{Answer, At, get, handbook_world, put};

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

const SCHEMA_TYPES: [&str; 7] = [
    "string", "number", "integer", "boolean", "array", "object", "null",
];

fn document_of(a: &Answer) -> Value {
    a.json()
}

/// Resolve a local `$ref` (`#/components/schemas/X`) in `document`.
fn resolve<'a>(document: &'a Value, reference: &str) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    document.pointer(pointer)
}

fn collect_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if key == "$ref" {
                    if let Some(text) = inner.as_str() {
                        out.push(text.to_owned());
                    }
                } else {
                    collect_refs(inner, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|i| collect_refs(i, out)),
        _ => {}
    }
}

fn placeholders(path: &str) -> BTreeSet<String> {
    path.split('/')
        .filter_map(|s| s.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
        .map(str::to_owned)
        .collect()
}

fn check_schema(schema: &Value, at: &str, problems: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        problems.push(format!("{at}: a schema is an object"));
        return;
    };
    if object.contains_key("$ref") {
        return;
    }
    match object.get("type") {
        None => {}
        Some(Value::String(t)) if SCHEMA_TYPES.contains(&t.as_str()) => {}
        Some(Value::Array(types))
            if !types.is_empty()
                && types
                    .iter()
                    .all(|t| t.as_str().is_some_and(|t| SCHEMA_TYPES.contains(&t))) => {}
        Some(other) => problems.push(format!("{at}: type {other}")),
    }
    if object.get("type").and_then(Value::as_str) == Some("array") && !object.contains_key("items")
    {
        problems.push(format!("{at}: an array schema has items"));
    }
    if let Some(items) = object.get("items") {
        check_schema(items, &format!("{at}/items"), problems);
    }
    if let Some(Value::Object(properties)) = object.get("properties") {
        for (name, inner) in properties {
            check_schema(inner, &format!("{at}/properties/{name}"), problems);
        }
        if let Some(required) = object.get("required") {
            match required.as_array() {
                Some(names) => {
                    for name in names {
                        if !name.as_str().is_some_and(|n| properties.contains_key(n)) {
                            problems.push(format!("{at}: required {name} is not a property"));
                        }
                    }
                }
                None => problems.push(format!("{at}: required is an array")),
            }
        }
    }
    if let (Some(min), Some(max)) = (
        object.get("minimum").and_then(Value::as_i64),
        object.get("maximum").and_then(Value::as_i64),
    ) && min > max
    {
        problems.push(format!("{at}: minimum above maximum"));
    }
}

fn is_media_type(name: &str) -> bool {
    let essence = name.split(';').next().unwrap_or("");
    matches!(essence.split_once('/'), Some((a, b)) if !a.is_empty() && !b.is_empty() && !b.contains('/'))
}

/// The structural rules of OpenAPI 3.1 that this project relies on, as a list of what is wrong.
fn validate(document: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(root) = document.as_object() else {
        return vec!["the document is an object".to_owned()];
    };
    if !root
        .get("openapi")
        .and_then(Value::as_str)
        .is_some_and(|v| v.starts_with("3.1."))
    {
        problems.push("openapi is a 3.1.x version string".to_owned());
    }
    match root.get("info").and_then(Value::as_object) {
        Some(info) => {
            for field in ["title", "version"] {
                let text = info.get(field).and_then(Value::as_str).unwrap_or("");
                if text.is_empty() {
                    problems.push(format!("info.{field} is a non-empty string"));
                }
            }
        }
        None => problems.push("info is an object".to_owned()),
    }
    let declared_tags: BTreeSet<&str> = root
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| tags.iter().filter_map(|t| t.at("/name").as_str()).collect())
        .unwrap_or_default();
    let Some(paths) = root.get("paths").and_then(Value::as_object) else {
        problems.push("paths is an object".to_owned());
        return problems;
    };
    if paths.is_empty() {
        problems.push("paths is not empty".to_owned());
    }
    let mut operation_ids = BTreeSet::new();
    for (path, item) in paths {
        if !path.starts_with('/') {
            problems.push(format!("{path}: a path begins with /"));
        }
        let Some(item) = item.as_object() else {
            problems.push(format!("{path}: a path item is an object"));
            continue;
        };
        for (key, operation) in item {
            if key.starts_with("x-")
                || ["summary", "description", "servers", "parameters"].contains(&key.as_str())
            {
                continue;
            }
            if !METHODS.contains(&key.as_str()) {
                problems.push(format!("{path}: {key} is not an HTTP method"));
                continue;
            }
            let at = format!("{path} {key}");
            let Some(operation) = operation.as_object() else {
                problems.push(format!("{at}: an operation is an object"));
                continue;
            };
            match operation.get("operationId").and_then(Value::as_str) {
                Some(id) if operation_ids.insert(id.to_owned()) => {}
                other => problems.push(format!(
                    "{at}: operationId {other:?} is missing or repeated"
                )),
            }
            for tag in operation
                .get("tags")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if !tag.as_str().is_some_and(|t| declared_tags.contains(t)) {
                    problems.push(format!("{at}: tag {tag} is not declared"));
                }
            }
            // Parameters: named, located, path parameters required and exactly the template's.
            let mut in_path = BTreeSet::new();
            let mut seen = BTreeSet::new();
            for parameter in operation
                .get("parameters")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = parameter.at("/name").as_str().unwrap_or("");
                let place = parameter.at("/in").as_str().unwrap_or("");
                if name.is_empty() || !["query", "header", "path", "cookie"].contains(&place) {
                    problems.push(format!(
                        "{at}: parameter {parameter} lacks a name or a location"
                    ));
                    continue;
                }
                if !seen.insert((name.to_owned(), place.to_owned())) {
                    problems.push(format!("{at}: parameter {name} is repeated"));
                }
                if place == "path" {
                    in_path.insert(name.to_owned());
                    if *parameter.at("/required") != Value::Bool(true) {
                        problems.push(format!("{at}: path parameter {name} is required"));
                    }
                }
                if !parameter.get("required").is_some_and(Value::is_boolean) {
                    problems.push(format!(
                        "{at}: parameter {name} states whether it is required"
                    ));
                }
                match parameter.get("schema") {
                    Some(schema) => {
                        check_schema(schema, &format!("{at} parameter {name}"), &mut problems)
                    }
                    None => problems.push(format!("{at}: parameter {name} has a schema")),
                }
            }
            if in_path != placeholders(path) {
                problems.push(format!(
                    "{at}: path parameters {in_path:?} are not the template's {:?}",
                    placeholders(path)
                ));
            }
            // Responses.
            match operation.get("responses").and_then(Value::as_object) {
                Some(responses) if !responses.is_empty() => {
                    for (status, response) in responses {
                        let ok = status == "default"
                            || (status.len() == 3
                                && status
                                    .bytes()
                                    .next()
                                    .is_some_and(|b| (b'1'..=b'5').contains(&b))
                                && status
                                    .bytes()
                                    .skip(1)
                                    .all(|b| b.is_ascii_digit() || b == b'X'));
                        if !ok {
                            problems.push(format!("{at}: status {status}"));
                        }
                        if !response.at("/description").is_string() {
                            problems.push(format!("{at}: response {status} has a description"));
                        }
                        for (media, content) in
                            response.at("/content").as_object().into_iter().flatten()
                        {
                            if !is_media_type(media) {
                                problems.push(format!("{at}: media type {media}"));
                            }
                            if let Some(schema) = content.get("schema") {
                                check_schema(
                                    schema,
                                    &format!("{at} {status} {media}"),
                                    &mut problems,
                                );
                            }
                        }
                    }
                }
                _ => problems.push(format!("{at}: responses is not empty")),
            }
        }
    }
    // Every reference resolves.
    let mut refs = Vec::new();
    collect_refs(document, &mut refs);
    for reference in refs {
        if resolve(document, &reference).is_none() {
            problems.push(format!("{reference} does not resolve"));
        }
    }
    // Every schema of the components is well formed.
    if let Some(schemas) = document
        .pointer("/components/schemas")
        .and_then(Value::as_object)
    {
        for (name, schema) in schemas {
            check_schema(
                schema,
                &format!("#/components/schemas/{name}"),
                &mut problems,
            );
        }
    }
    problems
}

// ------------------------------------------------------------------------------------ the tests

#[tokio::test]
async fn the_description_is_served_where_it_is_documented_as_a_valid_openapi_3_1_document() {
    let w = handbook_world();
    let a = get(&w, OPENAPI_PATH).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    assert_eq!(a.header("content-type"), Some(OPENAPI_MEDIA_TYPE));
    assert!(a.headers.contains_key("etag"));
    let problems = validate(&document_of(&a));
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_validator_can_fail_planted_violations_are_found() {
    let good = json!({
        "openapi": "3.1.0",
        "info": {"title": "t", "version": "1"},
        "tags": [{"name": "a"}],
        "paths": {"/{x}/y": {"get": {
            "operationId": "one", "tags": ["a"],
            "parameters": [{"name": "x", "in": "path", "required": true, "schema": {"type": "string"}}],
            "responses": {"200": {"description": "ok", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/S"}}}}},
        }}},
        "components": {"schemas": {"S": {"type": "object", "required": ["a"], "properties": {"a": {"type": "string"}}}}},
    });
    assert_eq!(validate(&good), Vec::<String>::new());
    let item = "/paths/~1{x}~1y";
    let op = "/paths/~1{x}~1y/get";
    type Plant = Box<dyn Fn(&mut Value)>;
    let planted: Vec<(&str, Plant)> = vec![
        (
            "version",
            Box::new(|d| put(d, "", "openapi", "3.0.3".into())),
        ),
        ("no info", Box::new(|d| put(d, "", "info", Value::Null))),
        ("empty paths", Box::new(|d| put(d, "", "paths", json!({})))),
        (
            "bad ref",
            Box::new(|d| {
                put(
                    d,
                    "/paths/~1{x}~1y/get/responses/200/content/application~1json/schema",
                    "$ref",
                    "#/components/schemas/Nope".into(),
                )
            }),
        ),
        (
            "missing path param",
            Box::new(move |d| put(d, op, "parameters", json!([]))),
        ),
        (
            "path param not required",
            Box::new(|d| {
                put(
                    d,
                    "/paths/~1{x}~1y/get/parameters/0",
                    "required",
                    false.into(),
                )
            }),
        ),
        (
            "repeated operationId",
            Box::new(move |d| {
                let copy = d.pointer(item).cloned().unwrap_or(Value::Null);
                put(d, "/paths", "/{x}/z", copy);
            }),
        ),
        (
            "not a method",
            Box::new(move |d| put(d, item, "fetch", json!({}))),
        ),
        (
            "no responses",
            Box::new(move |d| put(d, op, "responses", json!({}))),
        ),
        (
            "bad status",
            Box::new(|d| {
                put(
                    d,
                    "/paths/~1{x}~1y/get/responses",
                    "20",
                    json!({"description": "x"}),
                )
            }),
        ),
        (
            "undeclared tag",
            Box::new(move |d| put(d, op, "tags", json!(["zzz"]))),
        ),
        (
            "bad type",
            Box::new(|d| put(d, "/components/schemas/S", "type", "str".into())),
        ),
        (
            "required not a property",
            Box::new(|d| put(d, "/components/schemas/S", "required", json!(["b"]))),
        ),
        (
            "path without slash",
            Box::new(move |d| {
                let copy = d.pointer(item).cloned().unwrap_or(Value::Null);
                put(d, "/paths", "x", copy);
            }),
        ),
    ];
    for (what, plant) in planted {
        let mut broken = good.clone();
        plant(&mut broken);
        assert!(
            !validate(&broken).is_empty(),
            "the validator missed: {what}"
        );
    }
}

/// The path of a documented operation with its placeholders and required parameters filled.
fn concrete(path: &str, operation: &Value) -> String {
    let mut url = path
        .replace("{slug}", "handbook")
        .replace("{title}", "Home")
        .replace("{id}", "abc123");
    let required: Vec<&str> = operation
        .at("/parameters")
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p.at("/in") == "query" && p.at("/required") == true)
        .map(|p| p.at("/name").as_str().unwrap())
        .collect();
    let values: Vec<String> = required
        .iter()
        .map(|name| match *name {
            "path" => "path=Home".to_owned(),
            "list" => "list=orphans".to_owned(),
            other => format!("{other}=x"),
        })
        .collect();
    if !values.is_empty() {
        url.push('?');
        url.push_str(&values.join("&"));
    }
    url
}

#[tokio::test]
async fn the_description_names_every_route_the_router_serves_under_the_api_and_no_other() {
    let w = handbook_world();
    let document = document_of(&get(&w, OPENAPI_PATH).await);
    let documented: BTreeSet<String> = document
        .at("/paths")
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let table = application_table().unwrap();
    let registered: BTreeSet<String> = table
        .entries()
        .iter()
        .filter(|e| e.path.contains("/-/api/") || e.path.starts_with("/-/api/"))
        .map(|e| e.path.replace("{*", "{"))
        .collect();
    // The two catch-alls answer the not-found problem for what is not an endpoint, and are not described.
    let not_endpoints: BTreeSet<String> = ["/{slug}/-/api/{rest}", "/-/api/{rest}"]
        .map(String::from)
        .into();
    let served: BTreeSet<String> = registered.difference(&not_endpoints).cloned().collect();
    assert_eq!(
        documented.difference(&served).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "described and not served"
    );
    assert_eq!(
        served.difference(&documented).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "served and not described"
    );
    assert!(documented.len() >= 12, "{documented:?}");

    // The router really serves each documented operation with an API handler (only they set the link to the
    // description), and the catch-alls and nothing else answer the rest.
    for (path, item) in document.at("/paths").as_object().unwrap() {
        let url = concrete(path, item.at("/get"));
        let a = get(&w, &url).await;
        assert!(
            a.header("link").is_some_and(|l| l.contains("service-desc")),
            "{url} is not served by an API handler: {} {}",
            a.status,
            a.text()
        );
        // An answer of the shape the document promises: a success of a stated media type, or a problem of
        // a code the document lists for that operation.
        let promised: Vec<String> = item
            .at("/get/responses")
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert!(
            promised.contains(&a.status.as_u16().to_string()),
            "{url} answered {}, the document promises {promised:?}",
            a.status
        );
    }
}

#[test]
fn every_endpoint_is_in_the_route_registry_with_its_action() {
    let table = application_table().unwrap();
    let api: Vec<_> = table
        .entries()
        .iter()
        .filter(|e| e.path.contains("/-/api/"))
        .collect();
    assert!(api.len() >= 14, "{}", api.len());
    for entry in &api {
        assert_eq!(entry.methods, ["GET", "HEAD"], "{}", entry.path);
        match entry.guard {
            Guard::Action(action) => assert_eq!(action, Action::Read, "{}", entry.path),
            Guard::Public { reason } => {
                assert!(reason.len() >= 16, "{}", entry.path);
                assert!(
                    [OPENAPI_PATH, "/{slug}/-/api/{*rest}", "/-/api/{*rest}"].contains(&entry.path),
                    "{} is public",
                    entry.path
                );
            }
        }
    }
    // Everything that reads a wiki is guarded by the read action, not declared public.
    let guarded: Vec<_> = api
        .iter()
        .filter(|e| matches!(e.guard, Guard::Action(_)))
        .collect();
    assert!(guarded.len() >= 11, "{}", guarded.len());
    assert!(
        guarded
            .iter()
            .all(|e| e.path.starts_with("/{slug}/-/api/v1/"))
    );
}

#[test]
fn no_api_path_can_shadow_a_title() {
    // Every route of the API is under the reserved `-/` after the slug (or before any slug), so no title,
    // which never begins with `-/`, can collide with one (RFC 003 D-1).
    for entry in application_table().unwrap().entries() {
        if entry.path.contains("/-/api/") || entry.path.starts_with("/-/api/") {
            let segments: Vec<&str> = entry.path.trim_start_matches('/').split('/').collect();
            let reserved = matches!(segments.as_slice(), ["-", "api", ..])
                || matches!(segments.as_slice(), [slug, "-", "api", ..] if slug.starts_with('{'));
            assert!(reserved, "{}", entry.path);
        }
    }
}

#[tokio::test]
async fn the_problem_codes_the_document_lists_are_the_ones_the_server_has() {
    let w = handbook_world();
    let document = document_of(&get(&w, OPENAPI_PATH).await);
    let listed = document.at("/x-inklkunn-problems").as_array().unwrap();
    assert!(listed.len() >= 14);
    let mut types = BTreeSet::new();
    for problem in listed {
        let code = problem.at("/code").as_str().unwrap();
        assert_eq!(
            problem.at("/type").as_str().unwrap(),
            format!("urn:inklkunn:problem:{code}")
        );
        assert!(types.insert(code.to_owned()), "{code} is listed twice");
    }
    // A code the server answers is in the list.
    for address in [
        "/handbook/-/api/v1/pages/Nowhere",
        "/handbook/-/api/v1/pages?limit=0",
        "/handbook/-/api/v1/diff",
        "/handbook/-/api/v1/health",
        "/nobody/-/api/v1/pages",
    ] {
        let a = get(&w, address).await;
        let kind = a.json().at("/type").as_str().unwrap().to_owned();
        let code = kind.strip_prefix("urn:inklkunn:problem:").unwrap();
        assert!(types.contains(code), "{address}: {code}");
    }
}
