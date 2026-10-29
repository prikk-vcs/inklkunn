//! The read service (RFC 003 D-2; internal design RD-1, DM-6, DM-7, CON-5; handoff 003 T3): a page is
//! **re-derived from prikk and cached by prikk's ids**; nothing in the service store is authoritative for
//! content (INV-5), and the cache can be cleared and rebuilt to the same pages (ENF-5).
//!
//! ```text
//! wiki record ─▶ the view's tip (branch list, held a few seconds)
//!             ─▶ the title index at that tip (memory ▶ derived store ▶ one extract of the tip, once)
//!             ─▶ the page the title leads to (alias, redirect, the MKUP-5 rules)
//!             ─▶ the cache by (tip, path, language, presentation) ▶ HIT: sanitized again, returned
//!             ─▶ MISS: cat the page at the tip ▶ parse ▶ resolve ▶ render ▶ sanitize ▶ store
//! ```
//!
//! **Calls to prikk** (the counting harness, [`CountingStorage`], holds these to numbers): a hit makes
//! **none** while the tip is held. A miss makes **one** (`cat`), plus one `branch list` when the tip is not
//! held, so at most two; an included page (transclusion) costs one more `cat` each. The index is built from
//! **one `extract` of the tip per tip** and shared by every page and reader at it (single flight), because
//! each `cat` replays the whole tree and an index built page by page would cost one replay per page (hold
//! note IS-10). The first view at a new tip therefore also pays that one `extract`, and the one `log` that
//! names the tip's block: **four on the first miss at a new tip**, one on a later miss.
//!
//! **A page's past** (handoff 003 T5; the `history` module): the history of a page, one change, a page's diff and
//! a permalink are read through the same service, with their own bounds (a window of the view's log held per
//! tip, a block's operations kept by id, at most [`MAX_SHOWS_PER_REQUEST`] `show`s per request, exactly one `diff`
//! and always with the page's path), and the word-level refinement of a diff in `linediff` is inklkunn's own.
//!
//! **What this module never does**: it never passes `--allow-full-replay` (the driver has no way to), it
//! never reads a verification result from the cache (there is none in it: [`inklkunn_store::DerivedPage`]
//! has no field for one; the badge is read from the wiki record, RD-3), and it never hands a template HTML
//! that did not pass the sanitizer ([`SafeHtml`] cannot be made any other way).
//!
//! **A refusal for want of prikk's checkpoint record** (`extract` after a prikk upgrade, a restore or a
//! cleared cache; ledger PK-24) is the ordinary *could not be read now*, with the reason kept for the
//! operator ([`Cause::CheckpointNotRecorded`]), and the wiki is flagged for the checkpoint refresh
//! ([`inklkunn_store::Checkpoint`]) that the binary runs (handoff 003 T7).
//!
//! **Limits worth knowing.**
//!
//! - *The tip is pinned.* `branch list` gives a view's RefState id and not the block it points at, so the
//!   first time a state id is met one `log --limit 1` asks for the pair, and every read of that tip (`cat`,
//!   `extract`) is made *at the block*: a write between two calls of a request cannot cache newer content
//!   under an older key. The price is one more call on the first miss at a new tip (four: the list, the
//!   log, the extract, the `cat`), none on a hit; a tip block id in `branch-list-v1` would remove it (a
//!   candidate ask for prikk).
//! - *The index costs the whole tip.* `extract` writes every file the tip has, attachments included, so the
//!   bounds ([`ReadConfig::index_max_bytes`], [`ReadConfig::index_max_files`]) count them too, and a wiki
//!   with a great many attachments needs them raised. `extract` has no path filter (ledger PK-25).
//! - *Reads are not served stale.* When prikk cannot be asked for the tip, the read fails as *could not be
//!   read now*; serving the cache labelled stale (external design OP-03, internal design CON-5) is not part
//!   of this service yet.
//! - *Included pages cost a `cat` each* on a miss (each a replay of the whole tree), so a page may include at
//!   most [`inklkunn_markup::MAX_TRANSCLUSIONS`] pages and
//!   [`inklkunn_markup::MAX_TRANSCLUDED_BYTES`] of their source, however it nests them, and an included page's
//!   footnotes keep the ids the Markdown writer gives them.

mod counting;
mod history;
mod index;
mod linediff;
mod links;
mod sanitize;
mod tip;

// The read API's reads (handoff 003 T8).
mod api;

