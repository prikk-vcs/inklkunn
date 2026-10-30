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
    /// One of these words.
    Choice(&'static [&'static str]),
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

const FROM: Param = Param {
    name: "from",
    kind: Kind::Hex { max: 64 },
    required: false,
    missing: Code::InvalidParameter,
    description: "The point to compare from: the `id` of a change of the view's history. Without it, the change before `to`.",
};

const TO: Param = Param {
    name: "to",
    kind: Kind::Hex { max: 64 },
    required: false,
    missing: Code::InvalidParameter,
    description: "The point to compare to: the `id` of a change of the view's history. Without it, the tip of the view.",
};

const PATH: Param = Param {
    name: "path",
    kind: Kind::Text { max: 1024 },
    required: true,
    missing: Code::DiffPathRequired,
    description: "The title of the page the diff is for (an address, as everywhere in this API: an alias or a redirect leads to the page it names). Required: a diff is always for one page, and the API has no diff of a whole tree.",
};

const QUERY: Param = Param {
    name: "q",
    kind: Kind::Text { max: 500 },
    required: true,
    missing: Code::InvalidParameter,
    description: "What to search for.",
};

const REVISION_TITLE: Param = text(
    "title",
    1024,
    false,
    "A page's title. With it the answer also carries that page as it stood after this change (`page`); without it, `page` is `null`.",
);

const HEALTH_LIST: Param = Param {
    name: "list",
    kind: Kind::Choice(&[
        "wanted",
        "orphans",
        "dead-ends",
        "redirects",
        "wants-help",
        "stale",
    ]),
    required: true,
    missing: Code::InvalidParameter,
    description: "Which list: `wanted` (titles that links lead to and no page has), `orphans` (pages nothing links to), `dead-ends` (pages that link to no page), `redirects` (redirects that cannot be followed or take two hops), `wants-help` (pages marked work in progress, draft or stub), `stale` (pages that have gone longest without a change).",
};

const CHANGES_NAMESPACE: Param = text(
    "namespace",
    512,
    false,
    "Only changes that touched a page whose title is in this namespace: the part before the last `/` is this name, or begins with it and a `/`.",
);

const AUTOMATED: Param = Param {
    name: "automated",
    kind: Kind::Choice(&["show", "hide", "only"]),
    required: false,
    missing: Code::InvalidParameter,
    description: "The changes the wiki service made itself (imports, creating the wiki) or that were proposed as machine-assisted: `show` them (the default), `hide` them, or list `only` them.",
};

const MINOR: Param = Param {
    name: "minor",
    kind: Kind::Choice(&["show", "hide", "only"]),
    required: false,
    missing: Code::InvalidParameter,
    description: "The changes that were proposed as minor: `show` them (the default), `hide` them, or list `only` them.",
};

// ------------------------------------------------------------------------------------ the endpoints

const LIST_PARAMS: &[Param] = &[VIEW, LIMIT, CURSOR, NAMESPACE, KIND, TAG, CHANGED_SINCE];
const VIEW_ONLY: &[Param] = &[VIEW];
const VIEW_PAGED: &[Param] = &[VIEW, LIMIT, CURSOR];
const HISTORY_PARAMS: &[Param] = &[VIEW, CURSOR];
const REVISION_PARAMS: &[Param] = &[VIEW, REVISION_TITLE];
const SEARCH_PARAMS: &[Param] = &[QUERY, VIEW, LIMIT];
const DIFF_PARAMS: &[Param] = &[PATH, FROM, TO, VIEW];
const HEALTH_PARAMS: &[Param] = &[HEALTH_LIST, VIEW, LIMIT, CURSOR];
const CHANGES_PARAMS: &[Param] = &[VIEW, LIMIT, CURSOR, CHANGES_NAMESPACE, AUTOMATED, MINOR];

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
    description: "The changes that touched the page, newest first, one step of at most 20 rows at a time. A step looks at as many changes as one request may, so it can hold no row and still have a `next_cursor`: follow it until it is `null`. `beyond_window` is true when the history is longer than the server reads, so the oldest changes are not listed. A row says what the change did to the page (`events`) and what its message says about the people (`provenance`, and `people` in the words the pages use): the wiki service signs the history, and no row says that a person signed. A message that is not in the service's form names nobody. There is no time: the history carries none.",
    query: HISTORY_PARAMS,
    answer: Answer::Schema("History"),
    problems: &[
        Code::AmbiguousTitle,
        Code::InvalidTitle,
        Code::InvalidParameter,
        Code::InvalidCursor,
        Code::NoSuchView,
    ],
};

pub(crate) const REVISION: Doc = Doc {
    path: "/{slug}/-/api/v1/revisions/{id}",
    operation_id: "getRevision",
    tag: "history",
    summary: "Get one recorded change",
    description: "One change of the view's history, by the `id` a history or a list of changes gave: what it did (`did`, at most 200 things, `omitted` counts the rest), what its message says about the people, and, when `title` is given, the page as it stood after the change (the permalink of the page's version; a title that had no page then is `page-not-found`). A change whose operations are too large to list says so (`too_large`). An id that is not a change of this view is `change-not-found`, whatever the reason.",
    query: REVISION_PARAMS,
    answer: Answer::Schema("Revision"),
    problems: &[
        Code::ChangeNotFound,
        Code::PageNotFound,
        Code::AmbiguousTitle,
        Code::PageUnreadable,
        Code::InvalidTitle,
        Code::InvalidParameter,
        Code::NoSuchView,
    ],
};

