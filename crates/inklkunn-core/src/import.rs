//! `import-folder` (RFC 003 D-12; handoff 003 T7; internal design IQ-6): a folder of pages and attachments
//! becomes **one accepted change** under the system principal (`Kind: import`), through RFC 002's [`Accept`], so
//! the read path ships with content before the proposal loop exists.
//!
//! **A dry run is the plan.** [`plan_folder`] walks the folder without following a link, reads every page (and
//! nothing else), and says for each entry what would be written or why it is refused: a path over prikk's caps
//! (`inklkunn_markup::path`), a name that is not UTF-8 or not ASCII, a name a repository refuses (the signer's own
//! rule, [`inklkunn_signer::check_change_path`]), a page path that is not the canonical one of its title, a
//! link, a device or a pipe, a file with more than one hard link, a file over its cap, a page that is not
//! UTF-8 text or whose frontmatter the parser refuses, a duplicate page id (in the folder, and against the
//! wiki's pages), a path that collides with another on a case-insensitive file system (in the folder, and
//! against the wiki's files), a path the wiki already has. **Any refusal refuses the whole import** (the
//! change is all or nothing), every reason is named, and the report never repeats a byte of a refused file's
//! content (every reason is escaped as a name is). Running the import is the same plan, then the same checks
//! again on the bytes actually read, then [`Accept`]; the signer validates once more before it writes a byte.
//!
//! **Against the wiki, at one tip.** [`wiki_state`] reads the wiki's files at the default view's tip (one
//! extraction) and the plan is pinned to that tip's block id: [`run_import`] hands it to the signer, which
//! refuses the change if the view has moved since, so a concurrent change cannot turn an add into a replace. A
//! file the wiki already has with exactly these bytes is *unchanged* and is not written; a folder of nothing
//! but unchanged files is [`ImportError::NothingChanged`], not a failed commit.
//!
//! **The folder is not trusted, during the walk or between the plan and the run.** The folder the operator named
//! is opened once (a link there is followed, as the operator asked, and its identity is recorded); every
//! directory below it is then opened with `openat` relative to the handle of its parent, with `O_NOFOLLOW`, and
//! listed through its own handle; every file is opened the same way (`O_NOFOLLOW | O_NONBLOCK`, so a link is
//! refused and a pipe does not wait for a writer) and examined with `fstat` on the open handle. Nothing is looked
//! at by path after the folder's own, so a directory swapped for a link, or moved, while the walk is under way
//! cannot redirect it: the walk goes on in the directory it holds. The plan records the identity (device and
//! inode) of every directory and file it found. The run opens each file again the same way from a fresh handle on
//! the folder, component by component, checking each directory's identity against the plan's, then the file's
//! identity against the planned one (a file with more than one hard link is refused), and reads from that
//! handle. Bytes from anywhere but the planned inodes never reach the signer. The identities are the second line;
//! the handles are the first.
//!
//! **What a folder is.** The layout an export writes: `pages/**/*.md` and `attachments/**`. Everything else at the
//! top (the manifest, `SHA256SUMS`, `users.yaml`, `history/`, `knowledge/`, `governance/`) is *not imported* and is listed as
//! such: a Portable Wiki Bundle's import with its history is RFC 007's.

use std::collections::{BTreeMap, HashMap};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read as _;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use inklkunn_markup::path::{
    MAX_COMPONENT_BYTES, MAX_PATH_BYTES, collision_key, path_to_title, title_to_path,
};
use inklkunn_prikk::{ExtractBounds, Point, Storage, StorageError};
use inklkunn_signer::message::{Actor, BlockId, ImportSource, Kind, Message, Summary, SystemLabel};
use inklkunn_signer::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ChangeDigest, ExpectedMessage,
    LedgerError, MAX_CHANGE_BYTES, MAX_CHANGE_FILES, PreparedChange, PreparedFile, SealedChange,
    ViewCreationAct, ViewCreationFacts, check_change_path,
};
use inklkunn_store::{RootLayout, WikiRecord};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat};
use rustix::io::Errno;
use sha2::{Digest as _, Sha256};

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
        "SHA256SUMS",
        "the checksums of the export's files; the import reads the files themselves",
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

