//! What the read API asks of the read service (RFC 003 D-13; handoff 003 T8; external design API-02): the
//! reads that are not a page view, in plain data a surface turns into JSON.
//!
//! **This file is the seam between the API and the read service.** The web crate's endpoints call the methods
//! below and nothing else of the service that a page view does not already call, so the other read tasks
//! (T5 history and diff, T6 search, changes, links and health) land their reads by *filling in* the methods
//! that are stubs here, without touching the endpoints' shape:
//!
//! | method | state |
//! |---|---|
//! | [`ReadService::list_pages`] | real: the title index at the view's tip, filtered, in keyset pages |
//! | [`ReadService::raw_info`] | real: where a page's source is (title, storage path, tip), from the index alone |
//! | [`ReadService::api_history`], [`api_revision`](ReadService::api_revision), [`api_diff`](ReadService::api_diff), [`api_backlinks`](ReadService::api_backlinks), [`api_health`](ReadService::api_health), [`api_search`](ReadService::api_search), [`api_changes`](ReadService::api_changes) | **stubs**: answer [`ApiError::NotAvailable`] until the task that owns the read replaces the body and the [`Pending`] return type with its own |
//!
//! The page itself (metadata and the sanitized fragment) is [`ReadService::page`], unchanged.
//!
//! **Bounded by construction.** A page of results never holds more than [`MAX_PAGE_SIZE`] items whatever the
//! caller asks for, and a cursor is a position (the last item of the previous page), not an offset, so a
//! walk through a list that changes underneath it neither repeats nor skips an item that stayed.

use inklkunn_store::WikiRecord;

use super::{ReadError, ReadService, Tip};

/// The most items a page of a list holds. Asking for more is refused by the surface, not trimmed: a client is
/// told its request was not served as asked.
pub const MAX_PAGE_SIZE: usize = 100;

/// How many items a page of a list holds when the caller does not say.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// The longest cursor accepted, in characters: a path of at most 1024 bytes in hex, with its version tag.
const MAX_CURSOR_CHARS: usize = 2 * 1024 + 8;

const CURSOR_TAG: &str = "c1";

/// A position in a list: the last item of the page before. **Opaque to callers** (it is a version tag and the
/// item's sort key in hexadecimal; the format may change, and a cursor from another format is refused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(String);

impl Cursor {
    /// The position just after the item with this storage path.
    fn after_path(path: &str) -> Cursor {
        let mut out = String::with_capacity(CURSOR_TAG.len() + path.len() * 2);
        out.push_str(CURSOR_TAG);
        for byte in path.bytes() {
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
        Cursor(out)
    }

    /// The cursor a caller sent back, if it is one this service gave out.
    pub fn parse(text: &str) -> Option<Cursor> {
        Cursor::decode(text).map(|_| Cursor(text.to_owned()))
    }

    /// The text a caller sends back.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The storage path a cursor text is after: a version tag, then the path in lower-case hexadecimal, a
    /// path of a page and no longer than a repository path may be.
    fn decode(text: &str) -> Option<String> {
        if text.len() > MAX_CURSOR_CHARS {
            return None;
        }
        let digits = text.strip_prefix(CURSOR_TAG)?;
        if digits.is_empty() || !digits.len().is_multiple_of(2) {
            return None;
        }
        let bytes: Option<Vec<u8>> = digits
            .as_bytes()
            .chunks(2)
            .map(|pair| match pair {
                [hi, lo] => Some(hex_value(*hi)? << 4 | hex_value(*lo)?),
                _ => None,
            })
            .collect();
        let path = String::from_utf8(bytes?).ok()?;
        path.starts_with("pages/").then_some(path)
    }
}

fn hex_digit(nibble: u8) -> char {
    char::from(
        b"0123456789abcdef"
            .get(usize::from(nibble))
            .copied()
            .unwrap_or(b'0'),
    )
}

fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// Which pages a list holds. Every filter that is set must match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageFilter {
    /// Pages in this namespace: titles whose hierarchy (everything before the last `/`) is this name or
    /// begins with it and a `/` (external design GATED-6: namespaces are directories).
    pub namespace: Option<String>,
    /// Pages of this kind (`page`, `redirect`, …).
    pub kind: Option<String>,
    /// Pages with this tag. **Not offered yet**: the title index does not carry tags ([`Feature::TagFilter`]).
    pub tag: Option<String>,
    /// Pages changed since this Unix time. **Not offered yet**: it needs the history ([`Feature::ChangedSince`]).
    pub changed_since: Option<i64>,
}

