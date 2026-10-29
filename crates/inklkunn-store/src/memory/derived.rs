//! The in-memory derived cache: the reference the SQLite store must agree with.

use std::collections::{BTreeMap, HashMap};
use std::sync::PoisonError;

use crate::Result;
use crate::derived::{DerivedPage, DerivedStore, IndexEntry, PageKey, SearchDoc, SearchHit};
use crate::record::WikiId;

use super::MemoryStore;

/// `(wiki, tip, path, lang, presentation)`.
type PageId = (WikiId, String, String, String, String);

/// Everything derived, behind one lock so that a replacement is one step.
#[derive(Debug, Default)]
pub(super) struct Derived {
    pages: HashMap<PageId, DerivedPage>,
    /// `(wiki, tip)` → the complete index, by path.
    indexes: HashMap<(WikiId, String), BTreeMap<String, IndexEntry>>,
    /// `(wiki, tip)` → the complete search index, in the order stored.
    searches: HashMap<(WikiId, String), Vec<SearchDoc>>,
    /// `(wiki, block)` → when the block was first seen. Kept when the cache is cleared.
    seen: HashMap<(WikiId, String), i64>,
}

fn id_of(key: &PageKey<'_>) -> PageId {
    (
        key.wiki.clone(),
        key.tip.to_owned(),
        key.path.to_owned(),
        key.lang.to_owned(),
        key.presentation.to_owned(),
    )
}

impl MemoryStore {
    fn derived(&self) -> std::sync::MutexGuard<'_, Derived> {
        self.derived.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl DerivedStore for MemoryStore {
    fn page(&self, key: &PageKey<'_>) -> Result<Option<DerivedPage>> {
        Ok(self.derived().pages.get(&id_of(key)).cloned())
    }

    fn put_page(&self, key: &PageKey<'_>, page: &DerivedPage) -> Result<()> {
        self.derived().pages.insert(id_of(key), page.clone());
        Ok(())
    }

    fn title_index(&self, wiki: &WikiId, tip: &str) -> Result<Option<Vec<IndexEntry>>> {
        Ok(self
            .derived()
            .indexes
            .get(&(wiki.clone(), tip.to_owned()))
            .map(|entries| entries.values().cloned().collect()))
    }

    fn put_title_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        entries: &[IndexEntry],
        _built_at: i64,
    ) -> Result<()> {
        let by_path = entries
            .iter()
            .map(|e| (e.path.clone(), e.clone()))
            .collect::<BTreeMap<_, _>>();
        self.derived()
            .indexes
            .insert((wiki.clone(), tip.to_owned()), by_path);
        Ok(())
    }

    fn retain_tips(&self, wiki: &WikiId, keep: &[String]) -> Result<u64> {
        let mut d = self.derived();
        let keeps = |tip: &String| keep.contains(tip);
        let before = d.pages.len() + d.indexes.len() + d.searches.len();
        d.pages.retain(|(w, tip, ..), _| w != wiki || keeps(tip));
        d.indexes.retain(|(w, tip), _| w != wiki || keeps(tip));
        d.searches.retain(|(w, tip), _| w != wiki || keeps(tip));
        let removed = before - (d.pages.len() + d.indexes.len() + d.searches.len());
        Ok(u64::try_from(removed).unwrap_or(u64::MAX))
    }

    fn clear(&self, wiki: &WikiId) -> Result<u64> {
        let mut d = self.derived();
        let before = d.pages.len() + d.indexes.len() + d.searches.len();
        d.pages.retain(|(w, ..), _| w != wiki);
        d.indexes.retain(|(w, _), _| w != wiki);
        d.searches.retain(|(w, _), _| w != wiki);
        let removed = before - (d.pages.len() + d.indexes.len() + d.searches.len());
        Ok(u64::try_from(removed).unwrap_or(u64::MAX))
    }

    fn put_search_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        docs: &[SearchDoc],
        _built_at: i64,
    ) -> Result<()> {
        self.derived()
            .searches
            .insert((wiki.clone(), tip.to_owned()), docs.to_vec());
        Ok(())
    }

    fn search(
        &self,
        wiki: &WikiId,
        tip: &str,
        folded_query: &str,
        limit: usize,
    ) -> Result<Option<Vec<SearchHit>>> {
        Ok(self
            .derived()
            .searches
            .get(&(wiki.clone(), tip.to_owned()))
            .map(|docs| {
                docs.iter()
                    .filter(|d| d.folded.contains(folded_query))
                    .take(limit)
                    .map(|d| SearchHit {
                        path: d.path.clone(),
                        kind: d.kind,
                        text: d.text.clone(),
                    })
                    .collect()
            }))
    }

    fn first_seen(&self, wiki: &WikiId, blocks: &[String], now: i64) -> Result<Vec<i64>> {
        let mut d = self.derived();
        Ok(blocks
            .iter()
            .map(|b| *d.seen.entry((wiki.clone(), b.clone())).or_insert(now))
            .collect())
    }
}
