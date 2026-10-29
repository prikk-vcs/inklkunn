//! The creation mark (handoff 003 T7; hold note IS-13; planeter RFC 004 part 7 is the precedent): the state a
//! wiki is in while it is being created, so that **a `SIGKILL` at any step leaves either a complete wiki or
//! nothing that survives the next start**.
//!
//! *Create wiki* stores the record last, so a kill between the first directory and the record would leave a
//! repository directory and the wiki's sealed key record that nothing references and nothing removes. The mark
//! is the reference: a file beside the repository ([`RootLayout::creation_mark`]) that is made and made durable
//! **before the first directory**, is `flock`ed by the creating process for as long as the creation runs, and
//! is removed only **after the record is stored**. The start-up sweep ([`find_unfinished`]) takes the lock of
//! every mark it finds without waiting: a mark whose lock it can take belongs to a creation whose process is
//! dead (the kernel drops a lock with its holder), and the sweep removes that creation whole; a mark whose lock
//! is held belongs to a creation that is running right now (a command-line `wiki create` beside a starting
//! server) and is left alone. What "whole" means (the record, the repository directory, the scratch directory,
//! the key records) is `inklkunn-core`'s sweep, which knows the record store; this module is the mark.

use std::fs::File;
use std::path::{Path, PathBuf};

use inklkunn_store::{RootLayout, WikiId};

use crate::flock::{self, Acquired, FileLock};
use crate::locks::LOCK_GRACE;
use crate::writes::SignerError;

/// A wiki's creation mark, held. Dropping it **does not remove the mark** (that is the point: a creation that
/// ends without [`CreationMark::finish`] leaves it for the sweep); it only gives the lock up.
#[derive(Debug)]
pub struct CreationMark {
    id: WikiId,
    path: PathBuf,
    lock: FileLock,
}

/// What a start-up sweep found.
#[derive(Debug, Default)]
pub struct Unfinished {
    /// Marks of creations whose process is gone, each held by the sweep so that nothing else can claim it. The
    /// sweep removes each creation and then calls [`CreationMark::finish`], or drops the mark to leave it for
    /// the next start when it could not remove everything.
    pub dead: Vec<CreationMark>,
    /// Wikis whose creation is running right now, in another process or thread. Left alone.
    pub live: Vec<WikiId>,
}

fn sync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()
}

fn io_error(what: &str, e: &std::io::Error) -> SignerError {
    SignerError::Unavailable(format!("{what} ({})", e.kind()))
}

impl CreationMark {
    /// Mark the creation of wiki `id`: the mark file is made, locked and made durable. **Nothing else of the
    /// wiki may exist yet**; the caller makes its first directory only after this returns. Refused (and
    /// nothing is made) when another creation of the same id holds the mark.
    pub fn begin(layout: &RootLayout, id: &WikiId) -> Result<CreationMark, SignerError> {
        let path = layout.creation_mark(id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| io_error("the wiki's shard directory could not be made", &e))?;
        }
        let lock = match flock::try_exclusive(&path) {
            Ok(Acquired::Held(lock)) => lock,
            Ok(Acquired::Busy { .. }) => {
                return Err(SignerError::Unavailable(
                    "another act holds this wiki's creation mark; nothing was made".to_owned(),
                ));
            }
            Err(e) => return Err(io_error("the creation mark could not be made", &e)),
        };
        lock.sync()
            .map_err(|e| io_error("the creation mark could not be made durable", &e))?;
        if let Some(parent) = path.parent() {
            sync_dir(parent)
                .map_err(|e| io_error("the creation mark's directory could not be synced", &e))?;
        }
        Ok(CreationMark {
            id: id.clone(),
            path,
            lock,
        })
    }

    /// The wiki.
    pub fn id(&self) -> &WikiId {
        &self.id
    }

    /// The creation is over (the record is stored, or everything it made is removed): remove the mark. The
    /// lock is released after the file is gone.
    pub fn finish(self) -> Result<(), SignerError> {
        let CreationMark { path, lock, .. } = self;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error("the creation mark could not be removed", &e)),
        }
        if let Some(parent) = path.parent() {
            // Best effort: a mark that reappears after a crash is swept again, which is harmless.
            let _ = sync_dir(parent);
        }
        drop(lock);
        Ok(())
    }
}

/// Find the creation marks under `layout`: the dead ones, held, and the live ones, named. Nothing is removed.
pub fn find_unfinished(layout: &RootLayout) -> Result<Unfinished, SignerError> {
    let ids = layout
        .creation_marks()
        .map_err(|e| io_error("the wikis' root could not be listed", &e))?;
    let mut found = Unfinished::default();
    for id in ids {
        let path = layout.creation_mark(&id);
        // A short grace, not a wait: it rides out the moment a sibling thread's `fork` holds a copy of a
        // descriptor just released (see `LOCK_GRACE`). A live creation costs the sweep that much, once.
        match flock::exclusive_within(&path, LOCK_GRACE) {
            Ok(Acquired::Held(lock)) => found.dead.push(CreationMark { id, path, lock }),
            Ok(Acquired::Busy { .. }) => found.live.push(id),
            Err(e) => return Err(io_error("a creation mark could not be examined", &e)),
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
