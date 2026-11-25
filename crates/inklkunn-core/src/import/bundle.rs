//! `import-bundle`'s plan and report (RFC 007 D-1, D-4 – D-6; handoff 007 I-T2): a Portable Wiki Bundle read, judged and
//! reported **before anything is written**, which is the whole of this task (the replay is I-T3's).
//!
//! **The plan reads and judges everything.** [`plan_bundle`] walks the directory with `import-folder`'s own walk (handles,
//! `openat`, `O_NOFOLLOW`, identities pinned, the same path, name and size rules: [`super::Walk`]), and then judges, in this
//! order, what the walk gathered:
//!
//! 1. the manifest, by its known keys only ([`manifest::read`]); its `fidelity` is **reported and not trusted**;
//! 2. the licence the wiki takes (RFC 007 D-4): the manifest's, or `--bundle-license` when it declares none, and **never
//!    one that changes what the bundle says**; a licence that no wiki may have as its default refuses;
//! 3. the checksums the manifest names, **strictly when present**: every file against its line, every file of the bundle
//!    against the lines (a missing, an extra and a wrong file each refuse by name), and *none in the bundle* said when there
//!    are none; there is no way to skip them;
//! 4. `users.yaml`: an e-mail address in any entry refuses, naming the entry;
//! 5. every history file ([`history`]): each record by the schema's types, **the file's own order as the history** (a `parent`,
//!    `rev` or `at` that breaks it refuses, naming the page and the `rev`; times as instants), `content` or `patch` with its
//!    `sha256` checked, every revision's text judged as the folder import judges a page (frontmatter, the page cap), and the
//!    licence each declares;
//! 6. each page's and attachment's own licence by the matrix of chapter 15 ([`licence`]): files it cannot go into refuse the
//!    run unless `--leave-out-incompatible`, which leaves **exactly those** out (a page with all its history), and nothing
//!    imports them;
//! 7. the revision cap and the bytes of `history/` (C-5e), with the way on (`--no-history`).
//!
//! **Nothing is written, anywhere**, and **no byte of a refused file is repeated**: a refusal names a path (or a page and a
//! `rev`) and a rule, and every name in it or in the report is display-safe and bounded ([`safe::shown`]).
//!
//! The result is a [`BundlePlan`]: the files the first change would write (the same [`ImportPlan`] the folder import makes), each
//! page's judged history, the order of the replay (revisions of all pages merged by `at`, ties in path order, the same every
//! time), and the [`BundleReport`], a value that the replay's act can keep.

mod history;
pub mod licence;
mod manifest;
mod patch;
mod report;
mod safe;
pub mod time;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::File;
use std::os::fd::OwnedFd;
use std::path::Path;

use rustix::fs::FileType;

pub use history::{Author, Part, Revision, Skipped, Step};
pub use manifest::Manifest;
pub use report::{
    BundleReport, Checksums, FileKind, Found, Incompatible, LeftOut, LicenseFrom, MAX_LISTED,
    PageLine, PeopleLine,
};
pub use safe::{looks_like_an_address, shown, shown_up_to};

use super::{
    Digest, Existing, ImportError, ImportPlan, Refusal, Walk, canonical_fault, cap_fault,
    dir_identity, hash_all, kind_of, list_dir, lossy, open_regular, open_root, read_all, sha256,
    unit,
};
use crate::read::ReadConfig;
use inklkunn_markup::path::title_to_path;
use inklkunn_signer::{MAX_CHANGE_BYTES, MAX_CHANGE_FILES, check_change_path};

/// The revision cap's default (`INKLKUNN_IMPORT_MAX_REVISIONS`). **Provisional**, from ledger PK-50's measurement of building a
/// history through the signer (500 blocks: 154 s; 2,000 blocks: 1,438 s): 500 blocks are about 2½ minutes, and the cost per block
/// grows with the log. The replay's own cost is measured in I-T3, where the architect sets the number.
pub const DEFAULT_MAX_REVISIONS: u64 = 500;

/// The default cap on the bytes of `history/` (`INKLKUNN_IMPORT_MAX_HISTORY_MIB`): 256 MiB, a quarter of what an export holds at
/// most. **Provisional** as the revision cap is.
pub const DEFAULT_MAX_HISTORY_BYTES: u64 = 256 << 20;

/// What `--bundle-license`, `--leave-out-incompatible` and `--no-history` say, and the operator's two caps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleOptions {
    /// `--bundle-license`: the licence of a bundle that declares none (a licence is never changed).
    pub bundle_license: Option<String>,
    /// `--leave-out-incompatible`.
    pub leave_out_incompatible: bool,
    /// `--no-history`: the latest text only.
    pub no_history: bool,
    /// `INKLKUNN_IMPORT_MAX_REVISIONS`.
    pub max_revisions: u64,
    /// `INKLKUNN_IMPORT_MAX_HISTORY_MIB`, in bytes.
    pub max_history_bytes: u64,
}

impl Default for BundleOptions {
    fn default() -> Self {
        BundleOptions {
            bundle_license: None,
            leave_out_incompatible: false,
            no_history: false,
            max_revisions: DEFAULT_MAX_REVISIONS,
            max_history_bytes: DEFAULT_MAX_HISTORY_BYTES,
        }
    }
}

/// One page's judged history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageHistory {
    /// The history file, relative to the bundle (`history/<name>.jsonl`).
    pub file: String,
    /// The page's path in the bundle's files, when the page is there (a page removed since has a history and no file).
    pub page: Option<String>,
    /// The revisions, in the file's order.
    pub revisions: Vec<Revision>,
    /// The digest of the text after the last revision that gave one, when the plan has it: what the page's own file is compared
    /// with.
    pub last_text: Option<Digest>,
}

/// What an import of a bundle would do, or why it cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundlePlan {
    /// The files the first change would write (pages without a history to replay, attachments with their sidecars, knowledge
    /// files) and **everything that is refused**: a non-empty [`ImportPlan::refused`] refuses the whole import.
    pub plan: ImportPlan,
    /// The report.
    pub report: BundleReport,
    /// The histories to replay.
    pub histories: Vec<PageHistory>,
    /// The order of the replay: `(history, revision)` indexes, revisions of all pages merged by `at` as instants, ties in the
    /// path order of the page, then the file's order. The same bundle always gives the same order.
    pub schedule: Vec<(usize, usize)>,
}

