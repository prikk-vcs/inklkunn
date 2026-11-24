//! The OpenAPI 3.1 document of the read API, **generated from the route table** (`routes::application_table`)
//! and the [`Doc`]s that sit beside the handlers. Nothing here lists a path by hand: the paths come from the
//! registry, the descriptions from the docs, and a path that has one and not the other is a [`Drift`] that
//! fails the build of the document (and a test holds the same both ways against the router).
//!
//! What the document adds to a route is: the operation (its summary, description, parameters, answers), the
//! guard the registry declares for it (`x-inklkunn-action`, or `x-inklkunn-public` with the reason), and the
//! problems the endpoint can answer, from the registry of codes.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use inklkunn_core::{Guard, RouteTable};
use serde_json::{Map, Value, json};

use super::describe::{
    Answer, Base, Doc, EVERY_ENDPOINT, EVERY_WRITE, Kind, Param, SIZES, Write, path_param,
};
use super::problem::Code;
use super::{PROBLEM_MEDIA_TYPE, SITE_PREFIX, WIKI_PREFIX, endpoints};
use crate::catalog::{EN, T};

/// The document and the route table disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Drift {
    /// A route under the API's prefixes that no [`Doc`] describes (and that is not a declared non-endpoint).
    Undocumented(String),
    /// A [`Doc`] whose path is not in the route table.
    NoRoute(String),
    /// A route of the API that is not guarded by `Action::Read` and is not a public route with a reason.
    Unguarded(String),
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Drift::Undocumented(p) => write!(f, "the route {p} has no description"),
            Drift::NoRoute(p) => write!(f, "the description of {p} has no route"),
            Drift::Unguarded(p) => write!(f, "the route {p} has no guard of the read action"),
        }
    }
}

/// Is `path` a route of the API?
pub(crate) fn is_api_route(path: &str) -> bool {
    path.starts_with(&format!("{WIKI_PREFIX}/"))
        || path.starts_with(&format!("{SITE_PREFIX}/"))
        || path.starts_with("/{slug}/-/api/")
        || path.starts_with("/-/api/")
}

/// The OpenAPI spelling of a router path: `{*title}` is `{title}`.
pub(crate) fn openapi_path(path: &str) -> String {
    path.replace("{*", "{")
}

/// The names of the parameters in a router path, in order.
fn path_params(path: &str) -> Vec<&str> {
    path.split('/')
        .filter_map(|segment| {
            segment
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .map(|name| name.strip_prefix('*').unwrap_or(name))
        })
        .collect()
}

fn schema_of(kind: Kind) -> Value {
    match kind {
        Kind::Text { max } => json!({"type": "string", "minLength": 1, "maxLength": max}),
        Kind::Int { min, max } => json!({"type": "integer", "minimum": min, "maximum": max}),
        Kind::Hex { max } => json!({
            "type": "string",
            "pattern": format!("^[0-9a-f]{{1,{max}}}$"),
        }),
        Kind::Choice(words) => json!({"type": "string", "enum": words}),
    }
}

fn query_parameter(param: &Param) -> Value {
    let mut schema = schema_of(param.kind);
    if param.name == "limit"
        && let Some(object) = schema.as_object_mut()
    {
        object.insert("default".to_owned(), json!(SIZES.0));
    }
    json!({
        "name": param.name,
        "in": "query",
        "required": param.required,
        "description": param.description,
        "schema": schema,
    })
}

fn path_parameter(name: &str) -> Value {
    let (description, kind) = path_param(name).unwrap_or(("A part of the address.", "string"));
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "description": description,
        "schema": {"type": kind},
    })
}

fn problem_response(codes: &[Code]) -> Value {
    let english = T::new(EN);
    let list = codes
        .iter()
        .map(|code| {
            let (title, ..) = code.keys();
            format!("`{}` ({})", code.slug(), english.get(title))
        })
        .collect::<Vec<_>>()
        .join(", ");
    json!({
        "description": format!("A problem detail (RFC 9457). One of: {list}."),
        "content": {PROBLEM_MEDIA_TYPE: {"schema": {"$ref": "#/components/schemas/Problem"}}},
    })
}

