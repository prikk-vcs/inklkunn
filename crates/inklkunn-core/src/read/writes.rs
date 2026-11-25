//! **Who wrote a path at a block** (RFC 006 D-8; INV-13; handoff G-T4, corrections J-1 and J-2): the rule the one reader of a page's
//! text at a block ([`ReadService::text_at`]) decides *hidden* by, and the index that answers it with a lookup.
//!
//! # The rule (J-1)
//!
//! **The text of the path P at the block X is hidden if and only if the change that last wrote P at or before X has its content
//! hidden.** Nothing else hides text: bytes that another change wrote independently, equal or not, are not hidden, and a hide
//! covers the change it was made on. A change that **moved** P here without touching its bytes (a *pure rename*) wrote nothing: the
//! text is that of the change that last wrote the path it came from, which is followed back, as far as the renames go.
//!
//! **Any case that cannot be settled gives nothing.** No bound, no cap and no failure to read ever answers *not hidden*:
//!
//! - a change that **may have written** P, and that the index cannot say did not (an operation whose path prikk could not resolve,
//!   an operation of a kind this reading does not know, a change too large to list), is counted among the writers of P and of
//!   every path after it: the text is hidden when it **or** the change before it that certainly wrote P is. That is exact, not
//!   merely safe: if neither is hidden, whichever wrote the text is not;
//! - a block whose effect on the files its operations do not list (a merge, a repair or an import block, or prikk's own rollback)
//!   is **opaque**: a text that passes through one cannot be attributed, and is not given while any change of the wiki is hidden;
//! - a text that is there while the index knows nobody who wrote it (a block the index does not hold, a path the index has no row
//!   for, a rename chain longer than [`MAX_RENAME_HOPS`], a log too long to be indexed, an index that is not built that far yet, a
//!   block that only a view the request did not ask holds) is **unattributed**.
//!
//! **An unattributed text is not hidden, and it is not served** (correction L-11). It is answered with
//! [`ReadError::NotCheckedYet`] (*this text cannot be checked yet; try again in a little while*), which shows **no tombstone**: a
//! tombstone says that a moderator hid something, and this is not that. A hide is the one reason a tombstone is shown for.
//!
//! # What *wrote* means in prikk's block model
//!
//! A **block** is a sealed change of a view: one or more **patches**, each an ordered list of **operations** on files, applied to
//! the block's one parent (a view's log is a line; a view forked from another shares its first blocks). A block's `show` names its
//! operations: *create-file* and *create-symlink* (a path), *edit-text* and *replace-binary* (a file, by the node it is, named by
//! the path it has **at the end of its patch**), *delete-node* (a path), *rename-path* (an old path and a new one: the file moves
//! and keeps what it is), and *change-perm* (the mode: no bytes). The effect of a block on one path is read from its operations in
//! order, a patch's path-addressed operations first and then its edits:
//!
//! | The block did, to P | It is |
//! |---|---|
//! | created P, edited it, or replaced its bytes | **wrote** P |
//! | moved a file to P **and** edited it (a rename with edits) | **wrote** P (the file is the block's now) |
//! | moved a file to P and edited nothing | **carried** P from the old path: the writer is the old path's before the block |
//! | removed P, or moved it away | **deleted** P: there is no text |
//! | changed the mode of P | nothing |
//! | removed P and created it again, in this block or in a later one | **wrote** P: the writer is the block that created it, **whatever the bytes** (a path that was deleted and written again with the same bytes is the later change's) |
//!
//! A path that a later block *wrote the same bytes to* is that block's: equal bytes are not one writer.
//!
//! # The index (J-2)
//!
//! For each view, each block of its log in order and, per block, one row for each path it wrote, carried or deleted (and one for
//! the path `""` when the block may have written any, or is opaque) live in the derived store (memory and SQLite;
//! [`inklkunn_store::DerivedStore::last_writes`]). **A check is a lookup**: the block asked for is found by its id, the newest row of
//! the path at or before it is found by the path and the block's number, and the change is asked whether its content is hidden: a
//! few indexed queries, whatever the length of the history and the number of changes that are hidden. A hide or an unhide changes
//! nothing of the index: it is one more or one less change in the set the answer is read against, and that set is never listed.
//!
//! The index is **derived**: built from the repository's own record of each block (`show`, once per block, held), oldest block
//! first, **only while a change of the wiki has its content hidden** (a wiki in which nothing has its content hidden pays one query
//! for a read, and builds nothing), kept as the view's log grows (a new block costs one `show`), and built again after
//! [`ReadService::clear_derived`]. It is built **by the server in the background and, for what a request needs, by the request**
//! (correction P-5):
//!
//! - **In the background.** After a content hide, and at start-up while one is in force, the server asks for the index of each
//!   view to be built to the view's tip ([`WritesBackground`], [`ReadService::background_step`]), one small step at a time, so that
//!   an old version of a page answers with its text or its tombstone **without any reader having walked the history to make it**.
//!   A step is bounded ([`BACKGROUND_STEP_SHOWS`] blocks that cost a process, [`BACKGROUND_STEP_TIME`]), holds the view's lock only
//!   for itself, and is **resumable**: the index is derived and kept, so a restart, a failure or a shutdown loses nothing.
//! - **By a request.** **A request's own part is bounded by time** ([`DEFAULT_REQUEST_BUDGET`], about a second, **one budget for
//!   the whole request** and every view it asks; correction Q-3), with [`MAX_INDEXED_SHOWS`] blocks that cost a process as a hard
//!   backstop (blocks the service already holds cost none): it indexes what it may and answers *not checked yet* for what it could
//!   not reach (**never the text**); the requests after it, and the background, go on from where it stopped. A request always
//!   indexes at least the block it is at when the budget is first looked at (it is never refused for time before it began), and a
//!   lookup of the same request that comes after the budget is spent builds nothing: it finds what is indexed and is answered
//!   *not checked yet* for the rest, **without a `show`**.
//!
//!   **Which entries share one budget.** The budget is made once where the request enters the read service, and handed down:
//!   [`ReadService::page_diff`] (both sides), [`ReadService::page_at`] (the page and every page it includes),
//!   [`ReadService::rollback_plan`] and [`ReadService::way_back`] (every page's two texts), [`ReadService::later_holders`] (the
//!   writers of every path and their texts), [`ReadService::attached_sources`] (the files of the one change it is asked about),
//!   [`ReadService::rule_history`], and the two reads
//!   of the indexes at a proposal's base ([`ReadService::title_index_at_base`], [`ReadService::knowledge_index_at_base`], which
//!   ask the index once for both). **Every other public entry is one lookup and has the budget of one request for itself, per
//!   call**: [`ReadService::text_at`], [`ReadService::hidden_text`], [`ReadService::source_at`], [`ReadService::source_or_hidden`],
//!   [`ReadService::later_writers`], [`ReadService::warm_writes`]. A page that asks several of them in turn (a proposal's page, which
//!   reads each file at the base through its own call) pays one budget for each, and the second finds the block the first
//!   reached already indexed (an index is of blocks, and the base is one block): what a second call spends is spent on a block
//!   the first did not reach.
//!
//! Reading a view's log further back than [`MAX_WINDOW`] blocks is not done: the oldest blocks of a longer history are unindexed,
//! and a text written there is unattributed.
//!
//! **Every view is read under the same rule.** The index is per view (a view's log orders its blocks), built when a read names the
//! view, and a read of a block that is not in the view's log is unattributed.
//!
//! # Which view holds a block (correction L-1)
//!
//! A block that the **default** view's log does not hold at all (a proposal's base in another view) is looked for in the wiki's other
//! views, because the changes before a block are the block's own whichever view's log it is read in. **Only then**: a block that the
//! default view's log holds and the index has not reached yet is *not yet checked*, and is never looked for elsewhere. What it costs
//! is bounded and paid once:
//!
//! - that **a view does not hold a block** is kept in the derived store with the view's tip ([`inklkunn_store::DerivedStore::note_block_absent`]), so
//!   the question is asked once for the view's tip and never again for a request;
//! - the list of the open views is the one a `branch list` gave and the tip holds ([`ReadConfig::tip_ttl`](super::ReadConfig::tip_ttl)),
//!   not a call of its own;
//! - **at most [`MAX_VIEWS_ASKED`] views that were not known not to hold the block are asked in one request**; the rest are answered
//!   *not checked yet*, and the next request asks the ones after them (each answer kept), so that a block of a wiki of a hundred
//!   views is found in a few requests and none of them pays for all.
//!
//! # A read of an index that is dropped under it (correction L-9)
//!
//! The index has a **generation** ([`inklkunn_store::DerivedStore::writes_generation`]) that dropping it moves forward. A reader
//! reads it **before anything of the index** (before the block is placed in the view's index, so before the index is built toward
//! it too, correction P-3), and again **after** the rows it asked for, and a reader whose two numbers differ gives no answer: the
//! rows it read may be those of an index that was being dropped, or placed by one that is gone (no row for a path, with a state
//! that still says the index began at the view's first block, is *nobody wrote this*, which is not true of an index that is gone).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use inklkunn_store::{
    Appended, BlockWrites, IndexedBlock, LastWrite, LaterWrite, MAX_LATER_WRITES,
    MAX_MAYBE_WRITERS, PathWrite, StoreError, WikiId, WikiRecord, WriteKind, WritesState,
};