impl BundlePlan {
    /// Would the import go ahead? Nothing is refused.
    pub fn is_importable(&self) -> bool {
        self.plan.refused.is_empty()
    }

    /// What an operator reads: the report, then every refusal by name and rule.
    pub fn render(&self) -> String {
        let mut out = self.report.render();
        if !self.plan.refused.is_empty() {
            out.push_str(&format!(
                "refused ({}): the import is all or nothing, so none of it goes ahead:\n",
                self.plan.refused.len()
            ));
            for r in self.plan.refused.iter().take(200) {
                out.push_str(&format!("  {}: {}\n", r.name, r.reason));
            }
            if self.plan.refused.len() > 200 {
                out.push_str(&format!("  … and {} more\n", self.plan.refused.len() - 200));
            }
        }
        out
    }
}

// ----------------------------------------------------------------------------------------------- what the walk gathers

/// How large the files the plan reads whole may be.
const MAX_USERS_BYTES: u64 = 8 << 20;
/// The checksums file: a line of about a hundred bytes for each of up to 200,000 files.
const MAX_SUMS_BYTES: u64 = 32 << 20;
/// A sidecar (`<attachment>.meta.yaml`).
const MAX_SIDECAR_BYTES: u64 = 256 << 10;
/// A file the plan only counts and hashes (an extension's, a discussion's): larger is refused, as a size bomb.
const MAX_OTHER_BYTES: u64 = 4 << 30;

/// A file the walk saw.
#[derive(Debug, Clone, Copy)]
struct Seen {
    /// `None` when it was not read (no checksums to verify, or past a cap).
    digest: Option<Digest>,
}

/// What the plan needs of a page besides its place.
#[derive(Debug, Clone)]
pub(super) struct PageFacts {
    id: Option<String>,
    license: Option<String>,
    sha256: Digest,
    /// `[[prefix:target]]` links by prefix.
    prefixes: BTreeMap<String, usize>,
}

/// What the walk gathers in a bundle plan, beside the plan's own files.
#[derive(Debug, Default)]
pub(super) struct Gathered {
    verify: bool,
    sums_path: Option<String>,
    knowledge_known: bool,
    read_history: bool,
    history_cap: u64,
    history_bytes: u64,
    history_over: bool,
    files: BTreeMap<String, Seen>,
    pages: BTreeMap<String, PageFacts>,
    sidecars: BTreeMap<String, Vec<u8>>,
    /// The bytes of the files read whole: `users.yaml`, the checksums file, the history files.
    read: BTreeMap<String, Vec<u8>>,
}

/// What a file outside the content directories is for.
enum Role {
    Whole(u64),
    History,
    Other,
}

impl Gathered {
    /// Is `rel` content (a page, an attachment, a knowledge file) that the walk judges and the first change writes?
    pub(super) fn is_content(&self, rel: &str) -> bool {
        if rel.rsplit('/').next() == Some(".keep") {
            return false;
        }
        rel.starts_with("pages/")
            || rel.starts_with("attachments/")
            || (self.knowledge_known && rel.starts_with("knowledge/"))
    }

    fn role(&self, rel: &str) -> Role {
        if rel == crate::bundle::MANIFEST {
            Role::Whole(super::MAX_MANIFEST_BYTES)
        } else if rel == "users.yaml" {
            Role::Whole(MAX_USERS_BYTES)
        } else if self.sums_path.as_deref() == Some(rel) {
            Role::Whole(MAX_SUMS_BYTES)
        } else if self.read_history
            && rel
                .strip_prefix("history/")
                .is_some_and(|n| !n.contains('/') && n.ends_with(".jsonl"))
        {
            Role::History
        } else {
            Role::Other
        }
    }

    /// A file that is no content: read whole when the plan reads it, hashed when checksums are to be verified.
    fn other(&mut self, rel: &str, size: u64, file: File) -> Result<(), String> {
        let (bytes, digest) = match self.role(rel) {
            Role::Whole(cap) => {
                let bytes = read_all(file, cap)?;
                let digest = sha256(&bytes);
                (Some(bytes), Some(digest))
            }
            Role::History => {
                if self.history_bytes.saturating_add(size) > self.history_cap {
                    // Said once, by the plan, and not read: the history is over its cap.
                    self.history_over = true;
                    self.files.insert(rel.to_owned(), Seen { digest: None });
                    return Ok(());
                }
                let bytes = read_all(file, self.history_cap)?;
                self.history_bytes += bytes.len() as u64;
                let digest = sha256(&bytes);
                (Some(bytes), Some(digest))
            }
            Role::Other if self.verify => (None, Some(hash_all(file, MAX_OTHER_BYTES)?)),
            Role::Other => (None, None),
        };
        self.files.insert(rel.to_owned(), Seen { digest });
        if let Some(bytes) = bytes {
            self.read.insert(rel.to_owned(), bytes);
        }
        Ok(())
    }

    /// A knowledge file, read by the walk: its size and digest.
    pub(super) fn seen(&mut self, rel: &str, digest: Option<Digest>) {
        self.files.insert(rel.to_owned(), Seen { digest });
    }

    /// A page the walk accepted: what the plan reads of it. `bytes` is UTF-8 text with a frontmatter that parses.
    pub(super) fn page(&mut self, rel: &str, bytes: &[u8], id: Option<String>) {
        let digest = sha256(bytes);
        self.files.insert(
            rel.to_owned(),
            Seen {
                digest: Some(digest),
            },
        );
        let (mut license, mut prefixes) = (None, BTreeMap::new());
        if let Ok(text) = std::str::from_utf8(bytes) {
            license = inklkunn_markup::parse_metadata(text)
                .ok()
                .and_then(|m| m.license);
            for link in inklkunn_markup::scan(text).links {
                if let Some(prefix) = link.prefix {
                    *prefixes.entry(prefix).or_insert(0) += 1;
                }
            }
        }
        self.pages.insert(
            rel.to_owned(),
            PageFacts {
                id,
                license,
                sha256: digest,
                prefixes,
            },
        );
    }