/// A file system object's identity: the device and the inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Identity {
    dev: u64,
    ino: u64,
}

impl Identity {
    // The width of the fields of `Stat` differs by target; `u64` is what the identity keeps.
    #[allow(clippy::useless_conversion)]
    fn of(st: &Stat) -> Self {
        Identity {
            dev: u64::from(st.st_dev),
            ino: u64::from(st.st_ino),
        }
    }
}

/// A SHA-256 digest.
pub type Digest = [u8; 32];

fn sha256(bytes: &[u8]) -> Digest {
    Sha256::digest(bytes).into()
}

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
    /// The inode the plan found there; the run reads that file and no other.
    identity: Identity,
}

impl PlannedFile {
    fn is_page(&self) -> bool {
        self.path.starts_with("pages/")
    }
}

/// An entry the import refuses, and the one rule it broke. The name is the entry's place in the folder, escaped
/// for a terminal; **the reason never repeats content** and is escaped the same way.
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
    /// Files of the folder that the wiki already has with exactly these bytes (`--replace-existing`): they are
    /// not written.
    pub unchanged: Vec<String>,
    /// Everything that is refused. **Non-empty means the whole import is refused.**
    pub refused: Vec<Refusal>,
    /// Entries at the top of the folder that are not imported, with the reason.
    pub not_imported: Vec<(String, String)>,
    /// The block id of the wiki's tip the plan was made against; the signer refuses the change if the view has
    /// moved ([`Existing::tip`]).
    pub base: String,
    /// The folder, as it was given.
    root: PathBuf,
    /// Every directory the walk entered (and the folder itself) with its identity.
    dirs: BTreeMap<PathBuf, Identity>,
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
        self.files
            .iter()
            .fold(0_u64, |sum, f| sum.saturating_add(f.size))
    }

    /// Would the import go ahead? Something to write and nothing refused.
    pub fn is_importable(&self) -> bool {
        !self.files.is_empty() && self.refused.is_empty()
    }

    /// Is every file of the folder one the wiki already has, byte for byte? Nothing refused, nothing to write.
    pub fn is_unchanged(&self) -> bool {
        self.files.is_empty() && self.refused.is_empty() && !self.unchanged.is_empty()
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
        if !self.unchanged.is_empty() {
            out.push_str(&format!(
                "unchanged ({}): the wiki has these exactly as they are, so they are not written:\n",
                self.unchanged.len()
            ));
            for path in &self.unchanged {
                out.push_str(&format!("  {path}\n"));
            }
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

/// What the wiki has at one tip, to plan an import against: every file under `pages/` and `attachments/` by
/// path with the digest of its bytes, and the page ids. Read it with [`wiki_state`]; [`Existing::at`] and
/// [`Existing::with_file`] build one by hand (a test, or a caller that already has the files).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Existing {
    /// The block id of the tip these files are as of; the plan carries it and the signer holds the change to it.
    pub tip: String,
    files: HashMap<String, Digest>,
    /// page id → the path that has it.
    page_ids: HashMap<String, String>,
    /// [`collision_key`] → the path that took it.
    keys: HashMap<String, String>,
}

impl Existing {
    /// A wiki with no file at the tip `tip`.
    pub fn at(tip: &str) -> Self {
        Existing {
            tip: tip.to_owned(),
            ..Existing::default()
        }
    }

    /// The same, with the file `path` holding `content` (a page's `id` is read from it).
    #[must_use]
    pub fn with_file(mut self, path: &str, content: &[u8]) -> Self {
        let id = page_id_of(path, content);
        self.add(path, sha256(content), id);
        self
    }

    fn add(&mut self, path: &str, digest: Digest, page_id: Option<String>) {
        self.files.insert(path.to_owned(), digest);
        self.keys.insert(collision_key(path), path.to_owned());
        if let Some(id) = page_id {
            self.page_ids.insert(id, path.to_owned());
        }
    }
}

/// The `id` a page file declares, when `path` is a page and `content` is UTF-8 text whose frontmatter parses.
fn page_id_of(path: &str, content: &[u8]) -> Option<String> {
    if !(path.starts_with("pages/") && path.ends_with(".md")) {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    inklkunn_markup::parse_metadata(text)
        .ok()?
        .id
        .filter(|id| !id.is_empty())
}

/// Why an import did not happen. Each says whether anything changed.
#[derive(Debug)]
pub enum ImportError {
    /// The folder is not a directory (or could not be read). Nothing changed.
    Folder(String),
    /// There is nothing to import (no page and no attachment). Nothing changed.
    Nothing,
    /// Every file of the folder is one the wiki already has, byte for byte (the count). Nothing changed, and
    /// nothing needed to.
    NothingChanged(usize),
    /// The plan has refusals ([`ImportPlan::refused`]). Nothing changed.
    Refused(usize),
    /// The wiki's current files could not be listed (to tell a new path from an existing one). Nothing changed.
    Storage(StorageError),
    /// The wiki's current files could not be read from the extraction. Nothing changed.
    Wiki(String),
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
            ImportError::NothingChanged(n) => write!(
                f,
                "nothing changed: the wiki already has {} of the folder exactly as {}; nothing was written",
                if *n == 1 {
                    "the one file".to_owned()
                } else {
                    format!("all {n} files")
                },
                if *n == 1 { "it is" } else { "they are" }
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
            ImportError::Wiki(why) => write!(
                f,
                "the wiki's current files could not be read: {why}; nothing was changed"
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

/// A refusal's reason, escaped the way a name is, except that a quote stays a quote (the rules are sentences).
fn shown_reason(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\'' || c == '"' {
            out.push(c);
        } else {
            out.extend(c.escape_debug());
        }
    }
    out
}

fn lossy(name: &std::ffi::OsStr) -> String {
    shown(&name.to_string_lossy())
}

/// How a directory below the folder is opened, relative to its parent's handle: a link is refused, not followed.
const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// How a file of the folder is opened, relative to its directory's handle: a link is refused (`NOFOLLOW`), and a
/// pipe does not make the open wait for a writer (`NONBLOCK`; a regular file is not affected by it).
const FILE_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);

/// What an error of the operating system is called in a reason: its kind, never its text.
fn kind_of(errno: Errno) -> std::io::ErrorKind {
    std::io::Error::from(errno).kind()
}

/// The folder the operator named, opened once. A link there is followed (the operator asked for that place); the
/// handle is what the walk and the run hold, and nothing below it is reached by a path again.
fn open_root(folder: &Path) -> Result<OwnedFd, Errno> {
    rustix::fs::open(
        folder,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
}

/// The directory `name` of the open directory `parent`; a link in its place is refused.
fn open_dir_at(parent: &impl AsFd, name: &OsStr) -> Result<OwnedFd, Errno> {
    rustix::fs::openat(parent, name, DIR_FLAGS, Mode::empty())
}

/// Does this error say that what was opened is not a directory (a link in its place answers `ELOOP` or `ENOTDIR`,
/// by kernel and flags)?
fn is_not_a_directory(errno: Errno) -> bool {
    errno == Errno::LOOP || errno == Errno::NOTDIR
}

/// The identity of an open directory, or `ENOTDIR` when the handle is something else.
fn dir_identity(dir: &OwnedFd) -> Result<Identity, Errno> {
    let st = rustix::fs::fstat(dir)?;
    if FileType::from_raw_mode(st.st_mode) == FileType::Directory {
        Ok(Identity::of(&st))
    } else {
        Err(Errno::NOTDIR)
    }
}

/// An open regular file, as `fstat` on its handle sees it.
struct Opened {
    file: File,
    identity: Identity,
    nlink: u64,
    size: u64,
}

/// Open the file `name` of the open directory `dir` and look at the handle: a link is refused, a pipe does not
/// block the open, and what was opened must be a regular file.
#[allow(clippy::useless_conversion)] // the width of `st_nlink` differs by target
fn open_regular(dir: &impl AsFd, name: &OsStr) -> Result<Opened, String> {
    let read_error = |e: Errno| {
        if e == Errno::LOOP {
            "is a symbolic link now (it changed while being read)".to_owned()
        } else {
            format!("cannot be read ({})", kind_of(e))
        }
    };
    let fd = rustix::fs::openat(dir, name, FILE_FLAGS, Mode::empty()).map_err(read_error)?;
    let st = rustix::fs::fstat(&fd).map_err(read_error)?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile {
        return Err("is no longer a regular file".to_owned());
    }
    Ok(Opened {
        file: File::from(fd),
        identity: Identity::of(&st),
        nlink: u64::from(st.st_nlink),
        size: u64::try_from(st.st_size).unwrap_or_default(),
    })
}

/// Open a file of the folder for reading: [`open_regular`], and the open handle must have exactly the identity
/// that was planned and no other hard link. The handle is what is read; no path is consulted again.
fn open_checked(dir: &impl AsFd, name: &OsStr, expect: Identity) -> Result<File, String> {
    let opened = open_regular(dir, name)?;
    if opened.identity != expect {
        return Err("is not the file that was listed (it changed while being read)".to_owned());
    }
    if opened.nlink > 1 {
        return Err("has more than one hard link".to_owned());
    }
    Ok(opened.file)
}

/// At most `cap` bytes of an open file; a longer file is refused by size.
fn read_all(file: File, cap: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.take(cap.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot be read ({})", e.kind()))?;
    if bytes.len() as u64 > cap {
        return Err(format!("is larger than the {} cap", unit(cap)));
    }
    Ok(bytes)
}

/// The SHA-256 of an open file, without holding it in memory; a file over `cap` is refused by size.
fn hash_all(mut file: File, cap: u64) -> Result<Digest, String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; 64 * 1024];
    let mut seen: u64 = 0;
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("cannot be read ({})", e.kind()))?;
        if n == 0 {
            break;
        }
        seen = seen.saturating_add(n as u64);
        if seen > cap {
            return Err(format!("is larger than the {} cap", unit(cap)));
        }
        hasher.update(buf.get(..n).unwrap_or_default());
    }
    Ok(hasher.finalize().into())
}

/// Read a regular file as the plan found it: [`open_checked`], then at most `cap` bytes.
fn read_regular(
    dir: &impl AsFd,
    name: &OsStr,
    expect: Identity,
    cap: u64,
) -> Result<Vec<u8>, String> {
    read_all(open_checked(dir, name, expect)?, cap)
}

/// Open the directory that holds `file` again, from a fresh handle on the folder, one component at a time with
/// `openat`, and check each directory against the identity the walk recorded for it (the folder itself is
/// reached through a link once, as the operator named it, and must be the same inode). Returns the handle on the
/// directory and the file's name in it. No path below the folder is consulted, so a directory replaced by a link
/// or by another directory since the walk is refused, and nothing can be redirected while this opens.
fn open_chain(
    root: &Path,
    dirs: &BTreeMap<PathBuf, Identity>,
    file: &Path,
) -> Result<(OwnedFd, OsString), String> {
    let outside = || "is outside the folder that was planned".to_owned();
    let rel = file.strip_prefix(root).map_err(|_| outside())?;
    let mut parts: Vec<&OsStr> = rel.components().map(|c| c.as_os_str()).collect();
    let name = parts.pop().ok_or_else(outside)?;
    let moved = |at: &Path| {
        let shown_at = at
            .strip_prefix(root)
            .ok()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| "the folder".to_owned(), |p| lossy(p.as_os_str()));
        format!(
            "is reached through `{shown_at}`, which is no longer the directory that was planned"
        )
    };
    let still = |fd: &OwnedFd, at: &Path| match (dirs.get(at), dir_identity(fd)) {
        (Some(expect), Ok(now)) if *expect == now => Ok(()),
        _ => Err(moved(at)),
    };
    let mut at = root.to_path_buf();
    let mut fd = open_root(root).map_err(|_| moved(&at))?;
    still(&fd, &at)?;
    for part in parts {
        at.push(part);
        fd = open_dir_at(&fd, part).map_err(|_| moved(&at))?;
        still(&fd, &at)?;
    }
    Ok((fd, name.to_owned()))
}

