//! Advisory file locks (`flock(2)`), the one way this crate makes two *processes* take turns: the key store's
//! creation (a lock file in the key directory), the per-wiki write lock (a lock file in inklkunn's data
//! directory, never inside `.prikk/`, INT-10), and the per-call seed directories' liveness.
//!
//! A lock belongs to an open file description and the kernel drops it when the last descriptor closes, which
//! includes the death of the process that held it: a killed holder never leaves a stale lock, and nothing here
//! needs a timeout to be safe. The lock file also carries its holder's process id (written after the lock is
//! taken) so that a refused caller can say whom it waits for; the id is a courtesy for the operator, never the
//! lock: a lock file whose recorded holder is dead and whose lock is free is simply taken.
//!
//! The lock file is opened without following a link (`O_NOFOLLOW`) and created mode 0600.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

use rustix::fs::{FlockOperation, OFlags, flock};
use rustix::io::Errno;

/// A held exclusive lock. Dropping it (or the process dying) releases it.
#[derive(Debug)]
pub(crate) struct FileLock {
    _file: File,
}

/// The outcome of asking for a lock without waiting.
#[derive(Debug)]
pub(crate) enum Acquired {
    /// The lock is held.
    Held(FileLock),
    /// Another open file description holds it; `holder` is the process id it recorded, when it could be read.
    Busy {
        /// The recorded holder.
        #[cfg_attr(not(test), allow(dead_code))]
        // named in the per-wiki lock's refusal (next commit)
        holder: Option<u32>,
    },
}

fn open(path: &Path) -> std::io::Result<File> {
    let nofollow = i32::try_from(OFlags::NOFOLLOW.bits()).unwrap_or(0);
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nofollow)
        .open(path)
}

/// Take the exclusive lock on `path` (made if absent) **without waiting**.
pub(crate) fn try_exclusive(path: &Path) -> std::io::Result<Acquired> {
    let file = open(path)?;
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {
            // The holder's id, for whoever is refused next. A failure to record it does not undo the lock.
            let _ = file.set_len(0);
            let _ = file.write_all_at(std::process::id().to_string().as_bytes(), 0);
            Ok(Acquired::Held(FileLock { _file: file }))
        }
        Err(Errno::WOULDBLOCK) => {
            let mut buf = [0_u8; 24];
            let holder = file
                .read_at(&mut buf, 0)
                .ok()
                .and_then(|n| buf.get(..n))
                .and_then(|b| std::str::from_utf8(b).ok())
                .and_then(|s| s.trim().parse::<u32>().ok());
            Ok(Acquired::Busy { holder })
        }
        Err(e) => Err(e.into()),
    }
}

/// Take the exclusive lock on `path`, polling for at most `wait`. `None` if it stayed busy. For short critical
/// sections only (a store's creation); the per-wiki lock never waits.
pub(crate) fn exclusive_within(path: &Path, wait: Duration) -> std::io::Result<Option<FileLock>> {
    let started = Instant::now();
    loop {
        match try_exclusive(path)? {
            Acquired::Held(lock) => return Ok(Some(lock)),
            Acquired::Busy { .. } if started.elapsed() >= wait => return Ok(None),
            Acquired::Busy { .. } => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

#[cfg(test)]
mod tests;