    /// An attachment: a sidecar is read (the licence is in it), and anything else is hashed when checksums are to be verified.
    pub(super) fn attachment(&mut self, rel: &str, file: File, cap: u64) -> Result<(), String> {
        if rel.ends_with(".meta.yaml") {
            let bytes = read_all(file, MAX_SIDECAR_BYTES)?;
            let digest = sha256(&bytes);
            self.files.insert(
                rel.to_owned(),
                Seen {
                    digest: Some(digest),
                },
            );
            self.sidecars.insert(rel.to_owned(), bytes);
        } else if self.verify {
            let digest = hash_all(file, cap)?;
            self.files.insert(
                rel.to_owned(),
                Seen {
                    digest: Some(digest),
                },
            );
        } else {
            self.files.insert(rel.to_owned(), Seen { digest: None });
        }
        Ok(())
    }
}

impl Walk<'_> {
    /// A file of the bundle that is not content: opened and examined as every file of the walk is, then inventoried
    /// ([`Gathered::other`]).
    pub(super) fn other_file(&mut self, parent: &OwnedFd, name: &OsStr, rel: &str) {
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
        let size = opened.size;
        if size > MAX_OTHER_BYTES {
            self.refuse(
                rel,
                format!("is larger than the {} cap of a file", unit(MAX_OTHER_BYTES)),
            );
            return;
        }
        let Some(gathered) = self.bundle.as_mut() else {
            return;
        };
        if let Err(why) = gathered.other(rel, size, opened.file) {
            self.refuse(rel, why);
        }
    }
}

// ------------------------------------------------------------------------------------------------------- the plan

/// The manifest, read from the bundle's own handle (before the walk, which needs to know whether checksums are named).
fn read_manifest(
    root: &OwnedFd,
    top: &[(std::ffi::OsString, Result<FileType, rustix::io::Errno>)],
) -> Result<(Manifest, Vec<(String, inklkunn_markup::Value)>), Vec<String>> {
    let name = OsStr::new(crate::bundle::MANIFEST);
    if !top
        .iter()
        .any(|(n, kind)| n == name && matches!(kind, Ok(FileType::RegularFile)))
    {
        return Err(vec![
            "is missing: a bundle is described by its manifest (`wiki import-folder` adds a plain folder of pages to a wiki)"
                .to_owned(),
        ]);
    }
    let opened = open_regular(root, name).map_err(|why| vec![why])?;
    let bytes = read_all(opened.file, super::MAX_MANIFEST_BYTES).map_err(|why| vec![why])?;
    let text = String::from_utf8(bytes).map_err(|_| vec!["is not UTF-8 text".to_owned()])?;
    let value = inklkunn_markup::frontmatter::parse_yaml(&text)
        .map_err(|e| vec![format!("is not YAML in the plain subset ({e})")])?;
    let inklkunn_markup::Value::Map(pairs) = value else {
        return Err(vec!["is not a map of keys".to_owned()]);
    };
    manifest::read(&pairs).map(|m| (m, pairs))
}

fn refusal(name: &str, reason: impl Into<String>) -> Refusal {
    Refusal {
        name: shown(name),
        reason: reason.into(),
    }
}

/// The page path a history file named `stem` stands for, when the stem is a path (`~` and the title's path, every byte but
/// `A-Z a-z 0-9 . _ -` written `%XX`: the export's coding).
fn path_of_stem(stem: &str) -> Option<String> {
    let coded = stem.strip_prefix('~')?;
    let mut bytes: Vec<u8> = Vec::with_capacity(coded.len());
    let mut it = coded.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let hi = it.next()?;
            let lo = it.next()?;
            let hex = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            };
            bytes.push(hex(hi)? << 4 | hex(lo)?);
        } else if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            bytes.push(b);
        } else {
            return None;
        }
    }
    Some(format!("pages/{}.md", String::from_utf8(bytes).ok()?))
}

/// A page path a replayed revision names: the rules a file of that path would meet.
fn path_fault(path: &str) -> Option<String> {
    if let Some(why) = cap_fault(path) {
        return Some(why);
    }
    if let Err(rule) = check_change_path(path) {
        return Some(rule.to_owned());
    }
    if !path.starts_with("pages/") || !path.ends_with(".md") {
        return Some("is not a page path (`pages/….md`)".to_owned());
    }
    canonical_fault(path)
}

/// Where each revision of a page puts the page: the paths the records say (`inklkunn.path`), the renames' titles, and the page's
/// final path; the path a revision acts on (a deletion's is the path it removes). `None` where nothing says.
fn resolve_paths(
    revisions: &mut [Revision],
    final_path: Option<&str>,
) -> Result<(), (String, String)> {
    let n = revisions.len();
    // `after[i]`: the path of the page after revision i (None: not there, or not known).
    let mut after: Vec<Option<String>> = revisions.iter().map(|r| r.path.clone()).collect();
    let mut deleted: Vec<bool> = revisions
        .iter()
        .map(|r| matches!(r.step, Step::Delete))
        .collect();
    let title_path = |title: &str| title_to_path(title).ok();
    if let (Some(last), Some(p)) = (after.last_mut(), final_path)
        && last.is_none()
        && !deleted.last().copied().unwrap_or(false)
    {
        *last = Some(p.to_owned());
    }
    for i in 0..n {
        let Some(rev) = revisions.get(i) else {
            continue;
        };
        let (before, here) = (
            i.checked_sub(1)
                .and_then(|j| after.get(j).cloned().flatten()),
            after.get(i).cloned().flatten(),
        );
        if let Step::Rename { to, .. } = &rev.step {
            let Some(to_path) = title_path(to) else {
                return Err((
                    rev.rev.clone(),
                    "has a rename whose `to` is not a page title".to_owned(),
                ));
            };
            if here.as_deref().is_some_and(|h| h != to_path) {
                return Err((
                    rev.rev.clone(),
                    "has a rename whose `to` is not the path the page has afterwards".to_owned(),
                ));
            }
            if let Some(slot) = after.get_mut(i) {
                *slot = Some(to_path);
            }
        } else if here.is_none() && !deleted.get(i).copied().unwrap_or(false) {
            // Not moved here: the page is where it was before.
            if let Some(slot) = after.get_mut(i) {
                *slot = before;
            }
        }
    }
    // Backwards: the path before a revision is the one a rename moved from, else the one after it.
    for i in (0..n).rev() {
        let Some(rev) = revisions.get(i) else {
            continue;
        };
        let prior = match &rev.step {
            Step::Rename { from, .. } => {
                let Some(from_path) = title_path(from) else {
                    return Err((
                        rev.rev.clone(),
                        "has a rename whose `from` is not a page title".to_owned(),
                    ));
                };
                Some(from_path)
            }
            Step::Delete => None,
            _ => after.get(i).cloned().flatten(),
        };
        if let Some(j) = i.checked_sub(1)
            && let Some(slot) = after.get_mut(j)
            && slot.is_none()
            && !deleted.get(j).copied().unwrap_or(false)
        {
            *slot = prior;
        }
    }
    // What each revision acts on.
    for (i, rev) in revisions.iter_mut().enumerate() {
        let acts_on = if deleted.get(i).copied().unwrap_or(false) {
            i.checked_sub(1)
                .and_then(|j| after.get(j).cloned().flatten())
                .or_else(|| rev.path.clone())
        } else {
            after.get(i).cloned().flatten()
        };
        rev.path = acts_on;
    }
    deleted.clear();
    Ok(())
}

