//! The SQLite derived cache (schema versions 5 and 7). A row that cannot be decoded is a **miss**: the cache is
//! derived, so a damaged row is derived again rather than reported as a failure or guessed at.

use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use super::{SqliteStore, backend};
use crate::Result;
use crate::derived::{
    DerivedPage, DerivedStore, IndexEntry, KnowledgeAnchorRow, KnowledgeClaimRow,
    KnowledgeInvalidRow, KnowledgeLinkRow, KnowledgeRelationRow, KnowledgeRows, KnowledgeSourceRow,
    PageKey, SearchDoc, SearchHit, SearchKind,
};
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
                "SELECT tip FROM search_tips WHERE wiki_id = ?1",
                "SELECT tip FROM knowledge_tips WHERE wiki_id = ?1",
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
            let searches = delete_search(&tx, wiki, None)?;
            let knowledge = delete_knowledge(&tx, wiki, None)?;
            tx.commit().map_err(backend)?;
            Ok(u64::try_from(pages + indexes + searches + knowledge).unwrap_or(u64::MAX))
        })
    }

    fn put_search_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        docs: &[SearchDoc],
        built_at: i64,
    ) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            delete_search(&tx, wiki, Some(tip))?;
            {
                let mut doc = tx
                    .prepare(
                        "INSERT INTO search_docs (wiki_id, tip, path, kind, text) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                    )
                    .map_err(backend)?;
                let mut fts = tx
                    .prepare("INSERT INTO search_fts (rowid, folded) VALUES (?1, ?2)")
                    .map_err(backend)?;
                for d in docs {
                    doc.execute(params![wiki.as_str(), tip, d.path, d.kind.code(), d.text])
                        .map_err(backend)?;
                    fts.execute(params![tx.last_insert_rowid(), d.folded])
                        .map_err(backend)?;
                }
            }
            tx.execute(
                "INSERT INTO search_tips (wiki_id, tip, docs, built_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    wiki.as_str(),
                    tip,
                    i64::try_from(docs.len()).unwrap_or(i64::MAX),
                    built_at
                ],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn search(
        &self,
        wiki: &WikiId,
        tip: &str,
        folded_query: &str,
        limit: usize,
    ) -> Result<Option<Vec<SearchHit>>> {
        self.with(|conn| {
            let expected: Option<i64> = conn
                .query_row(
                    "SELECT docs FROM search_tips WHERE wiki_id = ?1 AND tip = ?2",
                    params![wiki.as_str(), tip],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            if expected.is_none() {
                return Ok(None);
            }
            let limit = i64::try_from(limit).unwrap_or(i64::MAX);
            let query = folded_query.trim();
            // The trigram tokenizer needs three characters to match on; a shorter query is a scan of this
            // tip's texts (LIKE, with the wildcards escaped), which gives the same substring answer.
            let hits = if query.chars().count() >= 3 {
                let phrase = format!("\"{}\"", query.replace('"', "\"\""));
                run_search(
                    conn,
                    "SELECT d.path, d.kind, d.text FROM search_fts f JOIN search_docs d ON d.id = f.rowid \
                     WHERE search_fts MATCH ?1 AND d.wiki_id = ?2 AND d.tip = ?3 ORDER BY d.id LIMIT ?4",
                    &phrase,
                    wiki,
                    tip,
                    limit,
                )?
            } else {
                let pattern = format!(
                    "%{}%",
                    query
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                );
                run_search(
                    conn,
                    "SELECT d.path, d.kind, d.text FROM search_fts f JOIN search_docs d ON d.id = f.rowid \
                     WHERE d.wiki_id = ?2 AND d.tip = ?3 AND f.folded LIKE ?1 ESCAPE '\\' \
                     ORDER BY d.id LIMIT ?4",
                    &pattern,
                    wiki,
                    tip,
                    limit,
                )?
            };
            Ok(Some(hits))
        })
    }

    fn knowledge_index(&self, wiki: &WikiId, tip: &str) -> Result<Option<KnowledgeRows>> {
        self.with(|conn| {
            let expected: Option<i64> = conn
                .query_row(
                    "SELECT row_count FROM knowledge_tips WHERE wiki_id = ?1 AND tip = ?2",
                    params![wiki.as_str(), tip],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            let Some(expected) = expected else {
                return Ok(None);
            };
            let rows = read_knowledge(conn, wiki, tip)?;
            // The tip row is written last, in the same transaction as the rows: a count that disagrees means a
            // table was altered, and an index that is not whole is a miss.
            Ok((i64::try_from(rows.len()).ok() == Some(expected)).then_some(rows))
        })
    }

    fn put_knowledge_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        rows: &KnowledgeRows,
        built_at: i64,
    ) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            delete_knowledge(&tx, wiki, Some(tip))?;
            let (w, t) = (wiki.as_str(), tip);
            for c in &rows.claims {
                tx.execute(
                    "INSERT OR REPLACE INTO knowledge_claims \
                     (wiki_id, tip, id, kind, lifecycle, standing, standing_proposal, standing_judgement) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        w,
                        t,
                        c.id,
                        c.kind,
                        c.lifecycle,
                        c.standing,
                        c.standing_proposal,
                        c.standing_judgement
                    ],
                )
                .map_err(backend)?;
            }
            for s in &rows.sources {
                tx.execute(
                    "INSERT OR REPLACE INTO knowledge_sources \
                     (wiki_id, tip, id, kind, availability, last_reviewed, superseded_by) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        w,
                        t,
                        s.id,
                        s.kind,
                        s.availability,
                        s.last_reviewed,
                        s.superseded_by
                    ],
                )
                .map_err(backend)?;
            }
            for r in &rows.relations {
                tx.execute(
                    "INSERT OR REPLACE INTO knowledge_relations (wiki_id, tip, claim, kind, other) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![w, t, r.claim, r.kind, r.other],
                )
                .map_err(backend)?;
            }
            for l in &rows.links {
                tx.execute(
                    "INSERT OR REPLACE INTO knowledge_links \
                     (wiki_id, tip, claim, source, relation, bearing, judgement) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![w, t, l.claim, l.source, l.relation, l.bearing, l.judgement],
                )
                .map_err(backend)?;
            }
            for a in &rows.anchors {
                tx.execute(
                    "INSERT OR REPLACE INTO knowledge_anchors \
                     (wiki_id, tip, claim, page_id, block, page_path, state, digest) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![w, t, a.claim, a.page_id, a.block, a.page_path, a.state, a.digest],
                )
                .map_err(backend)?;
            }
            for i in &rows.invalid {
                tx.execute(
                    "INSERT OR REPLACE INTO knowledge_invalid (wiki_id, tip, path, rule) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![w, t, i.path, i.rule],
                )
                .map_err(backend)?;
            }
            // `INSERT OR REPLACE` keeps one row per key, so the count is of the rows that landed.
            let landed = count_knowledge(&tx, wiki, tip)?;
            tx.execute(
                "INSERT INTO knowledge_tips (wiki_id, tip, row_count, built_at) VALUES (?1, ?2, ?3, ?4)",
                params![w, t, landed, built_at],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn first_seen(&self, wiki: &WikiId, blocks: &[String], now: i64) -> Result<Vec<i64>> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let mut out = Vec::with_capacity(blocks.len());
            {
                let mut insert = tx
                    .prepare(
                        "INSERT OR IGNORE INTO block_seen (wiki_id, block_id, seen_at) VALUES (?1, ?2, ?3)",
                    )
                    .map_err(backend)?;
                let mut read = tx
                    .prepare("SELECT seen_at FROM block_seen WHERE wiki_id = ?1 AND block_id = ?2")
                    .map_err(backend)?;
                for block in blocks {
                    insert
                        .execute(params![wiki.as_str(), block, now])
                        .map_err(backend)?;
                    out.push(
                        read.query_row(params![wiki.as_str(), block], |r| r.get::<_, i64>(0))
                            .map_err(backend)?,
                    );
                }
            }
            tx.commit().map_err(backend)?;
            Ok(out)
        })
    }
}

