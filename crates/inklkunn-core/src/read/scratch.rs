//! The extracts of the read path in a wiki's scratch directory, and who may remove them (handoff 005 corrections, sixth
//! round, K55; the fifth round's K51 redone; the seventh round's K65 for the export).
//!
//! More than one process extracts into one wiki's scratch directory: the server, and the command line (`wiki
//! import-folder` and `wiki export` each extract the wiki's files once). The build lock of a [`ReadService`] orders the
//! extracts of one service only.
//!
//! **An extract is live while its lock is held.** Each extract has a lock file beside it (`<name>.lock`), created before
//! the extract is made. Its owner holds an exclusive `flock` on that file for the extract's whole life and lets go only
//! after the directory is removed. The sweep removes an extract only after taking its lock **without waiting**, and
//! removes the directory and then the lock file. An advisory lock on one filesystem holds between processes, containers
//! (PID namespaces) and users alike, and the kernel drops it when its holder dies, so a leftover is exactly an extract
//! whose lock is free. Nothing is decided from `/proc`, from a clock or from a guess, and a live extract is never half
//! removed: a read never meets a partial tree.
//!
//! The export builds its bundle in a directory beside the destination, `.<name>.partial-<pid>-<start time>-<n>`, and holds
//! it the same way ([`Partial`]): a lock file beside it, taken before the directory is made, swept only when its lock can
//! be taken. The export's extraction is an [`Extract`] like the others.
//!
//! **What a sweep may touch depends on where it looks (K74).** The scratch directory is the service's: everything of the
//! shape `index-<pid>-<start time>-<n>` (and the earlier build's `index-<pid>-<n>`) in it was made by an extract, so a
//! directory with no lock file is a leftover and goes. A destination's directory is a person's: beside it a sweep removes a
//! partial bundle only when its **lock file already exists** and its tail is the three-part `<pid>-<start time>-<n>`. A
//! directory with no lock file, or with the earlier build's two-part tail, is never touched there, whatever its name
//! (`~/exports/.notes.partial-2026-11/` is a person's); a 0.2.x partial is removed by hand (the CHANGELOG says so).
//!
//! **Supported filesystems.** The data directory (and the directory an export is written to) must be on a local
//! filesystem, or a bind mount of one, where `flock` is one lock for every process that has the file open. A network
//! filesystem (NFS, SMB, anything mounted with `local_lock`) is **not supported**: there `flock` may be per process, or
//! local to one host, and two hosts then each "hold" the same lock. Nothing here detects that, and none of this is promised
//! to hold on such a filesystem.
//!
//! [`ReadService`]: super::ReadService

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use inklkunn_signer::flock::{Acquired, FileLock, try_exclusive};

use crate::lease::start_time;

/// The prefix of the read path's extracts.
const PREFIX: &str = "index-";

/// The suffix of a lock file: `<name>.lock`, beside the directory `<name>`.
const LOCK_SUFFIX: &str = ".lock";

/// How many names are tried for one claim before it is given up (a name is taken only if another process chose the same
/// one at the same moment, or a sweep took the lock file between its creation and its lock).
const ATTEMPTS: usize = 16;

/// One count for the whole process, so that two claims in it (a test's, a command line's) never choose the same name.
static CLAIMS: AtomicU64 = AtomicU64::new(0);

/// `<pid>-<start time>-<n>`: the tail of every name. It only needs to be unlikely to be chosen twice at once (the lock
/// decides who owns it); the pid and start time keep it readable for an operator.
fn unique_tail() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        start_time(),
        CLAIMS.fetch_add(1, Ordering::Relaxed)
    )
}

/// Is `tail` made of `parts` numbers (digits only) joined by `-`?
fn is_tail_of(tail: &str, parts: &[usize]) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let split: Vec<&str> = tail.split('-').collect();
    parts.contains(&split.len()) && split.iter().all(|p| digits(p))
}

/// Is `tail` the tail of an extract's name: `<pid>-<start time>-<n>`, or the earlier build's `<pid>-<n>`?
fn is_tail(tail: &str) -> bool {
    is_tail_of(tail, &[2, 3])
}

/// Is `tail` the tail of a partial bundle's name: `<pid>-<start time>-<n>`? The earlier build's two-part tail is not: a
/// name of that shape beside a destination may be a person's (K74).
fn is_partial_tail(tail: &str) -> bool {
    is_tail_of(tail, &[3])
}

