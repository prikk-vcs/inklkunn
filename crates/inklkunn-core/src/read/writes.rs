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
//! **Any case that cannot be settled hides.** No bound, no cap and no failure to read ever answers *not hidden*:
//!
//! - a change that **may have written** P, and that the index cannot say did not (an operation whose path prikk could not resolve,
//!   an operation of a kind this reading does not know, a change too large to list), is counted among the writers of P and of
//!   every path after it: the text is hidden when it **or** the change before it that certainly wrote P is. That is exact, not
//!   merely safe: if neither is hidden, whichever wrote the text is not;
//! - a block whose effect on the files its operations do not list (a merge, a repair or an import block, or prikk's own rollback)
//!   is **opaque**: a text that passes through one cannot be attributed, and is hidden while any change of the wiki is;
//! - a text that is there while the index knows nobody who wrote it (a block the index does not hold, a path the index has no row
//!   for, a rename chain longer than [`MAX_RENAME_HOPS`], a log too long to be indexed, an index that is not built that far yet)
//!   is **unattributed**, and hidden in the same way.
//!
//! Such a text is answered with the mark of the newest hiding of a content in force in the wiki (a hide that is in force
//! is what makes the question worth asking), because a tombstone must say something, and nothing else of the text is given.
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
//! first, **by the first reader that needs it** and no one else (a wiki in which nothing has its content hidden pays one query for
//! a read, and builds nothing), kept as the view's log grows (a new block costs one `show`), and built again after
//! [`ReadService::clear_derived`]. **A build is bounded per call**: at most [`MAX_INDEXED_SHOWS`] blocks that cost a process
//! (blocks the service already holds cost none), so a first request on a long history does not wait for all of it: it indexes what
//! it may and answers *hidden* for what it could not reach (**never an error, and never the text**); the requests after it go on
//! from where it stopped, and each makes the next one cheaper. Reading a view's log further back than [`MAX_WINDOW`] blocks is
//! not done: the oldest blocks of a longer history are unindexed, and a text written there is unattributed (hidden while any
//! change is hidden).
//!
//! **Every view is read under the same rule.** The index is per view (a view's log orders its blocks), built when a read names the
//! view, and a read of a block that is not in the view's log is unattributed.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_store::{
    BlockWrites, IndexedBlock, LastWrite, LaterWrite, MAX_LATER_WRITES, PathWrite, StoreError,
    WikiId, WikiRecord, WriteKind, WritesState,
};

use super::history::{MAX_WINDOW, Touch, TouchKind, Touches};
use super::{Cause, ReadError, ReadService};
use crate::tombstone::Mark;

/// The most blocks one call of the index's build indexes that cost a `show` (a process) each. A block the service holds already
/// costs none.
pub const MAX_INDEXED_SHOWS: usize = 64;
/// The most calls of the index's build [`ReadService::warm_writes`] makes.
pub const MAX_WARM_ROUNDS: usize = 4;
/// The most blocks one call indexes in all, held or not (a bound on the work of one request).
const MAX_INDEXED_BLOCKS: usize = 5_000;
/// The most renames followed back from a path to the change that wrote its text. A longer chain is unattributed.
pub const MAX_RENAME_HOPS: usize = 32;

/// What the index says of the text of a path at a block, for the reader.
#[derive(Debug, Clone)]
pub(super) enum Authorship {
    /// No change of the wiki has its content hidden: nothing is asked of the index.
    NothingHidden,
    /// The text is hidden by this hiding: the change that wrote it has its content hidden.
    Hidden(Box<Mark>),
    /// The text cannot be attributed (see the module's account) and is hidden, with the mark of the newest hiding in force.
    Unattributed(Box<Mark>),
    /// The text is no hidden change's.
    Shown {
        /// The index names a change that wrote the text (it is not a path nobody is recorded as having written, or one removed).
        written: bool,
        /// What a text that turns out to have no writer is hidden with.
        unattributed: Box<Mark>,
    },
}

/// What a reader of paths at one block is given by [`ReadService::reading`].
pub(super) enum Reading {
    /// No change of the wiki has its content hidden.
    NothingHidden,
    /// The block is not in the view's index, so no path can be attributed at it: every text there is hidden with this mark.
    Unattributed(Box<Mark>),
    /// The block's place in the view's index.
    Ready(Ready),
}

/// The block of a view's index a reader asks the writers of paths at, and what it needs to answer for each of them.
pub(super) struct Ready {
    view: String,
    at: IndexedBlock,
    state: WritesState,
    unattributed: Box<Mark>,
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

    fn tombstone_failed(&self, e: crate::tombstone::Unreadable) -> ReadError {
        self.store_failures.fetch_add(1, Ordering::Relaxed);
        ReadError::CouldNotBeReadNow(Cause::Store(StoreError::Backend(e.0)))
    }

