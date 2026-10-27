//! *Create wiki* (RFC 001 D-5, D-8, D-9): the record, the root layout, the repository, the bundle files,
//! the first page sealed as a system change, and the first verification. **All or nothing**: the record is
//! built first but stored last, after the repository exists, is sealed and verifies; a failure anywhere
//! before that removes the directories the attempt made (the id was fresh and nothing referenced them) and
//! says so. The write steps go through [`RepositoryBootstrap`], the signer's seam.

use std::path::{Path, PathBuf};

use inklkunn_prikk::{Storage, StorageError};
pub use inklkunn_signer::RepositoryBootstrap;
use inklkunn_store::{
    NewWiki, RootLayout, StoreError, Verification, VerificationState, WikiId, WikiRecord, WikiStore,
};

use crate::bundle;

/// What happened to the directories of a failed creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cleanup {
    /// The repository and scratch directories were removed; nothing is left of the attempt.
    Removed,
    /// Removal failed; the directory (the id's, never the name's) and the error, for the operator.
    LeftBehind {
        /// The repository directory.
        repo: PathBuf,
        /// Why it could not be removed.
        error: String,
    },
}

impl std::fmt::Display for Cleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cleanup::Removed => write!(f, "nothing was left on disk"),
            Cleanup::LeftBehind { repo, error } => write!(
                f,
                "the unfinished repository at {} could not be removed ({error}); remove it by hand",
                repo.display()
            ),
        }
    }
}

/// Why a wiki was not created. No variant leaves a record behind; the ones that touched the disk say what
/// became of it.
#[derive(Debug)]
pub enum CreateWikiError {
    /// The request was refused before anything was made: the slug is taken or invalid, a field is
    /// invalid, or the store failed.
    Store(StoreError),
    /// The root directories could not be made.
    Layout(std::io::Error),
    /// The signer could not create the repository or seal the first change.
    Signer {
        /// `create` (init and trust) or `seal` (commit and seal).
        step: &'static str,
        /// What the signer said.
        error: inklkunn_signer::SignerError,
        /// What became of the directories.
        cleanup: Cleanup,
    },
    /// The bundle files could not be written.
    Files {
        /// The I/O error.
        error: std::io::Error,
        /// What became of the directories.
        cleanup: Cleanup,
    },
    /// The new repository did not verify (`None`), or verification could not run (`Some`).
    Verification {
        /// The storage error, when the call did not complete.
        error: Option<StorageError>,
        /// What became of the directories.
        cleanup: Cleanup,
    },
}

impl std::fmt::Display for CreateWikiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateWikiError::Store(e) => write!(f, "the wiki was not created: {e}"),
            CreateWikiError::Layout(e) => {
                write!(f, "the wiki's directories could not be created: {e}")
            }
            CreateWikiError::Signer {
                step,
                error,
                cleanup,
            } => write!(
                f,
                "the repository's `{step}` step failed: {error}; {cleanup}"
            ),
            CreateWikiError::Files { error, cleanup } => {
                write!(
                    f,
                    "the bundle files could not be written: {error}; {cleanup}"
                )
            }
            CreateWikiError::Verification {
                error: Some(e),
                cleanup,
            } => write!(
                f,
                "the new repository could not be verified: {e}; {cleanup}"
            ),
            CreateWikiError::Verification {
                error: None,
                cleanup,
            } => write!(f, "the new repository failed verification; {cleanup}"),
        }
    }
}

impl std::error::Error for CreateWikiError {}

/// A created wiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWiki {
    /// The record, with key ids and the first verification recorded.
    pub record: WikiRecord,
    /// The repository directory.
    pub repo: PathBuf,
    /// The first sealed block.
    pub first_block_id: String,
}

/// The longest summary line CT-03 allows (RFC 002 D-5).
pub const SUMMARY_MAX_CHARS: usize = 120;

/// A CT-03 message for a change the service itself makes (`Kind: system`): the summary line (cut to
/// [`SUMMARY_MAX_CHARS`] characters, never containing a line break), an empty line, then the trailers in
/// RFC 002 D-5's order. A system change has no proposal and no base, so those trailers are absent; the
/// composer for proposals is RFC 002's handoff (T5), which this function will then call.
pub fn system_message(summary: &str) -> String {
    let one_line: String = summary
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut cut: String = one_line.chars().take(SUMMARY_MAX_CHARS).collect();
    if cut.chars().count() < one_line.chars().count() {
        cut.pop();
        cut.push('…');
    }
    format!(
        "{cut}\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\nKind: system\n"
    )
}

