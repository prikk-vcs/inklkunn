//! The shapes of the answers against the shapes the OpenAPI document promises, both ways: every answer the
//! server gives for these requests validates against the schema its operation names in the document (types,
//! enums, required members, no member the schema does not declare, array bounds), and every member a schema
//! declares is seen in some answer. A client generated from the document is therefore a client of the server.

use std::collections::{BTreeMap, BTreeSet};

use inklkunn_web::api::OPENAPI_PATH;
use serde_json::{Value, json};

use super::common::{World, block_id, diff_of, op, shown};
use super::get;
use super::{across, past};

/// A subset of JSON Schema, enough for the document this project writes.
struct Checker<'a> {
    document: &'a Value,
    /// Members seen, by schema name.
    seen: BTreeMap<String, BTreeSet<String>>,
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

impl<'a> Checker<'a> {
    fn check(&mut self, schema: &Value, value: &Value, at: &str, problems: &mut Vec<String>) {
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let Some(target) = reference
                .strip_prefix('#')
                .and_then(|p| self.document.pointer(p))
            else {
                problems.push(format!("{at}: {reference} does not resolve"));
                return;
            };
            let name = reference.rsplit('/').next().unwrap_or("").to_owned();
            let target = target.clone();
            self.check_named(&name, &target, value, at, problems);
            return;
        }
        self.check_named("", schema, value, at, problems);
    }

    fn check_named(
        &mut self,
        name: &str,
        schema: &Value,
        value: &Value,
        at: &str,
        problems: &mut Vec<String>,
    ) {
        if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
            let mut matching = 0;
            for variant in variants {
                let mut inner = Vec::new();
                let mut trial = Checker {
                    document: self.document,
                    seen: BTreeMap::new(),
                };
                trial.check(variant, value, at, &mut inner);
                if inner.is_empty() {
                    matching += 1;
                    for (k, v) in trial.seen {
                        self.seen.entry(k).or_default().extend(v);
                    }
                }
            }
            if matching != 1 {
                problems.push(format!("{at}: {matching} variants of oneOf match {value}"));
            }
            return;
        }
        let actual = kind_of(value);
        if let Some(types) = schema.get("type") {
            let allowed: Vec<&str> = match types {
                Value::String(t) => vec![t.as_str()],
                Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            let fits = allowed
                .iter()
                .any(|t| *t == actual || (*t == "number" && actual == "integer"));
            if !fits {
                problems.push(format!("{at}: {actual} where {allowed:?}"));
                return;
            }
        }
        if let Some(words) = schema.get("enum").and_then(Value::as_array)
            && !words.contains(value)
        {
            problems.push(format!("{at}: {value} is not one of {words:?}"));
        }
        if let (Some(min), Some(n)) = (
            schema.get("minimum").and_then(Value::as_i64),
            value.as_i64(),
        ) && n < min
        {
            problems.push(format!("{at}: {n} is below {min}"));
        }
        match value {
            Value::Object(members) => {
                let declared = schema.get("properties").and_then(Value::as_object);
                let required: Vec<&str> = schema
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                for r in &required {
                    if !members.contains_key(*r) {
                        problems.push(format!("{at}: required member {r} is missing"));
                    }
                }
                if let Some(declared) = declared {
                    for (key, inner) in members {
                        match declared.get(key) {
                            Some(schema) => {
                                if !name.is_empty() {
                                    self.seen
                                        .entry(name.to_owned())
                                        .or_default()
                                        .insert(key.clone());
                                }
                                self.check(schema, inner, &format!("{at}/{key}"), problems);
                            }
                            None => problems.push(format!("{at}: {key} is not declared")),
                        }
                    }
                }
            }
            Value::Array(items) => {
                if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
                    && items.len() as u64 > max
                {
                    problems.push(format!("{at}: {} items, at most {max}", items.len()));
                }
                if let Some(schema) = schema.get("items") {
                    for (i, item) in items.iter().enumerate() {
                        self.check(schema, item, &format!("{at}/{i}"), problems);
                    }
                }
            }
            _ => {}
        }
    }
}

fn schema_of_operation(document: &Value, template: &str) -> Value {
    let key = template.replace('/', "~1");
    document
        .pointer(&format!(
            "/paths/{key}/get/responses/200/content/application~1json/schema"
        ))
        .cloned()
        .unwrap_or_else(|| panic!("{template} has no JSON answer in the document"))
}

