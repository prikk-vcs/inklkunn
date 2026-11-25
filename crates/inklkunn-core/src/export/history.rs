//! The history records of an export (RFC 007 D-7; handoff I-T1): each line of `history/<page id>.jsonl` is a wiki-commons
//! *history record* (XFER-7; `schemas/history-record.schema.json`), one per revision of a page, oldest first.
//!
//! ```text
//! the view's log (oldest first) ─▶ each block's `show` ─▶ the pages it created, edited, moved or removed (a page is followed
//!   through its renames, so a page since removed or renamed has its history too) ─▶ one revision per page per block
//!   ─▶ each revision's text, read through the one reader (`ReadService::text_at`, RFC 006 J-1) and never otherwise
//!   ─▶ the pages' ids (the page's own, from its frontmatter) name the files; a page's lines are linked by `parent`
//! ```
//!
//! **One revision is one block** (a block writes a page once), so a record's `rev` is the block's id, unique in the page's file
//! and the id a permalink already names. A record carries:
//!
//! - `rev`, `parent` (the record before it in the same file, `null` for the first), `at`, `type` (`create`, `edit`, `rename`
//!   with `from` and `to` as titles, `delete`, or `revert` when an accepted rollback wrote the page), `author` (the proposer, by
//!   the account's id as `users.yaml` has it; an anonymous proposer by the label; the service as a `bot` with no id), `summary`;
//! - `content`, **the page's full text at that revision** (frontmatter and body), and its `sha256`. Both are absent for a
//!   revision whose text moderation hides (`suppressed: ["content"]`, with `reasons`): **the text is read through
//!   [`ReadService::text_at`] and through nothing else**, so a text that the one reader hides is not in the export, however
//!   the hide was made (a page moved by a change that did not write it is hidden if the change that wrote it is);
//! - inklkunn's own facts under **one** `inklkunn` key, which an importer may ignore: `block`, `seq`, `kind`, `path` (and
//!   `from_path` for a rename), `proposed_by` and `accepted_by`, `imported_from`, `message_readable`, and `at_from`.
//! - **`at`** is the best time this service has for the block, and **`inklkunn.at_from` says which** (prikk's log has no time):
//!   `sealed` (the end of the acceptance, or of the system act, that made the block: the service keeps it for every block it
//!   made), `source` (an imported change's own time, from its message: the hook is [`source_time`], which I-T3 fills), or
//!   `first-seen` (when a surface first asked for the block, an upper bound, and never later than any later change's time; a block
//!   no surface has asked for is recorded as seen now, and stays so: the same export of the same tip is the same bytes). **Within a
//!   file `at` never decreases** (RFC 007 D-2 refuses an import that does): a record whose time would be before the one before it
//!   takes that one's time, and says `ordered`.
//! - **`inklkunn.withheld`**: a text that no index can attribute (it passes through a block whose effect is not listed, has more
//!   possible writers than the index lists, or was moved more often than it follows) is **not given**: the record has no `content`
//!   and no `sha256`, says the reason here (`opaque-block`, `too-many-writers`, `too-many-renames`, `no-writer-known`), and is
//!   **not** `suppressed`, which is moderation's word. Later revisions of the page are whole when they can be attributed.
//! - **`inklkunn.after_untraced`**: the block (an id) whose effect on the files is not listed (a prikk rollback, a merge, a repair)
//!   that came between this record and the one before it in the file (the oldest of them): the page's chain may not be what its
//!   records say there. No record is made up for it.
//!
//! **What the index has not settled is waited for, and never guessed**: a text the reader answers *not checked yet* is either not
//! yet (the index is short of the tip: the export builds it, a round at a time, saying so, and a round counts only when the index
//! moved) or never (the index holds the block and cannot attribute the text: nothing waits for that, and the revision is withheld).
//! A bundle is never written with a gap, nor with a text that could not be told from a hidden one.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use inklkunn_markup::path::path_to_title;
use inklkunn_prikk::storage::{Change, HistoryBlock};
use inklkunn_prikk::{Point, Storage};
use inklkunn_signer::message::{Actor, Kind, Message};
use inklkunn_store::{DerivedStore, ReasonCategory, WikiRecord};
use sha2::{Digest, Sha256};

use super::{
    ExportError, MAX_HISTORY_BLOCKS, Suppressed, Suppressions, hex, history_file_allowed, person,
};
use crate::read::{BlockText, ReadError, ReadService, Unattributable, Warmed, quoted_summary};

/// How many rounds the export waits for the index of who wrote each page's text, in all: **a round is one request's part in
/// building it (about a second) that moved the index**, after which the text is asked for again. A round that did not move it is no
/// round of these ([`INDEX_STALL_ROUNDS`]).
pub const INDEX_WAIT_ROUNDS: usize = 30;

/// How many times in a row the export asks for the index to be built and it does not move (another process may hold the build, or
/// the store is slow) before it gives up with a sentence: the index is short of the tip, so this is *not yet*, said as such.
pub const INDEX_STALL_ROUNDS: usize = 10;

/// How often a text is asked for again once the index reaches the tip and the index does not say why the text cannot be attributed
/// (a moment in which the index was being rebuilt under the reader): then the revision is withheld as `no-writer-known`.
const SETTLED_RETRIES: usize = 3;

/// The pause between two rounds of waiting for the index, unless [`HistoryInputs::pausing`] says another.
pub const DEFAULT_ROUND_PAUSE: Duration = Duration::from_millis(100);

/// The most history lines' bytes one export holds before it writes them, **as they are written** (the text of every revision is in
/// its line, escaped: a text of control characters is six times its size). A history larger than this is refused rather than cut
/// short.
pub const MAX_HISTORY_TEXT_BYTES: u64 = 1 << 30;

