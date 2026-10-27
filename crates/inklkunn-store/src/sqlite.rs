//! The SQLite store (`rusqlite`, bundled): WAL, a versioned schema, one connection behind a mutex.
//!
//! **Opening is refused before any write when the database is newer than this build knows**: the
//! version is read first, and only a fresh database (no `schema_meta` table) or one at this build's
//! version is touched. Schema creation and the version row land in one transaction. Every change to a
//! record runs in an immediate transaction, so a second process (a command-line run beside the service)
//! cannot slip between a check and a write; a slug or id already taken is reported as exactly that.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use crate::record::{
    DisplayName, Slug, Verification, VerificationState, Visibility, WikiId, WikiRecord,
};
use crate::{Result, StoreError, WikiStore};

/// The schema version this build writes and reads.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
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

/// The SQLite-backed store.
#[derive(Debug)]
pub struct SqliteStore {
    conn: Mutex<Connection>,
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
        let has_meta: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'",
                [],
                |_| Ok(true),
            )
            .optional()
            .map_err(backend)?
            .unwrap_or(false);
        if has_meta {
            let found: Option<String> = conn
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            match found.as_deref().map(str::parse::<i64>) {
                Some(Ok(v)) if v == SCHEMA_VERSION => {}
                Some(Ok(v)) => {
                    return Err(StoreError::SchemaVersion {
                        found: v.to_string(),
                        known: SCHEMA_VERSION,
                    });
                }
                Some(Err(_)) => {
                    return Err(StoreError::SchemaVersion {
                        found: found.unwrap_or_default(),
                        known: SCHEMA_VERSION,
                    });
                }
                None => {
                    return Err(StoreError::SchemaVersion {
                        found: "(none)".to_owned(),
                        known: SCHEMA_VERSION,
                    });
                }
            }
        }
        // 2. A fresh database: schema and version row in one transaction.
        if !has_meta {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            tx.execute_batch(SCHEMA).map_err(backend)?;
            tx.execute(
                "INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)?;
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
