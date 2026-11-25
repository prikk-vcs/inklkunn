//! Export (RFC 003 D-11; handoff 003 T7; FL-08, FL-11): a **Portable Wiki Bundle at fidelity level 3** as a
//! directory, built from one materialized tip.
//!
//! - **The files as stored**: every file of the default view's tip, byte for byte (`pages/`, `attachments/`,
//!   `knowledge/`, `governance/`, `.prikkignore`), read from **one `extract` pinned to the tip's block id**, so the
//!   bundle is one state and a write in between cannot mix two.
//! - **`history/<page id>.jsonl`**, one **wiki-commons history record** per revision of the page, oldest first (RFC 007 D-7;
//!   XFER-7; the module `history` says what each carries): `rev` (the block's id), `parent`, `at`, `type`,
//!   `author`, `summary`, **the page's full text** (`content` and its `sha256`, read through the one reader of a page's text
//!   at a block) and inklkunn's own facts under one `inklkunn` key. A page since removed or renamed has its file too, by its
//!   page id; a change to a path that is not a page (an attachment, a rule) has no file to belong to and is counted in the
//!   manifest's report, not dropped silently.
//! - **`extensions/inklkunn-knowledge/`** (RFC 005 part 2 D-11, D-12; the knowledge extension, declared in the manifest's
//!   `extensions_used` as `inklkunn-knowledge`, version 1): what the knowledge layer keeps in the service's records and
//!   not in the repository. `threads/<claim id>.jsonl` is one file per claim whose thread has comments, one line per
//!   comment in the order written (`id`, `parent`, `at`, `author`, `body`), the body **a fenced text block** (the info
//!   string `text`, line endings LF) so that every Markdown reader of the bundle sees exactly the plain text inklkunn
//!   showed and no construct inside it ([`crate::bundle::markdown_text`]); `views.jsonl`
//!   is one line per alternative view that was created (its name, kind, the view and block it was forked from, who
//!   created it and when). A claim's thread is not a page's discussion (XFER-10 is one file per page), so nothing of it
//!   is under `discussions/` and the manifest's `discussions` stays a statement about page discussions. A thread never
//!   changes a claim: the bundle carries it beside the claim's file, not instead of it. All are listed in `sha256sums.txt`
//!   like every other file. (Comments on proposals are not exported here.)
//! - **What moderation hides is not in it** (RFC 006 D-13; [`Suppressions`]): an entry with a hidden part is marked
//!   `suppressed` with its `reasons` and omits that part (a history record's `content`, `summary` or `author`, a thread
//!   comment's `body` or `author`), the manifest declares that the bundle carries such entries, and `users.yaml` lists a person
//!   only for what is not hidden.
//! - **`users.yaml`**: the accounts that appear in those messages and in those threads, with the names they had.
//! - **`wiki-bundle.yaml`**: the manifest, with the fidelity, the real counts, the block it is as of, the
//!   interwiki map and, in `checksums`, the name of the file below (wiki-commons' shape).
//! - **`sha256sums.txt`** (wiki-commons' conventional name), beside the manifest: one `"<sha256>  <path>"` line for every other file.
//!
//! The bundle is built beside its destination and renamed into place only when complete, and the destination
//! must not exist: a failed export leaves nothing. The extraction directory in the wiki's scratch directory is
//! removed whatever happened. Both directories (the extraction, and `.<name>.partial-*` beside the destination) are
//! held by a lock file for as long as the export lives ([`crate::read::scratch`]), and an export begins by sweeping
//! what a killed one left there: only a directory whose lock is free, never one decided dead from `/proc` (K65). Beside
//! the destination, which is a person's directory, only a partial whose lock file already exists is swept (K74).

use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use inklkunn_prikk::storage::Entry;
use inklkunn_prikk::{ExtractBounds, Materialized, Point, Storage, StorageError};
use inklkunn_signer::message::Actor;
use inklkunn_store::{
    AcceptanceStore, AccountStore, ActStore, ClaimCommentRow, ClaimThreadStore, GovernanceStore,
    RootLayout, ViewActOutcome, WikiId, WikiRecord,
};
use sha2::{Digest, Sha256};

use crate::bundle::{self, ExportFacts, ManifestFacts, markdown_text, yaml_quote};
use crate::knowledge::ClaimId;
use crate::read::scratch::{Extract, Partial, partial_name_too_long_by};

mod history;
mod suppression;