use super::history::{Hold, MAX_WINDOW, Touch, TouchKind, Touches};
use super::{Cause, ReadError, ReadService};
use crate::tombstone::Mark;

/// **The backstop** of a request's part in building the index: the most blocks that cost a `show` (a process) each that one request
/// indexes, over every view it asks. What bounds a request is [`DEFAULT_REQUEST_BUDGET`] (time); this bounds it when time does not
/// (a clock that does not move, a storage that answers at once). A block the service holds already costs none.
pub const MAX_INDEXED_SHOWS: usize = 512;
/// How long a request may spend building the index (correction P-5): about a second, over every view the request asks.
/// [`ReadService::with_writes_budget`] sets another.
pub const DEFAULT_REQUEST_BUDGET: Duration = Duration::from_secs(1);
/// The most blocks that cost a `show` one **background** step indexes. A step is short so that a reader who needs the same view's
/// index does not wait for it for long.
pub const BACKGROUND_STEP_SHOWS: usize = 16;
/// The longest one background step goes on after its first `show` (it never stops before one).
pub const BACKGROUND_STEP_TIME: Duration = Duration::from_secs(1);
/// The most views one wiki's background build looks after (the default view first); a view beyond them is built by the
/// requests that need it, as before the background build.
pub const MAX_BACKGROUND_VIEWS: usize = MAX_VIEWS_ASKED;
/// The most blocks one call indexes in all, held or not (a bound on the work of one request).
const MAX_INDEXED_BLOCKS: usize = 5_000;
/// The most renames followed back from a path to the change that wrote its text. A longer chain is unattributed.
pub const MAX_RENAME_HOPS: usize = 32;
/// The most times a file's move away is followed from one path, over the whole chain of moves (correction L-6): the later changes of
/// the paths it was moved to are candidates too. A file moved more often than this is followed that far, and the answer says so
/// ([`LaterWriters::moves_unfollowed`]).
pub const MAX_FOLLOWED_MOVES: usize = 8;
/// The most views a request that looks for a block outside the default view asks, **counting only the views that were not known
/// not to hold it** (what a view is found not to hold is kept). The rest are answered *not checked yet*.
pub const MAX_VIEWS_ASKED: usize = 16;

/// A monotonic clock, as the time since an origin of its own: what a request's time budget is read from. A test passes one that
/// ticks by itself ([`ReadService::with_writes_budget`]).
pub type Monotonic = Arc<dyn Fn() -> Duration + Send + Sync>;

/// The clock a service has unless it is given another: the time since it was made.
pub(super) fn real_clock() -> Monotonic {
    let origin = std::time::Instant::now();
    Arc::new(move || origin.elapsed())
}

/// **What one call that builds the index may spend** (correction P-5): a time (read from the service's clock) and a number of
/// blocks that cost a process, shared by every view a request asks. It never stops a call **before** the call's first `show`
/// of the budget: whatever the clock says, a request that has not built anything yet builds at least one block, so a budget that is
/// spent only by time never starves the request that arrives late.
pub(super) struct Budget {
    clock: Monotonic,
    /// When the time is up, on `clock`; `None` for a budget of blocks only.
    until: Option<Duration>,
    /// The blocks that cost a process the budget still allows.
    shows_left: usize,
    /// The blocks that cost a process the budget has been spent on.
    fetched: usize,
    /// Whether the budget is a request's (and not the background's): a request that built something is a reader that is working,
    /// which the background yields to ([`ReadService::reader_advances`]).
    request: bool,
}

impl Budget {
    /// Whether the call is to stop now: something was built under the budget, and its time or its blocks are spent.
    fn stop(&self) -> bool {
        self.fetched > 0
            && (self.shows_left == 0 || self.until.is_some_and(|until| (self.clock)() >= until))
    }

    fn spent_one(&mut self) {
        self.fetched += 1;
        self.shows_left = self.shows_left.saturating_sub(1);
    }
}

