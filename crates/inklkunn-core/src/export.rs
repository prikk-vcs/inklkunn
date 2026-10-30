//! Export (RFC 003 D-11; handoff 003 T7; FL-08, FL-11): a **Portable Wiki Bundle at fidelity level 3** as a
//! directory, built from one materialized tip.
//!
//! - **The files as stored**: every file of the default view's tip, byte for byte (`pages/`, `attachments/`,
//!   `knowledge/`, `governance/`, `.prikkignore`), read from **one `extract` pinned to the tip's block id**, so the
//!   bundle is one state and a write in between cannot mix two.
//! - **`history/<page id>.jsonl`**, one line per change to the page, oldest first, from `history()` and `change()`:
//!   the block, the operation, the kind of change and the **people of the message** (who proposed, who accepted,
//!   and the service for an import or a system change) as the authors. A change to a path that is not a page of
//!   the tip (a page since removed or renamed) has no file to belong to and is counted in the manifest's report,
//!   not dropped silently.
//! - **`users.yaml`**: the accounts that appear in those messages, with the names they had.
//! - **`wiki-bundle.yaml`**: the manifest, with the fidelity, the real counts, the block it is as of, the
//!   interwiki map and, in `checksums`, the name of the file below (wiki-commons' shape).
//! - **`SHA256SUMS`**, beside the manifest: one `"<sha256>  <path>"` line for every other file.
//!
//! The bundle is built beside its destination and renamed into place only when complete, and the destination
//! must not exist: a failed export leaves nothing. The extraction directory in the wiki's scratch directory is
//! removed whatever happened, and an export begins by sweeping what a killed one left (its extraction there,
//! its `.<name>.partial-*` beside the destination).

use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use inklkunn_prikk::{ExtractBounds, Point, Storage, StorageError};
use inklkunn_signer::message::{Actor, Message};
use inklkunn_store::{RootLayout, WikiRecord};
use sha2::{Digest, Sha256};

use crate::bundle::{self, ExportFacts, ManifestFacts, yaml_quote};

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
    /// Pages whose history file was not written because the page id in its frontmatter cannot name a file
    /// (unsafe) or belongs to more than one page (shared): the manifest says how many.
    pub histories_not_written: usize,
}

