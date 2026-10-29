//! The SQLite store (`rusqlite`, bundled): WAL, a versioned schema, one connection behind a mutex.
//!
//! **Opening never guesses about the schema.** The version is read first. A database newer than this build
//! knows, or one whose version cannot be read, is refused before anything is written. An older one is
//! brought forward by [`MIGRATIONS`], in order, inside one immediate transaction that re-reads the version
//! first (a second process may have migrated already); a fresh database gets every migration and the
//! version row in one transaction. Every migration but version 6 only **adds**; version 6 rebuilds the role
//! grants with their foreign keys to accounts, copies every row, checks the result with SQLite's own
//! `foreign_key_check`, and **refuses** (naming the version, rolling everything back) if a grant refers to
//! an account that does not exist. A build one step behind refuses a newer database cleanly in either case. Every change to a record runs in an immediate transaction, so
//! a second process (a command-line run beside the service) cannot slip between a check and a write; a
//! slug or id already taken is reported as exactly that.

mod derived;
mod identity;

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use crate::access::{Role, RoleGrant, RoleStore};
use crate::audit::{AuditEvent, AuditLog, AuditOutcome, AuditRecord, AuditSink};
use crate::record::{
    AccountId, Checkpoint, DisplayName, PrikkVersion, Slug, Verification, VerificationState,
    Visibility, WikiId, WikiRecord,
};
use crate::{Result, StoreError, WikiStore};

/// The schema version this build writes and reads: the last of [`MIGRATIONS`].
pub const SCHEMA_VERSION: i64 = 7;

/// Version 1: M0's schema, the wiki records.
const V1: &str = "
CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS wikis (
    id                   TEXT PRIMARY KEY,
    slug                 TEXT NOT NULL UNIQUE,
    display_name         TEXT NOT NULL,
    default_view         TEXT NOT NULL,
    default_license      TEXT NOT NULL,
    languages            TEXT NOT NULL,
    visibility           TEXT NOT NULL,
    policy_bootstrap     TEXT NOT NULL,
    prikk_format_version TEXT,
    verification_state   TEXT NOT NULL,
    verification_at      INTEGER,
    author_key_id        TEXT,
    maintainer_key_id    TEXT,
    created_at           INTEGER NOT NULL,
    seq                  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS wikis_seq ON wikis(seq);
";

/// Version 2: who holds which role in which wiki (RFC 002 D-2). One role per account per wiki; `seq` orders
/// grants by when each was last made.
const V2: &str = "
CREATE TABLE IF NOT EXISTS role_grants (
    wiki_id    TEXT NOT NULL,
    account_id TEXT NOT NULL,
    role       TEXT NOT NULL,
    granted_by TEXT,
    granted_at INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, account_id)
);
CREATE INDEX IF NOT EXISTS role_grants_wiki ON role_grants(wiki_id, seq);
";

