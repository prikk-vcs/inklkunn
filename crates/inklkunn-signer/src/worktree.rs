//! Putting a prepared change into a wiki's worktree, and taking it back (RFC 002 D-4; threat model T-4,
//! C-4a): the signer is the only writer of a wiki's files, and it writes only what it has **checked**.
//!
//! - **[`Plan::new`] validates everything before a byte is written.** Each path is ASCII, relative, under
//!   one of the bundle's roots (or is the manifest), and obeys prikk's own path rules (so prikk's refusal at
//!   `commit` is the second line of defence and never the first); renames and writes do not collide; the
//!   change is bounded. A change that fails validation touches nothing.
//! - **Writes are atomic and never follow a link.** A file is written to a temporary name beside its
//!   destination (created exclusively) and renamed over it; every existing parent directory is checked to be
//!   a real directory, and an existing destination that is a link is refused.
//! - **Everything done is recorded to be undone.** Before a file is overwritten or removed its bytes are
//!   kept; [`Applied::undo`] puts the worktree back (the caller undoes renames with prikk, which cancels a
//!   declaration moved back: ledger PK-29). A change that fails before `commit` leaves the worktree as it
//!   was found, or says plainly that it could not.

use std::collections::HashSet;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use crate::accept::PreparedChange;

/// The directories of a wiki bundle a change may write under (RFC 001 D-5).
pub(crate) const ROOTS: &[&str] = &["pages", "attachments", "knowledge", "governance"];
/// The one file at the root of a bundle a change may write.
pub(crate) const MANIFEST: &str = "wiki-bundle.yaml";
/// Most files one change may carry: a bound against a runaway, generous for any import batch.
pub(crate) const MAX_FILES: usize = 10_000;
/// Most bytes one change may carry (a bound against a runaway; the policy limits are core's).
pub(crate) const MAX_TOTAL_BYTES: u64 = 512 << 20;

/// prikk's longest repository path, and longest component (`reference/path-safety.md`, RFC 125 §3).
const MAX_PATH: usize = 1024;
const MAX_COMPONENT: usize = 255;

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The `/`-separated form of a path a change may write, or the rule it broke. The rule never repeats the
/// path.
pub(crate) fn repo_path(p: &Path) -> Result<String, &'static str> {
    let mut parts: Vec<&str> = Vec::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str().ok_or("a path is not UTF-8")?),
            _ => return Err("a path is relative and has no `.` or `..` component"),
        }
    }
    if parts.is_empty() {
        return Err("a path is empty");
    }
    let joined = parts.join("/");
    // `Path` quietly drops a `.` or a doubled or trailing `/`; the path that is written must be the path that
    // was asked for, so only the plain spelling is accepted.
    if p.to_str() != Some(joined.as_str()) {
        return Err("a path is written plainly: no `.` component and no repeated or trailing `/`");
    }
    if joined.len() > MAX_PATH {
        return Err("a path is longer than prikk's 1024 bytes");
    }
    for part in &parts {
        if part.is_empty() || part.len() > MAX_COMPONENT {
            return Err("a path component is empty or longer than prikk's 255 bytes");
        }
        if !part.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
            return Err("a path is ASCII (a title is percent-encoded before it becomes a path)");
        }
        if part.contains(['\\', ':']) {
            return Err("a path has no backslash or colon");
        }
        if part.ends_with([' ', '.']) {
            return Err("a path component does not end in a space or a dot");
        }
        if part.starts_with('.') && *part != ".keep" {
            return Err("a path component does not start with a dot (but `.keep`)");
        }
        let stem = part.split('.').next().unwrap_or(part);
        if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
            return Err("a path component is a name Windows reserves");
        }
    }
    let first = parts.first().copied().unwrap_or("");
    let allowed = (ROOTS.contains(&first) && parts.len() >= 2) || joined == MANIFEST;
    if !allowed {
        return Err(
            "a path is under pages/, attachments/, knowledge/ or governance/, or is the manifest",
        );
    }
    Ok(joined)
}

/// A validated change, ready to apply.
#[derive(Debug)]
pub(crate) struct Plan {
    /// `(from, to)`, in the order given.
    pub(crate) renames: Vec<(String, String)>,
    /// `(path, bytes)` to write.
    pub(crate) writes: Vec<(String, Vec<u8>)>,
    /// Paths to remove.
    pub(crate) deletes: Vec<String>,
}

impl Plan {
    /// Validate `change`'s files and renames. Nothing is read from or written to disk.
    pub(crate) fn new(change: &PreparedChange) -> Result<Plan, &'static str> {
        if change.files.is_empty() && change.renames.is_empty() {
            return Err("the change names no file");
        }
        if change.files.len() > MAX_FILES || change.renames.len() > MAX_FILES {
            return Err("the change names more files than one acceptance may carry");
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut writes = Vec::new();
        let mut deletes = Vec::new();
        let mut total: u64 = 0;
        for f in &change.files {
            let path = repo_path(&f.path)?;
            if !seen.insert(path.clone()) {
                return Err("a path appears twice in the change");
            }
            match &f.content {
                Some(bytes) => {
                    total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
                    writes.push((path, bytes.clone()));
                }
                None => deletes.push(path),
            }
        }
        if total > MAX_TOTAL_BYTES {
            return Err("the change carries more bytes than one acceptance may");
        }
        let mut renames = Vec::new();
        let mut targets: HashSet<String> = HashSet::new();
        let mut sources: HashSet<String> = HashSet::new();
        for (from, to) in &change.renames {
            let (from, to) = (repo_path(from)?, repo_path(to)?);
            if from == to {
                return Err("a rename moves a page to another path");
            }
            if from == MANIFEST || to == MANIFEST {
                return Err("the manifest is not renamed");
            }
            if seen.contains(&from) {
                return Err("a renamed path is not also written or removed under its old name");
            }
            if !targets.insert(to.clone()) || !sources.insert(from.clone()) {
                return Err("a path is renamed twice");
            }
            renames.push((from, to));
        }
        if renames
            .iter()
            .any(|(from, to)| targets.contains(from) || sources.contains(to))
        {
            return Err("renames do not chain; send the final move");
        }
        Ok(Plan {
            renames,
            writes,
            deletes,
        })
    }
}