    /// **Whose text is the text of `path` at the block `block_id` of `view`** (the default view for `None`), as the module's account
    /// says. The one function [`ReadService::hidden_text`] and the reads of a file are decided by.
    pub(super) fn authorship(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
    ) -> Result<Authorship, ReadError> {
        Ok(match self.reading(wiki, view, block_id)? {
            Reading::NothingHidden => Authorship::NothingHidden,
            Reading::Unattributed(mark) => Authorship::Unattributed(mark),
            Reading::Ready(ready) => self.authorship_in(wiki, &ready, path)?,
        })
    }

    /// **What a reader of many paths at one block asks once**: whether any change of the wiki has its content hidden (one query), and
    /// the block's place in the view's index (built toward it within the bound of a call). [`ReadService::authorship_in`] then answers
    /// for each path.
    pub(super) fn reading(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
    ) -> Result<Reading, ReadError> {
        let Some(tombstones) = &self.tombstones else {
            return Ok(Reading::NothingHidden);
        };
        let Some(unattributed) = tombstones
            .content_hide_in_force(&wiki.id)
            .map_err(|e| self.tombstone_failed(e))?
            .map(Box::new)
        else {
            return Ok(Reading::NothingHidden);
        };
        // The view whose log orders the blocks: the one named, or the default view's. A block that is not in the default view's log
        // (a proposal's base in another view) is looked for in the wiki's other views: **the changes before a block are the
        // block's own, whichever view's log it is read in**, so the answer is the same, and a block that no view has is unattributed.
        let view = view.unwrap_or(&wiki.default_view);
        let mut found = self
            .indexed_through(wiki, view, block_id)?
            .map(|at| (view.to_owned(), at));
        if found.is_none() && view == wiki.default_view {
            for other in self.open_views(wiki)? {
                if other == wiki.default_view {
                    continue;
                }
                if let Some(at) = self.indexed_through(wiki, &other, block_id)? {
                    found = Some((other, at));
                    break;
                }
            }
        }
        let Some((view, at)) = found else {
            return Ok(Reading::Unattributed(unattributed));
        };
        let Some(state) = self
            .derived
            .writes_state(&wiki.id, &view)
            .map_err(|e| self.writes_failed(e))?
        else {
            return Ok(Reading::Unattributed(unattributed));
        };
        Ok(Reading::Ready(Ready {
            view,
            at,
            state,
            unattributed,
            hidden: RefCell::new(HashMap::new()),
        }))
    }