/// Plan the import of the bundle at `folder`: walk it, read and judge everything, and report. Writes nothing, anywhere.
/// `config` is the operator's: a page is judged under its `page_max_bytes` at **every revision**.
pub fn plan_bundle(
    folder: &Path,
    config: &ReadConfig,
    options: &BundleOptions,
) -> Result<BundlePlan, ImportError> {
    let root = open_root(folder).map_err(|e| {
        if e == rustix::io::Errno::NOTDIR {
            ImportError::Folder("it is not a directory".to_owned())
        } else {
            ImportError::Folder(format!("it cannot be read ({})", kind_of(e)))
        }
    })?;
    let root_identity = dir_identity(&root)
        .map_err(|e| ImportError::Folder(format!("it cannot be read ({})", kind_of(e))))?;
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

    let mut plan = ImportPlan {
        root: folder.to_path_buf(),
        page_cap: config.page_max_bytes,
        ..ImportPlan::default()
    };
    // 1. The manifest decides how the rest is read (are there checksums? is `knowledge/` meaningful?), so it is read first; a
    // bundle without a usable one is refused with nothing else judged.
    let (manifest, pairs) = match read_manifest(&root, &top) {
        Ok(m) => m,
        Err(reasons) => {
            for reason in reasons {
                plan.refused.push(refusal(crate::bundle::MANIFEST, reason));
            }
            plan.refused.push(refusal(
                "(the bundle)",
                "nothing else was judged: the manifest says how the rest of the bundle is read",
            ));
            return Ok(BundlePlan {
                plan,
                report: BundleReport::default(),
                histories: Vec::new(),
                schedule: Vec::new(),
            });
        }
    };

    // 2. The walk.
    let existing = Existing::at("");
    let sums_path = manifest
        .checksums
        .clone()
        .filter(|p| manifest::is_a_path_inside(p));
    let knowledge_known = matches!(
        super::declared_extension(Some(&pairs)),
        super::Extension::Known
    );
    let mut walk = Walk {
        existing: &existing,
        replace_existing: false,
        page_cap: config.page_max_bytes,
        plan,
        keys: Default::default(),
        ids: Default::default(),
        pages: Vec::new(),
        bundle: Some(Gathered {
            verify: manifest.checksums.is_some(),
            sums_path,
            knowledge_known,
            read_history: !options.no_history,
            history_cap: options.max_history_bytes,
            ..Gathered::default()
        }),
    };
    walk.plan.dirs.insert(folder.to_path_buf(), root_identity);
    for (name, kind) in top {
        let Some(n) = name.to_str() else {
            walk.refuse(
                &lossy(&name),
                "the name is not UTF-8 (a repository path is ASCII)",
            );
            continue;
        };
        let path = folder.join(&name);
        match kind {
            Err(e) => walk.refuse(n, format!("cannot be examined ({})", kind_of(e))),
            Ok(FileType::Symlink) => walk.refuse(n, "is a symbolic link; links are never followed"),
            Ok(FileType::Directory) => walk.descend(&root, &name, &path, n, 0),
            Ok(FileType::RegularFile)
                if matches!(
                    n,
                    "pages" | "attachments" | "knowledge" | "history" | "extensions" | "governance"
                ) =>
            {
                walk.refuse(n, "is not a directory");
            }
            Ok(FileType::RegularFile) => walk.file(&root, &name, &path, n),
            Ok(_) => walk.refuse(n, "is not a regular file (a device, a pipe or a socket)"),
        }
    }
    let gathered = walk.bundle.take().unwrap_or_default();
    let mut plan = walk.plan;
    plan.files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(judge(plan, gathered, &manifest, options, config))
}

/// What the walk refused, by name: files that were refused by a rule are not also "missing" or "extra".
fn names_refused(plan: &ImportPlan) -> BTreeSet<String> {
    plan.refused.iter().map(|r| r.name.clone()).collect()
}

