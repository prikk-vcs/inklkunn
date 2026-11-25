//! **Reading at a block that is not a tip** (RFC 006 D-8; INV-13; handoff G-T4, correction J-3): what a read of a place that is a
//! [`BlockPoint`] and not a [`Tip`] gives besides a file's text, so that none of it is read past the one reader
//! ([`ReadService::text_at`]).
//!
//! - **The pages a page includes** (transclusion) are read through the reader at the same block, each one checked. A page that
//!   includes a page whose text is hidden is a page of which part is hidden, and the permalink says so, with that tombstone, and
//!   gives none of it ([`ReadService::included_at`]).
//! - **The indexes at a base** are built from an extract of the whole block, which reads every file, so what each file says is
//!   asked of the reader afterwards, file by file, with one question of whether anything is hidden and one lookup of the block:
//!   an entry of the title index whose page a hidden change last wrote keeps its path and loses what was read from its text; a claim
//!   or a source whose file it last wrote is not in the knowledge index, which names it with the mark of the hiding
//!   ([`KnowledgeIndex::hidden_files`]), so that a surface says *hidden* in its place.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_store::{IndexEntry, WikiId, WikiRecord};

use super::index::TitleIndex;
use super::text::FileAt;
use super::writes::{Authorship, Reading, RequestBudget};
use super::{BlockPoint, ReadError, ReadService};
use crate::knowledge::index::{HiddenFile, HiddenKind, KnowledgeIndex};
use crate::knowledge::{ClaimId, SourceId, claim_path, source_path};
use crate::tombstone::Mark;

/// The problem an index entry says of a page whose text is hidden: it is findable (its path is a name the history shows) and nothing
/// is read from it.
pub const HIDDEN_PROBLEM: &str = "hidden";

/// How many title indexes at a base are held with what a hidden change wrote taken out.
const MAX_FILTERED_TITLES: usize = 16;

/// `(wiki, view, block, the newest event of the moderation log)`: what a filtered title index at a base was made under. The event
/// is what makes a hide or an unhide, which is one more event, a different key.
type FilteredKey = (WikiId, String, String, Option<String>);

/// The title indexes at a base with what a hidden change wrote taken out, newest last (correction L-10).
#[derive(Debug, Default)]
pub(super) struct FilteredTitles(Mutex<VecDeque<(FilteredKey, Arc<TitleIndex>)>>);

impl FilteredTitles {
    fn get(&self, key: &FilteredKey) -> Option<Arc<TitleIndex>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(held, _)| held == key)
            .map(|(_, index)| Arc::clone(index))
    }

    fn keep(&self, key: FilteredKey, index: Arc<TitleIndex>) {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        held.retain(|(k, _)| *k != key);
        held.push_back((key, index));
        while held.len() > MAX_FILTERED_TITLES {
            held.pop_front();
        }
    }
}

impl ReadService {
    /// The text of the page an embed names, **at a block that is not a tip**, as text: `Ok(None)` when it is not there or not text
    /// (the embed then stays a link); a text that moderation hides is `Ok(None)` too and is noted in `hidden`, so that the page that
    /// includes it says so instead of passing it off as a page with a link (see the module's account). A text that cannot be
    /// attributed is [`ReadError::NotCheckedYet`]: the page that includes it is not given either.
    pub(super) fn included_at(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
        hidden: &RefCell<Option<Box<Mark>>>,
        budget: &RequestBudget,
    ) -> Result<Option<String>, ReadError> {
        match self.file_at(
            wiki,
            view,
            block_id,
            path,
            self.config.page_max_bytes,
            budget,
        )? {
            FileAt::Held(bytes) => Ok(String::from_utf8(bytes).ok()),
            FileAt::Missing => Ok(None),
            FileAt::TooLarge(refused) => Err(self.storage_error(wiki, refused)),
            FileAt::Hidden(mark) => {
                hidden.borrow_mut().get_or_insert(mark);
                Ok(None)
            }
        }
    }

    /// Is the file at `path` of the block settled by `reading` hidden, by which hiding? **A text nobody is recorded as having written**
    /// (the entries of an index are of files that are there) **is [`ReadError::NotCheckedYet`]**, never hidden and never shown.
    fn hides(
        &self,
        wiki: &WikiRecord,
        reading: &Reading,
        path: &str,
    ) -> Result<Option<Mark>, ReadError> {
        Ok(match reading {
            Reading::NothingHidden => None,
            Reading::Ready(ready) => match self.authorship_in(wiki, ready, path)? {
                Authorship::NothingHidden | Authorship::Shown { written: true } => None,
                Authorship::Hidden(mark) => Some(*mark),
                Authorship::Shown { written: false } => return Err(ReadError::NotCheckedYet),
            },
        })
    }

