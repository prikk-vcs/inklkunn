//! What the read API asks of the read service (RFC 003 D-13; handoff 003 T8; external design API-02): the
//! reads that are not a page view, in plain data a surface turns into JSON.
//!
//! **This file is the seam between the API and the read service.** The web crate's endpoints call the methods
//! below and the methods of the service that a page view calls too, and nothing else:
//!
//! | method | what it is |
//! |---|---|
//! | [`ReadService::list_pages`] | the title index at the view's tip, filtered, in keyset pages |
//! | [`ReadService::raw_info`] | where a page's source is (title, storage path, tip), from the index alone |
//! | [`ReadService::api_history`] | [`ReadService::page_history`] with the history's position as a cursor |
//! | [`ReadService::api_diff`] | [`ReadService::page_diff`] with the points a request may leave out |
//! | [`ReadService::api_backlinks`], [`api_health`](ReadService::api_health), [`api_changes`](ReadService::api_changes) | the link graph, the health lists and the recent changes, in bounded pages |
//!
//! The reads that need no shaping are the page views' own, called as they are: [`ReadService::page`],
//! [`ReadService::change`], [`ReadService::page_at`], [`ReadService::search`].
//!
//! **Bounded by construction.** A page of results never holds more than [`MAX_PAGE_SIZE`] items whatever the
//! caller asks for. A cursor is a position, and which kind of position depends on the list: the pages, the
//! history of a page and the changes are walked by the *last item seen* (a path, a change number, a change), so
//! a walk through a list that changes underneath it neither repeats nor skips an item that stayed; the
//! backlinks and the health lists, which are computed whole for one tip, are walked by an offset that is
//! stamped with that tip, and a cursor of another tip is refused (start again) rather than followed into a
//! different list.

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
const NUMBER_TAG: &str = "n1";
const OFFSET_TAG: &str = "o1";
const KEY_TAG: &str = "k1";

/// A position in a list. **Opaque to callers** (it is a version tag and what the list's order is keyed by; the
/// format may change, and a cursor from another format, or one made for another list, is refused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(String);

/// What a cursor says, once read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Position {
    /// After the page with this storage path (the list of pages).
    Path(String),
    /// After the change with this number in the view's history (the history of a page).
    Number(u64),
    /// At this offset of a list computed for the tip with this state id (backlinks, health).
    Offset { at: usize, stamp: String },
    /// After the change whose block id has this key (the recent changes): the key is a hash, so a cursor does
    /// not carry the id of a change the reader may not be allowed to see.
    Key(String),
}

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

    /// The position just after the change with this number.
    fn after_number(number: u64) -> Cursor {
        Cursor(format!("{NUMBER_TAG}{number}"))
    }

    /// The position at this offset of the list computed for the tip `stamp`.
    fn at_offset(at: usize, stamp: &str) -> Cursor {
        Cursor(format!("{OFFSET_TAG}{at}.{stamp}"))
    }

    /// The position just after the change in block `block_id`.
    fn after_change(block_id: &str) -> Cursor {
        Cursor(format!("{KEY_TAG}{}", change_key(block_id)))
    }

    /// The cursor a caller sent back, if it is one this service gave out.
    pub fn parse(text: &str) -> Option<Cursor> {
        Cursor::decode(text).map(|_| Cursor(text.to_owned()))
    }

    /// The text a caller sends back.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// What the cursor says.
    fn position(&self) -> Option<Position> {
        Cursor::decode(&self.0)
    }

    fn decode(text: &str) -> Option<Position> {
        if text.len() > MAX_CURSOR_CHARS {
            return None;
        }
        if let Some(digits) = text.strip_prefix(CURSOR_TAG) {
            return Cursor::decode_path(digits).map(Position::Path);
        }
        if let Some(digits) = text.strip_prefix(NUMBER_TAG) {
            return canonical(digits).filter(|n| *n >= 1).map(Position::Number);
        }
        if let Some(rest) = text.strip_prefix(OFFSET_TAG) {
            let (at, stamp) = rest.split_once('.')?;
            let at = usize::try_from(canonical(at)?).ok()?;
            return is_hex(stamp, 1..=64).then(|| Position::Offset {
                at,
                stamp: stamp.to_owned(),
            });
        }
        let key = text.strip_prefix(KEY_TAG)?;
        is_hex(key, 16..=16).then(|| Position::Key(key.to_owned()))
    }

    /// The storage path a cursor of the list of pages is after: the path in lower-case hexadecimal, a path of
    /// a page and no longer than a repository path may be.
    fn decode_path(digits: &str) -> Option<String> {
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

/// Is `text` lower-case hexadecimal of a length in `lengths`?
fn is_hex(text: &str, lengths: std::ops::RangeInclusive<usize>) -> bool {
    lengths.contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A whole number in its one spelling (digits only, no leading zero): a cursor that is not what this service
/// would have written is not one it gave out.
fn canonical(digits: &str) -> Option<u64> {
    let number: u64 = digits.parse().ok()?;
    (number.to_string() == digits).then_some(number)
}

/// A short, stable key for a change: FNV-1a over its block id. It only has to tell the changes of one short
/// list apart, and it must not be the id itself.
fn change_key(block_id: &str) -> String {
    fnv_hex(block_id)
}

/// What an offset cursor is stamped with: the tip the list was computed for and which list it is, so a cursor
/// of another tip, or of another list, is not followed.
fn list_stamp(tip: &str, list: &str) -> String {
    fnv_hex(&format!("{tip}|{list}"))
}

/// FNV-1a over the bytes, as 16 lower-case hexadecimal digits.
fn fnv_hex(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
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

/// What a read of the API cannot give yet. Each names the filter that is not offered, so the answer says what
/// is missing and a client can tell one gap from another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// The `tag` filter of the list of pages.
    TagFilter,
    /// The `changed_since` filter of the list of pages.
    ChangedSince,
}

impl Feature {
    /// The stable name an answer carries.
    pub const fn name(self) -> &'static str {
        match self {
            Feature::TagFilter => "tag-filter",
            Feature::ChangedSince => "changed-since",
        }
    }
}

/// Why an API read gave nothing.
#[derive(Debug, Clone)]
pub enum ApiError {
    /// The read failed for a reason the page view has too.
    Read(ReadError),
    /// This build does not offer the filter yet.
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
        let after = match after.map(Cursor::position) {
            Some(Some(Position::Path(path))) => Some(path),
            Some(_) => return Err(ApiError::InvalidCursor),
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
}

mod discover;
mod past;

pub use discover::{
    BacklinkAnswer, BacklinkPage, ChangePage, HealthList, HealthPage, HealthRows, StaleScan,
};
pub use past::HistoryAnswer;

#[cfg(test)]
mod tests;