/// What an export says while it works (the command line prints it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportNote {
    /// The history is read to the end of the log: this many revisions of this many pages will be written, each with its text
    /// (one read of a block's page each).
    Revisions {
        /// The records.
        revisions: usize,
        /// The pages, past and present.
        pages: usize,
    },
    /// The index of who wrote each page's text is not built far enough: the export waits for it, then asks again.
    Waiting {
        /// This round (from 1).
        round: usize,
        /// The most rounds.
        rounds: usize,
    },
    /// A revision's text is **not given**: no index can attribute it, so it cannot be told from a text that moderation hides
    /// (I-T1 correction X-1). Its record says so (`inklkunn.withheld`).
    Withheld {
        /// The block.
        block: String,
        /// The page's path then.
        path: String,
        /// Why: the word the record carries.
        reason: &'static str,
    },
}

/// What the history of an export needs besides the repository: the one reader of a page's text at a block, and the record of when
/// each block was first seen. It may be given a way to say what it is doing.
pub struct HistoryInputs<'a> {
    reads: &'a ReadService,
    derived: &'a dyn DerivedStore,
    notes: Option<&'a mut dyn FnMut(ExportNote)>,
    waited: usize,
    pause: Duration,
    cap: u64,
}

impl<'a> HistoryInputs<'a> {
    /// Inputs that say nothing.
    pub fn new(reads: &'a ReadService, derived: &'a dyn DerivedStore) -> Self {
        HistoryInputs {
            reads,
            derived,
            notes: None,
            waited: 0,
            pause: DEFAULT_ROUND_PAUSE,
            cap: MAX_HISTORY_TEXT_BYTES,
        }
    }

    /// The same, saying what it does to `notes`.
    #[must_use]
    pub fn saying(mut self, notes: &'a mut dyn FnMut(ExportNote)) -> Self {
        self.notes = Some(notes);
        self
    }

    /// The same, pausing `pause` between two rounds of waiting for the index ([`DEFAULT_ROUND_PAUSE`] otherwise).
    #[must_use]
    pub fn pausing(mut self, pause: Duration) -> Self {
        self.pause = pause;
        self
    }

    /// The same, refusing a history of more than `bytes` bytes as written ([`MAX_HISTORY_TEXT_BYTES`] otherwise). A smaller bound
    /// is for a test; a larger one is no use, the bound is what one export holds in memory.
    #[must_use]
    pub fn holding_at_most(mut self, bytes: u64) -> Self {
        self.cap = bytes.min(MAX_HISTORY_TEXT_BYTES);
        self
    }

    fn say(&mut self, note: ExportNote) {
        if let Some(notes) = self.notes.as_mut() {
            notes(note);
        }
    }

    /// The read service this export reads texts through (and asks for moderation's head).
    pub(super) fn reads(&self) -> &ReadService {
        self.reads
    }
}

/// What the history of an export came to.
pub(super) struct Histories {
    /// The records of each history file, by the file's name, in the order of the file. **Held until the file is written, one file at
    /// a time** (X-8): `build` takes each out, writes it and drops it.
    pub(super) files: BTreeMap<String, Vec<Parts>>,
    /// Changes to paths that are not pages (an attachment, a knowledge file, a rule), and operations prikk could not name a path
    /// for: they belong to no page's file.
    pub(super) unattributed: usize,
    /// Accounts seen in messages: id → the names they had, in order of first sight. **Only for what is not hidden.**
    pub(super) accounts: BTreeMap<String, Vec<String>>,
    /// file → how many of its lines have a part hidden.
    pub(super) suppressed: BTreeMap<String, usize>,
    /// Pages with revisions whose file was not written (the id cannot name a file, two pages share it, or the page has none).
    pub(super) not_written: usize,
    /// Revisions whose text was withheld because no index can attribute it (`inklkunn.withheld`).
    pub(super) withheld: usize,
    /// The names of the files whose records pass a block whose effect is not listed (`inklkunn.after_untraced`), in order.
    pub(super) after_untraced: Vec<String>,
}

// ------------------------------------------------------------------------------------ following the pages

/// What one block did to one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Did {
    Create,
    Edit,
    Rename,
    Delete,
}

/// One revision of one page, before its text is read.
struct Revision {
    /// The page (an index of [`Tracker::chains`]).
    chain: usize,
    /// The block's place in the log, oldest first.
    block: usize,
    did: Did,
    /// The page's path after the block (before it, for a deletion).
    path: String,
    /// The path before, for a rename.
    from: Option<String>,
    message: Option<Message>,
    /// A summary that quotes one that is now hidden is hidden as it is (RFC 006 H-7).
    quoted_hidden: Option<ReasonCategory>,
}

/// A page, from the block that made it to the block that removed it: its path now (`None` once removed).
struct Chain {
    path: Option<String>,
}

/// What a block did to a page, gathered from its operations in order.
struct Effect {
    existed_before: bool,
    before: Option<String>,
    written: bool,
    patch: usize,
}

#[derive(Default)]
struct BlockState {
    effects: BTreeMap<usize, Effect>,
    /// Pages removed by this block, by the path they had: one removed and written again by the same block is one page.
    removed: BTreeMap<String, usize>,
    /// Pages moved out of `pages/` by this block, by the path they were moved to: **moved back by the same block, the page is the
    /// same page** (the writes index carries it the same way), not a removal and a new page. The page's own path before is kept.
    parked: BTreeMap<String, (usize, String)>,
}

/// An operation of a block, as the history reads it (prikk's own names; `read/history.rs` classifies the same ones for the pages).
enum Op {
    Create(String),
    Edit(String),
    Delete(String),
    Rename(String, String),
    /// A change of mode, or an operation whose path prikk could not resolve, or one of a kind not known: no page is followed by it.
    Other {
        kind: String,
        paths: Vec<Option<String>>,
    },
}

