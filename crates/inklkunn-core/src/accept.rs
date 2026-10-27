//! The `Accept` trait (RFC 002 D-4; internal design SGN-3): one operation that brings an **accepted**
//! proposal into the repository, and one that creates a view. In M0 the trait exists with its types and no
//! implementation; `inklkunn-signer` implements it in RFC 002's handoff, and nothing else may (the
//! key-reachability gate).

use std::path::PathBuf;

/// The id of an acceptance record in state *pending seal* (RFC 004's records; the guard is RFC 002 D-4's).
/// Holding one proves nothing by itself: the signer re-reads the record the id names and refuses unless it
/// is pending seal and the message matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AcceptanceToken(String);

impl AcceptanceToken {
    /// Name an acceptance record.
    pub fn new(record_id: impl Into<String>) -> Self {
        AcceptanceToken(record_id.into())
    }

    /// The record id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The id of a recorded view-creation act (RFC 002 D-4, SGN-4): `create_view` is bound to one as `accept`
/// is bound to an acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewCreationAct(String);

impl ViewCreationAct {
    /// Name a view-creation act.
    pub fn new(record_id: impl Into<String>) -> Self {
        ViewCreationAct(record_id.into())
    }

    /// The record id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One file of a prepared change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedFile {
    /// Repository-relative path under `pages/` or `attachments/`. **The signer validates it** against the
    /// title-to-path rule and prikk's path rules before anything is written; this type carries, it does
    /// not guarantee.
    pub path: PathBuf,
    /// The bytes to write; `None` removes the file.
    pub content: Option<Vec<u8>>,
}

/// A change ready to be written: files, declared renames, and the CT-03 message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedChange {
    /// Files written or removed.
    pub files: Vec<PreparedFile>,
    /// `(from, to)` renames, declared with `prikk mv` so identity survives.
    pub renames: Vec<(PathBuf, PathBuf)>,
    /// The CT-03 message: summary line, then `Proposal:`, `Proposed-By:`, `Accepted-By:`, `Base:`, `Kind:`.
    pub message: String,
}

/// What a sealed change leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedChange {
    /// The sealed block.
    pub block_id: String,
    /// The patches it seals.
    pub patch_ids: Vec<String>,
}

/// Why an acceptance was refused or did not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptError {
    /// The token names no acceptance record in state *pending seal* (or no recorded view-creation act).
    NotPending,
    /// The message does not name the token's proposal and acceptor (RFC 002 D-5).
    MessageMismatch,
    /// prikk refused before `commit`; nothing changed.
    Refused(String),
    /// `commit` succeeded and a later step failed; RFC 004's recovery completes it.
    Incomplete(String),
    /// The call could not run.
    Unavailable(String),
}

impl std::fmt::Display for AcceptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcceptError::NotPending => write!(f, "the acceptance is not pending seal"),
            AcceptError::MessageMismatch => write!(
                f,
                "the message does not name the acceptance's proposal and acceptor"
            ),
            AcceptError::Refused(m) => write!(f, "refused before commit: {m}"),
            AcceptError::Incomplete(m) => {
                write!(f, "committed but not sealed; recovery pending: {m}")
            }
            AcceptError::Unavailable(m) => write!(f, "could not run: {m}"),
        }
    }
}

impl std::error::Error for AcceptError {}

/// The one way into the repository's history.
pub trait Accept: Send + Sync {
    /// Bring the worktree to `view`, apply `change`, commit under the wiki's author key, seal under its
    /// maintainer key, verify, and return the ids. Refuses unless `token` is pending seal and the message
    /// names its proposal and acceptor.
    fn accept(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        token: &AcceptanceToken,
        change: &PreparedChange,
    ) -> Result<SealedChange, AcceptError>;

    /// Create view `name` from point `from`: a maintainer-signed publication bound to the recorded act
    /// `act`. Refuses unless the act is recorded and names this wiki, view and point.
    fn create_view(
        &self,
        wiki: &inklkunn_store::WikiId,
        name: &str,
        from: &str,
        act: &ViewCreationAct,
    ) -> Result<(), AcceptError>;
}