/// The message of the change that creates a wiki.
pub fn creation_message(record: &WikiRecord) -> String {
    system_message(&format!("Create the wiki {}", record.display_name))
}

fn discard(layout: &RootLayout, id: &WikiId) -> Cleanup {
    match layout.discard_unfinished(id) {
        Ok(()) => Cleanup::Removed,
        Err(e) => Cleanup::LeftBehind {
            repo: layout.repo_dir(id),
            error: e.to_string(),
        },
    }
}

/// Create a wiki: check the slug, build the record, make the directories, create the repository, write the
/// bundle files, seal the first change, verify, and only then store the record.
pub fn create_wiki(
    store: &dyn WikiStore,
    layout: &RootLayout,
    signer: &dyn RepositoryBootstrap,
    storage: &dyn Storage,
    new: NewWiki,
    inklkunn_version: &str,
) -> Result<CreatedWiki, CreateWikiError> {
    // 1. A taken slug is refused before anything touches the disk (the insert at the end is the guarantee
    //    against a race; this is the early, cheap answer).
    match store.by_slug(&new.slug) {
        Ok(_) => {
            return Err(CreateWikiError::Store(StoreError::SlugTaken(
                new.slug.to_string(),
            )));
        }
        Err(StoreError::NotFound) => {}
        Err(e) => return Err(CreateWikiError::Store(e)),
    }
    // 2. The record, validated, with a fresh id; not stored yet.
    let mut record = WikiRecord::from_new(WikiId::new(), new, inklkunn_store::record::now_secs())
        .map_err(|e| CreateWikiError::Store(StoreError::InvalidRequest(e)))?;
    let id = record.id.clone();
    // 3. The directories.
    let repo = layout.prepare(&id).map_err(CreateWikiError::Layout)?;
    // 4. The repository and the keys.
    let keys = match signer.create_repository(&repo) {
        Ok(k) => k,
        Err(error) => {
            return Err(CreateWikiError::Signer {
                step: "create",
                error,
                cleanup: discard(layout, &id),
            });
        }
    };
    record.author_key_id = Some(keys.author);
    record.maintainer_key_id = Some(keys.maintainer);
    // 5. The bundle files.
    if let Err(error) = bundle::write_layout(&repo, &record, inklkunn_version) {
        return Err(CreateWikiError::Files {
            error,
            cleanup: discard(layout, &id),
        });
    }
    // 6. The first change, sealed.
    let sealed = match signer.commit_and_seal(&repo, &creation_message(&record)) {
        Ok(s) => s,
        Err(error) => {
            return Err(CreateWikiError::Signer {
                step: "seal",
                error,
                cleanup: discard(layout, &id),
            });
        }
    };
    // 7. Verification: a fresh repository that does not verify is not a wiki.
    let verification = match storage.verify(&repo) {
        Ok(v) => v,
        Err(e) => {
            return Err(CreateWikiError::Verification {
                error: Some(e),
                cleanup: discard(layout, &id),
            });
        }
    };
    if !verification.verdict.ok {
        return Err(CreateWikiError::Verification {
            error: None,
            cleanup: discard(layout, &id),
        });
    }
    record.verification = Verification {
        state: VerificationState::Verified,
        at: Some(inklkunn_store::record::now_secs()),
    };
    // 8. The record, last. A lost slug race is reported as the slug being taken, and the attempt removed.
    let record = match store.insert(record) {
        Ok(r) => r,
        Err(e) => {
            let _ = discard(layout, &id);
            return Err(CreateWikiError::Store(e));
        }
    };
    Ok(CreatedWiki {
        record,
        repo,
        first_block_id: sealed.block_id,
    })
}

/// The repository directory of a wiki, for callers that hold a record.
pub fn repo_of(layout: &RootLayout, record: &WikiRecord) -> PathBuf {
    layout.repo_dir(&record.id)
}

/// Is `p` the repository directory of `record` under `layout`?
pub fn is_repo_of(layout: &RootLayout, record: &WikiRecord, p: &Path) -> bool {
    layout.repo_dir(&record.id) == p
}