fn is_page(path: &str) -> bool {
    path.starts_with("pages/") && path.ends_with(".md")
}

fn classify(patch: &inklkunn_prikk::storage::Patch) -> Vec<Op> {
    patch
        .operations
        .iter()
        .map(|op| {
            let path = |i: usize| op.paths.get(i).and_then(|p| p.path.clone());
            match (op.kind.as_str(), path(0), path(1)) {
                ("create-file" | "create-symlink", Some(p), _) => Op::Create(p),
                ("edit-text" | "replace-binary", Some(p), _) => Op::Edit(p),
                ("delete-node", Some(p), _) => Op::Delete(p),
                ("rename-path", Some(from), Some(to)) => Op::Rename(from, to),
                _ => Op::Other {
                    kind: op.kind.clone(),
                    paths: op.paths.iter().map(|p| p.path.clone()).collect(),
                },
            }
        })
        .collect()
}

#[derive(Default)]
struct Tracker {
    chains: Vec<Chain>,
    alive: BTreeMap<String, usize>,
    unattributed: usize,
}

impl Tracker {
    fn push(&mut self, path: &str) -> usize {
        self.chains.push(Chain {
            path: Some(path.to_owned()),
        });
        let id = self.chains.len() - 1;
        self.alive.insert(path.to_owned(), id);
        id
    }

    /// The page at `path`, or, when the history never said how it came to be there, one that began before the log is followed
    /// from (it is then *edited*, *moved* or *removed* by what is read, and never *created*).
    fn known(&mut self, state: &mut BlockState, path: &str, patch: usize) -> usize {
        let id = match self.alive.get(path) {
            Some(id) => *id,
            None => self.push(path),
        };
        state.effects.entry(id).or_insert_with(|| Effect {
            existed_before: true,
            before: Some(path.to_owned()),
            written: false,
            patch,
        });
        if let Some(effect) = state.effects.get_mut(&id) {
            effect.patch = patch;
        }
        id
    }

    fn born(&mut self, state: &mut BlockState, path: &str, patch: usize) {
        let id = self.push(path);
        state.effects.insert(
            id,
            Effect {
                existed_before: false,
                before: None,
                written: true,
                patch,
            },
        );
    }

    fn mark_written(state: &mut BlockState, id: usize) {
        if let Some(effect) = state.effects.get_mut(&id) {
            effect.written = true;
        }
    }

    /// The pages under a directory `dir` (a rename or a removal of a directory moves or removes them all).
    fn under(&self, dir: &str) -> Vec<(String, usize)> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        self.alive
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
            .map(|(path, id)| (path.clone(), *id))
            .collect()
    }

    fn remove(&mut self, state: &mut BlockState, path: &str, patch: usize) {
        let id = self.known(state, path, patch);
        self.alive.remove(path);
        if let Some(chain) = self.chains.get_mut(id) {
            chain.path = None;
        }
        state.removed.insert(path.to_owned(), id);
    }

    fn apply(&mut self, state: &mut BlockState, op: Op, patch: usize) {
        match op {
            Op::Create(path) if is_page(&path) => {
                if let Some(id) = state.removed.remove(&path) {
                    if let Some(chain) = self.chains.get_mut(id) {
                        chain.path = Some(path.clone());
                    }
                    self.alive.insert(path, id);
                    Self::mark_written(state, id);
                } else if self.alive.contains_key(&path) {
                    let id = self.known(state, &path, patch);
                    Self::mark_written(state, id);
                } else {
                    self.born(state, &path, patch);
                }
            }
            Op::Edit(path) if is_page(&path) => {
                let id = self.known(state, &path, patch);
                Self::mark_written(state, id);
            }
            Op::Delete(path) if is_page(&path) => self.remove(state, &path, patch),
            Op::Delete(dir) => {
                let pages = self.under(&dir);
                if pages.is_empty() {
                    self.unattributed += 1;
                }
                for (path, _) in pages {
                    self.remove(state, &path, patch);
                }
            }
            Op::Rename(from, to) => {
                let moves: Vec<(String, String)> = if is_page(&from) {
                    vec![(from.clone(), to.clone())]
                } else {
                    self.under(&from)
                        .into_iter()
                        .map(|(path, _)| {
                            let rest = path.get(from.trim_end_matches('/').len()..);
                            (
                                path.clone(),
                                format!("{}{}", to.trim_end_matches('/'), rest.unwrap_or_default()),
                            )
                        })
                        .collect()
                };
                if moves.is_empty() {
                    if let Some((id, origin)) = state.parked.remove(&from) {
                        if is_page(&to) {
                            // A page this block moved out of the pages and now moves back (X-4): the same page, carried.
                            state.removed.remove(&origin);
                            if let Some(chain) = self.chains.get_mut(id) {
                                chain.path = Some(to.clone());
                            }
                            self.alive.insert(to, id);
                        } else {
                            // Moved on, still out of the pages.
                            state.parked.insert(to, (id, origin));
                        }
                    } else if is_page(&to) {
                        // A file that was not a page, moved into the pages: the page is new.
                        self.born(state, &to, patch);
                    } else {
                        self.unattributed += 1;
                    }
                }
                for (a, b) in moves {
                    let id = self.known(state, &a, patch);
                    self.alive.remove(&a);
                    if is_page(&b) {
                        if let Some(chain) = self.chains.get_mut(id) {
                            chain.path = Some(b.clone());
                        }
                        self.alive.insert(b, id);
                    } else {
                        // Moved out of the pages: it is no page now (unless this block moves it back).
                        if let Some(chain) = self.chains.get_mut(id) {
                            chain.path = None;
                        }
                        state.parked.insert(b, (id, a.clone()));
                        state.removed.insert(a, id);
                    }
                }
            }
            // Not a page: counted, as a change that belongs to no page's file.
            Op::Create(_) | Op::Edit(_) => self.unattributed += 1,
            Op::Other { kind, paths } => {
                // A change of mode changes no text, so a page's own is nothing; anything else that names no page, or that is not
                // known, is counted (it may have written one).
                let counted = paths
                    .iter()
                    .filter(|path| kind != "change-perm" || !path.as_deref().is_some_and(is_page))
                    .count();
                self.unattributed += counted.max(usize::from(paths.is_empty()));
            }
        }
    }
}