fn judge(
    plan: ImportPlan,
    gathered: Gathered,
    manifest: &Manifest,
    options: &BundleOptions,
    config: &ReadConfig,
) -> BundlePlan {
    let mut refusals: Vec<Refusal> = Vec::new();
    let mut report = BundleReport {
        bundle_version: shown(&manifest.version),
        generator: manifest.generator.as_deref().map(shown),
        exported_at: manifest.exported_at.as_deref().map(shown),
        source_name: manifest.source_name.as_deref().map(shown),
        source_url: manifest.source_url.as_deref().map(shown),
        source_engine: manifest.source_engine.as_deref().map(shown),
        pseudonymized: manifest.pseudonymized,
        languages: manifest.languages.iter().map(|l| shown(l)).collect(),
        visibility_default: manifest.visibility_default.clone(),
        unknown_manifest_keys: manifest.unknown_keys.clone(),
        fidelity_declared: manifest.fidelity_level,
        degraded_at_source: manifest.degraded.iter().map(|c| shown(c)).collect(),
        dropped_at_source: manifest.dropped.iter().map(|c| shown(c)).collect(),
        no_history: options.no_history,
        max_revisions: options.max_revisions,
        interwiki_in_bundle: manifest.interwiki_prefixes,
        ..BundleReport::default()
    };
    let walked_refusals = names_refused(&plan);

    // --- the licence the wiki takes (D-4)
    let wiki_license: Option<String> = {
        let given = options.bundle_license.as_deref();
        let declared = manifest.license.as_deref();
        let chosen = match (declared, given) {
            (Some(d), Some(g)) if !d.eq_ignore_ascii_case(g) => {
                refusals.push(refusal(
                    crate::bundle::MANIFEST,
                    format!(
                        "declares the licence `{}`, and --bundle-license says `{}`: that option is for a bundle that declares none, and a licence is never changed on the way in",
                        shown(d),
                        shown(g)
                    ),
                ));
                None
            }
            (Some(d), _) => Some((d, LicenseFrom::Manifest, crate::bundle::MANIFEST)),
            (None, Some(g)) => Some((g, LicenseFrom::DeclaredByAdmin, "--bundle-license")),
            (None, None) => {
                refusals.push(refusal(
                    crate::bundle::MANIFEST,
                    "declares no `license`, and the wiki must take the bundle's licence: the admin who knows the source's licence declares it with --bundle-license <SPDX id>",
                ));
                None
            }
        };
        chosen.and_then(|(id, from, whose)| {
            if !licence::has_the_shape_of_an_id(id) {
                refusals.push(refusal(whose, "gives a licence that is not an SPDX identifier's shape"));
                None
            } else if !licence::may_be_a_default(id) {
                refusals.push(refusal(
                    whose,
                    format!(
                        "gives the licence `{}`, which no wiki can have as its default (nothing could be imported into it: NonCommercial, NoDerivatives, GFDL and unknown licences go into none; CC0, CC BY and CC BY-SA are the licences of a wiki)",
                        shown(id)
                    ),
                ));
                None
            } else {
                report.license = Some(shown(id));
                report.license_from = Some(from);
                Some(id.to_owned())
            }
        })
    };

    // --- checksums, strictly when present (D-5)
    if let Some(named) = &manifest.checksums {
        judge_checksums(
            named,
            &gathered,
            &walked_refusals,
            &mut refusals,
            &mut report,
        );
    }

    // --- people: users.yaml
    let mut people: BTreeMap<String, (String, String)> = BTreeMap::new(); // key → (display name, kind)
    if let Some(bytes) = gathered.read.get("users.yaml") {
        match std::str::from_utf8(bytes)
            .map_err(|_| vec!["is not UTF-8 text".to_owned()])
            .and_then(|text| {
                inklkunn_markup::frontmatter::parse_yaml(text)
                    .map_err(|e| vec![format!("is not YAML in the plain subset ({e})")])
            })
            .and_then(|value| manifest::read_people(&value))
        {
            Ok(list) => {
                report.people.in_users_file = list.people.len();
                for person in &list.people {
                    let key = person
                        .id
                        .clone()
                        .or_else(|| person.names.first().cloned())
                        .unwrap_or_else(|| format!("entry {}", person.entry));
                    let display = person.names.first().cloned().unwrap_or_else(|| key.clone());
                    people.entry(key).or_insert((
                        display,
                        person
                            .kind
                            .clone()
                            .unwrap_or_else(|| "unspecified".to_owned()),
                    ));
                }
            }
            Err(reasons) => {
                for reason in reasons {
                    refusals.push(refusal("users.yaml", reason));
                }
            }
        }
    }

    // --- pages and attachments of the first change, and the licence of each file
    let page_paths: Vec<String> = plan
        .files
        .iter()
        .filter(|f| f.path.starts_with("pages/"))
        .map(|f| f.path.clone())
        .collect();
    let mut by_id: BTreeMap<&str, &str> = BTreeMap::new();
    for path in &page_paths {
        if let Some(id) = gathered.pages.get(path).and_then(|f| f.id.as_deref()) {
            by_id.insert(id, path.as_str());
        }
    }

    // --- histories (D-2, D-3, D-5)
    let mut histories: Vec<PageHistory> = Vec::new();
    let mut history_licences: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new(); // page path → (licence, rev)
    let mut history_of_page: BTreeMap<String, String> = BTreeMap::new(); // page path → file
    let mut unknown_fields: BTreeSet<String> = BTreeSet::new();
    let history_files: Vec<(&String, &Vec<u8>)> = gathered
        .read
        .iter()
        .filter(|(rel, _)| rel.starts_with("history/"))
        .collect();
    if gathered.history_over {
        refusals.push(refusal(
            "history/",
            format!(
                "holds more than the {} cap on the bytes of the history (INKLKUNN_IMPORT_MAX_HISTORY_MIB); `--no-history` imports the pages as they are, in one change",
                unit(options.max_history_bytes)
            ),
        ));
    }
    for (rel, bytes) in history_files {
        let stem = rel
            .strip_prefix("history/")
            .and_then(|n| n.strip_suffix(".jsonl"))
            .unwrap_or_default();
        let records = match history::read(bytes) {
            Ok(r) => r,
            Err(fault) => {
                refusals.push(refusal(rel, format!("{}: {}", fault.place(), fault.rule)));
                continue;
            }
        };
        report.records += records.len();
        // The page the file is of: by its id, or by the path its name codes.
        let page: Option<String> = match path_of_stem(stem) {
            Some(path) => gathered.pages.contains_key(&path).then_some(path),
            None => by_id.get(stem).map(|p| (*p).to_owned()),
        };
        if let Some(page) = &page
            && let Some(first) = history_of_page.get(page)
        {
            refusals.push(refusal(
                rel,
                format!(
                    "is a second history file of the page `{}`, which `{}` has already",
                    shown(page),
                    shown(first)
                ),
            ));
            continue;
        }
        let mut judged = match history::judge(&records, config.page_max_bytes) {
            Ok(j) => j,
            Err((rev, rule)) => {
                let at = rev.map_or(String::new(), |r| format!("rev `{}` ", shown(&r)));
                refusals.push(refusal(rel, format!("{at}{rule}")));
                continue;
            }
        };
        if let Some(rev) = &judged.address_in_author {
            refusals.push(refusal(
                rel,
                format!(
                    "rev `{}` has an author that is an e-mail address; addresses are not imported, so the bundle must not carry one",
                    shown(rev)
                ),
            ));
            continue;
        }
        if let Err((rev, rule)) = resolve_paths(&mut judged.revisions, page.as_deref()) {
            refusals.push(refusal(rel, format!("rev `{}` {rule}", shown(&rev))));
            continue;
        }
        let mut bad_path = false;
        for r in &judged.revisions {
            if matches!(r.step, Step::NotReplayed(_)) {
                continue;
            }
            if let Some(path) = &r.path
                && let Some(why) = path_fault(path)
            {
                refusals.push(refusal(
                    rel,
                    format!(
                        "rev `{}` is at a path that cannot be a page's: it {why}",
                        shown(&r.rev)
                    ),
                ));
                bad_path = true;
                break;
            }
        }
        if bad_path {
            continue;
        }
        unknown_fields.extend(judged.unknown_members.iter().map(|n| shown_up_to(n, 60)));
        if let Some(p) = &page {
            history_of_page.insert(p.clone(), rel.clone());
            history_licences.insert(p.clone(), judged.licences.clone());
        } else {
            // A page that is not among the bundle's files (removed since): its revisions' licences count under the file's name.
            history_licences.insert(rel.clone(), judged.licences.clone());
        }
        histories.push(PageHistory {
            file: rel.clone(),
            page,
            revisions: judged.revisions,
            last_text: judged.last_text,
        });
    }
    finish(
        plan,
        &gathered,
        report,
        refusals,
        histories,
        &history_licences,
        people,
        wiki_license.as_deref(),
        manifest,
        options,
        unknown_fields,
    )
}

