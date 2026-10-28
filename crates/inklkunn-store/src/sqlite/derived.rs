//! The SQLite derived cache (schema version 5). A row that cannot be decoded is a **miss**: the cache is
//! derived, so a damaged row is derived again rather than reported as a failure or guessed at.

use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use super::{SqliteStore, backend};
use crate::Result;
use crate::derived::{DerivedPage, DerivedStore, IndexEntry, PageKey};
use crate::record::WikiId;

/// A page row as stored: `html`, then the three JSON columns, then the time.
struct StoredPage {
    html: String,
    meta: String,
    links: String,
    toc: String,
    computed_at: i64,
}

impl StoredPage {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(StoredPage {
            html: row.get(0)?,
            meta: row.get(1)?,
            links: row.get(2)?,
            toc: row.get(3)?,
            computed_at: row.get(4)?,
        })
    }

    /// `None` when any column does not decode: the row is a miss.
    fn decode(self) -> Option<DerivedPage> {
        Some(DerivedPage {
            html: self.html,
            meta: serde_json::from_str(&self.meta).ok()?,
            links: serde_json::from_str(&self.links).ok()?,
            toc: serde_json::from_str(&self.toc).ok()?,
            computed_at: self.computed_at,
        })
    }
}

/// One index entry. `None` for a row whose aliases do not decode: the whole index is then a miss.
fn decode_entry(row: &Row<'_>) -> rusqlite::Result<Option<IndexEntry>> {
    let path: String = row.get(0)?;
    let title: String = row.get(1)?;
    let aliases: String = row.get(2)?;
    let redirect: Option<String> = row.get(3)?;
    let page_id: Option<String> = row.get(4)?;
    let kind: String = row.get(5)?;
    let problem: Option<String> = row.get(6)?;
    Ok(serde_json::from_str::<Vec<String>>(&aliases)
        .ok()
        .map(|aliases| IndexEntry {
            path,
            title,
            aliases,
            redirect,
            page_id,
            kind,
            problem,
        }))
}

impl DerivedStore for SqliteStore {
    fn page(&self, key: &PageKey<'_>) -> Result<Option<DerivedPage>> {
        self.with(|conn| {
            let stored = conn
                .query_row(
                    "SELECT html, meta, links, toc, computed_at FROM derived_pages \
                     WHERE wiki_id = ?1 AND tip = ?2 AND path = ?3 AND lang = ?4 AND presentation = ?5",
                    params![
                        key.wiki.as_str(),
                        key.tip,
                        key.path,
                        key.lang,
                        key.presentation
                    ],
                    StoredPage::from_row,
                )
                .optional()
                .map_err(backend)?;
            Ok(stored.and_then(StoredPage::decode))
        })
    }

    fn put_page(&self, key: &PageKey<'_>, page: &DerivedPage) -> Result<()> {
        // The three JSON columns are plain data (strings and numbers) and always serialize.
        let meta = serde_json::to_string(&page.meta).map_err(|e| backend_text(&e.to_string()))?;
        let links = serde_json::to_string(&page.links).map_err(|e| backend_text(&e.to_string()))?;
        let toc = serde_json::to_string(&page.toc).map_err(|e| backend_text(&e.to_string()))?;
        self.with(|conn| {
            conn.execute(
                "INSERT INTO derived_pages \
                 (wiki_id, tip, path, lang, presentation, html, meta, links, toc, computed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT (wiki_id, tip, path, lang, presentation) DO UPDATE SET \
                 html = excluded.html, meta = excluded.meta, links = excluded.links, \
                 toc = excluded.toc, computed_at = excluded.computed_at",
                params![
                    key.wiki.as_str(),
                    key.tip,
                    key.path,
                    key.lang,
                    key.presentation,
                    page.html,
                    meta,
                    links,
                    toc,
                    page.computed_at
                ],
            )
            .map(|_| ())
            .map_err(backend)
        })
    }

    fn title_index(&self, wiki: &WikiId, tip: &str) -> Result<Option<Vec<IndexEntry>>> {
        self.with(|conn| {
            let expected: Option<i64> = conn
                .query_row(
                    "SELECT entries FROM title_index_tips WHERE wiki_id = ?1 AND tip = ?2",
                    params![wiki.as_str(), tip],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            let Some(expected) = expected else {
                return Ok(None);
            };
            let mut stmt = conn
                .prepare(
                    "SELECT path, title, aliases, redirect, page_id, kind, problem \
                     FROM title_index_entries WHERE wiki_id = ?1 AND tip = ?2 ORDER BY path",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str(), tip], decode_entry)
                .map_err(backend)?;
            let mut entries = Vec::new();
            for row in rows {
                // A row that does not decode makes the whole index a miss (it is rebuilt and replaced).
                match row.map_err(backend)? {
                    Some(entry) => entries.push(entry),
                    None => return Ok(None),
                }
            }
            // The tip row is written last, in the same transaction as the entries: a count that disagrees
            // means the table was altered, and an index that is not whole is a miss.
            Ok((i64::try_from(entries.len()).ok() == Some(expected)).then_some(entries))
        })
    }

