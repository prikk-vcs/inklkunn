//! What each endpoint is, written once and kept beside the endpoint list (`endpoints.rs`): the OpenAPI
//! document is generated from these and from the route table, and the handlers parse and check their query
//! with the very same [`Param`]s, so what the document says a parameter may be and what the server accepts
//! cannot drift apart.
//!
//! The descriptions are **English by decision**: they are the reference a programmer reads, like the operator's
//! log lines and start-up refusals, and not a sentence a reader of the wiki sees (`tests/literals.allow` says
//! so). The problem details a caller can get are catalog messages in the caller's language.

use inklkunn_core::read::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

use super::problem::Code;

/// What a parameter may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Text of 1 to `max` characters, with no control character.
    Text { max: usize },
    /// A whole number from `min` to `max`.
    Int { min: i64, max: i64 },
    /// Lower-case hexadecimal digits, 1 to `max` of them.
    Hex { max: usize },
}

/// A query parameter.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
    /// The problem answered when a required parameter is missing or empty.
    pub missing: Code,
    pub description: &'static str,
}

/// What an endpoint answers on success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    /// JSON of the named schema of `components`.
    Schema(&'static str),
    /// The page's sanitized HTML fragment.
    Fragment,
    /// The OpenAPI document itself.
    Description,
    /// Nothing yet: the endpoint exists and answers `not-available` until the read behind it does.
    NotOffered,
}

/// One endpoint, as the document describes it. `path` is the key into the route registry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Doc {
    pub path: &'static str,
    pub operation_id: &'static str,
    pub tag: &'static str,
    pub summary: &'static str,
    pub description: &'static str,
    pub query: &'static [Param],
    pub answer: Answer,
    /// Problems the endpoint can answer besides the ones every endpoint can (`not-found`, `unavailable`,
    /// `failed`, `method-not-allowed`).
    pub problems: &'static [Code],
}

/// The problems any endpoint can answer.
pub(crate) const EVERY_ENDPOINT: &[Code] = &[
    Code::NotFound,
    Code::Unavailable,
    Code::Failed,
    Code::MethodNotAllowed,
];

