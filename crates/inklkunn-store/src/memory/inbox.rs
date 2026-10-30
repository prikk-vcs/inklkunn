//! The in-memory watches and inbox events: the reference the SQLite store must agree with.

use std::sync::{MutexGuard, PoisonError};

use crate::inbox::{InboxEvent, InboxStore, WatchRecord, WatchTarget};
use crate::record::{AccountId, InboxEventId, WikiId};
use crate::{Result, StoreError};

use super::MemoryStore;

/// Watches and events behind one lock.
#[derive(Debug, Default)]
pub(super) struct Inbox {
    watches: Vec<WatchRecord>,
    events: Vec<InboxEvent>,
}

impl MemoryStore {
    fn inbox_lock(&self) -> MutexGuard<'_, Inbox> {
        self.inbox.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl InboxStore for MemoryStore {
    fn watch(&self, watch: &WatchRecord) -> Result<bool> {
        if !self.wiki_exists(&watch.wiki) {
            return Err(StoreError::NotFound);
        }
        if !self.has_account(&watch.account) {
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.inbox_lock();
        if d.watches
            .iter()
            .any(|w| w.account == watch.account && w.wiki == watch.wiki && w.target == watch.target)
        {
            return Ok(false);
        }
        d.watches.push(watch.clone());
        Ok(true)
    }

    fn unwatch(&self, account: &AccountId, wiki: &WikiId, target: &WatchTarget) -> Result<bool> {
        let mut d = self.inbox_lock();
        let before = d.watches.len();
        d.watches
            .retain(|w| !(w.account == *account && w.wiki == *wiki && w.target == *target));
        Ok(d.watches.len() != before)
    }

    fn watches_of(&self, account: &AccountId) -> Result<Vec<WatchRecord>> {
        let d = self.inbox_lock();
        Ok(d.watches
            .iter()
            .filter(|w| w.account == *account)
            .cloned()
            .collect())
    }

    fn watches_in(&self, wiki: &WikiId) -> Result<Vec<WatchRecord>> {
        let d = self.inbox_lock();
        Ok(d.watches
            .iter()
            .filter(|w| w.wiki == *wiki)
            .cloned()
            .collect())
    }

    fn push_event(&self, event: &InboxEvent) -> Result<()> {
        if !self.wiki_exists(&event.wiki) {
            return Err(StoreError::NotFound);
        }
        let known = |a: &AccountId| self.has_account(a);
        if !known(&event.account) || event.actor.as_ref().is_some_and(|a| !known(a)) {
            return Err(StoreError::AccountNotFound);
        }
        if let Some(p) = &event.proposal
            && !self.proposal_exists(p)
        {
            return Err(StoreError::NotFound);
        }
        let mut d = self.inbox_lock();
        if d.events.iter().any(|e| e.id == event.id) {
            return Err(StoreError::IdTaken(event.id.to_string()));
        }
        d.events.push(event.clone());
        Ok(())
    }

    fn inbox(
        &self,
        account: &AccountId,
        unread_only: bool,
        limit: usize,
    ) -> Result<Vec<InboxEvent>> {
        let d = self.inbox_lock();
        Ok(d.events
            .iter()
            .rev()
            .filter(|e| e.account == *account && (!unread_only || e.read_at.is_none()))
            .take(limit)
            .cloned()
            .collect())
    }

    fn mark_read(&self, account: &AccountId, event: &InboxEventId, at: i64) -> Result<bool> {
        let mut d = self.inbox_lock();
        let Some(e) = d
            .events
            .iter_mut()
            .find(|e| e.id == *event && e.account == *account)
        else {
            return Ok(false);
        };
        e.read_at.get_or_insert(at);
        Ok(true)
    }
}
