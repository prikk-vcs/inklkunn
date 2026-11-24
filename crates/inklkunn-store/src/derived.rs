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

/// A claim in the derived knowledge index of a tip (RFC 005 part 2 D-6). Words are the knowledge format's own
/// (`kind`, `lifecycle`, `standing`); the store keeps them as text and judges none of them. **Nothing numeric and
/// nothing that ranks**: the columns are the claim's own words and where its judgements stand.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeClaimRow {
    /// The claim's id (the file's name).
    pub id: String,
    /// What kind of statement.
    pub kind: String,
    /// Where it stands in its life.
    pub lifecycle: String,
    /// Why it was retired, as the file says (RFC 006 D-10), cut at the bound the index is built with; only a retired claim has one.
    pub retired_reason: Option<String>,
    /// What the community has decided.
    pub standing: String,
    /// The proposal that decided it, when one did.
    pub standing_proposal: Option<String>,
    /// Whether the standing was judged on the wording the claim has now: `current`, `earlier` or `none`.
    pub standing_judgement: String,
}

/// A source in the derived knowledge index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeSourceRow {
    /// The source's id.
    pub id: String,
    /// What kind of source.
    pub kind: String,
    /// Whether it can be had, as a person who checked says.
    pub availability: String,
    /// `active` or `retired` (RFC 006 D-10): a version 1 file that does not say is `active`.
    pub lifecycle: String,
    /// Why it was retired, as the file says, cut at the bound the index is built with; only a retired source has one.
    pub retired_reason: Option<String>,
    /// The day a person last checked it, as written, when one did.
    pub last_reviewed: Option<String>,
    /// The source that replaces it.
    pub superseded_by: Option<String>,
    /// Its title, as written, cut at the bound the index is built with (K9).
    pub title: Option<String>,
    /// Its author or publisher, as written, cut likewise.
    pub author: Option<String>,
    /// Its date, as written (a year, a season, a day), cut likewise.
    pub date: Option<String>,
    /// Where it is found (its reference's value), as written, cut likewise.
    pub reference: Option<String>,
}

/// A claim's relation to another claim.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeRelationRow {
    /// The claim.
    pub claim: String,
    /// `refines`, `supersedes`, …
    pub kind: String,
    /// The other claim.
    pub other: String,
}

/// A claim's relation to a source, and whether it was judged on the claim's wording as it is now.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeLinkRow {
    /// The claim.
    pub claim: String,
    /// The source.
    pub source: String,
    /// `supports` or `challenges`.
    pub relation: String,
    /// `direct`, `partial` or `indirect`: how directly, never how true.
    pub bearing: String,
    /// `current` when the digest it was judged on is the digest of an expression's text now, else `earlier`.
    pub judgement: String,
}

/// Where a claim is said, and what the block is now.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeAnchorRow {
    /// The claim.
    pub claim: String,
    /// The page's id as the expression names it.
    pub page_id: String,
    /// The block's `^id`.
    pub block: String,
    /// The page's path at the tip, when exactly one page has the id.
    pub page_path: Option<String>,
    /// `present`, `page-missing`, `page-repeated`, `block-missing` or `block-repeated`.
    pub state: String,
    /// The digest of the block's text now, when `present`.
    pub digest: Option<String>,
}

/// A file under `knowledge/` that cannot be read as a claim or a source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeInvalidRow {
    /// The file's repository path.
    pub path: String,
    /// The stable code of the rule it broke.
    pub rule: String,
}

/// The derived knowledge index of one tip, as the store keeps it (all of it or none).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KnowledgeRows {
    /// The readable claims, by id.
    pub claims: Vec<KnowledgeClaimRow>,
    /// The readable sources, by id.
    pub sources: Vec<KnowledgeSourceRow>,
    /// Claim to claim.
    pub relations: Vec<KnowledgeRelationRow>,
    /// Claim to source.
    pub links: Vec<KnowledgeLinkRow>,
    /// Where each claim is said.
    pub anchors: Vec<KnowledgeAnchorRow>,
    /// The files that cannot be read, by path.
    pub invalid: Vec<KnowledgeInvalidRow>,
}

