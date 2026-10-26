//! The on-disk root (RFC 001 D-8): `<root>/<shard>/<wiki id>/` holds the wiki's repository; the shard is
//! the id's first two hex characters so no directory grows past 256 children of wikis. The wiki's name
//! appears nowhere on disk, so a rename moves no bytes.

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

    /// Create the directories for a wiki (idempotent). The repository itself is created by the signer
    /// (`prikk init` is a write).
    pub fn prepare(&self, id: &WikiId) -> std::io::Result<PathBuf> {
        let repo = self.repo_dir(id);
        std::fs::create_dir_all(&repo)?;
        std::fs::create_dir_all(self.scratch_dir(id))?;
        Ok(repo)
    }
}

#[cfg(test)]
mod tests;
