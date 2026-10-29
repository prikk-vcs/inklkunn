//! `import-folder` (RFC 003 D-12; handoff 003 T7; internal design IQ-6): a folder of pages and attachments
//! becomes **one accepted change** under the system principal (`Kind: import`), through RFC 002's [`Accept`], so
//! the read path ships with content before the proposal loop exists.
//!
//! **A dry run is the plan.** [`plan_folder`] walks the folder without following a link, reads every page (and
//! nothing else), and says for each entry what would be written or why it is refused: a path over prikk's caps
//! (`inklkunn_markup::path`), a name that is not UTF-8 or not ASCII, a name a repository refuses (the signer's own
//! rule, [`inklkunn_signer::check_change_path`]), a link, a device or a pipe, a page that is not
//! UTF-8 text or whose frontmatter the parser refuses, a duplicate page id, a path that collides with another on a
//! case-insensitive file system, a path the wiki already has. **Any refusal refuses the whole import** (the
//! change is all or nothing), every reason is named, and the report never repeats a byte of a refused file's
//! content. Running the import is the same plan, then the same checks again on the bytes actually read, then
//! [`Accept`]; the signer validates once more before it writes a byte.
//!
//! **What a folder is.** The layout an export writes: `pages/**/*.md` and `attachments/**`. Everything else at the
//! top (the manifest, `users.yaml`, `history/`, `knowledge/`, `governance/`) is *not imported* and is listed as
//! such: a Portable Wiki Bundle's import with its history is RFC 007's.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use inklkunn_markup::path::{MAX_COMPONENT_BYTES, MAX_PATH_BYTES};
use inklkunn_prikk::{Point, Storage, StorageError};
use inklkunn_signer::message::{Actor, ImportSource, Kind, Message, Summary, SystemLabel};
use inklkunn_signer::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ChangeDigest, ExpectedMessage,
    LedgerError, MAX_CHANGE_BYTES, MAX_CHANGE_FILES, PreparedChange, PreparedFile, SealedChange,
    ViewCreationAct, ViewCreationFacts, check_change_path,
};
use inklkunn_store::WikiRecord;

use crate::read::ReadConfig;

/// Folders nested deeper than this are refused (a path cap would refuse them anyway; this bounds the walk).
const MAX_DEPTH: usize = 48;

/// Top-level entries of an exported bundle that `import-folder` does not read, and why.
const NOT_IMPORTED: &[(&str, &str)] = &[
    (
        "wiki-bundle.yaml",
        "the manifest describes the export; the wiki keeps its own",
    ),
    (
        "users.yaml",
        "people are accounts of this service, not files (bundle import is RFC 007's)",
    ),
    (
        "history",
        "the history of the source is not replayed; the import is one change (bundle import is RFC 007's)",
    ),
    ("knowledge", "reserved for a later release"),
    ("governance", "reserved for a later release"),
    (".prikkignore", "service-local"),
];

/// What would be done with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The path is new.
    Add,
    /// The path exists and `--replace-existing` was given.
    Replace,
}

/// A file the import would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    /// The repository path (`pages/…` or `attachments/…`).
    pub path: String,
    /// The size in bytes when the plan was made.
    pub size: u64,
    /// Add or replace.
    pub action: Action,
    /// Where it is read from.
    pub source: PathBuf,
}

impl PlannedFile {
    fn is_page(&self) -> bool {
        self.path.starts_with("pages/")
    }
}

/// An entry the import refuses, and the one rule it broke. The name is the entry's place in the folder, escaped
/// for a terminal; **the reason never repeats content**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The entry, relative to the folder.
    pub name: String,
    /// The rule.
    pub reason: String,
}

/// What an import would do, or why it cannot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    /// The files that would be written, in path order.
    pub files: Vec<PlannedFile>,
    /// Everything that is refused. **Non-empty means the whole import is refused.**
    pub refused: Vec<Refusal>,
    /// Entries at the top of the folder that are not imported, with the reason.
    pub not_imported: Vec<(String, String)>,
}

