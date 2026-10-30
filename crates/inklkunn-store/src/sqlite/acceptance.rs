//! The SQLite acceptance, intent and lease records (schema version 9). The write-once tables carry the
//! triggers of [`V9`](super::V9); the start of an acceptance is checked against the ones already there in the
//! same immediate transaction that writes it, so two retries cannot both start.

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use super::proposal::{count, invalid, probe, write_refusal};
use super::{SqliteStore, backend};
use crate::acceptance::{
    AcceptanceBegin, AcceptanceEntry, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult,
    AcceptanceStore, IntentCompletion, IntentEnd, IntentEntry, IntentRecord, LeaseHolder,
    SealedAcceptance,
};
use crate::access::Role;
use crate::record::{AcceptanceId, AccountId, IntentId, ProposalId, WikiId};
use crate::{Result, StoreError};

const ACCEPTANCE_COLUMNS: &str = "a.id, a.proposal_id, a.revision, a.wiki_id, a.view, a.acceptor, \
     a.acceptor_role, a.policy_version, a.proposer, a.digest, a.bytes, a.created_at, \
     r.outcome, r.block_id, r.patch_id, r.word, r.at";

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<AcceptanceEntry> {
    let id: String = row.get(0)?;
    let proposal: String = row.get(1)?;
    let wiki: String = row.get(3)?;
    let acceptor: String = row.get(5)?;
    let role: String = row.get(6)?;
    let proposer: String = row.get(8)?;
    let bytes: i64 = row.get(10)?;
    let outcome: Option<String> = row.get(12)?;
    let acceptance = AcceptanceId::parse(&id).ok_or_else(|| invalid("id"))?;
    let result = match outcome.as_deref() {
        None => None,
        Some("sealed") => Some(AcceptanceOutcome::Sealed {
            block_id: row.get(13)?,
            patch_id: row.get(14)?,
        }),
        Some("failed") => Some(AcceptanceOutcome::Failed { word: row.get(15)? }),
        Some(_) => return Err(invalid("outcome")),
    }
    .map(|outcome| -> rusqlite::Result<AcceptanceResult> {
        Ok(AcceptanceResult {
            acceptance: acceptance.clone(),
            outcome,
            at: row.get(16)?,
        })
    })
    .transpose()?;
    Ok(AcceptanceEntry {
        acceptance: AcceptanceRecord {
            id: acceptance,
            proposal: ProposalId::parse(&proposal).ok_or_else(|| invalid("proposal_id"))?,
            revision: row.get(2)?,
            wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
            view: row.get(4)?,
            acceptor: AccountId::parse(&acceptor).ok_or_else(|| invalid("acceptor"))?,
            acceptor_role: Role::parse(&role).ok_or_else(|| invalid("acceptor_role"))?,
            policy_version: row.get(7)?,
            proposer: AccountId::parse(&proposer).ok_or_else(|| invalid("proposer"))?,
            digest: row.get(9)?,
            bytes: u64::try_from(bytes).map_err(|_| invalid("bytes"))?,
            created_at: row.get(11)?,
        },
        result,
    })
}

fn entries_of(
    conn: &Connection,
    proposal: &ProposalId,
    revision: u32,
) -> Result<Vec<AcceptanceEntry>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {ACCEPTANCE_COLUMNS} FROM acceptances a \
             LEFT JOIN acceptance_results r ON r.acceptance_id = a.id \
             WHERE a.proposal_id = ?1 AND a.revision = ?2 ORDER BY a.seq"
        ))
        .map_err(backend)?;
    let rows = stmt
        .query_map(params![proposal.as_str(), revision], row_to_entry)
        .map_err(backend)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| StoreError::Backend("a stored acceptance could not be read".to_owned()))
}

fn row_to_intent(row: &Row<'_>) -> rusqlite::Result<IntentEntry> {
    let id: String = row.get(0)?;
    let acceptance: String = row.get(1)?;
    let wiki: String = row.get(2)?;
    let how: Option<String> = row.get(6)?;
    let intent = IntentId::parse(&id).ok_or_else(|| invalid("id"))?;
    let completion = match how {
        None => None,
        Some(how) => Some(IntentCompletion {
            intent: intent.clone(),
            end: IntentEnd::parse(&how).ok_or_else(|| invalid("how"))?,
            at: row.get(7)?,
        }),
    };
    Ok(IntentEntry {
        intent: IntentRecord {
            id: intent,
            acceptance: AcceptanceId::parse(&acceptance).ok_or_else(|| invalid("acceptance_id"))?,
            wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
            view: row.get(3)?,
            base_block: row.get(4)?,
            message_digest: row.get(5)?,
            message: row.get(9)?,
            created_at: row.get(8)?,
        },
        completion,
    })
}

const INTENT_SELECT: &str = "SELECT i.id, i.acceptance_id, i.wiki_id, i.view, i.base_block, \
     i.message_digest, c.how, c.at, i.created_at, i.message FROM intents i \
     LEFT JOIN intent_completions c ON c.intent_id = i.id";