/// One page's line in a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedPage {
    /// The title: a page's address.
    pub title: String,
    /// `page`, `redirect`, …
    pub kind: String,
    /// Other titles that lead here.
    pub aliases: Vec<String>,
    /// The page's stable id, if it declares one.
    pub id: Option<String>,
    /// A stable code naming why the page's information could not be read, `None` when it could.
    pub problem: Option<String>,
}

/// One page of a list of pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageList {
    /// The tip the list was read at.
    pub tip: Tip,
    /// The items, in the list's fixed order, at most the size asked for.
    pub items: Vec<ListedPage>,
    /// Where the next page begins, `None` on the last.
    pub next: Option<Cursor>,
}

/// What a page's raw source is called and where it is kept. The source itself is served on the content
/// origin only (RFC 003 D-3); the API says where it is and never carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawInfo {
    /// The tip the page was found at.
    pub tip: Tip,
    /// The page's title.
    pub title: String,
    /// Where the page's file is kept in the wiki's repository: the **storage path**, labelled as such wherever
    /// it is shown.
    pub storage_path: String,
    /// `page`, `redirect`, …
    pub kind: String,
}

/// What a read of the API cannot give yet. Each names the read that will, so the answer says what is missing
/// and a client can tell one gap from another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// The `tag` filter of the list of pages.
    TagFilter,
    /// The `changed_since` filter of the list of pages.
    ChangedSince,
    /// The history of a page.
    History,
    /// One recorded change.
    Revision,
    /// The difference between two points, for one page.
    Diff,
    /// The pages that link to a page.
    Backlinks,
    /// The wiki's link health.
    Health,
    /// Search.
    Search,
    /// Recent changes.
    Changes,
}

impl Feature {
    /// The stable name an answer carries.
    pub const fn name(self) -> &'static str {
        match self {
            Feature::TagFilter => "tag-filter",
            Feature::ChangedSince => "changed-since",
            Feature::History => "history",
            Feature::Revision => "revision",
            Feature::Diff => "diff",
            Feature::Backlinks => "backlinks",
            Feature::Health => "health",
            Feature::Search => "search",
            Feature::Changes => "changes",
        }
    }
}

/// Why an API read gave nothing.
#[derive(Debug, Clone)]
pub enum ApiError {
    /// The read failed for a reason the page view has too.
    Read(ReadError),
    /// This build does not offer the read yet.
    NotAvailable(Feature),
    /// The cursor is not one this service gave out.
    InvalidCursor,
}

impl From<ReadError> for ApiError {
    fn from(e: ReadError) -> Self {
        ApiError::Read(e)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Read(e) => write!(f, "{e}"),
            ApiError::NotAvailable(feature) => {
                write!(
                    f,
                    "the read `{}` is not available in this build",
                    feature.name()
                )
            }
            ApiError::InvalidCursor => f.write_str("the cursor is not one this service gave out"),
        }
    }
}

impl std::error::Error for ApiError {}

/// The result type of a read that has no data type yet: it has no values, so a stub can only be an error and
/// the surface cannot be written to depend on a shape that is not decided. The task that fills the read in
/// replaces this with its own type.
#[derive(Debug, Clone, Copy)]
pub enum Pending {}

/// The title's namespace: the part before the last `/`, if there is one.
fn namespace_of(title: &str) -> Option<&str> {
    title.rsplit_once('/').map(|(namespace, _)| namespace)
}

