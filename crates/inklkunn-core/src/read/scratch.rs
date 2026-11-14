//! The extracts of the read path in a wiki's scratch directory, and who may remove them (handoff 005 corrections, sixth
//! round, K55; the fifth round's K51 redone).
//!
//! More than one process extracts into one wiki's scratch directory: the server, and the command line (`wiki
//! import-folder` builds a read service of its own and reads the wiki's files once more before it). The build lock of a
//! [`ReadService`] orders the extracts of one service only.
//!
//! **An extract is live while its lock is held.** Each extract has a lock file beside it (`<name>.lock`), created before
//! the extract is made. Its owner holds an exclusive `flock` on that file for the extract's whole life and lets go only
//! after the directory is removed. The sweep removes an extract only after taking its lock **without waiting**, and
//! removes the directory and then the lock file. An advisory lock on one filesystem holds between processes, containers
//! (PID namespaces) and users alike, and the kernel drops it when its holder dies, so a leftover is exactly an extract
//! whose lock is free. Nothing is decided from `/proc`, from a clock or from a guess, and a live extract is never half
//! removed: a read never meets a partial tree.
//!
//! The data directory must therefore be on a filesystem where `flock` works between the processes that share it (a local
//! filesystem or a bind mount of one), as the key store already needs. Where it does not, the lock cannot be taken and a
//! read says so (could not be read now); it never goes on without the lock.
//!
//! [`ReadService`]: super::ReadService

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use inklkunn_signer::flock::{Acquired, FileLock, try_exclusive};

use crate::lease::start_time;

/// The prefix of the read path's extracts.
const PREFIX: &str = "index-";

/// The suffix of an extract's lock file: `<name>.lock`, beside the directory `<name>`.
const LOCK_SUFFIX: &str = ".lock";

/// How many names are tried for one extract before it is given up (a name is taken only if another process chose the
/// same one at the same moment, or a sweep took the lock file between its creation and its lock).
const ATTEMPTS: usize = 16;

/// One count for the whole process, so that two read services in it (a test's, a command line's) never choose the same
/// name.
static EXTRACTS: AtomicU64 = AtomicU64::new(0);

/// A name for a new extract: `index-<pid>-<start time>-<n>`. The name only needs to be unlikely to be chosen twice at
/// once (the lock decides who owns it); the pid and start time keep it readable for an operator.
fn extract_name() -> String {
    format!(
        "{PREFIX}{}-{}-{}",
        std::process::id(),
        start_time(),
        EXTRACTS.fetch_add(1, Ordering::Relaxed)
    )
}

/// Is `name` the name of an extract (`index-<pid>-<start time>-<n>`, or the earlier build's `index-<pid>-<n>`), and
/// nothing else? Only these are ever removed.
fn is_extract_name(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    name.strip_prefix(PREFIX).is_some_and(|rest| {
        let parts: Vec<&str> = rest.split('-').collect();
        matches!(parts.len(), 2 | 3) && parts.iter().all(|p| digits(p))
    })
}

fn lock_of(scratch: &Path, name: &str) -> PathBuf {
    scratch.join(format!("{name}{LOCK_SUFFIX}"))
}

/// One extract in a wiki's scratch directory, held for as long as this value lives: [`Extract::begin`] takes its lock,
/// the caller has `materialize` fill [`Extract::root`], and dropping the value removes the directory, then the lock
/// file, then lets go of the lock.
#[derive(Debug)]
pub struct Extract {
    scratch: PathBuf,
    name: String,
    /// Let go when the value is dropped, which is after [`Drop::drop`] has removed the directory and the lock file.
    _lock: FileLock,
}

impl Extract {
    /// Sweep what no one holds in `scratch`, then claim a name there: its lock file made and locked exclusively. The
    /// directory is not made (`materialize` makes it).
    pub fn begin(scratch: &Path) -> std::io::Result<Self> {
        sweep_leftovers(scratch);
        for _ in 0..ATTEMPTS {
            let name = extract_name();
            let path = lock_of(scratch, &name);
            let Acquired::Held(lock) = try_exclusive(&path)? else {
                continue;
            };
            // A sweep may have taken the file (it was free for a moment) and removed it after our open: the lock we
            // won is then on a file nobody can find, and protects nothing.
            if !lock.is_at(&path)? {
                continue;
            }
            let extract = Extract {
                scratch: scratch.to_path_buf(),
                name,
                _lock: lock,
            };
            // Nothing lives under a name whose lock we hold: a directory here is the leftover of a name that was used
            // before (its owner is gone), and `materialize` makes the directory itself.
            extract.remove_directory();
            return Ok(extract);
        }
        Err(std::io::Error::other(
            "no free name for an extract in the scratch directory",
        ))
    }

    /// The extract's name in the scratch directory, for `materialize`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The extract's directory.
    pub fn root(&self) -> PathBuf {
        self.scratch.join(&self.name)
    }

    /// Remove the directory (whatever is under its name); `true` when nothing is there now.
    fn remove_directory(&self) -> bool {
        let root = self.root();
        match std::fs::symlink_metadata(&root) {
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(&root).is_ok(),
            Ok(_) => std::fs::remove_file(&root).is_ok(),
        }
    }
}

impl Drop for Extract {
    fn drop(&mut self) {
        // The directory goes whatever came of the build; then the lock file, and only then the lock. A directory that
        // will not go keeps its lock file, and the next sweep finishes the work.
        if self.remove_directory() {
            let _ = std::fs::remove_file(lock_of(&self.scratch, &self.name));
        }
    }
}

/// Remove the extracts in `scratch` that no process holds: those whose lock can be taken without waiting. Only names of
/// exactly the shape `index-<pid>-<start time>-<n>` (and of the earlier build's) are looked at, nothing is followed, and an
/// extract whose lock is held, by this process or another, is not touched.
pub fn sweep_leftovers(scratch: &Path) {
    let Ok(listing) = std::fs::read_dir(scratch) else {
        return;
    };
    // An extract is its directory, its lock file or both (a crash between the two leaves either).
    let names: BTreeSet<String> = listing
        .flatten()
        .filter_map(|item| item.file_name().into_string().ok())
        .map(|file| {
            file.strip_suffix(LOCK_SUFFIX)
                .map_or(file.clone(), str::to_owned)
        })
        .filter(|name| is_extract_name(name))
        .collect();
    for name in names {
        sweep_one(scratch, &name);
    }
}

fn sweep_one(scratch: &Path, name: &str) {
    let path = lock_of(scratch, name);
    // Without waiting: busy means someone is using the extract. A lock file that cannot be opened (a link, a directory)
    // is not ours.
    let Ok(Acquired::Held(lock)) = try_exclusive(&path) else {
        return;
    };
    // A file another sweep took and removed while this one opened it is no one's lock.
    if !lock.is_at(&path).is_ok_and(|same| same) {
        return;
    }
    let root = scratch.join(name);
    let gone = match std::fs::symlink_metadata(&root) {
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(&root).is_ok(),
        // A file or a link under an extract's name was not made by an extract: left alone, with its lock file.
        Ok(_) => false,
    };
    if gone {
        let _ = std::fs::remove_file(&path);
    }
}