fn success_response(answer: Answer, write: Option<Write>) -> Value {
    let validators = if write.is_some() {
        json!({
            "ETag": {
                "description": "The proposal's revision, as the base of the next write: send it back as `If-Match`. It names the revision, not these bytes.",
                "schema": {"type": "string"},
            },
        })
    } else {
        json!({
            "ETag": {
                "description": "A strong validator of the body. Send it back as `If-None-Match` to get `304` while the answer has not changed.",
                "schema": {"type": "string"},
            },
        })
    };
    match answer {
        Answer::Schema(name) => json!({
            "description": "The answer.",
            "headers": validators,
            "content": {"application/json": {"schema": {"$ref": format!("#/components/schemas/{name}")}}},
        }),
        Answer::Fragment => json!({
            "description": "The page body's sanitized HTML, as a fragment.",
            "headers": validators,
            "content": {"text/html": {"schema": {"type": "string"}}},
        }),
        Answer::Description => json!({
            "description": "This document.",
            "headers": validators,
            "content": {super::OPENAPI_MEDIA_TYPE: {"schema": {"type": "object"}}},
        }),
    }
}

/// The `If-Match` header of a write: what it names depends on what the write is made against.
fn if_match_parameter(base: Base) -> Value {
    let (description, pattern) = match base {
        Base::View => (
            "The state of the view's tip the proposal was written against: the `state_id` of the `tip` the read endpoints give, in quotes. A tip that has moved on is `409 stale-base` carrying the current one.",
            "^\"[0-9a-f]{64}\"$",
        ),
        Base::Proposal => (
            "The proposal's revision this write is made against: the `etag` the proposal was last read with, in quotes. An older revision is `409 stale-base` carrying the current one. One strong tag; `*` and lists are refused.",
            "^\"[0-9a-f-]{36}-r[0-9]+\"$",
        ),
    };
    json!({
        "name": "If-Match",
        "in": "header",
        "required": true,
        "description": description,
        "schema": {"type": "string", "pattern": pattern},
    })
}

/// The name of the CSRF header as the document spells it (the server reads it without regard to case).
pub(super) const CSRF_HEADER_NAME: &str = "X-CSRF-Token";
/// The security schemes of a write: a bearer token, or a session cookie **with** its CSRF token.
pub(super) const BEARER_SCHEME: &str = "bearerAuth";
pub(super) const SESSION_SCHEME: &str = "sessionCookie";
pub(super) const CSRF_SCHEME: &str = "csrfToken";

/// The `X-CSRF-Token` header of a write made with the session cookie (corrections G9).
fn csrf_parameter() -> Value {
    json!({
        "name": CSRF_HEADER_NAME,
        "in": "header",
        "required": false,
        "description": "The CSRF token of the signed-in session: a client that is a browser page finds it in the hidden field `csrf` of the forms of the pages it was served (the account page's, for one), a client that is not a form sends it as this header. **Required when the write is authenticated by the session cookie**, never otherwise: a browser sends the cookie by itself, so the cookie alone is refused with `forbidden`. A request that carries a bearer token is judged by the token alone and needs neither the cookie nor this header.",
        "schema": {"type": "string"},
    })
}

fn operation(doc: &Doc, path: &str, guard: Guard) -> Value {
    let mut parameters: Vec<Value> = path_params(path).into_iter().map(path_parameter).collect();
    if let Some(write) = doc.write {
        parameters.push(if_match_parameter(write.base));
        parameters.push(csrf_parameter());
    }
    parameters.extend(doc.query.iter().map(query_parameter));

    let mut responses: BTreeMap<String, Value> = BTreeMap::new();
    let success = doc.write.map_or(200, |w| w.success);
    responses.insert(success.to_string(), success_response(doc.answer, doc.write));
    if doc.write.is_none() {
        responses.insert(
            "304".to_owned(),
            json!({"description": "Not modified: the `If-None-Match` matched the answer's `ETag`."}),
        );
    }
    let writes: &[Code] = if doc.write.is_some() {
        EVERY_WRITE
    } else {
        &[]
    };
    let mut by_status: BTreeMap<u16, Vec<Code>> = BTreeMap::new();
    for code in EVERY_ENDPOINT.iter().chain(writes).chain(doc.problems) {
        let codes = by_status.entry(code.status().as_u16()).or_default();
        if !codes.contains(code) {
            codes.push(*code);
        }
    }
    for (status, codes) in by_status {
        responses.insert(status.to_string(), problem_response(&codes));
    }

    let mut op = Map::new();
    op.insert("operationId".to_owned(), json!(doc.operation_id));
    op.insert("tags".to_owned(), json!([doc.tag]));
    op.insert("summary".to_owned(), json!(doc.summary));
    op.insert("description".to_owned(), json!(doc.description));
    op.insert("parameters".to_owned(), Value::Array(parameters));
    if let Some(write) = doc.write {
        op.insert(
            "requestBody".to_owned(),
            json!({
                "required": true,
                "description": format!("A JSON object, at most {} bytes. Every member is named, typed and used: one that is unknown, repeated or of the wrong type is `invalid-body`.", write.limit),
                "content": {"application/json": {"schema": {"$ref": format!("#/components/schemas/{}", write.request)}}},
            }),
        );
        op.insert(
            "security".to_owned(),
            json!([
                {BEARER_SCHEME: []},
                {SESSION_SCHEME: [], CSRF_SCHEME: []},
            ]),
        );
        op.insert(
            "x-inklkunn-scope".to_owned(),
            json!(write.action.required_scope().as_str()),
        );
    }
    op.insert("responses".to_owned(), json!(responses));
    match guard {
        Guard::Action(action) => {
            op.insert(
                "x-inklkunn-action".to_owned(),
                json!(format!("{action:?}").to_lowercase()),
            );
        }
        Guard::Public { reason } => {
            op.insert("x-inklkunn-public".to_owned(), json!(reason));
        }
    }
    Value::Object(op)
}

