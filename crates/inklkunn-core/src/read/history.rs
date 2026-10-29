//! A page's history, a change, a diff and a permalink (RFC 003 D-4; internal design RD-2; handoff 003 T5).
//!
//! ```text
//! history  ─▶ the view's tip ─▶ the title index (which path is this title) ─▶ the log window (held per tip)
//!          ─▶ the blocks newest first, each one's touches (one `show`, kept by block id) ─▶ the rows
//! change   ─▶ the block in the log window ─▶ one `show`
//! diff     ─▶ the path of the title ─▶ ONE `diff --path <the page's path>` between two blocks
//! at       ─▶ the block in the log window ─▶ one `cat` at the block, rendered against the current titles
//! ```
//!
//! **What prikk gives and what it does not (ledger PK-36).** `log` lists a view's blocks newest first and
//! cannot start from a block; `--limit` may be as large as the history. There is no per-path log and no time
//! in a block. So a page's history is **the view's log, read once per tip and kept (a window of the newest
//! blocks), and each block's `show` (kept by block id for good: a block never changes) filtered to the page's
//! path**. A request inspects at most [`MAX_INSPECTED_PER_REQUEST`] blocks and asks prikk for at most
//! [`MAX_SHOWS_PER_REQUEST`] `show`s it did not already hold; when it stops before the end it returns where to
//! continue, so a long history is read in steps and never in one unbounded request. A rename is followed: the
//! history of a moved page goes on under its old title.
//!
//! **A diff is always the page's own.** [`ReadService::page_diff`] issues exactly one `diff`, always with
//! `--path <the page's path>`: prikk's `diff --path` filters only the output, so a whole-tree diff costs the
//! same replay and says far more than the page needs; none is ever asked for here.
//! Both points must be changes of the view's own history (found in the held log): a block that is in the
//! repository and not in the view is not a reader's to compare, so no `diff` is asked for it.
//!
//! **A permalink renders the page's text at the block, against today's titles.** Rendering a page "as it was"
//! with the title index of that block would cost one `extract` of every block a reader (or a crawler) names,
//! and the derived cache is kept only for the tips a wiki has now. So the text is read at the block (one
//! `cat`), and which of its links lead to pages is decided by the index of the view's tip. The result is held
//! in a small memory cache, never in the derived store.
//!
//! **What a change says about who made it is what its message says**, read with the CT-03 parser: a message
//! that does not parse is shown as a first line and *not recorded* as to people, never guessed.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_markup::path::{path_to_title, title_to_path};
use inklkunn_prikk::{
    Change as PrikkChange, HistoryBlock, PatchMessage, Point, StorageError, Unavailable,
};
use inklkunn_store::{DerivedPage, IndexEntry, WikiId, WikiRecord};

use super::linediff::HunkView;
use super::{Lookup, PageView, Presentation, ReadError, ReadService, Tip, TitleIndex};
use crate::message::{Actor, Kind as MessageKind, Message, SUMMARY_MAX_CHARS};

/// The rows of one history page.
pub const HISTORY_PAGE_ROWS: usize = 20;
/// The most blocks one request inspects, held or not (a bound on CPU).
pub const MAX_INSPECTED_PER_REQUEST: usize = 400;
/// The most `show`s one request makes that were not already held (a bound on prikk calls).
pub const MAX_SHOWS_PER_REQUEST: usize = 24;
/// How many blocks the log is read for at first.
const DEFAULT_WINDOW: usize = 200;
/// The most blocks of a view's log the service holds and reads: older changes are not listed.
pub const MAX_WINDOW: usize = 10_000;
/// The most log windows held, over all wikis and views.
const MAX_WINDOWS: usize = 16;
/// The most operations held for blocks together, and the most one block may hold.
const MAX_TOUCH_WEIGHT: usize = 200_000;
const MAX_TOUCHES_PER_BLOCK: usize = 20_000;
/// The most permalink pages held in memory.
const MAX_AT_PAGES: usize = 32;
/// The most operations a change page lists.
pub const MAX_DID: usize = 200;
/// The most lines of a diff that are returned; the rest is counted and not sent.
pub const MAX_DIFF_LINES: usize = 4_000;

// ------------------------------------------------------------------------------------ who and what