#[cfg(test)]
mod testing;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use inklkunn_markup::path::{PathError, title_to_path};
use inklkunn_markup::scan::ScanOptions;
use inklkunn_markup::{AuthorHtml, Page, ParseError, RenderOptions, render_page_with};
use inklkunn_prikk::{ExtractBounds, Point, SizeHints, Storage, StorageError};
use inklkunn_store::{
    DerivedHeading, DerivedMeta, DerivedPage, DerivedStore, IndexEntry, PageKey, RootLayout,
    StoreError, WikiId, WikiRecord, WikiStore,
};

use links::PageLinks;

pub use counting::{Calls, CountingStorage};
pub use history::{
    AtOutcome, ChangeDetail, ChangeInfo, ChangeKind, ChangeOutcome, Did, DiffOutcome, DiffStatus,
    HISTORY_PAGE_ROWS, HistoryOutcome, HistoryRow, MAX_DID, MAX_DIFF_LINES,
    MAX_INSPECTED_PER_REQUEST, MAX_SHOWS_PER_REQUEST, MAX_WINDOW, PageDiff, PageEvent, PageHistory,
    Provenance, Subject, Who,
};
pub use index::{
    FRONTMATTER_READ_BYTES, Found, Lookup, MAX_REDIRECT_HOPS, Problem, RedirectProblem,
    RedirectedBy, RedirectedFrom, TitleIndex, TreeError, entries_from_tree,
};
pub use inklkunn_markup::{TransclusionRefusal, Via};
pub use linediff::{DiffLine, Hunk, HunkView, LineKind, MAX_WORDS, Segment};
pub use links::Presentation;
pub use sanitize::{HTML_SUBSET, SafeHtml, Sanitizer};
pub use tip::Tip;

// The read API's reads (handoff 003 T8).
pub use api::{
    ApiError, Cursor, DEFAULT_PAGE_SIZE, Feature, ListedPage, MAX_PAGE_SIZE, PageFilter, PageList,
    Pending, RawInfo,
};

/// Plain-data views of what a page carries, as the derived cache keeps them.
pub use inklkunn_store::{
    DerivedHeading as Heading, DerivedLinks as LinkSets, DerivedMeta as PageMeta,
};

/// What the read service is allowed and how long it holds things.
#[derive(Debug, Clone)]
pub struct ReadConfig {
    /// How long a view's tip is held before prikk is asked again (D-2: "a few seconds").
    pub tip_ttl: Duration,
    /// The largest page source read, in bytes (PERF-4: 1 MiB).
    pub page_max_bytes: u64,
    /// The largest tip the index is built from, in bytes: `extract` refuses the whole point before it writes
    /// a byte when the tip is larger. It counts **everything** the tip holds, attachments included.
    pub index_max_bytes: u64,
    /// The most files the index is built from (and the most the tree walk visits).
    pub index_max_files: u64,
}

impl Default for ReadConfig {
    fn default() -> Self {
        ReadConfig {
            tip_ttl: Duration::from_secs(5),
            page_max_bytes: 1 << 20,
            index_max_bytes: 4 << 30,
            index_max_files: 200_000,
        }
    }
}

/// Why a page was not read, in the terms a surface chooses its words by.
#[derive(Debug, Clone)]
pub enum ReadError {
    /// The title cannot name a page: it is empty, has an empty or reserved component, or its path would
    /// exceed one of prikk's caps (255 bytes a component, 1024 the path). Refused before anything else saw
    /// it, so no prikk call was made.
    InvalidTitle(PathError),
    /// The wiki has no such open view.
    NoSuchView(String),
    /// The page is there and cannot be shown: its source is not what a page is.
    PageUnreadable(PageProblem),
    /// It could not be read now: show *could not be read now* and nothing of the cause; the operator gets
    /// the cause.
    CouldNotBeReadNow(Cause),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::InvalidTitle(e) => write!(f, "the title cannot name a page: {e}"),
            ReadError::NoSuchView(v) => write!(f, "the wiki has no open view `{v}`"),
            ReadError::PageUnreadable(p) => write!(f, "the page cannot be shown: {p}"),
            ReadError::CouldNotBeReadNow(c) => write!(f, "could not be read now: {c}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Why a page that exists cannot be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageProblem {
    /// The file is not UTF-8.
    NotUtf8,
    /// The frontmatter is outside the plain YAML subset or malformed (the reason is prikk-independent text
    /// of the parser's, for the page's authors).
    Frontmatter(String),
    /// The frontmatter is longer than [`FRONTMATTER_READ_BYTES`].
    FrontmatterTooLong,
}

impl std::fmt::Display for PageProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageProblem::NotUtf8 => f.write_str("the file is not UTF-8"),
            PageProblem::Frontmatter(e) => write!(f, "the frontmatter is not readable: {e}"),
            PageProblem::FrontmatterTooLong => write!(
                f,
                "the frontmatter is longer than {FRONTMATTER_READ_BYTES} bytes"
            ),
        }
    }
}