/// The messages of a block's patches, by the patch's place in the change.
fn messages_of(block: &HistoryBlock, change: &Change) -> Vec<Option<Message>> {
    change
        .patches
        .iter()
        .map(|patch| {
            block
                .patch_messages
                .iter()
                .find(|m| m.patch_id == patch.patch_id)
                .and_then(|m| Message::parse(&m.message).ok())
        })
        .collect()
}

// ------------------------------------------------------------------------------------ the text of a revision

/// What a revision's text came to.
enum Text {
    /// No text belongs to it (a removal).
    None,
    /// The page's full text.
    Given(String),
    /// Moderation hides it (the reason is in the record's `hidden` part).
    Hidden,
    /// No index can attribute it, so it cannot be told from a text that moderation hides: **not given** (`inklkunn.withheld`).
    Withheld(Unattributable),
}

fn unreadable(path: &str, block: &str, why: &str) -> ExportError {
    ExportError::Io(format!(
        "the text of `{path}` at block {block} could not be read for the history ({why}); nothing was written"
    ))
}

/// What reading a revision's text gave.
enum Got {
    /// The reader's answer.
    Read(BlockText),
    /// The reader cannot attribute the text and the index says it never will.
    Never(Unattributable),
}

/// The text of `path` at `block`, through the one reader. **A text the reader cannot check yet is *not yet* or *never*, and the
/// two are told apart** (I-T1 correction X-1):
///
/// - **never**: the index holds the block and says why it cannot attribute the text ([`ReadService::unattributable_reason`]): an
///   opaque block, more possible writers than it lists, more renames than it follows. Nothing waits for that, and the text is
///   **not given**; the caller withholds the revision, and goes on.
/// - **not yet**: the index is short of the tip. It is built a round at a time, saying so; **a round counts only when the index
///   moved** ([`INDEX_WAIT_ROUNDS`] of them in all), and there is a pause between rounds; an index that does not move at all is
///   given up on after [`INDEX_STALL_ROUNDS`]. Then the export fails with a sentence that says the index is not built yet, **never a
///   gap and never a guess**.
///
/// An index that cannot be built (prikk or the store failing) fails the export with a sentence that names what failed.
fn read_text(
    inputs: &mut HistoryInputs<'_>,
    wiki: &WikiRecord,
    view: &str,
    block: &str,
    path: &str,
) -> Result<Got, ExportError> {
    let (mut stalled, mut settled) = (0_usize, 0_usize);
    loop {
        match inputs.reads.text_at(wiki, Some(view), block, path) {
            Ok(text) => return Ok(Got::Read(text)),
            Err(ReadError::NotCheckedYet) => {}
            Err(e) => return Err(unreadable(path, block, &e.to_string())),
        }
        // Never, if the index says why (a final answer: the rows up to a block never change).
        match inputs
            .reads
            .unattributable_reason(wiki, Some(view), block, path)
        {
            Ok(Some(reason)) => return Ok(Got::Never(reason)),
            Ok(None) => {}
            Err(e) => {
                return Err(unreadable(
                    path,
                    block,
                    &format!("why it cannot be attributed could not be read: {e}"),
                ));
            }
        }
        // Not yet, or a moment in which the index was moved under the reader.
        match inputs.reads.warm_writes_progress(wiki) {
            Err(e) => {
                return Err(ExportError::Io(format!(
                    "the index of who wrote each page's text could not be built ({e}), so the text of `{path}` at block {block} cannot be told from one that moderation hides; nothing was written"
                )));
            }
            Ok(Warmed::AtTip) => {
                settled += 1;
                if settled > SETTLED_RETRIES {
                    return Ok(Got::Never(Unattributable::NoWriterKnown));
                }
            }
            Ok(Warmed::Short { advanced }) => {
                if advanced {
                    if inputs.waited >= INDEX_WAIT_ROUNDS {
                        return Err(ExportError::IndexNotSettled {
                            rounds: INDEX_WAIT_ROUNDS,
                        });
                    }
                    inputs.waited += 1;
                    inputs.say(ExportNote::Waiting {
                        round: inputs.waited,
                        rounds: INDEX_WAIT_ROUNDS,
                    });
                } else {
                    stalled += 1;
                    if stalled > INDEX_STALL_ROUNDS {
                        return Err(ExportError::IndexNotSettled {
                            rounds: inputs.waited,
                        });
                    }
                }
                if !inputs.pause.is_zero() {
                    std::thread::sleep(inputs.pause);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------ the time of a record

/// Where a record's `at` comes from (`inklkunn.at_from`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AtFrom {
    /// The end of the acceptance, or of the system act, that made the block.
    Sealed,
    /// The imported change's own time, from its message.
    Source,
    /// When a surface of the service first asked for the block (an upper bound).
    FirstSeen,
    /// The time of the record before it in the file, because the time it would have had is before that.
    Ordered,
}

impl AtFrom {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            AtFrom::Sealed => "sealed",
            AtFrom::Source => "source",
            AtFrom::FirstSeen => "first-seen",
            AtFrom::Ordered => "ordered",
        }
    }
}

/// **The time a message says its change was made at, for an imported change** (I-T1 correction X-2 (b); RFC 007 D-3). The message
/// type has no such trailer yet: I-T3 adds it, and this is where it is read. Until then there is none, and a block is timed by its
/// seal or by when it was first seen.
pub(super) fn source_time(_message: &Message) -> Option<i64> {
    None
}

/// **The best time for a block, in this order** (X-2): when its act sealed (the service keeps it for every block it made), then an
/// imported change's own time, then when the block was first seen. `None` only when none is known.
pub(super) fn best_time(
    sealed: Option<i64>,
    source: Option<i64>,
    seen: Option<i64>,
) -> Option<(i64, AtFrom)> {
    sealed
        .map(|at| (at, AtFrom::Sealed))
        .or(source.map(|at| (at, AtFrom::Source)))
        .or(seen.map(|at| (at, AtFrom::FirstSeen)))
}

// ------------------------------------------------------------------------------------ the records

/// What names a page's history file: its id, or, for a page that has none, its path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Id(String),
    Path(String),
}

/// The longest a history file's name may be, in bytes (a file system's component, less `.jsonl`).
const MAX_STEM_BYTES: usize = 255 - ".jsonl".len();

/// The name of the history file of a page with no id (XFER-7: *or by its path when no identifier exists*): **`~` and the page's
/// path under `pages/`, without `.md`, every byte but `a-z 0-9 . _ -` written as `%` and two upper-case hex digits, the upper-case
/// letters of the path included** (`pages/Docs/Deep Page.md` is `~%44ocs%2F%44eep%20%50age`). The `~` is what no page id can start
/// with (an id is letters, digits, `.`, `_` and `-`), so a path's name is never an id's; the coding is one to one, so two paths
/// never share a file; and, since the only letters left unescaped are lower-case and the escapes' hex digits are always upper-case,
/// **two names that are not one never become one when their case is ignored** (a bundle that is unpacked on a file system that does
/// not tell `Ab` from `aB` keeps both files). `None` when the name would be longer than a file system's name may be.
fn path_file_name(path: &str) -> Option<String> {
    let title = path.strip_prefix("pages/")?.strip_suffix(".md")?;
    let mut name = String::from("~");
    for byte in title.bytes() {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        {
            name.push(char::from(byte));
        } else {
            name.push_str(&format!("%{byte:02X}"));
        }
    }
    (name.len() <= MAX_STEM_BYTES).then_some(name)
}

fn author_json(actor: &Actor) -> serde_json::Value {
    match actor {
        Actor::Account { id, name } => {
            serde_json::json!({"id": id.as_str(), "name": name.as_str(), "kind": "person"})
        }
        Actor::Anonymous(p) => serde_json::json!({"anonymous": true, "label": p.as_str()}),
        Actor::System(label) => serde_json::json!({"name": label.as_str(), "kind": "bot"}),
    }
}

fn person_of(actor: &Actor) -> serde_json::Value {
    let mut p = person("", actor);
    if let Some(members) = p.as_object_mut() {
        members.remove("role");
    }
    p
}

/// A record, with what links it to the file it is in. **Held until its file is written** (X-8).
pub(super) struct Parts {
    chain: usize,
    /// The block's place in the log, oldest first.
    block: usize,
    rev: String,
    did: Did,
    text: Text,
    hidden: Suppressed,
    message: Option<Message>,
    path: String,
    from: Option<String>,
    seq: u64,
    at: i64,
    at_from: AtFrom,
    revert: bool,
    /// The record before it in its file (`None` for the first), set when the file is made.
    parent: Option<String>,
    /// The oldest block whose effect is not listed between this record and the one before it (X-3), set when the file is made.
    untraced: Option<String>,
}

impl Parts {
    fn type_name(&self) -> &'static str {
        match self.did {
            Did::Create | Did::Edit if self.revert => "revert",
            Did::Create => "create",
            Did::Edit => "edit",
            Did::Rename => "rename",
            Did::Delete => "delete",
        }
    }

    /// This record's line, as it is written (no line feed).
    pub(super) fn line(&self) -> String {
        self.json().to_string()
    }

    /// Does the record carry a part moderation hides?
    pub(super) fn is_hidden(&self) -> bool {
        !self.hidden.is_empty()
    }

    fn json(&self) -> serde_json::Value {
        let mut inklkunn = serde_json::Map::new();
        inklkunn.insert("block".to_owned(), self.rev.clone().into());
        inklkunn.insert("seq".to_owned(), self.seq.into());
        inklkunn.insert(
            "kind".to_owned(),
            self.message
                .as_ref()
                .map_or(serde_json::Value::Null, |m| m.kind().as_str().into()),
        );
        inklkunn.insert("path".to_owned(), self.path.clone().into());
        if let Some(from) = &self.from {
            inklkunn.insert("from_path".to_owned(), from.clone().into());
        }
        inklkunn.insert(
            "imported_from".to_owned(),
            self.message
                .as_ref()
                .and_then(|m| m.imported_from())
                .map_or(serde_json::Value::Null, |s| s.as_str().into()),
        );
        inklkunn.insert("message_readable".to_owned(), self.message.is_some().into());
        inklkunn.insert("at_from".to_owned(), self.at_from.as_str().into());
        if let Text::Withheld(reason) = &self.text {
            inklkunn.insert("withheld".to_owned(), reason.as_str().into());
        }
        if let Some(block) = &self.untraced {
            inklkunn.insert("after_untraced".to_owned(), block.clone().into());
        }
        let mut line = serde_json::json!({
            "rev": self.rev,
            "parent": self.parent,
            "at": inklkunn_store::record::rfc3339(self.at),
            "type": self.type_name(),
        });
        let Some(members) = line.as_object_mut() else {
            return line;
        };
        if self.did == Did::Rename
            && let (Some(from), Some(to)) = (
                self.from.as_deref().and_then(path_to_title),
                path_to_title(&self.path),
            )
        {
            members.insert("from".to_owned(), from.into());
            members.insert("to".to_owned(), to.into());
        }
        if let Some(message) = &self.message {
            // What is hidden is left out, not blanked, and the record says what and why (RFC 006 D-13).
            if self.hidden.author.is_none() {
                members.insert("author".to_owned(), author_json(message.proposed_by()));
                inklkunn.insert("proposed_by".to_owned(), person_of(message.proposed_by()));
                inklkunn.insert("accepted_by".to_owned(), person_of(message.accepted_by()));
            }
            if self.hidden.summary.is_none() {
                members.insert("summary".to_owned(), message.summary().as_str().into());
            }
        }
        if let Text::Given(text) = &self.text {
            members.insert("content".to_owned(), text.clone().into());
            members.insert(
                "sha256".to_owned(),
                hex(&Sha256::digest(text.as_bytes())).into(),
            );
        }
        self.hidden.mark(members);
        members.insert("inklkunn".to_owned(), inklkunn.into());
        line
    }
}