/// Is `name` the name of an extract (`index-<tail>`), and nothing else? Only these are ever removed from a scratch
/// directory.
fn is_extract_name(name: &str) -> bool {
    name.strip_prefix(PREFIX).is_some_and(is_tail)
}

/// The prefix of the partial bundles of the export whose destination is called `dest`.
fn partial_prefix(dest: &str) -> String {
    format!(".{dest}.partial-")
}

/// The longest name a directory entry may have (`NAME_MAX` on Linux).
const NAME_MAX: usize = 255;

/// The digits of the claim count that [`partial_name_too_long_by`] reserves: a process makes far fewer than a million
/// claims, and a count that varied the answer from one call to the next would make a limit that cannot be tested.
const RESERVED_COUNT_DIGITS: usize = 6;

/// By how many bytes the partial bundle of an export to a destination called `dest` would have a name (or a lock file's name)
/// too long for the file system, or `None` when it fits: what the person must cut from `dest` (K74). The tail is the one
/// this process would make, with room for a count of six digits, so a name that is not refused can
/// always be claimed, and the number can be up to five bytes more than the claim needs.
pub fn partial_name_too_long_by(dest: &str) -> Option<usize> {
    let digits = |n: u64| n.to_string().len();
    let tail = digits(u64::from(std::process::id()))
        + 1
        + digits(start_time())
        + 1
        + digits(CLAIMS.load(Ordering::Relaxed)).max(RESERVED_COUNT_DIGITS);
    let longest = partial_prefix(dest).len() + tail + LOCK_SUFFIX.len();
    longest.checked_sub(NAME_MAX).filter(|over| *over > 0)
}

fn lock_of(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{LOCK_SUFFIX}"))
}

/// A directory name in `dir` claimed by its lock file: the lock is held from before the directory exists until after it
/// is gone. The kinds of claim ([`Extract`], [`Partial`]) differ in their names only.
#[derive(Debug)]
struct Held {
    dir: PathBuf,
    name: String,
    /// Let go when the value is dropped, which is after [`Drop::drop`] has removed the directory and the lock file.
    _lock: FileLock,
}

impl Held {
    /// Claim a name `next_name()` gives in `dir`: its lock file made and locked exclusively. The directory is not made.
    fn begin(dir: &Path, mut next_name: impl FnMut() -> String) -> std::io::Result<Self> {
        for _ in 0..ATTEMPTS {
            let name = next_name();
            let path = lock_of(dir, &name);
            let Acquired::Held(lock) = try_exclusive(&path)? else {
                continue;
            };
            // A sweep may have taken the file (it was free for a moment) and removed it after our open: the lock we
            // won is then on a file nobody can find, and protects nothing.
            if !lock.is_at(&path)? {
                continue;
            }
            let held = Held {
                dir: dir.to_path_buf(),
                name,
                _lock: lock,
            };
            // Nothing lives under a name whose lock we hold: a directory here is the leftover of a name that was used
            // before (its owner is gone), and whoever fills the directory makes it itself.
            held.remove_directory();
            return Ok(held);
        }
        Err(std::io::Error::other(
            "no free name for a directory to be claimed",
        ))
    }

    fn path(&self) -> PathBuf {
        self.dir.join(&self.name)
    }

    /// Remove the directory (whatever is under its name); `true` when nothing is there now.
    fn remove_directory(&self) -> bool {
        let root = self.path();
        match std::fs::symlink_metadata(&root) {
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(&root).is_ok(),
            Ok(_) => std::fs::remove_file(&root).is_ok(),
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // The directory goes whatever came of the build; then the lock file, and only then the lock. A directory that
        // will not go keeps its lock file, and the next sweep finishes the work.
        if self.remove_directory() {
            let _ = std::fs::remove_file(lock_of(&self.dir, &self.name));
        }
    }
}

/// One extract in a wiki's scratch directory, held for as long as this value lives: [`Extract::begin`] takes its lock,
/// the caller has `materialize` fill [`Extract::root`], and dropping the value removes the directory, then the lock
/// file, then lets go of the lock.
#[derive(Debug)]
pub struct Extract(Held);

impl Extract {
    /// Sweep what no one holds in `scratch`, then claim a name there: its lock file made and locked exclusively. The
    /// directory is not made (`materialize` makes it).
    pub fn begin(scratch: &Path) -> std::io::Result<Self> {
        sweep_leftovers(scratch);
        Held::begin(scratch, || format!("{PREFIX}{}", unique_tail())).map(Extract)
    }

