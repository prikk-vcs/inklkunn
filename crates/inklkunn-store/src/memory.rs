//! The in-memory store: tests, and the reference the SQLite store must agree with.

mod derived;
mod identity;

use std::sync::{Mutex, PoisonError};

use crate::access::{Role, RoleGrant, RoleStore};
use crate::record::{AccountId, DisplayName, PrikkVersion, Slug, Verification, WikiId, WikiRecord};
use crate::{Result, StoreError, WikiStore};

/// Records in a mutex.
#[derive(Debug, Default)]
pub struct MemoryStore {
    records: Mutex<Vec<WikiRecord>>,
    grants: Mutex<Vec<RoleGrant>>,
    identities: Mutex<identity::Identities>,
    derived: Mutex<derived::Derived>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn update(
        &self,
        id: &WikiId,
        f: impl FnOnce(&mut WikiRecord) -> Result<()>,
    ) -> Result<WikiRecord> {
        let mut records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(r) = records.iter_mut().find(|r| r.id == *id) else {
            return Err(StoreError::NotFound);
        };
        f(r)?;
        Ok(r.clone())
    }
}

impl WikiStore for MemoryStore {
    fn insert(&self, record: WikiRecord) -> Result<WikiRecord> {
        let mut records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        if records.iter().any(|r| r.id == record.id) {
            return Err(StoreError::IdTaken(record.id.to_string()));
        }
        if records.iter().any(|r| r.slug == record.slug) {
            return Err(StoreError::SlugTaken(record.slug.to_string()));
        }
        records.push(record.clone());
        Ok(record)
    }

    fn get(&self, id: &WikiId) -> Result<WikiRecord> {
        let records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        records
            .iter()
            .find(|r| r.id == *id)
            .cloned()
            .ok_or(StoreError::NotFound)
    }

    fn by_slug(&self, slug: &Slug) -> Result<WikiRecord> {
        let records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        records
            .iter()
            .find(|r| r.slug == *slug)
            .cloned()
            .ok_or(StoreError::NotFound)
    }

    fn list(&self) -> Result<Vec<WikiRecord>> {
        let records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(records.clone())
    }

    fn rename(
        &self,
        id: &WikiId,
        slug: Option<Slug>,
        display_name: Option<DisplayName>,
    ) -> Result<WikiRecord> {
        let mut records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(s) = &slug
            && records.iter().any(|r| r.slug == *s && r.id != *id)
        {
            return Err(StoreError::SlugTaken(s.to_string()));
        }
        let Some(r) = records.iter_mut().find(|r| r.id == *id) else {
            return Err(StoreError::NotFound);
        };
        if let Some(s) = slug {
            r.slug = s;
        }
        if let Some(d) = display_name {
            r.display_name = d;
        }
        Ok(r.clone())
    }

    fn set_verification(&self, id: &WikiId, verification: Verification) -> Result<WikiRecord> {
        self.update(id, |r| {
            r.verification = verification;
            Ok(())
        })
    }

    fn set_keys(
        &self,
        id: &WikiId,
        author_key_id: Option<String>,
        maintainer_key_id: Option<String>,
        prikk_format_version: Option<String>,
    ) -> Result<WikiRecord> {
        self.update(id, |r| {
            r.author_key_id = author_key_id;
            r.maintainer_key_id = maintainer_key_id;
            r.prikk_format_version = prikk_format_version;
            Ok(())
        })
    }

    fn request_checkpoint_refresh(&self, id: &WikiId, at: i64) -> Result<WikiRecord> {
        self.update(id, |r| {
            r.checkpoint.refresh_requested_at.get_or_insert(at);
            Ok(())
        })
    }

    fn record_checkpoint_refresh(&self, id: &WikiId, prikk: &PrikkVersion) -> Result<WikiRecord> {
        self.update(id, |r| {
            r.checkpoint.refreshed_with = Some(prikk.clone());
            r.checkpoint.refresh_requested_at = None;
            Ok(())
        })
    }
}

impl RoleStore for MemoryStore {
    fn grant(&self, grant: RoleGrant) -> Result<()> {
        // The wiki must exist; the check and the write are one critical section over both lists.
        let records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
        if !records.iter().any(|r| r.id == grant.wiki) {
            return Err(StoreError::NotFound);
        }
        // The account, and the account that grants, must exist (the SQLite store's foreign keys).
        if !self.has_account(&grant.account)
            || grant
                .granted_by
                .as_ref()
                .is_some_and(|by| !self.has_account(by))
        {
            return Err(StoreError::AccountNotFound);
        }
        let mut grants = self.grants.lock().unwrap_or_else(PoisonError::into_inner);
        // A grant replaces the account's earlier one in this wiki and counts as the latest, like the SQLite
        // store's `seq`: grants are listed in the order each was last made.
        grants.retain(|g| !(g.account == grant.account && g.wiki == grant.wiki));
        grants.push(grant);
        Ok(())
    }

    fn revoke(&self, account: &AccountId, wiki: &WikiId) -> Result<bool> {
        let mut grants = self.grants.lock().unwrap_or_else(PoisonError::into_inner);
        let before = grants.len();
        grants.retain(|g| !(g.account == *account && g.wiki == *wiki));
        Ok(grants.len() != before)
    }

    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>> {
        let grants = self.grants.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(grants
            .iter()
            .find(|g| g.account == *account && g.wiki == *wiki)
            .map(|g| g.role))
    }

    fn grants_in(&self, wiki: &WikiId) -> Result<Vec<RoleGrant>> {
        let grants = self.grants.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(grants.iter().filter(|g| g.wiki == *wiki).cloned().collect())
    }
}
