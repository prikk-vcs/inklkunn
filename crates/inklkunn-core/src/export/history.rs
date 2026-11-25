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
//!
//! **`at` is when this service first saw the block** ([`DerivedStore::first_seen`]), because prikk's log has no time and this is
//! the only time every page of the service shows (the changes list, the feed and the API all say *first seen*). It is an upper
//! bound on when the change was made, never a later time than any later change's, and the record says where it comes from
//! (`inklkunn.at_from`). A block that no surface has asked for yet is recorded as seen now, as it would be by the changes
//! list, and it stays so: the same export of the same tip is the same bytes.
//!
//! **What the index has not settled is waited for, and never guessed** (`ReadError::NotCheckedYet`): the export asks the index of
//! who wrote each page to be built, for a few rounds and saying so, and then fails with a sentence. A bundle is never written with
//! a gap, nor with a text that could not be told from a hidden one.

use std::collections::{BTreeMap, HashMap};

use inklkunn_markup::path::path_to_title;
use inklkunn_prikk::storage::{Change, HistoryBlock};
use inklkunn_prikk::{Point, Storage};
use inklkunn_signer::message::{Actor, Kind, Message};
use inklkunn_store::{DerivedStore, ReasonCategory, WikiRecord};
use sha2::{Digest, Sha256};

use super::{
    ExportError, MAX_HISTORY_BLOCKS, Suppressed, Suppressions, hex, history_file_allowed, person,
};
use crate::read::{BlockText, ReadError, ReadService, quoted_summary};

/// How many rounds the export waits for the index of who wrote each page's text, in all: each round is one request's part in
/// building it (about a second), after which the text is asked for again.
pub const INDEX_WAIT_ROUNDS: usize = 30;

/// The most bytes of page text one export holds before it writes them (every revision's full text is in a history line). A history
/// larger than this is refused rather than cut short.
pub const MAX_HISTORY_TEXT_BYTES: u64 = 1 << 30;

/// What an export says while it works (the command line prints it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// What the history of an export needs besides the repository: the one reader of a page's text at a block, and the record of when
/// each block was first seen. It may be given a way to say what it is doing.
pub struct HistoryInputs<'a> {
    reads: &'a ReadService,
    derived: &'a dyn DerivedStore,
    notes: Option<&'a mut dyn FnMut(ExportNote)>,
    waited: usize,
}

impl<'a> HistoryInputs<'a> {
    /// Inputs that say nothing.
    pub fn new(reads: &'a ReadService, derived: &'a dyn DerivedStore) -> Self {
        HistoryInputs {
            reads,
            derived,
            notes: None,
            waited: 0,
        }
    }

    /// The same, saying what it does to `notes`.
    #[must_use]
    pub fn saying(mut self, notes: &'a mut dyn FnMut(ExportNote)) -> Self {
        self.notes = Some(notes);
        self
    }

    fn say(&mut self, note: ExportNote) {
        if let Some(notes) = self.notes.as_mut() {
            notes(note);
        }
    }
}