/// The entries of an open directory (never `.` and `..`), each with its type, or the error that said the type
/// could not be told. `failed` is the error that stopped the listing short, if one did.
struct Listing {
    entries: Vec<(OsString, Result<FileType, Errno>)>,
    failed: Option<Errno>,
}

/// List the open directory `dir` through its handle. A type the file system leaves unsaid is asked of the entry
/// itself, without following a link.
fn list_dir(dir: &OwnedFd) -> Result<Listing, Errno> {
    let mut stream = rustix::fs::Dir::read_from(dir)?;
    let mut listing = Listing {
        entries: Vec::new(),
        failed: None,
    };
    while let Some(next) = stream.read() {
        match next {
            Ok(entry) => {
                let raw = entry.file_name();
                if raw == c"." || raw == c".." {
                    continue;
                }
                let kind = match entry.file_type() {
                    FileType::Unknown => rustix::fs::statat(dir, raw, AtFlags::SYMLINK_NOFOLLOW)
                        .map(|st| FileType::from_raw_mode(st.st_mode)),
                    known => Ok(known),
                };
                listing
                    .entries
                    .push((OsString::from_vec(raw.to_bytes().to_vec()), kind));
            }
            Err(e) => {
                listing.failed = Some(e);
                break;
            }
        }
    }
    Ok(listing)
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
    existing: &'a Existing,
    replace_existing: bool,
    page_cap: u64,
    plan: ImportPlan,
    /// `collision_key` → the path that took it.
    keys: HashMap<String, String>,
    /// page id → the path that has it.
    ids: HashMap<String, String>,
    /// The page paths of the folder.
    pages: Vec<String>,
}

