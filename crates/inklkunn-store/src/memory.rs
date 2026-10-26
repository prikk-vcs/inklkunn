//! The in-memory store: tests, and the reference the SQLite store must agree with.

use std::sync::Mutex;

use crate::record::{NewWiki, Slug, Verification, WikiId, WikiRecord, now_secs};
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
        let mut records = self
            .records
            .lock()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        let Some(r) = records.iter_mut().find(|r| r.id == *id) else {
            return Err(StoreError::NotFound);
        };
        f(r)?;
        Ok(r.clone())
    }
}

impl WikiStore for MemoryStore {
    fn create(&self, new: NewWiki) -> Result<WikiRecord> {
        let mut records = self
            .records
            .lock()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        if records.iter().any(|r| r.slug == new.slug) {
            return Err(StoreError::SlugTaken(new.slug.to_string()));
        }
        let record = WikiRecord::from_new(WikiId::new(), new, now_secs());
        records.push(record.clone());
        Ok(record)
    }

    fn get(&self, id: &WikiId) -> Result<WikiRecord> {
        let records = self
            .records
            .lock()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        records
            .iter()
            .find(|r| r.id == *id)
            .cloned()
            .ok_or(StoreError::NotFound)
    }

    fn by_slug(&self, slug: &Slug) -> Result<WikiRecord> {
        let records = self
            .records
            .lock()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        records
            .iter()
            .find(|r| r.slug == *slug)
            .cloned()
            .ok_or(StoreError::NotFound)
    }

    fn list(&self) -> Result<Vec<WikiRecord>> {
        let records = self
            .records
            .lock()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        Ok(records.clone())
    }

    fn rename(
        &self,
        id: &WikiId,
        slug: Option<Slug>,
        display_name: Option<String>,
    ) -> Result<WikiRecord> {
        if let Some(s) = &slug {
            let records = self
                .records
                .lock()
                .map_err(|e| StoreError::Backend(e.to_string()))?;
            if records.iter().any(|r| r.slug == *s && r.id != *id) {
                return Err(StoreError::SlugTaken(s.to_string()));
            }
        }
        self.update(id, |r| {
            if let Some(s) = slug {
                r.slug = s;
            }
            if let Some(d) = display_name {
                r.display_name = d;
            }
            Ok(())
        })
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
