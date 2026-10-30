//! The names a proposal's forms and addresses are made of: form fields, their values and the query parameters
//! the entry points use. Code, not words: nothing here is a sentence a reader sees, so the file is scoped in
//! `tests/literals.allow`, and the handlers and templates name these constants, never a bare string.

// --- which button was pressed
pub(crate) const OP: &str = "op";
pub(crate) const OP_PREVIEW: &str = "preview";
pub(crate) const OP_CHANGES: &str = "changes";
pub(crate) const OP_DRAFT: &str = "draft";
pub(crate) const OP_SUBMIT: &str = "submit";
pub(crate) const OP_APPROVE: &str = "approve";
pub(crate) const OP_REQUEST_CHANGES: &str = "request-changes";

// --- the proposal form
pub(crate) const BASE_STATE: &str = "base_state";
pub(crate) const BASE_BLOCK: &str = "base_block";
pub(crate) const VIEW: &str = "view";
pub(crate) const SUMMARY: &str = "summary";
pub(crate) const SOURCES: &str = "sources";
pub(crate) const PAGES: &str = "pages";
pub(crate) const RENAMES: &str = "renames";
pub(crate) const PART: &str = "part";
pub(crate) const ORDINAL: &str = "ordinal";

pub(crate) const PART_WHOLE: &str = "whole";
pub(crate) const PART_SECTION: &str = "section";
pub(crate) const PART_BLOCK: &str = "block";

/// The name of field `what` of the `i`-th page of a form (`p0_text`).
pub(crate) fn page_field(i: usize, what: &str) -> String {
    format!("p{i}_{what}")
}
pub(crate) const F_PATH: &str = "path";
pub(crate) const F_TITLE: &str = "title";
pub(crate) const F_TEXT: &str = "text";
pub(crate) const F_NEW: &str = "new";
pub(crate) const F_REMOVED: &str = "removed";
/// The name of field `what` of the `i`-th declared rename (`rn0_from`).
pub(crate) fn rename_field(i: usize, what: &str) -> String {
    format!("rn{i}_{what}")
}
pub(crate) const F_FROM: &str = "from";
pub(crate) const F_TO: &str = "to";

// --- query parameters of the entry points
pub(crate) const Q_PAGE: &str = "page";
pub(crate) const Q_NEW: &str = "new";
pub(crate) const Q_TITLE: &str = "title";
pub(crate) const Q_PARTS: &str = "parts";
pub(crate) const Q_SAVED: &str = "saved";
pub(crate) const Q_REPLY: &str = "reply";
pub(crate) const Q_ON: &str = "on";

// --- the discussion on a proposal
pub(crate) const BODY: &str = "body";
pub(crate) const PARENT: &str = "parent";
pub(crate) const ANCHOR_PAGE: &str = "anchor_page";
pub(crate) const ANCHOR_ORDINAL: &str = "anchor_ordinal";
pub(crate) const ANCHOR_REVISION: &str = "anchor_revision";
pub(crate) const REVISION: &str = "revision";
pub(crate) const REASON: &str = "reason";

// --- element ids a page's fragments point at
pub(crate) const ID_COMMENT: &str = "comment";
pub(crate) const ID_THREAD: &str = "thread";

/// The frontmatter keys only the service writes (RFC 004 D-2; threat model C-4b).
pub(crate) const OWNED_KEYS: [&str; 3] = ["kind", "visibility", "system"];