fn components() -> Value {
    let tip = json!({
        "type": "object",
        "description": "Where the answer was read: the view and the id of the change at its tip (`null` when the answer is the last one the server had, because the newest could not be asked for).",
        "required": ["view", "block_id", "state_id"],
        "properties": {
            "view": {"type": "string"},
            "block_id": {"type": ["string", "null"]},
            "state_id": {"type": ["string", "null"], "description": "The view's state at that change (64 lower-case hexadecimal digits): what a write that opens a proposal names in `If-Match`. `null` like `block_id`."},
        },
    });
    let mut schemas = json!({
            "Problem": {
                "type": "object",
                "description": "A problem detail (RFC 9457). `type` is stable and does not change with the language; `title`, `detail` and `hint` are in the language of `Content-Language`. The members after `hint` appear only for the codes that use them.",
                "required": ["type", "title", "status", "detail", "hint"],
                "properties": {
                    "type": {"type": "string", "description": "`urn:inklkunn:problem:` and the code: see `x-inklkunn-problems`."},
                    "title": {"type": "string"},
                    "status": {"type": "integer"},
                    "detail": {"type": "string"},
                    "hint": {"type": "string", "description": "What to do next."},
                    "parameter": {"type": "string", "description": "`invalid-parameter`: the parameter."},
                    "view": {"type": "string", "description": "`no-such-view`: the view asked for."},
                    "feature": {"type": "string", "description": "`not-available`: the filter that is not offered."},
                    "candidates": {"type": "array", "items": {"type": "string"}, "description": "`ambiguous-title`: the titles that match."},
                    "reason": {"type": "string", "description": "`unacceptable` and `forbidden`, where there is one: the stable word that names what was wrong (for the rules of a wiki: `governance-path`, `governance-mixed`, `governance-invalid`, `governance-rung`, `approvals-missing`, `summary-hidden`, `protected`, `rules-unreadable`, `author-not-allowed`; for an account that is restricted in the wiki: `restricted`). It does not change with the language."},
                    "refusals": {
                        "type": "array",
                        "description": "`governance-invalid`: what is wrong with each governance file the change writes. Never what a file said.",
                        "items": {
                            "type": "object",
                            "required": ["file", "rule", "field", "line"],
                            "properties": {
                                "file": {"type": "string", "description": "The file's fixed name (`governance/policy.yaml`, `governance/constitution.yaml`), or `governance/` when it is no file of the two."},
                                "rule": {"type": "string", "description": "The rule's stable name (`unknown-key`, `wrong-shape`, `ordering-propose-above-review`, `platform-limit-entry-max`, `outside-default-view`, `unknown-file`, …)."},
                                "field": {"type": "string", "description": "The schema's field the rule is about (a name of the schema, never read from the file); empty when the rule is about the whole file."},
                                "line": {"type": ["integer", "null"], "description": "The line of the file, from 1, where it can be said."},
                            },
                        },
                    },
                    "needs": {"type": "string", "description": "`governance-rung` and `protected` (when `lacks` is `role`): the role the rules ask."},
                    "lacks": {"type": "string", "enum": ["role", "approval"], "description": "`protected`: what the acceptance lacked of what the protections ask: a higher role (`needs` names it), or an approval of the latest revision from someone other than the proposer."},
                    "protections": {
                        "type": "array",
                        "description": "`protected`: every protection in force on the pages the change touches, each once, as the pages show them. Both kinds are listed; the stricter decides.",
                        "items": {
                            "type": "object",
                            "required": ["kind", "on", "title", "requires", "role", "reason", "until"],
                            "properties": {
                                "kind": {"type": "string", "enum": ["standing", "temporary"], "description": "`standing`: by the rules of the wiki (its policy). `temporary`: put in place for a time by the role that moderates."},
                                "on": {"type": "string", "enum": ["page", "namespace"], "description": "One page, or every page below a title (never the page of that title)."},
                                "title": {"type": "string", "description": "The page's title, or the namespace."},
                                "requires": {"type": "string", "enum": ["review", "rung"], "description": "An approval first, or a role."},
                                "role": {"type": ["string", "null"], "description": "The role it asks, for `rung`."},
                                "reason": {"type": "string", "description": "Why, in the words of whoever set it."},
                                "until": {"type": ["string", "null"], "description": "When a temporary protection ends (RFC 3339, UTC); null for a standing one."},
                            },
                        },
                    },
                    "author_restricted": {"type": "boolean", "description": "`author-not-allowed`: a restriction is the reason the proposal's author no longer holds the role that may replace a file the wiki cannot read. Its reason and end are not given: they are the restricted person's alone."},
                    "needed": {"type": "integer", "description": "`approvals-missing`: the approvals of the latest revision the rules ask for."},
                    "have": {"type": "integer", "description": "`approvals-missing`: the people other than the proposer who approved the latest revision, who hold the review role now and whose account is not disabled."},
                    "file": {"type": "string", "description": "`rules-unreadable`: the governance file that cannot be read."},
                    "rule": {"type": "string", "description": "`rules-unreadable`: the rule it breaks, by its stable name (`not-a-file` where something that is not an ordinary file is at its place)."},
                    "line": {"type": ["integer", "null"], "description": "`rules-unreadable`: the line, where it can be said."},
                    "found": {"type": "array", "items": {"type": "string"}, "description": "`rules-unreadable` with the rule `not-a-file`: the repository paths found at or below the file's place, at most 50, in order; the repair is a change by an admin that removes these and writes the file."},
                    "found_more": {"type": "integer", "description": "`rules-unreadable` with the rule `not-a-file`: how many more paths than `found` lists were there."},
                    "restriction_reason": {"type": "string", "description": "`restricted`: the reason the account was given when it was restricted. Said to the restricted account alone."},
                    "until": {"type": "integer", "description": "`restricted`: when the restriction ends, in Unix seconds: from that second on it no longer applies."},
                    "explanation": {"type": "string", "description": "`restricted`: the same, as sentences in the caller's language."},
                },
            },
            "Tip": tip,
            "ListedPage": {
                "type": "object",
                "required": ["title", "kind", "id", "aliases", "problem"],
                "properties": {
                    "title": {"type": "string"},
                    "kind": {"type": "string", "description": "`page`, `redirect`, …"},
                    "id": {"type": ["string", "null"], "description": "The page's stable id, if it declares one."},
                    "aliases": {"type": "array", "items": {"type": "string"}},
                    "problem": {"type": ["string", "null"], "description": "A stable code naming why the page's information could not be read; `null` when it could."},
                },
            },
            "PageList": {
                "type": "object",
                "required": ["tip", "items", "next_cursor"],
                "properties": {
                    "tip": {"$ref": "#/components/schemas/Tip"},
                    "items": {"type": "array", "maxItems": SIZES.1, "items": {"$ref": "#/components/schemas/ListedPage"}},
                    "next_cursor": {"type": ["string", "null"], "description": "Where the next page of the list begins; `null` on the last. Opaque."},
                },
            },
            "Heading": {
                "type": "object",
                "required": ["level", "id", "text"],
                "properties": {
                    "level": {"type": "integer", "minimum": 1, "maximum": 6, "description": "The level of the heading in `html` (the page's own title is the one `h1`, so a page's headings begin at 2)."},
                    "id": {"type": "string"},
                    "text": {"type": "string"},
                },
            },
            "Page": {
                "type": "object",
                "required": ["title", "id", "kind", "status", "lang", "license", "description", "tags", "aliases", "toc", "html", "redirected_from", "matched_by", "stale", "tip"],
                "properties": {
                    "title": {"type": "string"},
                    "id": {"type": ["string", "null"]},
                    "kind": {"type": "string"},
                    "status": {"type": "string"},
                    "lang": {"type": ["string", "null"], "description": "The page's own language, if it declares one."},
                    "license": {"type": "string", "description": "The page's license, else the wiki's default."},
                    "description": {"type": ["string", "null"]},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "aliases": {"type": "array", "items": {"type": "string"}},
                    "toc": {"type": "array", "items": {"$ref": "#/components/schemas/Heading"}},
                    "html": {"type": "string", "description": "The page body as a sanitized HTML fragment."},
                    "redirected_from": {
                        "type": ["object", "null"],
                        "description": "Set when the title asked for was an alias or a redirect.",
                        "required": ["title", "by"],
                        "properties": {
                            "title": {"type": "string"},
                            "by": {"type": "string", "enum": ["alias", "redirect"]},
                        },
                    },
                    "matched_by": {"type": "string", "description": "How the title asked for was matched: `exact`, `alias`, `case`, `normalized`, …"},
                    "stale": {"type": "boolean", "description": "True when the server could not ask for the newest version and this is the last one it has."},
                    "tip": {"$ref": "#/components/schemas/Tip"},
                },
            },
            "Raw": {
                "type": "object",
                "required": ["title", "kind", "storage_path", "tip"],
                "properties": {
                    "title": {"type": "string"},
                    "kind": {"type": "string"},
                    "storage_path": {"type": "string", "description": "Where the page's file is kept in the wiki's repository."},
                    "tip": {"$ref": "#/components/schemas/Tip"},
                },
            },
    });
    if let Some(map) = schemas.as_object_mut() {
        map.extend(shapes::schemas());
        map.extend(writes::schemas());
        map.extend(knowledge_api::schemas());
        map.extend(governance_api::schemas());
    }
    json!({
        "schemas": schemas,
        "securitySchemes": {
            BEARER_SCHEME: {
                "type": "http",
                "scheme": "bearer",
                "description": "An API token (`ink_…`) issued to an account, with a scope (`read`, `propose`, `review`, `accept`, `admin`). The scope is intersected with the account's role in the wiki: a token never does more than either allows.",
            },
            SESSION_SCHEME: {
                "type": "apiKey",
                "in": "cookie",
                "name": inklkunn_auth::session::SESSION_COOKIE,
                "description": "The site's own session, set by signing in on its pages. For a read it is enough; **a write made with it must also present the session's CSRF token** (`csrfToken`), and the two are checked together.",
            },
            CSRF_SCHEME: {
                "type": "apiKey",
                "in": "header",
                "name": CSRF_HEADER_NAME,
                "description": "The session's CSRF token, for a client that is not a form (a form sends it as the field `csrf`). Used only together with `sessionCookie`.",
            },
        },
    })
}