/// **The budget of one request** (correction Q-3): made **once**, where the request enters the read service
/// ([`ReadService::request_budget`]), and drawn from by **every lookup the request makes** (a diff's two sides, a rollback's
/// reads, a list of later holders, a page and the pages it includes). What the first lookup spends the second does not have: a
/// lookup that finds it spent builds nothing and is answered *not checked yet* (**never the text**), as a lookup that ran out
/// of it is. So a request's part in building the index is **one** [`DEFAULT_REQUEST_BUDGET`], not one for each thing it asks.
///
/// An entry that cannot be handed one (a public entry that is one lookup, or a caller that asks the service for many things in
/// turn) makes its own for each call: **per lookup**, which the entry's doc says.
pub(super) struct RequestBudget(Mutex<Budget>);

impl RequestBudget {
    /// Draw from the budget for one thing (one lookup, or one call of the builder).
    pub(super) fn with<T>(&self, draw: impl FnOnce(&mut Budget) -> T) -> T {
        draw(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// What the server does to build a wiki's writes index **in the background** (correction P-5). `inklkunn-core` starts no thread
/// it cannot stop and leaves the how to the binary, as it does for [`AfterSeal`](crate::AfterSeal): the binary runs one worker that
/// calls [`ReadService::background_step`] for the wikis it is asked about.
pub trait WritesBackground: Send + Sync {
    /// The wiki's index is to be built toward the tip of each of its views. **Never blocks and never waits for the work**; asking
    /// again while the wiki is being built is the same ask.
    fn ask(&self, wiki: &WikiRecord, why: AskedFor);
}

/// Why the background build is asked for (the binary may treat a reader's ask more patiently than a hide's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskedFor {
    /// A content hide was recorded, or one is in force as the server starts: the work is due.
    Hide,
    /// A reader met *not checked yet* while a hide is in force: it may be that nothing is building (an earlier build failed).
    Reader,
}

/// What one [`ReadService::background_step`] did.
#[derive(Debug, Clone)]
pub enum BackgroundStep {
    /// Nothing is left to do: no change of the wiki has its content hidden (any more), or every view's index reaches its tip.
    Idle,
    /// An index was built a step further and another step may be due.
    Progress,
    /// The step failed (prikk or the store): it is the read error. A step that failed built nothing wrong, and the next one retries.
    Failed(ReadError),
}

/// What the index says of the text of a path at a block, for the reader.
///
/// **A text that cannot be attributed is no outcome of this type**: it is the error [`ReadError::NotCheckedYet`], so that no
/// consumer can serve it, or show a tombstone for it, by forgetting a case.
#[derive(Debug, Clone)]
pub(super) enum Authorship {
    /// No change of the wiki has its content hidden: nothing is asked of the index.
    NothingHidden,
    /// The text is hidden by this hiding: the change that wrote it has its content hidden.
    Hidden(Box<Mark>),
    /// The text is no hidden change's.
    Shown {
        /// The index names a change that wrote the text. When it is `false` (a path nobody is recorded as having written, or one
        /// removed) a text that is **there** is unattributed, and the reader that finds one answers [`ReadError::NotCheckedYet`].
        written: bool,
    },
}

/// What asking one view's index for a block found (correction L-1).
enum Located {
    /// The block's place in the view's index, with the index's generation read **before** the block was looked for (L-9).
    At(IndexedBlock, u64),
    /// The view's log, as far as it is read, does not hold the block: nothing about it can be learned from this view.
    NotInLog,
    /// The view's log holds the block and the index has not reached it (the bound of a call), or the request has no more views to
    /// ask: *not checked yet*.
    NotYet,
}

/// What [`ReadService::advance_writes`] found of the block it was asked to build toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reach {
    /// The block is in the view's log (or none was asked for): the index was built as far as the bound of a call allowed.
    InLog,
    /// The block is not in the part of the view's log that is read: building the index does not reach it.
    NotInLog,
}

/// What a reader of paths at one block is given by [`ReadService::reading`]. **A block the index cannot place is no value of it**:
/// that is [`ReadError::NotCheckedYet`].
pub(super) enum Reading {
    /// No change of the wiki has its content hidden.
    NothingHidden,
    /// The block's place in the view's index.
    Ready(Ready),
}

/// The block of a view's index a reader asks the writers of paths at, and what it needs to answer for each of them.
pub(super) struct Ready {
    view: String,
    at: IndexedBlock,
    state: WritesState,
    /// The index's generation when the reader began: the reader gives no answer if it is not this at the end.
    generation: u64,
    /// What each change asked about is hidden by, by number, kept for the paths of one read.
    hidden: RefCell<HashMap<u64, Option<Mark>>>,
}

/// A later change that wrote a path: what the moderation form lists beside the sentence about a tip (J-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaterWriter {
    /// The change's block id.
    pub block: String,
    /// The change's number in the view's history.
    pub number: u64,
    /// The change moved the path here without touching its bytes (a pure rename), instead of writing it.
    pub carried: bool,
    /// The path the change wrote: the one asked about, or one the file was **moved to** after the block asked about (correction L-6;
    /// the text it wrote at the first path is, after the move, the text at this one).
    pub path: String,
}

/// The block before another in a view's log, as the writes index says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ParentBlock {
    /// The block is the first of the view: nothing came before it.
    First,
    /// The block before it.
    Block(String),
    /// The index does not reach the block (or its parent): nothing can be said.
    Unsettled,
}

/// The changes after a block that wrote a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaterWriters {
    /// At most the limit asked for, oldest first.
    pub writers: Vec<LaterWriter>,
    /// There are more than are listed.
    pub more: bool,
    /// The index does not reach the block yet (or it is not a block of the view), so nothing can be said: the list is not an answer.
    pub settled: bool,
    /// The numbers of the changes after the block that **may have written** the path (or any) and that the index cannot say did or
    /// did not (a path no operation of theirs resolved, a change too large to list, a kind of change whose effect is not listed), oldest
    /// first, **not counting one that is listed in `writers`**: they could not be checked, and the answer says so (correction L-6).
    pub untraced: Vec<u64>,
    /// There are more of them than the bound of the index lists: `untraced` is a lower bound.
    pub untraced_at_least: bool,
    /// The file was moved away more often than [`MAX_FOLLOWED_MOVES`] follows: the later changes of the paths it went to after the
    /// last one followed are not listed.
    pub moves_unfollowed: bool,
}

// ------------------------------------------------------------------------------------ what a block did

/// What a block did to one path, within the block (see the module's account).
enum Cell {
    Wrote,
    Deleted,
    /// Moved here, nothing of its bytes touched: from this path, as it was before the block.
    Moved(String),
}

