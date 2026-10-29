//! The decrypted seeds' only home (RFC 002 D-3; operations OP-07): a directory made for **one write call**,
//! on a tmpfs, owned by the server and nobody else, holding the **one** seed file the call signs with (the
//! other role's seed is never in a call that does not use it), removed when the call is over.
//!
//! - **tmpfs, checked.** The base directory's filesystem is read from `/proc/self/mountinfo` and must be
//!   `tmpfs` or `ramfs`; a seed is never written to a disk the operator did not choose to trust with it.
//! - **A name nobody can pre-create.** The directory is `inklkunn-seeds-<128 random bits>`, made with `mkdir`
//!   (which refuses an existing path, a symlink included) and mode 0700; the files are made exclusively with
//!   mode 0600. Nothing here ever opens a path another user could have put there first.
//! - **Removed, and told if not.** [`SeedDir::finish`] overwrites each file with zeros, unlinks it, removes
//!   the directory, and reports a failure instead of swallowing it. [`Drop`] does the same on a panic or an
//!   early return, where nothing can be reported.
//! - **Never named.** No error text and no `Debug` output carries the directory's path or a seed.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use super::KeyStoreError;
use super::random;
use super::record::Seeds;
use crate::keys::{KeyDir, Role};

/// What every seed directory's name starts with: the operator can find and remove a stray one by it.
pub(crate) const DIR_PREFIX: &str = "inklkunn-seeds-";

/// A tmpfs directory under which seed directories are made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SeedBase {
    dir: PathBuf,
}

impl SeedBase {
    /// Accept `dir` as a seed base: absolute, existing, **owned by the user this process runs as and not
    /// writable by anyone else** (a directory another local user can write to lets them swap a call's
    /// directory for their own between its creation and its use), and on a tmpfs (or ramfs).
    pub(crate) fn tmpfs(dir: &Path) -> Result<Self, KeyStoreError> {
        let refuse = KeyStoreError::SeedBase;
        if !dir.is_absolute() {
            return Err(refuse("the seed directory must be an absolute path"));
        }
        let real = dir
            .canonicalize()
            .map_err(|_| refuse("the seed directory does not exist"))?;
        if !real.is_dir() {
            return Err(refuse("the seed directory is not a directory"));
        }
        let meta = std::fs::metadata(&real)
            .map_err(|_| refuse("the seed directory could not be examined"))?;
        // `/proc/self` is owned by the process's own user: the identity to compare with, without a syscall
        // this crate (which forbids `unsafe`) could not make.
        let me = std::fs::metadata("/proc/self")
            .map_err(|_| refuse("the process's own user could not be determined"))?
            .uid();
        if meta.uid() != me {
            return Err(refuse(
                "the seed directory must be owned by the user the service runs as",
            ));
        }
        if meta.mode() & 0o022 != 0 {
            return Err(refuse(
                "the seed directory must not be writable by its group or by others (mode 0700 is right)",
            ));
        }
        let mountinfo = std::fs::read_to_string("/proc/self/mountinfo")
            .map_err(|_| refuse("the mount table could not be read"))?;
        match fs_type_of(&mountinfo, &real).as_deref() {
            Some("tmpfs" | "ramfs") => Ok(SeedBase { dir: real }),
            Some(_) => Err(refuse(
                "the seed directory is not on a tmpfs; decrypted seeds are never written to a disk",
            )),
            None => Err(refuse("the mount of the seed directory could not be found")),
        }
    }

    /// The directory.
    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }
}

/// `\NNN` octal escapes in a `mountinfo` path (space is `\040`).
fn unescape(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        if c == b'\\' {
            let digits = b.get(i + 1..i + 4)?;
            let n = digits.iter().try_fold(0u32, |acc, d| match d {
                b'0'..=b'7' => Some(acc * 8 + u32::from(d - b'0')),
                _ => None,
            })?;
            out.push(u8::try_from(n).ok()?);
            i += 4;
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The filesystem type of the mount that holds `path`, from the text of `/proc/self/mountinfo`: the
/// deepest mount point that is a prefix of the path wins, and a later mount over the same point shadows an
/// earlier one.
pub(super) fn fs_type_of(mountinfo: &str, path: &Path) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in mountinfo.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let Some(point) = before.split(' ').nth(4).and_then(unescape) else {
            continue;
        };
        let Some(fstype) = after.split(' ').next().filter(|t| !t.is_empty()) else {
            continue;
        };
        let point = Path::new(&point);
        if path.starts_with(point) {
            let depth = point.components().count();
            if best.as_ref().is_none_or(|(d, _)| depth >= *d) {
                best = Some((depth, fstype.to_owned()));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Say that the call using `dir` is still running: its modification time is set to now. The start-up sweep
/// takes a directory not touched for five minutes to be a killed call's; an acceptance is many prikk calls
/// and can run longer than that, so every call touches the directory first.
pub(crate) fn touch(dir: &Path) {
    if let Ok(f) = std::fs::File::open(dir) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
}

impl SeedBase {
    /// A scratch directory of the same kind as a seed directory (private, random name, tmpfs), empty: for
    /// work that produces key material, such as generating a wiki's keys. Its name starts with
    /// [`DIR_PREFIX`], so the start-up sweep covers it.
    pub(crate) fn scratch(&self) -> Result<Scratch, KeyStoreError> {
        let name = format!(
            "{DIR_PREFIX}{}",
            random::hex(&random::bytes::<16>().map_err(|_| KeyStoreError::Random)?)
        );
        let dir = self.dir.join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|e| KeyStoreError::Io("a scratch directory could not be made", e.kind()))?;
        Ok(Scratch {
            dir,
            removed: false,
        })
    }

    /// Remove seed and scratch directories older than `older_than`: what a call leaves behind when the
    /// process was killed (`SIGKILL` runs no `Drop`). A directory is touched only if it is a real directory
    /// (never a link) whose name starts with [`DIR_PREFIX`]; a live call's directory is younger than the
    /// write deadline, so the age keeps another process's call safe. Returns how many were removed.
    pub(crate) fn sweep_stale(
        &self,
        older_than: std::time::Duration,
    ) -> Result<usize, KeyStoreError> {
        let entries = std::fs::read_dir(&self.dir)
            .map_err(|e| KeyStoreError::Io("the seed directory could not be listed", e.kind()))?;
        let mut removed = 0;
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with(DIR_PREFIX) {
                continue;
            }
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let age = meta
                .modified()
                .ok()
                .and_then(|t| t.elapsed().ok())
                .unwrap_or_default();
            if age < older_than {
                continue;
            }
            if remove_all(&path) {
                removed += 1;
            } else {
                return Err(KeyStoreError::SeedsNotRemoved);
            }
        }
        Ok(removed)
    }
}

/// A private, empty directory on the tmpfs for work that produces key material; removed when finished or
/// dropped. Its path is never in an error or `Debug` output.
pub(crate) struct Scratch {
    dir: PathBuf,
    removed: bool,
}

impl std::fmt::Debug for Scratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Scratch(..)")
    }
}