fn unit(n: u64) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MiB", n as f64 / f64::from(1 << 20))
    } else if n >= 1 << 10 {
        format!("{:.1} KiB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

impl ImportPlan {
    /// The bytes the import would write.
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Would the import go ahead? Something to write and nothing refused.
    pub fn is_importable(&self) -> bool {
        !self.files.is_empty() && self.refused.is_empty()
    }

    /// The pages and the attachments in the plan.
    pub fn counts(&self) -> (usize, usize) {
        let pages = self.files.iter().filter(|f| f.is_page()).count();
        (pages, self.files.len() - pages)
    }

    /// The report an operator reads: what would be written, what is refused and why, what is not imported.
    /// Operator text in English; the same for a dry run and for the run.
    pub fn report(&self) -> String {
        let (pages, attachments) = self.counts();
        let mut out = String::new();
        out.push_str(&format!(
            "{} file(s) to write ({pages} page(s), {attachments} attachment(s), {}):\n",
            self.files.len(),
            unit(self.total_bytes())
        ));
        for f in &self.files {
            let verb = match f.action {
                Action::Add => "add",
                Action::Replace => "replace",
            };
            out.push_str(&format!("  {verb:<8}{}  ({})\n", f.path, unit(f.size)));
        }
        if !self.refused.is_empty() {
            out.push_str(&format!(
                "refused ({}): the import is all or nothing, so none of it goes ahead:\n",
                self.refused.len()
            ));
            for r in &self.refused {
                out.push_str(&format!("  {}: {}\n", r.name, r.reason));
            }
        }
        if !self.not_imported.is_empty() {
            out.push_str("not imported:\n");
            for (name, why) in &self.not_imported {
                out.push_str(&format!("  {name}: {why}\n"));
            }
        }
        out
    }
}

/// Why an import did not happen. Each says whether anything changed.
#[derive(Debug)]
pub enum ImportError {
    /// The folder is not a directory (or could not be read). Nothing changed.
    Folder(String),
    /// There is nothing to import (no page and no attachment). Nothing changed.
    Nothing,
    /// The plan has refusals ([`ImportPlan::refused`]). Nothing changed.
    Refused(usize),
    /// The wiki's current files could not be listed (to tell a new path from an existing one). Nothing changed.
    Storage(StorageError),
    /// A file changed between the plan and the read, or could not be read. Nothing changed.
    Changed(String),
    /// The change could not be described to the signer. Nothing changed.
    Message(String),
    /// The signer's answer, which says what state the repository is in ([`AcceptError`]).
    Accept(AcceptError),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Folder(why) => write!(f, "the folder cannot be imported: {why}"),
            ImportError::Nothing => write!(
                f,
                "the folder holds no page and no attachment (`pages/**/*.md` and `attachments/**`); nothing was done"
            ),
            ImportError::Refused(n) => write!(
                f,
                "{n} entr{} refused, so none of the folder was imported; nothing was changed",
                if *n == 1 { "y is" } else { "ies are" }
            ),
            ImportError::Storage(e) => write!(
                f,
                "the wiki's current files could not be listed: {e}; nothing was changed"
            ),
            ImportError::Changed(why) => {
                write!(f, "{why}; nothing was changed, plan the import again")
            }
            ImportError::Message(why) => write!(f, "the import could not be described: {why}"),
            ImportError::Accept(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ImportError {}

/// A name as an operator's terminal may safely show it: control characters escaped, non-UTF-8 bytes shown as
/// replacement characters.
fn shown(name: &str) -> String {
    name.escape_debug().to_string()
}

fn lossy(name: &std::ffi::OsStr) -> String {
    shown(&name.to_string_lossy())
}

/// Read a regular file without following a link and without trusting the listing: the open handle's identity
/// (device and inode) must be that of the entry that was listed, so a link swapped in after the listing is not
/// followed. At most `cap` bytes are read; a longer file is refused by size.
fn read_regular(path: &Path, listed: &std::fs::Metadata, cap: u64) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).map_err(|e| format!("cannot be read ({})", e.kind()))?;
    let opened = file
        .metadata()
        .map_err(|e| format!("cannot be read ({})", e.kind()))?;
    if !opened.is_file() || opened.dev() != listed.dev() || opened.ino() != listed.ino() {
        return Err("is not the file that was listed (it changed while being read)".to_owned());
    }
    let mut bytes = Vec::new();
    file.take(cap.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot be read ({})", e.kind()))?;
    if bytes.len() as u64 > cap {
        return Err(format!("is larger than the {} cap", unit(cap)));
    }
    Ok(bytes)
}

/// What a page must be to be imported: UTF-8 text whose frontmatter the parser accepts. Returns the page's `id`
/// when it has one. The reason never contains the text.
fn inspect_page(bytes: &[u8]) -> Result<Option<String>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| {
        format!(
            "is not UTF-8 text (invalid byte at offset {})",
            e.valid_up_to()
        )
    })?;
    let meta = inklkunn_markup::parse_metadata(text)
        .map_err(|e| format!("has a frontmatter the parser refuses ({e})"))?;
    Ok(meta.id)
}

struct Walk<'a> {
    existing: &'a dyn Fn(&str) -> bool,
    replace_existing: bool,
    page_cap: u64,
    plan: ImportPlan,
    /// `collision_key` → the path that took it.
    keys: HashMap<String, String>,
    /// page id → the path that has it.
    ids: HashMap<String, String>,
}

