//! The SQLite review record (schema version 16; handoff 005 K3 b). The tables and their triggers are
//! [`V16`](super::V16)'s; the semantics are [`crate::review`]'s. The trait methods that call these are in
//! [`super::proposal`], beside the proposal records.

use rusqlite::{Transaction, TransactionBehavior, params};

use super::proposal::{invalid, probe, write_refusal};
use super::{SqliteStore, backend};
use crate::record::{AcceptanceId, AccountId, DecisionId, ProposalId, WikiId};
use crate::review::{ClaimHow, ClaimOutcome, ClaimReview, check_claims};
use crate::{Result, StoreError};

/// The acceptance has no end yet: records are written while it is pending, never after.
fn pending(tx: &Transaction<'_>, id: &AcceptanceId) -> Result<()> {
    if !probe(
        tx,
        "SELECT 1 FROM acceptances WHERE id = ?1",
        &[id.as_str()],
    )? {
        return Err(StoreError::NotFound);
    }
    if probe(
        tx,
        "SELECT 1 FROM acceptance_results WHERE acceptance_id = ?1",
        &[id.as_str()],
    )? {
        return Err(StoreError::StateChanged);
    }
    Ok(())
}

impl SqliteStore {
    pub(super) fn claim_reviews_write(
        &self,
        acceptance: &AcceptanceId,
        outcomes: &[ClaimOutcome],
    ) -> Result<()> {
        check_claims(outcomes.iter().map(|o| o.claim.as_str()))?;
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            pending(&tx, acceptance)?;
            for o in outcomes {
                tx.execute(
                    "INSERT INTO change_claims (acceptance_id, claim, how) VALUES (?1, ?2, ?3)",
                    params![acceptance.as_str(), o.claim, o.how.as_str()],
                )
                .map_err(write_refusal)?;
            }
            tx.commit().map_err(backend)
        })
    }

    pub(super) fn reconfirmations_write(
        &self,
        by: &AcceptanceId,
        change: &AcceptanceId,
        claims: &[String],
    ) -> Result<()> {
        check_claims(claims.iter().map(String::as_str))?;
        if by == change {
            return Err(StoreError::Backend(
                "a change is not re-confirmed by itself".to_owned(),
            ));
        }
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            pending(&tx, by)?;
            // The change sealed, in the same wiki, and each claim was kept by it.
            if !probe(
                &tx,
                "SELECT 1 FROM acceptances a JOIN acceptances b ON b.wiki_id = a.wiki_id \
                 JOIN acceptance_results r ON r.acceptance_id = a.id AND r.outcome = 'sealed' \
                 WHERE a.id = ?1 AND b.id = ?2",
                &[change.as_str(), by.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            for claim in claims {
                if !probe(
                    &tx,
                    "SELECT 1 FROM change_claims WHERE acceptance_id = ?1 AND claim = ?2 AND how = 'kept'",
                    &[change.as_str(), claim],
                )? {
                    return Err(StoreError::NotFound);
                }
                tx.execute(
                    "INSERT INTO claim_reconfirmations (change_id, claim, by_id) VALUES (?1, ?2, ?3)",
                    params![change.as_str(), claim, by.as_str()],
                )
                .map_err(write_refusal)?;
            }
            tx.commit().map_err(backend)
        })
    }

    /// The records of sealed acceptances: of one change, or (with `None`) of every change in a wiki.
    fn claim_review_query(&self, filter: &str, arg: &str) -> Result<Vec<ClaimReview>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT c.acceptance_id, a.proposal_id, c.claim, c.how, \
                       (SELECT r.by_id FROM claim_reconfirmations r \
                          JOIN acceptance_results x ON x.acceptance_id = r.by_id AND x.outcome = 'sealed' \
                          WHERE r.change_id = c.acceptance_id AND r.claim = c.claim \
                          ORDER BY r.rowid LIMIT 1) \
                     FROM change_claims c \
                     JOIN acceptances a ON a.id = c.acceptance_id \
                     JOIN acceptance_results e ON e.acceptance_id = a.id AND e.outcome = 'sealed' \
                     WHERE {filter} ORDER BY a.seq, c.rowid"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![arg], |row| {
                    let acceptance: String = row.get(0)?;
                    let proposal: String = row.get(1)?;
                    let how: String = row.get(3)?;
                    let by: Option<String> = row.get(4)?;
                    Ok(ClaimReview {
                        acceptance: AcceptanceId::parse(&acceptance)
                            .ok_or_else(|| invalid("acceptance_id"))?,
                        proposal: ProposalId::parse(&proposal)
                            .ok_or_else(|| invalid("proposal_id"))?,
                        claim: row.get(2)?,
                        how: ClaimHow::parse(&how).ok_or_else(|| invalid("how"))?,
                        reconfirmed_by: by
                            .map(|b| AcceptanceId::parse(&b).ok_or_else(|| invalid("by_id")))
                            .transpose()?,
                    })
                })
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| StoreError::Backend("a stored review record could not be read".to_owned()))
        })
    }

    pub(super) fn claim_reviews_of_change(
        &self,
        change: &AcceptanceId,
    ) -> Result<Vec<ClaimReview>> {
        self.claim_review_query("a.id = ?1", change.as_str())
    }

    pub(super) fn claim_reviews_in_wiki(&self, wiki: &WikiId) -> Result<Vec<ClaimReview>> {
        self.claim_review_query("a.wiki_id = ?1", wiki.as_str())
    }

    pub(super) fn touching_decisions_in_wiki(&self, wiki: &WikiId) -> Result<Vec<DecisionId>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT d.id FROM proposal_decisions d JOIN proposals p ON p.id = d.proposal_id \
                     WHERE p.wiki_id = ?1 AND p.submitted_at IS NOT NULL \
                       AND EXISTS (SELECT 1 FROM decision_claims c WHERE c.decision_id = d.id) \
                     ORDER BY d.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], |row| {
                    let id: String = row.get(0)?;
                    DecisionId::parse(&id).ok_or_else(|| invalid("id"))
                })
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| StoreError::Backend("a stored decision could not be read".to_owned()))
        })
    }

    pub(super) fn reconfirm_of_row(
        &self,
        proposal: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Option<AcceptanceId>> {
        use rusqlite::OptionalExtension;
        self.with(|conn| {
            let found: Option<String> = conn
                .query_row(
                    "SELECT r.change_id FROM proposal_reconfirms r JOIN proposals p ON p.id = r.proposal_id \
                     WHERE r.proposal_id = ?1 AND (p.submitted_at IS NOT NULL OR p.author = ?2)",
                    params![proposal.as_str(), viewer.map(AccountId::as_str)],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            found
                .map(|f| {
                    AcceptanceId::parse(&f)
                        .ok_or_else(|| StoreError::Backend("a stored change id is not one".to_owned()))
                })
                .transpose()
        })
    }

    pub(super) fn reconfirmations_of_row(
        &self,
        change: &AcceptanceId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<ProposalId>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT r.proposal_id FROM proposal_reconfirms r JOIN proposals p ON p.id = r.proposal_id \
                     WHERE r.change_id = ?1 AND (p.submitted_at IS NOT NULL OR p.author = ?2) \
                     ORDER BY p.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(
                    params![change.as_str(), viewer.map(AccountId::as_str)],
                    |row| {
                        let id: String = row.get(0)?;
                        ProposalId::parse(&id).ok_or_else(|| invalid("proposal_id"))
                    },
                )
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| StoreError::Backend("a stored proposal id could not be read".to_owned()))
        })
    }
}
