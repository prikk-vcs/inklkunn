#![forbid(unsafe_code)]
//! `inklkunn-store` — inklkunn's service store behind traits, SQLite first (RFC 001 D-8; internal design
//! DM-1, IQ-5). It holds the **wiki record**, the **on-disk root layout**, accounts and the audit trail, and
//! the **derived cache**, and the **proposal, review, acceptance and inbox records** (RFC 004).
//!
//! - [`record`]: the wiki record and its vocabulary; validated slugs, display names and ids, and what the
//!   service knows about prikk's checkpoint record.
//! - [`root`]: `<root>/<shard>/<wiki id>/`, never the wiki's name, so renames move no bytes.
//! - [`access`]: roles, token scopes and the role grants (RFC 002 D-1, D-2).
//! - [`identity`]: accounts, external identities, browser sessions and API tokens, as the service keeps them
//!   (RFC 002 D-1): hashes and ids, never a secret in the form a client holds it.
//! - [`audit`]: the shape of the audit trail and its sink trait (RFC 002 D-9).
//! - [`derived`]: the derived cache, rendered pages and the title index of a view's tip (RFC 003 D-2; DM-6,
//!   DM-7): keyed by prikk's ids, never authoritative, with no field for a verification result.
//! - [`proposal`]: proposals, revisions, comments, decisions and the log of every transition (RFC 004 D-1,
//!   D-3), with drafts visible to their author only.
//! - [`acceptance`]: acceptances, intents and the lease holder (RFC 004 D-4, D-7), written once and never
//!   updated.
//! - [`inbox`]: watches and inbox events (RFC 004 D-9).
//! - [`WikiStore`] and [`RoleStore`]: the traits; [`MemoryStore`] for tests and [`SqliteStore`] for service.
//!
//! The store never holds a key seed (threat model INV-2 for this crate's part): key *ids* only.

pub mod acceptance;
pub mod access;
pub mod audit;
pub mod derived;
pub mod identity;
pub mod inbox;
pub mod memory;
pub mod proposal;
pub mod record;
pub mod root;
pub mod sqlite;

pub use acceptance::{
    AcceptanceBegin, AcceptanceEntry, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult,
    AcceptanceStore, IntentCompletion, IntentEnd, IntentEntry, IntentRecord, LeaseHolder,
};
pub use access::{Role, RoleGrant, RoleStore, Scope};
pub use audit::{
    AuditEvent, AuditLog, AuditOutcome, AuditRecord, AuditSink, MemoryAudit, export_jsonl,
};
pub use derived::{
    DerivedHeading, DerivedLinks, DerivedMeta, DerivedPage, DerivedStore, IndexEntry, PageKey,
    SearchDoc, SearchHit, SearchKind,
};
pub use identity::{
    Account, AccountStore, ExternalIdentity, ExternalIdentityError, LocalCredential, LoginName,
    LoginNameError, SessionRecord, SessionStore, TokenRecord, TokenStore,
};
pub use inbox::{InboxEvent, InboxEventKind, InboxStore, WatchRecord, WatchTarget};
pub use memory::MemoryStore;
pub use proposal::{
    Anchor, Base, CommentRecord, DecisionKind, DecisionRecord, FileChange, ProposalChange,
    ProposalFilter, ProposalOp, ProposalRecord, ProposalState, ProposalStore, ProposalTransition,
    Quote, Rename, RevisionContent, RevisionFlags, RevisionRecord,
};
pub use record::{
    AcceptanceId, AccountId, Attempt, Checkpoint, CommentId, DecisionId, DisplayName,
    DisplayNameError, InboxEventId, IntentId, NewWiki, NewWikiError, NotRun, PrikkVersion,
    ProposalId, Slug, SlugError, TokenId, Verification, VerificationState, Visibility, WikiId,
    WikiRecord,
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
    /// No account has this id. Also what a role grant is refused with when its account, or the account that
    /// grants it, does not exist.
    AccountNotFound,
    /// Another account already signs in with this name.
    LoginTaken(String),
    /// Another account is already linked to this external identity.
    IdentityTaken,
    /// A password hash is not shaped like an Argon2id PHC string (`$argon2id$v=19$m=…,t=…,p=…$salt$hash`).
    InvalidCredential,
    /// The database was written by a newer build (or holds a value this build cannot read); refused
    /// before anything is changed.
    SchemaVersion {
        /// What the database says.
        found: String,
        /// What this build knows.
        known: i64,
    },
    /// A migration found data it cannot carry forward and changed nothing (the transaction was rolled
    /// back); `why` says what to repair. Version 6 refuses a role grant that refers to an account that does
    /// not exist.
    MigrationRefused {
        /// The schema version the migration was bringing the database to.
        version: i64,
        /// What is wrong and what to do.
        why: String,
    },
    /// A change was made against a record that is no longer as the change assumed (a proposal that left the
    /// state the change needs, a revision that is no longer the latest); nothing was changed. The caller
    /// reads the record again and decides.
    StateChanged,
    /// A record that is written once already exists under its key (a second decision by one reviewer on one
    /// revision, a second end of an acceptance); nothing was changed.
    AlreadyRecorded,
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
            StoreError::AccountNotFound => write!(f, "no such account"),
            StoreError::LoginTaken(s) => write!(f, "the sign-in name `{s}` is taken"),
            StoreError::IdentityTaken => {
                write!(f, "that external identity is linked to another account")
            }
            StoreError::InvalidCredential => {
                write!(f, "a password hash is not an Argon2id string")
            }
            StoreError::SchemaVersion { found, known } => write!(
                f,
                "the database has schema version `{found}`; this build knows {known} and will not touch it"
            ),
            StoreError::MigrationRefused { version, why } => write!(
                f,
                "the database cannot be brought to schema version {version}: {why}; nothing was changed"
            ),
            StoreError::StateChanged => {
                write!(
                    f,
                    "the record changed since it was read; nothing was changed"
                )
            }
            StoreError::AlreadyRecorded => {
                write!(
                    f,
                    "that record is written once and already exists; nothing was changed"
                )
            }
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
    /// Record a verification **verdict** (a check that ran to completion) and clear any later attempt that
    /// could not run: the verdict is now the latest thing that happened.
    fn set_verification(&self, id: &WikiId, verification: Verification) -> Result<WikiRecord>;
    /// Record that a verification could not run to completion. **The last verdict is left as it was**: a check
    /// that did not run never replaces what the last one found (handoff 003, review R1).
    fn record_attempt(&self, id: &WikiId, attempt: Attempt) -> Result<WikiRecord>;
    /// Record the wiki's key ids (never seeds) and prikk format version after the signer created them.
    fn set_keys(
        &self,
        id: &WikiId,
        author_key_id: Option<String>,
        maintainer_key_id: Option<String>,
        prikk_format_version: Option<String>,
    ) -> Result<WikiRecord>;
    /// A reader met `extract` refusing for want of prikk's checkpoint record (ledger PK-24): ask for a
    /// refresh. The first unanswered request's time is kept, so asking again changes nothing.
    fn request_checkpoint_refresh(&self, id: &WikiId, at: i64) -> Result<WikiRecord>;
    /// The checkpoint record was refreshed under `prikk` (a writable `verify`): record it and clear the
    /// request.
    fn record_checkpoint_refresh(&self, id: &WikiId, prikk: &PrikkVersion) -> Result<WikiRecord>;
}
