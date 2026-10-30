//! The in-memory recovery flag, overlaps and system acts: the reference the SQLite store must agree with.

use std::sync::{MutexGuard, PoisonError};

use crate::acts::{
    ActStore, OverlapRecord, RecoveryFlag, SystemActEnd, SystemActEntry, SystemActRecord,
};
use crate::record::{ProposalId, WikiId};
use crate::{Result, StoreError};

use super::MemoryStore;

/// Everything act-shaped, behind one lock.
#[derive(Debug, Default)]
pub(super) struct Acts {
    flags: Vec<RecoveryFlag>,
    overlaps: Vec<OverlapRecord>,
    acts: Vec<SystemActRecord>,
    ends: Vec<SystemActEnd>,
}

impl MemoryStore {
    fn acts_lock(&self) -> MutexGuard<'_, Acts> {
        self.acts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ActStore for MemoryStore {
    fn recovery_flag(&self, wiki: &WikiId) -> Result<Option<RecoveryFlag>> {
        Ok(self
            .acts_lock()
            .flags
            .iter()
            .find(|f| f.wiki == *wiki)
            .cloned())
    }

    fn set_recovery_flag(&self, flag: &RecoveryFlag) -> Result<bool> {
        if !self.wiki_exists(&flag.wiki) {
            return Err(StoreError::NotFound);
        }
        let mut d = self.acts_lock();
        if d.flags.iter().any(|f| f.wiki == flag.wiki) {
            return Ok(false);
        }
        d.flags.push(flag.clone());
        Ok(true)
    }

    fn clear_recovery_flag(&self, wiki: &WikiId) -> Result<bool> {
        let mut d = self.acts_lock();
        let before = d.flags.len();
        d.flags.retain(|f| f.wiki != *wiki);
        Ok(d.flags.len() != before)
    }

    fn record_overlap(&self, overlap: &OverlapRecord) -> Result<()> {
        if !self.proposal_exists(&overlap.proposal)
            || !self.revision_exists(&overlap.proposal, overlap.revision)
        {
            return Err(StoreError::NotFound);
        }
        self.acts_lock().overlaps.push(overlap.clone());
        Ok(())
    }

    fn overlaps_of(&self, proposal: &ProposalId) -> Result<Vec<OverlapRecord>> {
        Ok(self
            .acts_lock()
            .overlaps
            .iter()
            .filter(|o| o.proposal == *proposal)
            .cloned()
            .collect())
    }

    fn begin_system_act(&self, act: &SystemActRecord) -> Result<()> {
        if !self.wiki_exists(&act.wiki) {
            return Err(StoreError::NotFound);
        }
        let mut d = self.acts_lock();
        if d.acts.iter().any(|a| a.id == act.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.acts.push(act.clone());
        Ok(())
    }

    fn finish_system_act(&self, end: &SystemActEnd) -> Result<()> {
        let mut d = self.acts_lock();
        if !d.acts.iter().any(|a| a.id == end.act) {
            return Err(StoreError::NotFound);
        }
        if d.ends.iter().any(|e| e.act == end.act) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.ends.push(end.clone());
        Ok(())
    }

    fn system_acts(&self, wiki: &WikiId) -> Result<Vec<SystemActEntry>> {
        let d = self.acts_lock();
        Ok(d.acts
            .iter()
            .filter(|a| a.wiki == *wiki)
            .map(|a| SystemActEntry {
                act: a.clone(),
                end: d.ends.iter().find(|e| e.act == a.id).cloned(),
            })
            .collect())
    }
}
