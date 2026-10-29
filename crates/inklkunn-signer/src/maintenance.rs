//! The signer's **keyless writable mode** (handoff 003 T7; RFC 003 D-11; hold note IS-8): the repository
//! writable, **no key directory mounted and no seed variable set**, for exactly two operations that write
//! inside `.prikk/` but sign nothing:
//!
//! - [`Maintenance::archive_export`]: `prikk archive export`, which writes prikk's own lock files and caches
//!   and refuses a read-only repository (ledger PK-23), so it is not a read verb and the read driver may not
//!   name it. It has its own cost rule and its own pinned report schema in the signer's tables.
//! - [`Maintenance::refresh_checkpoint`]: a `verify` with the repository writable, which re-records prikk's
//!   checkpoint record that `extract` anchors on after a prikk upgrade or a restore (ledger PK-24; IS-9).
//!
//! **No key is reachable from here.** This type is opened with the prikk binary, a sandbox and the root layout,
//! and nothing else: it holds no key store, no secret and no seed directory, so it cannot be asked to sign, and
//! the mounts it builds ([`Mounts::write_keyless_with_output`](inklkunn_prikk::sandbox::Mounts)) have no key
//! directory and no variable. Every operation takes the **wiki's write lock** (the same lock the [`Signer`]
//! takes, a file in inklkunn's data directory), so an export never runs beside an acceptance, a key act, or
//! another export, in this process or another: a second one is told the wiki is busy and nothing is done.
//! Whether the acts are audited is the caller's (the binary writes the trail); this module has no store.
//!
//! [`Signer`]: crate::Signer

use std::ffi::OsString;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::storage::Verification;
use inklkunn_store::{RootLayout, WikiId};

use crate::locks::WikiLocks;
use crate::writes::{SignerError, Writes};

/// The file name prefix of an archive being written in the wiki's scratch directory.
const TEMP_PREFIX: &str = "archive-export-";
/// …and its suffix.
const TEMP_SUFFIX: &str = ".tmp";

/// What a finished archive export says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveExported {
    /// The archive's size in bytes.
    pub bytes: u64,
    /// The repository format the archive carries.
    pub repository_format: u64,
    /// The prikk that wrote it.
    pub prikk_version: String,
}

/// The keyless writable mode. See the module documentation.
pub struct Maintenance {
    writes: Writes,
    root: RootLayout,
    locks: WikiLocks,
}

impl std::fmt::Debug for Maintenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Maintenance(..)")
    }
}

impl Maintenance {
    /// Open the keyless mode over `root`'s wikis with `prikk` (a bare name is resolved through `PATH` once, here)
    /// under `sandbox`. It takes no secret and reads no key.
    pub fn new(prikk: impl Into<OsString>, sandbox: Sandbox, root: RootLayout) -> Self {
        Maintenance {
            writes: Writes::new(prikk, sandbox),
            root,
            locks: WikiLocks::default(),
        }
    }

    fn repo_of(&self, wiki: &WikiId) -> Result<PathBuf, SignerError> {
        let repo = self.root.repo_dir(wiki);
        if repo.is_dir() {
            Ok(repo)
        } else {
            Err(SignerError::Refused(
                "the wiki has no repository on this host".to_owned(),
            ))
        }
    }

    /// Write a whole-repository archive of `wiki` to `destination` (an absolute path in an existing directory;
    /// a file that exists is never overwritten), **serialized with the wiki's writes**. The archive is written
    /// by prikk into the wiki's scratch directory, checked against its report, and then copied to
    /// `destination` with mode 0600 (it holds every page, private ones included) and the scratch copy removed.
    /// A failure leaves no destination file.
    pub fn archive_export(
        &self,
        wiki: &WikiId,
        destination: &Path,
    ) -> Result<ArchiveExported, SignerError> {
        if !destination.is_absolute() {
            return Err(SignerError::Refused(
                "the archive's destination must be an absolute path".to_owned(),
            ));
        }
        if !destination.parent().is_some_and(Path::is_dir) {
            return Err(SignerError::Refused(
                "the archive's directory does not exist".to_owned(),
            ));
        }
        if std::fs::symlink_metadata(destination).is_ok() {
            return Err(SignerError::Refused(
                "the archive's destination already exists; nothing was written".to_owned(),
            ));
        }
        let repo = self.repo_of(wiki)?;
        let _held = self.locks.take(&self.root, wiki)?;
        let scratch = self.root.scratch_dir(wiki);
        std::fs::create_dir_all(&scratch).map_err(|e| {
            SignerError::Unavailable(format!(
                "the wiki's scratch directory could not be made ({})",
                e.kind()
            ))
        })?;
        remove_temporaries(&scratch);
        let temp = scratch.join(format!(
            "{TEMP_PREFIX}{}-{}{TEMP_SUFFIX}",
            std::process::id(),
            nanos()
        ));
        let result = self.export_and_copy(&repo, &scratch, &temp, destination);
        // Whatever happened, the scratch copy goes.
        let _ = std::fs::remove_file(&temp);
        result
    }

