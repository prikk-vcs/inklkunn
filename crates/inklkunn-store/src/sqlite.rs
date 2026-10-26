//! The SQLite store (`rusqlite`, bundled): WAL, an idempotent schema, one connection behind a mutex. The
//! schema is versioned in `schema_meta`; a newer schema than this build knows is refused at open rather than
//! read wrong.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::record::{
    NewWiki, Slug, Verification, VerificationState, Visibility, WikiId, WikiRecord, now_secs,
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

impl SqliteStore {
    /// Open or create the database at `path` (its parent must exist), in WAL mode, with the schema applied.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(backend)?;
        Self::init(conn)
    }

    /// An in-memory database (tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(backend)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )
        .map_err(backend)?;
        conn.execute_batch(SCHEMA).map_err(backend)?;
        let found: Option<String> = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        match found.and_then(|v| v.parse::<i64>().ok()) {
            None => {
                conn.execute(
                    "INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )
                .map_err(backend)?;
            }
            Some(v) if v == SCHEMA_VERSION => {}
            Some(v) => {
                return Err(StoreError::Backend(format!(
                    "the database has schema version {v}; this build knows {SCHEMA_VERSION}"
                )));
            }
        }
        Ok(SqliteStore {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        f(&conn)
    }

    fn fetch(conn: &Connection, where_clause: &str, value: &str) -> Result<WikiRecord> {
        let sql = format!("SELECT {COLUMNS} FROM wikis WHERE {where_clause} = ?1");
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
    let languages: String = row.get(5)?;
    let visibility: String = row.get(6)?;
    let state: String = row.get(9)?;
    let invalid = |what: &str| {
        rusqlite::Error::InvalidColumnType(0, what.to_owned(), rusqlite::types::Type::Text)
    };
    Ok(WikiRecord {
        id: WikiId::parse(&id).ok_or_else(|| invalid("id"))?,
        slug: Slug::new(&slug).map_err(|_| invalid("slug"))?,
        display_name: row.get(2)?,
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
    fn create(&self, new: NewWiki) -> Result<WikiRecord> {
        self.with(|conn| {
            let taken: Option<String> = conn
                .query_row("SELECT id FROM wikis WHERE slug = ?1", params![new.slug.as_str()], |r| r.get(0))
                .optional()
                .map_err(backend)?;
            if taken.is_some() {
                return Err(StoreError::SlugTaken(new.slug.to_string()));
            }
            let record = WikiRecord::from_new(WikiId::new(), new, now_secs());
            let languages = serde_json::to_string(&record.languages).unwrap_or_else(|_| "[]".to_owned());
            conn.execute(
                "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
                 policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
                 maintainer_key_id, created_at, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, NULL, NULL, NULL, ?10, \
                 (SELECT COALESCE(MAX(seq), 0) + 1 FROM wikis))",
                params![
                    record.id.as_str(),
                    record.slug.as_str(),
                    record.display_name,
                    record.default_view,
                    record.default_license,
                    languages,
                    record.visibility.as_str(),
                    record.policy_bootstrap,
                    record.verification.state.as_str(),
                    record.created_at,
                ],
            )
            .map_err(backend)?;
            Ok(record)
        })
    }

    fn get(&self, id: &WikiId) -> Result<WikiRecord> {
        self.with(|conn| Self::fetch(conn, "id", id.as_str()))
    }

    fn by_slug(&self, slug: &Slug) -> Result<WikiRecord> {
        self.with(|conn| Self::fetch(conn, "slug", slug.as_str()))
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
        display_name: Option<String>,
    ) -> Result<WikiRecord> {
        self.with(|conn| {
            let current = Self::fetch(conn, "id", id.as_str())?;
            if let Some(s) = &slug {
                let other: Option<String> = conn
                    .query_row(
                        "SELECT id FROM wikis WHERE slug = ?1 AND id <> ?2",
                        params![s.as_str(), id.as_str()],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(backend)?;
                if other.is_some() {
                    return Err(StoreError::SlugTaken(s.to_string()));
                }
            }
            let slug = slug.unwrap_or(current.slug);
            let display_name = display_name.unwrap_or(current.display_name);
            conn.execute(
                "UPDATE wikis SET slug = ?1, display_name = ?2 WHERE id = ?3",
                params![slug.as_str(), display_name, id.as_str()],
            )
            .map_err(backend)?;
            Self::fetch(conn, "id", id.as_str())
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
            Self::fetch(conn, "id", id.as_str())
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
            Self::fetch(conn, "id", id.as_str())
        })
    }
}
