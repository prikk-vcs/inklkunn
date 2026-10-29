//! The derived cache (RFC 003 D-2; internal design DM-6, DM-7; threat model T-10, INV-5): rendered pages and
//! the title index of a view's tip, **keyed by prikk's ids and never authoritative**. Everything here can be
//! thrown away and rebuilt from the repository, and a test does (ENF-5).
//!
//! **The cache never holds a verification result, and the types say so**: [`DerivedPage`], [`IndexEntry`]
//! and the tables behind them have no field for a verdict, a state or a time of a verification, so no
//! caller *can* put one in and a page served from here cannot carry a badge. The badge is read from the wiki
//! record's last verification at the moment of display (RFC 003 D-5, RD-3).
//!
//! The store keeps these values as the service wrote them and judges none of them: the HTML in a
//! [`DerivedPage`] was sanitized by the writer (`inklkunn-core`), and the reader sanitizes it again before
//! it goes anywhere, so a row altered in the database is no more trusted than any other input.

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::record::WikiId;

/// What a cached page is keyed by. A page is the same page at the same tip, path, language **and
/// presentation**: the cached HTML contains the hrefs and the catalog words the reader's presentation chose
/// (the URL scheme of the wiki and view it was asked for, the language's sentences), so a wiki renamed
/// (a new URL base) or read in another language is another key and never the old HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageKey<'a> {
    /// The wiki.
    pub wiki: &'a WikiId,
    /// The view tip's RefState id (prikk changes it whenever the view moves, so it keys caches).
    pub tip: &'a str,
    /// The page's repository path.
    pub path: &'a str,
    /// The catalog language (a BCP 47 tag).
    pub lang: &'a str,
    /// The presentation key: what else the written HTML depends on (link base, view).
    pub presentation: &'a str,
}

/// The metadata of a page as the page view needs it without reading the file again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedMeta {
    /// The page's title.
    pub title: String,
    /// The page's stable id (frontmatter `id`).
    pub id: Option<String>,
    /// `page`, `redirect`, … (META-7).
    pub kind: String,
    /// Lifecycle state (META-6).
    pub status: String,
    /// The page's language (META-10).
    pub lang: Option<String>,
    /// The page's license overriding the wiki default (META-9).
    pub license: Option<String>,
    /// One or two sentences for previews. Plain text: a surface escapes it.
    pub description: Option<String>,
    /// The page's tags.
    pub tags: Vec<String>,
    /// Other titles that resolve here. Plain text: a surface escapes them.
    pub aliases: Vec<String>,
}

/// The links of a page as the renderer resolved them at one tip (the link graph's edges out of the page).
/// Sorted and without duplicates, so a rebuild is byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DerivedLinks {
    /// `(target as written, page path)` of every link that led to one page.
    pub resolved: Vec<(String, String)>,
    /// Targets as written that led nowhere (the dangling links, NAV-3).
    pub dangling: Vec<String>,
    /// `(target as written, candidate paths)` of every link that matched several pages (NAV-15).
    pub ambiguous: Vec<(String, Vec<String>)>,
    /// Interwiki links as written (`prefix:Title`).
    pub interwiki: Vec<String>,
    /// Paths of the pages this page includes (transclusion; NAV-16).
    pub transcluded: Vec<String>,
}

/// One heading of a page, for a table of contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedHeading {
    /// 1–6.
    pub level: u8,
    /// The `id` attribute written into the HTML.
    pub id: String,
    /// The plain text.
    pub text: String,
}

/// A rendered page: derived, sanitized HTML and what was learned while rendering it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedPage {
    /// The page body as HTML, sanitized by the writer.
    pub html: String,
    /// The metadata the page view shows.
    pub meta: DerivedMeta,
    /// The links the page makes.
    pub links: DerivedLinks,
    /// The headings, in order.
    pub toc: Vec<DerivedHeading>,
    /// Unix seconds.
    pub computed_at: i64,
}

/// A page of the title index at a tip (DM-7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// The page's repository path (`pages/…md`).
    pub path: String,
    /// The title: the frontmatter's, else the one the path spells.
    pub title: String,
    /// Other titles that resolve here (META-4).
    pub aliases: Vec<String>,
    /// The redirect target, when the page only redirects (META-4).
    pub redirect: Option<String>,
    /// The page's stable id (META-2).
    pub page_id: Option<String>,
    /// `page`, `redirect`, … (META-7).
    pub kind: String,
    /// A stable code naming why the page's metadata could not be read, `None` when it could. The page is
    /// still listed: a page that cannot be parsed must be findable to be fixed.
    pub problem: Option<String>,
}

