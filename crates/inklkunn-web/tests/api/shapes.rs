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

/// A subset of JSON Schema, enough for the document this project writes: `$ref`, `type`, `enum`, `const`, `minimum`,
/// `maxLength`, `format: uuid`, `pattern` (a small subset of regular expressions: see [`pattern_matches`]), `required`,
/// `properties`, `items`, `maxItems`, `additionalProperties`, `oneOf` and `anyOf`. No JSON-schema crate is in the lock,
/// so a keyword the document uses and this does not know is a gap in this walk.
///
/// **Closedness.** An *answer* is held to a promise the document does not need to make in its schemas: no member a
/// schema with `properties` does not declare ([`Checker::new`]). A *request* is held to what its schema says
/// ([`Checker::for_request`]): as in JSON Schema, an object is **open unless `additionalProperties` is `false`**, so
/// a request schema that forgets `closed()` is seen to admit a member the handler refuses (`write/shapes.rs`).
pub(super) struct Checker<'a> {
    pub(super) document: &'a Value,
    /// Members seen, by schema name.
    pub(super) seen: BTreeMap<String, BTreeSet<String>>,
    /// The schemas are those of a request: closed only where they say so.
    requests: bool,
}

/// One thing a pattern matches, with how many times.
struct Repeated {
    ranges: Vec<(char, char)>,
    space: bool,
    min: usize,
    max: usize,
}

impl Repeated {
    fn takes(&self, c: char) -> bool {
        (self.space && c.is_whitespace()) || self.ranges.iter().any(|(a, b)| (*a..=*b).contains(&c))
    }
}

/// Parse the subset of regular expressions the document's patterns use: `^` and `$` at the ends, literal characters,
/// `\s`, an escaped character, classes such as `[0-9a-f-]` and the repetitions `{n}`, `{m,n}`, `*`, `+` and `?`. Anything
/// else is an error: this walk does not guess at a pattern it cannot read.
fn parse_pattern(pattern: &str) -> Result<Vec<Repeated>, String> {
    let body = pattern
        .strip_prefix('^')
        .and_then(|p| p.strip_suffix('$'))
        .ok_or_else(|| format!("{pattern}: not anchored at both ends"))?;
    let mut chars = body.chars().peekable();
    let mut out: Vec<Repeated> = Vec::new();
    while let Some(c) = chars.next() {
        let mut item = Repeated {
            ranges: Vec::new(),
            space: false,
            min: 1,
            max: 1,
        };
        match c {
            '[' => loop {
                let a = chars.next().ok_or("an unclosed class")?;
                if a == ']' {
                    break;
                }
                if chars.peek() == Some(&'-') {
                    chars.next();
                    match chars.next().ok_or("an unclosed class")? {
                        ']' => {
                            item.ranges.push((a, a));
                            item.ranges.push(('-', '-'));
                            break;
                        }
                        b => item.ranges.push((a, b)),
                    }
                } else {
                    item.ranges.push((a, a));
                }
            },
            '\\' => match chars.next().ok_or("a dangling escape")? {
                's' => item.space = true,
                other => item.ranges.push((other, other)),
            },
            '(' | ')' | '|' | '.' | '^' | '$' | '{' | '}' | '*' | '+' | '?' => {
                return Err(format!("{pattern}: {c} is beyond this walk"));
            }
            other => item.ranges.push((other, other)),
        }
        match chars.peek().copied() {
            Some('*') => (item.min, item.max) = (0, usize::MAX),
            Some('+') => (item.min, item.max) = (1, usize::MAX),
            Some('?') => (item.min, item.max) = (0, 1),
            Some('{') => {
                chars.next();
                let mut spelled = String::new();
                for d in chars.by_ref() {
                    if d == '}' {
                        break;
                    }
                    spelled.push(d);
                }
                let (lo, hi) = spelled.split_once(',').unwrap_or((&spelled, &spelled));
                item.min = lo
                    .parse()
                    .map_err(|_| format!("{pattern}: {{{spelled}}}"))?;
                item.max = hi
                    .parse()
                    .map_err(|_| format!("{pattern}: {{{spelled}}}"))?;
                out.push(item);
                continue;
            }
            _ => {
                out.push(item);
                continue;
            }
        }
        chars.next();
        out.push(item);
    }
    Ok(out)
}