/// The rows of one block: what it did to each path, read from its operations in order. `opaque` is a block of a kind whose effect
/// the operations do not list.
pub(super) fn rows_of(touches: &Touches, opaque: bool) -> Vec<PathWrite> {
    if opaque {
        return vec![PathWrite {
            path: String::new(),
            kind: WriteKind::Opaque,
        }];
    }
    let list = match touches {
        Touches::Known(list) => list,
        // Too large to list: it may have written anything.
        Touches::TooLarge => {
            return vec![PathWrite {
                path: String::new(),
                kind: WriteKind::Maybe,
            }];
        }
    };
    let mut cells: BTreeMap<String, Cell> = BTreeMap::new();
    let mut unreadable = false;
    for patch in list.chunk_by(|a: &Touch, b: &Touch| a.patch == b.patch) {
        // The operations that name a path are applied in order; an edit names the path its node has when the patch is done, so it
        // is applied last.
        for touch in patch {
            match &touch.kind {
                TouchKind::Create => {
                    cells.insert(touch.path.clone(), Cell::Wrote);
                }
                TouchKind::Delete => {
                    cells.insert(touch.path.clone(), Cell::Deleted);
                }
                TouchKind::Rename { from } if *from != touch.path => {
                    let moved = match cells.get(from) {
                        // A file made in this block is the block's, wherever it ends up (and what is not there cannot be moved:
                        // the block is then taken to have written the path).
                        Some(Cell::Wrote | Cell::Deleted) => Cell::Wrote,
                        Some(Cell::Moved(origin)) => Cell::Moved(origin.clone()),
                        None => Cell::Moved(from.clone()),
                    };
                    cells.insert(from.clone(), Cell::Deleted);
                    cells.insert(touch.path.clone(), moved);
                }
                TouchKind::Unreadable => unreadable = true,
                TouchKind::Rename { .. } | TouchKind::Edit | TouchKind::Other => {}
            }
        }
        for touch in patch.iter().filter(|t| t.kind == TouchKind::Edit) {
            cells.insert(touch.path.clone(), Cell::Wrote);
        }
    }
    let mut rows: Vec<PathWrite> = cells
        .into_iter()
        .filter(|(path, _)| !path.is_empty())
        .map(|(path, cell)| PathWrite {
            path,
            kind: match cell {
                Cell::Wrote => WriteKind::Wrote,
                Cell::Deleted => WriteKind::Deleted,
                Cell::Moved(from) => WriteKind::Carried { from },
            },
        })
        .collect();
    if unreadable {
        rows.push(PathWrite {
            path: String::new(),
            kind: WriteKind::Maybe,
        });
    }
    rows
}

// ------------------------------------------------------------------------------------ the service

/// One lock per `(wiki, view)`, so that an index is built by one reader at a time and the next finds it built.
pub(super) type Locks = Mutex<HashMap<(WikiId, String), Arc<Mutex<()>>>>;

impl ReadService {
    fn writes_failed(&self, e: StoreError) -> ReadError {
        self.store_failures.fetch_add(1, Ordering::Relaxed);
        ReadError::CouldNotBeReadNow(Cause::Store(e))
    }

    pub(super) fn tombstone_failed(&self, e: crate::tombstone::Unreadable) -> ReadError {
        self.store_failures.fetch_add(1, Ordering::Relaxed);
        ReadError::CouldNotBeReadNow(Cause::Store(StoreError::Backend(e.0)))
    }

    /// **Whose text is the text of `path` at the block `block_id` of `view`** (the default view for `None`), as the module's account
    /// says. The one function [`ReadService::hidden_text`] and the reads of a file are decided by. A text that cannot be attributed is
    /// [`ReadError::NotCheckedYet`].
    pub(super) fn authorship(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
        budget: &RequestBudget,
    ) -> Result<Authorship, ReadError> {
        Ok(match self.reading(wiki, view, block_id, budget)? {
            Reading::NothingHidden => Authorship::NothingHidden,
            Reading::Ready(ready) => self.authorship_in(wiki, &ready, path)?,
        })
    }

    /// **What a reader of many paths at one block asks once**: whether any change of the wiki has its content hidden (one query), and
    /// the block's place in the view's index (built toward it within what is left of the request's `budget`). [`ReadService::authorship_in`] then answers
    /// for each path. **A service without the record of what moderation hides is an error** (correction L-8), and so is a block the
    /// index cannot place ([`ReadError::NotCheckedYet`]).
    pub(super) fn reading(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        budget: &RequestBudget,
    ) -> Result<Reading, ReadError> {
        let Some(tombstones) = &self.tombstones else {
            return Err(ReadError::CouldNotBeReadNow(Cause::ModerationNotConfigured));
        };
        if tombstones
            .content_hide_in_force(&wiki.id)
            .map_err(|e| self.tombstone_failed(e))?
            .is_none()
        {
            return Ok(Reading::NothingHidden);
        }
        // The index's generation is read inside, before the block is looked for in the view's index (before anything of the index is
        // read, and before it is built toward the block), and again after the rows (see the module's account).
        let (view, at, generation) = match budget.with(|budget| {
            self.view_holding(wiki, view.unwrap_or(&wiki.default_view), block_id, budget)
        }) {
            Err(ReadError::NotCheckedYet) => return Err(self.starved(wiki)),
            held => held?,
        };
        let Some(state) = self
            .derived
            .writes_state(&wiki.id, &view)
            .map_err(|e| self.writes_failed(e))?
        else {
            return Err(self.starved(wiki));
        };
        Ok(Reading::Ready(Ready {
            view,
            at,
            state,
            generation,
            hidden: RefCell::new(HashMap::new()),
        }))
    }

    /// **The view whose index places the block, and the block's place in it.** The view named is asked first. A block that is not in
    /// the **default** view's log (a proposal's base in another view) is looked for in the wiki's other views, within the bounds of
    /// the module's account ([`MAX_VIEWS_ASKED`]): **the changes before a block are the block's own, whichever view's log it is read
    /// in**, so the answer is the same. A block the default view's log holds and the index has not reached, a block no view asked
    /// holds, and a request that had to leave views unasked are [`ReadError::NotCheckedYet`].
    fn view_holding(
        &self,
        wiki: &WikiRecord,
        view: &str,
        block_id: &str,
        budget: &mut Budget,
    ) -> Result<(String, IndexedBlock, u64), ReadError> {
        // The view named is always asked: the bound is of the views looked through after it.
        let mut unbounded = usize::MAX;
        match self.locate(wiki, view, block_id, &mut unbounded, Hold::Keep, budget)? {
            Located::At(at, generation) => return Ok((view.to_owned(), at, generation)),
            Located::NotYet => return Err(ReadError::NotCheckedYet),
            Located::NotInLog if view != wiki.default_view => return Err(ReadError::NotCheckedYet),
            Located::NotInLog => {}
        }
        let mut allowed = MAX_VIEWS_ASKED;
        for other in self.open_views_held(wiki)? {
            if other == wiki.default_view {
                continue;
            }
            // The logs of the views asked in turn are read once and not held (P-2): they would put out the default view's.
            match self.locate(wiki, &other, block_id, &mut allowed, Hold::Once, budget) {
                Ok(Located::At(at, generation)) => return Ok((other, at, generation)),
                Ok(Located::NotInLog) => {}
                // The block is in that view's log, and the index has not reached it; or no more views may be asked now.
                Ok(Located::NotYet) => return Err(ReadError::NotCheckedYet),
                // A view that closed since the list was read has nothing to ask.
                Err(ReadError::NoSuchView(_)) => {}
                Err(e) => return Err(e),
            }
        }
        // No view holds it.
        Err(ReadError::NotCheckedYet)
    }