/// What a path parameter is, for the document: its description and what it may hold.
pub(crate) fn path_param(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "slug" => Some((
            "The wiki's slug: the first part of every address of the wiki.",
            "string",
        )),
        "title" => Some((
            "A page's title, percent-encoded as in the page's address; a title may hold `/`. An alias or a redirect leads to the page it names.",
            "string",
        )),
        "id" => Some((
            "The id of one recorded change: 1 to 64 lower-case hexadecimal digits.",
            "string",
        )),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------ the parameters

const fn text(name: &'static str, max: usize, required: bool, description: &'static str) -> Param {
    Param {
        name,
        kind: Kind::Text { max },
        required,
        missing: Code::InvalidParameter,
        description,
    }
}

/// The view a page is read in.
pub(crate) const VIEW: Param = text(
    "view",
    100,
    false,
    "The name of the view to read (for example `main`). Without it, the wiki's default view.",
);

/// How many items a page of a list holds.
pub(crate) const LIMIT: Param = Param {
    name: "limit",
    kind: Kind::Int {
        min: 1,
        max: MAX_PAGE_SIZE as i64,
    },
    required: false,
    missing: Code::InvalidParameter,
    description: "How many items one page of the list holds: 1 to 100 (the default is 50). A larger number is refused, not trimmed.",
};

/// Where the next page of a list begins.
pub(crate) const CURSOR: Param = Param {
    name: "cursor",
    kind: Kind::Text { max: 2200 },
    required: false,
    missing: Code::InvalidParameter,
    description: "The `next_cursor` of the previous page. Opaque: do not build or change one.",
};

const NAMESPACE: Param = text(
    "namespace",
    512,
    false,
    "Only pages in this namespace: titles whose part before the last `/` is this name, or begins with it and a `/`.",
);

const KIND: Param = text(
    "kind",
    40,
    false,
    "Only pages of this kind (`page`, `redirect`, …).",
);

const TAG: Param = text(
    "tag",
    200,
    false,
    "Only pages with this tag. Not offered by this build: it answers `not-available`.",
);

const CHANGED_SINCE: Param = Param {
    name: "changed_since",
    kind: Kind::Int {
        min: 0,
        max: i64::MAX,
    },
    required: false,
    missing: Code::InvalidParameter,
    description: "Only pages changed since this time, in Unix seconds. Not offered by this build: it answers `not-available`.",
};

const FROM: Param = text(
    "from",
    128,
    false,
    "The point to compare from: the id of a change. Without it, the point before `to`.",
);

const TO: Param = text(
    "to",
    128,
    false,
    "The point to compare to: the id of a change. Without it, the tip of the default view.",
);

const PATH: Param = Param {
    name: "path",
    kind: Kind::Text { max: 1024 },
    required: true,
    missing: Code::DiffPathRequired,
    description: "The storage path of the page the diff is for, as `raw` reports it. Required: a diff is always for one page, and the API has no diff of a whole tree.",
};

const QUERY: Param = Param {
    name: "q",
    kind: Kind::Text { max: 500 },
    required: true,
    missing: Code::InvalidParameter,
    description: "What to search for.",
};

// ------------------------------------------------------------------------------------ the endpoints

const LIST_PARAMS: &[Param] = &[VIEW, LIMIT, CURSOR, NAMESPACE, KIND, TAG, CHANGED_SINCE];
const VIEW_ONLY: &[Param] = &[VIEW];
const VIEW_PAGED: &[Param] = &[VIEW, LIMIT, CURSOR];
const SEARCH_PARAMS: &[Param] = &[QUERY, VIEW, LIMIT, CURSOR];
const DIFF_PARAMS: &[Param] = &[PATH, FROM, TO];

const PAGE_PROBLEMS: &[Code] = &[
    Code::PageNotFound,
    Code::AmbiguousTitle,
    Code::RedirectBroken,
    Code::PageUnreadable,
    Code::InvalidTitle,
    Code::NoSuchView,
    Code::InvalidParameter,
];

pub(crate) const OPENAPI: Doc = Doc {
    path: super::OPENAPI_PATH,
    operation_id: "getDescription",
    tag: "description",
    summary: "This description",
    description: "The OpenAPI 3.1 document of the read API, generated from the server's own route table. One document for the whole site: every path in it begins with the wiki's slug as a parameter.",
    query: &[],
    answer: Answer::Description,
    problems: &[],
};

pub(crate) const PAGES: Doc = Doc {
    path: "/{slug}/-/api/v1/pages",
    operation_id: "listPages",
    tag: "pages",
    summary: "List the pages of a wiki",
    description: "The pages of the view's tip, one page of the list at a time, in a fixed order. Follow `next_cursor` until it is `null`: a walk visits every matching page once, whatever changes meanwhile to the pages already passed. Redirects and pages whose information could not be read are listed (`kind`, `problem`).",
    query: LIST_PARAMS,
    answer: Answer::Schema("PageList"),
    problems: &[
        Code::InvalidParameter,
        Code::InvalidCursor,
        Code::NoSuchView,
        Code::NotAvailable,
    ],
};

pub(crate) const PAGE: Doc = Doc {
    path: "/{slug}/-/api/v1/pages/{*title}",
    operation_id: "getPage",
    tag: "pages",
    summary: "Get a page: its information and its rendered body",
    description: "A page's metadata, its table of contents and its body as a sanitized HTML fragment (the same HTML a page view shows; its headings begin at `h2`). `stale` is true when the server could not ask for the newest version and answers with the last one it has.",
    query: VIEW_ONLY,
    answer: Answer::Schema("Page"),
    problems: PAGE_PROBLEMS,
};

pub(crate) const RAW: Doc = Doc {
    path: "/{slug}/-/api/v1/raw/{*title}",
    operation_id: "getRaw",
    tag: "pages",
    summary: "Say where a page's source is kept",
    description: "The page's title, its kind, the tip it was found at and its storage path. The source text itself is served only from the content origin (RFC 003 D-3) and never by this API.",
    query: VIEW_ONLY,
    answer: Answer::Schema("Raw"),
    problems: PAGE_PROBLEMS,
};

pub(crate) const FRAGMENT: Doc = Doc {
    path: "/{slug}/-/api/v1/fragment/{*title}",
    operation_id: "getFragment",
    tag: "pages",
    summary: "Get a page's body as an HTML fragment",
    description: "Only the sanitized HTML of the page body, as `text/html`: a fragment to place inside a document of your own, not a document. Links in it are the wiki's own page addresses.",
    query: VIEW_ONLY,
    answer: Answer::Fragment,
    problems: PAGE_PROBLEMS,
};

pub(crate) const HISTORY: Doc = Doc {
    path: "/{slug}/-/api/v1/history/{*title}",
    operation_id: "listHistory",
    tag: "history",
    summary: "List the changes to a page",
    description: "Not offered by this build: it answers `not-available`.",
    query: VIEW_PAGED,
    answer: Answer::NotOffered,
    problems: &[
        Code::NotAvailable,
        Code::InvalidTitle,
        Code::InvalidParameter,
        Code::NoSuchView,
    ],
};

pub(crate) const REVISION: Doc = Doc {
    path: "/{slug}/-/api/v1/revisions/{id}",
    operation_id: "getRevision",
    tag: "history",
    summary: "Get one recorded change",
    description: "Not offered by this build: it answers `not-available`.",
    query: &[],
    answer: Answer::NotOffered,
    problems: &[Code::NotAvailable, Code::InvalidParameter],
};

pub(crate) const DIFF: Doc = Doc {
    path: "/{slug}/-/api/v1/diff",
    operation_id: "getDiff",
    tag: "history",
    summary: "Compare one page between two points of the history",
    description: "`path` is required: a diff is always for one page, and the API has no diff of a whole tree. Not offered by this build beyond that check: it answers `not-available`.",
    query: DIFF_PARAMS,
    answer: Answer::NotOffered,
    problems: &[
        Code::DiffPathRequired,
        Code::NotAvailable,
        Code::InvalidParameter,
    ],
};

pub(crate) const BACKLINKS: Doc = Doc {
    path: "/{slug}/-/api/v1/backlinks/{*title}",
    operation_id: "listBacklinks",
    tag: "links",
    summary: "List the pages that link to a page",
    description: "Not offered by this build: it answers `not-available`.",
    query: VIEW_PAGED,
    answer: Answer::NotOffered,
    problems: &[
        Code::NotAvailable,
        Code::InvalidTitle,
        Code::InvalidParameter,
        Code::NoSuchView,
    ],
};

pub(crate) const HEALTH: Doc = Doc {
    path: "/{slug}/-/api/v1/health",
    operation_id: "getLinkHealth",
    tag: "links",
    summary: "Get the wiki's link health",
    description: "Not offered by this build: it answers `not-available`. (The liveness check of the server is `/-/health`, a different thing.)",
    query: VIEW_ONLY,
    answer: Answer::NotOffered,
    problems: &[Code::NotAvailable, Code::InvalidParameter, Code::NoSuchView],
};

pub(crate) const SEARCH: Doc = Doc {
    path: "/{slug}/-/api/v1/search",
    operation_id: "search",
    tag: "search",
    summary: "Search the pages of a wiki",
    description: "Not offered by this build: it answers `not-available`.",
    query: SEARCH_PARAMS,
    answer: Answer::NotOffered,
    problems: &[Code::NotAvailable, Code::InvalidParameter, Code::NoSuchView],
};

pub(crate) const CHANGES: Doc = Doc {
    path: "/{slug}/-/api/v1/changes",
    operation_id: "listChanges",
    tag: "history",
    summary: "List the wiki's recent changes",
    description: "Not offered by this build: it answers `not-available`.",
    query: VIEW_PAGED,
    answer: Answer::NotOffered,
    problems: &[Code::NotAvailable, Code::InvalidParameter, Code::NoSuchView],
};

/// The default and the largest size of a page of a list, for the document.
pub(crate) const SIZES: (usize, usize) = (DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