pub use history::{ExportNote, HistoryInputs, INDEX_WAIT_ROUNDS, MAX_HISTORY_TEXT_BYTES};
pub use suppression::{Suppressed, Suppressions};

/// How many blocks of history an export reads (`log --limit`). A history longer than this is refused rather than
/// cut short: an export that silently lacks its oldest changes would be a lie about the wiki.
pub const MAX_HISTORY_BLOCKS: u64 = 1_000_000;

/// Why an export did not happen. Nothing is left at the destination in any case.
#[derive(Debug)]
pub enum ExportError {
    /// The destination is unusable (relative, exists, its directory does not).
    Destination(String),
    /// prikk's checkpoint record is missing, so `extract` refused (ledger PK-24): the checkpoint refresh
    /// (the signer's keyless mode) re-records it, and the export can be run again.
    CheckpointNotRecorded,
    /// A storage call failed.
    Storage(StorageError),
    /// The history is longer than [`MAX_HISTORY_BLOCKS`].
    HistoryTooLong,
    /// The pages' texts at their revisions, which the history carries whole, are more than [`MAX_HISTORY_TEXT_BYTES`].
    HistoryTextTooLarge,
    /// The index of who wrote each page's text did not settle in [`INDEX_WAIT_ROUNDS`] rounds: a text that cannot be told from a
    /// hidden one is never written, and a revision is never left out.
    IndexNotSettled {
        /// The rounds the export waited.
        rounds: usize,
    },
    /// A file operation failed (what, and the kind of failure).
    Io(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Destination(why) => {
                write!(f, "the export's destination is unusable: {why}")
            }
            ExportError::CheckpointNotRecorded => write!(
                f,
                "prikk's checkpoint record is missing, so the wiki's files cannot be extracted; refresh it (the server does at start, or run the export again after `inklkunn wiki export`'s own refresh) and retry"
            ),
            ExportError::Storage(e) => write!(f, "the wiki could not be read: {e}"),
            ExportError::HistoryTooLong => write!(
                f,
                "the history is longer than {MAX_HISTORY_BLOCKS} blocks; an export that lacked its oldest changes would be false, so none was made"
            ),
            ExportError::HistoryTextTooLarge => write!(
                f,
                "the pages' texts at their revisions are more than {MAX_HISTORY_TEXT_BYTES} bytes in all; an export that left revisions out would be false, so none was made"
            ),
            ExportError::IndexNotSettled { rounds } => write!(
                f,
                "the index of who wrote each page's text is not built yet (the export waited {rounds} rounds for it), so a text that moderation hides cannot be told from one it does not, and none was written; run the export again in a little while (a running server builds the index in the background)"
            ),
            ExportError::Io(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Is this the refusal the checkpoint refresh answers (`no-checkpoint-in-reach`, reason `not-recorded`)?
pub fn is_checkpoint_not_recorded(e: &StorageError) -> bool {
    matches!(
        e,
        StorageError::Refused { kind: Some(kind), reason: Some(reason), .. }
            if kind == "no-checkpoint-in-reach" && reason == "not-recorded"
    )
}

/// The person who wrote a comment, as the bundle carries them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentAuthor {
    /// The account id.
    pub id: String,
    /// The name as the account has it now.
    pub name: String,
}

/// One comment of a claim's thread, as the bundle's knowledge extension carries it. **What moderation hides is not held
/// here at all:** a hidden body or author is `None` and the comment says what is [`suppressed`](Self::suppressed), so no
/// later step can write what it was never given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscussionComment {
    /// The comment's id.
    pub id: String,
    /// The comment it answers, in the same thread.
    pub parent: Option<String>,
    /// Unix seconds.
    pub at: i64,
    /// The author; `None` when the author is hidden.
    pub author: Option<CommentAuthor>,
    /// The text, as it was written (the bundle carries it as a fenced text block); `None` when the content is hidden.
    pub body: Option<String>,
    /// What moderation hides of this comment, with the reasons.
    pub suppressed: Suppressed,
}

/// One claim's thread, whole, for the bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimDiscussion {
    /// The claim's id: a checked one, so that the file it names is built from nothing else.
    pub claim: ClaimId,
    /// Its comments, in the order written.
    pub comments: Vec<DiscussionComment>,
}