impl KnowledgeRows {
    /// Every list in its one order (the order the stores read back in).
    pub fn sorted(mut self) -> Self {
        self.claims.sort();
        self.sources.sort();
        self.relations.sort();
        self.links.sort();
        self.anchors.sort();
        self.invalid.sort();
        self
    }

    /// How many rows there are in all.
    pub fn len(&self) -> usize {
        self.claims.len()
            + self.sources.len()
            + self.relations.len()
            + self.links.len()
            + self.anchors.len()
            + self.invalid.len()
    }

    /// Is there nothing at all?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What one block of a view's log did to one path, as the writes index keeps it (RFC 006 D-8; handoff G-T4, corrections J-1, J-2).
/// **The text of a path at a block is the text the change that last wrote the path gave it**; this is what says who that was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteKind {
    /// The block created the path, or edited it (or replaced it, or moved it here **and** edited it): its bytes are this block's.
    Wrote,
    /// The block removed the path, or moved it away: nothing is there.
    Deleted,
    /// The block **moved the path here, with none of its bytes touched** (a pure rename): the text is that of the last writer of
    /// `from` before this block, which is followed back when asked.
    Carried {
        /// The path the file had before the block.
        from: String,
    },
    /// The block **may have written** the path, and the index cannot say (an operation it could not read, a block too large to
    /// list). A row of the path `""` says it of **every** path. A reader counts the block among those that may have written the
    /// text.
    Maybe,
    /// The block is of a kind whose effect on the files is not listed (a merge, a repair, an import, a rollback): a text that
    /// passes through it **cannot be attributed**. Only a row of the path `""` is of this kind.
    Opaque,
}

/// One path a block wrote, moved or removed, with what it did (see [`WriteKind`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathWrite {
    /// The repository path; `""` for a row that is about every path ([`WriteKind::Maybe`], [`WriteKind::Opaque`]).
    pub path: String,
    /// What the block did.
    pub kind: WriteKind,
}

/// One block of a view's log and what it did, in the order the view numbers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWrites {
    /// The block's number in its view's log: what blocks are ordered by (prikk's `update_seq`).
    pub number: u64,
    /// The block's id.
    pub block: String,
    /// The proposal its message names, when it does (a text as the message gives it, never checked here).
    pub proposal: Option<String>,
    /// What it did, one row per path.
    pub writes: Vec<PathWrite>,
}

impl BlockWrites {
    /// Is the block well formed? A row of the path `""` is a *may have written* or an *opaque* one and no other; an opaque row is of
    /// that path alone; a carried row names where it came from. (The SQLite schema's own checks say the same.)
    pub fn validate(&self) -> crate::Result<()> {
        let refused = |what: &str| Err(crate::StoreError::Backend(what.to_owned()));
        if self.block.is_empty() {
            return refused("a block of the writes index has an id");
        }
        for write in &self.writes {
            let general = matches!(write.kind, WriteKind::Maybe | WriteKind::Opaque);
            if write.path.is_empty() && !general {
                return refused("a certain row of the writes index names a path");
            }
            if write.kind == WriteKind::Opaque && !write.path.is_empty() {
                return refused("an opaque row of the writes index is of no one path");
            }
            if matches!(&write.kind, WriteKind::Carried { from } if from.is_empty()) {
                return refused("a carried row of the writes index names where it came from");
            }
        }
        Ok(())
    }
}

/// How far a view's writes index goes, and from where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritesState {
    /// The number of the newest block indexed: every block of the view from `floor_number` to here is.
    pub head_number: u64,
    /// That block's id, which a caller compares with the log to see that the index is of the log it reads.
    pub head_block: String,
    /// The number of the oldest block indexed.
    pub floor_number: u64,
    /// The oldest block indexed is the view's first block: nothing is written before it. When not, a path that has no row was
    /// written, if at all, **before what the index knows**.
    pub genesis: bool,
}

/// A block of a view's log, as the writes index names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedBlock {
    /// The block's number in the view.
    pub number: u64,
    /// Its id.
    pub block: String,
    /// The proposal its message names.
    pub proposal: Option<String>,
}

/// The newest row of a path at or before a block that settles who wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastWrite {
    /// The number of the block.
    pub number: u64,
    /// What it did to the path.
    pub kind: WriteKind,
}

