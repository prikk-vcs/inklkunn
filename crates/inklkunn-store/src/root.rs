//! The on-disk root (RFC 001 D-8): `<root>/<shard>/<wiki id>/` holds the wiki's repository; the shard is
//! the id's first two hex characters so no directory grows past 256 children of wikis. The wiki's name
//! appears nowhere on disk, so a rename moves no bytes. Only a validated [`WikiId`] ever joins a path.

use std::path::{Path, PathBuf};

use crate::record::WikiId;

/// The root layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootLayout {
    root: PathBuf,
}

impl RootLayout {
    /// A layout under `root` (absolute).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        RootLayout { root: root.into() }
    }

    /// The root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The wiki's directory: its repository (worktree and `.prikk`).
    pub fn repo_dir(&self, id: &WikiId) -> PathBuf {
        self.root.join(id.shard()).join(id.as_str())
    }

    /// The wiki's scratch directory, a sibling of the repository so that the two never overlap as mounts.
    pub fn scratch_dir(&self, id: &WikiId) -> PathBuf {
        self.root
            .join(id.shard())
            .join(format!("{}.scratch", id.as_str()))
    }

    /// The wiki's write-lock file, a sibling of the repository (inklkunn's data directory, never inside
    /// `.prikk/`): the signer holds an exclusive `flock` on it for every act that writes the repository, so the
    /// command line and a running server take turns (corrections handoff C3).
    pub fn lock_file(&self, id: &WikiId) -> PathBuf {
        self.root
            .join(id.shard())
            .join(format!("{}.lock", id.as_str()))
    }

    /// The wiki's **creation mark**, a sibling of the repository: it exists from before the first directory of a
    /// wiki's creation is made until after its record is stored, and a start-up sweep removes whatever a
    /// creation that was killed in between left (handoff 003 T7, IS-13). The signer holds a lock on it while the
    /// creation runs, which is how a sweep tells a dead creation from a live one.
    pub fn creation_mark(&self, id: &WikiId) -> PathBuf {
        self.root
            .join(id.shard())
            .join(format!("{}.creating", id.as_str()))
    }

    /// The ids of the wikis whose creation mark exists, live or dead (the caller decides which: the mark is
    /// locked while its creation runs). A root that does not exist yet has none.
    pub fn creation_marks(&self) -> std::io::Result<Vec<WikiId>> {
        let mut ids = Vec::new();
        let shards = match std::fs::read_dir(&self.root) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
            Err(e) => return Err(e),
        };
        for shard in shards.filter_map(Result::ok) {
            if !shard.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(shard.path()) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name();
                let id = name
                    .to_str()
                    .and_then(|n| n.strip_suffix(".creating"))
                    .and_then(WikiId::parse);
                // Only a plain file in the id's own shard is a mark.
                if let Some(id) = id
                    && id.shard() == shard.file_name().to_string_lossy()
                    && entry.file_type().is_ok_and(|t| t.is_file())
                {
                    ids.push(id);
                }
            }
        }
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(ids)
    }

    /// Create the directories for a wiki (idempotent). The repository itself is created by the signer
    /// (`prikk init` is a write).
    pub fn prepare(&self, id: &WikiId) -> std::io::Result<PathBuf> {
        let repo = self.repo_dir(id);
        std::fs::create_dir_all(&repo)?;
        std::fs::create_dir_all(self.scratch_dir(id))?;
        Ok(repo)
    }

    /// Remove the directories of a creation that did not complete. Only for an id that was drawn by the
    /// failed creation itself and never stored: the id is fresh and nothing references the directories.
    pub fn discard_unfinished(&self, id: &WikiId) -> std::io::Result<()> {
        for dir in [self.repo_dir(id), self.scratch_dir(id)] {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        match std::fs::remove_file(self.lock_file(id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests;
