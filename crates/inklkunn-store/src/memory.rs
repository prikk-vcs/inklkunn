//! The in-memory store: tests, and the reference the SQLite store must agree with.

use std::sync::{Mutex, PoisonError};

use crate::record::{DisplayName, Slug, Verification, WikiId, WikiRecord};
use crate::{Result, StoreError, WikiStore};

/// Records in a mutex.
#[derive(Debug, Default)]
pub struct MemoryStore {
    records: Mutex<Vec<WikiRecord>>,
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
}