/// What stood in the way of a read, for the operator. Never shown to a reader.
#[derive(Debug, Clone)]
pub enum Cause {
    /// A prikk call did not complete or was refused; the typed outcome.
    Storage(StorageError),
    /// `extract` refused for want of prikk's checkpoint record (`no-checkpoint-in-reach`, reason
    /// `not-recorded`): after a prikk upgrade, a restore or a cleared cache. The wiki is flagged for the
    /// checkpoint refresh unless the flag could not be stored (`refresh_requested: false`).
    CheckpointNotRecorded {
        /// The wiki's record now asks for a refresh.
        refresh_requested: bool,
    },
    /// `extract` refused for want of a checkpoint for another reason (`signer-not-adopted`: adopt the
    /// signing key; `no-checkpoint`: only a seal or a full replay helps, which a request never starts).
    CheckpointUnavailable {
        /// prikk's `reason`.
        reason: String,
    },
    /// The tip is larger than the bounds the index is built under ([`ReadConfig::index_max_bytes`],
    /// [`ReadConfig::index_max_files`]).
    IndexBounds(String),
    /// The scratch directory or the extracted tree could not be used.
    Scratch(String),
    /// The service store failed where the read could not go on without it.
    Store(StoreError),
}

impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cause::Storage(e) => write!(f, "{e}"),
            Cause::CheckpointNotRecorded { refresh_requested } => write!(
                f,
                "prikk's checkpoint record is not recorded for this repository (an upgrade, a restore or a \
                 cleared cache); the title index cannot be built until a checkpoint refresh runs, and one \
                 {}",
                if *refresh_requested {
                    "has been requested"
                } else {
                    "could NOT be requested (the store failed)"
                }
            ),
            Cause::CheckpointUnavailable { reason } => write!(
                f,
                "prikk has no usable checkpoint for this repository (`{reason}`); a refresh will not help"
            ),
            Cause::IndexBounds(what) => {
                write!(f, "the tip is larger than the index is built from: {what}")
            }
            Cause::Scratch(e) => write!(f, "the scratch directory could not be used: {e}"),
            Cause::Store(e) => write!(f, "{e}"),
        }
    }
}

/// A page, ready to be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageView {
    /// The tip the page was read at.
    pub tip: Tip,
    /// The page's path in the repository. A storage path: shown nowhere but labelled as one.
    pub path: String,
    /// The page's metadata. Its text fields are plain text: a surface escapes them.
    pub meta: PageMeta,
    /// The page's body, sanitized.
    pub html: SafeHtml,
    /// The page's headings.
    pub toc: Vec<Heading>,
    /// The links the page makes.
    pub links: LinkSets,
    /// Set when the reader did not ask for this page by its own title (D-1: *redirected from*).
    pub redirected_from: Option<RedirectedFrom>,
    /// The rule of MKUP-5 that matched the title asked for; anything but `Exact` is a tolerant match.
    pub via: Via,
    /// The page came from the derived cache and was not rendered for this request.
    pub cached: bool,
    /// The page is the last one this server has, served because prikk could not be asked for the newest
    /// ([`ReadService::stale_page`]): the surface says so, and the verification badge reads *Could not be
    /// checked now* (INV-5), whatever the record says.
    pub stale: bool,
}

/// What a title leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageOutcome {
    /// A page.
    Found(Box<PageView>),
    /// No page: the 404 that invites it (NAV-3).
    Missing {
        /// The title asked for.
        title: String,
    },
    /// Several pages match and none is nearer (NAV-15).
    Ambiguous {
        /// Every candidate, in index order.
        candidates: Vec<IndexEntry>,
    },
    /// A redirect page whose chain cannot be followed to a page.
    BrokenRedirect {
        /// The redirect page the chain began at.
        from: IndexEntry,
        /// Why it ends.
        problem: RedirectProblem,
    },
}

/// What becomes of an author's raw HTML under a profile whose raw-HTML subset is `subset`: escaped to text while
/// it is empty, handed to the sanitizer (whose allowlist is then that subset) when it is not.
fn author_html_for(subset: &[&str]) -> AuthorHtml {
    if subset.is_empty() {
        AuthorHtml::Escape
    } else {
        AuthorHtml::PassToSanitizer
    }
}

