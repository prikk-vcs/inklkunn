//! The SQLite recovery flag, overlaps, system acts and recovery acts (schema versions 11 and 12). The overlaps, the
//! system acts, the recovery acts and their ends carry the write-once triggers of [`V11`](super::V11) and
//! [`V12`](super::V12), as the acceptance records do.

use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use super::proposal::{count, from_json, invalid, probe, to_json, write_refusal};
use super::{SqliteStore, backend};
use crate::acceptance::AcceptanceOutcome;
use crate::acts::{
    ActStore, MAX_RECOVERY_PLAN_BYTES, OverlapRecord, OverlapText, RecoveryActEnd,
    RecoveryActEntry, RecoveryActRecord, RecoveryEnding, RecoveryFlag, RecoveryReason,
    SystemActEnd, SystemActEntry, SystemActKind, SystemActRecord,
};
use crate::record::{ProposalId, RecoveryActId, SystemActId, WikiId};
use crate::{Result, StoreError};

fn row_to_flag(row: &Row<'_>) -> rusqlite::Result<RecoveryFlag> {
    let wiki: String = row.get(0)?;
    let reason: String = row.get(1)?;
    Ok(RecoveryFlag {
        wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
        reason: RecoveryReason::parse(&reason).ok_or_else(|| invalid("reason"))?,
        act: row.get(2)?,
        since: row.get(3)?,
    })
}

const ACT_COLUMNS: &str = "a.id, a.wiki_id, a.view, a.kind, a.base_block, a.message, \
     a.message_digest, a.digest, a.bytes, a.created_at, e.outcome, e.block_id, e.patch_id, e.word, e.at";

fn row_to_act(row: &Row<'_>) -> rusqlite::Result<SystemActEntry> {
    let id: String = row.get(0)?;
    let wiki: String = row.get(1)?;
    let kind: String = row.get(3)?;
    let bytes: i64 = row.get(8)?;
    let outcome: Option<String> = row.get(10)?;
    let act = SystemActId::parse(&id).ok_or_else(|| invalid("id"))?;
    let end = match outcome.as_deref() {
        None => None,
        Some("sealed") => Some(AcceptanceOutcome::Sealed {
            block_id: row.get(11)?,
            patch_id: row.get(12)?,
        }),
        Some("failed") => Some(AcceptanceOutcome::Failed { word: row.get(13)? }),
        Some(_) => return Err(invalid("outcome")),
    }
    .map(|outcome| -> rusqlite::Result<SystemActEnd> {
        Ok(SystemActEnd {
            act: act.clone(),
            outcome,
            at: row.get(14)?,
        })
    })
    .transpose()?;
    Ok(SystemActEntry {
        act: SystemActRecord {
            id: act,
            wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
            view: row.get(2)?,
            kind: SystemActKind::parse(&kind).ok_or_else(|| invalid("kind"))?,
            base_block: row.get(4)?,
            message: row.get(5)?,
            message_digest: row.get(6)?,
            digest: row.get(7)?,
            bytes: count(bytes),
            created_at: row.get(9)?,
        },
        end,
    })
}

const RECOVERY_COLUMNS: &str = "a.id, a.wiki_id, a.plan, a.created_at, e.ending, e.at";

fn row_to_recovery(row: &Row<'_>) -> rusqlite::Result<RecoveryActEntry> {
    let id: String = row.get(0)?;
    let wiki: String = row.get(1)?;
    let ending: Option<String> = row.get(4)?;
    let act = RecoveryActId::parse(&id).ok_or_else(|| invalid("id"))?;
    let end = match ending {
        None => None,
        Some(e) => Some(RecoveryActEnd {
            act: act.clone(),
            ending: RecoveryEnding::parse(&e).ok_or_else(|| invalid("ending"))?,
            at: row.get(5)?,
        }),
    };
    Ok(RecoveryActEntry {
        act: RecoveryActRecord {
            id: act,
            wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
            plan: row.get(2)?,
            created_at: row.get(3)?,
        },
        end,
    })
}

impl ActStore for SqliteStore {
    fn recovery_flag(&self, wiki: &WikiId) -> Result<Option<RecoveryFlag>> {
        self.with(|conn| {
            conn.query_row(
                "SELECT wiki_id, reason, act, since FROM wiki_flags WHERE wiki_id = ?1",
                params![wiki.as_str()],
                row_to_flag,
            )
            .optional()
            .map_err(backend)
        })
    }