/// Version 3: the audit trail (RFC 002 D-9). **Append-only in the database itself**: triggers abort any
/// `UPDATE`, any `DELETE` and any `INSERT` over a position already taken (`INSERT OR REPLACE`), so even code
/// that holds a connection cannot edit or erase an event through SQL's data statements. This is a guard
/// against code paths, not against someone with the file: they can drop the triggers, and there is no hash
/// chain to show that they did (shipping the trail off the box, RFC 002 D-9's sink trait, is what answers that).
const V3: &str = "
CREATE TABLE IF NOT EXISTS audit_log (
    seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    at        INTEGER NOT NULL,
    principal TEXT NOT NULL,
    action    TEXT NOT NULL,
    resource  TEXT NOT NULL,
    outcome   TEXT NOT NULL,
    detail    TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
CREATE TRIGGER IF NOT EXISTS audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
-- `INSERT OR REPLACE` with a position that exists deletes the old row without firing a delete trigger
-- (recursive triggers are off by default), so a row may not be inserted under a position already taken.
CREATE TRIGGER IF NOT EXISTS audit_log_no_replace BEFORE INSERT ON audit_log
WHEN NEW.seq IS NOT NULL AND EXISTS (SELECT 1 FROM audit_log WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
";

/// Version 4: accounts, the external identities linked to them, browser sessions and API tokens (RFC 002
/// D-1). A password is an Argon2id PHC string and a session id or a token is the SHA-256 of what the client
/// holds, so a copy of this file opens no session and reveals no token. An account has a password and a
/// sign-in name together or neither (`CHECK`); a name is unique; an external identity belongs to at most one
/// account; a token's scope is one of the five (`CHECK`). Foreign keys are enforced.
const V4: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    id            TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL,
    login         TEXT UNIQUE,
    password_hash TEXT,
    created_at    INTEGER NOT NULL,
    disabled      INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1)),
    CHECK ((login IS NULL) = (password_hash IS NULL))
);
CREATE TABLE IF NOT EXISTS account_identities (
    issuer     TEXT NOT NULL,
    subject    TEXT NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    linked_at  INTEGER NOT NULL,
    PRIMARY KEY (issuer, subject)
);
CREATE INDEX IF NOT EXISTS account_identities_account ON account_identities(account_id);
CREATE TABLE IF NOT EXISTS sessions (
    id_hash    TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    csrf       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_account ON sessions(account_id);
CREATE INDEX IF NOT EXISTS sessions_expiry ON sessions(expires_at);
CREATE TABLE IF NOT EXISTS api_tokens (
    id         TEXT PRIMARY KEY,
    hash       TEXT NOT NULL UNIQUE,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    scope      TEXT NOT NULL CHECK (scope IN ('read', 'propose', 'review', 'accept', 'admin')),
    label      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked_at INTEGER
);
CREATE INDEX IF NOT EXISTS api_tokens_account ON api_tokens(account_id);
";

/// Version 5: the derived cache and the checkpoint columns (RFC 003 D-2; handoff 003 T3). **Everything
/// here but the two columns can be dropped and rebuilt from the repositories** (INV-5): rendered pages, and
/// the title index of a view's tip, keyed by prikk's ids. A tip's index is written with its entries in one
/// transaction and counted in `title_index_tips`, which is how a reader knows it is whole. No column holds a
/// verification result, a state or a time of a verification: the cache has nowhere to put one.
///
/// The two columns added to `wikis` are the service's knowledge of prikk's checkpoint record (ledger PK-24):
/// the prikk version it was last refreshed under, and the time of the first unanswered request for a refresh.
const V5: &str = "
CREATE TABLE IF NOT EXISTS derived_pages (
    wiki_id      TEXT NOT NULL,
    tip          TEXT NOT NULL,
    path         TEXT NOT NULL,
    lang         TEXT NOT NULL,
    presentation TEXT NOT NULL,
    html         TEXT NOT NULL,
    meta         TEXT NOT NULL,
    links        TEXT NOT NULL,
    toc          TEXT NOT NULL,
    computed_at  INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip, path, lang, presentation)
);
CREATE INDEX IF NOT EXISTS derived_pages_tip ON derived_pages(wiki_id, tip);
CREATE TABLE IF NOT EXISTS title_index_tips (
    wiki_id  TEXT NOT NULL,
    tip      TEXT NOT NULL,
    entries  INTEGER NOT NULL,
    built_at INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip)
);
CREATE TABLE IF NOT EXISTS title_index_entries (
    wiki_id TEXT NOT NULL,
    tip     TEXT NOT NULL,
    path    TEXT NOT NULL,
    title   TEXT NOT NULL,
    aliases TEXT NOT NULL,
    redirect TEXT,
    page_id TEXT,
    kind    TEXT NOT NULL,
    problem TEXT,
    PRIMARY KEY (wiki_id, tip, path)
);
ALTER TABLE wikis ADD COLUMN checkpoint_prikk_version TEXT;
ALTER TABLE wikis ADD COLUMN checkpoint_refresh_requested_at INTEGER;
";