fn io(what: &str, e: &std::io::Error) -> ExportError {
    ExportError::Io(format!("{what} ({})", e.kind()))
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Remove a directory tree; a failure is the caller's to ignore (the next export sweeps its own leftovers).
fn remove_tree(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

/// Is `pid` a process that has ended? `false` for this process, for a live one, and whenever `/proc` cannot say
/// (no export is ever swept on a guess).
fn process_has_ended(pid: u32) -> bool {
    pid != std::process::id()
        && Path::new("/proc/self").exists()
        && !Path::new(&format!("/proc/{pid}")).exists()
}

/// The process id of a leftover named `<prefix><pid>-<nanos>`, when the name has exactly that shape.
fn leftover_pid(name: &str, prefix: &str) -> Option<u32> {
    let (pid, nanos) = name.strip_prefix(prefix)?.split_once('-')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if digits(pid) && digits(nanos) {
        pid.parse().ok()
    } else {
        None
    }
}

/// Remove the directories a killed export left: `export-<pid>-<nanos>` in `scratch` and
/// `.<name>.partial-<pid>-<nanos>` in `parent`, when the process that made them has ended (a live export of the
/// same wiki or destination, in another process, keeps its own). Only directories of exactly these names; nothing
/// is followed.
fn sweep_leftovers(scratch: &Path, parent: &Path, name: &str) {
    let partial_prefix = format!(".{name}.partial-");
    for (dir, prefix) in [(scratch, "export-"), (parent, partial_prefix.as_str())] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let file_name = entry.file_name();
            let is_leftover = file_name
                .to_str()
                .and_then(|n| leftover_pid(n, prefix))
                .is_some_and(process_has_ended);
            if is_leftover && entry.file_type().is_ok_and(|t| t.is_dir()) {
                remove_tree(&entry.path());
            }
        }
    }
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

/// The history lines of the pages of a tip, from a view's blocks at or below `tip`, oldest first.
struct Histories {
    /// page id → its lines.
    lines: BTreeMap<String, Vec<String>>,
    /// Changes to paths that are not pages of the tip.
    unattributed: usize,
    /// Accounts seen in messages: id → the names they had, in order of first sight.
    accounts: BTreeMap<String, Vec<String>>,
}

fn histories(
    storage: &dyn Storage,
    repo: &Path,
    view: &str,
    tip: &str,
    page_ids: &HashMap<String, String>,
) -> Result<Histories, ExportError> {
    let history = storage
        .history(
            repo,
            &Point::View(view.to_owned()),
            Some(MAX_HISTORY_BLOCKS),
        )
        .map_err(ExportError::Storage)?;
    if history.blocks.len() as u64 >= MAX_HISTORY_BLOCKS {
        return Err(ExportError::HistoryTooLong);
    }
    // Newest first; blocks newer than the tip (a write since the extraction) are not part of this bundle.
    let from = history
        .blocks
        .iter()
        .position(|b| b.block_id == tip)
        .ok_or_else(|| {
            ExportError::Io("the tip is not in the view's history (the view moved)".to_owned())
        })?;
    let mut out = Histories {
        lines: BTreeMap::new(),
        unattributed: 0,
        accounts: BTreeMap::new(),
    };
    for block in history.blocks.iter().skip(from).rev() {
        let change = storage
            .change(repo, &block.block_id)
            .map_err(ExportError::Storage)?;
        for patch in &change.patches {
            let message = block
                .patch_messages
                .iter()
                .find(|m| m.patch_id == patch.patch_id)
                .and_then(|m| Message::parse(&m.message).ok());
            let (kind, summary, authors, source) = match &message {
                Some(m) => {
                    for a in [m.proposed_by(), m.accepted_by()] {
                        if let Actor::Account { id, name } = a {
                            let names = out.accounts.entry(id.as_str().to_owned()).or_default();
                            if !names.iter().any(|n| n == name.as_str()) {
                                names.push(name.as_str().to_owned());
                            }
                        }
                    }
                    (
                        m.kind().as_str().to_owned(),
                        m.summary().as_str().to_owned(),
                        vec![
                            person("proposer", m.proposed_by()),
                            person("acceptor", m.accepted_by()),
                        ],
                        m.imported_from().map(|s| s.as_str().to_owned()),
                    )
                }
                // A change not made by inklkunn's contract has no people to name; say so rather than guess.
                None => (String::new(), String::new(), Vec::new(), None),
            };
            for op in &patch.operations {
                for path in op.paths.iter().filter_map(|p| p.path.as_deref()) {
                    let Some(id) = page_ids.get(path) else {
                        out.unattributed += 1;
                        continue;
                    };
                    let line = serde_json::json!({
                        "block": block.block_id,
                        "seq": block.update_seq,
                        "operation": op.kind,
                        "path": path,
                        "kind": kind,
                        "summary": summary,
                        "authors": authors,
                        "imported_from": source,
                        "message_readable": message.is_some(),
                    });
                    out.lines
                        .entry(id.clone())
                        .or_default()
                        .push(line.to_string());
                }
            }
        }
    }
    Ok(out)
}

/// Export the wiki as a bundle directory `dest` (absolute; must not exist; its directory must). `now` is the
/// time written in the manifest. The wiki's scratch directory is where the tip is extracted (and removed from).
pub fn export_bundle(
    storage: &dyn Storage,
    layout: &RootLayout,
    record: &WikiRecord,
    dest: &Path,
    inklkunn_version: &str,
    now: i64,
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
    let repo = layout.repo_dir(&record.id);
    let view = record.default_view.as_str();
    // One tip, by its block id.
    let tree = storage
        .tree(&repo, &Point::View(view.to_owned()), None)
        .map_err(ExportError::Storage)?;
    let tip = tree.target_block_id;
    // One extraction of it, in the wiki's scratch directory.
    let scratch = layout.scratch_dir(&record.id);
    std::fs::create_dir_all(&scratch)
        .map_err(|e| io("the wiki's scratch directory could not be made", &e))?;
    sweep_leftovers(&scratch, parent, name);
    let extraction = format!("export-{}-{}", std::process::id(), nanos());
    let result = (|| {
        storage
            .materialize(
                &repo,
                &Point::Block(tip.clone()),
                &scratch,
                &extraction,
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
        let partial = parent.join(format!(
            ".{name}.partial-{}-{}",
            std::process::id(),
            nanos()
        ));
        let built = build(
            storage,
            &repo,
            record,
            view,
            &tip,
            &scratch.join(&extraction),
            &partial,
            inklkunn_version,
            now,
        );
        match built {
            Ok(mut done) => {
                put_in_place(&partial, dest).inspect_err(|_| remove_tree(&partial))?;
                done.dir = dest.to_path_buf();
                Ok(done)
            }
            Err(e) => {
                remove_tree(&partial);
                Err(e)
            }
        }
    })();
    remove_tree(&scratch.join(&extraction));
    result
}

/// Move the finished bundle `partial` to `dest`, which must not exist. `rename(2)` replaces an empty directory,
/// and `RENAME_NOREPLACE` (which would refuse) is not reachable from `std`, so `dest` is first made here with
/// `mkdir` (which cannot succeed on anything that exists) and only that empty directory of ours is renamed over.
/// What is left is a window in which another process removes our empty directory and makes its own empty one.
fn put_in_place(partial: &Path, dest: &Path) -> Result<(), ExportError> {
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(dest)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                ExportError::Destination("it already exists; nothing was written".to_owned())
            } else {
                io("the bundle could not be put in place", &e)
            }
        })?;
    std::fs::rename(partial, dest).map_err(|e| {
        // `dest` is the empty directory this call made; `remove_dir` removes nothing else.
        let _ = std::fs::remove_dir(dest);
        io("the bundle could not be put in place", &e)
    })
}