    fn set_recovery_flag(&self, flag: &RecoveryFlag) -> Result<bool> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM wikis WHERE id = ?1",
                &[flag.wiki.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            let added = tx
                .execute(
                    "INSERT OR IGNORE INTO wiki_flags (wiki_id, reason, act, since) VALUES (?1, ?2, ?3, ?4)",
                    params![flag.wiki.as_str(), flag.reason.as_str(), flag.act, flag.since],
                )
                .map_err(backend)?;
            tx.commit().map_err(backend)?;
            Ok(added > 0)
        })
    }

    fn clear_recovery_flag(&self, wiki: &WikiId) -> Result<bool> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "DELETE FROM wiki_flags WHERE wiki_id = ?1",
                    params![wiki.as_str()],
                )
                .map_err(backend)?;
            Ok(n > 0)
        })
    }

    fn record_overlap(&self, overlap: &OverlapRecord) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM proposal_revisions WHERE proposal_id = ?1 AND seq = ?2",
                &[overlap.proposal.as_str(), &overlap.revision.to_string()],
            )? {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO proposal_overlaps (proposal_id, revision, tip_block, texts, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    overlap.proposal.as_str(),
                    overlap.revision,
                    overlap.tip_block,
                    to_json(&overlap.texts)?,
                    overlap.created_at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn overlaps_of(&self, proposal: &ProposalId) -> Result<Vec<OverlapRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT revision, tip_block, texts, created_at FROM proposal_overlaps \
                     WHERE proposal_id = ?1 ORDER BY seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![proposal.as_str()], |row| {
                    let texts: String = row.get(2)?;
                    Ok(OverlapRecord {
                        proposal: proposal.clone(),
                        revision: row.get(0)?,
                        tip_block: row.get(1)?,
                        texts: from_json::<Vec<OverlapText>>(&texts, "texts")?,
                        created_at: row.get(3)?,
                    })
                })
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| StoreError::Backend("a stored overlap could not be read".to_owned()))
        })
    }

    fn begin_system_act(&self, act: &SystemActRecord) -> Result<()> {
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
            tx.execute(
                "INSERT INTO system_acts (id, seq, wiki_id, view, kind, base_block, message, \
                 message_digest, digest, bytes, created_at) VALUES (?1, \
                 (SELECT COALESCE(MAX(seq), 0) + 1 FROM system_acts), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    act.id.as_str(),
                    act.wiki.as_str(),
                    act.view,
                    act.kind.as_str(),
                    act.base_block,
                    act.message,
                    act.message_digest,
                    act.digest,
                    i64::try_from(act.bytes).unwrap_or(i64::MAX),
                    act.created_at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn finish_system_act(&self, end: &SystemActEnd) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM system_acts WHERE id = ?1",
                &[end.act.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            let (outcome, block, patch, word) = match &end.outcome {
                AcceptanceOutcome::Sealed { block_id, patch_id } => {
                    ("sealed", Some(block_id), Some(patch_id), None)
                }
                AcceptanceOutcome::Failed { word } => ("failed", None, None, Some(word)),
            };
            tx.execute(
                "INSERT INTO system_act_ends (act_id, outcome, block_id, patch_id, word, at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![end.act.as_str(), outcome, block, patch, word, end.at],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn open_system_act(&self, id: &SystemActId) -> Result<Option<SystemActRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    &format!(
                        "SELECT {ACT_COLUMNS} FROM system_acts a \
                         LEFT JOIN system_act_ends e ON e.act_id = a.id \
                         WHERE a.id = ?1 AND e.act_id IS NULL"
                    ),
                    params![id.as_str()],
                    row_to_act,
                )
                .optional()
                .map_err(backend)?;
            Ok(found.map(|e| e.act))
        })
    }

    fn system_acts(&self, wiki: &WikiId) -> Result<Vec<SystemActEntry>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {ACT_COLUMNS} FROM system_acts a \
                     LEFT JOIN system_act_ends e ON e.act_id = a.id \
                     WHERE a.wiki_id = ?1 ORDER BY a.seq"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_act)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| {
                StoreError::Backend("a stored system act could not be read".to_owned())
            })
        })
    }

    fn begin_recovery_act(&self, act: &RecoveryActRecord) -> Result<()> {
        if act.plan.len() > MAX_RECOVERY_PLAN_BYTES {
            return Err(StoreError::Backend(format!(
                "a recovery plan is longer than {MAX_RECOVERY_PLAN_BYTES} bytes"
            )));
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
            tx.execute(
                "INSERT INTO recovery_acts (id, seq, wiki_id, plan, created_at) VALUES (?1, \
                 (SELECT COALESCE(MAX(seq), 0) + 1 FROM recovery_acts), ?2, ?3, ?4)",
                params![act.id.as_str(), act.wiki.as_str(), act.plan, act.created_at],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn finish_recovery_act(&self, end: &RecoveryActEnd) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM recovery_acts WHERE id = ?1",
                &[end.act.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO recovery_act_ends (act_id, ending, at) VALUES (?1, ?2, ?3)",
                params![end.act.as_str(), end.ending.as_str(), end.at],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn open_recovery_act(&self, id: &RecoveryActId) -> Result<Option<RecoveryActRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    &format!(
                        "SELECT {RECOVERY_COLUMNS} FROM recovery_acts a \
                         LEFT JOIN recovery_act_ends e ON e.act_id = a.id \
                         WHERE a.id = ?1 AND e.act_id IS NULL"
                    ),
                    params![id.as_str()],
                    row_to_recovery,
                )
                .optional()
                .map_err(backend)?;
            Ok(found.map(|e| e.act))
        })
    }

    fn recovery_acts(&self, wiki: &WikiId) -> Result<Vec<RecoveryActEntry>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {RECOVERY_COLUMNS} FROM recovery_acts a \
                     LEFT JOIN recovery_act_ends e ON e.act_id = a.id \
                     WHERE a.wiki_id = ?1 ORDER BY a.seq"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_recovery)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| {
                StoreError::Backend("a stored recovery act could not be read".to_owned())
            })
        })
    }
}