/// Version 6: the role grants gain their foreign keys (handoff 002 C10; version 4 left them out because the
/// accounts table did not exist yet when version 2 made them): `account_id` and `granted_by` refer to
/// `accounts(id)`. `granted_by` stays nullable: `NULL` is the operator, from the command line (there is no
/// system principal stored; the first administrator of a wiki can only be named that way), and every other
/// value is an account. SQLite cannot add a constraint to a table, so the table is rebuilt: the new one is
/// made, every row copied, the old one dropped, the new one renamed, the index remade. Violations are
/// **deferred** for this transaction only (`defer_foreign_keys` resets at its end), so a database that holds a
/// grant to an account that is not there copies whole and is then refused by name by `apply_migration`,
/// which runs `PRAGMA foreign_key_check(role_grants)` before the version is raised and lets the transaction
/// roll back. The fresh-database path runs the same code.
const V6: &str = "
PRAGMA defer_foreign_keys = ON;
CREATE TABLE role_grants_v6 (
    wiki_id    TEXT NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    role       TEXT NOT NULL,
    granted_by TEXT REFERENCES accounts(id),
    granted_at INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, account_id)
);
INSERT INTO role_grants_v6 (wiki_id, account_id, role, granted_by, granted_at, seq)
    SELECT wiki_id, account_id, role, granted_by, granted_at, seq FROM role_grants;
DROP TABLE role_grants;
ALTER TABLE role_grants_v6 RENAME TO role_grants;
CREATE INDEX role_grants_wiki ON role_grants(wiki_id, seq);
";

/// Version 7: the search index and the observed times (RFC 003 D-8, D-7; handoff 003 T6). **Only adds.**
/// `search_docs` holds every title and alias of a tip with the folded text the search compares, `search_fts` is
/// an FTS5 index over that folded text with the **trigram** tokenizer (a query matches any part of a text, which
/// is how a Japanese title is found by a substring), and `search_tips` says an index is whole (the same device
/// as `title_index_tips`). All three are derived and can be dropped and rebuilt (INV-5). `block_seen` is not:
/// prikk's history carries no time, so the time a block was **first seen by this service** is the only time
/// the pages can show, and it cannot be recovered once lost; the cache's `clear` leaves it alone.
const V7: &str = "
CREATE TABLE IF NOT EXISTS search_tips (
    wiki_id  TEXT NOT NULL,
    tip      TEXT NOT NULL,
    docs     INTEGER NOT NULL,
    built_at INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip)
);
CREATE TABLE IF NOT EXISTS search_docs (
    id      INTEGER PRIMARY KEY,
    wiki_id TEXT NOT NULL,
    tip     TEXT NOT NULL,
    path    TEXT NOT NULL,
    kind    TEXT NOT NULL,
    text    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS search_docs_tip ON search_docs(wiki_id, tip);
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(folded, tokenize = 'trigram');
CREATE TABLE IF NOT EXISTS block_seen (
    wiki_id  TEXT NOT NULL,
    block_id TEXT NOT NULL,
    seen_at  INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, block_id)
);
";

/// Every migration, in order, by the version it brings the database to. A new version is appended here and
/// [`SCHEMA_VERSION`] moves with it (a test holds the two together).
pub const MIGRATIONS: &[(i64, &str)] = &[
    (1, V1),
    (2, V2),
    (3, V3),
    (4, V4),
    (5, V5),
    (6, V6),
    (7, V7),
];

/// The SQLite-backed store.
#[derive(Debug)]
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

/// The schema version a database records: `None` for a fresh one (no `schema_meta` table), the version
/// otherwise. A table without a readable version row is refused, never treated as fresh.
fn read_version(conn: &Connection) -> Result<Option<i64>> {
    let has_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(backend)?
        .unwrap_or(false);
    if !has_meta {
        return Ok(None);
    }
    let found: Option<String> = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(backend)?;
    match found.as_deref().map(str::parse::<i64>) {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(_)) => Err(StoreError::SchemaVersion {
            found: found.unwrap_or_default(),
            known: SCHEMA_VERSION,
        }),
        None => Err(StoreError::SchemaVersion {
            found: "(none)".to_owned(),
            known: SCHEMA_VERSION,
        }),
    }
}