fn problem_types() -> Value {
    let english = T::new(EN);
    Value::Array(
        Code::ALL
            .iter()
            .map(|code| {
                let (title, ..) = code.keys();
                json!({
                    "code": code.slug(),
                    "type": code.type_uri(),
                    "status": code.status().as_u16(),
                    "title": english.get(title),
                })
            })
            .collect(),
    )
}

const INTRO: &str = "The API of an inklkunn site: what the pages show, as JSON, and the writes that propose, review and accept changes. Every wiki's endpoints are under `/{slug}/-/api/v1/`, the version is in the path, and this description is generated from the server's route table. Every answer is a success of the stated shape or a problem detail (RFC 9457) whose `type` is stable (`x-inklkunn-problems` lists them) and whose `title`, `detail` and `hint` follow the language cookie or `Accept-Language`. A wiki that does not exist and a wiki you may not read answer the same `not-found`. Every success has an `ETag` and `304` answers a matching `If-None-Match`. HEAD is answered like GET without the body. There is no cross-origin access. The reads take the site's session or an API token; **a write takes a JSON body (`application/json`, bounded before it is parsed) and names its base in `If-Match`** — the proposal revision's `etag`, or for a new proposal the `state_id` of the view's tip — and a stale base is `409 stale-base`, which carries the current one. A write is authenticated by an API token (`Authorization: Bearer …`), whose scope (`propose`, `review`, `accept`) is intersected with the account's role in the wiki, or by the site's own session: **the session cookie together with the session's CSRF token in `X-CSRF-Token`** (a cookie alone is refused with `forbidden`; a request that carries a bearer token is judged by the token alone). Where waiting helps, a refusal carries `Retry-After`. A wiki whose rules (its `governance/` files) cannot be read refuses every write, `forbidden` with `reason` `rules-unreadable`, until they are repaired; a read is never refused for it. An account that is **restricted** in a wiki (a moderator's act with a reason and an end, made on the site and never through the API) reads it and is refused every write that comments, proposes or reviews: `forbidden` with `reason` `restricted`, `restriction_reason`, `until` (Unix seconds) and `explanation`, said to that account alone; a restriction ends by itself at its end. **Moderation, restrictions, protections, grants and the decisions of appeals are web-only in 0.4.0, and this API does not do them**: they are deliberate human acts, and a scripted moderator is the automated sanction the rules of a wiki rule out, so the site's own forms for them refuse a request that presents an API token (`403`), whatever its scope. What it offers of them is reading: the rules of a wiki as data (`policy`), the protections in force (`protections`), the moderation log (`moderation-log`, which never carries hidden content, a name or a note) and the calling account's own appeals (`appeals/mine`); a change to the rules is proposed through the propose API, as the governance files.";