/// How an alternative view came to be, as the bundle's knowledge extension carries it (RFC 005 part 2 D-9, D-12): the
/// service's record of the act. The view's files are not in the bundle (a bundle is the default view's tip); this is the
/// fact that the view exists and where it was forked from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewCreation {
    /// The record's id.
    pub id: String,
    /// The view's short name.
    pub name: String,
    /// `experimental` or `interpretation`.
    pub kind: String,
    /// The short name of the view it was forked from.
    pub forked_from: String,
    /// The block that view's tip was when the fork was made.
    pub from_block: String,
    /// The account that created it.
    pub author_id: String,
    /// That account's name as it has it now.
    pub author_name: String,
    /// Unix seconds.
    pub at: i64,
}

/// What the knowledge extension of a bundle is made of: the service's records that are not files of the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnowledgeRecords {
    /// Every claim thread, by claim id.
    pub threads: Vec<ClaimDiscussion>,
    /// Every alternative view that was created, oldest first.
    pub views: Vec<ViewCreation>,
    /// What moderation hides of the wiki's page revisions: the history lines are written without it.
    pub suppressions: Suppressions,
}

/// The names accounts have, remembered for the people of one export.
struct Names<'a> {
    accounts: &'a dyn AccountStore,
    seen: HashMap<String, String>,
}

impl Names<'_> {
    fn of(&mut self, id: &inklkunn_store::AccountId) -> Result<String, ExportError> {
        let key = id.to_string();
        if let Some(name) = self.seen.get(&key) {
            return Ok(name.clone());
        }
        let name = self
            .accounts
            .account(id)
            .map_err(|_| ExportError::Io("an account could not be read".to_owned()))?
            .map_or_else(String::new, |a| a.display_name.as_str().to_owned());
        self.seen.insert(key, name.clone());
        Ok(name)
    }
}

/// Every claim thread of the wiki, whole and in order, with the names of the people who wrote: what an export carries as
/// the knowledge extension's threads. Read from the service's records; nothing is read from a repository. A record whose
/// claim id is not the spelling of one is an error, never a path. **A comment whose body or author is hidden by moderation
/// is returned without it** (RFC 006 D-13), and the person is not looked up.
pub fn claim_discussions(
    threads: &dyn ClaimThreadStore,
    accounts: &dyn AccountStore,
    suppressions: &Suppressions,
    wiki: &WikiId,
) -> Result<Vec<ClaimDiscussion>, ExportError> {
    let rows = threads
        .claim_comments_in_wiki(wiki)
        .map_err(|_| ExportError::Io("the claim threads could not be read".to_owned()))?;
    let mut names = Names {
        accounts,
        seen: HashMap::new(),
    };
    let mut out: Vec<ClaimDiscussion> = Vec::new();
    for ClaimCommentRow { comment, .. } in rows {
        let claim = ClaimId::parse(&comment.claim).ok_or_else(|| {
            ExportError::Io(
                "a claim thread is recorded under an id that is not a claim's".to_owned(),
            )
        })?;
        let id = comment.id.to_string();
        let suppressed = suppressions.of_claim_comment(&id);
        let author = if suppressed.author.is_some() {
            None
        } else {
            Some(CommentAuthor {
                name: names.of(&comment.author)?,
                id: comment.author.to_string(),
            })
        };
        let line = DiscussionComment {
            parent: comment.parent.as_ref().map(ToString::to_string),
            at: comment.created_at,
            author,
            body: suppressed.content.is_none().then_some(comment.body),
            suppressed,
            id,
        };
        match out.last_mut() {
            Some(d) if d.claim == claim => d.comments.push(line),
            _ => out.push(ClaimDiscussion {
                claim,
                comments: vec![line],
            }),
        }
    }
    Ok(out)
}

/// Every alternative view of the wiki that was created, oldest first: the service's record of each (an act that failed
/// took nothing in, and one that was interrupted is not known to have made a view, so neither is listed).
pub fn view_creations(
    acts: &dyn ActStore,
    accounts: &dyn AccountStore,
    wiki: &WikiId,
) -> Result<Vec<ViewCreation>, ExportError> {
    let entries = acts
        .view_acts(wiki)
        .map_err(|_| ExportError::Io("the records of the views could not be read".to_owned()))?;
    let mut names = Names {
        accounts,
        seen: HashMap::new(),
    };
    let mut out = Vec::new();
    for entry in entries {
        if !matches!(
            entry.end.as_ref().map(|e| &e.outcome),
            Some(ViewActOutcome::Created)
        ) {
            continue;
        }
        let act = entry.act;
        out.push(ViewCreation {
            id: act.id.to_string(),
            name: act.name,
            kind: act.kind.as_str().to_owned(),
            forked_from: act.forked_from,
            from_block: act.from_block,
            author_name: names.of(&act.created_by)?,
            author_id: act.created_by.to_string(),
            at: act.created_at,
        });
    }
    Ok(out)
}