/// How far every heading of a page body is moved down: the page's title is the page's one `h1`, so a body's
/// `#` is an `h2` (accessibility: one `h1`, headings in order).
pub const HEADING_DEMOTION: u8 = 1;

/// What the derived cache keys a page by besides its tip, path and language: the surface's own presentation
/// key and the render options this service draws with, so a row drawn another way is never read back.
fn cache_presentation(presentation: &dyn Presentation) -> String {
    format!("{}|d{HEADING_DEMOTION}", presentation.key())
}

/// The most title indexes kept in memory at once, over all wikis: a bound, since a title index is a few
/// megabytes for a large wiki. The derived store holds the rest.
const MAX_MEMORY_INDEXES: usize = 64;

/// The read service. Cheap to share: every method takes `&self`.
///
/// **Every method blocks the calling thread** (the storage does; see [`Storage`]); an async caller runs it
/// on a blocking thread.
pub struct ReadService {
    storage: Arc<dyn Storage>,
    derived: Arc<dyn DerivedStore>,
    wikis: Arc<dyn WikiStore>,
    layout: RootLayout,
    config: ReadConfig,
    sanitizer: Sanitizer,
    tips: tip::Tips,
    /// `(wiki, tip)` → the index, newest last.
    indexes: Mutex<Vec<(WikiId, Arc<TitleIndex>)>>,
    /// One lock per wiki, so that an index is built once however many readers meet the new tip together.
    builds: Mutex<HashMap<WikiId, Arc<Mutex<()>>>>,
    /// An index build that was refused for a reason that will not change by asking again, with the tip it
    /// was refused at and when: asked again within the hold of a tip, the answer is the same one, and prikk
    /// is not spawned for every page request while the operator fixes it.
    refused_builds: Mutex<HashMap<WikiId, (Instant, String, ReadError)>>,
    scratch_names: AtomicU64,
    store_failures: AtomicU64,
    /// What history, change, diff and permalink pages hold (handoff 003 T5): log windows, the operations of
    /// blocks, a few permalink renders.
    history: history::State,
}

impl ReadService {
    /// A read service over `storage` for the wikis under `layout`. `hints` is the handle the driver was
    /// given (`PrikkCli::with_size_hints`): the service keeps it true, walking the store once per change of
    /// a wiki's views; `None` leaves the driver to walk on every call that follows the store's size.
    pub fn new(
        storage: Arc<dyn Storage>,
        derived: Arc<dyn DerivedStore>,
        wikis: Arc<dyn WikiStore>,
        layout: RootLayout,
        config: ReadConfig,
        hints: Option<SizeHints>,
    ) -> Self {
        ReadService {
            storage,
            derived,
            wikis,
            layout,
            tips: tip::Tips::new(config.tip_ttl, hints),
            config,
            sanitizer: Sanitizer::profile(),
            indexes: Mutex::new(Vec::new()),
            builds: Mutex::new(HashMap::new()),
            refused_builds: Mutex::new(HashMap::new()),
            scratch_names: AtomicU64::new(0),
            store_failures: AtomicU64::new(0),
            history: history::State::default(),
        }
    }

    /// The sanitizer every page passes through, for a surface that has other HTML of its own to clean (a diff,
    /// a search snippet) under the same policy.
    pub fn sanitizer(&self) -> &Sanitizer {
        &self.sanitizer
    }

    /// How many times the derived store failed in a place where the read went on without it (a cache that
    /// could not be read or written is a miss, never a failed page). For metrics.
    pub fn store_failures(&self) -> u64 {
        self.store_failures.load(Ordering::Relaxed)
    }

    /// The store size last measured for the wiki (the figure the driver was hinted).
    pub fn measured_store_bytes(&self, wiki: &WikiRecord) -> Option<u64> {
        self.tips.store_bytes(&wiki.id)
    }