fn in_namespace(title: &str, wanted: &str) -> bool {
    namespace_of(title).is_some_and(|ns| {
        ns == wanted
            || ns
                .strip_prefix(wanted)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

impl ReadService {
    /// One page of the pages of the view's tip: those that match `filter`, after `after`, at most `limit`
    /// (never more than [`MAX_PAGE_SIZE`], and at least one). The order is fixed (the title index's), so a
    /// walk with each page's [`PageList::next`] visits every matching page once.
    pub fn list_pages(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        filter: &PageFilter,
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<PageList, ApiError> {
        if filter.tag.is_some() {
            return Err(ApiError::NotAvailable(Feature::TagFilter));
        }
        if filter.changed_since.is_some() {
            return Err(ApiError::NotAvailable(Feature::ChangedSince));
        }
        let after = match after {
            Some(cursor) => Some(Cursor::decode(&cursor.0).ok_or(ApiError::InvalidCursor)?),
            None => None,
        };
        let limit = limit.clamp(1, MAX_PAGE_SIZE);
        let tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        // The entries are sorted by path, so "after the cursor" is a partition point, not a scan.
        let entries = index.entries();
        let start = after.as_deref().map_or(0, |path| {
            entries.partition_point(|entry| entry.path.as_str() <= path)
        });
        let mut items = Vec::with_capacity(limit.min(entries.len()));
        let mut more = false;
        let mut last_path = None;
        for entry in entries.iter().skip(start) {
            let matches = filter
                .namespace
                .as_deref()
                .is_none_or(|ns| in_namespace(&entry.title, ns))
                && filter.kind.as_deref().is_none_or(|k| entry.kind == k);
            if !matches {
                continue;
            }
            if items.len() == limit {
                more = true;
                break;
            }
            last_path = Some(entry.path.as_str());
            items.push(ListedPage {
                title: entry.title.clone(),
                kind: entry.kind.clone(),
                aliases: entry.aliases.clone(),
                id: entry.page_id.clone(),
                problem: entry.problem.clone(),
            });
        }
        let next = if more {
            last_path.map(Cursor::after_path)
        } else {
            None
        };
        Ok(PageList { tip, items, next })
    }

    /// Where the page titled `title` keeps its source: found as a page view finds it (an alias or a redirect
    /// leads to the page it names), from the index alone, so no page is read or rendered. `None` when the
    /// title leads to no single page.
    pub fn raw_info(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
    ) -> Result<Option<RawInfo>, ApiError> {
        inklkunn_markup::path::title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        Ok(match index.lookup(title) {
            super::Lookup::Found(found) => Some(RawInfo {
                tip,
                title: found.entry.title,
                storage_path: found.entry.path,
                kind: found.entry.kind,
            }),
            _ => None,
        })
    }

    /// The changes to one page, newest first. **Stub** (T5 owns the read).
    pub fn api_history(
        &self,
        _wiki: &WikiRecord,
        _view: Option<&str>,
        _title: &str,
    ) -> Result<Pending, ApiError> {
        Err(ApiError::NotAvailable(Feature::History))
    }

    /// One recorded change, by the id of its step in the history. **Stub** (T5 owns the read).
    pub fn api_revision(&self, _wiki: &WikiRecord, _id: &str) -> Result<Pending, ApiError> {
        Err(ApiError::NotAvailable(Feature::Revision))
    }

    /// What changed in one page between two points of the history. `path` is the page's storage path: there
    /// is no read of a whole tree, so there is no way to ask for one. **Stub** (T5 owns the read).
    pub fn api_diff(
        &self,
        _wiki: &WikiRecord,
        _from: Option<&str>,
        _to: Option<&str>,
        _path: &str,
    ) -> Result<Pending, ApiError> {
        Err(ApiError::NotAvailable(Feature::Diff))
    }

    /// The pages that link to the page titled `title`. **Stub** (T6 owns the link graph).
    pub fn api_backlinks(
        &self,
        _wiki: &WikiRecord,
        _view: Option<&str>,
        _title: &str,
    ) -> Result<Pending, ApiError> {
        Err(ApiError::NotAvailable(Feature::Backlinks))
    }

    /// The wiki's link health: wanted pages, ambiguous and broken links. **Stub** (T6 owns it).
    pub fn api_health(&self, _wiki: &WikiRecord, _view: Option<&str>) -> Result<Pending, ApiError> {
        Err(ApiError::NotAvailable(Feature::Health))
    }

    /// Search over the wiki's pages. **Stub** (T6 owns it).
    pub fn api_search(
        &self,
        _wiki: &WikiRecord,
        _view: Option<&str>,
        _query: &str,
    ) -> Result<Pending, ApiError> {
        Err(ApiError::NotAvailable(Feature::Search))
    }

    /// The wiki's recent changes. **Stub** (T6 owns it).
    pub fn api_changes(
        &self,
        _wiki: &WikiRecord,
        _view: Option<&str>,
    ) -> Result<Pending, ApiError> {
        Err(ApiError::NotAvailable(Feature::Changes))
    }
}

#[cfg(test)]
mod tests;