/// The service's records an export carries in the knowledge extension: the claim threads and the views' creation records,
/// and what moderation hides ([`Suppressions::read`] reads every hide and unhide of the wiki, `acceptances` to find the
/// block a proposal's accepted revision became). A hide that cannot be read fails the read: no export is made without it.
pub fn knowledge_records(
    threads: &dyn ClaimThreadStore,
    acts: &dyn ActStore,
    accounts: &dyn AccountStore,
    governance: &dyn GovernanceStore,
    acceptances: &dyn AcceptanceStore,
    wiki: &WikiId,
) -> Result<KnowledgeRecords, ExportError> {
    let suppressions = Suppressions::read(governance, acceptances, wiki)?;
    Ok(KnowledgeRecords {
        threads: claim_discussions(threads, accounts, &suppressions, wiki)?,
        views: view_creations(acts, accounts, wiki)?,
        suppressions,
    })
}

/// What an export made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    /// The bundle directory.
    pub dir: PathBuf,
    /// The block the files are as of.
    pub tip: String,
    /// Pages.
    pub pages: usize,
    /// Attachments.
    pub attachments: usize,
    /// Pages that have a history file.
    pub histories: usize,
    /// Lines in all history files.
    pub history_entries: usize,
    /// Changes to paths that are not pages of the tip (no file to belong to).
    pub unattributed_changes: usize,
    /// Accounts in `users.yaml`.
    pub users: usize,
    /// Claim threads written as `extensions/inklkunn-knowledge/threads/<claim id>.jsonl`.
    pub threads: usize,
    /// Views whose creation was written to `extensions/inklkunn-knowledge/views.jsonl`.
    pub views: usize,
    /// Entries (history lines and thread comments) written with a hidden part omitted and marked `suppressed`; the manifest
    /// declares them when there are any.
    pub suppressed: usize,
    /// Claim and source files of `knowledge/`, carried as they are stored.
    pub knowledge_files: usize,
    /// Pages whose history file was not written because the page id in its frontmatter cannot name a file
    /// (unsafe) or belongs to more than one page (shared): the manifest says how many.
    pub histories_not_written: usize,
}

