//! The in-memory recovery flag, overlaps and system acts: the reference the SQLite store must agree with.

use std::sync::{MutexGuard, PoisonError};

use crate::acts::{
    ActStore, MAX_RECOVERY_PLAN_BYTES, OverlapRecord, RecoveryActEnd, RecoveryActEntry,
    RecoveryActRecord, RecoveryFlag, SystemActEnd, SystemActEntry, SystemActRecord,
};
use crate::record::{ProposalId, RecoveryActId, SystemActId, ViewActId, WikiId, WikiRecord};
use crate::views::{ViewActEnd, ViewActEntry, ViewActOutcome, ViewActRecord, is_view_name};
use crate::{Result, StoreError};

use super::MemoryStore;

/// Everything act-shaped, behind one lock.
#[derive(Debug, Default)]
pub(super) struct Acts {
    flags: Vec<RecoveryFlag>,
    overlaps: Vec<OverlapRecord>,
    acts: Vec<SystemActRecord>,
    ends: Vec<SystemActEnd>,
    recoveries: Vec<RecoveryActRecord>,
    recovery_ends: Vec<RecoveryActEnd>,
    view_acts: Vec<ViewActRecord>,
    view_ends: Vec<ViewActEnd>,
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

    fn open_system_act(&self, id: &SystemActId) -> Result<Option<SystemActRecord>> {
        let d = self.acts_lock();
        if d.ends.iter().any(|e| e.act == *id) {
            return Ok(None);
        }
        Ok(d.acts.iter().find(|a| a.id == *id).cloned())
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

    fn insert_wiki_with_creation_act(
        &self,
        record: WikiRecord,
        begin: &SystemActRecord,
        end: &SystemActEnd,
    ) -> Result<WikiRecord> {
        if begin.wiki != record.id || end.act != begin.id {
            return Err(StoreError::Backend(
                "the creation act is not of this wiki".to_owned(),
            ));
        }
        // Both locks, records first (nothing takes them the other way round), held until both are written.
        let mut records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        let mut d = self.acts_lock();
        if records.iter().any(|r| r.id == record.id) {
            return Err(StoreError::IdTaken(record.id.to_string()));
        }
        if records.iter().any(|r| r.slug == record.slug) {
            return Err(StoreError::SlugTaken(record.slug.to_string()));
        }
        if d.acts.iter().any(|a| a.id == begin.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        records.push(record.clone());
        d.acts.push(begin.clone());
        d.ends.push(end.clone());
        Ok(record)
    }

    fn begin_recovery_act(&self, act: &RecoveryActRecord) -> Result<()> {
        if act.plan.len() > MAX_RECOVERY_PLAN_BYTES {
            return Err(StoreError::Backend(format!(
                "a recovery plan is longer than {MAX_RECOVERY_PLAN_BYTES} bytes"
            )));
        }
        if !self.wiki_exists(&act.wiki) {
            return Err(StoreError::NotFound);
        }
        let mut d = self.acts_lock();
        if d.recoveries.iter().any(|a| a.id == act.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.recoveries.push(act.clone());
        Ok(())
    }

    fn finish_recovery_act(&self, end: &RecoveryActEnd) -> Result<()> {
        let mut d = self.acts_lock();
        if !d.recoveries.iter().any(|a| a.id == end.act) {
            return Err(StoreError::NotFound);
        }
        if d.recovery_ends.iter().any(|e| e.act == end.act) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.recovery_ends.push(end.clone());
        Ok(())
    }

    fn open_recovery_act(&self, id: &RecoveryActId) -> Result<Option<RecoveryActRecord>> {
        let d = self.acts_lock();
        if d.recovery_ends.iter().any(|e| e.act == *id) {
            return Ok(None);
        }
        Ok(d.recoveries.iter().find(|a| a.id == *id).cloned())
    }

    fn recovery_acts(&self, wiki: &WikiId) -> Result<Vec<RecoveryActEntry>> {
        let d = self.acts_lock();
        Ok(d.recoveries
            .iter()
            .filter(|a| a.wiki == *wiki)
            .map(|a| RecoveryActEntry {
                act: a.clone(),
                end: d.recovery_ends.iter().find(|e| e.act == a.id).cloned(),
            })
            .collect())
    }

    fn keeps_view_acts(&self) -> bool {
        true
    }

    fn begin_view_act(&self, act: &ViewActRecord) -> Result<()> {
        if !is_view_name(&act.name) {
            return Err(StoreError::Backend(
                "a view act names its view in the form a view's name has".to_owned(),
            ));
        }
        if !self.wiki_exists(&act.wiki) {
            return Err(StoreError::NotFound);
        }
        if !self.has_account(&act.created_by) {
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.acts_lock();
        if d.view_acts.iter().any(|a| a.id == act.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        // The name belongs to the first act that stands: one that failed took nothing in and frees it.
        let held = d
            .view_acts
            .iter()
            .filter(|a| a.wiki == act.wiki && a.name == act.name)
            .any(|a| {
                !matches!(
                    d.view_ends
                        .iter()
                        .find(|e| e.act == a.id)
                        .map(|e| &e.outcome),
                    Some(ViewActOutcome::Failed { .. })
                )
            });
        if held {
            return Err(StoreError::AlreadyRecorded);
        }
        d.view_acts.push(act.clone());
        Ok(())
    }

    fn finish_view_act(&self, end: &ViewActEnd) -> Result<()> {
        if matches!(&end.outcome, ViewActOutcome::Failed { word } if word.is_empty()) {
            return Err(StoreError::Backend(
                "a failed view act carries the word of its failure".to_owned(),
            ));
        }
        let mut d = self.acts_lock();
        if !d.view_acts.iter().any(|a| a.id == end.act) {
            return Err(StoreError::NotFound);
        }
        if d.view_ends.iter().any(|e| e.act == end.act) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.view_ends.push(end.clone());
        Ok(())
    }

    fn open_view_act(&self, id: &ViewActId) -> Result<Option<ViewActRecord>> {
        let d = self.acts_lock();
        if d.view_ends.iter().any(|e| e.act == *id) {
            return Ok(None);
        }
        Ok(d.view_acts.iter().find(|a| a.id == *id).cloned())
    }

    fn view_acts(&self, wiki: &WikiId) -> Result<Vec<ViewActEntry>> {
        let d = self.acts_lock();
        Ok(d.view_acts
            .iter()
            .filter(|a| a.wiki == *wiki)
            .map(|a| ViewActEntry {
                act: a.clone(),
                end: d.view_ends.iter().find(|e| e.act == a.id).cloned(),
            })
            .collect())
    }
}