impl Walk<'_> {
    fn refuse(&mut self, name: &str, reason: impl AsRef<str>) {
        self.plan.refused.push(Refusal {
            name: shown(name),
            reason: shown_reason(reason.as_ref()),
        });
    }

    /// Open the directory `name` of `parent` (a link in its place is refused) and walk it.
    fn descend(&mut self, parent: &OwnedFd, name: &OsStr, path: &Path, rel: &str, depth: usize) {
        match open_dir_at(parent, name) {
            Ok(handle) => self.dir(&handle, path, rel, depth),
            Err(e) if is_not_a_directory(e) => self.refuse(
                rel,
                "is no longer a directory (it changed while the folder was walked)",
            ),
            Err(e) => self.refuse(rel, format!("cannot be examined ({})", kind_of(e))),
        }
    }

    /// Walk the open directory `handle`, which is `path` (`rel` in the folder). It is listed through the handle
    /// and every entry is opened relative to it, so what is walked is the directory that was opened, whatever
    /// happens to its path meanwhile.
    fn dir(&mut self, handle: &OwnedFd, path: &Path, rel: &str, depth: usize) {
        match dir_identity(handle) {
            Ok(identity) => {
                self.plan.dirs.insert(path.to_path_buf(), identity);
            }
            Err(e) => {
                self.refuse(rel, format!("cannot be examined ({})", kind_of(e)));
                return;
            }
        }
        #[cfg(test)]
        tests::at_directory(path, tests::Stage::Opened);
        let listing = match list_dir(handle) {
            Ok(l) => l,
            Err(e) => {
                self.refuse(rel, format!("cannot be listed ({})", kind_of(e)));
                return;
            }
        };
        if let Some(e) = listing.failed {
            self.refuse(rel, format!("cannot be listed ({})", kind_of(e)));
        }
        #[cfg(test)]
        tests::at_directory(path, tests::Stage::Listed);
        let mut entries = listing.entries;
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
            let child = path.join(&name);
            match kind {
                Err(e) => self.refuse(&child_rel, format!("cannot be examined ({})", kind_of(e))),
                Ok(FileType::Symlink) => self.refuse(
                    &child_rel,
                    "is a symbolic link; links are never followed, so it is not imported",
                ),
                Ok(FileType::Directory) => {
                    if depth >= MAX_DEPTH {
                        self.refuse(
                            &child_rel,
                            format!("is nested deeper than {MAX_DEPTH} folders"),
                        );
                    } else {
                        self.descend(handle, &name, &child, &child_rel, depth + 1);
                    }
                }
                Ok(FileType::RegularFile) => self.file(handle, &name, &child, &child_rel),
                Ok(_) => self.refuse(
                    &child_rel,
                    "is not a regular file (a device, a pipe or a socket)",
                ),
            }
        }
    }

    fn file(&mut self, parent: &OwnedFd, name: &OsStr, source: &Path, rel: &str) {
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
        // A page lives at the path its title spells, and no other (the same title would otherwise have two).
        if is_page {
            let canonical = path_to_title(rel).and_then(|t| title_to_path(&t).ok());
            if canonical.as_deref() != Some(rel) {
                self.refuse(
                    rel,
                    match canonical {
                        Some(c) => format!(
                            "is not the canonical path of the page it spells (that page is `{c}`)"
                        ),
                        None => "is not a path any page title spells".to_owned(),
                    },
                );
                return;
            }
        }
        // Opened relative to its directory's handle and looked at through the open handle: what is examined, and
        // read below, is that file.
        let opened = match open_regular(parent, name) {
            Ok(o) => o,
            Err(why) => {
                self.refuse(rel, why);
                return;
            }
        };
        if opened.nlink > 1 {
            self.refuse(
                rel,
                "has more than one hard link (another name could change it after it was checked)",
            );
            return;
        }
        let identity = opened.identity;
        let size = opened.size;
        let key = collision_key(rel);
        if let Some(first) = self.keys.get(&key) {
            let first = first.clone();
            self.refuse(
                rel,
                format!(
                    "collides with `{first}` on a case-insensitive file system (prikk refuses such a pair)"
                ),
            );
            return;
        }
        if let Some(have) = self.existing.keys.get(&key)
            && have != rel
        {
            let have = have.clone();
            self.refuse(
                rel,
                format!(
                    "collides with `{have}`, which the wiki has, on a case-insensitive file system (prikk refuses such a pair)"
                ),
            );
            return;
        }
        self.keys.insert(key, rel.to_owned());
        let cap = if is_page {
            self.page_cap
        } else {
            MAX_CHANGE_BYTES
        };
        if size > cap {
            self.refuse(
                rel,
                format!(
                    "is larger than the {} cap of {}",
                    unit(cap),
                    if is_page { "a page" } else { "one acceptance" }
                ),
            );
            return;
        }
        // The bytes are compared only for a file the wiki has and the operator allowed to replace.
        let have = self.existing.files.get(rel).copied();
        let want_digest = have.is_some() && self.replace_existing;
        let mut digest: Option<Digest> = None;
        if is_page {
            match read_all(opened.file, cap).and_then(|b| inspect_page(&b).map(|id| (b, id))) {
                Err(why) => {
                    self.refuse(rel, why);
                    return;
                }
                Ok((bytes, id)) => {
                    if want_digest {
                        digest = Some(sha256(&bytes));
                    }
                    if let Some(id) = id {
                        if let Some(other) = self.ids.get(&id) {
                            let other = other.clone();
                            self.refuse(
                                rel,
                                format!("has the page id of `{other}`; ids are unique"),
                            );
                            return;
                        }
                        self.ids.insert(id, rel.to_owned());
                    }
                }
            }
            self.pages.push(rel.to_owned());
        } else if want_digest {
            match hash_all(opened.file, cap) {
                Ok(d) => digest = Some(d),
                Err(why) => {
                    self.refuse(rel, why);
                    return;
                }
            }
        }
        let action = if let Some(have) = have {
            if !self.replace_existing {
                self.refuse(
                    rel,
                    "already exists in the wiki (`--replace-existing` replaces it)",
                );
                return;
            }
            if digest == Some(have) {
                self.plan.unchanged.push(rel.to_owned());
                return;
            }
            Action::Replace
        } else {
            Action::Add
        };
        self.plan.files.push(PlannedFile {
            path: rel.to_owned(),
            size,
            action,
            source: source.to_path_buf(),
            identity,
        });
    }

    /// The page ids against the wiki's: an id the wiki has at a path this import does not rewrite is taken.
    fn ids_against_the_wiki(&mut self) {
        let mut taken: Vec<(String, String, String)> = Vec::new();
        for (id, path) in &self.ids {
            if let Some(owner) = self.existing.page_ids.get(id)
                && owner != path
                && !self.pages.contains(owner)
            {
                taken.push((path.clone(), owner.clone(), id.clone()));
            }
        }
        taken.sort();
        for (path, owner, _) in taken {
            self.refuse(
                &path,
                format!("has the page id of `{owner}`, which the wiki has; ids are unique"),
            );
        }
    }
}

