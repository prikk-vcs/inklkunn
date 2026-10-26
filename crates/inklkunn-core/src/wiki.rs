//! *Create wiki* (RFC 001 D-5, D-8, D-9): the store record, the root layout, the repository, the bundle
//! files, the first page sealed as a system change, and the first verification. The write steps go through
//! a [`RepositoryBootstrap`], the seam the signer fills; in M0 that is `inklkunn-signer`'s bootstrap
//! signer, which this crate adapts here so that the signer never depends upward.

use std::path::{Path, PathBuf};

use inklkunn_prikk::{Storage, StorageError};
use inklkunn_store::{
    NewWiki, RootLayout, StoreError, Verification, VerificationState, WikiRecord, WikiStore,
};

use crate::bundle;

/// The wiki-creating writes, in the signer's hands.
pub trait RepositoryBootstrap {
    /// Create the repository in `repo` and adopt the wiki's maintainer key; return the key ids.
    fn create_repository(
        &self,
        repo: &Path,
    ) -> Result<inklkunn_signer::KeyIds, inklkunn_signer::SignerError>;
    /// Commit the worktree with `message` and seal it.
    fn commit_and_seal(
        &self,
        repo: &Path,
        message: &str,
    ) -> Result<inklkunn_signer::SealedIds, inklkunn_signer::SignerError>;
}

impl RepositoryBootstrap for inklkunn_signer::BootstrapSigner {
    fn create_repository(
        &self,
        repo: &Path,
    ) -> Result<inklkunn_signer::KeyIds, inklkunn_signer::SignerError> {
        inklkunn_signer::BootstrapSigner::create_repository(self, repo)
    }

    fn commit_and_seal(
        &self,
        repo: &Path,
        message: &str,
    ) -> Result<inklkunn_signer::SealedIds, inklkunn_signer::SignerError> {
        inklkunn_signer::BootstrapSigner::commit_and_seal(self, repo, message)
    }
}

/// Why a wiki was not created. Every variant says which step; a repository left behind is reported, never
/// hidden, so an operator can remove or finish it.
#[derive(Debug)]
pub enum CreateWikiError {
    /// The record could not be stored (slug taken, invalid, backend).
    Store(StoreError),
    /// The root directories could not be made.
    Layout(std::io::Error),
    /// The repository could not be created or the first change not sealed.
    Signer {
        /// Where the repository is, for the operator.
        repo: PathBuf,
        /// What the signer said.
        error: inklkunn_signer::SignerError,
    },
    /// The bundle files could not be written.
    Files {
        /// Where the repository is.
        repo: PathBuf,
        /// The I/O error.
        error: std::io::Error,
    },
    /// The new repository did not verify, or verification could not run.
    Verification {
        /// Where the repository is.
        repo: PathBuf,
        /// The storage error, when the call did not complete.
        error: Option<StorageError>,
    },
}

impl std::fmt::Display for CreateWikiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateWikiError::Store(e) => write!(f, "the wiki record could not be stored: {e}"),
            CreateWikiError::Layout(e) => {
                write!(f, "the wiki's directories could not be created: {e}")
            }
            CreateWikiError::Signer { repo, error } => {
                write!(
                    f,
                    "the repository at {} could not be created or sealed: {error}",
                    repo.display()
                )
            }
            CreateWikiError::Files { repo, error } => {
                write!(
                    f,
                    "the bundle files at {} could not be written: {error}",
                    repo.display()
                )
            }
            CreateWikiError::Verification {
                repo,
                error: Some(e),
            } => {
                write!(
                    f,
                    "the new repository at {} could not be verified: {e}",
                    repo.display()
                )
            }
            CreateWikiError::Verification { repo, error: None } => {
                write!(
                    f,
                    "the new repository at {} failed verification",
                    repo.display()
                )
            }
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

/// The CT-03 message of the creating change.
pub fn creation_message(record: &WikiRecord) -> String {
    format!(
        "Create the wiki {name}\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\nKind: system\nWiki: {id}\n",
        name = record.display_name,
        id = record.id
    )
}

/// Create a wiki: record, directories, repository, bundle files, first sealed change, verification.
pub fn create_wiki(
    store: &dyn WikiStore,
    layout: &RootLayout,
    signer: &dyn RepositoryBootstrap,
    storage: &dyn Storage,
    new: NewWiki,
    inklkunn_version: &str,
) -> Result<CreatedWiki, CreateWikiError> {
    let record = store.create(new).map_err(CreateWikiError::Store)?;
    let repo = layout
        .prepare(&record.id)
        .map_err(CreateWikiError::Layout)?;
    let keys = signer
        .create_repository(&repo)
        .map_err(|error| CreateWikiError::Signer {
            repo: repo.clone(),
            error,
        })?;
    bundle::write_layout(&repo, &record, inklkunn_version).map_err(|error| {
        CreateWikiError::Files {
            repo: repo.clone(),
            error,
        }
    })?;
    let sealed = signer
        .commit_and_seal(&repo, &creation_message(&record))
        .map_err(|error| CreateWikiError::Signer {
            repo: repo.clone(),
            error,
        })?;
    let verification = storage
        .verify(&repo)
        .map_err(|e| CreateWikiError::Verification {
            repo: repo.clone(),
            error: Some(e),
        })?;
    let state = if verification.verdict.ok {
        VerificationState::Verified
    } else {
        VerificationState::Failed
    };
    let record = store
        .set_keys(&record.id, Some(keys.author), Some(keys.maintainer), None)
        .and_then(|_| {
            store.set_verification(
                &record.id,
                Verification {
                    state,
                    at: Some(inklkunn_store::record::now_secs()),
                },
            )
        })
        .map_err(CreateWikiError::Store)?;
    if !verification.verdict.ok {
        return Err(CreateWikiError::Verification { repo, error: None });
    }
    Ok(CreatedWiki {
        record,
        repo,
        first_block_id: sealed.block_id,
    })
}