    /// Forget what is held about a wiki: its tips, its store figure and the indexes in memory. A write path
    /// calls it after it seals, so that the next read asks prikk and walks (the truth after a write); the
    /// derived store needs nothing, its rows are keyed by tips the wiki has left.
    pub fn forget_wiki(&self, wiki: &WikiRecord) {
        self.tips.forget(&wiki.id, &self.layout.repo_dir(&wiki.id));
        self.history.forget(&wiki.id);
        self.refused_builds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&wiki.id);
        self.indexes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(w, _)| *w != wiki.id);
    }

    /// Clear everything derived for a wiki, in the store and in memory (ENF-5: the next reads rebuild it).
    pub fn clear_derived(&self, wiki: &WikiRecord) -> Result<u64, ReadError> {
        self.forget_wiki(wiki);
        self.derived
            .clear(&wiki.id)
            .map_err(|e| ReadError::CouldNotBeReadNow(Cause::Store(e)))
    }

    /// The tip of `view` (the wiki's default view when `None`). Held for [`ReadConfig::tip_ttl`]: a held
    /// answer makes no prikk call.
    pub fn tip(&self, wiki: &WikiRecord, view: Option<&str>) -> Result<Tip, ReadError> {
        let view = view.unwrap_or(&wiki.default_view);
        let repo = self.layout.repo_dir(&wiki.id);
        let (name, state) = match self.tips.fresh(&wiki.id, view) {
            Some(held) => held.ok_or_else(|| ReadError::NoSuchView(view.to_owned()))?,
            None => {
                let views = self
                    .storage
                    .views(&repo)
                    .map_err(|e| self.storage_error(wiki, e))?;
                self.tips.hold(&wiki.id, &repo, &views);
                views
                    .views
                    .iter()
                    .find(|v| v.name == view && !v.closed)
                    .map(|v| (v.name.clone(), v.state_id.clone()))
                    .ok_or_else(|| ReadError::NoSuchView(view.to_owned()))?
            }
        };
        if let Some(block_id) = self.tips.block(&wiki.id, &name, &state) {
            return Ok(Tip {
                view: name,
                state_id: state,
                block_id,
            });
        }
        // `branch list` names the state, not the block: ask once, here, for the pair. If the view moved
        // since the list, the pair the log gives is the newer tip, whole.
        let newest = self
            .storage
            .history(&repo, &Point::View(name.clone()), Some(1))
            .map_err(|e| self.storage_error(wiki, e))?;
        let block = newest.blocks.into_iter().next().ok_or_else(|| {
            ReadError::CouldNotBeReadNow(Cause::Scratch(format!(
                "the log of `{name}` lists no block"
            )))
        })?;
        if block.ref_state_id != state {
            self.tips.moved(&wiki.id, &name, &block.ref_state_id);
        }
        self.tips
            .remember_block(&wiki.id, &name, &block.ref_state_id, &block.block_id);
        Ok(Tip {
            view: name,
            state_id: block.ref_state_id,
            block_id: block.block_id,
        })
    }

    /// The title index of the view's tip: from memory, else from the derived store, else built from **one
    /// extract of the tip**, once, however many callers ask at the same time.
    pub fn title_index(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
    ) -> Result<Arc<TitleIndex>, ReadError> {
        let tip = self.tip(wiki, view)?;
        self.index_at(wiki, &tip)
    }

    /// Where `title` leads at the view's tip, without reading the page.
    pub fn lookup(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
    ) -> Result<Lookup, ReadError> {
        title_to_path(title).map_err(ReadError::InvalidTitle)?;
        Ok(self.title_index(wiki, view)?.lookup(title))
    }

    /// The page titled `title` at the view's tip, rendered for `presentation`: from the cache when it is
    /// there (no prikk call while the tip is held), else read, parsed, resolved, rendered and sanitized.
    pub fn page(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
        presentation: &dyn Presentation,
    ) -> Result<PageOutcome, ReadError> {
        // A title that cannot name a page is refused here, before anything else sees it.
        title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        let found = match index.lookup(title) {
            Lookup::Found(found) => found,
            Lookup::Missing => {
                return Ok(PageOutcome::Missing {
                    title: title.trim().to_owned(),
                });
            }
            Lookup::Ambiguous { candidates } => return Ok(PageOutcome::Ambiguous { candidates }),
            Lookup::BrokenRedirect { from, problem } => {
                return Ok(PageOutcome::BrokenRedirect { from, problem });
            }
        };
        if let Some(problem) = found.entry.problem.as_deref() {
            return Err(ReadError::PageUnreadable(match problem {
                "not-utf8" => PageProblem::NotUtf8,
                "frontmatter-too-long" => PageProblem::FrontmatterTooLong,
                _ => PageProblem::Frontmatter("the frontmatter is not readable".to_owned()),
            }));
        }
        let presentation_key = cache_presentation(presentation);
        let key = PageKey {
            wiki: &wiki.id,
            tip: &tip.state_id,
            path: &found.entry.path,
            lang: presentation.lang(),
            presentation: &presentation_key,
        };
        // A cache that cannot be read is a miss.
        let cached = self.derived.page(&key).unwrap_or_else(|_| {
            self.store_failures.fetch_add(1, Ordering::Relaxed);
            None
        });
        let (row, from_cache) = match cached {
            Some(row) => (row, true),
            None => {
                let row = self.render(wiki, &tip, &index, &found.entry, presentation)?;
                if self.derived.put_page(&key, &row).is_err() {
                    self.store_failures.fetch_add(1, Ordering::Relaxed);
                }
                (row, false)
            }
        };
        Ok(PageOutcome::Found(Box::new(PageView {
            tip,
            path: found.entry.path,
            // The text came out of a database or a renderer; it goes out through the sanitizer either way.
            html: self.sanitizer.clean(&row.html),
            meta: row.meta,
            toc: row.toc,
            links: row.links,
            redirected_from: found.redirected_from,
            via: found.via,
            cached: from_cache,
            stale: false,
        })))
    }

    /// The page titled `title` as this server last had it, **without asking prikk anything**, for a surface that
    /// has just been told prikk is busy or unavailable (external design OP-03, internal design CON-5). It is the
    /// page of the last tip this process saw, from the title index and the page cache, and only if both are
    /// there: a miss is never rendered from nothing. The view carries [`PageView::stale`].
    pub fn stale_page(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
        presentation: &dyn Presentation,
    ) -> Option<PageView> {
        title_to_path(title).ok()?;
        let view = view.unwrap_or(&wiki.default_view);
        let state_id = self.tips.last_known(&wiki.id, view)?;
        let index = self
            .remembered(&wiki.id, &state_id)
            .or_else(|| self.stored_index(wiki, &Tip::unpinned(view, &state_id)))?;
        let Lookup::Found(found) = index.lookup(title) else {
            return None;
        };
        if found.entry.problem.is_some() {
            return None;
        }
        let presentation_key = cache_presentation(presentation);
        let key = PageKey {
            wiki: &wiki.id,
            tip: &state_id,
            path: &found.entry.path,
            lang: presentation.lang(),
            presentation: &presentation_key,
        };
        let row = self.derived.page(&key).ok().flatten()?;
        Some(PageView {
            tip: Tip::unpinned(view, &state_id),
            path: found.entry.path,
            html: self.sanitizer.clean(&row.html),
            meta: row.meta,
            toc: row.toc,
            links: row.links,
            redirected_from: found.redirected_from,
            via: found.via,
            cached: true,
            stale: true,
        })
    }

    // ------------------------------------------------------------------------------------ rendering

    /// Read, parse, resolve, render and sanitize one page.
    fn render(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        index: &TitleIndex,
        entry: &IndexEntry,
        presentation: &dyn Presentation,
    ) -> Result<DerivedPage, ReadError> {
        let text = self
            .read_text(wiki, tip, &entry.path)?
            .ok_or(ReadError::PageUnreadable(PageProblem::NotUtf8))?;
        self.render_text(wiki, tip, index, entry, presentation, &text)
    }

    /// Parse, resolve, render and sanitize the source `text` of the page `entry` (read at `tip`, which is also
    /// where an included page is read).
    fn render_text(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        index: &TitleIndex,
        entry: &IndexEntry,
        presentation: &dyn Presentation,
        text: &str,
    ) -> Result<DerivedPage, ReadError> {
        let page = Page::parse(text).map_err(|ParseError::Frontmatter(e)| {
            ReadError::PageUnreadable(PageProblem::Frontmatter(e.to_string()))
        })?;
        let included = |e: &IndexEntry| self.read_text(wiki, tip, &e.path);
        let links = PageLinks::new(index, presentation, &entry.path, &included);
        // The author's raw HTML is escaped to text while the profile declares no subset; with one, it goes on
        // to the sanitizer, whose allowlist is exactly that subset (RFC 003 D-3).
        let author_html = author_html_for(HTML_SUBSET);
        let rendered = render_page_with(
            Some((&entry.path, &entry.title)),
            &page.body,
            &links,
            ScanOptions::default(),
            RenderOptions {
                author_html,
                demote_headings: HEADING_DEMOTION,
            },
        );
        // A page rendered around an included page that could not be read is not a page to keep.
        if let Some(failure) = links.take_failure() {
            return Err(failure);
        }
        let html = self.sanitizer.clean(&rendered.unsanitized_html);
        let md = &page.metadata;
        Ok(DerivedPage {
            html: html.as_str().to_owned(),
            meta: DerivedMeta {
                title: entry.title.clone(),
                id: md.id.clone(),
                kind: md.kind.clone(),
                status: md.status.clone(),
                lang: md.lang.clone(),
                license: md.license.clone(),
                description: md.description.clone(),
                tags: md.tags.clone(),
                aliases: md.aliases.clone(),
            },
            links: links.into_links(rendered.transcluded),
            toc: rendered
                .toc
                .into_iter()
                .map(|t| DerivedHeading {
                    level: t.level,
                    id: t.id,
                    text: t.text,
                })
                .collect(),
            computed_at: inklkunn_store::record::now_secs(),
        })
    }

    /// A page's source at the tip as text: `Ok(None)` when it is not UTF-8 (a content problem), an error when
    /// prikk could not be asked.
    fn read_text(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        path: &str,
    ) -> Result<Option<String>, ReadError> {
        let bytes = self
            .storage
            .read(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(tip.block_id.clone()),
                path,
                self.config.page_max_bytes,
            )
            .map_err(|e| self.storage_error(wiki, e))?;
        Ok(String::from_utf8(bytes).ok())
    }

    // ------------------------------------------------------------------------------------ the index

    fn index_at(&self, wiki: &WikiRecord, tip: &Tip) -> Result<Arc<TitleIndex>, ReadError> {
        if let Some(index) = self.remembered(&wiki.id, &tip.state_id) {
            return Ok(index);
        }
        if let Some(index) = self.stored_index(wiki, tip) {
            return Ok(index);
        }
        // One build per wiki at a time. Whoever waited for the lock finds the index already there.
        let lock = self.build_lock(&wiki.id);
        let _building = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = self.remembered(&wiki.id, &tip.state_id) {
            return Ok(index);
        }
        if let Some(index) = self.stored_index(wiki, tip) {
            return Ok(index);
        }
        if let Some(refusal) = self.recently_refused(&wiki.id, &tip.state_id) {
            return Err(refusal);
        }
        let entries = match self.build_entries(wiki, tip) {
            Ok(entries) => entries,
            Err(e) => {
                self.note_refusal(&wiki.id, &tip.state_id, &e);
                return Err(e);
            }
        };
        if self
            .derived
            .put_title_index(
                &wiki.id,
                &tip.state_id,
                &entries,
                inklkunn_store::record::now_secs(),
            )
            .is_err()
        {
            self.store_failures.fetch_add(1, Ordering::Relaxed);
        }
        let index = Arc::new(TitleIndex::new(tip.state_id.clone(), entries));
        self.remember(&wiki.id, &index);
        self.forget_the_past(wiki);
        Ok(index)
    }

    /// The refusal an index build met at this tip within the hold of a tip, if it did.
    fn recently_refused(&self, wiki: &WikiId, tip: &str) -> Option<ReadError> {
        let refused = self
            .refused_builds
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        refused
            .get(wiki)
            .filter(|(at, seen_tip, _)| seen_tip == tip && at.elapsed() < self.config.tip_ttl)
            .map(|(_, _, e)| e.clone())
    }

    /// Remember a refusal that asking again will not change: the checkpoint record's, and the bounds'. A
    /// timeout, a busy server or a failed spawn is tried again by the next request.
    fn note_refusal(&self, wiki: &WikiId, tip: &str, e: &ReadError) {
        let lasting = matches!(
            e,
            ReadError::CouldNotBeReadNow(
                Cause::CheckpointNotRecorded { .. }
                    | Cause::CheckpointUnavailable { .. }
                    | Cause::IndexBounds(_)
            )
        );
        if lasting {
            self.refused_builds
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(wiki.clone(), (Instant::now(), tip.to_owned(), e.clone()));
        }
    }

    fn remembered(&self, wiki: &WikiId, tip: &str) -> Option<Arc<TitleIndex>> {
        let held = self.indexes.lock().unwrap_or_else(PoisonError::into_inner);
        held.iter()
            .find(|(w, ix)| w == wiki && ix.tip() == tip)
            .map(|(_, ix)| Arc::clone(ix))
    }

    fn remember(&self, wiki: &WikiId, index: &Arc<TitleIndex>) {
        let mut held = self.indexes.lock().unwrap_or_else(PoisonError::into_inner);
        held.retain(|(w, ix)| !(w == wiki && ix.tip() == index.tip()));
        held.push((wiki.clone(), Arc::clone(index)));
        let excess = held.len().saturating_sub(MAX_MEMORY_INDEXES);
        held.drain(..excess);
    }

    /// The index the derived store has for the tip, put in memory. A store that cannot answer is a miss.
    fn stored_index(&self, wiki: &WikiRecord, tip: &Tip) -> Option<Arc<TitleIndex>> {
        match self.derived.title_index(&wiki.id, &tip.state_id) {
            Ok(Some(entries)) => {
                let index = Arc::new(TitleIndex::new(tip.state_id.clone(), entries));
                self.remember(&wiki.id, &index);
                Some(index)
            }
            Ok(None) => None,
            Err(_) => {
                self.store_failures.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    fn build_lock(&self, wiki: &WikiId) -> Arc<Mutex<()>> {
        let mut builds = self.builds.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(builds.entry(wiki.clone()).or_default())
    }

    /// After a tip moved: keep the cache of the tips the wiki has now and drop the rest.
    fn forget_the_past(&self, wiki: &WikiRecord) {
        let keep = self.tips.current_tips(&wiki.id);
        if keep.is_empty() {
            return;
        }
        if self.derived.retain_tips(&wiki.id, &keep).is_err() {
            self.store_failures.fetch_add(1, Ordering::Relaxed);
        }
        self.indexes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(w, ix)| *w != wiki.id || keep.iter().any(|t| t == ix.tip()));
    }

    /// One `extract` of the tip into the wiki's scratch directory, read, and the directory removed.
    fn build_entries(&self, wiki: &WikiRecord, tip: &Tip) -> Result<Vec<IndexEntry>, ReadError> {
        let scratch_error = |what: &str, e: std::io::Error| {
            ReadError::CouldNotBeReadNow(Cause::Scratch(format!("{what}: {e}")))
        };
        let repo = self.layout.repo_dir(&wiki.id);
        let scratch = self.layout.scratch_dir(&wiki.id);
        std::fs::create_dir_all(&scratch)
            .map_err(|e| scratch_error("creating the scratch directory", e))?;
        // Only one build runs per wiki (the caller holds its lock), so anything of an earlier build still
        // here was left by a process that died.
        sweep_leftovers(&scratch);
        let name = format!(
            "index-{}-{}",
            std::process::id(),
            self.scratch_names.fetch_add(1, Ordering::Relaxed)
        );
        let bounds = ExtractBounds {
            max_bytes: Some(self.config.index_max_bytes),
            max_files: Some(self.config.index_max_files),
        };
        let extracted = self
            .storage
            .materialize(
                &repo,
                &Point::Block(tip.block_id.clone()),
                &scratch,
                &name,
                bounds,
            )
            .map_err(|e| self.storage_error(wiki, e));
        let entries =
            match extracted {
                Ok(_) => entries_from_tree(&scratch.join(&name), self.config.index_max_files)
                    .map_err(|e| match e {
                        TreeError::TooManyEntries(n) => ReadError::CouldNotBeReadNow(
                            Cause::IndexBounds(format!("more than {n} entries under `pages/`")),
                        ),
                        TreeError::Io(e) => scratch_error("reading the extracted tree", e),
                    }),
                Err(e) => Err(e),
            };
        // The directory goes whatever came of the build. A directory that will not go is swept by the next
        // build; the index itself is good.
        remove_tree(&scratch.join(&name));
        entries
    }

    // ------------------------------------------------------------------------------------ errors

    /// A prikk outcome as the read path's: a refusal for want of the checkpoint record is named and flags
    /// the wiki for the refresh; everything else is *could not be read now* with the outcome kept.
    fn storage_error(&self, wiki: &WikiRecord, e: StorageError) -> ReadError {
        if let StorageError::Refused {
            kind: Some(kind),
            reason,
            ..
        } = &e
            && kind.as_str() == "no-checkpoint-in-reach"
        {
            return ReadError::CouldNotBeReadNow(match reason.as_deref() {
                Some("not-recorded") => {
                    let asked = self
                        .wikis
                        .request_checkpoint_refresh(&wiki.id, inklkunn_store::record::now_secs())
                        .is_ok();
                    if !asked {
                        self.store_failures.fetch_add(1, Ordering::Relaxed);
                    }
                    Cause::CheckpointNotRecorded {
                        refresh_requested: asked,
                    }
                }
                other => Cause::CheckpointUnavailable {
                    reason: other.unwrap_or("none given").to_owned(),
                },
            });
        }
        if let StorageError::Refused {
            kind: Some(kind), ..
        } = &e
            && matches!(kind.as_str(), "over-max-bytes" | "over-max-files")
        {
            return ReadError::CouldNotBeReadNow(Cause::IndexBounds(format!(
                "prikk refused the whole tip (`{kind}`)"
            )));
        }
        ReadError::CouldNotBeReadNow(Cause::Storage(e))
    }
}

/// Remove directories an earlier, dead build left in the scratch directory.
fn sweep_leftovers(scratch: &Path) {
    let Ok(listing) = std::fs::read_dir(scratch) else {
        return;
    };
    for item in listing.flatten() {
        if item.file_name().to_string_lossy().starts_with("index-") {
            remove_tree(&item.path());
        }
    }
}

fn remove_tree(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
mod tests;