/// Run one migration inside the transaction `tx`, then its check, if it has one. Version 6's check is the
/// foreign keys of the rebuilt role grants: a row that refers to an account that is not there refuses the
/// whole migration, by name; the caller's transaction is dropped, which rolls it back.
fn apply_migration(tx: &rusqlite::Transaction<'_>, version: i64, sql: &str) -> Result<()> {
    tx.execute_batch(sql).map_err(backend)?;
    if version == 6 {
        check_role_grants(tx, version)?;
    }
    Ok(())
}

/// `PRAGMA foreign_key_check(role_grants)`: refuse if any grant names an account that does not exist.
fn check_role_grants(conn: &Connection, version: i64) -> Result<()> {
    let mut stmt = conn
        .prepare("PRAGMA foreign_key_check(role_grants)")
        .map_err(backend)?;
    let rowids: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(1))
        .map_err(backend)?
        .collect::<std::result::Result<_, _>>()
        .map_err(backend)?;
    let Some(first) = rowids.first() else {
        return Ok(());
    };
    // Say which one, by the ids the person who repairs the database will look for.
    let (wiki, account): (String, String) = conn
        .query_row(
            "SELECT wiki_id, account_id FROM role_grants WHERE rowid = ?1",
            params![first],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(backend)?;
    Err(StoreError::MigrationRefused {
        version,
        why: format!(
            "{} role grant(s) refer to an account that does not exist (the first: wiki {wiki}, account {account}); \
             remove those grants or add the accounts, then start again",
            rowids.len()
        ),
    })
}

/// Bring an older database forward. The version is read **again inside the transaction**: a second process
/// may have migrated it between our read and our lock, and migrating twice must not happen.
fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(backend)?;
    let current = read_version(&tx)?.ok_or_else(|| StoreError::SchemaVersion {
        found: "(none)".to_owned(),
        known: SCHEMA_VERSION,
    })?;
    if !(1..=SCHEMA_VERSION).contains(&current) {
        return Err(StoreError::SchemaVersion {
            found: current.to_string(),
            known: SCHEMA_VERSION,
        });
    }
    for (version, sql) in MIGRATIONS {
        if *version > current {
            apply_migration(&tx, *version, sql)?;
        }
    }
    tx.execute(
        "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
        params![SCHEMA_VERSION.to_string()],
    )
    .map_err(backend)?;
    tx.commit().map_err(backend)
}

fn backend(e: rusqlite::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

/// Did SQLite refuse the statement because of a `REFERENCES` clause?
fn is_foreign_key_refusal(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    )
}

/// A uniqueness failure, read from SQLite's own message: which column refused.
fn constraint(e: &rusqlite::Error, record: &WikiRecord) -> Option<StoreError> {
    let rusqlite::Error::SqliteFailure(code, Some(msg)) = e else {
        return None;
    };
    if code.code != rusqlite::ErrorCode::ConstraintViolation {
        return None;
    }
    if msg.contains("wikis.slug") {
        Some(StoreError::SlugTaken(record.slug.to_string()))
    } else if msg.contains("wikis.id") {
        Some(StoreError::IdTaken(record.id.to_string()))
    } else {
        None
    }
}

/// Which column a lookup is by.
#[derive(Debug, Clone, Copy)]
enum Key {
    Id,
    Slug,
}

impl Key {
    const fn column(self) -> &'static str {
        match self {
            Key::Id => "id",
            Key::Slug => "slug",
        }
    }
}