impl Walk<'_> {
    fn refuse(&mut self, name: &str, reason: impl Into<String>) {
        self.plan.refused.push(Refusal {
            name: shown(name),
            reason: reason.into(),
        });
    }

    fn dir(&mut self, dir: &Path, rel: &str, depth: usize) {
        let mut entries: Vec<(OsString, std::fs::FileType)> = Vec::new();
        match std::fs::read_dir(dir) {
            Ok(rd) => {
                for e in rd {
                    match e {
                        Ok(e) => match e.file_type() {
                            Ok(t) => entries.push((e.file_name(), t)),
                            Err(err) => self.refuse(
                                &format!("{rel}/{}", lossy(&e.file_name())),
                                format!("cannot be examined ({})", err.kind()),
                            ),
                        },
                        Err(err) => {
                            self.refuse(rel, format!("cannot be listed ({})", err.kind()));
                        }
                    }
                }
            }
            Err(err) => {
                self.refuse(rel, format!("cannot be listed ({})", err.kind()));
                return;
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, kind) in entries {
            let Some(part) = name.to_str() else {
                self.refuse(
                    &format!("{rel}/{}", lossy(&name)),
                    "the name is not UTF-8 (a repository path is ASCII)",
                );
                continue;
            };
            let child_rel = format!("{rel}/{part}");
            let child = dir.join(&name);
            if kind.is_symlink() {
                self.refuse(
                    &child_rel,
                    "is a symbolic link; links are never followed, so it is not imported",
                );
            } else if kind.is_dir() {
                if depth >= MAX_DEPTH {
                    self.refuse(
                        &child_rel,
                        format!("is nested deeper than {MAX_DEPTH} folders"),
                    );
                } else {
                    self.dir(&child, &child_rel, depth + 1);
                }
            } else if kind.is_file() {
                self.file(&child, &child_rel);
            } else {
                self.refuse(
                    &child_rel,
                    "is not a regular file (a device, a pipe or a socket)",
                );
            }
        }
    }

    fn file(&mut self, source: &Path, rel: &str) {
        // The caps first, with the numbers: prikk's own (`inklkunn_markup::path`).
        if rel.len() > MAX_PATH_BYTES {
            self.refuse(
                rel,
                format!(
                    "the path is {} bytes, over the {MAX_PATH_BYTES}-byte cap of a repository path",
                    rel.len()
                ),
            );
            return;
        }
        if let Some(long) = rel.split('/').find(|c| c.len() > MAX_COMPONENT_BYTES) {
            self.refuse(
                rel,
                format!(
                    "a name of {} bytes is over the {MAX_COMPONENT_BYTES}-byte cap of a path component",
                    long.len()
                ),
            );
            return;
        }
        // Then the signer's rule, which is the one that would refuse it at the end.
        if let Err(rule) = check_change_path(rel) {
            self.refuse(rel, rule);
            return;
        }
        let is_page = rel.starts_with("pages/");
        if is_page && !rel.ends_with(".md") {
            self.refuse(rel, "a page is a `.md` file");
            return;
        }
        let listed = match std::fs::symlink_metadata(source) {
            Ok(m) if m.is_file() => m,
            Ok(_) => {
                self.refuse(rel, "is not a regular file");
                return;
            }
            Err(e) => {
                self.refuse(rel, format!("cannot be examined ({})", e.kind()));
                return;
            }
        };
        let key = inklkunn_markup::collision_key(rel);
        if let Some(first) = self.keys.get(&key) {
            let first = first.clone();
            self.refuse(
                rel,
                format!(
                    "collides with `{}` on a case-insensitive file system (prikk refuses such a pair)",
                    shown(&first)
                ),
            );
            return;
        }
        self.keys.insert(key, rel.to_owned());
        if is_page {
            if listed.len() > self.page_cap {
                self.refuse(
                    rel,
                    format!("is larger than the {} cap of a page", unit(self.page_cap)),
                );
                return;
            }
            match read_regular(source, &listed, self.page_cap).and_then(|b| inspect_page(&b)) {
                Err(why) => {
                    self.refuse(rel, why);
                    return;
                }
                Ok(Some(id)) => {
                    if let Some(other) = self.ids.get(&id) {
                        let other = other.clone();
                        self.refuse(
                            rel,
                            format!("has the page id of `{}`; ids are unique", shown(&other)),
                        );
                        return;
                    }
                    self.ids.insert(id, rel.to_owned());
                }
                Ok(None) => {}
            }
        }
        let action = if (self.existing)(rel) {
            if !self.replace_existing {
                self.refuse(
                    rel,
                    "already exists in the wiki (`--replace-existing` replaces it)",
                );
                return;
            }
            Action::Replace
        } else {
            Action::Add
        };
        self.plan.files.push(PlannedFile {
            path: rel.to_owned(),
            size: listed.len(),
            action,
            source: source.to_path_buf(),
        });
    }
}