/// What a searchable text of a page is: its title, or one of its aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    /// The page's title.
    Title,
    /// One of the page's aliases.
    Alias,
}

impl SearchKind {
    /// The stable code stored with the text.
    pub const fn code(self) -> &'static str {
        match self {
            SearchKind::Title => "title",
            SearchKind::Alias => "alias",
        }
    }

    /// Read a code back; `None` for one this build does not know (the row is then a miss).
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "title" => Some(SearchKind::Title),
            "alias" => Some(SearchKind::Alias),
            _ => None,
        }
    }
}

/// One text the search finds a page by (RFC 003 D-8): the text as the page wrote it, and the **folded** form
/// the search compares (case, diacritics and width variants folded by the writer; the store compares what it
/// is given and judges none of it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDoc {
    /// The page's repository path.
    pub path: String,
    /// A title or an alias.
    pub kind: SearchKind,
    /// The text as written. Plain text: a surface escapes it.
    pub text: String,
    /// The text, folded.
    pub folded: String,
}

/// A text the search found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// The page's repository path.
    pub path: String,
    /// A title or an alias.
    pub kind: SearchKind,
    /// The text as written.
    pub text: String,
}

/// The derived cache. Every implementation passes the same invariant tests (`tests/derived.rs`).
///
/// **A row that cannot be read back is a miss**, never an error and never a made-up value: the cache is
/// derived, so the right answer to a damaged row is to derive it again.
pub trait DerivedStore: Send + Sync {
    /// The cached page under `key`, if any.
    fn page(&self, key: &PageKey<'_>) -> Result<Option<DerivedPage>>;
    /// Cache a page under `key`, replacing what was there.
    fn put_page(&self, key: &PageKey<'_>, page: &DerivedPage) -> Result<()>;
    /// The title index of `wiki` at `tip`, by path, if it was stored complete.
    fn title_index(&self, wiki: &WikiId, tip: &str) -> Result<Option<Vec<IndexEntry>>>;
    /// Store the complete title index of `wiki` at `tip`, replacing what was there. The index becomes
    /// visible all at once: a reader sees all of it or none.
    fn put_title_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        entries: &[IndexEntry],
        built_at: i64,
    ) -> Result<()>;
    /// Drop everything cached for `wiki` at any tip **not** in `keep`; how many cached items went (a page
    /// counts once and so does a whole index). Called after a tip moves, with the tips every view of the
    /// wiki has now, so the cache holds the present and not the past.
    fn retain_tips(&self, wiki: &WikiId, keep: &[String]) -> Result<u64>;
    /// Drop everything cached for `wiki` (the derived store cleared, ENF-5); how many cached items went,
    /// counted as in [`DerivedStore::retain_tips`]. **Not the observations** ([`DerivedStore::first_seen`]):
    /// they are not derived from the repository and cannot be rebuilt from it.
    fn clear(&self, wiki: &WikiId) -> Result<u64>;

    /// Store the complete search index of `wiki` at `tip` (every title and alias), replacing what was there.
    /// It becomes visible all at once, like the title index.
    fn put_search_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        docs: &[SearchDoc],
        built_at: i64,
    ) -> Result<()>;
    /// The texts of `wiki` at `tip` that **contain** `folded_query` (a substring of the folded text, so a
    /// Japanese title is found by any part of it), in the order they were stored, at most `limit`. `None`
    /// when no complete index is stored for the tip.
    fn search(
        &self,
        wiki: &WikiId,
        tip: &str,
        folded_query: &str,
        limit: usize,
    ) -> Result<Option<Vec<SearchHit>>>;

    /// When this service first saw each block of `wiki`, in Unix seconds, in the order asked: a block not
    /// seen before is recorded as seen at `now`. **An observation, not a fact about the change**: prikk's
    /// history carries no time, so this is the only time the pages can show (ledger: the log has no
    /// timestamps), and they say so. It is kept when the cache is cleared and dropped with nothing else.
    fn first_seen(&self, wiki: &WikiId, blocks: &[String], now: i64) -> Result<Vec<i64>>;
}