fn io(what: &str, e: &std::io::Error) -> ExportError {
    ExportError::Io(format!("{what} ({})", e.kind()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Every regular file under `dir`, as `/`-separated relative paths in order. A link or any other kind of entry
/// is an error: prikk's `extract` writes files and directories only.
fn files_under(dir: &Path) -> Result<Vec<String>, ExportError> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> Result<(), ExportError> {
        let mut names: Vec<(std::ffi::OsString, std::fs::FileType)> = Vec::new();
        for e in
            std::fs::read_dir(dir).map_err(|e| io("the extracted files could not be listed", &e))?
        {
            let e = e.map_err(|e| io("the extracted files could not be listed", &e))?;
            let t = e
                .file_type()
                .map_err(|e| io("an extracted file could not be examined", &e))?;
            names.push((e.file_name(), t));
        }
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, t) in names {
            let name = name
                .to_str()
                .ok_or_else(|| ExportError::Io("an extracted file's name is not UTF-8".to_owned()))?
                .to_owned();
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if t.is_dir() {
                walk(&dir.join(&name), &path, out)?;
            } else if t.is_file() {
                out.push(path);
            } else {
                return Err(ExportError::Io(
                    "the extraction holds an entry that is neither a file nor a directory"
                        .to_owned(),
                ));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out)?;
    Ok(out)
}

/// Copy `from` to the new file `to` (mode 0600), returning the SHA-256 of what was copied.
fn copy_hashed(from: &Path, to: &Path) -> Result<String, ExportError> {
    use std::io::Read as _;
    let mut src =
        std::fs::File::open(from).map_err(|e| io("an extracted file could not be read", &e))?;
    if let Some(parent) = to.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|e| io("a bundle directory could not be made", &e))?;
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(to)
        .map_err(|e| io("a bundle file could not be created", &e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; 64 * 1024];
    loop {
        let n = src
            .read(&mut buf)
            .map_err(|e| io("an extracted file could not be read", &e))?;
        if n == 0 {
            break;
        }
        let chunk = buf.get(..n).unwrap_or_default();
        hasher.update(chunk);
        out.write_all(chunk)
            .map_err(|e| io("a bundle file could not be written", &e))?;
    }
    out.sync_all()
        .map_err(|e| io("a bundle file could not be made durable", &e))?;
    Ok(hex(&hasher.finalize()))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<String, ExportError> {
    if let Some(parent) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|e| io("a bundle directory could not be made", &e))?;
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| io("a bundle file could not be created", &e))?;
    out.write_all(bytes)
        .map_err(|e| io("a bundle file could not be written", &e))?;
    out.sync_all()
        .map_err(|e| io("a bundle file could not be made durable", &e))?;
    Ok(hex(&Sha256::digest(bytes)))
}

/// A person in a history line.
fn person(role: &str, actor: &Actor) -> serde_json::Value {
    match actor {
        Actor::Account { id, name } => serde_json::json!({
            "role": role, "type": "account", "id": id.as_str(), "name": name.as_str(),
        }),
        Actor::Anonymous(p) => serde_json::json!({
            "role": role, "type": "anonymous", "name": p.as_str(),
        }),
        Actor::System(label) => serde_json::json!({
            "role": role, "type": "system", "name": label.as_str(),
        }),
    }
}

/// Export the wiki as a bundle directory `dest` (absolute; must not exist; its directory must). `now` is the
/// time written in the manifest. The wiki's scratch directory is where the tip is extracted (and removed from). The
/// bundle carries none of the service's records (claim threads, views), and its manifest says so in `fidelity.dropped`.
/// **It is not told what moderation hides either**, so its history lines are written whole: **a function that ignores
/// moderation is a trap, so it exists in this crate's own unit tests alone** (handoff G-T4, decision request 13). A wiki is
/// exported with [`export_bundle_with`] and the records [`knowledge_records`] reads.
#[cfg(test)]
pub(crate) fn export_bundle(
    storage: &dyn Storage,
    layout: &RootLayout,
    record: &WikiRecord,
    dest: &Path,
    inklkunn_version: &str,
    now: i64,
    inputs: &mut HistoryInputs<'_>,
) -> Result<Exported, ExportError> {
    export_bundle_inner(
        storage,
        layout,
        record,
        dest,
        inklkunn_version,
        now,
        None,
        inputs,
    )
}

/// Export the wiki as a bundle directory `dest` (as the tests' own `export_bundle` does), carrying these records in the bundle's
/// knowledge extension ([`knowledge_records`] reads them): the claim threads, the views' creation records and **what moderation
/// hides**, which the bundle then carries as tombstones and never as the hidden words (RFC 006 D-13). `inputs` is what the history
/// records need besides the repository ([`HistoryInputs`]): the one reader of a page's text at a block, and the record of when each
/// block was first seen.
#[allow(clippy::too_many_arguments)]
pub fn export_bundle_with(
    storage: &dyn Storage,
    layout: &RootLayout,
    record: &WikiRecord,
    dest: &Path,
    inklkunn_version: &str,
    now: i64,
    records: &KnowledgeRecords,
    inputs: &mut HistoryInputs<'_>,
) -> Result<Exported, ExportError> {
    export_bundle_inner(
        storage,
        layout,
        record,
        dest,
        inklkunn_version,
        now,
        Some(records),
        inputs,
    )
}

#[allow(clippy::too_many_arguments)]
fn export_bundle_inner(
    storage: &dyn Storage,
    layout: &RootLayout,
    record: &WikiRecord,
    dest: &Path,
    inklkunn_version: &str,
    now: i64,
    records: Option<&KnowledgeRecords>,
    inputs: &mut HistoryInputs<'_>,
) -> Result<Exported, ExportError> {
    if !dest.is_absolute() {
        return Err(ExportError::Destination(
            "it must be an absolute path".to_owned(),
        ));
    }
    let parent = dest
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| ExportError::Destination("its directory does not exist".to_owned()))?;
    if std::fs::symlink_metadata(dest).is_ok() {
        return Err(ExportError::Destination(
            "it already exists; nothing was written".to_owned(),
        ));
    }
    let name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ExportError::Destination("its name is not UTF-8".to_owned()))?;
    // The bundle is built in `.<name>.partial-<tail>` beside the destination, with a lock file beside that: a name the file
    // system could not hold is said so, with the bytes to cut, before anything is read or made.
    if let Some(over) = partial_name_too_long_by(name) {
        return Err(ExportError::Destination(format!(
            "the name is too long by {over} bytes"
        )));
    }
    let repo = layout.repo_dir(&record.id);
    let view = record.default_view.as_str();
    // One tip, by its block id.
    let tree = storage
        .tree(&repo, &Point::View(view.to_owned()), None)
        .map_err(ExportError::Storage)?;
    let tip = tree.target_block_id;
    let listed = tree.entries;
    // One extraction of it, in the wiki's scratch directory: claimed by a lock, which holds until the extraction is
    // removed (at the end of this function, whatever happened), so no other process's sweep takes it from under the
    // export, and a killed export's is swept by the next one.
    let scratch = layout.scratch_dir(&record.id);
    std::fs::create_dir_all(&scratch)
        .map_err(|e| io("the wiki's scratch directory could not be made", &e))?;
    let extraction = Extract::begin(&scratch)
        .map_err(|e| io("the wiki's scratch directory could not be used", &e))?;
    let extracted = storage
        .materialize(
            &repo,
            &Point::Block(tip.clone()),
            &scratch,
            extraction.name(),
            ExtractBounds {
                max_bytes: None,
                max_files: None,
            },
        )
        .map_err(|e| {
            if is_checkpoint_not_recorded(&e) {
                ExportError::CheckpointNotRecorded
            } else {
                ExportError::Storage(e)
            }
        })?;
    // The bundle is built in a directory beside the destination that is held the same way. Dropping it removes what is
    // left of it (nothing, once the bundle is in place).
    let partial = Partial::begin(parent, name)
        .map_err(|e| io("the destination's directory could not be used", &e))?;
    let mut done = build(
        storage,
        &repo,
        record,
        view,
        &tip,
        &extraction.root(),
        &extracted,
        &listed,
        &partial.path(),
        inklkunn_version,
        now,
        records,
        inputs,
    )?;
    put_in_place(&partial.path(), dest)?;
    done.dir = dest.to_path_buf();
    Ok(done)
}

