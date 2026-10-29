//! A seed directory is **alive** while a process holds an exclusive `flock` on it, and dead the moment that
//! process is gone, killed or not (corrections handoff C6). So a start can remove every directory it can lock
//! at once, without guessing from an age how long a call might still run, and a long acceptance is never taken
//! for a dead one.
//!
//! Three rules keep it honest:
//!
//! - **Born locked.** A directory is made under a name the sweep ignores (`.new-<hex>`), locked, and only then
//!   renamed to `inklkunn-seeds-<hex>`: from the moment it can be found by its final name it is already held.
//! - **Removed by handle.** Opening is `O_NOFOLLOW | O_DIRECTORY`, and every entry is overwritten and unlinked
//!   through that handle (`openat`/`unlinkat` relative to it, never by a path another user could have swapped for
//!   a link), then the directory is unlinked from its parent's handle.
//! - **Never a link, never a surprise.** An entry that is not a regular file, or a name that is not a plain
//!   component, makes the removal fail (and be reported), never be followed.

use std::ffi::{CStr, OsStr};
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use rustix::fs::{AtFlags, Dir, FileType, FlockOperation, Mode, OFlags, flock, fstat, openat};
use rustix::io::Errno;

/// What a not-yet-visible directory is called while it is being made and locked.
pub(super) const BIRTH_PREFIX: &str = ".new-";

fn dir_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

/// Open the directory `name` of `parent` without following a link.
fn open_child_dir(parent: &OwnedFd, name: &CStr) -> Result<OwnedFd, Errno> {
    openat(parent, name, dir_flags(), Mode::empty())
}

fn open_dir(path: &Path) -> Result<OwnedFd, Errno> {
    rustix::fs::open(path, dir_flags(), Mode::empty())
}

/// Make `<base>/<final_name>` (mode 0700) and return the handle that holds its lock. The directory is created
/// as `<base>/.new-…`, locked, then renamed, so it is never visible unlocked.
pub(super) fn make_locked(base: &Path, final_name: &str, birth_hex: &str) -> std::io::Result<File> {
    let birth = base.join(format!("{BIRTH_PREFIX}{birth_hex}"));
    std::fs::DirBuilder::new().mode(0o700).create(&birth)?;
    let handle: OwnedFd = open_dir(&birth).map_err(std::io::Error::from)?;
    let file = File::from(handle);
    if let Err(e) = flock(&file, FlockOperation::NonBlockingLockExclusive) {
        let _ = std::fs::remove_dir(&birth);
        return Err(e.into());
    }
    if let Err(e) = std::fs::rename(&birth, base.join(final_name)) {
        let _ = std::fs::remove_dir(&birth);
        return Err(e);
    }
    Ok(file)
}

/// Is somebody holding the lock on this directory? Locks it for the caller when nobody is (the returned file
/// keeps the lock; drop it when done).
pub(super) enum Liveness {
    /// A live holder has it.
    Alive,
    /// Nobody holds it; the caller now does.
    Dead(File),
    /// It could not be opened as a real directory (a link, or gone).
    NotADirectory,
}

pub(super) fn try_take(base: &Path, name: &OsStr) -> Liveness {
    let Ok(base_fd) = open_dir(base) else {
        return Liveness::NotADirectory;
    };
    let Ok(cname) = std::ffi::CString::new(name.as_bytes()) else {
        return Liveness::NotADirectory;
    };
    let Ok(dir_fd) = open_child_dir(&base_fd, &cname) else {
        return Liveness::NotADirectory;
    };
    let file = File::from(dir_fd);
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Liveness::Dead(file),
        Err(Errno::WOULDBLOCK) => Liveness::Alive,
        Err(_) => Liveness::NotADirectory,
    }
}

/// Overwrite with zeros, unlink, and remove the directory `<parent>/<name>`, all through handles. `true` if it
/// is gone (including: it was already gone). A name that is not a single plain component is refused.
pub(super) fn remove_dir_at(path: &Path) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    let Ok(cname) = std::ffi::CString::new(name.as_bytes()) else {
        return false;
    };
    let Ok(parent_fd) = open_dir(parent) else {
        // The parent is gone, so is everything in it.
        return !parent.exists();
    };
    let dir_fd = match open_child_dir(&parent_fd, &cname) {
        Ok(fd) => fd,
        Err(Errno::NOENT) => return true,
        Err(_) => return false,
    };
    let clean = empty_dir(&dir_fd);
    drop(dir_fd);
    if !clean {
        return false;
    }
    match rustix::fs::unlinkat(&parent_fd, &cname, AtFlags::REMOVEDIR) {
        Ok(()) | Err(Errno::NOENT) => true,
        Err(_) => false,
    }
}

/// Zero and unlink every regular file of the directory behind `dir_fd`. Anything else is left and fails it.
fn empty_dir(dir_fd: &OwnedFd) -> bool {
    let Ok(mut dir) = Dir::read_from(dir_fd.as_fd()) else {
        return false;
    };
    let mut names: Vec<std::ffi::CString> = Vec::new();
    while let Some(entry) = dir.read() {
        let Ok(entry) = entry else {
            return false;
        };
        let n = entry.file_name();
        if n.to_bytes() == b"." || n.to_bytes() == b".." {
            continue;
        }
        names.push(n.to_owned());
    }
    let mut clean = true;
    for name in names {
        let file_fd = openat(
            dir_fd,
            name.as_c_str(),
            OFlags::WRONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        );
        if let Ok(fd) = file_fd {
            // Only a regular file is written to (the open refused a link; this refuses a device or a pipe).
            if let Ok(st) = fstat(&fd)
                && FileType::from_raw_mode(st.st_mode) == FileType::RegularFile
            {
                let len = usize::try_from(st.st_size).unwrap_or(0);
                let mut f = File::from(fd);
                let _ = f.write_all(&vec![0_u8; len]);
                let _ = f.sync_all();
            }
        }
        if rustix::fs::unlinkat(dir_fd, name.as_c_str(), AtFlags::empty()).is_err() {
            clean = false;
        }
    }
    clean
}