/// A block that may have written a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaybeWriter {
    /// The block's number.
    pub number: u64,
    /// Its effect cannot be told at all ([`WriteKind::Opaque`]), not only for this path.
    pub opaque: bool,
}

/// What the writes index says of a path at or before a block: the newest block that **certainly** did something to it, and the
/// blocks after that one (up to the block asked for) that **may have** written it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Writers {
    /// The newest [`WriteKind::Wrote`], [`WriteKind::Deleted`] or [`WriteKind::Carried`] row, if the index has one.
    pub last: Option<LastWrite>,
    /// The blocks after `last` (every block when there is none) that [`WriteKind::Maybe`] or [`WriteKind::Opaque`] cover,
    /// oldest first: at most [`MAX_MAYBE_WRITERS`].
    pub maybes: Vec<MaybeWriter>,
    /// There were more of them than are listed.
    pub more: bool,
}

/// The most blocks a [`Writers`] lists that may have written a path.
pub const MAX_MAYBE_WRITERS: usize = 64;

/// A later block that wrote a path, as [`DerivedStore::later_writes`] lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaterWrite {
    /// The block's number.
    pub number: u64,
    /// Its id.
    pub block: String,
    /// What it did to the path ([`WriteKind::Wrote`] or [`WriteKind::Carried`]).
    pub kind: WriteKind,
}

/// The most blocks [`DerivedStore::later_writes`] lists.
pub const MAX_LATER_WRITES: usize = 100;

/// A later block that moved a file away with its bytes, as [`DerivedStore::later_moves`] lists it (correction L-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaterMove {
    /// The block's number.
    pub number: u64,
    /// Its id.
    pub block: String,
    /// The path the file was moved to (the row is a [`WriteKind::Carried`] row of this path, from the path asked about).
    pub to: String,
}

/// The most blocks [`DerivedStore::later_moves`] lists.
pub const MAX_LATER_MOVES: usize = 32;