/// The worlds and requests that give every shape its members: the route template of each (to find its schema in
/// the document), the request, and the answer.
async fn samples() -> Vec<(String, String, Value)> {
    let route = |tail: &str| format!("/{{slug}}/-/api/v1/{tail}");
    let b = block_id;
    let mut results = Vec::new();

    // The handbook with a scripted past, and a move and a removal so that every event and thing shows.
    let w = past::handbook();
    w.fake.set_files(&[(
        "pages/Start.md",
        &super::common::page(&["title: Start"], "Moved here.\n"),
    )]);
    w.fake.say(5, "Move the home page");
    w.fake.show(
        5,
        shown(vec![
            op("rename-path", &["pages/Home.md", "pages/Start.md"]),
            op("delete-node", &["pages/Guide.md"]),
        ]),
    );
    let asked = [
        ("history/{title}", "history/Home".to_owned()),
        ("history/{title}", "history/Start".to_owned()),
        ("history/{title}", "history/Guide".to_owned()),
        ("revisions/{id}", format!("revisions/{}", b(3))),
        ("revisions/{id}", format!("revisions/{}", b(5))),
        ("revisions/{id}", format!("revisions/{}?title=Home", b(2))),
        ("diff", format!("diff?path=Home&from={}&to={}", b(1), b(2))),
        ("pages", "pages?limit=2".to_owned()),
        ("pages/{title}", "pages/Home".to_owned()),
        ("raw/{title}", "raw/Home".to_owned()),
    ];
    for (template, tail) in asked {
        let a = get(&w, &format!("/handbook/-/api/v1/{tail}")).await;
        results.push((route(template), tail, a.json()));
    }
    // A hunk that is not in the shape of a unified hunk is text.
    w.fake.diff_says(Ok(diff_of(
        "pages/Home.md",
        "modified",
        &["not a hunk\n"],
        1,
        2,
    )));
    let tail = format!("diff?path=Home&from={}&to={}", b(1), b(2));
    let a = get(&w, &format!("/handbook/-/api/v1/{tail}")).await;
    results.push((route("diff"), tail, a.json()));

    // The wiki of links, and the changes.
    let w = across::wiki();
    w.fake.tip_message("Touch the Home page");
    w.fake.touches(&b(1), &["pages/Home.md"]);
    let mut asked = vec![
        ("backlinks/{title}", "backlinks/Home".to_owned()),
        ("backlinks/{title}", "backlinks/Guide".to_owned()),
        ("backlinks/{title}", "backlinks/Missing%20Page".to_owned()),
        ("search", "search?q=cafe".to_owned()),
        ("search", "search?q=faq".to_owned()),
        ("search", "search?q=zzzz".to_owned()),
    ];
    for list in [
        "wanted",
        "orphans",
        "dead-ends",
        "redirects",
        "wants-help",
        "stale",
    ] {
        asked.push(("health", format!("health?list={list}&limit=2")));
    }
    for (template, tail) in asked {
        let a = get(&w, &format!("/handbook/-/api/v1/{tail}")).await;
        results.push((route(template), tail, a.json()));
    }
    let w = across::handbook();
    for tail in ["changes", "changes?limit=1", "changes?view=draft"] {
        let a = get(&w, &format!("/handbook/-/api/v1/{tail}")).await;
        results.push((route("changes"), tail.to_owned(), a.json()));
    }
    results
}

#[tokio::test]
async fn every_answer_is_of_the_shape_the_document_promises_and_every_declared_member_is_seen() {
    let w = World::new("handbook", "The Handbook", &[]);
    let document = get(&w, OPENAPI_PATH).await.json();
    let mut checker = Checker {
        document: &document,
        seen: BTreeMap::new(),
    };
    let mut problems = Vec::new();
    let answers = samples().await;
    assert!(answers.len() >= 25, "{}", answers.len());
    for (template, tail, body) in &answers {
        assert!(
            body.get("type")
                .and_then(Value::as_str)
                .is_none_or(|t| !t.starts_with("urn:")),
            "{tail} answered a problem: {body}"
        );
        let schema = schema_of_operation(&document, template);
        checker.check(&schema, body, tail, &mut problems);
    }
    assert!(problems.is_empty(), "{problems:#?}");
    // Every member a schema declares was in some answer, but for the ones that need a state no sample
    // reaches (an unreadable page, a page with a declared language).
    let not_reached: BTreeSet<(&str, &str)> = BTreeSet::from([
        ("ListedPage", "problem"),
        ("Page", "lang"),
        ("Page", "redirected_from"),
    ]);
    let schemas = document
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .unwrap();
    let covered = [
        "History",
        "HistoryRow",
        "PageEvent",
        "Revision",
        "Did",
        "Subject",
        "Diff",
        "Hunk",
        "DiffLine",
        "Segment",
        "Backlinks",
        "Backlink",
        "Health",
        "HealthItem",
        "Search",
        "SearchHit",
        "Changes",
        "ChangeRow",
        "ChangePage",
        "Provenance",
        "Person",
    ];
    for name in covered {
        let declared = schemas
            .get(name)
            .and_then(|s| s.get("properties"))
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{name} is not declared"));
        let seen = checker.seen.get(name).cloned().unwrap_or_default();
        for member in declared.keys() {
            if not_reached.contains(&(name, member.as_str())) {
                continue;
            }
            assert!(
                seen.contains(member),
                "{name}.{member} is declared and no answer has it (seen: {seen:?})"
            );
        }
    }
}

#[test]
fn the_checker_can_fail_a_member_nobody_declared_a_wrong_type_and_a_missing_member() {
    let document = json!({"components": {"schemas": {"S": {
        "type": "object", "required": ["a"],
        "properties": {"a": {"type": "string", "enum": ["x", "y"]}, "b": {"type": ["integer", "null"], "minimum": 1}},
    }}}});
    let schema = json!({"$ref": "#/components/schemas/S"});
    let mut checker = Checker {
        document: &document,
        seen: BTreeMap::new(),
    };
    let mut check = |value: Value| {
        let mut problems = Vec::new();
        checker.check(&schema, &value, "v", &mut problems);
        problems
    };
    assert!(check(json!({"a": "x", "b": null})).is_empty());
    assert!(!check(json!({"b": 1})).is_empty(), "missing");
    assert!(!check(json!({"a": "z"})).is_empty(), "enum");
    assert!(!check(json!({"a": "x", "c": 1})).is_empty(), "undeclared");
    assert!(!check(json!({"a": 1})).is_empty(), "type");
    assert!(!check(json!({"a": "x", "b": 0})).is_empty(), "minimum");
}