    fn put_title_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        entries: &[IndexEntry],
        built_at: i64,
    ) -> Result<()> {
        let mut encoded = Vec::with_capacity(entries.len());
        for e in entries {
            let aliases =
                serde_json::to_string(&e.aliases).map_err(|e| backend_text(&e.to_string()))?;
            encoded.push((e, aliases));
        }
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            tx.execute(
                "DELETE FROM title_index_entries WHERE wiki_id = ?1 AND tip = ?2",
                params![wiki.as_str(), tip],
            )
            .map_err(backend)?;
            tx.execute(
                "DELETE FROM title_index_tips WHERE wiki_id = ?1 AND tip = ?2",
                params![wiki.as_str(), tip],
            )
            .map_err(backend)?;
            {
                let mut insert = tx
                    .prepare(
                        "INSERT OR REPLACE INTO title_index_entries \
                         (wiki_id, tip, path, title, aliases, redirect, page_id, kind, problem) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    )
                    .map_err(backend)?;
                for (e, aliases) in &encoded {
                    insert
                        .execute(params![
                            wiki.as_str(),
                            tip,
                            e.path,
                            e.title,
                            aliases,
                            e.redirect,
                            e.page_id,
                            e.kind,
                            e.problem
                        ])
                        .map_err(backend)?;
                }
            }
            // `INSERT OR REPLACE` keeps one row per path, so the count is of the rows that landed.
            let landed: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM title_index_entries WHERE wiki_id = ?1 AND tip = ?2",
                    params![wiki.as_str(), tip],
                    |r| r.get(0),
                )
                .map_err(backend)?;
            tx.execute(
                "INSERT INTO title_index_tips (wiki_id, tip, entries, built_at) VALUES (?1, ?2, ?3, ?4)",
                params![wiki.as_str(), tip, landed, built_at],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn retain_tips(&self, wiki: &WikiId, keep: &[String]) -> Result<u64> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            // The tips this wiki has cached anywhere, so that the keep-list is compared in Rust and never
            // spliced into SQL.
            let mut stale: Vec<String> = Vec::new();
            for sql in [
                "SELECT DISTINCT tip FROM derived_pages WHERE wiki_id = ?1",
                "SELECT tip FROM title_index_tips WHERE wiki_id = ?1",
            ] {
                let mut stmt = tx.prepare(sql).map_err(backend)?;
                let tips = stmt
                    .query_map(params![wiki.as_str()], |r| r.get::<_, String>(0))
                    .map_err(backend)?;
                for tip in tips {
                    let tip = tip.map_err(backend)?;
                    if !keep.contains(&tip) && !stale.contains(&tip) {
                        stale.push(tip);
                    }
                }
            }
            let mut removed: u64 = 0;
            for tip in &stale {
                removed += delete_tip(&tx, wiki, tip)?;
            }
            tx.commit().map_err(backend)?;
            Ok(removed)
        })
    }

    fn clear(&self, wiki: &WikiId) -> Result<u64> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let pages = tx
                .execute(
                    "DELETE FROM derived_pages WHERE wiki_id = ?1",
                    params![wiki.as_str()],
                )
                .map_err(backend)?;
            tx.execute(
                "DELETE FROM title_index_entries WHERE wiki_id = ?1",
                params![wiki.as_str()],
            )
            .map_err(backend)?;
            let indexes = tx
                .execute(
                    "DELETE FROM title_index_tips WHERE wiki_id = ?1",
                    params![wiki.as_str()],
                )
                .map_err(backend)?;
            tx.commit().map_err(backend)?;
            Ok(u64::try_from(pages + indexes).unwrap_or(u64::MAX))
        })
    }
}

/// Everything cached for one tip: its pages and its index (counted once).
fn delete_tip(tx: &rusqlite::Transaction<'_>, wiki: &WikiId, tip: &str) -> Result<u64> {
    let pages = tx
        .execute(
            "DELETE FROM derived_pages WHERE wiki_id = ?1 AND tip = ?2",
            params![wiki.as_str(), tip],
        )
        .map_err(backend)?;
    tx.execute(
        "DELETE FROM title_index_entries WHERE wiki_id = ?1 AND tip = ?2",
        params![wiki.as_str(), tip],
    )
    .map_err(backend)?;
    let indexes = tx
        .execute(
            "DELETE FROM title_index_tips WHERE wiki_id = ?1 AND tip = ?2",
            params![wiki.as_str(), tip],
        )
        .map_err(backend)?;
    Ok(u64::try_from(pages + indexes).unwrap_or(u64::MAX))
}

fn backend_text(what: &str) -> crate::StoreError {
    crate::StoreError::Backend(what.to_owned())
}
