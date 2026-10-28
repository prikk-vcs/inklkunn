#![forbid(unsafe_code)]
//! `inklkunn-store` — inklkunn's service store behind traits, SQLite first (RFC 001 D-8; internal design
//! DM-1, IQ-5). M0 holds the **wiki record** and the **on-disk root layout**; proposals, review state,
//! audit and the derived cache arrive with RFCs 004 and 003.
//!
//! - [`record`]: the wiki record and its vocabulary; validated slugs, display names and ids.
//! - [`root`]: `<root>/<shard>/<wiki id>/`, never the wiki's name, so renames move no bytes.
//! - [`access`]: roles, token scopes and the role grants (RFC 002 D-1, D-2).
//! - [`audit`]: the shape of the audit trail and its sink trait (RFC 002 D-9).
//! - [`WikiStore`] and [`RoleStore`]: the traits; [`MemoryStore`] for tests and [`SqliteStore`] for service.
//!
//! The store never holds a key seed (threat model INV-2 for this crate's part): key *ids* only.

pub mod access;
pub mod audit;
pub mod memory;
pub mod record;
pub mod root;
pub mod sqlite;

pub use access::{Role, RoleGrant, RoleStore, Scope};
pub use audit::{AuditEvent, AuditOutcome, AuditSink, MemoryAudit};
pub use memory::MemoryStore;
pub use record::{
    AccountId, DisplayName, DisplayNameError, NewWiki, NewWikiError, ProposalId, Slug, SlugError,
    Verification, VerificationState, Visibility, WikiId, WikiRecord,
};
pub use root::RootLayout;
pub use sqlite::SqliteStore;

/// Why a store call failed. Every variant is one plain fact; a backend's own text appears only in
/// [`StoreError::Backend`], never dressed as a user-facing refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// No wiki has this id or slug.
    NotFound,
    /// Another wiki already has this slug.
    SlugTaken(String),
    /// Another wiki already has this id (a fresh id collided; the caller may draw another).
    IdTaken(String),
    /// The slug is not valid (see [`Slug`]).
    InvalidSlug(SlugError),
    /// A creation request's free-text field is not valid (see [`NewWikiError`]).
    InvalidRequest(NewWikiError),
    /// The database was written by a newer build (or holds a value this build cannot read); refused
    /// before anything is changed.
    SchemaVersion {
        /// What the database says.
        found: String,
        /// What this build knows.
        known: i64,
    },
    /// The backing store failed; the text is the backend's, without content.
    Backend(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound => write!(f, "no such wiki"),
            StoreError::SlugTaken(s) => write!(f, "the slug `{s}` is taken"),
            StoreError::IdTaken(s) => write!(f, "the id `{s}` is taken"),
            StoreError::InvalidSlug(e) => write!(f, "invalid slug: {e}"),
            StoreError::InvalidRequest(e) => write!(f, "invalid request: {e}"),
            StoreError::SchemaVersion { found, known } => write!(
                f,
                "the database has schema version `{found}`; this build knows {known} and will not touch it"
            ),
            StoreError::Backend(e) => write!(f, "store backend: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<SlugError> for StoreError {
    fn from(e: SlugError) -> Self {
        StoreError::InvalidSlug(e)
    }
}

impl From<NewWikiError> for StoreError {
    fn from(e: NewWikiError) -> Self {
        StoreError::InvalidRequest(e)
    }
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, StoreError>;

/// The wiki record store. Every implementation passes the same invariant tests (`tests/invariants.rs`).
pub trait WikiStore: Send + Sync {
    /// Store a complete record. The slug and the id must be free; the whole record lands or nothing does.
    /// *Create wiki* builds the record first, makes the repository, and inserts last, so a failure on the
    /// way leaves no record behind.
    fn insert(&self, record: WikiRecord) -> Result<WikiRecord>;
    /// Create a wiki record with a fresh id from a creation request (validated by
    /// [`WikiRecord::from_new`]).
    fn create(&self, new: NewWiki) -> Result<WikiRecord> {
        let record = WikiRecord::from_new(WikiId::new(), new, record::now_secs())?;
        self.insert(record)
    }
    /// The record with this id.
    fn get(&self, id: &WikiId) -> Result<WikiRecord>;
    /// The record with this slug.
    fn by_slug(&self, slug: &Slug) -> Result<WikiRecord>;
    /// Every record, by creation order.
    fn list(&self) -> Result<Vec<WikiRecord>>;
    /// Change the slug and/or the display name. The id, and so the on-disk path, never change.
    fn rename(
        &self,
        id: &WikiId,
        slug: Option<Slug>,
        display_name: Option<DisplayName>,
    ) -> Result<WikiRecord>;
    /// Record the outcome of a verification.
    fn set_verification(&self, id: &WikiId, verification: Verification) -> Result<WikiRecord>;
    /// Record the wiki's key ids (never seeds) and prikk format version after the signer created them.
    fn set_keys(
        &self,
        id: &WikiId,
        author_key_id: Option<String>,
        maintainer_key_id: Option<String>,
        prikk_format_version: Option<String>,
    ) -> Result<WikiRecord>;
}