/// Move the finished bundle `partial` to `dest`, which must not exist. One `renameat2(2)` with `RENAME_NOREPLACE`:
/// the kernel refuses the move when anything is at `dest`, an empty directory and a link included, and does it
/// in the same step otherwise, so there is no window between the check and the move (a plain `rename` replaces
/// an empty directory). A file system that cannot do it answers with an error, and nothing is moved.
fn put_in_place(partial: &Path, dest: &Path) -> Result<(), ExportError> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        partial,
        rustix::fs::CWD,
        dest,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(placement_error)
}

/// What a refused `renameat2` says. `EXIST`: something is at the destination. `EINVAL` and `ENOTSUP`: the file
/// system cannot move without replacing, so the move was not tried, and that is said in words and not as an
/// opaque kind. Anything else is the file operation's own failure.
fn placement_error(errno: rustix::io::Errno) -> ExportError {
    if errno == rustix::io::Errno::EXIST {
        ExportError::Destination("it already exists; nothing was written".to_owned())
    } else if errno == rustix::io::Errno::INVAL || errno == rustix::io::Errno::NOTSUP {
        ExportError::Io(
            "this file system cannot place the bundle without the risk of replacing an existing one; nothing was written"
                .to_owned(),
        )
    } else {
        io(
            "the bundle could not be put in place",
            &std::io::Error::from(errno),
        )
    }
}

/// May the history of the page id `id` be written as `history/<id>.jsonl`, given that `carried` pages of the tip
/// carry that id? A page id is a plain value from a person's frontmatter: it names a file only when it is safe to,
/// and only when no more than one page of the tip has it (a shared id cannot say whose history a file holds; an id no page of
/// the tip carries belongs to a page since removed or renamed, which has its history as the pages of the tip have theirs). The
/// ids that are refused are counted, never dropped silently.
fn history_file_allowed(id: &str, carried: usize) -> bool {
    let safe = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !id.starts_with('.');
    safe && carried <= 1
}