fn matches_from(items: &[Repeated], text: &[char]) -> bool {
    let Some((item, rest)) = items.split_first() else {
        return text.is_empty();
    };
    let mut reach = 0;
    while reach < item.max && text.get(reach).is_some_and(|c| item.takes(*c)) {
        reach += 1;
    }
    (item.min..=reach).rev().any(|taken| {
        text.get(taken..)
            .is_some_and(|after| matches_from(rest, after))
    })
}

/// Does `text` match `pattern`, a regular expression of the subset [`parse_pattern`] reads (the whole text, as the
/// document's patterns are anchored)?
pub(super) fn pattern_matches(pattern: &str, text: &str) -> Result<bool, String> {
    let items = parse_pattern(pattern)?;
    let chars: Vec<char> = text.chars().collect();
    Ok(matches_from(&items, &chars))
}

/// The shape of a version-7 id in the format's words: 36 characters, lower-case hexadecimal, hyphens in their places.
fn is_uuid_shape(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        })
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
    /// A checker of an *answer* against the schemas of `document`: no member a schema with `properties` does not declare.
    pub(super) fn new(document: &'a Value) -> Self {
        Checker {
            document,
            seen: BTreeMap::new(),
            requests: false,
        }
    }

    /// A checker of a *request* against the schemas of `document`: an object is open unless its schema says
    /// `additionalProperties: false`, or gives the schema of the members it does not list.
    pub(super) fn for_request(document: &'a Value) -> Self {
        Checker {
            requests: true,
            ..Checker::new(document)
        }
    }

    /// Why `value` is not what `schema` (a schema or a `$ref` to one) describes, as paths and reasons.
    pub(super) fn check(
        &mut self,
        schema: &Value,
        value: &Value,
        at: &str,
        problems: &mut Vec<String>,
    ) {
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
        // `oneOf`: exactly one alternative holds; `anyOf`: at least one does.
        for (keyword, exactly_one) in [("oneOf", true), ("anyOf", false)] {
            let Some(variants) = schema.get(keyword).and_then(Value::as_array) else {
                continue;
            };
            let mut matching = 0;
            for variant in variants {
                let mut inner = Vec::new();
                let mut trial = Checker {
                    requests: self.requests,
                    ..Checker::new(self.document)
                };
                trial.check(variant, value, at, &mut inner);
                if inner.is_empty() {
                    matching += 1;
                    for (k, v) in trial.seen {
                        self.seen.entry(k).or_default().extend(v);
                    }
                }
            }
            if (exactly_one && matching != 1) || matching == 0 {
                problems.push(format!(
                    "{at}: {matching} variants of {keyword} match {value}"
                ));
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
        if let Some(only) = schema.get("const")
            && only != value
        {
            problems.push(format!("{at}: {value} is not {only}"));
        }
        if let Some(text) = value.as_str() {
            if schema.get("format").and_then(Value::as_str) == Some("uuid") && !is_uuid_shape(text)
            {
                problems.push(format!("{at}: not an id"));
            }
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64)
                && text.chars().count() as u64 > max
            {
                problems.push(format!("{at}: longer than {max}"));
            }
            if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
                match pattern_matches(pattern, text) {
                    Ok(true) => {}
                    Ok(false) => problems.push(format!("{at}: does not match {pattern}")),
                    Err(why) => problems.push(format!("{at}: {why}")),
                }
            }
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
                // A member the schema does not list: refused when the schema says `additionalProperties: false`
                // (and, for an answer, whenever it lists members at all); held to the schema `additionalProperties`
                // is, when it is one; otherwise free, as in JSON Schema.
                let extra = schema.get("additionalProperties");
                let closed =
                    extra == Some(&Value::Bool(false)) || (!self.requests && declared.is_some());
                for (key, inner) in members {
                    match declared.and_then(|d| d.get(key)) {
                        Some(schema) => {
                            if !name.is_empty() {
                                self.seen
                                    .entry(name.to_owned())
                                    .or_default()
                                    .insert(key.clone());
                            }
                            self.check(schema, inner, &format!("{at}/{key}"), problems);
                        }
                        None if closed => problems.push(format!("{at}: {key} is not declared")),
                        None => {
                            if let Some(rest) = extra.filter(|e| e.is_object()) {
                                self.check(rest, inner, &format!("{at}/{key}"), problems);
                            }
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
        "knowledge-files",
        "sources-to-review",
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
    // The knowledge layer: a claim judged on an earlier wording, its sources, a page's provenance, two views compared.
    let w = super::knowledge::reworded();
    let claim = inklkunn_core::knowledge::ClaimId::parse("01930000-0000-7000-8000-000000000001")
        .expect("a claim id");
    let source = inklkunn_core::knowledge::SourceId::parse("01931111-0000-7000-8000-000000000001")
        .expect("a source id");
    for (template, tail) in [
        ("claims/{id}", format!("claims/{claim}")),
        ("sources/{id}", format!("sources/{source}")),
        ("provenance/{title}", "provenance/Home".to_owned()),
    ] {
        let a = get(&w, &format!("/handbook/-/api/v1/{tail}")).await;
        results.push((route(template), tail, a.json()));
    }
    let w = super::knowledge::forked();
    let tail = "views/compare?a=main&b=alt".to_owned();
    let a = get(&w, &format!("/handbook/-/api/v1/{tail}")).await;
    results.push((route("views/compare"), tail, a.json()));
    results
}

#[tokio::test]
async fn every_answer_is_of_the_shape_the_document_promises_and_every_declared_member_is_seen() {
    let w = World::new("handbook", "The Handbook", &[]);
    let document = get(&w, OPENAPI_PATH).await.json();
    let mut checker = Checker::new(&document);
    let mut problems = Vec::new();
    let answers = samples().await;
    assert!(answers.len() >= 29, "{}", answers.len());
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
        "Claim",
        "ClaimSaid",
        "ClaimSource",
        "Source",
        "SourceCitedBy",
        "PageProvenance",
        "ProvenanceParagraph",
        "ProvenanceChange",
        "ViewComparison",
        "ComparedPage",
        "ComparedClaim",
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
    let mut checker = Checker::new(&document);
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

#[test]
fn the_checker_knows_a_constant_an_id_a_length_and_exactly_one_alternative() {
    let document = json!({"components": {"schemas": {"S": {"oneOf": [
        {"type": "object", "required": ["id", "go"], "properties": {"id": {"type": "string", "format": "uuid"}, "go": {"const": true}}},
        {"type": "object", "required": ["id"], "properties": {"id": {"type": "string", "format": "uuid"}, "name": {"type": "string", "maxLength": 3}}},
    ]}}}});
    let schema = json!({"$ref": "#/components/schemas/S"});
    let mut checker = Checker::new(&document);
    let mut check = |value: Value| {
        let mut problems = Vec::new();
        checker.check(&schema, &value, "v", &mut problems);
        problems
    };
    let id = "01930000-0000-7000-8000-000000000001";
    assert!(check(json!({"id": id, "go": true})).is_empty());
    assert!(check(json!({"id": id, "name": "abc"})).is_empty());
    assert!(
        !check(json!({"id": "nope", "go": true})).is_empty(),
        "format"
    );
    assert!(
        !check(json!({"id": id, "name": "abcd"})).is_empty(),
        "maxLength"
    );
    assert!(!check(json!({"id": id, "go": false})).is_empty(), "const");
    assert!(!check(json!({"go": true})).is_empty(), "required");
}

#[test]
fn the_checker_reads_a_pattern_and_refuses_one_it_cannot_read() {
    let id = "^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$";
    let padded = "^\\s*[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\\s*$";
    let good = "01930000-0000-7000-8000-000000000001";
    for (pattern, text, matches) in [
        (id, good, true),
        (id, "01930000-0000-7000-8000-00000000000A", false),
        (id, "01930000-0000-4000-8000-000000000001", false),
        (id, "01930000-0000-7000-c000-000000000001", false),
        (id, "", false),
        (id, " 01930000-0000-7000-8000-000000000001", false),
        (padded, " \t01930000-0000-7000-8000-000000000001\n", true),
        (padded, "", false),
        ("^\\s*$", "", true),
        ("^\\s*$", " \n", true),
        ("^\\s*$", " x", false),
        ("^[0-9a-f-]{3,5}$", "a-f", true),
        ("^[0-9a-f-]{3,5}$", "a-", false),
        ("^[0-9a-f-]{3,5}$", "a-f-0-", false),
        ("^a+b?$", "aaa", true),
        ("^a+b?$", "aab", true),
        ("^a+b?$", "b", false),
        ("^a*a$", "aaaa", true),
    ] {
        assert_eq!(
            pattern_matches(pattern, text),
            Ok(matches),
            "{pattern} on {text:?}"
        );
    }
    for pattern in ["a|b", "^(a)$", "^a.b$", "abc", "^[a-$"] {
        assert!(pattern_matches(pattern, "a").is_err(), "{pattern}");
    }
    let document = json!({"components": {"schemas": {"S": {
        "type": "string",
        "anyOf": [{"pattern": id}, {"pattern": "^\\s*$"}],
    }}}});
    let schema = json!({"$ref": "#/components/schemas/S"});
    let mut checker = Checker::new(&document);
    let mut check = |value: Value| {
        let mut problems = Vec::new();
        checker.check(&schema, &value, "v", &mut problems);
        problems
    };
    assert!(check(json!(good)).is_empty());
    assert!(check(json!("")).is_empty(), "the other alternative");
    assert!(!check(json!("the later one")).is_empty(), "neither");
}

#[test]
fn an_answer_is_closed_by_what_it_declares_and_a_request_only_by_additional_properties() {
    let schema = |closed: bool| {
        let mut s =
            json!({"type": "object", "required": ["a"], "properties": {"a": {"type": "string"}}});
        if closed {
            s.as_object_mut()
                .expect("an object")
                .insert("additionalProperties".to_owned(), json!(false));
        }
        json!({"components": {"schemas": {"S": s}}})
    };
    let reference = json!({"$ref": "#/components/schemas/S"});
    let body = json!({"a": "x", "b": 1});
    let problems = |document: &Value, request: bool| {
        let mut checker = if request {
            Checker::for_request(document)
        } else {
            Checker::new(document)
        };
        let mut problems = Vec::new();
        checker.check(&reference, &body, "v", &mut problems);
        problems
    };
    // An answer may not hold what the document does not declare, closed or not.
    assert!(!problems(&schema(false), false).is_empty());
    assert!(!problems(&schema(true), false).is_empty());
    // A request is free to hold it where the schema does not close the object, as in JSON Schema.
    assert!(problems(&schema(false), true).is_empty());
    assert!(!problems(&schema(true), true).is_empty());
    // And `additionalProperties` may be the schema of the rest.
    let typed = json!({"components": {"schemas": {"S": {
        "type": "object", "properties": {"a": {"type": "string"}},
        "additionalProperties": {"type": "integer"},
    }}}});
    assert!(problems(&typed, true).is_empty());
    let mut checker = Checker::for_request(&typed);
    let mut wrong = Vec::new();
    checker.check(&reference, &json!({"a": "x", "b": "no"}), "v", &mut wrong);
    assert!(!wrong.is_empty(), "{wrong:?}");
}

#[test]
fn any_of_holds_when_one_alternative_does_and_one_of_when_exactly_one_does() {
    let document = json!({"components": {"schemas": {
        "Any": {"anyOf": [{"type": "string"}, {"const": "x"}]},
        "One": {"oneOf": [{"type": "string"}, {"const": "x"}]},
    }}});
    let any = json!({"$ref": "#/components/schemas/Any"});
    let one = json!({"$ref": "#/components/schemas/One"});
    let mut checker = Checker::new(&document);
    let mut check = |schema: &Value, value: Value| {
        let mut problems = Vec::new();
        checker.check(schema, &value, "v", &mut problems);
        problems.is_empty()
    };
    assert!(check(&any, json!("x")), "both match: any");
    assert!(!check(&one, json!("x")), "both match: not exactly one");
    assert!(check(&any, json!("y")));
    assert!(check(&one, json!("y")));
    assert!(!check(&any, json!(1)), "neither");
}