    /// The title index at a block **without what a hidden change wrote**: the entries of pages whose text is hidden keep their path
    /// and nothing read from the text (its aliases, redirect, id and kind, and a title that is not the path's).
    ///
    /// **Held** for the base, with the newest event of the moderation log it was made under (correction L-10): a request after the
    /// first costs one query and no walk of the index, and a hide or an unhide, which is one more event, is never answered from what
    /// was held before it.
    pub(super) fn titles_without_hidden(
        &self,
        wiki: &WikiRecord,
        at: &BlockPoint,
        whole: Arc<TitleIndex>,
        budget: &RequestBudget,
    ) -> Result<Arc<TitleIndex>, ReadError> {
        let reading = self.reading(wiki, Some(at.view()), at.block_id(), budget)?;
        if matches!(reading, Reading::NothingHidden) {
            return Ok(whole);
        }
        // Read **before** the walk, so that a hide that lands during it makes the next request a miss.
        let key = (
            wiki.id.clone(),
            at.view().to_owned(),
            at.block_id().to_owned(),
            self.newest_moderation_event(wiki)?,
        );
        if let Some(held) = self.titles_filtered.get(&key) {
            return Ok(held);
        }
        #[cfg(test)]
        self.titles_filter_runs
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut changed = false;
        let mut entries: Vec<IndexEntry> = Vec::with_capacity(whole.entries().len());
        for entry in whole.entries() {
            if self.hides(wiki, &reading, &entry.path)?.is_some() {
                changed = true;
                entries.push(IndexEntry {
                    path: entry.path.clone(),
                    title: inklkunn_markup::path::path_to_title(&entry.path)
                        .unwrap_or_else(|| entry.path.clone()),
                    aliases: Vec::new(),
                    redirect: None,
                    page_id: None,
                    kind: "page".to_owned(),
                    problem: Some(HIDDEN_PROBLEM.to_owned()),
                });
            } else {
                entries.push(entry.clone());
            }
        }
        let filtered = if changed {
            Arc::new(TitleIndex::new(whole.tip().to_owned(), entries))
        } else {
            whole
        };
        self.titles_filtered.keep(key, Arc::clone(&filtered));
        Ok(filtered)
    }

    /// The newest event of the wiki's moderation log: what a held answer about what is hidden is keyed by.
    fn newest_moderation_event(&self, wiki: &WikiRecord) -> Result<Option<String>, ReadError> {
        let Some(tombstones) = &self.tombstones else {
            return Err(ReadError::CouldNotBeReadNow(
                super::Cause::ModerationNotConfigured,
            ));
        };
        tombstones
            .newest_event(&wiki.id)
            .map_err(|e| self.tombstone_failed(e))
    }

    /// The knowledge index at a block **without what a hidden change wrote**: the claims and sources a hidden change wrote (their
    /// rows, and the relations, links and places that are theirs, are not in it, and it names them,
    /// [`KnowledgeIndex::hidden_files`]), **and the places of a claim on a page whose text is hidden there** (correction L-5): an
    /// anchor says whether the paragraph is there and what its digest is, which is read from the page's text, and the frontmatter
    /// `id` that finds the page is read from it too, so every anchor of a page that is hidden, by the id it carries, is out.
    pub(super) fn knowledge_without_hidden(
        &self,
        wiki: &WikiRecord,
        at: &BlockPoint,
        whole: Arc<KnowledgeIndex>,
        titles: Arc<TitleIndex>,
        budget: &RequestBudget,
    ) -> Result<Arc<KnowledgeIndex>, ReadError> {
        let reading = self.reading(wiki, Some(at.view()), at.block_id(), budget)?;
        if matches!(reading, Reading::NothingHidden) {
            return Ok(whole);
        }
        let mut hidden: Vec<HiddenFile> = Vec::new();
        for claim in whole.claims() {
            let Some(id) = ClaimId::parse(&claim.id) else {
                continue;
            };
            if let Some(mark) = self.hides(wiki, &reading, &claim_path(&id))? {
                hidden.push(HiddenFile {
                    kind: HiddenKind::Claim,
                    id: claim.id.clone(),
                    mark,
                });
            }
        }
        for source in whole.sources() {
            let Some(id) = SourceId::parse(&source.id) else {
                continue;
            };
            if let Some(mark) = self.hides(wiki, &reading, &source_path(&id))? {
                hidden.push(HiddenFile {
                    kind: HiddenKind::Source,
                    id: source.id.clone(),
                    mark,
                });
            }
        }
        // The ids of the pages whose text is hidden here: the entries of the filtered title index that say so, by what the whole
        // index read from them.
        let filtered = self.titles_without_hidden(wiki, at, Arc::clone(&titles), budget)?;
        let hidden_pages: Vec<String> = titles
            .entries()
            .iter()
            .filter(|entry| {
                filtered
                    .entry(&entry.path)
                    .is_some_and(|e| e.problem.as_deref() == Some(HIDDEN_PROBLEM))
            })
            .filter_map(|entry| entry.page_id.clone())
            .collect();
        if hidden.is_empty() && hidden_pages.is_empty() {
            return Ok(whole);
        }
        Ok(Arc::new(whole.without(hidden, &hidden_pages)))
    }
}

#[cfg(test)]
mod tests;