/// Plan the import of `folder`: walk it, check every entry, and say what would be written. `existing` tells
/// whether the wiki already has a path; `replace_existing` allows replacing it. Reads pages (to check them) and
/// nothing else; writes nothing, anywhere.
pub fn plan_folder(
    folder: &Path,
    existing: &dyn Fn(&str) -> bool,
    replace_existing: bool,
) -> Result<ImportPlan, ImportError> {
    let root = std::fs::metadata(folder)
        .map_err(|e| ImportError::Folder(format!("it cannot be read ({})", e.kind())))?;
    if !root.is_dir() {
        return Err(ImportError::Folder("it is not a directory".to_owned()));
    }
    let mut walk = Walk {
        existing,
        replace_existing,
        page_cap: ReadConfig::default().page_max_bytes,
        plan: ImportPlan::default(),
        keys: HashMap::new(),
        ids: HashMap::new(),
    };
    let mut top: Vec<(OsString, Option<std::fs::FileType>)> = std::fs::read_dir(folder)
        .map_err(|e| ImportError::Folder(format!("it cannot be listed ({})", e.kind())))?
        .filter_map(Result::ok)
        .map(|e| (e.file_name(), e.file_type().ok()))
        .collect();
    top.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, kind) in top {
        let shown_name = lossy(&name);
        match (name.to_str(), kind) {
            (Some(n @ ("pages" | "attachments")), Some(t)) => {
                let path = folder.join(n);
                if t.is_symlink() {
                    walk.refuse(n, "is a symbolic link; links are never followed");
                } else if t.is_dir() {
                    walk.dir(&path, n, 0);
                } else {
                    walk.refuse(n, "is not a directory");
                }
            }
            (Some(n), _) => {
                let why = NOT_IMPORTED
                    .iter()
                    .find(|(known, _)| *known == n)
                    .map_or("not part of the bundle layout", |(_, why)| *why);
                walk.plan.not_imported.push((shown_name, why.to_owned()));
            }
            (None, _) => walk
                .plan
                .not_imported
                .push((shown_name, "not part of the bundle layout".to_owned())),
        }
    }
    let mut plan = walk.plan;
    plan.files.sort_by(|a, b| a.path.cmp(&b.path));
    if plan.files.len() > MAX_CHANGE_FILES {
        plan.refused.push(Refusal {
            name: "(the folder)".to_owned(),
            reason: format!(
                "{} files; one acceptance carries at most {MAX_CHANGE_FILES}",
                plan.files.len()
            ),
        });
    }
    if plan.total_bytes() > MAX_CHANGE_BYTES {
        plan.refused.push(Refusal {
            name: "(the folder)".to_owned(),
            reason: format!(
                "{}; one acceptance carries at most {}",
                unit(plan.total_bytes()),
                unit(MAX_CHANGE_BYTES)
            ),
        });
    }
    if plan.files.is_empty() && plan.refused.is_empty() {
        return Err(ImportError::Nothing);
    }
    Ok(plan)
}

/// The paths the wiki has at its default view's tip, to tell a new path from an existing one.
pub fn existing_paths(
    storage: &dyn Storage,
    repo: &Path,
    view: &str,
) -> Result<HashSet<String>, ImportError> {
    let tree = storage
        .tree(repo, &Point::View(view.to_owned()), None)
        .map_err(ImportError::Storage)?;
    Ok(tree
        .entries
        .into_iter()
        .filter(|e| e.kind == "file")
        .map(|e| e.path)
        .collect())
}

