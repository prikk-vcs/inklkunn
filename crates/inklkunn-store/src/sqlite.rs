//! The SQLite store (`rusqlite`, bundled): WAL, a versioned schema, one connection behind a mutex.
//!
//! **Opening never guesses about the schema.** The version is read first. A database newer than this build
//! knows, or one whose version cannot be read, is refused before anything is written. An older one is
//! brought forward by [`MIGRATIONS`], in order, inside one immediate transaction that re-reads the version
//! first (a second process may have migrated already); a fresh database gets every migration and the
//! version row in one transaction. Every migration only **adds**, so a build one step behind can still
//! refuse cleanly and nothing is rewritten. Every change to a record runs in an immediate transaction, so
//! a second process (a command-line run beside the service) cannot slip between a check and a write; a
//! slug or id already taken is reported as exactly that.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use crate::access::{Role, RoleGrant, RoleStore};
use crate::audit::{AuditEvent, AuditLog, AuditOutcome, AuditRecord, AuditSink};
use crate::record::{
    AccountId, DisplayName, Slug, Verification, VerificationState, Visibility, WikiId, WikiRecord,
};
use crate::{Result, StoreError, WikiStore};

/// The schema version this build writes and reads: the last of [`MIGRATIONS`].
pub const SCHEMA_VERSION: i64 = 3;

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
/// `UPDATE` or `DELETE`, so even code that holds a connection cannot edit or erase an event through SQL's
/// data statements (dropping the table needs a deliberate schema change, which no code path here makes).
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
";

/// Every migration, in order, by the version it brings the database to. A new version is appended here and
/// [`SCHEMA_VERSION`] moves with it (a test holds the two together).
pub const MIGRATIONS: &[(i64, &str)] = &[(1, V1), (2, V2), (3, V3)];

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
            tx.execute_batch(sql).map_err(backend)?;
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
        conn.execute_batch("PRAGMA busy_timeout=5000;")
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
                for (_, sql) in MIGRATIONS {
                    tx.execute_batch(sql).map_err(backend)?;
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
                       author_key_id, maintainer_key_id, created_at";

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<WikiRecord> {
    let id: String = row.get(0)?;
    let slug: String = row.get(1)?;
    let display_name: String = row.get(2)?;
    let languages: String = row.get(5)?;
    let visibility: String = row.get(6)?;
    let state: String = row.get(9)?;
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
                 maintainer_key_id, created_at, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, \
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
            .map_err(backend)?;
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