/// How many blocks one write of `first_seen` records (X-6): a long export must not hold SQLite's write lock for longer than a running
/// server waits for it (its `busy_timeout`).
pub(super) const FIRST_SEEN_CHUNK: usize = 10_000;

/// **A page removed and made again by one block, with the same id, is one page moved** (X-4): the removal and the creation of the
/// block are one `rename` record, from the path the page had to the path it has, with the text of the new one. Where it is not
/// the same block, they are two records.
fn join_removal_and_creation(records: Vec<Parts>) -> Vec<Parts> {
    let mut out: Vec<Parts> = Vec::with_capacity(records.len());
    for mut record in records {
        let joined = out
            .last()
            .is_some_and(|last| last.did == Did::Delete && last.block == record.block)
            && record.did == Did::Create;
        if joined && let Some(removal) = out.pop() {
            record.did = Did::Rename;
            record.from = Some(removal.path);
            record.hidden = record.hidden.or(removal.hidden);
        }
        out.push(record);
    }
    out
}

/// The history of the pages of a log, for the export of the tip `tip` of `view`. `tip_pages` is every page of the tip by path, with
/// the id its frontmatter gives it, if any (what names a file for a page that is still there). `sealed` is the time each block
/// that this service made was sealed at (block id → Unix seconds; [`super::sealing_times`]).
#[allow(clippy::too_many_arguments)]
pub(super) fn histories(
    storage: &dyn Storage,
    repo: &std::path::Path,
    wiki: &WikiRecord,
    view: &str,
    tip: &str,
    tip_pages: &HashMap<String, Option<String>>,
    suppressions: &Suppressions,
    sealed: &BTreeMap<String, i64>,
    now: i64,
    inputs: &mut HistoryInputs<'_>,
) -> Result<Histories, ExportError> {
    let log = storage
        .history(
            repo,
            &Point::View(view.to_owned()),
            Some(MAX_HISTORY_BLOCKS),
        )
        .map_err(ExportError::Storage)?;
    if log.blocks.len() as u64 >= MAX_HISTORY_BLOCKS {
        return Err(ExportError::HistoryTooLong);
    }
    // Newest first; blocks newer than the tip (a write since the extraction) are not part of this bundle.
    let from = log
        .blocks
        .iter()
        .position(|b| b.block_id == tip)
        .ok_or_else(|| {
            ExportError::Io("the tip is not in the view's history (the view moved)".to_owned())
        })?;
    let blocks: Vec<&HistoryBlock> = log.blocks.iter().skip(from).rev().collect();

    // The pages and what each block did to them. A block whose effect on the files its operations do not list (prikk's own rollback,
    // a merge, a repair; the one reader of a text calls such a block opaque) is counted as a change that no page's file holds, and
    // remembered: the records after it say so (X-3).
    let mut tracker = Tracker::default();
    let mut revisions: Vec<Revision> = Vec::new();
    let mut opaque: Vec<usize> = Vec::new();
    let mut hidden_summaries: Vec<(String, ReasonCategory)> = Vec::new();
    for (place, block) in blocks.iter().enumerate() {
        if block.rollback_block || !matches!(block.kind.as_str(), "Root" | "Normal") {
            tracker.unattributed += 1;
            opaque.push(place);
        }
        let change = storage
            .change(repo, &block.block_id)
            .map_err(ExportError::Storage)?;
        let messages = messages_of(block, &change);
        let of_change = suppressions.of_change(&block.block_id);
        for message in messages.iter().flatten() {
            if let Some(reason) = of_change.summary {
                hidden_summaries.push((message.summary().as_str().to_owned(), reason));
            }
        }
        let mut state = BlockState::default();
        for (patch_place, patch) in change.patches.iter().enumerate() {
            let ops = classify(patch);
            // A patch's path-addressed operations come first, then its edits (an edit names the path the file has at the end of
            // its patch).
            let (edits, others): (Vec<Op>, Vec<Op>) =
                ops.into_iter().partition(|op| matches!(op, Op::Edit(_)));
            for op in others.into_iter().chain(edits) {
                tracker.apply(&mut state, op, patch_place);
            }
        }
        for (id, effect) in std::mem::take(&mut state.effects) {
            let after = tracker.chains.get(id).and_then(|c| c.path.clone());
            let (did, path, moved_from) = match (effect.existed_before, after) {
                (false, Some(path)) => (Did::Create, path, None),
                (false, None) => continue,
                (true, None) => (Did::Delete, effect.before.unwrap_or_default(), None),
                (true, Some(path)) => {
                    if effect.before.as_deref() != Some(path.as_str()) {
                        (Did::Rename, path, effect.before)
                    } else if effect.written {
                        (Did::Edit, path, None)
                    } else {
                        continue;
                    }
                }
            };
            let message = messages.get(effect.patch).cloned().flatten();
            let quoted_hidden = message.as_ref().and_then(|m| {
                let summary = m.summary().as_str();
                summary
                    .starts_with("Undo: ")
                    .then(|| {
                        hidden_summaries
                            .iter()
                            .find(|(words, _)| quoted_summary(summary, words))
                            .map(|(_, reason)| *reason)
                    })
                    .flatten()
            });
            revisions.push(Revision {
                chain: id,
                block: place,
                did,
                path,
                from: moved_from,
                message,
                quoted_hidden,
            });
        }
    }
    inputs.say(ExportNote::Revisions {
        revisions: revisions.len(),
        pages: revisions
            .iter()
            .map(|r| r.chain)
            .collect::<BTreeSet<_>>()
            .len(),
    });

    // The time of each block (X-2): when its act sealed, then an imported change's own time, then when the service first saw it.
    // Only a block with neither is asked of `first_seen` (which writes), and in chunks (X-6). A block's first-seen time is an
    // upper bound that never runs past the time of any later block that is known, since no change was made after the first sight
    // (or the seal) of a change that came after it.
    let sealed_of = |place: usize| {
        blocks
            .get(place)
            .and_then(|b| sealed.get(&b.block_id))
            .copied()
    };
    let source_of = |message: &Option<Message>| message.as_ref().and_then(source_time);
    let mut need: BTreeMap<usize, String> = BTreeMap::new();
    for rev in &revisions {
        if sealed_of(rev.block).is_none()
            && source_of(&rev.message).is_none()
            && let Some(block) = blocks.get(rev.block)
        {
            need.insert(rev.block, block.block_id.clone());
        }
    }
    let need: Vec<(usize, String)> = need.into_iter().collect();
    let mut seen_at: HashMap<usize, i64> = HashMap::new();
    for chunk in need.chunks(FIRST_SEEN_CHUNK) {
        let ids: Vec<String> = chunk.iter().map(|(_, id)| id.clone()).collect();
        let seen = inputs
            .derived
            .first_seen(&wiki.id, &ids, now)
            .map_err(|_| {
                ExportError::Io(
                    "when the service first saw each change could not be read".to_owned(),
                )
            })?;
        for ((place, _), at) in chunk.iter().zip(seen) {
            seen_at.insert(*place, at);
        }
    }
    let mut floor = i64::MAX;
    let mut floors: Vec<i64> = vec![i64::MAX; blocks.len()];
    for place in (0..blocks.len()).rev() {
        if let Some(at) = sealed_of(place).or_else(|| seen_at.get(&place).copied()) {
            floor = floor.min(at);
        }
        if let Some(slot) = floors.get_mut(place) {
            *slot = floor;
        }
    }

    // The text of each revision, through the one reader.
    let mut held_bytes = 0_u64;
    let mut withheld = 0_usize;
    let mut parts: Vec<Parts> = Vec::with_capacity(revisions.len());
    for rev in revisions {
        let Some(block) = blocks.get(rev.block) else {
            continue;
        };
        let mut hidden = suppressions.of_revision(&block.block_id, &rev.path);
        if let Some(from) = &rev.from {
            hidden = hidden.or(suppressions.of_revision(&block.block_id, from));
        }
        if hidden.summary.is_none() {
            hidden.summary = rev.quoted_hidden;
        }
        let text = if rev.did == Did::Delete {
            Text::None
        } else if hidden.content.is_some() {
            // Already hidden by the record of the hide: its text is not asked for at all.
            Text::Hidden
        } else {
            match read_text(inputs, wiki, view, &block.block_id, &rev.path)? {
                Got::Read(BlockText::Text(text)) => Text::Given(text),
                Got::Read(BlockText::Hidden(mark)) => {
                    hidden.content = Some(mark.category);
                    Text::Hidden
                }
                Got::Never(reason) => {
                    withheld += 1;
                    inputs.say(ExportNote::Withheld {
                        block: block.block_id.clone(),
                        path: rev.path.clone(),
                        reason: reason.as_str(),
                    });
                    Text::Withheld(reason)
                }
                Got::Read(BlockText::Missing) => {
                    return Err(unreadable(
                        &rev.path,
                        &block.block_id,
                        "the page is not there",
                    ));
                }
                Got::Read(BlockText::NotText) => {
                    return Err(unreadable(
                        &rev.path,
                        &block.block_id,
                        "it is not UTF-8 text",
                    ));
                }
                Got::Read(BlockText::TooLarge) => {
                    return Err(unreadable(
                        &rev.path,
                        &block.block_id,
                        "it is larger than the most a page may be",
                    ));
                }
            }
        };
        let (at, at_from) = best_time(
            sealed_of(rev.block),
            source_of(&rev.message),
            floors.get(rev.block).copied().filter(|at| *at != i64::MAX),
        )
        .unwrap_or((now, AtFrom::FirstSeen));
        let record = Parts {
            chain: rev.chain,
            block: rev.block,
            rev: block.block_id.clone(),
            revert: rev
                .message
                .as_ref()
                .is_some_and(|m| m.kind() == Kind::Rollback),
            did: rev.did,
            text,
            hidden,
            message: rev.message,
            path: rev.path,
            from: rev.from,
            seq: block.update_seq,
            at,
            at_from,
            parent: None,
            untraced: None,
        };
        // What is held is what will be written: the line as it is serialised, escapes counted (X-8).
        if matches!(record.text, Text::Given(_)) {
            held_bytes += record.line().len() as u64 + 1;
            if held_bytes > inputs.cap {
                return Err(ExportError::HistoryTextTooLarge);
            }
        }
        parts.push(record);
    }

    // Which file each page's records go to: the page's own id, as its frontmatter has it (the tip's for a page that is still
    // there, the last readable revision's for one that is not), and **its path where it has none** (XFER-7: *named by the page's
    // `id`, or by its path when no identifier exists*; a page made in the wiki's own editor has no id). A page that comes back
    // with the same id or at the same path (a way back) is the same page, and its records are one file.
    let mut by_chain: BTreeMap<usize, Vec<Parts>> = BTreeMap::new();
    for part in parts {
        by_chain.entry(part.chain).or_default().push(part);
    }
    let mut by_key: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (chain, records) in &by_chain {
        let key = match tracker.chains.get(*chain).and_then(|c| c.path.as_deref()) {
            Some(path) => match tip_pages.get(path) {
                Some(Some(id)) => Key::Id(id.clone()),
                _ => Key::Path(path.to_owned()),
            },
            None => records
                .iter()
                .rev()
                .find_map(|r| match &r.text {
                    Text::Given(text) => inklkunn_markup::parse_metadata(text)
                        .ok()
                        .and_then(|m| m.id),
                    _ => None,
                })
                .map_or_else(
                    || Key::Path(records.last().map(|r| r.path.clone()).unwrap_or_default()),
                    Key::Id,
                ),
        };
        by_key.entry(key).or_default().push(*chain);
    }
    let mut out = Histories {
        files: BTreeMap::new(),
        unattributed: tracker.unattributed,
        accounts: BTreeMap::new(),
        suppressed: BTreeMap::new(),
        not_written: 0,
        withheld,
        after_untraced: Vec::new(),
    };
    // How many pages of the tip carry each id: an id that two pages share cannot say whose history a file holds.
    let mut carriers: HashMap<&str, usize> = HashMap::new();
    for id in tip_pages.values().flatten() {
        *carriers.entry(id.as_str()).or_default() += 1;
    }
    for (key, mut chains) in by_key {
        chains.sort_by_key(|c| by_chain.get(c).and_then(|r| r.first()).map(|r| r.block));
        // One page at a time: a page that comes back begins after the one before it was removed, and none is there beside another.
        // The one exception is a page removed and made again by one block, which is that page moved (X-4).
        let mut previous: Option<Option<usize>> = None;
        let mut one_page_at_a_time = true;
        for chain in &chains {
            let records = by_chain.get(chain).map_or(&[][..], Vec::as_slice);
            let first = records.first().map(|r| (r.block, r.did));
            match (previous, first) {
                (None, _) => {}
                (Some(Some(end)), Some((first, _))) if first > end => {}
                (Some(Some(end)), Some((first, Did::Create))) if first == end => {}
                _ => one_page_at_a_time = false,
            }
            previous = Some(match records.last() {
                Some(r) if r.did == Did::Delete => Some(r.block),
                _ => None,
            });
        }
        let name = match &key {
            Key::Id(id) => {
                let carried = carriers.get(id.as_str()).copied().unwrap_or(0);
                history_file_allowed(id, carried).then(|| id.clone())
            }
            Key::Path(path) => path_file_name(path),
        };
        let Some(name) = name.filter(|_| one_page_at_a_time) else {
            out.not_written += chains.len();
            continue;
        };
        let mut records: Vec<Parts> = Vec::new();
        for chain in &chains {
            records.extend(by_chain.remove(chain).unwrap_or_default());
        }
        let mut records = join_removal_and_creation(records);
        // The links: each record's parent, a time that never runs backwards (a record whose time would be before the one before it
        // takes that one's), and the block with no listed effect that lies between two records (the oldest, X-3).
        let mut before: Option<(String, i64, usize)> = None;
        let mut passes_untraced = false;
        for record in &mut records {
            if let Some((rev, at, _)) = &before {
                record.parent = Some(rev.clone());
                if record.at < *at {
                    record.at = *at;
                    record.at_from = AtFrom::Ordered;
                }
            }
            let lower = before.as_ref().map_or(0, |(_, _, place)| *place);
            let between = opaque
                .get(opaque.partition_point(|place| *place < lower))
                .copied()
                .filter(|place| *place < record.block);
            if let Some(place) = between
                && (before.is_some() || record.did != Did::Create)
            {
                record.untraced = blocks.get(place).map(|b| b.block_id.clone());
                passes_untraced = true;
            }
            before = Some((record.rev.clone(), record.at, record.block));
        }
        if passes_untraced {
            out.after_untraced.push(name.clone());
        }
        let mut hidden_lines = 0;
        for record in &records {
            if record.is_hidden() {
                hidden_lines += 1;
            }
            // A person hidden here is not a person of the bundle for it.
            if record.hidden.author.is_none()
                && let Some(message) = &record.message
            {
                for actor in [message.proposed_by(), message.accepted_by()] {
                    if let Actor::Account { id, name } = actor {
                        let names = out.accounts.entry(id.as_str().to_owned()).or_default();
                        if !names.iter().any(|n| n == name.as_str()) {
                            names.push(name.as_str().to_owned());
                        }
                    }
                }
            }
        }
        out.suppressed.insert(name.clone(), hidden_lines);
        out.files.insert(name, records);
    }
    // A page of the tip with no record at all (a log that never says how it came to be) has no history file either, and says so.
    let followed: BTreeSet<&str> = tracker
        .chains
        .iter()
        .filter_map(|c| c.path.as_deref())
        .collect();
    out.not_written += tip_pages
        .keys()
        .filter(|path| !followed.contains(path.as_str()))
        .count();
    Ok(out)
}