/// Read the files of a plan that can be imported, checking each again as it is read.
fn read_files(plan: &ImportPlan) -> Result<Vec<PreparedFile>, ImportError> {
    let page_cap = ReadConfig::default().page_max_bytes;
    let mut files = Vec::with_capacity(plan.files.len());
    for f in &plan.files {
        let listed = std::fs::symlink_metadata(&f.source).map_err(|e| {
            ImportError::Changed(format!("`{}` cannot be examined ({})", f.path, e.kind()))
        })?;
        if !listed.is_file() {
            return Err(ImportError::Changed(format!(
                "`{}` is no longer a regular file",
                f.path
            )));
        }
        let cap = if f.is_page() {
            page_cap
        } else {
            MAX_CHANGE_BYTES
        };
        let bytes = read_regular(&f.source, &listed, cap)
            .map_err(|why| ImportError::Changed(format!("`{}` {why}", f.path)))?;
        if bytes.len() as u64 != f.size {
            return Err(ImportError::Changed(format!(
                "`{}` changed size since it was planned",
                f.path
            )));
        }
        if f.is_page() {
            inspect_page(&bytes)
                .map_err(|why| ImportError::Changed(format!("`{}` {why}", f.path)))?;
        }
        files.push(PreparedFile {
            path: PathBuf::from(&f.path),
            content: Some(bytes),
        });
    }
    Ok(files)
}

/// The acceptance records of imports in flight: the ledger the signer reads (RFC 002 D-4) for the **system
/// principal's** import. There is no stored record for it (an import is an administrative act, audited by the
/// caller, with no proposal); this ledger holds exactly the facts of the one change in hand, under a fresh token,
/// for exactly the length of the call, so the signer's guard (the token, the message, the digest of the files)
/// binds the seal to what was planned and nothing else.
#[derive(Default)]
pub struct ImportLedger {
    pending: Mutex<HashMap<String, ExpectedMessage>>,
}

impl ImportLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        ImportLedger::default()
    }

    fn insert(&self, token: &AcceptanceToken, expected: ExpectedMessage) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(token.as_str().to_owned(), expected);
    }

    fn remove(&self, token: &AcceptanceToken) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(token.as_str());
    }
}

impl AcceptanceLedger for ImportLedger {
    fn pending(&self, token: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(token.as_str())
            .cloned())
    }

    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

/// What a finished import left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The sealed change.
    pub sealed: SealedChange,
    /// The digest of the files that were committed.
    pub digest: ChangeDigest,
    /// Files written.
    pub files: usize,
}

/// Run an import: read the planned files (checking them again), describe the change (`Kind: import`, both people
/// the service), bind it in `ledger` under a fresh token, and hand it to `accept`. The `ledger` must be the one the
/// signer behind `accept` was opened with. A plan with refusals, or without files, is refused here.
pub fn run_import(
    accept: &dyn Accept,
    ledger: &ImportLedger,
    wiki: &WikiRecord,
    plan: &ImportPlan,
) -> Result<Imported, ImportError> {
    if !plan.refused.is_empty() {
        return Err(ImportError::Refused(plan.refused.len()));
    }
    if plan.files.is_empty() {
        return Err(ImportError::Nothing);
    }
    let files = read_files(plan)?;
    let (pages, attachments) = plan.counts();
    let describe = |e: inklkunn_signer::message::MessageError| ImportError::Message(e.to_string());
    let summary = Summary::from_text(&format!(
        "Import {pages} page(s) and {attachments} attachment(s) from a folder"
    ))
    .map_err(describe)?;
    let mut change = PreparedChange {
        files,
        renames: Vec::new(),
        message: String::new(),
    };
    let digest = change.digest();
    let label = SystemLabel::inklkunn();
    let source = ImportSource::new(&format!("folder sha256:{digest}")).map_err(describe)?;
    let message = Message::import(summary, label.clone(), source.clone());
    change.message = message.to_text();
    let token = AcceptanceToken::new(inklkunn_store::record::uuid_v7());
    ledger.insert(
        &token,
        ExpectedMessage {
            wiki: wiki.id.clone(),
            view: wiki.default_view.clone(),
            kind: Kind::Import,
            proposal: None,
            proposed_by: Actor::System(label.clone()),
            accepted_by: Actor::System(label),
            base: None,
            imported_from: Some(source),
            change_digest: digest,
        },
    );
    let result = accept.accept(&wiki.id, &wiki.default_view, &token, &change);
    // The record lives for this call only.
    ledger.remove(&token);
    let sealed = result.map_err(ImportError::Accept)?;
    Ok(Imported {
        sealed,
        digest,
        files: plan.files.len(),
    })
}

#[cfg(test)]
mod tests;