/// May the history of the page id `id` be written as `history/<id>.jsonl`, given that `carried` pages of the tip
/// carry that id? A page id is a plain value from a person's frontmatter: it names a file only when it is safe to,
/// and only when exactly one page has it (a shared id cannot say whose history a file holds; an id no page of the
/// tip carries belongs to a page since removed). The ids that are refused are counted, never dropped silently.
fn history_file_allowed(id: &str, carried: usize) -> bool {
    let safe = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !id.starts_with('.');
    safe && carried == 1
}

#[allow(clippy::too_many_arguments)]
fn build(
    storage: &dyn Storage,
    repo: &Path,
    record: &WikiRecord,
    view: &str,
    tip: &str,
    extracted: &Path,
    partial: &Path,
    inklkunn_version: &str,
    now: i64,
) -> Result<Exported, ExportError> {
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(partial)
        .map_err(|e| io("the bundle directory could not be made", &e))?;
    let mut checksums: Vec<(String, String)> = Vec::new();
    let (mut pages, mut attachments) = (0, 0);
    let mut page_ids: HashMap<String, String> = HashMap::new();
    for rel in files_under(extracted)? {
        // The manifest is rewritten; everything else is the file as stored.
        if rel == bundle::MANIFEST {
            continue;
        }
        let from = extracted.join(&rel);
        let hash = copy_hashed(&from, &partial.join(&rel))?;
        checksums.push((rel.clone(), hash));
        if rel.starts_with("pages/") && rel.ends_with(".md") {
            pages += 1;
            if let Ok(text) = std::fs::read_to_string(&from)
                && let Ok(meta) = inklkunn_markup::parse_metadata(&text)
                && let Some(id) = meta.id
            {
                page_ids.insert(rel, id);
            }
        } else if rel.starts_with("attachments/")
            && !rel.ends_with(".keep")
            && !rel.ends_with(".meta.yaml")
        {
            attachments += 1;
        }
    }
    let h = histories(storage, repo, view, tip, &page_ids)?;
    let mut history_entries = 0;
    // How many pages carry each id: an id two pages share cannot say whose history a file holds.
    let mut carriers: HashMap<&str, usize> = HashMap::new();
    for id in page_ids.values() {
        *carriers.entry(id.as_str()).or_default() += 1;
    }
    let mut histories_not_written = 0;
    for (id, lines) in &h.lines {
        let carried = carriers.get(id.as_str()).copied().unwrap_or(0);
        if !history_file_allowed(id, carried) {
            // Counted in the manifest, never dropped silently.
            histories_not_written += carried;
            continue;
        }
        let mut text = lines.join("\n");
        text.push('\n');
        let rel = format!("history/{id}.jsonl");
        let hash = write_new(&partial.join(&rel), text.as_bytes())?;
        checksums.push((rel, hash));
        history_entries += lines.len();
    }
    let histories_written = checksums
        .iter()
        .filter(|(p, _)| p.starts_with("history/"))
        .count();
    // users.yaml: the accounts the messages name, with the names they had.
    let mut users = String::from("users:\n");
    if h.accounts.is_empty() {
        users = String::from("users: []\n");
    }
    for (id, names) in &h.accounts {
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
    let accounts = h.accounts.len();
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
        histories_not_written,
    })
}

#[cfg(test)]
mod tests;