fn intents_where(conn: &Connection, clause: &str, wiki: &WikiId) -> Result<Vec<IntentEntry>> {
    let mut stmt = conn
        .prepare(&format!(
            "{INTENT_SELECT} WHERE i.wiki_id = ?1 {clause} ORDER BY i.seq"
        ))
        .map_err(backend)?;
    let rows = stmt
        .query_map(params![wiki.as_str()], row_to_intent)
        .map_err(backend)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| StoreError::Backend("a stored intent could not be read".to_owned()))
}

impl AcceptanceStore for SqliteStore {
    fn begin_acceptance(&self, a: &AcceptanceRecord) -> Result<AcceptanceBegin> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            // The revision is looked up by its number as text: a column of integer affinity compares it as a number.
            let known = probe(&tx, "SELECT 1 FROM wikis WHERE id = ?1", &[a.wiki.as_str()])?
                && Self::proposal_known(&tx, &a.proposal)?
                && probe(
                    &tx,
                    "SELECT 1 FROM proposal_revisions WHERE proposal_id = ?1 AND seq = ?2",
                    &[a.proposal.as_str(), &a.revision.to_string()],
                )?;
            if !known {
                return Err(StoreError::NotFound);
            }
            for who in [&a.acceptor, &a.proposer] {
                if !probe(&tx, "SELECT 1 FROM accounts WHERE id = ?1", &[who.as_str()])? {
                    return Err(StoreError::AccountNotFound);
                }
            }
            if probe(
                &tx,
                "SELECT 1 FROM acceptances WHERE id = ?1",
                &[a.id.as_str()],
            )? {
                return Err(StoreError::AlreadyRecorded);
            }
            // A pending or sealed acceptance of this revision stands in the way; a failed one does not.
            let standing = entries_of(&tx, &a.proposal, a.revision)?
                .into_iter()
                .find(|e| {
                    !matches!(
                        e.result.as_ref().map(|r| &r.outcome),
                        Some(AcceptanceOutcome::Failed { .. })
                    )
                });
            if let Some(e) = standing {
                return Ok(AcceptanceBegin::Existing(Box::new(e)));
            }
            tx.execute(
                "INSERT INTO acceptances (id, seq, proposal_id, revision, wiki_id, view, acceptor, \
                 acceptor_role, policy_version, proposer, digest, bytes, created_at) \
                 VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM acceptances), ?2, ?3, ?4, ?5, ?6, \
                 ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    a.id.as_str(),
                    a.proposal.as_str(),
                    a.revision,
                    a.wiki.as_str(),
                    a.view,
                    a.acceptor.as_str(),
                    a.acceptor_role.as_str(),
                    a.policy_version,
                    a.proposer.as_str(),
                    a.digest,
                    i64::try_from(a.bytes).unwrap_or(i64::MAX),
                    a.created_at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)?;
            Ok(AcceptanceBegin::Started)
        })
    }

    fn finish_acceptance(&self, result: &AcceptanceResult) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM acceptances WHERE id = ?1",
                &[result.acceptance.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            let (outcome, block, patch, word) = match &result.outcome {
                AcceptanceOutcome::Sealed { block_id, patch_id } => {
                    ("sealed", Some(block_id), Some(patch_id), None)
                }
                AcceptanceOutcome::Failed { word } => ("failed", None, None, Some(word)),
            };
            tx.execute(
                "INSERT INTO acceptance_results (acceptance_id, outcome, block_id, patch_id, word, at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![result.acceptance.as_str(), outcome, block, patch, word, result.at],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn acceptances_of(&self, proposal: &ProposalId, revision: u32) -> Result<Vec<AcceptanceEntry>> {
        self.with(|conn| entries_of(conn, proposal, revision))
    }

    fn pending_acceptance(
        &self,
        id: &AcceptanceId,
    ) -> Result<Option<(AcceptanceRecord, IntentRecord)>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    &format!(
                        "SELECT {ACCEPTANCE_COLUMNS} FROM acceptances a \
                         LEFT JOIN acceptance_results r ON r.acceptance_id = a.id \
                         WHERE a.id = ?1 AND r.acceptance_id IS NULL"
                    ),
                    params![id.as_str()],
                    row_to_entry,
                )
                .optional()
                .map_err(backend)?;
            let Some(entry) = found else {
                return Ok(None);
            };
            let intent = conn
                .query_row(
                    &format!("{INTENT_SELECT} WHERE i.acceptance_id = ?1 AND c.intent_id IS NULL"),
                    params![id.as_str()],
                    row_to_intent,
                )
                .optional()
                .map_err(backend)?;
            Ok(intent.map(|i| (entry.acceptance, i.intent)))
        })
    }

    fn sealed_acceptances(&self, wiki: &WikiId) -> Result<Vec<SealedAcceptance>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {ACCEPTANCE_COLUMNS} FROM acceptances a \
                     JOIN acceptance_results r ON r.acceptance_id = a.id \
                     WHERE a.wiki_id = ?1 AND r.outcome = 'sealed' ORDER BY a.seq"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_entry)
                .map_err(backend)?;
            let entries = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| {
                StoreError::Backend("a stored acceptance could not be read".to_owned())
            })?;
            let mut out = Vec::with_capacity(entries.len());
            for entry in entries {
                let Some(AcceptanceResult {
                    outcome: AcceptanceOutcome::Sealed { block_id, patch_id },
                    ..
                }) = entry.result
                else {
                    continue;
                };
                let intent = conn
                    .query_row(
                        &format!("{INTENT_SELECT} WHERE i.acceptance_id = ?1"),
                        params![entry.acceptance.id.as_str()],
                        row_to_intent,
                    )
                    .optional()
                    .map_err(backend)?;
                if let Some(intent) = intent {
                    out.push(SealedAcceptance {
                        acceptance: entry.acceptance,
                        intent: intent.intent,
                        block_id,
                        patch_id,
                    });
                }
            }
            Ok(out)
        })
    }

    fn accepted_bytes_since(
        &self,
        wiki: &WikiId,
        proposer: &AccountId,
        since: i64,
    ) -> Result<Vec<(i64, u64)>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT a.created_at, a.bytes FROM acceptances a \
                     LEFT JOIN acceptance_results r ON r.acceptance_id = a.id \
                     WHERE a.wiki_id = ?1 AND a.proposer = ?2 AND a.created_at >= ?3 \
                     AND COALESCE(r.outcome, 'pending') <> 'failed' ORDER BY a.created_at, a.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str(), proposer.as_str(), since], |r| {
                    Ok((r.get::<_, i64>(0)?, count(r.get::<_, i64>(1)?)))
                })
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }

    fn record_intent(&self, intent: &IntentRecord) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM acceptances WHERE id = ?1",
                &[intent.acceptance.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO intents (id, seq, acceptance_id, wiki_id, view, base_block, message_digest, \
                 created_at, message) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM intents), ?2, \
                 ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    intent.id.as_str(),
                    intent.acceptance.as_str(),
                    intent.wiki.as_str(),
                    intent.view,
                    intent.base_block,
                    intent.message_digest,
                    intent.created_at,
                    intent.message
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn complete_intent(&self, completion: &IntentCompletion) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM intents WHERE id = ?1",
                &[completion.intent.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO intent_completions (intent_id, how, at) VALUES (?1, ?2, ?3)",
                params![
                    completion.intent.as_str(),
                    completion.end.as_str(),
                    completion.at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn open_intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>> {
        self.with(|conn| intents_where(conn, "AND c.intent_id IS NULL", wiki))
    }

    fn intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>> {
        self.with(|conn| intents_where(conn, "", wiki))
    }

    fn lease_holder(&self, wiki: &WikiId) -> Result<Option<LeaseHolder>> {
        self.with(|conn| {
            conn.query_row(
                "SELECT wiki_id, holder, purpose, acceptance_id, since FROM lease_holders \
                 WHERE wiki_id = ?1",
                params![wiki.as_str()],
                row_to_lease,
            )
            .optional()
            .map_err(backend)
        })
    }

    fn record_lease_holder(&self, holder: &LeaseHolder) -> Result<Option<LeaseHolder>> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM wikis WHERE id = ?1",
                &[holder.wiki.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            let previous = tx
                .query_row(
                    "SELECT wiki_id, holder, purpose, acceptance_id, since FROM lease_holders \
                     WHERE wiki_id = ?1",
                    params![holder.wiki.as_str()],
                    row_to_lease,
                )
                .optional()
                .map_err(backend)?;
            tx.execute(
                "INSERT INTO lease_holders (wiki_id, holder, purpose, acceptance_id, since) \
                 VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (wiki_id) DO UPDATE SET \
                 holder = excluded.holder, purpose = excluded.purpose, \
                 acceptance_id = excluded.acceptance_id, since = excluded.since",
                params![
                    holder.wiki.as_str(),
                    holder.holder,
                    holder.purpose,
                    holder.acceptance.as_ref().map(AcceptanceId::as_str),
                    holder.since
                ],
            )
            .map_err(|e| {
                if super::is_foreign_key_refusal(&e) {
                    StoreError::NotFound
                } else {
                    backend(e)
                }
            })?;
            tx.commit().map_err(backend)?;
            Ok(previous)
        })
    }

    fn release_lease_holder(&self, wiki: &WikiId, holder: &str) -> Result<bool> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "DELETE FROM lease_holders WHERE wiki_id = ?1 AND holder = ?2",
                    params![wiki.as_str(), holder],
                )
                .map_err(backend)?;
            Ok(n > 0)
        })
    }
}

fn row_to_lease(row: &Row<'_>) -> rusqlite::Result<LeaseHolder> {
    let wiki: String = row.get(0)?;
    let acceptance: Option<String> = row.get(3)?;
    Ok(LeaseHolder {
        wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
        holder: row.get(1)?,
        purpose: row.get(2)?,
        acceptance: acceptance
            .map(|s| AcceptanceId::parse(&s).ok_or_else(|| invalid("acceptance_id")))
            .transpose()?,
        since: row.get(4)?,
    })
}
