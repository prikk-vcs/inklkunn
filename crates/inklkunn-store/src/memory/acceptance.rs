//! The in-memory acceptance, intent and lease records: the reference the SQLite store must agree with.

use std::sync::{MutexGuard, PoisonError};

use crate::acceptance::{
    AcceptanceBegin, AcceptanceEntry, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult,
    AcceptanceStore, IntentCompletion, IntentEntry, IntentRecord, LeaseHolder, SealedAcceptance,
};
use crate::record::{AccountId, ProposalId, WikiId};
use crate::{Result, StoreError};

use super::MemoryStore;

/// Everything acceptance-shaped, behind one lock.
#[derive(Debug, Default)]
pub(super) struct Acceptances {
    acceptances: Vec<AcceptanceRecord>,
    results: Vec<AcceptanceResult>,
    intents: Vec<IntentRecord>,
    completions: Vec<IntentCompletion>,
    leases: Vec<LeaseHolder>,
}

impl Acceptances {
    fn entry(&self, a: &AcceptanceRecord) -> AcceptanceEntry {
        AcceptanceEntry {
            acceptance: a.clone(),
            result: self.results.iter().find(|r| r.acceptance == a.id).cloned(),
        }
    }

    fn intent_entry(&self, i: &IntentRecord) -> IntentEntry {
        IntentEntry {
            intent: i.clone(),
            completion: self.completions.iter().find(|c| c.intent == i.id).cloned(),
        }
    }
}

impl MemoryStore {
    fn acceptances_lock(&self) -> MutexGuard<'_, Acceptances> {
        self.acceptances
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl AcceptanceStore for MemoryStore {
    fn begin_acceptance(&self, acceptance: &AcceptanceRecord) -> Result<AcceptanceBegin> {
        if !self.wiki_exists(&acceptance.wiki)
            || !self.proposal_exists(&acceptance.proposal)
            || !self.revision_exists(&acceptance.proposal, acceptance.revision)
        {
            return Err(StoreError::NotFound);
        }
        if !self.has_account(&acceptance.acceptor) || !self.has_account(&acceptance.proposer) {
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.acceptances_lock();
        if d.acceptances.iter().any(|a| a.id == acceptance.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        // A pending or sealed acceptance of this revision stands in the way; a failed one does not.
        let standing = d
            .acceptances
            .iter()
            .filter(|a| a.proposal == acceptance.proposal && a.revision == acceptance.revision)
            .map(|a| d.entry(a))
            .find(|e| {
                !matches!(
                    e.result.as_ref().map(|r| &r.outcome),
                    Some(AcceptanceOutcome::Failed { .. })
                )
            });
        if let Some(e) = standing {
            return Ok(AcceptanceBegin::Existing(Box::new(e)));
        }
        d.acceptances.push(acceptance.clone());
        Ok(AcceptanceBegin::Started)
    }

    fn finish_acceptance(&self, result: &AcceptanceResult) -> Result<()> {
        let mut d = self.acceptances_lock();
        if !d.acceptances.iter().any(|a| a.id == result.acceptance) {
            return Err(StoreError::NotFound);
        }
        if d.results.iter().any(|r| r.acceptance == result.acceptance) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.results.push(result.clone());
        Ok(())
    }

    fn acceptances_of(&self, proposal: &ProposalId, revision: u32) -> Result<Vec<AcceptanceEntry>> {
        let d = self.acceptances_lock();
        Ok(d.acceptances
            .iter()
            .filter(|a| a.proposal == *proposal && a.revision == revision)
            .map(|a| d.entry(a))
            .collect())
    }

    fn sealed_acceptances(&self, wiki: &WikiId) -> Result<Vec<SealedAcceptance>> {
        let d = self.acceptances_lock();
        Ok(d.acceptances
            .iter()
            .filter(|a| a.wiki == *wiki)
            .filter_map(|a| {
                let result = d.results.iter().find(|r| r.acceptance == a.id)?;
                let AcceptanceOutcome::Sealed { block_id, patch_id } = &result.outcome else {
                    return None;
                };
                let intent = d.intents.iter().find(|i| i.acceptance == a.id)?;
                Some(SealedAcceptance {
                    acceptance: a.clone(),
                    intent: intent.clone(),
                    block_id: block_id.clone(),
                    patch_id: patch_id.clone(),
                })
            })
            .collect())
    }

    fn accepted_bytes_since(
        &self,
        wiki: &WikiId,
        proposer: &AccountId,
        since: i64,
    ) -> Result<Vec<(i64, u64)>> {
        let d = self.acceptances_lock();
        let mut out: Vec<(i64, u64)> = d
            .acceptances
            .iter()
            .filter(|a| a.wiki == *wiki && a.proposer == *proposer && a.created_at >= since)
            .filter(|a| {
                !d.results.iter().any(|r| {
                    r.acceptance == a.id && matches!(r.outcome, AcceptanceOutcome::Failed { .. })
                })
            })
            .map(|a| (a.created_at, a.bytes))
            .collect();
        out.sort_unstable();
        Ok(out)
    }

    fn record_intent(&self, intent: &IntentRecord) -> Result<()> {
        let mut d = self.acceptances_lock();
        if !d.acceptances.iter().any(|a| a.id == intent.acceptance) {
            return Err(StoreError::NotFound);
        }
        if d.intents
            .iter()
            .any(|i| i.id == intent.id || i.acceptance == intent.acceptance)
        {
            return Err(StoreError::AlreadyRecorded);
        }
        d.intents.push(intent.clone());
        Ok(())
    }

    fn complete_intent(&self, completion: &IntentCompletion) -> Result<()> {
        let mut d = self.acceptances_lock();
        if !d.intents.iter().any(|i| i.id == completion.intent) {
            return Err(StoreError::NotFound);
        }
        if d.completions.iter().any(|c| c.intent == completion.intent) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.completions.push(completion.clone());
        Ok(())
    }

    fn open_intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>> {
        Ok(self
            .intents(wiki)?
            .into_iter()
            .filter(|e| e.completion.is_none())
            .collect())
    }

    fn intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>> {
        let d = self.acceptances_lock();
        Ok(d.intents
            .iter()
            .filter(|i| i.wiki == *wiki)
            .map(|i| d.intent_entry(i))
            .collect())
    }

    fn lease_holder(&self, wiki: &WikiId) -> Result<Option<LeaseHolder>> {
        let d = self.acceptances_lock();
        Ok(d.leases.iter().find(|l| l.wiki == *wiki).cloned())
    }

    fn record_lease_holder(&self, holder: &LeaseHolder) -> Result<Option<LeaseHolder>> {
        if !self.wiki_exists(&holder.wiki) {
            return Err(StoreError::NotFound);
        }
        let mut d = self.acceptances_lock();
        if let Some(a) = &holder.acceptance
            && !d.acceptances.iter().any(|x| x.id == *a)
        {
            return Err(StoreError::NotFound);
        }
        let previous = d
            .leases
            .iter()
            .position(|l| l.wiki == holder.wiki)
            .map(|at| d.leases.remove(at));
        d.leases.push(holder.clone());
        Ok(previous)
    }

    fn release_lease_holder(&self, wiki: &WikiId, holder: &str) -> Result<bool> {
        let mut d = self.acceptances_lock();
        let before = d.leases.len();
        d.leases
            .retain(|l| !(l.wiki == *wiki && l.holder == holder));
        Ok(d.leases.len() != before)
    }
}