pub(crate) const DIFF: Doc = Doc {
    path: "/{slug}/-/api/v1/diff",
    operation_id: "getDiff",
    tag: "history",
    summary: "Compare one page between two points of the history",
    description: "`path` is required and is the page's title: a diff is always for one page, and the API has no diff of a whole tree. The lines are the version control's own; `marked` segments are the wiki's word-level refinement of a removed line followed by an added one, and never change a character of a line. At most 4000 lines are returned (`truncated`). A page that is the same at both points is `unchanged`, one that exists at `to` only is `added`, at `from` only is `removed`.",
    query: DIFF_PARAMS,
    answer: Answer::Schema("Diff"),
    problems: &[
        Code::DiffPathRequired,
        Code::ChangeNotFound,
        Code::AmbiguousTitle,
        Code::InvalidTitle,
        Code::InvalidParameter,
        Code::NoSuchView,
    ],
};

pub(crate) const BACKLINKS: Doc = Doc {
    path: "/{slug}/-/api/v1/backlinks/{*title}",
    operation_id: "listBacklinks",
    tag: "links",
    summary: "List the pages that link to a page",
    description: "The pages that link to the page the title leads to (an alias or a redirect leads to the page it names), each with the line of its source that makes the link, in the order of the linking pages' storage paths. For a title that leads to no page but is linked to, the pages that link to it (`exists` is false: a wanted title). The cursor is an offset into the list of the tip it was given for; once the wiki has changed it is refused with `invalid-cursor` and the walk starts again. The graph keeps at most 500 links for a title (`cut`).",
    query: VIEW_PAGED,
    answer: Answer::Schema("Backlinks"),
    problems: &[
        Code::PageNotFound,
        Code::AmbiguousTitle,
        Code::InvalidTitle,
        Code::InvalidParameter,
        Code::InvalidCursor,
        Code::NoSuchView,
    ],
};

pub(crate) const HEALTH: Doc = Doc {
    path: "/{slug}/-/api/v1/health",
    operation_id: "getLinkHealth",
    tag: "links",
    summary: "Get one of the wiki's link health lists",
    description: "What can be improved in the wiki, from the link graph of the tip: one of six lists, each row with the `action` that would resolve it as a code (`add-a-link-to-it`, `retarget-the-redirect`, …; the words are the client's to choose). These are opportunities, not faults. The list is walked by `next_cursor` like the backlinks. The stale list looks only through the newest changes (`scan`), because the history does not say which pages a change touched until it is asked. (The liveness check of the server is `/-/health`, a different thing.)",
    query: HEALTH_PARAMS,
    answer: Answer::Schema("Health"),
    problems: &[
        Code::InvalidParameter,
        Code::InvalidCursor,
        Code::NoSuchView,
    ],
};

pub(crate) const SEARCH: Doc = Doc {
    path: "/{slug}/-/api/v1/search",
    operation_id: "search",
    tag: "search",
    summary: "Search the pages of a wiki",
    description: "Search over titles and aliases: any part of them matches, with width variants, diacritics and case folded. Best first: an exact title, an exact alias, a title that begins with the query, an alias that does, a title that contains it, an alias that does. A hit says whether its title or an alias matched (`kind`) and shows the text that matched as `snippet`: a list of segments of plain text, `marked` for the part that matched. A snippet is never HTML; write it with your own escaping. At most 50 pages are found whatever `limit` is (`truncated` says there were more): narrow the query for the others. `names_a_page` is true when the query is itself a title, an alias or a redirect.",
    query: SEARCH_PARAMS,
    answer: Answer::Schema("Search"),
    problems: &[Code::InvalidParameter, Code::NoSuchView],
};

pub(crate) const CHANGES: Doc = Doc {
    path: "/{slug}/-/api/v1/changes",
    operation_id: "listChanges",
    tag: "history",
    summary: "List the wiki's recent changes",
    description: "The changes of the wiki's open views, newest first (the order they were first seen by this server), looking through the newest 100 changes of each view (`beyond_window` says there are older ones). What the caller may not read is left out and counted in `left_out`, without saying which; a change is left out when it touched pages and the caller may read none of them. Each row says what its message says about the people (`proposed_by`, `accepted_by`, `kind`, and `people` in the words the pages use) and no more: the wiki service signs the history, and no row says a person signed. `first_seen` is when this server first saw the change, not when it was made. The cursor names the last change of the page; one that names a change no longer in the list is `invalid-cursor`.",
    query: CHANGES_PARAMS,
    answer: Answer::Schema("Changes"),
    problems: &[
        Code::InvalidParameter,
        Code::InvalidCursor,
        Code::NoSuchView,
    ],
};

/// The default and the largest size of a page of a list, for the document.
pub(crate) const SIZES: (usize, usize) = (DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