/// Who a change's record names. The words for them are the surface's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Who {
    /// A person, by the display name they had at the time. Plain text: a surface escapes it.
    Person {
        /// The name.
        name: String,
    },
    /// An anonymous contributor, by temporary pseudonym.
    Anonymous {
        /// The pseudonym (`~` and letters or digits).
        token: String,
    },
    /// The wiki service itself.
    Service,
}

/// What a message records the change as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// An accepted proposal.
    Proposal,
    /// An accepted undo.
    Rollback,
    /// An import.
    Import,
    /// Something the wiki service did itself.
    System,
}

/// What a change's message says about the people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// A message in the form the service writes (CT-03).
    Recorded {
        /// What the change is.
        kind: ChangeKind,
        /// Who proposed it.
        proposed_by: Who,
        /// Who accepted it.
        accepted_by: Who,
    },
    /// A message that is not in that form: the change was sealed by someone else, or before the contract. It
    /// names nobody, and nobody is assumed.
    NotRecorded,
}

/// One change of a view's history: a block, as the log and its message describe it. There is no time here:
/// prikk's log carries none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeInfo {
    /// The block's id (64 hex). Its address in the change and permalink pages; never shown as text.
    pub block_id: String,
    /// The view's RefState after this block: what a cache is keyed by.
    pub state_id: String,
    /// The block's number in its view's history (1 for the first).
    pub number: u64,
    /// The block before this one, when the log window holds it and this is not the first.
    pub parent: Option<String>,
    /// The first block of the view.
    pub first: bool,
    /// prikk says this block undoes earlier ones.
    pub undoes: bool,
    /// The summary line: the message's first line, one line, at most 120 characters. Untrusted text.
    pub summary: String,
    /// What the message says about the people.
    pub provenance: Provenance,
}

/// What a change did to the page the history is of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEvent {
    /// The page was created.
    Created,
    /// The page was edited.
    Edited,
    /// The page was removed.
    Deleted,
    /// The page was moved here from another title.
    MovedFrom {
        /// The title it had.
        title: String,
    },
    /// The page was moved to another title.
    MovedTo {
        /// The title it has.
        title: String,
    },
    /// The change is too large to list what it did, so it is not known whether it touched this page.
    NotInspected,
}

/// One row of a page's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    /// The change.
    pub change: ChangeInfo,
    /// The page's path after the change: it differs from the history's own for a change before a move.
    pub path: String,
    /// What it did to the page, oldest first.
    pub events: Vec<PageEvent>,
}

/// A page's history, one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageHistory {
    /// The page's title.
    pub title: String,
    /// The page's path (a storage path).
    pub path: String,
    /// The tip the history was read at.
    pub tip: Tip,
    /// Changes that touched the page, newest first.
    pub rows: Vec<HistoryRow>,
    /// Where to continue: pass as `after`. `None` when the history is read to its end.
    pub older: Option<u64>,
    /// The log is longer than [`MAX_WINDOW`] blocks, so the oldest changes are not listed.
    pub beyond_window: bool,
}

/// What a history request came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryOutcome {
    /// The page's history (empty when no change touched it among the blocks read).
    Page(Box<PageHistory>),
    /// Several pages match the title and none is nearer.
    Ambiguous {
        /// Every candidate.
        candidates: Vec<IndexEntry>,
    },
}

/// What a change did, to what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// A page, by title.
    Page {
        /// Its title.
        title: String,
    },
    /// Any other file (an attachment, the manifest): not named.
    File,
}

/// One thing a change did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Did {
    /// Added.
    Added(Subject),
    /// Edited.
    Edited(Subject),
    /// Removed.
    Removed(Subject),
    /// Moved.
    Moved {
        /// From what.
        from: Subject,
        /// To what.
        to: Subject,
    },
}

/// A change page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeDetail {
    /// The change.
    pub info: ChangeInfo,
    /// What it did, at most [`MAX_DID`], without repeats.
    pub did: Vec<Did>,
    /// How many more it did than are listed.
    pub omitted: usize,
    /// The change is too large for prikk to describe within the bound the service keeps.
    pub too_large: bool,
}

/// What a change request came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeOutcome {
    /// The change.
    Found(Box<ChangeDetail>),
    /// The view's history has no such block.
    NoSuchChange,
}