    /// [`ReadService::authorship`] of one path of the block [`ReadService::reading`] settled. **Checked at its end** that the index
    /// it read is the one `ready` began with (the generation, see the module's account): an index dropped meanwhile is
    /// [`ReadError::NotCheckedYet`], whatever the rows said.
    pub(super) fn authorship_in(
        &self,
        wiki: &WikiRecord,
        ready: &Ready,
        path: &str,
    ) -> Result<Authorship, ReadError> {
        let outcome = self.authorship_of(wiki, ready, path)?;
        let now = self
            .derived
            .writes_generation(&wiki.id, &ready.view)
            .map_err(|e| self.writes_failed(e))?;
        if now != ready.generation {
            return Err(ReadError::NotCheckedYet);
        }
        Ok(outcome)
    }

    fn authorship_of(
        &self,
        wiki: &WikiRecord,
        ready: &Ready,
        path: &str,
    ) -> Result<Authorship, ReadError> {
        let Some(tombstones) = &self.tombstones else {
            return Err(ReadError::CouldNotBeReadNow(Cause::ModerationNotConfigured));
        };
        let Ready {
            view,
            at,
            state,
            hidden,
            ..
        } = ready;
        let view = view.as_str();
        let mut candidates: Vec<u64> = Vec::new();
        let (mut path, mut number) = (path.to_owned(), at.number);
        let mut written = false;
        let mut settled = false;
        for _ in 0..=MAX_RENAME_HOPS {
            let writers = self
                .derived
                .last_writes(&wiki.id, view, &path, number)
                .map_err(|e| self.writes_failed(e))?;
            if writers.more || writers.maybes.iter().any(|m| m.opaque) {
                return Err(ReadError::NotCheckedYet);
            }
            candidates.extend(writers.maybes.iter().map(|m| m.number));
            match writers.last {
                Some(LastWrite {
                    number: wrote,
                    kind: WriteKind::Wrote,
                }) => {
                    candidates.push(wrote);
                    written = true;
                    settled = true;
                    break;
                }
                // Removed: there is no text of the writer's to serve.
                Some(LastWrite {
                    kind: WriteKind::Deleted,
                    ..
                }) => {
                    settled = true;
                    break;
                }
                // Moved here, nothing touched: the writer is the one of the path it came from, before this block.
                Some(LastWrite {
                    number: carried,
                    kind: WriteKind::Carried { from },
                }) => {
                    let Some(before) = carried.checked_sub(1) else {
                        return Err(ReadError::NotCheckedYet);
                    };
                    path = from;
                    number = before;
                }
                // No row of the path: nothing wrote it since the index begins, and if the index begins at the view's first block,
                // nothing wrote it at all.
                None => {
                    if state.genesis {
                        settled = true;
                        break;
                    }
                    return Err(ReadError::NotCheckedYet);
                }
                // A row that is no certain one is never `last`.
                Some(LastWrite { .. }) => return Err(ReadError::NotCheckedYet),
            }
        }
        if !settled {
            // A rename chain longer than the bound.
            return Err(ReadError::NotCheckedYet);
        }
        candidates.sort_unstable();
        candidates.dedup();
        for number in candidates {
            // What a change's hiding is, asked once for a reader of many paths (a hide cannot change within one read's account).
            let held = hidden.borrow().get(&number).cloned();
            let verdict = match held {
                Some(verdict) => verdict,
                None => {
                    let Some(block) = self
                        .derived
                        .indexed_block_at(&wiki.id, view, number)
                        .map_err(|e| self.writes_failed(e))?
                    else {
                        return Err(ReadError::NotCheckedYet);
                    };
                    let verdict = tombstones
                        .content_hidden(&wiki.id, &block.block, block.proposal.as_deref())
                        .map_err(|e| self.tombstone_failed(e))?;
                    hidden.borrow_mut().insert(number, verdict.clone());
                    verdict
                }
            };
            if let Some(mark) = verdict {
                return Ok(Authorship::Hidden(Box::new(mark)));
            }
        }
        Ok(Authorship::Shown { written })
    }

    /// The block of `view`'s index that is `block_id`, built toward when it is not there yet (within what is left of the request's
    /// `budget`): `None`
    /// when the view's index does not hold it (it is not a block of the view's log, or the index has not reached it). A caller that
    /// must tell the two apart asks [`ReadService::locate`].
    fn indexed_through(
        &self,
        wiki: &WikiRecord,
        view: &str,
        block_id: &str,
        budget: &RequestBudget,
    ) -> Result<Option<IndexedBlock>, ReadError> {
        let mut unbounded = usize::MAX;
        Ok(
            match budget.with(|budget| {
                self.locate(wiki, view, block_id, &mut unbounded, Hold::Keep, budget)
            })? {
                Located::At(at, _) => Some(at),
                Located::NotInLog | Located::NotYet => None,
            },
        )
    }

    /// **Where `block_id` is in `view`'s index**, and why not when it is not (correction L-1). What is already known costs a query
    /// or two and no process: the block held by the index, or the view found not to hold it **at the tip it has now**
    /// ([`inklkunn_store::DerivedStore::block_absent`]). What is not is **paid for out of `allowed`**, one for each view that has to be
    /// asked (built toward the block, within the bound of a call), and a view that cannot be paid for is [`Located::NotYet`]: the
    /// request is not the one to ask it. A view found not to hold the block is **kept** as one, with its tip. The log of the view is held for the next
    /// requests only when `hold` says so (P-2).
    fn locate(
        &self,
        wiki: &WikiRecord,
        view: &str,
        block_id: &str,
        allowed: &mut usize,
        hold: Hold,
        budget: &mut Budget,
    ) -> Result<Located, ReadError> {
        let failed = |e| self.writes_failed(e);
        // Before anything of the view's index is read or built (correction P-3, L-9).
        let generation = self
            .derived
            .writes_generation(&wiki.id, view)
            .map_err(failed)?;
        if let Some(at) = self
            .derived
            .indexed_block(&wiki.id, view, block_id)
            .map_err(failed)?
        {
            return Ok(Located::At(at, generation));
        }
        let tip = self.tip(wiki, Some(view))?;
        if self
            .derived
            .block_absent(&wiki.id, view, block_id, tip.block_id())
            .map_err(failed)?
        {
            return Ok(Located::NotInLog);
        }
        let Some(left) = allowed.checked_sub(1) else {
            return Ok(Located::NotYet);
        };
        *allowed = left;
        let held = self.advance_writes(wiki, view, Some(block_id), hold, budget)?;
        if let Some(at) = self
            .derived
            .indexed_block(&wiki.id, view, block_id)
            .map_err(failed)?
        {
            return Ok(Located::At(at, generation));
        }
        Ok(match held {
            Reach::InLog => Located::NotYet,
            Reach::NotInLog => {
                self.derived
                    .note_block_absent(&wiki.id, view, block_id, tip.block_id())
                    .map_err(failed)?;
                Located::NotInLog
            }
        })
    }