fn judge_checksums(
    named: &str,
    gathered: &Gathered,
    walked_refusals: &BTreeSet<String>,
    refusals: &mut Vec<Refusal>,
    report: &mut BundleReport,
) {
    let manifest_name = crate::bundle::MANIFEST;
    if !manifest::is_a_path_inside(named) {
        refusals.push(refusal(
            manifest_name,
            "has a `checksums` that is not a path inside the bundle",
        ));
        return;
    }
    let Some(bytes) = gathered.read.get(named) else {
        refusals.push(refusal(
            named,
            "is the file of checksums the manifest names, and it is not in the bundle",
        ));
        return;
    };
    let sums = match std::str::from_utf8(bytes)
        .map_err(|_| manifest::SumsFault {
            line: 0,
            rule: "is not UTF-8 text",
        })
        .and_then(manifest::read_sums)
    {
        Ok(s) => s,
        Err(fault) => {
            refusals.push(refusal(
                named,
                format!("line {}: {}", fault.line, fault.rule),
            ));
            return;
        }
    };
    let named_paths: BTreeSet<&str> = sums.iter().map(|(p, _)| p.as_str()).collect();
    for (path, digest) in &sums {
        if walked_refusals.contains(&super::shown(path)) {
            continue;
        }
        match gathered.files.get(path) {
            None => refusals.push(refusal(
                path,
                "is named in the checksums and is missing from the bundle",
            )),
            Some(Seen {
                digest: Some(have), ..
            }) if have != digest => refusals.push(refusal(
                path,
                "has a SHA-256 that is not the one the checksums file gives it",
            )),
            Some(_) => {}
        }
    }
    for path in gathered.files.keys() {
        if named_paths.contains(path.as_str())
            || path == named
            || path == crate::bundle::MANIFEST
            || walked_refusals.contains(&super::shown(path))
        {
            continue;
        }
        let hint = if path == "import-report.md" || path == "export-report.md" {
            " (wiki-commons' tools write this report after the checksums: run them with `--no-report`, or take the file out)"
        } else {
            ""
        };
        refusals.push(refusal(
            path,
            format!("is in the bundle and the checksums file does not name it{hint}"),
        ));
    }
    report.checksums = Checksums::Verified {
        file: shown(named),
        entries: sums.len(),
    };
}