impl SqliteStore {
    /// Open or create the database at `path` (its parent must exist), in WAL mode, with the schema applied.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(backend)?;
        Self::init(conn, true)
    }

    /// An in-memory database (tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(backend)?;
        Self::init(conn, false)
    }

    fn init(mut conn: Connection, file_backed: bool) -> Result<Self> {
        conn.execute_batch("PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;")
            .map_err(backend)?;
        if file_backed {
            let mode: String = conn
                .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
                .map_err(backend)?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(StoreError::Backend(format!(
                    "the database could not be put in WAL mode (journal_mode is `{mode}`)"
                )));
            }
        }
        // 1. Read the version before touching anything.
        match read_version(&conn)? {
            // 2a. A fresh database: every migration and the version row in one transaction.
            None => {
                let tx = conn
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(backend)?;
                for (version, sql) in MIGRATIONS {
                    apply_migration(&tx, *version, sql)?;
                }
                tx.execute(
                    "INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )
                .map_err(backend)?;
                tx.commit().map_err(backend)?;
            }
            Some(v) if v == SCHEMA_VERSION => {}
            // 2b. An older database: forward, in order, in one transaction.
            Some(v) if (1..SCHEMA_VERSION).contains(&v) => migrate(&mut conn)?,
            // 2c. Newer than this build, or not a version at all: refused, nothing written.
            Some(v) => {
                return Err(StoreError::SchemaVersion {
                    found: v.to_string(),
                    known: SCHEMA_VERSION,
                });
            }
        }
        Ok(SqliteStore {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut conn)
    }

    fn fetch(conn: &Connection, key: Key, value: &str) -> Result<WikiRecord> {
        let sql = format!("SELECT {COLUMNS} FROM wikis WHERE {} = ?1", key.column());
        conn.query_row(&sql, params![value], row_to_record)
            .optional()
            .map_err(backend)?
            .ok_or(StoreError::NotFound)
    }
}

const COLUMNS: &str = "id, slug, display_name, default_view, default_license, languages, visibility, \
                       policy_bootstrap, prikk_format_version, verification_state, verification_at, \
                       author_key_id, maintainer_key_id, created_at, checkpoint_prikk_version, \
                       checkpoint_refresh_requested_at";

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<WikiRecord> {
    let id: String = row.get(0)?;
    let slug: String = row.get(1)?;
    let display_name: String = row.get(2)?;
    let languages: String = row.get(5)?;
    let visibility: String = row.get(6)?;
    let state: String = row.get(9)?;
    let checkpoint_version: Option<String> = row.get(14)?;
    let invalid = |what: &str| {
        rusqlite::Error::InvalidColumnType(0, what.to_owned(), rusqlite::types::Type::Text)
    };
    Ok(WikiRecord {
        id: WikiId::parse(&id).ok_or_else(|| invalid("id"))?,
        slug: Slug::from_stored(&slug).map_err(|_| invalid("slug"))?,
        display_name: DisplayName::new(&display_name).map_err(|_| invalid("display_name"))?,
        default_view: row.get(3)?,
        default_license: row.get(4)?,
        languages: serde_json::from_str(&languages).map_err(|_| invalid("languages"))?,
        visibility: Visibility::parse(&visibility).ok_or_else(|| invalid("visibility"))?,
        policy_bootstrap: row.get(7)?,
        prikk_format_version: row.get(8)?,
        verification: Verification {
            state: VerificationState::parse(&state).ok_or_else(|| invalid("verification_state"))?,
            at: row.get(10)?,
        },
        author_key_id: row.get(11)?,
        maintainer_key_id: row.get(12)?,
        checkpoint: Checkpoint {
            refreshed_with: checkpoint_version
                .map(|v| PrikkVersion::parse(&v).ok_or_else(|| invalid("checkpoint_prikk_version")))
                .transpose()?,
            refresh_requested_at: row.get(15)?,
        },
        created_at: row.get(13)?,
    })
}

