//! The SQLite claim threads (schema version 18; handoff 005 K-T10). The table and its triggers are [`V18`](super::V18)'s;
//! the semantics are [`crate::claim_thread`]'s.

use rusqlite::{Row, TransactionBehavior, params};

use super::proposal::{count, probe, write_refusal};
use super::{SqliteStore, backend};
use crate::claim_thread::{
    ClaimCommentRecord, ClaimCommentRow, ClaimThreadPage, ClaimThreadStore, MAX_PAGE, well_formed,
};
use crate::record::{AccountId, CommentId, WikiId};
use crate::{Result, StoreError};

const COLUMNS: &str = "c.id, c.wiki_id, c.claim_id, c.seq, c.author, c.parent, c.body, c.created_at, \
                       (SELECT p.seq FROM claim_comments p WHERE p.id = c.parent)";

fn row_of(row: &Row<'_>) -> rusqlite::Result<ClaimCommentRow> {
    let bad = |what: &str| super::proposal::invalid(what);
    let text = |i: usize| row.get::<_, String>(i);
    let parent = row.get::<_, Option<String>>(5)?;
    Ok(ClaimCommentRow {
        position: u32::try_from(row.get::<_, i64>(3)?).map_err(|_| bad("position"))?,
        parent_position: row
            .get::<_, Option<i64>>(8)?
            .map(|p| u32::try_from(p).map_err(|_| bad("parent position")))
            .transpose()?,
        comment: ClaimCommentRecord {
            id: CommentId::parse(&text(0)?).ok_or_else(|| bad("comment id"))?,
            wiki: WikiId::parse(&text(1)?).ok_or_else(|| bad("wiki id"))?,
            claim: text(2)?,
            author: AccountId::parse(&text(4)?).ok_or_else(|| bad("account id"))?,
            parent: parent
                .map(|p| CommentId::parse(&p).ok_or_else(|| bad("comment id")))
                .transpose()?,
            body: text(6)?,
            created_at: row.get(7)?,
        },
    })
}

impl ClaimThreadStore for SqliteStore {
    fn add_claim_comment(&self, comment: &ClaimCommentRecord) -> Result<()> {
        if !well_formed(&comment.claim) {
            return Err(StoreError::Backend(
                "a claim thread is named by the claim's id".to_owned(),
            ));
        }
        if comment.body.is_empty() {
            return Err(StoreError::Backend(
                "a comment in a claim thread has a text".to_owned(),
            ));
        }
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM wikis WHERE id = ?1",
                &[comment.wiki.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            if !probe(
                &tx,
                "SELECT 1 FROM accounts WHERE id = ?1",
                &[comment.author.as_str()],
            )? {
                return Err(StoreError::AccountNotFound);
            }
            if let Some(parent) = &comment.parent
                && !probe(
                    &tx,
                    "SELECT 1 FROM claim_comments WHERE id = ?1 AND wiki_id = ?2 AND claim_id = ?3",
                    &[parent.as_str(), comment.wiki.as_str(), &comment.claim],
                )?
            {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO claim_comments (id, wiki_id, claim_id, seq, author, parent, body, created_at) \
                 VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(seq), 0) + 1 FROM claim_comments \
                 WHERE wiki_id = ?2 AND claim_id = ?3), ?4, ?5, ?6, ?7)",
                params![
                    comment.id.as_str(),
                    comment.wiki.as_str(),
                    comment.claim,
                    comment.author.as_str(),
                    comment.parent.as_ref().map(CommentId::as_str),
                    comment.body,
                    comment.created_at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn claim_comments(
        &self,
        wiki: &WikiId,
        claim: &str,
        after: Option<&CommentId>,
        limit: usize,
    ) -> Result<ClaimThreadPage> {
        let limit = limit.clamp(1, MAX_PAGE);
        self.with(|conn| {
            let total: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM claim_comments WHERE wiki_id = ?1 AND claim_id = ?2",
                    params![wiki.as_str(), claim],
                    |r| r.get(0),
                )
                .map_err(backend)?;
            let start: i64 = match after {
                None => 0,
                Some(id) => conn
                    .query_row(
                        "SELECT seq FROM claim_comments WHERE id = ?1 AND wiki_id = ?2 AND claim_id = ?3",
                        params![id.as_str(), wiki.as_str(), claim],
                        |r| r.get(0),
                    )
                    .map_err(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
                        other => backend(other),
                    })?,
            };
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {COLUMNS} FROM claim_comments c WHERE c.wiki_id = ?1 AND c.claim_id = ?2 \
                     AND c.seq > ?3 ORDER BY c.seq LIMIT ?4"
                ))
                .map_err(backend)?;
            // One more than the page, to know whether another follows.
            let fetch = i64::try_from(limit + 1).unwrap_or(i64::MAX);
            let mut rows = stmt
                .query_map(params![wiki.as_str(), claim, start, fetch], row_of)
                .map_err(backend)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(backend)?;
            let more = rows.len() > limit;
            rows.truncate(limit);
            let next = more
                .then(|| rows.last().map(|r| r.comment.id.clone()))
                .flatten();
            Ok(ClaimThreadPage {
                rows,
                total: count(total),
                next,
            })
        })
    }

    fn claim_comments_since(
        &self,
        wiki: &WikiId,
        author: &AccountId,
        since: i64,
    ) -> Result<Vec<i64>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT created_at FROM claim_comments \
                     WHERE wiki_id = ?1 AND author = ?2 AND created_at >= ?3 ORDER BY created_at",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str(), author.as_str(), since], |r| {
                    r.get::<_, i64>(0)
                })
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }

    fn claim_comments_in_wiki(&self, wiki: &WikiId) -> Result<Vec<ClaimCommentRow>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {COLUMNS} FROM claim_comments c WHERE c.wiki_id = ?1 ORDER BY c.claim_id, c.seq"
                ))
                .map_err(backend)?;
            let rows = stmt.query_map(params![wiki.as_str()], row_of).map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }
}