/// Plan the import of `folder`: walk it, check every entry, and say what would be written. `existing` is what
/// the wiki has at the tip the plan is pinned to ([`wiki_state`]); `replace_existing` allows replacing a file it
/// has. Reads pages (to check them) and nothing else; writes nothing, anywhere.
pub fn plan_folder(
    folder: &Path,
    existing: &Existing,
    replace_existing: bool,
) -> Result<ImportPlan, ImportError> {
    // The folder is opened once; everything below it is reached through this handle.
    let root = open_root(folder).map_err(|e| {
        if e == Errno::NOTDIR {
            ImportError::Folder("it is not a directory".to_owned())
        } else {
            ImportError::Folder(format!("it cannot be read ({})", kind_of(e)))
        }
    })?;
    let root_identity = dir_identity(&root)
        .map_err(|e| ImportError::Folder(format!("it cannot be read ({})", kind_of(e))))?;
    let mut walk = Walk {
        existing,
        replace_existing,
        page_cap: ReadConfig::default().page_max_bytes,
        plan: ImportPlan {
            base: existing.tip.clone(),
            root: folder.to_path_buf(),
            ..ImportPlan::default()
        },
        keys: HashMap::new(),
        ids: HashMap::new(),
        pages: Vec::new(),
    };
    walk.plan.dirs.insert(folder.to_path_buf(), root_identity);
    let listing = list_dir(&root)
        .map_err(|e| ImportError::Folder(format!("it cannot be listed ({})", kind_of(e))))?;
    if let Some(e) = listing.failed {
        return Err(ImportError::Folder(format!(
            "it cannot be listed ({})",
            kind_of(e)
        )));
    }
    let mut top = listing.entries;
    top.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, kind) in top {
        let shown_name = lossy(&name);
        match (name.to_str(), kind.ok()) {
            (Some(n @ ("pages" | "attachments")), Some(t)) => {
                let path = folder.join(n);
                match t {
                    FileType::Symlink => {
                        walk.refuse(n, "is a symbolic link; links are never followed");
                    }
                    FileType::Directory => walk.descend(&root, &name, &path, n, 0),
                    _ => walk.refuse(n, "is not a directory"),
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
    walk.ids_against_the_wiki();
    let mut plan = walk.plan;
    plan.files.sort_by(|a, b| a.path.cmp(&b.path));
    plan.unchanged.sort();
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
    if plan.files.is_empty() && plan.refused.is_empty() && plan.unchanged.is_empty() {
        return Err(ImportError::Nothing);
    }
    Ok(plan)
}

/// What the wiki has at its default view's tip: one extraction of the tip (in the wiki's scratch directory,
/// removed again), every file under `pages/` and `attachments/` hashed and every page's id read. The tip's
/// block id is [`Existing::tip`], and the extraction is of that block, so the files and the tip agree.
pub fn wiki_state(
    storage: &dyn Storage,
    layout: &RootLayout,
    record: &WikiRecord,
) -> Result<Existing, ImportError> {
    let repo = layout.repo_dir(&record.id);
    let tree = storage
        .tree(&repo, &Point::View(record.default_view.clone()), None)
        .map_err(ImportError::Storage)?;
    let tip = tree.target_block_id;
    let scratch = layout.scratch_dir(&record.id);
    std::fs::create_dir_all(&scratch).map_err(|e| {
        ImportError::Wiki(format!(
            "the wiki's scratch directory could not be made ({})",
            e.kind()
        ))
    })?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let extraction = format!("import-{}-{nanos}", std::process::id());
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
            .map_err(ImportError::Storage)?;
        let mut state = Existing::at(&tip);
        let extracted = scratch.join(&extraction);
        for top in ["pages", "attachments"] {
            collect(&extracted, top, &mut state)?;
        }
        Ok(state)
    })();
    let _ = std::fs::remove_dir_all(scratch.join(&extraction));
    result
}

/// Add the regular files under `<extracted>/<rel>` (a missing directory has none).
fn collect(extracted: &Path, rel: &str, state: &mut Existing) -> Result<(), ImportError> {
    let wiki_error =
        |what: &str, e: &std::io::Error| ImportError::Wiki(format!("{what} ({})", e.kind()));
    let dir = extracted.join(rel);
    let listing = match std::fs::read_dir(&dir) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(wiki_error(
                "a directory of the extraction could not be listed",
                &e,
            ));
        }
    };
    for item in listing {
        let item = item.map_err(|e| wiki_error("the extraction could not be listed", &e))?;
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let child = format!("{rel}/{name}");
        let kind = item
            .file_type()
            .map_err(|e| wiki_error("the extraction could not be examined", &e))?;
        if kind.is_dir() {
            collect(extracted, &child, state)?;
        } else if kind.is_file() {
            let path = item.path();
            let io = |e: std::io::Error| wiki_error("an extracted file could not be read", &e);
            let is_page = child.starts_with("pages/") && child.ends_with(".md");
            let (digest, id) = if is_page
                && item.metadata().map_err(io)?.len() <= ReadConfig::default().page_max_bytes
            {
                let bytes = std::fs::read(&path).map_err(io)?;
                (sha256(&bytes), page_id_of(&child, &bytes))
            } else {
                let file = File::open(&path).map_err(io)?;
                let digest = hash_all(file, u64::MAX).map_err(ImportError::Wiki)?;
                (digest, None)
            };
            state.add(&child, digest, id);
        }
    }
    Ok(())
}