impl WikiStore for SqliteStore {
    fn insert(&self, record: WikiRecord) -> Result<WikiRecord> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let languages =
                serde_json::to_string(&record.languages).unwrap_or_else(|_| "[]".to_owned());
            let r = tx.execute(
                "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
                 policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
                 maintainer_key_id, created_at, checkpoint_prikk_version, checkpoint_refresh_requested_at, seq) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
                 (SELECT COALESCE(MAX(seq), 0) + 1 FROM wikis))",
                params![
                    record.id.as_str(),
                    record.slug.as_str(),
                    record.display_name.as_str(),
                    record.default_view,
                    record.default_license,
                    languages,
                    record.visibility.as_str(),
                    record.policy_bootstrap,
                    record.prikk_format_version,
                    record.verification.state.as_str(),
                    record.verification.at,
                    record.author_key_id,
                    record.maintainer_key_id,
                    record.created_at,
                    record.checkpoint.refreshed_with.as_ref().map(PrikkVersion::as_str),
                    record.checkpoint.refresh_requested_at,
                ],
            );
            if let Err(e) = r {
                return Err(constraint(&e, &record).unwrap_or_else(|| backend(e)));
            }
            tx.commit().map_err(backend)?;
            Ok(record)
        })
    }

    fn get(&self, id: &WikiId) -> Result<WikiRecord> {
        self.with(|conn| Self::fetch(conn, Key::Id, id.as_str()))
    }

    fn by_slug(&self, slug: &Slug) -> Result<WikiRecord> {
        self.with(|conn| Self::fetch(conn, Key::Slug, slug.as_str()))
    }

    fn list(&self) -> Result<Vec<WikiRecord>> {
        self.with(|conn| {
            let sql = format!("SELECT {COLUMNS} FROM wikis ORDER BY seq");
            let mut stmt = conn.prepare(&sql).map_err(backend)?;
            let rows = stmt.query_map([], row_to_record).map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }

    fn rename(
        &self,
        id: &WikiId,
        slug: Option<Slug>,
        display_name: Option<DisplayName>,
    ) -> Result<WikiRecord> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let current = Self::fetch(&tx, Key::Id, id.as_str())?;
            let slug = slug.unwrap_or(current.slug);
            let display_name = display_name.unwrap_or(current.display_name);
            let r = tx.execute(
                "UPDATE wikis SET slug = ?1, display_name = ?2 WHERE id = ?3",
                params![slug.as_str(), display_name.as_str(), id.as_str()],
            );
            if let Err(e) = r {
                let probe = WikiRecord {
                    slug: slug.clone(),
                    ..Self::fetch(&tx, Key::Id, id.as_str())?
                };
                return Err(constraint(&e, &probe).unwrap_or_else(|| backend(e)));
            }
            let record = Self::fetch(&tx, Key::Id, id.as_str())?;
            tx.commit().map_err(backend)?;
            Ok(record)
        })
    }

    fn set_verification(&self, id: &WikiId, verification: Verification) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET verification_state = ?1, verification_at = ?2 WHERE id = ?3",
                    params![verification.state.as_str(), verification.at, id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn set_keys(
        &self,
        id: &WikiId,
        author_key_id: Option<String>,
        maintainer_key_id: Option<String>,
        prikk_format_version: Option<String>,
    ) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET author_key_id = ?1, maintainer_key_id = ?2, prikk_format_version = ?3 WHERE id = ?4",
                    params![author_key_id, maintainer_key_id, prikk_format_version, id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn request_checkpoint_refresh(&self, id: &WikiId, at: i64) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET checkpoint_refresh_requested_at = COALESCE(checkpoint_refresh_requested_at, ?1) \
                     WHERE id = ?2",
                    params![at, id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn record_checkpoint_refresh(&self, id: &WikiId, prikk: &PrikkVersion) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET checkpoint_prikk_version = ?1, checkpoint_refresh_requested_at = NULL \
                     WHERE id = ?2",
                    params![prikk.as_str(), id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }
}

fn unreadable(what: &str) -> StoreError {
    StoreError::Backend(format!("a stored {what} could not be read"))
}

fn row_to_grant(row: &Row<'_>) -> rusqlite::Result<RoleGrant> {
    let wiki: String = row.get(0)?;
    let account: String = row.get(1)?;
    let role: String = row.get(2)?;
    let granted_by: Option<String> = row.get(3)?;
    let invalid = |what: &str| {
        rusqlite::Error::InvalidColumnType(0, what.to_owned(), rusqlite::types::Type::Text)
    };
    Ok(RoleGrant {
        wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
        account: AccountId::parse(&account).ok_or_else(|| invalid("account_id"))?,
        role: Role::parse(&role).ok_or_else(|| invalid("role"))?,
        granted_by: granted_by
            .map(|g| AccountId::parse(&g).ok_or_else(|| invalid("granted_by")))
            .transpose()?,
        granted_at: row.get(4)?,
    })
}

impl RoleStore for SqliteStore {
    fn grant(&self, grant: RoleGrant) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let wiki_exists = tx
                .query_row(
                    "SELECT 1 FROM wikis WHERE id = ?1",
                    params![grant.wiki.as_str()],
                    |_| Ok(()),
                )
                .optional()
                .map_err(backend)?
                .is_some();
            if !wiki_exists {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO role_grants (wiki_id, account_id, role, granted_by, granted_at, seq) \
                 VALUES (?1, ?2, ?3, ?4, ?5, (SELECT COALESCE(MAX(seq), 0) + 1 FROM role_grants)) \
                 ON CONFLICT (wiki_id, account_id) DO UPDATE SET role = excluded.role, \
                 granted_by = excluded.granted_by, granted_at = excluded.granted_at, seq = excluded.seq",
                params![
                    grant.wiki.as_str(),
                    grant.account.as_str(),
                    grant.role.as_str(),
                    grant.granted_by.as_ref().map(AccountId::as_str),
                    grant.granted_at,
                ],
            )
            .map_err(|e| {
                // The foreign keys to `accounts` are the check: an account (or a grantor) that is not there.
                if is_foreign_key_refusal(&e) {
                    StoreError::AccountNotFound
                } else {
                    backend(e)
                }
            })?;
            tx.commit().map_err(backend)
        })
    }

    fn revoke(&self, account: &AccountId, wiki: &WikiId) -> Result<bool> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "DELETE FROM role_grants WHERE wiki_id = ?1 AND account_id = ?2",
                    params![wiki.as_str(), account.as_str()],
                )
                .map_err(backend)?;
            Ok(n > 0)
        })
    }

    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>> {
        self.with(|conn| {
            let stored: Option<String> = conn
                .query_row(
                    "SELECT role FROM role_grants WHERE wiki_id = ?1 AND account_id = ?2",
                    params![wiki.as_str(), account.as_str()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            stored
                .map(|s| Role::parse(&s).ok_or_else(|| unreadable("role")))
                .transpose()
        })
    }

    fn grants_in(&self, wiki: &WikiId) -> Result<Vec<RoleGrant>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT wiki_id, account_id, role, granted_by, granted_at FROM role_grants \
                     WHERE wiki_id = ?1 ORDER BY seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_grant)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("role grant"))
        })
    }
}