    fn writes_lock(&self, wiki: &WikiId, view: &str) -> Arc<Mutex<()>> {
        let mut locks = self
            .writes_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry((wiki.clone(), view.to_owned())).or_default())
    }

    /// **Index the blocks of `view`'s log that the index does not hold, oldest first**, until the block `until` is in it (when one
    /// is named) or the `budget` is spent (its time, or its blocks that cost a `show`; see [`Budget`]). One builder works on a view
    /// at a time; the next finds what it built. An index that is not of the log read now (its newest block is not the log's block of that
    /// number) is dropped and built again; the logs it reads are held only when `hold` says so (P-2). **An error is prikk's or the store's**, never the index being unfinished. [`Reach::NotInLog`]
    /// says that the block asked for is not in the part of the view's log that is read.
    pub(super) fn advance_writes(
        &self,
        wiki: &WikiRecord,
        view: &str,
        until: Option<&str>,
        hold: Hold,
        budget: &mut Budget,
    ) -> Result<Reach, ReadError> {
        let lock = self.writes_lock(&wiki.id, view);
        let _building = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let failed = |e| self.writes_failed(e);
        if let Some(block) = until
            && self
                .derived
                .indexed_block(&wiki.id, view, block)
                .map_err(failed)?
                .is_some()
        {
            return Ok(Reach::InLog);
        }
        let tip = self.tip(wiki, Some(view))?;
        // The generation the build begins under: an index dropped while it builds (an administrator's refresh, in this process or
        // another) is not appended to, for what would be left is the newest blocks of a log with the oldest ones missing, under a
        // state that says the index begins at the view's first block. **The store guarantees it, and nothing else does**: the generation
        // goes into `append_writes`, which compares it with the view's inside the one step that appends (a transaction that takes the
        // write lock, or the memory store's lock), so a drop at any moment of the build, up to the append, appends nothing (correction
        // Q-2). There is no look at the generation before the append: it would only spare the store a call it refuses, and it could not
        // be the guard.
        let mut generation = self
            .derived
            .writes_generation(&wiki.id, view)
            .map_err(failed)?;
        let mut state = self.derived.writes_state(&wiki.id, view).map_err(failed)?;
        // The log, back to the newest block the index holds (the whole log, as far as it is read, for an index that has none).
        let first = self.window_held(wiki, &tip, super::history::DEFAULT_WINDOW, hold)?;
        let top = first.infos().first().map_or(0, |b| b.number);
        let need = match &state {
            Some(held) => usize::try_from(top.saturating_sub(held.head_number))
                .unwrap_or(MAX_WINDOW)
                .saturating_add(2),
            None => MAX_WINDOW,
        };
        let mut window = if first.is_complete() || first.infos().len() >= need {
            first
        } else {
            self.window_held(wiki, &tip, need, hold)?
        };
        if let Some(held) = &state {
            let same = window
                .infos()
                .iter()
                .find(|b| b.number == held.head_number)
                .is_some_and(|b| b.block_id == held.head_block);
            if !same {
                // Not the log this index was built from: begin again, from the whole log.
                self.derived.reset_writes(&wiki.id, view).map_err(failed)?;
                // Our own drop moves the generation: the build goes on under the one it left.
                generation = self
                    .derived
                    .writes_generation(&wiki.id, view)
                    .map_err(failed)?;
                state = None;
                window = self.window_held(wiki, &tip, MAX_WINDOW, hold)?;
            }
        }
        let infos = window.infos();
        // A block that is not in the view's log is not indexed by building it: there is nothing to reach.
        if let Some(block) = until
            && !infos.iter().any(|b| b.block_id == block)
        {
            return Ok(Reach::NotInLog);
        }
        let head = state.as_ref().map(|s| s.head_number);
        let mut batch: Vec<BlockWrites> = Vec::new();
        let mut first_indexed: Option<bool> = None;
        let mut visited = 0_usize;
        for (at, info) in infos.iter().enumerate().rev() {
            if head.is_some_and(|head| info.number <= head) {
                continue;
            }
            if budget.stop() || visited >= MAX_INDEXED_BLOCKS {
                break;
            }
            visited += 1;
            let rows = if window.is_opaque(at) {
                rows_of(&Touches::TooLarge, true)
            } else {
                let (touches, fetched) = self.touches_of(wiki, &info.block_id)?;
                if fetched {
                    budget.spent_one();
                }
                rows_of(&touches, false)
            };
            first_indexed.get_or_insert(info.first);
            batch.push(BlockWrites {
                number: info.number,
                block: info.block_id.clone(),
                proposal: info.proposal.clone(),
                writes: rows,
            });
            if until == Some(info.block_id.as_str()) {
                break;
            }
        }
        let Some(newest) = batch.last() else {
            return Ok(Reach::InLog);
        };
        let next = WritesState {
            head_number: newest.number,
            head_block: newest.block.clone(),
            floor_number: state
                .as_ref()
                .map_or_else(|| batch.first().map_or(0, |b| b.number), |s| s.floor_number),
            genesis: state
                .as_ref()
                .map_or_else(|| first_indexed.unwrap_or(false), |s| s.genesis),
        };
        match self
            .derived
            .append_writes(&wiki.id, view, &batch, &next, generation)
            .map_err(failed)?
        {
            Appended::Done => {}
            // Dropped since the build began: nothing was added, and the index is not built (a later reader builds it).
            Appended::Dropped => return Ok(Reach::InLog),
        }
        if budget.request {
            self.request_advances.fetch_add(1, Ordering::Relaxed);
        }
        Ok(Reach::InLog)
    }

    /// **Build the default view's index toward its tip now**, within a request's budget ([`DEFAULT_REQUEST_BUDGET`]; one call is
    /// one request, **per call**): what a hide does when the server has no background build to ask
    /// ([`ReadService::warm_writes_after_hide`]), and what a test does to have the index built. Nothing depends on it: a read that
    /// finds the index short of what it needs builds on, and answers *not checked yet* for what it could not reach. `true` when the
    /// index reaches the view's tip.
    pub fn warm_writes(&self, wiki: &WikiRecord) -> Result<bool, ReadError> {
        self.warm_writes_of(wiki, wiki.default_view.as_str(), &self.request_budget())
    }

    /// [`ReadService::warm_writes`] for any view: what a list of **later** writers needs, because it is an answer only when the index
    /// reaches the view's tip (an index built until the block asked about knows nothing of the blocks after it).
    pub(super) fn warm_writes_of(
        &self,
        wiki: &WikiRecord,
        view: &str,
        budget: &RequestBudget,
    ) -> Result<bool, ReadError> {
        budget.with(|budget| self.warm_writes_within(wiki, view, budget))
    }

    fn warm_writes_within(
        &self,
        wiki: &WikiRecord,
        view: &str,
        budget: &mut Budget,
    ) -> Result<bool, ReadError> {
        let mut before = None;
        loop {
            self.advance_writes(wiki, view, None, Hold::Keep, budget)?;
            let tip = self.tip(wiki, Some(view))?;
            let state = self
                .derived
                .writes_state(&wiki.id, view)
                .map_err(|e| self.writes_failed(e))?;
            let head = state.map(|s| (s.head_number, s.head_block));
            if head
                .as_ref()
                .is_some_and(|(_, block)| block == tip.block_id())
            {
                return Ok(true);
            }
            // Out of budget, or a call that went no further: what is built is kept, and the next call goes on.
            if budget.stop() || head == before {
                return Ok(false);
            }
            before = head;
        }
    }

    // ------------------------------------------------------------------------------------ the background build

    /// **The budget of one request** for the part it has in building the index: [`DEFAULT_REQUEST_BUDGET`] (or what
    /// [`ReadService::with_writes_budget`] gave) from now, over [`MAX_INDEXED_SHOWS`] blocks that cost a process. Made where a request
    /// enters the read service and handed to every lookup it makes ([`RequestBudget`]); never made inside a lookup that is one of many.
    pub(super) fn request_budget(&self) -> RequestBudget {
        RequestBudget(Mutex::new(Budget {
            until: Some((self.monotonic)() + self.writes_budget),
            clock: Arc::clone(&self.monotonic),
            shows_left: MAX_INDEXED_SHOWS,
            fetched: 0,
            request: true,
        }))
    }

    /// A moderator's content hide was recorded: **the writes index is to be built** (correction P-5). With a background build
    /// ([`ReadService::with_writes_background`]) that is asked for and nothing waits for it; without one (a service no server runs)
    /// it is [`ReadService::warm_writes`] now, within a request's budget, best effort.
    pub fn warm_writes_after_hide(&self, wiki: &WikiRecord) {
        match &self.writes_background {
            Some(background) => background.ask(wiki, AskedFor::Hide),
            None => {
                let _ = self.warm_writes(wiki);
            }
        }
    }

    /// The server is starting: **ask for the background build of the wiki if a content hide is in force**, so that a restart
    /// continues where the last run stopped (the index is derived and kept). `true` when it was asked for. Nothing is asked for a
    /// wiki in which nothing is hidden, and nothing when this service has no background build.
    pub fn resume_writes_background(&self, wiki: &WikiRecord) -> Result<bool, ReadError> {
        let (Some(background), Some(tombstones)) = (&self.writes_background, &self.tombstones)
        else {
            return Ok(false);
        };
        if tombstones
            .content_hide_in_force(&wiki.id)
            .map_err(|e| self.tombstone_failed(e))?
            .is_none()
        {
            return Ok(false);
        }
        background.ask(wiki, AskedFor::Hide);
        Ok(true)
    }

    /// A reader could not be answered because the index is short, while a hide is in force: say so to the background build (it may
    /// be that nothing is building) and give the reader its answer, *not checked yet*.
    fn starved(&self, wiki: &WikiRecord) -> ReadError {
        if let Some(background) = &self.writes_background {
            background.ask(wiki, AskedFor::Reader);
        }
        ReadError::NotCheckedYet
    }

    /// How many times a **request** built something of an index, since the service was made: what the background build yields to (a
    /// reader that is building is doing its work, and a step of its own would only make the reader wait for the view's lock).
    pub fn reader_advances(&self) -> u64 {
        self.request_advances.load(Ordering::Relaxed)
    }

    /// **One step of the background build of `wiki`'s writes index** (correction P-5): the first view, the default one first (at most
    /// [`MAX_BACKGROUND_VIEWS`]), whose index does not reach its tip is built [`BACKGROUND_STEP_SHOWS`] blocks that cost a process
    /// further, or for [`BACKGROUND_STEP_TIME`] (and never less than one block). The step holds the view's lock for itself only: a
    /// reader that needs the same view waits for one step, not for the build.
    ///
    /// [`BackgroundStep::Idle`] when no change of the wiki has its content hidden, or every view's index reaches its tip: the caller
    /// stops asking. [`BackgroundStep::Failed`] builds nothing wrong, and the next step retries. **Never panics, never poisons**: a
    /// step touches the same locks a reader does, and each lock is taken the way a reader takes it.
    pub fn background_step(&self, wiki: &WikiRecord) -> BackgroundStep {
        match self.background_step_inner(wiki) {
            Ok(step) => step,
            Err(e) => BackgroundStep::Failed(e),
        }
    }

    fn background_step_inner(&self, wiki: &WikiRecord) -> Result<BackgroundStep, ReadError> {
        let Some(tombstones) = &self.tombstones else {
            return Ok(BackgroundStep::Idle);
        };
        if tombstones
            .content_hide_in_force(&wiki.id)
            .map_err(|e| self.tombstone_failed(e))?
            .is_none()
        {
            return Ok(BackgroundStep::Idle);
        }
        let views = self.open_views_held(wiki)?;
        for view in views.iter().take(MAX_BACKGROUND_VIEWS) {
            let tip = match self.tip(wiki, Some(view)) {
                Ok(tip) => tip,
                // A view that closed since the list was read has nothing to build.
                Err(ReadError::NoSuchView(_)) => continue,
                Err(e) => return Err(e),
            };
            let head = |this: &Self| -> Result<Option<(u64, String)>, ReadError> {
                Ok(this
                    .derived
                    .writes_state(&wiki.id, view)
                    .map_err(|e| this.writes_failed(e))?
                    .map(|s| (s.head_number, s.head_block)))
            };
            let before = head(self)?;
            if before
                .as_ref()
                .is_some_and(|(_, block)| block == tip.block_id())
            {
                continue;
            }
            let mut budget = Budget {
                until: Some((self.monotonic)() + BACKGROUND_STEP_TIME),
                clock: Arc::clone(&self.monotonic),
                shows_left: BACKGROUND_STEP_SHOWS,
                fetched: 0,
                request: false,
            };
            // The logs of the other views are read once and not held (P-2): they would put out the default view's.
            let hold = if *view == wiki.default_view {
                Hold::Keep
            } else {
                Hold::Once
            };
            self.advance_writes(wiki, view, None, hold, &mut budget)?;
            // A view that went no further (a log that is read no further back, a view that moved while it was asked) is left to the
            // readers: stopping here is what keeps the build from asking again for ever.
            if head(self)? != before {
                return Ok(BackgroundStep::Progress);
            }
        }
        Ok(BackgroundStep::Idle)
    }

    /// **The block before `block_id` in `view`'s log**, by the writes index: one indexed lookup, whatever the length of the history
    /// (the moderation form's reading of a change's own diff does not walk the log to find a parent). [`ParentBlock::Unsettled`] when
    /// the index does not reach the block (see the module's account).
    pub(super) fn indexed_parent(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        budget: &RequestBudget,
    ) -> Result<ParentBlock, ReadError> {
        let view = view.unwrap_or(&wiki.default_view);
        let Some(at) = self.indexed_through(wiki, view, block_id, budget)? else {
            return Ok(ParentBlock::Unsettled);
        };
        let Some(before) = at.number.checked_sub(1).filter(|n| *n > 0) else {
            return Ok(ParentBlock::First);
        };
        Ok(
            match self
                .derived
                .indexed_block_at(&wiki.id, view, before)
                .map_err(|e| self.writes_failed(e))?
            {
                Some(parent) => ParentBlock::Block(parent.block),
                None => ParentBlock::Unsettled,
            },
        )
    }

    /// **The changes after the block `block_id` that wrote `path`** (a write or a pure rename into it), oldest first, at most `limit`
    /// (and at most [`MAX_LATER_WRITES`]), with whether there are more: what the moderation form lists beside its sentence about a
    /// tip, because hiding a change hides the change and not the text a later one wrote. One indexed query, after the index has been
    /// built on to the view's tip (within the bound of [`ReadService::warm_writes`]: a list of the changes *after* a block is an
    /// answer only when the index holds them). `settled` is false when the index does not reach the block **or the tip** yet (see the
    /// module's account): the list is then no answer.
    ///
    /// **The list is never silently short** (correction L-6):
    ///
    /// - A file **moved away** with its bytes after the block (a pure rename: a `carried` row of the new path) is followed to the new
    ///   path, and the changes that wrote *it* after the move (and the move itself, which carries the text) are listed, each with its
    ///   own [`LaterWriter::path`]. The moves of a chain are followed up to [`MAX_FOLLOWED_MOVES`] in all; past it
    ///   [`LaterWriters::moves_unfollowed`] says so. (A rename that **also edits** the file in the same block is a write of the new
    ///   path and a removal of the old one, and nothing in the index links them: it is not followed.)
    /// - The changes after the block that **may have written** the path or any (an operation whose path was not resolved, a change too
    ///   large to list, a kind of change whose effect is not listed) cannot be told to have or not, and are returned as
    ///   [`LaterWriters::untraced`] for the moderation form to say so.
    ///
    /// **One call is one request, with a budget of its own** ([`DEFAULT_REQUEST_BUDGET`]): *per call*, whatever the caller asks next.
    /// The one request that asks it for many paths ([`ReadService::later_holders`]) hands it the budget of the whole request
    /// instead.
    pub fn later_writers(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
        limit: usize,
    ) -> Result<LaterWriters, ReadError> {
        self.later_writers_within(wiki, view, block_id, path, limit, &self.request_budget())
    }

    /// [`ReadService::later_writers`] drawing from the budget of the request that asks.
    pub(super) fn later_writers_within(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
        limit: usize,
        budget: &RequestBudget,
    ) -> Result<LaterWriters, ReadError> {
        let view = view.unwrap_or(&wiki.default_view);
        let at = match self.indexed_through(wiki, view, block_id, budget)? {
            Some(at) if self.warm_writes_of(wiki, view, budget)? => at,
            _ => {
                return Ok(LaterWriters {
                    writers: Vec::new(),
                    more: false,
                    settled: false,
                    untraced: Vec::new(),
                    untraced_at_least: false,
                    moves_unfollowed: false,
                });
            }
        };
        let limit = limit.clamp(1, MAX_LATER_WRITES);
        // The paths the text may be at after the block, each with the number of the block it is looked at from (exclusive): the path
        // asked about from the block itself, a path the file was moved to from the block before the move (the move carries the text).
        let mut paths: Vec<(String, u64)> = vec![(path.to_owned(), at.number)];
        let (mut writers, mut more, mut moves_unfollowed) = (Vec::new(), false, false);
        let (mut untraced, mut untraced_at_least) = (BTreeSet::new(), false);
        let mut next = 0;
        while let Some((here, after)) = paths.get(next).cloned() {
            next += 1;
            let mut found = self
                .derived
                .later_writes(&wiki.id, view, &here, after, limit)
                .map_err(|e| self.writes_failed(e))?;
            more |= found.len() > limit;
            found.truncate(limit);
            writers.extend(found.into_iter().map(
                |LaterWrite {
                     number,
                     block,
                     kind,
                 }| LaterWriter {
                    block,
                    number,
                    carried: matches!(kind, WriteKind::Carried { .. }),
                    path: here.clone(),
                },
            ));
            let maybes = self
                .derived
                .later_maybes(&wiki.id, view, &here, after)
                .map_err(|e| self.writes_failed(e))?;
            untraced_at_least |= maybes.len() > MAX_MAYBE_WRITERS;
            untraced.extend(maybes.into_iter().take(MAX_MAYBE_WRITERS).map(|m| m.number));
            let moved = self
                .derived
                .later_moves(&wiki.id, view, &here, after, MAX_FOLLOWED_MOVES)
                .map_err(|e| self.writes_failed(e))?;
            moves_unfollowed |= moved.len() > MAX_FOLLOWED_MOVES;
            for to in moved.into_iter().take(MAX_FOLLOWED_MOVES) {
                if paths.iter().any(|(known, _)| *known == to.to) {
                    continue;
                }
                if paths.len() > MAX_FOLLOWED_MOVES {
                    moves_unfollowed = true;
                } else {
                    paths.push((to.to, to.number.saturating_sub(1)));
                }
            }
        }
        // Oldest first over every path; one entry for a block and a path.
        writers.sort_by(|a: &LaterWriter, b: &LaterWriter| {
            a.number.cmp(&b.number).then_with(|| a.path.cmp(&b.path))
        });
        writers.dedup_by(|a, b| a.number == b.number && a.path == b.path);
        more |= writers.len() > limit;
        writers.truncate(limit);
        // What is listed was read; only what could not be told is untraced.
        for writer in &writers {
            untraced.remove(&writer.number);
        }
        Ok(LaterWriters {
            writers,
            more,
            settled: true,
            untraced: untraced.into_iter().collect(),
            untraced_at_least,
            moves_unfollowed,
        })
    }
}

#[cfg(test)]
pub(super) mod tests;
