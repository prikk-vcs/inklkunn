//! The SQLite watches and inbox events (schema version 9).

use rusqlite::{Row, TransactionBehavior, params};

use super::proposal::{invalid, probe};
use super::{SqliteStore, backend};
use crate::inbox::{InboxEvent, InboxEventKind, InboxStore, WatchRecord, WatchTarget};
use crate::record::{AccountId, InboxEventId, ProposalId, WikiId};
use crate::{Result, StoreError};

fn row_to_watch(row: &Row<'_>) -> rusqlite::Result<WatchRecord> {
    let account: String = row.get(0)?;
    let wiki: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let target: String = row.get(3)?;
    Ok(WatchRecord {
        account: AccountId::parse(&account).ok_or_else(|| invalid("account_id"))?,
        wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
        target: WatchTarget::from_parts(&kind, &target).ok_or_else(|| invalid("kind"))?,
        created_at: row.get(4)?,
    })
}

fn row_to_event(row: &Row<'_>) -> rusqlite::Result<InboxEvent> {
    let id: String = row.get(0)?;
    let account: String = row.get(1)?;
    let wiki: String = row.get(2)?;
    let proposal: Option<String> = row.get(3)?;
    let kind: String = row.get(4)?;
    let actor: Option<String> = row.get(5)?;
    Ok(InboxEvent {
        id: InboxEventId::parse(&id).ok_or_else(|| invalid("id"))?,
        account: AccountId::parse(&account).ok_or_else(|| invalid("account_id"))?,
        wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
        proposal: proposal
            .map(|s| ProposalId::parse(&s).ok_or_else(|| invalid("proposal_id")))
            .transpose()?,
        kind: InboxEventKind::parse(&kind).ok_or_else(|| invalid("kind"))?,
        actor: actor
            .map(|s| AccountId::parse(&s).ok_or_else(|| invalid("actor")))
            .transpose()?,
        created_at: row.get(6)?,
        read_at: row.get(7)?,
    })
}

fn unreadable(what: &str) -> StoreError {
    StoreError::Backend(format!("a stored {what} could not be read"))
}

impl InboxStore for SqliteStore {
    fn watch(&self, watch: &WatchRecord) -> Result<bool> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM wikis WHERE id = ?1",
                &[watch.wiki.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            if !probe(
                &tx,
                "SELECT 1 FROM accounts WHERE id = ?1",
                &[watch.account.as_str()],
            )? {
                return Err(StoreError::AccountNotFound);
            }
            let (kind, target) = watch.target.parts();
            let n = tx
                .execute(
                    "INSERT OR IGNORE INTO watches (account_id, wiki_id, kind, target, created_at, seq) \
                     VALUES (?1, ?2, ?3, ?4, ?5, (SELECT COALESCE(MAX(seq), 0) + 1 FROM watches))",
                    params![
                        watch.account.as_str(),
                        watch.wiki.as_str(),
                        kind,
                        target,
                        watch.created_at
                    ],
                )
                .map_err(backend)?;
            tx.commit().map_err(backend)?;
            Ok(n > 0)
        })
    }

    fn unwatch(&self, account: &AccountId, wiki: &WikiId, target: &WatchTarget) -> Result<bool> {
        let (kind, target) = target.parts();
        self.with(|conn| {
            let n = conn
                .execute(
                    "DELETE FROM watches WHERE account_id = ?1 AND wiki_id = ?2 AND kind = ?3 \
                     AND target = ?4",
                    params![account.as_str(), wiki.as_str(), kind, target],
                )
                .map_err(backend)?;
            Ok(n > 0)
        })
    }

    fn watches_of(&self, account: &AccountId) -> Result<Vec<WatchRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT account_id, wiki_id, kind, target, created_at FROM watches \
                     WHERE account_id = ?1 ORDER BY seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![account.as_str()], row_to_watch)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("watch"))
        })
    }

    fn watches_in(&self, wiki: &WikiId) -> Result<Vec<WatchRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT account_id, wiki_id, kind, target, created_at FROM watches \
                     WHERE wiki_id = ?1 ORDER BY seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_watch)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("watch"))
        })
    }

    fn push_event(&self, event: &InboxEvent) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM wikis WHERE id = ?1",
                &[event.wiki.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            for who in std::iter::once(&event.account).chain(event.actor.as_ref()) {
                if !probe(&tx, "SELECT 1 FROM accounts WHERE id = ?1", &[who.as_str()])? {
                    return Err(StoreError::AccountNotFound);
                }
            }
            if let Some(p) = &event.proposal
                && !Self::proposal_known(&tx, p)?
            {
                return Err(StoreError::NotFound);
            }
            if probe(
                &tx,
                "SELECT 1 FROM inbox_events WHERE id = ?1",
                &[event.id.as_str()],
            )? {
                return Err(StoreError::IdTaken(event.id.to_string()));
            }
            tx.execute(
                "INSERT INTO inbox_events (id, seq, account_id, wiki_id, proposal_id, kind, actor, \
                 created_at, read_at) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM inbox_events), \
                 ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    event.id.as_str(),
                    event.account.as_str(),
                    event.wiki.as_str(),
                    event.proposal.as_ref().map(ProposalId::as_str),
                    event.kind.as_str(),
                    event.actor.as_ref().map(AccountId::as_str),
                    event.created_at,
                    event.read_at
                ],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn inbox(
        &self,
        account: &AccountId,
        unread_only: bool,
        limit: usize,
    ) -> Result<Vec<InboxEvent>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, account_id, wiki_id, proposal_id, kind, actor, created_at, read_at \
                     FROM inbox_events WHERE account_id = ?1 AND (?2 = 0 OR read_at IS NULL) \
                     ORDER BY seq DESC LIMIT ?3",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![account.as_str(), unread_only, limit], row_to_event)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("inbox event"))
        })
    }

    fn mark_read(&self, account: &AccountId, event: &InboxEventId, at: i64) -> Result<bool> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE inbox_events SET read_at = COALESCE(read_at, ?1) \
                     WHERE id = ?2 AND account_id = ?3",
                    params![at, event.as_str(), account.as_str()],
                )
                .map_err(backend)?;
            Ok(n > 0)
        })
    }
}