impl AuditSink for SqliteStore {
    fn record(&self, event: &AuditEvent) -> Result<()> {
        let e = event.cleaned();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO audit_log (at, principal, action, resource, outcome, detail) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    e.at,
                    e.principal,
                    e.action,
                    e.resource,
                    e.outcome.as_str(),
                    e.detail
                ],
            )
            .map(|_| ())
            .map_err(backend)
        })
    }
}

impl AuditLog for SqliteStore {
    fn read(&self, after_seq: i64, limit: usize) -> Result<Vec<AuditRecord>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT seq, at, principal, action, resource, outcome, detail FROM audit_log \
                     WHERE seq > ?1 ORDER BY seq LIMIT ?2",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![after_seq, limit], |r| {
                    let outcome: String = r.get(5)?;
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        outcome,
                        r.get::<_, String>(6)?,
                    ))
                })
                .map_err(backend)?;
            let mut out = Vec::new();
            for row in rows {
                let (seq, at, principal, action, resource, outcome, detail) =
                    row.map_err(|_| unreadable("audit event"))?;
                let outcome =
                    AuditOutcome::parse(&outcome).ok_or_else(|| unreadable("audit event"))?;
                out.push(AuditRecord {
                    seq,
                    event: AuditEvent {
                        at,
                        principal,
                        action,
                        resource,
                        outcome,
                        detail,
                    },
                });
            }
            Ok(out)
        })
    }
}
