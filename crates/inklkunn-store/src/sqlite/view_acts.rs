//! The SQLite record of a view's creation (schema version 16; handoff 005 K3). The tables and their triggers are
//! [`V16`](super::V16)'s: written once, the name held by one act that stands, checked here inside the `IMMEDIATE`
//! transaction (so that the refusal is the plain `AlreadyRecorded`) and by a trigger from any connection. The
//! [`ActStore`](crate::ActStore) methods that call these are in [`super::acts`], beside the other acts.

use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use super::proposal::{invalid, probe, write_refusal};
use super::{SqliteStore, backend};
use crate::record::{AccountId, ViewActId, WikiId};
use crate::views::{
    ViewActEnd, ViewActEntry, ViewActOutcome, ViewActRecord, ViewKind, is_view_name,
};
use crate::{Result, StoreError};

const COLUMNS: &str = "a.id, a.wiki_id, a.name, a.forked_from, a.from_block, a.kind, a.created_by, \
                       a.created_at, e.outcome, e.word, e.at";

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<ViewActEntry> {
    let id: String = row.get(0)?;
    let wiki: String = row.get(1)?;
    let kind: String = row.get(5)?;
    let by: String = row.get(6)?;
    let outcome: Option<String> = row.get(8)?;
    let act = ViewActId::parse(&id).ok_or_else(|| invalid("id"))?;
    let end = match outcome.as_deref() {
        None => None,
        Some("created") => Some(ViewActOutcome::Created),
        Some("failed") => Some(ViewActOutcome::Failed { word: row.get(9)? }),
        Some(_) => return Err(invalid("outcome")),
    }
    .map(|outcome| -> rusqlite::Result<ViewActEnd> {
        Ok(ViewActEnd {
            act: act.clone(),
            outcome,
            at: row.get(10)?,
        })
    })
    .transpose()?;
    Ok(ViewActEntry {
        act: ViewActRecord {
            id: act,
            wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
            name: row.get(2)?,
            forked_from: row.get(3)?,
            from_block: row.get(4)?,
            kind: ViewKind::parse(&kind).ok_or_else(|| invalid("kind"))?,
            created_by: AccountId::parse(&by).ok_or_else(|| invalid("created_by"))?,
            created_at: row.get(7)?,
        },
        end,
    })
}

impl SqliteStore {
    /// Does `name` belong to an act that stands in the wiki (one that is open or created)?
    fn view_name_held(tx: &rusqlite::Transaction<'_>, wiki: &WikiId, name: &str) -> Result<bool> {
        probe(
            tx,
            "SELECT 1 FROM view_acts a LEFT JOIN view_act_ends e ON e.act_id = a.id \
             WHERE a.wiki_id = ?1 AND a.name = ?2 AND (e.act_id IS NULL OR e.outcome = 'created')",
            &[wiki.as_str(), name],
        )
    }

    pub(super) fn begin_view_act_row(&self, act: &ViewActRecord) -> Result<()> {
        if !is_view_name(&act.name) {
            return Err(StoreError::Backend(
                "a view act names its view in the form a view's name has".to_owned(),
            ));
        }
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM wikis WHERE id = ?1",
                &[act.wiki.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            if !probe(
                &tx,
                "SELECT 1 FROM accounts WHERE id = ?1",
                &[act.created_by.as_str()],
            )? {
                return Err(StoreError::AccountNotFound);
            }
            if probe(
                &tx,
                "SELECT 1 FROM view_acts WHERE id = ?1",
                &[act.id.as_str()],
            )? || Self::view_name_held(&tx, &act.wiki, &act.name)?
            {
                return Err(StoreError::AlreadyRecorded);
            }
            tx.execute(
                "INSERT INTO view_acts (id, seq, wiki_id, name, forked_from, from_block, kind, created_by, \
                 created_at) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM view_acts), ?2, ?3, ?4, ?5, \
                 ?6, ?7, ?8)",
                params![
                    act.id.as_str(),
                    act.wiki.as_str(),
                    act.name,
                    act.forked_from,
                    act.from_block,
                    act.kind.as_str(),
                    act.created_by.as_str(),
                    act.created_at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    pub(super) fn finish_view_act_row(&self, end: &ViewActEnd) -> Result<()> {
        let (outcome, word) = match &end.outcome {
            ViewActOutcome::Created => ("created", None),
            ViewActOutcome::Failed { word } => ("failed", Some(word)),
        };
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM view_acts WHERE id = ?1",
                &[end.act.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO view_act_ends (act_id, outcome, word, at) VALUES (?1, ?2, ?3, ?4)",
                params![end.act.as_str(), outcome, word, end.at],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    pub(super) fn open_view_act_row(&self, id: &ViewActId) -> Result<Option<ViewActRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    &format!(
                        "SELECT {COLUMNS} FROM view_acts a \
                         LEFT JOIN view_act_ends e ON e.act_id = a.id \
                         WHERE a.id = ?1 AND e.act_id IS NULL"
                    ),
                    params![id.as_str()],
                    row_to_entry,
                )
                .optional()
                .map_err(backend)?;
            Ok(found.map(|e| e.act))
        })
    }

    pub(super) fn view_act_rows(&self, wiki: &WikiId) -> Result<Vec<ViewActEntry>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {COLUMNS} FROM view_acts a \
                     LEFT JOIN view_act_ends e ON e.act_id = a.id \
                     WHERE a.wiki_id = ?1 ORDER BY a.seq"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_entry)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| StoreError::Backend("a stored view act could not be read".to_owned()))
        })
    }
}