fn run_search(
    conn: &rusqlite::Connection,
    sql: &str,
    pattern: &str,
    wiki: &WikiId,
    tip: &str,
    limit: i64,
) -> Result<Vec<SearchHit>> {
    let mut stmt = conn.prepare(sql).map_err(backend)?;
    let rows = stmt
        .query_map(params![pattern, wiki.as_str(), tip, limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(backend)?;
    let mut hits = Vec::new();
    for row in rows {
        let (path, kind, text) = row.map_err(backend)?;
        // A kind this build does not know is a damaged row: skipped, the cache is derived.
        if let Some(kind) = SearchKind::from_code(&kind) {
            hits.push(SearchHit { path, kind, text });
        }
    }
    Ok(hits)
}

/// Drop the search index of one tip (`Some`) or of every tip (`None`) of a wiki; how many indexes went.
fn delete_search(
    tx: &rusqlite::Transaction<'_>,
    wiki: &WikiId,
    tip: Option<&str>,
) -> Result<usize> {
    match tip {
        Some(tip) => {
            tx.execute(
                "DELETE FROM search_fts WHERE rowid IN \
                 (SELECT id FROM search_docs WHERE wiki_id = ?1 AND tip = ?2)",
                params![wiki.as_str(), tip],
            )
            .map_err(backend)?;
            tx.execute(
                "DELETE FROM search_docs WHERE wiki_id = ?1 AND tip = ?2",
                params![wiki.as_str(), tip],
            )
            .map_err(backend)?;
            tx.execute(
                "DELETE FROM search_tips WHERE wiki_id = ?1 AND tip = ?2",
                params![wiki.as_str(), tip],
            )
            .map_err(backend)
        }
        None => {
            tx.execute(
                "DELETE FROM search_fts WHERE rowid IN (SELECT id FROM search_docs WHERE wiki_id = ?1)",
                params![wiki.as_str()],
            )
            .map_err(backend)?;
            tx.execute(
                "DELETE FROM search_docs WHERE wiki_id = ?1",
                params![wiki.as_str()],
            )
            .map_err(backend)?;
            tx.execute(
                "DELETE FROM search_tips WHERE wiki_id = ?1",
                params![wiki.as_str()],
            )
            .map_err(backend)
        }
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
    let searches = delete_search(tx, wiki, Some(tip))?;
    let knowledge = delete_knowledge(tx, wiki, Some(tip))?;
    Ok(u64::try_from(pages + indexes + searches + knowledge).unwrap_or(u64::MAX))
}

/// The tables of the knowledge index (all but the tips row).
const KNOWLEDGE_TABLES: [&str; 6] = [
    "knowledge_claims",
    "knowledge_sources",
    "knowledge_relations",
    "knowledge_links",
    "knowledge_anchors",
    "knowledge_invalid",
];

/// Drop the knowledge index of one tip (`Some`) or of every tip (`None`) of a wiki; how many indexes went.
fn delete_knowledge(
    tx: &rusqlite::Transaction<'_>,
    wiki: &WikiId,
    tip: Option<&str>,
) -> Result<usize> {
    for table in KNOWLEDGE_TABLES {
        match tip {
            Some(tip) => tx.execute(
                &format!("DELETE FROM {table} WHERE wiki_id = ?1 AND tip = ?2"),
                params![wiki.as_str(), tip],
            ),
            None => tx.execute(
                &format!("DELETE FROM {table} WHERE wiki_id = ?1"),
                params![wiki.as_str()],
            ),
        }
        .map_err(backend)?;
    }
    match tip {
        Some(tip) => tx.execute(
            "DELETE FROM knowledge_tips WHERE wiki_id = ?1 AND tip = ?2",
            params![wiki.as_str(), tip],
        ),
        None => tx.execute(
            "DELETE FROM knowledge_tips WHERE wiki_id = ?1",
            params![wiki.as_str()],
        ),
    }
    .map_err(backend)
}

/// How many rows the tables hold for the tip.
fn count_knowledge(conn: &rusqlite::Connection, wiki: &WikiId, tip: &str) -> Result<i64> {
    let mut total = 0;
    for table in KNOWLEDGE_TABLES {
        let n: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE wiki_id = ?1 AND tip = ?2"),
                params![wiki.as_str(), tip],
                |r| r.get(0),
            )
            .map_err(backend)?;
        total += n;
    }
    Ok(total)
}

/// The rows of a tip's knowledge index, each table in its key order.
fn read_knowledge(conn: &rusqlite::Connection, wiki: &WikiId, tip: &str) -> Result<KnowledgeRows> {
    let (w, t) = (wiki.as_str(), tip);
    let mut rows = KnowledgeRows::default();
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, lifecycle, standing, standing_proposal, standing_judgement \
             FROM knowledge_claims WHERE wiki_id = ?1 AND tip = ?2 ORDER BY id",
        )
        .map_err(backend)?;
    for row in stmt
        .query_map(params![w, t], |r| {
            Ok(KnowledgeClaimRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                lifecycle: r.get(2)?,
                standing: r.get(3)?,
                standing_proposal: r.get(4)?,
                standing_judgement: r.get(5)?,
            })
        })
        .map_err(backend)?
    {
        rows.claims.push(row.map_err(backend)?);
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, availability, last_reviewed, superseded_by \
             FROM knowledge_sources WHERE wiki_id = ?1 AND tip = ?2 ORDER BY id",
        )
        .map_err(backend)?;
    for row in stmt
        .query_map(params![w, t], |r| {
            Ok(KnowledgeSourceRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                availability: r.get(2)?,
                last_reviewed: r.get(3)?,
                superseded_by: r.get(4)?,
            })
        })
        .map_err(backend)?
    {
        rows.sources.push(row.map_err(backend)?);
    }
    let mut stmt = conn
        .prepare(
            "SELECT claim, kind, other FROM knowledge_relations \
             WHERE wiki_id = ?1 AND tip = ?2 ORDER BY claim, kind, other",
        )
        .map_err(backend)?;
    for row in stmt
        .query_map(params![w, t], |r| {
            Ok(KnowledgeRelationRow {
                claim: r.get(0)?,
                kind: r.get(1)?,
                other: r.get(2)?,
            })
        })
        .map_err(backend)?
    {
        rows.relations.push(row.map_err(backend)?);
    }
    let mut stmt = conn
        .prepare(
            "SELECT claim, source, relation, bearing, judgement FROM knowledge_links \
             WHERE wiki_id = ?1 AND tip = ?2 ORDER BY claim, source",
        )
        .map_err(backend)?;
    for row in stmt
        .query_map(params![w, t], |r| {
            Ok(KnowledgeLinkRow {
                claim: r.get(0)?,
                source: r.get(1)?,
                relation: r.get(2)?,
                bearing: r.get(3)?,
                judgement: r.get(4)?,
            })
        })
        .map_err(backend)?
    {
        rows.links.push(row.map_err(backend)?);
    }
    let mut stmt = conn
        .prepare(
            "SELECT claim, page_id, block, page_path, state, digest FROM knowledge_anchors \
             WHERE wiki_id = ?1 AND tip = ?2 ORDER BY claim, page_id, block",
        )
        .map_err(backend)?;
    for row in stmt
        .query_map(params![w, t], |r| {
            Ok(KnowledgeAnchorRow {
                claim: r.get(0)?,
                page_id: r.get(1)?,
                block: r.get(2)?,
                page_path: r.get(3)?,
                state: r.get(4)?,
                digest: r.get(5)?,
            })
        })
        .map_err(backend)?
    {
        rows.anchors.push(row.map_err(backend)?);
    }
    let mut stmt = conn
        .prepare(
            "SELECT path, rule FROM knowledge_invalid WHERE wiki_id = ?1 AND tip = ?2 ORDER BY path",
        )
        .map_err(backend)?;
    for row in stmt
        .query_map(params![w, t], |r| {
            Ok(KnowledgeInvalidRow {
                path: r.get(0)?,
                rule: r.get(1)?,
            })
        })
        .map_err(backend)?
    {
        rows.invalid.push(row.map_err(backend)?);
    }
    Ok(rows)
}

fn backend_text(what: &str) -> crate::StoreError {
    crate::StoreError::Backend(what.to_owned())
}