impl Scratch {
    /// The directory.
    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }

    /// Remove it (files overwritten, then unlinked) and report whether it is gone.
    pub(crate) fn finish(mut self) -> Result<(), KeyStoreError> {
        self.removed = true;
        if remove_all(&self.dir) {
            Ok(())
        } else {
            Err(KeyStoreError::SeedsNotRemoved)
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.removed {
            let _ = remove_all(&self.dir);
        }
    }
}

/// The seeds of one wiki, as files, for one call.
pub(crate) struct SeedDir {
    dir: PathBuf,
    key_dir: KeyDir,
    removed: bool,
}

impl std::fmt::Debug for SeedDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The path is where the seeds are: it stays out of every output (RFC 002 D-3).
        f.write_str("SeedDir(..)")
    }
}

/// Overwrite each file with zeros, unlink it, remove the directory. `false` if anything is left.
fn remove_all(dir: &Path) -> bool {
    let mut clean = true;
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(meta) = std::fs::symlink_metadata(&path)
                    && meta.is_file()
                    && let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&path)
                {
                    let zeros = vec![0u8; usize::try_from(meta.len()).unwrap_or(0)];
                    let _ = f.write_all(&zeros);
                    let _ = f.sync_all();
                }
                if std::fs::remove_file(&path).is_err() {
                    clean = false;
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => clean = false,
    }
    match std::fs::remove_dir(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => clean = false,
    }
    clean
}

impl SeedDir {
    /// Make the directory under `base` and write **one** seed in it, the `role`'s, with the key id prikk will be
    /// told. A call sees only the seed it signs with (RFC 002 C-1a, C-1c): the other role's seed is not in this
    /// directory, and its path is not in the call's environment.
    pub(crate) fn create(
        base: &SeedBase,
        role: Role,
        seeds: &Seeds,
        key_id: &str,
    ) -> Result<Self, KeyStoreError> {
        let name = format!(
            "{DIR_PREFIX}{}",
            random::hex(&random::bytes::<16>().map_err(|_| KeyStoreError::Random)?)
        );
        let dir = base.path().join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|e| KeyStoreError::Io("the seed directory could not be made", e.kind()))?;
        let write = || -> Result<(), KeyStoreError> {
            let bytes = match role {
                Role::Author => &seeds.author,
                Role::Maintainer => &seeds.maintainer,
            };
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(dir.join(format!("{}.seed", role.name())))
                .map_err(|e| KeyStoreError::Io("a seed file could not be made", e.kind()))?;
            f.write_all(bytes)
                .map_err(|e| KeyStoreError::Io("a seed file could not be written", e.kind()))
        };
        if let Err(e) = write() {
            let _ = remove_all(&dir);
            return Err(e);
        }
        match KeyDir::single(&dir, role, key_id) {
            Ok(key_dir) => Ok(SeedDir {
                dir,
                key_dir,
                removed: false,
            }),
            Err(_) => {
                let _ = remove_all(&dir);
                Err(KeyStoreError::SeedBase(
                    "the seed directory path is not usable",
                ))
            }
        }
    }

    /// The directory as the write sequence uses it: seed-file paths and key ids for prikk's environment.
    pub(crate) fn key_dir(&self) -> &KeyDir {
        &self.key_dir
    }

    /// Remove the directory and report whether it is gone. Call it after every use; [`Drop`] is the backstop.
    pub(crate) fn finish(mut self) -> Result<(), KeyStoreError> {
        self.removed = true;
        if remove_all(&self.dir) {
            Ok(())
        } else {
            Err(KeyStoreError::SeedsNotRemoved)
        }
    }
}

impl Drop for SeedDir {
    fn drop(&mut self) {
        if !self.removed {
            let _ = remove_all(&self.dir);
        }
    }
}