#[allow(clippy::too_many_arguments)]
fn finish(
    mut plan: ImportPlan,
    gathered: &Gathered,
    mut report: BundleReport,
    mut refusals: Vec<Refusal>,
    mut histories: Vec<PageHistory>,
    history_licences: &BTreeMap<String, Vec<(String, String)>>,
    mut people: BTreeMap<String, (String, String)>,
    wiki_license: Option<&str>,
    manifest: &Manifest,
    options: &BundleOptions,
    unknown_fields: BTreeSet<String>,
) -> BundlePlan {
    // --- the licence of each file against the wiki's (D-4)
    let mut incompatible: Vec<Incompatible> = Vec::new();
    let mut left_out_paths: BTreeSet<String> = BTreeSet::new();
    if let Some(wiki) = wiki_license {
        for f in &plan.files {
            if f.path.starts_with("pages/") {
                let facts = gathered.pages.get(&f.path);
                if let Some(own) = facts.and_then(|p| p.license.as_deref())
                    && !licence::goes_into(own, wiki)
                {
                    incompatible.push(Incompatible {
                        kind: FileKind::Page,
                        path: shown(&f.path),
                        license: shown(own),
                        rev: None,
                    });
                    left_out_paths.insert(f.path.clone());
                    continue;
                }
                if !options.no_history
                    && let Some((own, rev)) = history_licences
                        .get(&f.path)
                        .into_iter()
                        .flatten()
                        .find(|(l, _)| !licence::goes_into(l, wiki))
                {
                    incompatible.push(Incompatible {
                        kind: FileKind::Page,
                        path: shown(&f.path),
                        license: shown(own),
                        rev: Some(shown(rev)),
                    });
                    left_out_paths.insert(f.path.clone());
                }
            }
        }
        // An attachment's own licence is in its sidecar.
        let planned: BTreeSet<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
        for (sidecar, bytes) in &gathered.sidecars {
            let Some(base) = sidecar.strip_suffix(".meta.yaml") else {
                continue;
            };
            if !planned.contains(base) || !planned.contains(sidecar.as_str()) {
                continue;
            }
            let parsed = std::str::from_utf8(bytes)
                .ok()
                .and_then(|t| inklkunn_markup::frontmatter::parse_yaml(t).ok());
            let Some(inklkunn_markup::Value::Map(members)) = parsed else {
                refusals.push(refusal(
                    sidecar,
                    "is not a map of keys in YAML's plain subset",
                ));
                continue;
            };
            let value = members.iter().find(|(k, _)| k == "license").map(|(_, v)| v);
            match value {
                None | Some(inklkunn_markup::Value::Null) => {}
                Some(inklkunn_markup::Value::Str(own)) => {
                    if !licence::goes_into(own, wiki) {
                        incompatible.push(Incompatible {
                            kind: FileKind::Attachment,
                            path: shown(base),
                            license: shown(own),
                            rev: None,
                        });
                        left_out_paths.insert(base.to_owned());
                        left_out_paths.insert(sidecar.clone());
                    }
                }
                Some(_) => refusals.push(refusal(sidecar, "has a `license` that is not text")),
            }
        }
    }
    report.incompatible_left_out = options.leave_out_incompatible;
    if !incompatible.is_empty() && !options.leave_out_incompatible {
        let wiki = wiki_license.as_deref().unwrap_or_default();
        for i in &incompatible {
            refusals.push(refusal(
                &i.path,
                format!(
                    "has the licence `{}`{}, which cannot go into a wiki whose licence is `{}`; `--leave-out-incompatible` leaves such files out, and nothing imports them",
                    i.license,
                    i.rev
                        .as_ref()
                        .map_or(String::new(), |r| format!(" at revision `{r}`")),
                    shown(wiki)
                ),
            ));
        }
    }
    report.incompatible = incompatible;
    if options.leave_out_incompatible && !left_out_paths.is_empty() {
        plan.files.retain(|f| !left_out_paths.contains(&f.path));
        histories.retain(|h| !h.page.as_ref().is_some_and(|p| left_out_paths.contains(p)));
        report.left_out.push(LeftOut {
            what: "files with an incompatible licence".to_owned(),
            files: left_out_paths.len(),
            why: "--leave-out-incompatible: left out with their history, named above".to_owned(),
        });
    }

    // --- what the history comes to
    let mut replayed_pages: BTreeSet<String> = BTreeSet::new();
    let mut identity: BTreeMap<String, (String, String)> = BTreeMap::new();
    for h in &histories {
        let (mut replayed, mut kept) = (0_usize, 0_usize);
        for r in &h.revisions {
            kept += 1;
            match &r.step {
                Step::NotReplayed(why) => match why {
                    Skipped::SuppressedContent => report.suppressed_content += 1,
                    Skipped::Withheld(reason) => {
                        *report.withheld.entry(shown_up_to(reason, 60)).or_insert(0) += 1;
                    }
                    Skipped::NoText => {
                        *report
                            .not_replayed
                            .entry("with no text".to_owned())
                            .or_insert(0) += 1;
                    }
                    Skipped::Type(t) => {
                        *report
                            .not_replayed
                            .entry(format!("of type `{}`", shown_up_to(t, 40)))
                            .or_insert(0) += 1;
                    }
                },
                _ => replayed += 1,
            }
            if !matches!(r.step, Step::NotReplayed(Skipped::SuppressedContent)) {
                if r.hidden.contains(&Part::Author) {
                    report.suppressed_author += 1;
                }
                if r.hidden.contains(&Part::Summary) {
                    report.suppressed_summary += 1;
                }
            } else if r.hidden.contains(&Part::Author) {
                report.suppressed_author += 1;
            }
            if let Some(a) = &r.author
                && let Some(key) = a.identity()
            {
                let kind = a.kind.clone().unwrap_or_else(|| {
                    if a.anonymous {
                        "anonymous".to_owned()
                    } else {
                        "unspecified".to_owned()
                    }
                });
                identity
                    .entry(key.clone())
                    .or_insert((a.display().unwrap_or(key), kind));
            }
        }
        report.revisions += replayed;
        if replayed > 0
            && let Some(page) = &h.page
        {
            replayed_pages.insert(page.clone());
        }
        report.histories.push(PageLine {
            page: shown(h.page.as_deref().unwrap_or(&h.file)),
            records: kept,
            revisions: replayed,
        });
    }
    for (k, v) in identity {
        people.entry(k).or_insert(v);
    }
    report.unknown_record_fields = unknown_fields.into_iter().collect();
    if !options.no_history && report.revisions as u64 > options.max_revisions {
        refusals.push(refusal(
            "history/",
            format!(
                "would make {} blocks, and the cap is {} (INKLKUNN_IMPORT_MAX_REVISIONS); `--no-history` imports the pages as they are, in one change",
                report.revisions, options.max_revisions
            ),
        ));
    }
    report.estimate_seconds = (!options.no_history && report.revisions > 0)
        .then(|| BundleReport::estimate_for(report.revisions as u64));
    report.fidelity_found = if report.revisions > 0 {
        Found::History
    } else {
        Found::ContentOnly
    };
    report.history_bytes = gathered.history_bytes;

    // --- pages: who comes in with the first change, who needs a last one
    let kept_pages: Vec<String> = plan
        .files
        .iter()
        .filter(|f| f.path.starts_with("pages/"))
        .map(|f| f.path.clone())
        .collect();
    let mut first_change: Vec<String> = Vec::new();
    for path in &kept_pages {
        let has_history = replayed_pages.contains(path) && !options.no_history;
        if !has_history {
            report.pages_without_history.push(shown(path));
            first_change.push(path.clone());
            continue;
        }
        let last = histories
            .iter()
            .find(|h| h.page.as_deref() == Some(path.as_str()))
            .and_then(|h| h.last_text);
        let final_sha = gathered.pages.get(path).map(|p| p.sha256);
        if last.is_none() || last != final_sha {
            report.final_state_pages.push(shown(path));
        }
    }
    report.pages = kept_pages.len();
    let sidecars: Vec<&str> = plan
        .files
        .iter()
        .filter(|f| f.path.ends_with(".meta.yaml") && f.path.starts_with("attachments/"))
        .map(|f| f.path.as_str())
        .collect();
    let sidecar_of_planned = |path: &str| {
        path.strip_suffix(".meta.yaml")
            .is_some_and(|base| plan.files.iter().any(|f| f.path == base))
    };
    report.sidecars = sidecars.iter().filter(|p| sidecar_of_planned(p)).count();
    let attachments_total = plan
        .files
        .iter()
        .filter(|f| f.path.starts_with("attachments/"))
        .count();
    report.attachments = attachments_total - report.sidecars;
    report.knowledge_files = plan.knowledge_count();

    // --- the caps of one acceptance: the first change and the last
    let (mut first_files, mut first_bytes) = (0_usize, 0_u64);
    for f in &plan.files {
        if !f.path.starts_with("pages/") || first_change.contains(&f.path) {
            first_files += 1;
            first_bytes = first_bytes.saturating_add(f.size);
        }
    }
    if first_files > MAX_CHANGE_FILES {
        refusals.push(refusal(
            "(the first change)",
            format!("{first_files} files; one acceptance carries at most {MAX_CHANGE_FILES}"),
        ));
    }
    if first_bytes > MAX_CHANGE_BYTES {
        refusals.push(refusal(
            "(the first change)",
            format!(
                "{}; one acceptance carries at most {}",
                unit(first_bytes),
                unit(MAX_CHANGE_BYTES)
            ),
        ));
    }
    if report.final_state_pages.len() > MAX_CHANGE_FILES {
        refusals.push(refusal(
            "(the change at the end)",
            format!(
                "{} pages end differently from their last revision; one acceptance carries at most {MAX_CHANGE_FILES}",
                report.final_state_pages.len()
            ),
        ));
    }

    // --- links no map resolves
    let namespaces: BTreeSet<&str> = manifest.namespaces.iter().map(String::as_str).collect();
    for path in &kept_pages {
        if let Some(facts) = gathered.pages.get(path) {
            for (prefix, n) in &facts.prefixes {
                if !namespaces.contains(prefix.as_str()) {
                    *report
                        .unmapped_prefixes
                        .entry(shown_up_to(prefix, 60))
                        .or_insert(0) += *n;
                }
            }
        }
    }

    // --- what is left out
    let used_history = !options.no_history;
    let mut groups: BTreeMap<String, (usize, &'static str)> = BTreeMap::new();
    for rel in gathered.files.keys() {
        if plan.files.iter().any(|f| &f.path == rel) || left_out_paths.contains(rel) {
            continue;
        }
        if rel == crate::bundle::MANIFEST
            || rel == "users.yaml"
            || gathered.sums_path.as_deref() == Some(rel.as_str())
        {
            continue;
        }
        let top = rel.split('/').next().unwrap_or(rel);
        let (what, why): (String, &'static str) = if rel.rsplit('/').next() == Some(".keep") {
            ("keep-files".to_owned(), "every wiki makes its own")
        } else if top == "history" {
            if used_history
                && rel
                    .strip_prefix("history/")
                    .is_some_and(|n| !n.contains('/') && n.ends_with(".jsonl"))
            {
                continue;
            }
            (
                "history/".to_owned(),
                if used_history {
                    "not a history file (`history/<page>.jsonl`)"
                } else {
                    "--no-history: the history is not replayed"
                },
            )
        } else {
            match top {
                "extensions" => (
                    "extensions/".to_owned(),
                    "the exporting service's own data (claim threads, how views were made); not imported",
                ),
                "governance" => (
                    "governance/".to_owned(),
                    "a wiki makes its own rules; the interwiki map is imported by its own task (RFC 007 D-8)",
                ),
                "discussions" => (
                    "discussions/".to_owned(),
                    "discussions are fidelity level 4, not imported",
                ),
                "structured" => (
                    "structured/".to_owned(),
                    "structured data is fidelity level 5, not imported",
                ),
                "knowledge" => (
                    "knowledge/".to_owned(),
                    "the manifest does not declare the knowledge extension this build reads, so nothing in it is read",
                ),
                ".prikkignore" => (".prikkignore".to_owned(), "service-local"),
                _ => (shown(rel), "not part of the bundle layout"),
            }
        };
        let entry = groups.entry(what).or_insert((0, why));
        entry.0 += 1;
    }
    for (what, (files, why)) in groups {
        report.left_out.push(LeftOut {
            what,
            files,
            why: why.to_owned(),
        });
    }

    // --- people
    report.people.total = people.len();
    for (_, kind) in people.values() {
        *report
            .people
            .by_kind
            .entry(shown_up_to(kind, 30))
            .or_insert(0) += 1;
    }
    report.people.names = people
        .values()
        .take(MAX_LISTED)
        .map(|(name, _)| shown(name))
        .collect();

    if plan.files.is_empty() && refusals.is_empty() && plan.refused.is_empty() {
        refusals.push(refusal(
            "(the bundle)",
            "holds no page and no attachment to import",
        ));
    }

    // --- the order of the replay
    let mut schedule: Vec<(usize, usize)> = Vec::new();
    for (h, history) in histories.iter().enumerate() {
        for (r, rev) in history.revisions.iter().enumerate() {
            if !matches!(rev.step, Step::NotReplayed(_)) {
                schedule.push((h, r));
            }
        }
    }
    schedule.sort_by(|a, b| {
        let key = |&(h, r): &(usize, usize)| {
            let history = histories.get(h);
            let rev = history.and_then(|x| x.revisions.get(r));
            (
                rev.map(|v| v.at),
                rev.and_then(|v| v.path.clone()),
                history.map(|x| x.file.clone()),
                r,
            )
        };
        key(a).cmp(&key(b))
    });

    // The plan's refusals are the walk's and the judgement's together, by name.
    plan.refused.extend(refusals);
    plan.refused
        .sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.reason.cmp(&b.reason)));
    BundlePlan {
        plan,
        report,
        histories,
        schedule,
    }
}

#[cfg(test)]
mod tests;