/// What has been done to a worktree, so that it can be undone.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    /// Files written or removed, with the bytes they held (`None`: they did not exist).
    saved: Vec<(PathBuf, Option<Vec<u8>>)>,
    /// Directories this change created, outermost first.
    made_dirs: Vec<PathBuf>,
}

fn io_text(what: &'static str, e: &std::io::Error) -> String {
    format!("{what} ({})", e.kind())
}

/// Check that every existing directory on the way to `dest` (below `root`) is a real directory and that
/// `dest` itself, if it exists, is a regular file.
fn check_route(root: &Path, dest: &Path) -> Result<(), String> {
    let mut at = root.to_path_buf();
    let rel = dest
        .strip_prefix(root)
        .map_err(|_| "a path left the worktree".to_owned())?;
    let comps: Vec<_> = rel.components().collect();
    for (i, c) in comps.iter().enumerate() {
        at.push(c);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if i + 1 < comps.len() => {
                if !m.is_dir() {
                    return Err("a path runs through something that is not a directory".to_owned());
                }
            }
            Ok(m) => {
                if !m.is_file() {
                    return Err("a path names something that is not a regular file".to_owned());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(io_text("a path could not be read", &e)),
        }
    }
    Ok(())
}

impl Applied {
    /// Write `bytes` to `rel` below `root`: parents created (and noted), the file made beside its
    /// destination and renamed over it.
    pub(crate) fn write(&mut self, root: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
        let dest = root.join(rel);
        check_route(root, &dest)?;
        let before = match std::fs::read(&dest) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(io_text(
                    "a file could not be read before it was replaced",
                    &e,
                ));
            }
        };
        if let Some(parent) = dest.parent() {
            self.make_dirs(root, parent)?;
        }
        let tmp = dest.with_file_name(format!(
            ".{}.inklkunn-tmp-{}",
            dest.file_name().and_then(|n| n.to_str()).unwrap_or("f"),
            std::process::id()
        ));
        let result = (|| -> std::io::Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o644)
                .open(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            std::fs::rename(&tmp, &dest)
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(io_text("a file could not be written", &e));
        }
        self.saved.push((dest, before));
        Ok(())
    }

    /// Make sure the directory `rel` will live in exists (a rename into a new directory), noting what was made.
    pub(crate) fn ensure_parent(&mut self, root: &Path, rel: &str) -> Result<(), String> {
        let dest = root.join(rel);
        check_route(root, &dest)?;
        match dest.parent() {
            Some(parent) => self.make_dirs(root, parent),
            None => Ok(()),
        }
    }

    /// Remove `rel` below `root` (it must be a regular file).
    pub(crate) fn delete(&mut self, root: &Path, rel: &str) -> Result<(), String> {
        let dest = root.join(rel);
        check_route(root, &dest)?;
        let before =
            std::fs::read(&dest).map_err(|e| io_text("a file to remove could not be read", &e))?;
        std::fs::remove_file(&dest).map_err(|e| io_text("a file could not be removed", &e))?;
        self.saved.push((dest, Some(before)));
        Ok(())
    }

    fn make_dirs(&mut self, root: &Path, dir: &Path) -> Result<(), String> {
        let rel = dir
            .strip_prefix(root)
            .map_err(|_| "a path left the worktree".to_owned())?;
        let mut at = root.to_path_buf();
        for c in rel.components() {
            at.push(c);
            match std::fs::symlink_metadata(&at) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => {
                    return Err("a path runs through something that is not a directory".to_owned());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&at)
                        .map_err(|e| io_text("a directory could not be made", &e))?;
                    let _ = std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755));
                    self.made_dirs.push(at.clone());
                }
                Err(e) => return Err(io_text("a path could not be read", &e)),
            }
        }
        Ok(())
    }

    /// Put back what this change wrote and removed, newest first, and take away the directories it made.
    /// `false` if anything could not be restored (the caller then says recovery is needed).
    pub(crate) fn undo(self) -> bool {
        let mut ok = true;
        for (path, before) in self.saved.into_iter().rev() {
            let r = match before {
                Some(bytes) => std::fs::write(&path, bytes),
                None => match std::fs::remove_file(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                },
            };
            if r.is_err() {
                ok = false;
            }
        }
        for dir in self.made_dirs.into_iter().rev() {
            // Only an empty directory is removed; one that holds anything else is not ours to take.
            let _ = std::fs::remove_dir(&dir);
        }
        ok
    }
}

#[cfg(test)]
mod tests;