/// The most blocks a view keeps as *not in its log* ([`DerivedStore::note_block_absent`]).
pub const MAX_ABSENT_BLOCKS: usize = 1_000;

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
    /// counts once and so does a whole index; the knowledge index counts once too). Called after a tip moves, with the tips every view of the
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

    /// The derived knowledge index of `wiki` at `tip` (schema version 13), if it was stored complete. Built from
    /// the same extract of the tip as the title index and dropped and rebuilt the same way; never a source of truth.
    fn knowledge_index(&self, wiki: &WikiId, tip: &str) -> Result<Option<KnowledgeRows>>;
    /// Store the complete knowledge index of `wiki` at `tip`, replacing what was there. It becomes visible all at
    /// once: a reader sees all of it or none.
    fn put_knowledge_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        rows: &KnowledgeRows,
        built_at: i64,
    ) -> Result<()>;

    /// When this service first saw each block of `wiki`, in Unix seconds, in the order asked: a block not
    /// seen before is recorded as seen at `now`. **An observation, not a fact about the change**: prikk's
    /// history carries no time, so this is the only time the pages can show (ledger: the log has no
    /// timestamps), and they say so. It is kept when the cache is cleared and dropped with nothing else.
    fn first_seen(&self, wiki: &WikiId, blocks: &[String], now: i64) -> Result<Vec<i64>>;

    // ------------------------------------------------------------------------------------ the writes index

    /// How far `view`'s writes index of `wiki` goes (RFC 006 D-8; handoff G-T4, correction J-2): `None` when it has none. **The
    /// index is of blocks and not of tips**: a block's writes never change, so no tip moving drops it, and only
    /// [`DerivedStore::clear`] and [`DerivedStore::reset_writes`] do.
    fn writes_state(&self, wiki: &WikiId, view: &str) -> Result<Option<WritesState>>;
    /// Add blocks to `view`'s index **and move its state to `state`, in one step** (a reader sees all of it or none). `blocks` are
    /// in the order of their numbers and follow what the index has, or begin it; a block already there is kept as it is (two
    /// readers that build the same blocks agree, and the second changes nothing).
    fn append_writes(
        &self,
        wiki: &WikiId,
        view: &str,
        blocks: &[BlockWrites],
        state: &WritesState,
    ) -> Result<()>;
    /// Drop `view`'s index of `wiki`: the next reader builds it again. **The view's generation moves** ([`DerivedStore::writes_generation`]),
    /// in the same step.
    fn reset_writes(&self, wiki: &WikiId, view: &str) -> Result<()>;
    /// **The generation of `view`'s writes index** (RFC 006 D-8; handoff G-T4, correction L-9): a number that [`DerivedStore::reset_writes`]
    /// and [`DerivedStore::clear`] move forward when they drop the index, and nothing else does, so that a reader who found the index
    /// `Ready` and went on to read its rows can see, at the end, that the index it read is still the one it began with. It is
    /// **read before the index's state** and read again after the rows, and a reader whose two numbers differ gives no answer
    /// (the rows it read may be of a half-dropped index, which says *nobody wrote this* when it only says *nothing is kept*). A view
    /// that has never been dropped is at 0.
    fn writes_generation(&self, wiki: &WikiId, view: &str) -> Result<u64>;
    /// **Was `block` found not to be in `view`'s log, with the view at the block `view_tip`?** (handoff G-T4, correction L-1.) What a
    /// reader that asked once and found the view without the block keeps, so that the question is not asked again for every request:
    /// the answer holds **while the view's tip is `view_tip`** (a view that has moved is asked again), and is dropped with the index
    /// ([`DerivedStore::reset_writes`], [`DerivedStore::clear`]). A view keeps at most [`MAX_ABSENT_BLOCKS`] of them, the oldest dropped.
    fn block_absent(&self, wiki: &WikiId, view: &str, block: &str, view_tip: &str) -> Result<bool>;
    /// Keep that `block` is not in `view`'s log with the view at the block `view_tip` (see [`DerivedStore::block_absent`]), replacing
    /// what was kept for the block.
    fn note_block_absent(
        &self,
        wiki: &WikiId,
        view: &str,
        block: &str,
        view_tip: &str,
    ) -> Result<()>;
    /// The block of `view`'s index with this id: `None` when the index does not hold it (not built that far, or not a block of the
    /// view).
    fn indexed_block(&self, wiki: &WikiId, view: &str, block: &str)
    -> Result<Option<IndexedBlock>>;
    /// The block of `view`'s index with this number.
    fn indexed_block_at(
        &self,
        wiki: &WikiId,
        view: &str,
        number: u64,
    ) -> Result<Option<IndexedBlock>>;
    /// **Who wrote `path` at or before the block numbered `at`**: the newest certain row of the path, and the blocks after it that
    /// may have written it. A few indexed lookups, whatever the length of the history and the number of changes hidden.
    fn last_writes(&self, wiki: &WikiId, view: &str, path: &str, at: u64) -> Result<Writers>;
    /// The blocks after the one numbered `after` that wrote `path` (a [`WriteKind::Wrote`] or a [`WriteKind::Carried`] row), oldest
    /// first, at most `limit` (never more than [`MAX_LATER_WRITES`], at least one) **and one more** when there are more, so that a
    /// caller can say *and more*.
    fn later_writes(
        &self,
        wiki: &WikiId,
        view: &str,
        path: &str,
        after: u64,
        limit: usize,
    ) -> Result<Vec<LaterWrite>>;
    /// The blocks after the one numbered `after` that **may have written** `path` and that the index cannot say did or did not
    /// (correction L-6): a [`WriteKind::Maybe`] row of the path or of every path, or an [`WriteKind::Opaque`] block. Oldest first, one
    /// per block (opaque when either row is), at most [`MAX_MAYBE_WRITERS`] **and one more** when there are more.
    fn later_maybes(
        &self,
        wiki: &WikiId,
        view: &str,
        path: &str,
        after: u64,
    ) -> Result<Vec<MaybeWriter>>;
    /// The blocks after the one numbered `after` that **moved the file at `path` away with its bytes** (a [`WriteKind::Carried`] row
    /// whose origin is `path`), oldest first, at most `limit` (never more than [`MAX_LATER_MOVES`], at least one) **and one more**
    /// when there are more. One indexed lookup, whatever the length of the history.
    fn later_moves(
        &self,
        wiki: &WikiId,
        view: &str,
        path: &str,
        after: u64,
        limit: usize,
    ) -> Result<Vec<LaterMove>>;
}