/// Build the document from the route `table` and the endpoints' `docs`.
pub(crate) fn document(table: &RouteTable, docs: &[&'static Doc]) -> Result<Value, Drift> {
    let listed: Vec<&str> = endpoints::NOT_ENDPOINTS.to_vec();
    let mut paths: BTreeMap<String, Value> = BTreeMap::new();
    for entry in table.entries().iter().filter(|e| is_api_route(e.path)) {
        if listed.contains(&entry.path) {
            continue;
        }
        let doc = docs
            .iter()
            .find(|d| d.path == entry.path)
            .ok_or_else(|| Drift::Undocumented(entry.path.to_owned()))?;
        // A read is guarded by `Read` (or says why it is public); a write by the action its description names.
        let method = match (doc.write, entry.guard) {
            (None, Guard::Action(inklkunn_core::Action::Read) | Guard::Public { .. }) => "get",
            (Some(write), Guard::Action(action)) if action == write.action => "post",
            _ => return Err(Drift::Unguarded(entry.path.to_owned())),
        };
        paths.insert(
            openapi_path(entry.path),
            json!({method: operation(doc, entry.path, entry.guard)}),
        );
    }
    if let Some(doc) = docs.iter().find(|d| table.find(d.path).is_none()) {
        return Err(Drift::NoRoute(doc.path.to_owned()));
    }
    Ok(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "inklkunn API",
            "version": "1.0.0",
            "summary": "Wikis over HTTP: pages, their rendering, their history, links and search; proposals, review and acceptance.",
            "description": INTRO,
            "license": {"name": "Apache-2.0", "identifier": "Apache-2.0"},
        },
        "tags": [
            {"name": "pages", "description": "Pages and their rendering."},
            {"name": "history", "description": "A page's history, one change, the difference between two changes of a page, and the wiki's recent changes."},
            {"name": "links", "description": "The pages that link to a page, and the wiki's link health lists."},
            {"name": "search", "description": "Search over titles and aliases."},
            {"name": "knowledge", "description": "Claims, sources, a page's provenance paragraph by paragraph, and the comparison of two views (RFC 005). Nothing in them ranks, scores or judges: they say what is recorded and where views differ."},
            {"name": "governance", "description": "The rules of a wiki as data, the protections in force, the moderation log and a person's own appeals (RFC 006). They only read: moderation, restrictions, protections, grants and the decisions of appeals are web-only in 0.4.0."},
            {"name": "proposals", "description": "Opening, reading, revising and taking back proposals (writes: scope `propose`)."},
            {"name": "review", "description": "Comments, decisions, acceptance and declining (scopes `propose`, `review`, `accept`)."},
            {"name": "description", "description": "This document."},
        ],
        "paths": paths,
        "components": components(),
        "x-inklkunn-problems": problem_types(),
    }))
}

/// The document served, built once from the live registry. An error is a registry that drifted from the
/// descriptions, which the tests prevent; the endpoint then answers `failed` rather than a wrong document.
pub(crate) fn served() -> Result<&'static [u8], super::problem::Problem> {
    static DOCUMENT: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    DOCUMENT
        .get_or_init(|| {
            let table = crate::routes::application_table().ok()?;
            let all = endpoints::all();
            let docs: Vec<&'static Doc> = all.iter().filter_map(|e| e.doc).collect();
            let value = document(&table, &docs).ok()?;
            serde_json::to_vec(&value).ok()
        })
        .as_deref()
        .ok_or_else(|| super::problem::Problem::new(Code::Failed))
}

mod governance_api;
mod knowledge_api;
mod shapes;
mod writes;

#[cfg(test)]
mod tests;