/// How the page differs between two points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    /// The page is the same at both points.
    Unchanged,
    /// The page exists at the second point only.
    Added,
    /// The page exists at the first point only.
    Removed,
    /// The page differs.
    Modified,
    /// A status this service does not describe; the lines, if any, are still shown.
    Other,
}

/// A page's diff between two blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDiff {
    /// The page's title.
    pub title: String,
    /// The page's path.
    pub path: String,
    /// The first block.
    pub from: String,
    /// The second block.
    pub to: String,
    /// The first block's number in the view's history.
    pub from_number: u64,
    /// The second block's number, likewise.
    pub to_number: u64,
    /// How the page differs.
    pub status: DiffStatus,
    /// The hunks prikk gave, as lines with inklkunn's word marks, at most [`MAX_DIFF_LINES`] lines.
    pub hunks: Vec<HunkView>,
    /// Lines added, over all hunks prikk gave.
    pub added: u64,
    /// Lines removed.
    pub removed: u64,
    /// More lines were given than are returned.
    pub truncated: bool,
    /// prikk gave no lines for a page that differs: it is not text, or prikk could not diff it.
    pub no_lines: bool,
}

/// What a diff request came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOutcome {
    /// The diff.
    Found(Box<PageDiff>),
    /// One of the blocks is not one of this wiki's.
    NoSuchChange,
    /// Several pages match the title and none is nearer.
    Ambiguous {
        /// Every candidate.
        candidates: Vec<IndexEntry>,
    },
}

/// What a permalink request came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtOutcome {
    /// The page as the block had it.
    Found {
        /// The page, read at the block.
        view: Box<PageView>,
        /// The change the page is shown after.
        change: Box<ChangeInfo>,
    },
    /// The page did not exist after that change.
    PageMissing {
        /// The title asked for.
        title: String,
        /// The change.
        change: Box<ChangeInfo>,
    },
    /// The view's history has no such block.
    NoSuchChange,
    /// Several pages match the title and none is nearer.
    Ambiguous {
        /// Every candidate.
        candidates: Vec<IndexEntry>,
    },
}

// ------------------------------------------------------------------------------------ what is held

/// A view's log, as far as it was read.
#[derive(Debug)]
struct Window {
    wiki: WikiId,
    view: String,
    /// The state id of the newest block read.
    head_state: String,
    blocks: Vec<ChangeInfo>,
    /// The whole log is here (the read came back short).
    complete: bool,
}

/// What one operation of a block did to one path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TouchKind {
    Create,
    Edit,
    Delete,
    Rename { from: String },
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Touch {
    kind: TouchKind,
    /// The path touched (the new path of a rename).
    path: String,
}

/// What is known of a block's operations.
#[derive(Debug, Clone)]
enum Touches {
    Known(Arc<Vec<Touch>>),
    /// Too large to list within the bounds.
    TooLarge,
}

#[derive(Debug, Default)]
struct TouchCache {
    map: HashMap<String, Touches>,
    order: VecDeque<String>,
    weight: usize,
}

impl TouchCache {
    fn get(&self, block: &str) -> Option<Touches> {
        self.map.get(block).cloned()
    }

    fn put(&mut self, block: &str, touches: Touches) {
        if self.map.contains_key(block) {
            return;
        }
        let cost = match &touches {
            Touches::Known(list) => list.len() + 1,
            Touches::TooLarge => 1,
        };
        self.weight += cost;
        self.order.push_back(block.to_owned());
        self.map.insert(block.to_owned(), touches);
        while self.weight > MAX_TOUCH_WEIGHT {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(Touches::Known(list)) = self.map.remove(&oldest) {
                self.weight = self.weight.saturating_sub(list.len() + 1);
            } else {
                self.weight = self.weight.saturating_sub(1);
            }
        }
    }
}

/// What the read service holds for T5, all bounded and none authoritative (ENF-5): rebuilt from prikk.
#[derive(Debug, Default)]
pub(super) struct State {
    windows: Mutex<Vec<Arc<Window>>>,
    touches: Mutex<TouchCache>,
    at_pages: Mutex<VecDeque<(String, DerivedPage)>>,
}

impl State {
    /// Forget the logs of a wiki (a write path calls it after it seals).
    pub(super) fn forget(&self, wiki: &WikiId) {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|w| w.wiki != *wiki);
    }
}