/// Read the files of a plan that can be imported, checking each again as it is read: the directories above it,
/// the open handle's identity against the plan's, its size and (for a page) its content.
fn read_files(plan: &ImportPlan) -> Result<Vec<PreparedFile>, ImportError> {
    let page_cap = ReadConfig::default().page_max_bytes;
    let mut files = Vec::with_capacity(plan.files.len());
    let changed =
        |f: &PlannedFile, why: String| ImportError::Changed(format!("`{}` {why}", f.path));
    for f in &plan.files {
        let (parent, name) =
            open_chain(&plan.root, &plan.dirs, &f.source).map_err(|why| changed(f, why))?;
        let cap = if f.is_page() {
            page_cap
        } else {
            MAX_CHANGE_BYTES
        };
        let bytes = read_regular(&parent, &name, f.identity, cap).map_err(|why| changed(f, why))?;
        if bytes.len() as u64 != f.size {
            return Err(changed(f, "changed size since it was planned".to_owned()));
        }
        if f.is_page() {
            inspect_page(&bytes).map_err(|why| changed(f, why))?;
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
/// the service), bind it in `ledger` under a fresh token and to the tip the plan was made against, and hand it to
/// `accept`. The `ledger` must be the one the signer behind `accept` was opened with. A plan with refusals, or
/// without files, is refused here; the signer refuses it if the wiki's tip is no longer the plan's.
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
        return Err(if plan.unchanged.is_empty() {
            ImportError::Nothing
        } else {
            ImportError::NothingChanged(plan.unchanged.len())
        });
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
    // The tip the plan was made against: the signer holds the change to it (a message of kind `import` has no
    // `Base`, so the pin is part of the record, not of the message).
    let pinned_tip = BlockId::new(&plan.base).map_err(describe)?;
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
            pinned_tip: Some(pinned_tip),
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