/// What the history of an export came to.
pub(super) struct Histories {
    /// The lines of each history file, by page id.
    pub(super) files: BTreeMap<String, Vec<String>>,
    /// Changes to paths that are not pages (an attachment, a knowledge file, a rule), and operations prikk could not name a path
    /// for: they belong to no page's file.
    pub(super) unattributed: usize,
    /// Accounts seen in messages: id → the names they had, in order of first sight. **Only for what is not hidden.**
    pub(super) accounts: BTreeMap<String, Vec<String>>,
    /// file → how many of its lines have a part hidden.
    pub(super) suppressed: BTreeMap<String, usize>,
    /// Pages with revisions whose file was not written (the id cannot name a file, two pages share it, or the page has none).
    pub(super) not_written: usize,
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
                    // A file that was not a page, moved into the pages: the page is new.
                    if is_page(&to) {
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
                        // Moved out of the pages: it is no page now.
                        if let Some(chain) = self.chains.get_mut(id) {
                            chain.path = None;
                        }
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
    Withheld,
}

fn unreadable(path: &str, block: &str, why: &str) -> ExportError {
    ExportError::Io(format!(
        "the text of `{path}` at block {block} could not be read for the history ({why}); nothing was written"
    ))
}

/// The text of `path` at `block`, through the one reader. **A text that cannot be checked yet is waited for** (the index of who
/// wrote each page is asked to be built, a round at a time, and the export says so), **and then the export fails**: it never
/// guesses that a text is not hidden and never leaves a revision out.
fn read_text(
    inputs: &mut HistoryInputs<'_>,
    wiki: &WikiRecord,
    view: &str,
    block: &str,
    path: &str,
) -> Result<BlockText, ExportError> {
    loop {
        match inputs.reads.text_at(wiki, Some(view), block, path) {
            Ok(text) => return Ok(text),
            Err(ReadError::NotCheckedYet) => {
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
                inputs.reads.warm_writes(wiki).map_err(|e| {
                    unreadable(path, block, &format!("the index could not be built: {e}"))
                })?;
            }
            Err(e) => return Err(unreadable(path, block, &e.to_string())),
        }
    }
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
/// path under `pages/`, without `.md`, every byte but `A-Z a-z 0-9 . _ -` written as `%` and two hex digits** (`pages/Docs/Deep
/// Page.md` is `~Docs%2FDeep%20Page`). The `~` is what no page id can start with (an id is letters, digits, `.`, `_` and `-`), so a
/// path's name is never an id's; and the coding is one to one, so two paths never share a file. `None` when the name would be
/// longer than a file system's name may be.
fn path_file_name(path: &str) -> Option<String> {
    let title = path.strip_prefix("pages/")?.strip_suffix(".md")?;
    let mut name = String::from("~");
    for byte in title.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
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

/// A record before it is linked to the one before it.
struct Parts {
    chain: usize,
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
    revert: bool,
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

    fn json(&self, parent: Option<&str>) -> serde_json::Value {
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
        inklkunn.insert("at_from".to_owned(), "first-seen".into());
        let mut line = serde_json::json!({
            "rev": self.rev,
            "parent": parent,
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

/// The history of the pages of a log, for the export of the tip `tip` of `view`. `tip_pages` is every page of the tip by path, with
/// the id its frontmatter gives it, if any (what names a file for a page that is still there).
#[allow(clippy::too_many_arguments)]
pub(super) fn histories(
    storage: &dyn Storage,
    repo: &std::path::Path,
    wiki: &WikiRecord,
    view: &str,
    tip: &str,
    tip_pages: &HashMap<String, Option<String>>,
    suppressions: &Suppressions,
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

    // When each block was first seen, as an upper bound that never runs backwards: no change was made after the first sight of a
    // change that came after it.
    let ids: Vec<String> = blocks.iter().map(|b| b.block_id.clone()).collect();
    let seen = inputs
        .derived
        .first_seen(&wiki.id, &ids, now)
        .map_err(|_| {
            ExportError::Io("when the service first saw each change could not be read".to_owned())
        })?;
    let mut floor = i64::MAX;
    let mut times: Vec<i64> = seen
        .iter()
        .rev()
        .map(|t| {
            floor = floor.min(*t);
            floor
        })
        .collect();
    times.reverse();

    // The pages and what each block did to them.
    let mut tracker = Tracker::default();
    let mut revisions: Vec<Revision> = Vec::new();
    let mut hidden_summaries: Vec<(String, ReasonCategory)> = Vec::new();
    for (place, block) in blocks.iter().enumerate() {
        if block.rollback_block || !matches!(block.kind.as_str(), "Root" | "Normal") {
            // prikk's own rollback, a merge, a repair: what it did to the files is not in its operations (the one reader of a
            // text calls such a block opaque), so it is counted as a change that no page's file holds.
            tracker.unattributed += 1;
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
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
    });

    // The text of each revision, through the one reader.
    let mut held_bytes = 0_u64;
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
            Text::Withheld
        } else {
            match read_text(inputs, wiki, view, &block.block_id, &rev.path)? {
                BlockText::Text(text) => Text::Given(text),
                BlockText::Hidden(mark) => {
                    hidden.content = Some(mark.category);
                    Text::Withheld
                }
                BlockText::Missing => {
                    return Err(unreadable(
                        &rev.path,
                        &block.block_id,
                        "the page is not there",
                    ));
                }
                BlockText::NotText => {
                    return Err(unreadable(
                        &rev.path,
                        &block.block_id,
                        "it is not UTF-8 text",
                    ));
                }
                BlockText::TooLarge => {
                    return Err(unreadable(
                        &rev.path,
                        &block.block_id,
                        "it is larger than the most a page may be",
                    ));
                }
            }
        };
        if let Text::Given(text) = &text {
            held_bytes += text.len() as u64;
            if held_bytes > MAX_HISTORY_TEXT_BYTES {
                return Err(ExportError::HistoryTextTooLarge);
            }
        }
        parts.push(Parts {
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
            at: times.get(rev.block).copied().unwrap_or(now),
        });
    }

    // Which file each page's records go to: the page's own id, as its frontmatter has it (the tip's for a page that is still
    // there, the last readable revision's for one that is not), and **its path where it has none** (XFER-7: *named by the page's
    // `id`, or by its path when no identifier exists*; a page made in the wiki's own editor has no id). A page that comes back
    // with the same id or at the same path (a way back) is the same page, and its records are one file.
    let mut by_chain: BTreeMap<usize, Vec<&Parts>> = BTreeMap::new();
    for part in &parts {
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
    };
    // How many pages of the tip carry each id: an id that two pages share cannot say whose history a file holds.
    let mut carriers: HashMap<&str, usize> = HashMap::new();
    for id in tip_pages.values().flatten() {
        *carriers.entry(id.as_str()).or_default() += 1;
    }
    for (key, mut chains) in by_key {
        chains.sort_by_key(|c| by_chain.get(c).and_then(|r| r.first()).map(|r| r.block));
        // One page at a time: a page that comes back begins after the one before it was removed, and none is there beside another.
        let mut previous: Option<Option<usize>> = None;
        let mut one_page_at_a_time = true;
        for chain in &chains {
            let records = by_chain.get(chain).map_or(&[][..], Vec::as_slice);
            let first = records.first().map(|r| r.block);
            match (previous, first) {
                (None, _) => {}
                (Some(Some(end)), Some(first)) if first > end => {}
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
        let mut lines: Vec<String> = Vec::new();
        let mut hidden_lines = 0;
        let mut parent: Option<&str> = None;
        for chain in &chains {
            for record in by_chain.get(chain).map_or(&[][..], Vec::as_slice) {
                lines.push(record.json(parent).to_string());
                parent = Some(record.rev.as_str());
                if !record.hidden.is_empty() {
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
        }
        out.suppressed.insert(name.clone(), hidden_lines);
        out.files.insert(name, lines);
    }
    // A page of the tip with no record at all (a log that never says how it came to be) has no history file either, and says so.
    let followed: std::collections::BTreeSet<&str> = tracker
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