    /// [`ReadService::authorship`] of one path of the block [`ReadService::reading`] settled.
    pub(super) fn authorship_in(
        &self,
        wiki: &WikiRecord,
        ready: &Ready,
        path: &str,
    ) -> Result<Authorship, ReadError> {
        let Some(tombstones) = &self.tombstones else {
            return Ok(Authorship::NothingHidden);
        };
        let Ready {
            view,
            at,
            state,
            unattributed,
            hidden,
        } = ready;
        let view = view.as_str();
        let unattributed = unattributed.clone();
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
                return Ok(Authorship::Unattributed(unattributed));
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
                        return Ok(Authorship::Unattributed(unattributed));
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
                    return Ok(Authorship::Unattributed(unattributed));
                }
                // A row that is no certain one is never `last`.
                Some(LastWrite { .. }) => return Ok(Authorship::Unattributed(unattributed)),
            }
        }
        if !settled {
            // A rename chain longer than the bound.
            return Ok(Authorship::Unattributed(unattributed));
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
                        return Ok(Authorship::Unattributed(unattributed));
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
        Ok(Authorship::Shown {
            written,
            unattributed,
        })
    }

    /// The block of `view`'s index that is `block_id`, building the index toward it (within the bound of a call) when it is not
    /// there yet. `None` when the index does not reach it: not built that far, or not a block of the view. **A store that cannot
    /// answer is `None` too**: what cannot be attributed is not shown.
    fn indexed_through(
        &self,
        wiki: &WikiRecord,
        view: &str,
        block_id: &str,
    ) -> Result<Option<IndexedBlock>, ReadError> {
        let held = |this: &Self| match this.derived.indexed_block(&wiki.id, view, block_id) {
            Ok(found) => found,
            Err(_) => {
                this.store_failures.fetch_add(1, Ordering::Relaxed);
                None
            }
        };
        if let Some(found) = held(self) {
            return Ok(Some(found));
        }
        self.advance_writes(wiki, view, Some(block_id))?;
        Ok(held(self))
    }

    fn writes_lock(&self, wiki: &WikiId, view: &str) -> Arc<Mutex<()>> {
        let mut locks = self
            .writes_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry((wiki.clone(), view.to_owned())).or_default())
    }

    /// **Index the blocks of `view`'s log that the index does not hold, oldest first**, until the block `until` is in it (when one
    /// is named) or the bound of a call is reached: [`MAX_INDEXED_SHOWS`] blocks that cost a `show`. One reader builds at a time;
    /// the next finds what it built. An index that is not of the log read now (its newest block is not the log's block of that
    /// number) is dropped and built again. **An error is prikk's or the store's**, never the index being unfinished.
    pub(super) fn advance_writes(
        &self,
        wiki: &WikiRecord,
        view: &str,
        until: Option<&str>,
    ) -> Result<(), ReadError> {
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
            return Ok(());
        }
        let tip = self.tip(wiki, Some(view))?;
        let mut state = self.derived.writes_state(&wiki.id, view).map_err(failed)?;
        // The log, back to the newest block the index holds (the whole log, as far as it is read, for an index that has none).
        let first = self.window_for(wiki, &tip, super::history::DEFAULT_WINDOW)?;
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
            self.window_for(wiki, &tip, need)?
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
                state = None;
                window = self.window_for(wiki, &tip, MAX_WINDOW)?;
            }
        }
        let infos = window.infos();
        // A block that is not in the view's log is not indexed by building it: there is nothing to reach.
        if let Some(block) = until
            && !infos.iter().any(|b| b.block_id == block)
        {
            return Ok(());
        }
        let head = state.as_ref().map(|s| s.head_number);
        let mut batch: Vec<BlockWrites> = Vec::new();
        let mut first_indexed: Option<bool> = None;
        let (mut shows, mut visited) = (0_usize, 0_usize);
        for (at, info) in infos.iter().enumerate().rev() {
            if head.is_some_and(|head| info.number <= head) {
                continue;
            }
            if shows >= MAX_INDEXED_SHOWS || visited >= MAX_INDEXED_BLOCKS {
                break;
            }
            visited += 1;
            let rows = if window.is_opaque(at) {
                rows_of(&Touches::TooLarge, true)
            } else {
                let (touches, fetched) = self.touches_of(wiki, &info.block_id)?;
                shows += usize::from(fetched);
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
            return Ok(());
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
        self.derived
            .append_writes(&wiki.id, view, &batch, &next)
            .map_err(failed)
    }

    /// **Build the default view's index toward its tip now**, in at most [`MAX_WARM_ROUNDS`] calls of the bound of a call each (so at
    /// most `MAX_WARM_ROUNDS * MAX_INDEXED_SHOWS` blocks that cost a process): what a moderator's hide does after it is recorded,
    /// so that the first reads after it find the index built, in a wiki of any size a hide is likely to be made in. Nothing depends
    /// on it: a read that finds the index short of what it needs builds on, and answers *hidden* for what it could not reach. `true`
    /// when the index reaches the view's tip.
    pub fn warm_writes(&self, wiki: &WikiRecord) -> Result<bool, ReadError> {
        let view = wiki.default_view.as_str();
        for _ in 0..MAX_WARM_ROUNDS {
            self.advance_writes(wiki, view, None)?;
            let tip = self.tip(wiki, Some(view))?;
            let state = self
                .derived
                .writes_state(&wiki.id, view)
                .map_err(|e| self.writes_failed(e))?;
            if state.is_some_and(|s| s.head_block == tip.block_id()) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// **The changes after the block `block_id` that wrote `path`** (a write or a pure rename into it), oldest first, at most `limit`
    /// (and at most [`MAX_LATER_WRITES`]), with whether there are more: what the moderation form lists beside its sentence about a
    /// tip, because hiding a change hides the change and not the text a later one wrote. One indexed query. `settled` is false when
    /// the index does not reach the block yet (see the module's account): the list is then no answer.
    pub fn later_writers(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
        limit: usize,
    ) -> Result<LaterWriters, ReadError> {
        let view = view.unwrap_or(&wiki.default_view);
        let Some(at) = self.indexed_through(wiki, view, block_id)? else {
            return Ok(LaterWriters {
                writers: Vec::new(),
                more: false,
                settled: false,
            });
        };
        let limit = limit.clamp(1, MAX_LATER_WRITES);
        let mut found = self
            .derived
            .later_writes(&wiki.id, view, path, at.number, limit)
            .map_err(|e| self.writes_failed(e))?;
        let more = found.len() > limit;
        found.truncate(limit);
        Ok(LaterWriters {
            writers: found
                .into_iter()
                .map(
                    |LaterWrite {
                         number,
                         block,
                         kind,
                     }| LaterWriter {
                        block,
                        number,
                        carried: matches!(kind, WriteKind::Carried { .. }),
                    },
                )
                .collect(),
            more,
            settled: true,
        })
    }
}

#[cfg(test)]
mod tests;