#[allow(clippy::too_many_arguments)]
fn build(
    storage: &dyn Storage,
    repo: &Path,
    record: &WikiRecord,
    view: &str,
    tip: &str,
    extracted: &Path,
    report: &Materialized,
    listed: &[Entry],
    partial: &Path,
    inklkunn_version: &str,
    now: i64,
    records: Option<&KnowledgeRecords>,
    inputs: &mut HistoryInputs<'_>,
) -> Result<Exported, ExportError> {
    let no_suppressions = Suppressions::default();
    let suppressions = records.map_or(&no_suppressions, |r| &r.suppressions);
    // The bundle is of the files the tip has: a tree with a file missing (or one more, or one of another size) is not that
    // tree, and a bundle of it, with checksums that match what it holds, would be a false statement about the wiki. The
    // witness is the listing of the tip's files, path by path with their sizes (the totals alone pass a missing file and an
    // extra one of the same size), and prikk's own report of the extraction, which must agree in its totals.
    let files = files_under(extracted)?;
    let mut held: BTreeMap<&str, u64> = BTreeMap::new();
    let mut bytes = 0_u64;
    for rel in &files {
        let len = std::fs::symlink_metadata(extracted.join(rel))
            .map_err(|e| io("an extracted file could not be examined", &e))?
            .len();
        bytes += len;
        held.insert(rel.as_str(), len);
    }
    let mut expected: BTreeMap<&str, Option<u64>> = BTreeMap::new();
    for entry in listed.iter().filter(|e| e.kind == "file") {
        expected.insert(entry.path.as_str(), entry.size);
    }
    let differs = |path: &str| -> String {
        format!(
            "the extraction does not hold the files the tip lists (`{path}` is missing, extra or of another size); nothing was written"
        )
    };
    for (path, len) in &held {
        if expected.get(path) != Some(&Some(*len)) {
            return Err(ExportError::Io(differs(path)));
        }
    }
    if let Some(path) = expected.keys().find(|path| !held.contains_key(*path)) {
        return Err(ExportError::Io(differs(path)));
    }
    if files.len() as u64 != report.file_count || bytes != report.total_bytes {
        return Err(ExportError::Io(format!(
            "the extraction holds {} files of {bytes} bytes and prikk reported {} files of {} bytes; nothing was written",
            files.len(),
            report.file_count,
            report.total_bytes
        )));
    }
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(partial)
        .map_err(|e| io("the bundle directory could not be made", &e))?;
    let mut checksums: Vec<(String, String)> = Vec::new();
    let (mut pages, mut attachments, mut knowledge_files) = (0, 0, 0);
    let mut page_ids: HashMap<String, Option<String>> = HashMap::new();
    for rel in files {
        // The manifest is rewritten; everything else is the file as stored.
        if rel == bundle::MANIFEST {
            continue;
        }
        let from = extracted.join(&rel);
        let hash = copy_hashed(&from, &partial.join(&rel))?;
        checksums.push((rel.clone(), hash));
        if rel.starts_with("pages/") && rel.ends_with(".md") {
            pages += 1;
            // The id the page's frontmatter gives it, if it has one (a page made in the editor has none: its history file is named
            // by its path).
            let id = std::fs::read_to_string(&from)
                .ok()
                .and_then(|text| inklkunn_markup::parse_metadata(&text).ok())
                .and_then(|meta| meta.id);
            page_ids.insert(rel, id);
        } else if rel.starts_with("attachments/")
            && !rel.ends_with(".keep")
            && !rel.ends_with(".meta.yaml")
        {
            attachments += 1;
        } else if crate::knowledge::is_knowledge_path(&rel) && !crate::knowledge::is_keep(&rel) {
            // `knowledge/` is carried as it is stored (RFC 005 part 2 D-12): readable YAML beside readable pages.
            knowledge_files += 1;
        }
    }
    let h = history::histories(
        storage,
        repo,
        record,
        view,
        tip,
        &page_ids,
        suppressions,
        now,
        inputs,
    )?;
    let mut suppressed_entries = 0;
    let mut history_entries = 0;
    // Pages whose file was not written are counted in the manifest, never dropped silently.
    let histories_not_written = h.not_written;
    for (id, lines) in &h.files {
        let mut text = lines.join("\n");
        text.push('\n');
        let rel = format!("history/{id}.jsonl");
        let hash = write_new(&partial.join(&rel), text.as_bytes())?;
        checksums.push((rel, hash));
        history_entries += lines.len();
        suppressed_entries += h.suppressed.get(id).copied().unwrap_or(0);
    }
    // The knowledge extension's records (RFC 005 part 2 D-11, D-12), not under `discussions/`: a claim's thread is not a
    // page's discussion. The people who wrote or created join the accounts of users.yaml.
    let mut accounts = h.accounts;
    let mut remember = |id: &str, name: &str| {
        let names = accounts.entry(id.to_owned()).or_default();
        if !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
    };
    let (mut threads_written, mut views_written) = (0, 0);
    for thread in records.map_or(&[][..], |r| r.threads.as_slice()) {
        let mut text = String::new();
        for c in &thread.comments {
            let mut line = serde_json::json!({
                "id": c.id,
                "parent": c.parent,
                "at": inklkunn_store::record::rfc3339(c.at),
            });
            if let Some(members) = line.as_object_mut() {
                // What is hidden was never given to this function (`claim_discussions`), and what is not given is not written.
                if let Some(author) = &c.author {
                    remember(&author.id, &author.name);
                    members.insert(
                        "author".to_owned(),
                        serde_json::json!({"id": author.id, "name": author.name}),
                    );
                }
                if let Some(body) = &c.body {
                    // A fenced text block that shows the plain text it was written as.
                    members.insert("body".to_owned(), markdown_text(body).into());
                }
                c.suppressed.mark(members);
            }
            if !c.suppressed.is_empty() {
                suppressed_entries += 1;
            }
            text.push_str(&line.to_string());
            text.push('\n');
        }
        let rel = format!("{}/{}.jsonl", bundle::KNOWLEDGE_THREADS_DIR, thread.claim);
        let hash = write_new(&partial.join(&rel), text.as_bytes())?;
        checksums.push((rel, hash));
        threads_written += 1;
    }
    let views = records.map_or(&[][..], |r| r.views.as_slice());
    if !views.is_empty() {
        let mut text = String::new();
        for v in views {
            remember(&v.author_id, &v.author_name);
            text.push_str(
                &serde_json::json!({
                    "id": v.id,
                    "name": v.name,
                    "kind": v.kind,
                    "forked_from": v.forked_from,
                    "from_block": v.from_block,
                    "at": inklkunn_store::record::rfc3339(v.at),
                    "created_by": {"id": v.author_id, "name": v.author_name},
                })
                .to_string(),
            );
            text.push('\n');
            views_written += 1;
        }
        let hash = write_new(&partial.join(bundle::KNOWLEDGE_VIEWS_FILE), text.as_bytes())?;
        checksums.push((bundle::KNOWLEDGE_VIEWS_FILE.to_owned(), hash));
    }
    let histories_written = checksums
        .iter()
        .filter(|(p, _)| p.starts_with("history/"))
        .count();
    // users.yaml: the accounts the messages name, with the names they had.
    let mut users = String::from("users:\n");
    if accounts.is_empty() {
        users = String::from("users: []\n");
    }
    for (id, names) in &accounts {
        users.push_str(&format!("  - id: {}\n", yaml_quote(id)));
        users.push_str(&format!(
            "    names: [{}]\n",
            names
                .iter()
                .map(|n| yaml_quote(n))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let hash = write_new(&partial.join("users.yaml"), users.as_bytes())?;
    checksums.push(("users.yaml".to_owned(), hash));
    checksums.sort();
    let sums = bundle::sha256sums(&checksums).ok_or_else(|| {
        ExportError::Io(
            "a file's name holds a line break; it cannot be listed in the checksums".to_owned(),
        )
    })?;
    write_new(&partial.join(bundle::CHECKSUMS), sums.as_bytes())?;
    let accounts = accounts.len();
    let manifest = bundle::manifest_with(
        record,
        inklkunn_version,
        &ManifestFacts {
            exported: Some(ExportFacts {
                pages,
                attachments,
                exported_at: now,
                tip: tip.to_owned(),
                interwiki: Vec::new(),
                unattributed_changes: h.unattributed,
                histories_not_written,
                knowledge_extension: knowledge_files > 0
                    || threads_written > 0
                    || views_written > 0,
                records_read: records.is_some(),
                suppressed_entries,
            }),
        },
    );
    write_new(&partial.join(bundle::MANIFEST), manifest.as_bytes())?;
    Ok(Exported {
        dir: partial.to_path_buf(),
        tip: tip.to_owned(),
        pages,
        attachments,
        histories: histories_written,
        history_entries,
        unattributed_changes: h.unattributed,
        users: accounts,
        threads: threads_written,
        views: views_written,
        suppressed: suppressed_entries,
        knowledge_files,
        histories_not_written,
    })
}

#[cfg(test)]
mod tests;