    /// The extract's name in the scratch directory, for `materialize`.
    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// The extract's directory.
    pub fn root(&self) -> PathBuf {
        self.0.path()
    }
}

/// The directory in which an export builds its bundle before it is moved to the destination, held for as long as this
/// value lives: [`Partial::begin`] takes the lock of a name beside the destination, the export makes the directory, and
/// dropping the value removes whatever is left of it (nothing, once the bundle has been moved), then the lock file, then
/// lets go of the lock.
#[derive(Debug)]
pub struct Partial(Held);

impl Partial {
    /// Sweep the partial bundles of destinations called `dest` that no one holds in `parent`, then claim a name there
    /// (`.<dest>.partial-<pid>-<start time>-<n>`): its lock file made and locked exclusively. The directory is not made.
    pub fn begin(parent: &Path, dest: &str) -> std::io::Result<Self> {
        sweep_partials(parent, dest);
        let prefix = partial_prefix(dest);
        Held::begin(parent, || format!("{prefix}{}", unique_tail())).map(Partial)
    }

    /// The directory to build the bundle in.
    pub fn path(&self) -> PathBuf {
        self.0.path()
    }
}

/// Remove the extracts in `scratch` that no process holds: those whose lock can be taken without waiting. Only names of
/// exactly the shape `index-<pid>-<start time>-<n>` (and of the earlier build's) are looked at, nothing is followed, and an
/// extract whose lock is held, by this process or another, is not touched. The scratch directory is the service's own, so
/// an extract with no lock file (the earlier build's) is a leftover and goes.
pub fn sweep_leftovers(scratch: &Path) {
    sweep(scratch, Where::Scratch, is_extract_name);
}

/// Remove the partial bundles in `parent` of destinations called `dest` that no process holds, as [`sweep_leftovers`]
/// does for extracts, but more narrowly, because `parent` is a person's directory and not the service's (K74): only a
/// partial whose lock file **already exists** and whose name is `.<dest>.partial-<pid>-<start time>-<n>` is looked at. A
/// directory with no lock file, or with the earlier build's `<pid>-<n>` tail, is never touched.
pub fn sweep_partials(parent: &Path, dest: &str) {
    let prefix = partial_prefix(dest);
    sweep(parent, Where::BesideDestination, |name| {
        name.strip_prefix(prefix.as_str())
            .is_some_and(is_partial_tail)
    });
}

/// Whose directory a sweep looks in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Where {
    /// The service's scratch directory: whatever has the shape of an extract's name was made by one.
    Scratch,
    /// A directory a person chose, beside an export's destination: only a claim whose lock file exists is ours.
    BesideDestination,
}

fn sweep(dir: &Path, place: Where, is_ours: impl Fn(&str) -> bool) {
    let Ok(listing) = std::fs::read_dir(dir) else {
        return;
    };
    // In the scratch directory a claim is its directory, its lock file or both (a crash between the two leaves either).
    // Beside a destination it is a lock file: the directory alone may be anyone's.
    let names: BTreeSet<String> = listing
        .flatten()
        .filter_map(|item| item.file_name().into_string().ok())
        .filter_map(|file| match file.strip_suffix(LOCK_SUFFIX) {
            Some(name) => Some(name.to_owned()),
            None => (place == Where::Scratch).then_some(file),
        })
        .filter(|name| is_ours(name))
        .collect();
    for name in names {
        sweep_one(dir, place, &name);
    }
}

fn sweep_one(dir: &Path, place: Where, name: &str) {
    let path = lock_of(dir, name);
    // Beside a destination the lock file must be there already: opening it would otherwise make one, take it, and call the
    // directory a leftover. (A file that goes between this look and the open is made by the open and held; it was a
    // finished claim's, whose directory is gone, so nothing is removed on its account but the empty lock file.)
    if place == Where::BesideDestination
        && !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
    {
        return;
    }
    // Without waiting: busy means someone is using the directory. A lock file that cannot be opened (a link, a
    // directory) is not ours.
    let Ok(Acquired::Held(lock)) = try_exclusive(&path) else {
        return;
    };
    // A file another sweep took and removed while this one opened it is no one's lock.
    if !lock.is_at(&path).is_ok_and(|same| same) {
        return;
    }
    let root = dir.join(name);
    let gone = match std::fs::symlink_metadata(&root) {
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(&root).is_ok(),
        // A file or a link under a claimed name was not made by a claimer: left alone, with its lock file.
        Ok(_) => false,
    };
    if gone {
        let _ = std::fs::remove_file(&path);
    }
}