// ------------------------------------------------------------------------------------ reading blocks

/// Is `id` a block id as prikk prints it: 64 lower-case hex characters?
fn is_block_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One line of at most [`SUMMARY_MAX_CHARS`] characters from untrusted message text.
fn one_line(text: &str) -> String {
    let first = text.lines().next().unwrap_or("");
    let cleaned: String = first
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().count() <= SUMMARY_MAX_CHARS {
        return trimmed.to_owned();
    }
    let mut cut: String = trimmed.chars().take(SUMMARY_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

fn who(actor: &Actor) -> Who {
    match actor {
        Actor::Account { name, .. } => Who::Person {
            name: name.as_str().to_owned(),
        },
        Actor::Anonymous(p) => Who::Anonymous {
            token: p.as_str().to_owned(),
        },
        Actor::System(_) => Who::Service,
    }
}

fn provenance(message: &Message) -> Provenance {
    Provenance::Recorded {
        kind: match message.kind() {
            MessageKind::Proposal => ChangeKind::Proposal,
            MessageKind::Rollback => ChangeKind::Rollback,
            MessageKind::Import => ChangeKind::Import,
            MessageKind::System => ChangeKind::System,
        },
        proposed_by: who(message.proposed_by()),
        accepted_by: who(message.accepted_by()),
    }
}

/// The summary and the people of a block: from the first of its patch messages that is in CT-03 form, else the
/// first line of the first message with the people *not recorded*.
fn read_messages(messages: &[PatchMessage]) -> (String, Provenance) {
    for pm in messages {
        if let Ok(message) = Message::parse(&pm.message) {
            return (message.summary().as_str().to_owned(), provenance(&message));
        }
    }
    let first = messages.first().map(|m| one_line(&m.message));
    (first.unwrap_or_default(), Provenance::NotRecorded)
}

/// The changes of a log, newest first, each read once: the raw log (with every message whole) is not kept, only
/// what the pages show, so a held window is small however long the messages are.
fn infos_of(blocks: &[HistoryBlock]) -> Vec<ChangeInfo> {
    blocks
        .iter()
        .enumerate()
        .map(|(at, block)| {
            let (summary, provenance) = read_messages(&block.patch_messages);
            let first = block.kind == "Root" || block.previous_ref_state_id.is_none();
            ChangeInfo {
                block_id: block.block_id.clone(),
                state_id: block.ref_state_id.clone(),
                number: block.update_seq,
                parent: if first {
                    None
                } else {
                    blocks.get(at + 1).map(|b| b.block_id.clone())
                },
                first,
                undoes: block.rollback_block,
                summary,
                provenance,
            }
        })
        .collect()
}

/// What a block's operations did, in order, with the paths they name. An operation whose path prikk could not
/// resolve names no path and is left out.
fn classify(change: &PrikkChange) -> Vec<Touch> {
    let mut out = Vec::new();
    for patch in &change.patches {
        for op in &patch.operations {
            let path = |i: usize| op.paths.get(i).and_then(|p| p.path.clone());
            let touch = match op.kind.as_str() {
                "create-file" | "create-symlink" => path(0).map(|p| (TouchKind::Create, p)),
                "edit-text" | "replace-binary" => path(0).map(|p| (TouchKind::Edit, p)),
                "delete-node" => path(0).map(|p| (TouchKind::Delete, p)),
                "rename-path" => match (path(0), path(1)) {
                    (Some(from), Some(to)) => Some((TouchKind::Rename { from }, to)),
                    _ => None,
                },
                _ => path(0).map(|p| (TouchKind::Other, p)),
            };
            if let Some((kind, path)) = touch {
                out.push(Touch { kind, path });
            }
        }
    }
    out
}

/// What the block did to the page now at `tracked`, oldest first, and the path the page had before the block
/// (the same unless the block moved it). Operations are read backwards, so a block that edits and then moves
/// a page is read the way the repository applied it.
fn events_for(touches: &[Touch], tracked: &mut String) -> Vec<PageEvent> {
    let mut events = Vec::new();
    for touch in touches.iter().rev() {
        match &touch.kind {
            TouchKind::Rename { from } if touch.path == *tracked => {
                events.push(match path_to_title(from) {
                    Some(title) => PageEvent::MovedFrom { title },
                    None => PageEvent::Created,
                });
                from.clone_into(tracked);
            }
            TouchKind::Rename { from } if from == tracked => {
                if let Some(title) = path_to_title(&touch.path) {
                    events.push(PageEvent::MovedTo { title });
                }
            }
            TouchKind::Create if touch.path == *tracked => events.push(PageEvent::Created),
            TouchKind::Edit if touch.path == *tracked => events.push(PageEvent::Edited),
            TouchKind::Delete if touch.path == *tracked => events.push(PageEvent::Deleted),
            _ => {}
        }
    }
    events.reverse();
    events.dedup();
    events
}

fn subject(path: &str) -> Subject {
    match path_to_title(path) {
        Some(title) => Subject::Page { title },
        None => Subject::File,
    }
}

fn did_of(touches: &[Touch]) -> Vec<Did> {
    let mut out: Vec<Did> = Vec::new();
    for touch in touches {
        let did = match &touch.kind {
            TouchKind::Create => Did::Added(subject(&touch.path)),
            TouchKind::Edit => Did::Edited(subject(&touch.path)),
            TouchKind::Delete => Did::Removed(subject(&touch.path)),
            TouchKind::Rename { from } => Did::Moved {
                from: subject(from),
                to: subject(&touch.path),
            },
            TouchKind::Other => continue,
        };
        if !out.contains(&did) {
            out.push(did);
        }
    }
    out
}

/// Where a title leads, for a surface that is about the page itself and does not follow redirects away from it.
enum Target {
    Page { path: String, title: String },
    Ambiguous(Vec<IndexEntry>),
}

fn resolve(index: &TitleIndex, title: &str) -> Result<Target, ReadError> {
    let fallback = || {
        title_to_path(title)
            .map(|path| Target::Page {
                path,
                title: title.trim().to_owned(),
            })
            .map_err(ReadError::InvalidTitle)
    };
    match index.lookup(title) {
        Lookup::Found(found) => Ok(Target::Page {
            path: found.entry.path,
            title: found.entry.title,
        }),
        Lookup::BrokenRedirect { from, .. } => Ok(Target::Page {
            path: from.path,
            title: from.title,
        }),
        Lookup::Ambiguous { candidates } => Ok(Target::Ambiguous(candidates)),
        // A page that was removed, or never made: its history may still exist.
        Lookup::Missing => fallback(),
    }
}

// ------------------------------------------------------------------------------------ the service

impl ReadService {
    /// The log of `tip`'s view, at least `need` blocks of it when it has them (and no more than
    /// [`MAX_WINDOW`]): the one held for this tip, else one `log` read and held.
    fn window_for(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        need: usize,
    ) -> Result<Arc<Window>, ReadError> {
        let need = need.clamp(1, MAX_WINDOW);
        {
            let held = self
                .history
                .windows
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(w) = held.iter().find(|w| {
                w.wiki == wiki.id
                    && w.view == tip.view
                    && w.head_state == tip.state_id
                    && (w.complete || w.blocks.len() >= need)
            }) {
                return Ok(Arc::clone(w));
            }
        }
        let limit = need.clamp(DEFAULT_WINDOW, MAX_WINDOW);
        let log = self
            .storage
            .history(
                &self.layout.repo_dir(&wiki.id),
                &Point::View(tip.view.clone()),
                Some(limit as u64),
            )
            .map_err(|e| self.storage_error(wiki, e))?;
        let window = Arc::new(Window {
            wiki: wiki.id.clone(),
            view: tip.view.clone(),
            head_state: log
                .blocks
                .first()
                .map(|b| b.ref_state_id.clone())
                .unwrap_or_default(),
            complete: log.blocks.len() < limit,
            blocks: infos_of(&log.blocks),
        });
        let mut held = self
            .history
            .windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        held.retain(|w| !(w.wiki == wiki.id && w.view == tip.view));
        held.push(Arc::clone(&window));
        let excess = held.len().saturating_sub(MAX_WINDOWS);
        held.drain(..excess);
        Ok(window)
    }

    /// The window that holds `block_id`, and where, growing the read (at most [`MAX_WINDOW`] blocks) until it
    /// is found or the log ends. `None` when the view's history has no such block.
    fn find_block(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        block_id: &str,
    ) -> Result<Option<(Arc<Window>, usize)>, ReadError> {
        let mut need = DEFAULT_WINDOW;
        loop {
            let window = self.window_for(wiki, tip, need)?;
            if let Some(at) = window.blocks.iter().position(|b| b.block_id == block_id) {
                return Ok(Some((window, at)));
            }
            if window.complete || window.blocks.len() >= MAX_WINDOW {
                return Ok(None);
            }
            need = window.blocks.len().saturating_mul(4);
        }
    }

    /// What a block did, from the held record or one `show`. The flag says a `show` was made.
    fn touches_of(&self, wiki: &WikiRecord, block_id: &str) -> Result<(Touches, bool), ReadError> {
        if let Some(held) = self
            .history
            .touches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(block_id)
        {
            return Ok((held, false));
        }
        let touches = match self
            .storage
            .change(&self.layout.repo_dir(&wiki.id), block_id)
        {
            Ok(change) => {
                let list = classify(&change);
                if list.len() > MAX_TOUCHES_PER_BLOCK {
                    Touches::TooLarge
                } else {
                    Touches::Known(Arc::new(list))
                }
            }
            // The report would not fit the bound the driver keeps: this block is too large to list, now and
            // always (a block never changes).
            Err(StorageError::Unavailable {
                reason: Unavailable::OutputExceeded,
                ..
            }) => Touches::TooLarge,
            Err(e) => return Err(self.storage_error(wiki, e)),
        };
        self.history
            .touches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put(block_id, touches.clone());
        Ok((touches, true))
    }

    fn target(&self, wiki: &WikiRecord, tip: &Tip, title: &str) -> Result<Target, ReadError> {
        let index = self.index_at(wiki, tip)?;
        resolve(&index, title)
    }

    /// The changes that touched the page titled `title`, newest first, one step of at most
    /// [`HISTORY_PAGE_ROWS`] rows. `after` is the number of the last change inspected by the step before
    /// (a step's [`PageHistory::older`]): the step begins with the change before it.
    pub fn page_history(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
        after: Option<u64>,
    ) -> Result<HistoryOutcome, ReadError> {
        title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let tip = self.tip(wiki, view)?;
        let (path, title) = match self.target(wiki, &tip, title)? {
            Target::Page { path, title } => (path, title),
            Target::Ambiguous(candidates) => return Ok(HistoryOutcome::Ambiguous { candidates }),
        };
        // Blocks are numbered 1, 2, 3 … in a view's log, newest last: the window must reach from the newest
        // block past `after` and this step's share.
        let first = self.window_for(wiki, &tip, DEFAULT_WINDOW)?;
        let ahead: usize = match (after, first.blocks.first()) {
            (Some(after), Some(head)) => head
                .number
                .saturating_sub(after.saturating_sub(1))
                .try_into()
                .unwrap_or(MAX_WINDOW),
            _ => 0,
        };
        let window = if first.complete || first.blocks.len() > ahead + MAX_INSPECTED_PER_REQUEST {
            first
        } else {
            self.window_for(wiki, &tip, ahead + MAX_INSPECTED_PER_REQUEST + 1)?
        };
        let start = after.map_or(0, |after| {
            window
                .blocks
                .iter()
                .position(|b| b.number < after)
                .unwrap_or(window.blocks.len())
        });
        let mut tracked = path.clone();
        let mut rows: Vec<HistoryRow> = Vec::new();
        let (mut inspected, mut shown) = (0_usize, 0_usize);
        let mut last = after;
        let mut stopped = false;
        for at in start..window.blocks.len() {
            if rows.len() >= HISTORY_PAGE_ROWS
                || shown >= MAX_SHOWS_PER_REQUEST
                || inspected >= MAX_INSPECTED_PER_REQUEST
            {
                stopped = true;
                break;
            }
            let Some(block) = window.blocks.get(at) else {
                break;
            };
            inspected += 1;
            last = Some(block.number);
            let (touches, fetched) = self.touches_of(wiki, &block.block_id)?;
            shown += usize::from(fetched);
            let path_after = tracked.clone();
            let events = match touches {
                Touches::Known(list) => events_for(&list, &mut tracked),
                Touches::TooLarge => vec![PageEvent::NotInspected],
            };
            if events.is_empty() {
                continue;
            }
            if let Some(change) = window.blocks.get(at).cloned() {
                rows.push(HistoryRow {
                    change,
                    path: path_after,
                    events,
                });
            }
        }
        Ok(HistoryOutcome::Page(Box::new(PageHistory {
            title,
            path,
            tip,
            rows,
            older: if stopped { last } else { None },
            beyond_window: !stopped && !window.complete,
        })))
    }

    /// The change in the view's history with this block id, and what it did: one `show` (the log window it is
    /// found in is held per tip).
    pub fn change(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
    ) -> Result<ChangeOutcome, ReadError> {
        if !is_block_id(block_id) {
            return Ok(ChangeOutcome::NoSuchChange);
        }
        let tip = self.tip(wiki, view)?;
        let Some((window, at)) = self.find_block(wiki, &tip, block_id)? else {
            return Ok(ChangeOutcome::NoSuchChange);
        };
        let Some(info) = window.blocks.get(at).cloned() else {
            return Ok(ChangeOutcome::NoSuchChange);
        };
        let shown = self
            .storage
            .change(&self.layout.repo_dir(&wiki.id), block_id);
        let (mut did, too_large) = match shown {
            Ok(change) => (did_of(&classify(&change)), false),
            Err(StorageError::Unavailable {
                reason: Unavailable::OutputExceeded,
                ..
            }) => (Vec::new(), true),
            Err(e) => return Err(self.storage_error(wiki, e)),
        };
        let omitted = did.len().saturating_sub(MAX_DID);
        did.truncate(MAX_DID);
        Ok(ChangeOutcome::Found(Box::new(ChangeDetail {
            info,
            did,
            omitted,
            too_large,
        })))
    }

    /// The page titled `title` between two blocks: **one `diff`, always with the page's path**. The lines are
    /// prikk's; the word marks are inklkunn's. A block that is not in the wiki's repository is
    /// [`DiffOutcome::NoSuchChange`].
    pub fn page_diff(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        from: &str,
        to: &str,
        title: &str,
    ) -> Result<DiffOutcome, ReadError> {
        title_to_path(title).map_err(ReadError::InvalidTitle)?;
        if !is_block_id(from) || !is_block_id(to) {
            return Ok(DiffOutcome::NoSuchChange);
        }
        let tip = self.tip(wiki, view)?;
        let (path, title) = match self.target(wiki, &tip, title)? {
            Target::Page { path, title } => (path, title),
            Target::Ambiguous(candidates) => return Ok(DiffOutcome::Ambiguous { candidates }),
        };
        // Both points must be changes of this view's history: a block that is in the repository and not in the
        // view (work held for review, another view) is not a reader's to compare.
        let (Some((from_window, from_at)), Some((to_window, to_at))) = (
            self.find_block(wiki, &tip, from)?,
            self.find_block(wiki, &tip, to)?,
        ) else {
            return Ok(DiffOutcome::NoSuchChange);
        };
        let (Some(from_block), Some(to_block)) =
            (from_window.blocks.get(from_at), to_window.blocks.get(to_at))
        else {
            return Ok(DiffOutcome::NoSuchChange);
        };
        let diff = match self.storage.diff(
            &self.layout.repo_dir(&wiki.id),
            &Point::Block(from.to_owned()),
            &Point::Block(to.to_owned()),
            &[path.as_str()],
        ) {
            Ok(diff) => diff,
            // prikk does not know a block: not one of this wiki's.
            Err(StorageError::Refused { .. }) => return Ok(DiffOutcome::NoSuchChange),
            Err(e) => return Err(self.storage_error(wiki, e)),
        };
        let entry = diff.entries.iter().find(|e| e.path == path);
        let status = match entry.map(|e| e.status.as_str()) {
            None => DiffStatus::Unchanged,
            Some("added") => DiffStatus::Added,
            Some("removed") => DiffStatus::Removed,
            Some("modified") => DiffStatus::Modified,
            Some(_) => DiffStatus::Other,
        };
        let (mut hunks, mut added, mut removed, mut lines) = (Vec::new(), 0_u64, 0_u64, 0_usize);
        let mut truncated = false;
        let given = entry.map_or(&[][..], |e| e.hunks.as_slice());
        for raw in given {
            let view = HunkView::of(raw);
            if let HunkView::Lines(h) = &view {
                added += h.added();
                removed += h.removed();
                lines += h.lines.len();
            }
            if truncated || lines > MAX_DIFF_LINES {
                truncated = true;
            } else {
                hunks.push(view);
            }
        }
        let no_lines = (given.is_empty() && status != DiffStatus::Unchanged)
            || diff.unsupported_paths.contains(&path);
        Ok(DiffOutcome::Found(Box::new(PageDiff {
            title,
            path,
            from: from.to_owned(),
            to: to.to_owned(),
            from_number: from_block.number,
            to_number: to_block.number,
            status,
            hunks,
            added,
            removed,
            truncated,
            no_lines,
        })))
    }

    /// The page titled `title` as it stood after the change `block_id` (a permalink): its text read at the
    /// block, rendered for `presentation` against the titles of the view's tip (see the module's account).
    pub fn page_at(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        title: &str,
        presentation: &dyn Presentation,
    ) -> Result<AtOutcome, ReadError> {
        title_to_path(title).map_err(ReadError::InvalidTitle)?;
        if !is_block_id(block_id) {
            return Ok(AtOutcome::NoSuchChange);
        }
        let tip = self.tip(wiki, view)?;
        let Some((window, at)) = self.find_block(wiki, &tip, block_id)? else {
            return Ok(AtOutcome::NoSuchChange);
        };
        let Some(change) = window.blocks.get(at).cloned() else {
            return Ok(AtOutcome::NoSuchChange);
        };
        let index = self.index_at(wiki, &tip)?;
        let (path, page_title) = match resolve(&index, title)? {
            Target::Page { path, title } => (path, title),
            Target::Ambiguous(candidates) => return Ok(AtOutcome::Ambiguous { candidates }),
        };
        let at_tip = Tip {
            view: tip.view.clone(),
            state_id: change.state_id.clone(),
            block_id: block_id.to_owned(),
        };
        let key = format!(
            "{}|{block_id}|{path}|{}|{}|{}",
            wiki.id,
            presentation.lang(),
            super::cache_presentation(presentation),
            index.tip()
        );
        let held = self
            .history
            .at_pages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, row)| row.clone());
        let (row, cached) = match held {
            Some(row) => (row, true),
            None => {
                let bytes = match self.storage.read(
                    &self.layout.repo_dir(&wiki.id),
                    &Point::Block(block_id.to_owned()),
                    &path,
                    self.config.page_max_bytes,
                ) {
                    Ok(bytes) => bytes,
                    Err(refused @ StorageError::Refused { .. }) => {
                        // Either there was no such page then, or it is over the size read: `stat` tells.
                        return match self.storage.stat(
                            &self.layout.repo_dir(&wiki.id),
                            &Point::Block(block_id.to_owned()),
                            &path,
                        ) {
                            Err(StorageError::Refused { .. }) => Ok(AtOutcome::PageMissing {
                                title: page_title,
                                change: Box::new(change),
                            }),
                            Ok(_) => Err(self.storage_error(wiki, refused)),
                            Err(e) => Err(self.storage_error(wiki, e)),
                        };
                    }
                    Err(e) => return Err(self.storage_error(wiki, e)),
                };
                let text = String::from_utf8(bytes)
                    .map_err(|_| ReadError::PageUnreadable(super::PageProblem::NotUtf8))?;
                let entry = IndexEntry {
                    path: path.clone(),
                    title: page_title.clone(),
                    aliases: Vec::new(),
                    redirect: None,
                    page_id: None,
                    kind: "page".to_owned(),
                    problem: None,
                };
                let row = self.render_text(wiki, &at_tip, &index, &entry, presentation, &text)?;
                let mut held = self
                    .history
                    .at_pages
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                held.push_back((key, row.clone()));
                while held.len() > MAX_AT_PAGES {
                    held.pop_front();
                }
                (row, false)
            }
        };
        Ok(AtOutcome::Found {
            view: Box::new(PageView {
                tip: at_tip,
                path,
                html: self.sanitizer.clean(&row.html),
                meta: row.meta,
                toc: row.toc,
                links: row.links,
                redirected_from: None,
                via: super::Via::Exact,
                cached,
                stale: false,
            }),
            change: Box::new(change),
        })
    }
}