    fn export_and_copy(
        &self,
        repo: &Path,
        scratch: &Path,
        temp: &Path,
        destination: &Path,
    ) -> Result<ArchiveExported, SignerError> {
        let report = self.writes.archive_export(repo, scratch, temp)?;
        let on_disk = std::fs::metadata(temp)
            .map_err(|e| {
                SignerError::Damaged(format!(
                    "the archive prikk wrote is not there ({})",
                    e.kind()
                ))
            })?
            .len();
        if on_disk != report.total_bytes {
            return Err(SignerError::Damaged(format!(
                "the archive is {on_disk} bytes and prikk's report says {}",
                report.total_bytes
            )));
        }
        let copied = copy_private(temp, destination);
        if let Err(e) = copied {
            let _ = std::fs::remove_file(destination);
            return Err(e);
        }
        Ok(ArchiveExported {
            bytes: report.total_bytes,
            repository_format: report.repository_format,
            prikk_version: report.tool_version,
        })
    }

    /// Re-record prikk's checkpoint record for `wiki`: a `verify` with the repository writable and no key,
    /// serialized with the wiki's writes. Returns the verification; **the checkpoint is refreshed only when it
    /// is complete and passes**, so the caller records the refresh only then. A memory ceiling is
    /// [`SignerError::CeilingExceeded`], never a failed verification.
    pub fn refresh_checkpoint(&self, wiki: &WikiId) -> Result<Verification, SignerError> {
        let repo = self.repo_of(wiki)?;
        let _held = self.locks.take(&self.root, wiki)?;
        let verification = self.writes.verify_writable(&repo)?;
        if verification.is_partial() {
            // A report that did not run to an outcome about the repository is no verdict (ledger PK-22,
            // PL-7), and the checkpoint was not necessarily re-recorded.
            return Err(SignerError::Unavailable(format!(
                "`verify` did not run to a complete verdict ({}); the checkpoint was not refreshed",
                verification.stage_summary()
            )));
        }
        Ok(verification)
    }
}

/// A temporary name's nanoseconds: unique enough beside the process id, under the wiki's write lock.
fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// Remove the archives a killed export left in the scratch directory (the caller holds the wiki's write lock,
/// so no live export has one).
fn remove_temporaries(scratch: &Path) {
    let Ok(entries) = std::fs::read_dir(scratch) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let is_temp = name
            .to_str()
            .is_some_and(|n| n.starts_with(TEMP_PREFIX) && n.ends_with(TEMP_SUFFIX));
        if is_temp && entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Copy `from` to the new file `to` (created exclusively, mode 0600, never following a link), and make it
/// durable.
fn copy_private(from: &Path, to: &Path) -> Result<(), SignerError> {
    let fail =
        |what: &str, e: &std::io::Error| SignerError::Unavailable(format!("{what} ({})", e.kind()));
    let mut source =
        std::fs::File::open(from).map_err(|e| fail("the archive could not be read", &e))?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(to)
        .map_err(|e| fail("the archive's destination could not be created", &e))?;
    std::io::copy(&mut source, &mut out)
        .map_err(|e| fail("the archive could not be copied", &e))?;
    out.flush()
        .map_err(|e| fail("the archive could not be written", &e))?;
    out.sync_all()
        .map_err(|e| fail("the archive could not be made durable", &e))
}

#[cfg(test)]
mod tests;
