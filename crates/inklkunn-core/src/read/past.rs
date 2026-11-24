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
use std::sync::Arc;

use inklkunn_store::{IndexEntry, WikiRecord};

use super::index::TitleIndex;
use super::text::FileAt;
use super::writes::{Authorship, Reading};
use super::{BlockPoint, ReadError, ReadService};
use crate::knowledge::index::{HiddenFile, HiddenKind, KnowledgeIndex};
use crate::knowledge::{ClaimId, SourceId, claim_path, source_path};
use crate::tombstone::Mark;

/// The problem an index entry says of a page whose text is hidden: it is findable (its path is a name the history shows) and nothing
/// is read from it.
pub const HIDDEN_PROBLEM: &str = "hidden";

impl ReadService {
    /// The text of the page an embed names, **at a block that is not a tip**, as text: `Ok(None)` when it is not there or not text
    /// (the embed then stays a link); a text that moderation hides is `Ok(None)` too and is noted in `hidden`, so that the page that
    /// includes it says so instead of passing it off as a page with a link (see the module's account).
    pub(super) fn included_at(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
        hidden: &RefCell<Option<Box<Mark>>>,
    ) -> Result<Option<String>, ReadError> {
        match self.file_at(wiki, view, block_id, path, self.config.page_max_bytes)? {
            FileAt::Held(bytes) => Ok(String::from_utf8(bytes).ok()),
            FileAt::Missing => Ok(None),
            FileAt::TooLarge(refused) => Err(self.storage_error(wiki, refused)),
            FileAt::Hidden(mark) => {
                hidden.borrow_mut().get_or_insert(mark);
                Ok(None)
            }
        }
    }

    /// Is the file at `path` of the block settled by `reading` hidden, by which hiding? (A text nobody is recorded as having written
    /// is hidden when the file is there; the entries of an index are of files that are.)
    fn hides(
        &self,
        wiki: &WikiRecord,
        reading: &Reading,
        path: &str,
    ) -> Result<Option<Mark>, ReadError> {
        Ok(match reading {
            Reading::NothingHidden => None,
            Reading::Unattributed(mark) => Some((**mark).clone()),
            Reading::Ready(ready) => match self.authorship_in(wiki, ready, path)? {
                Authorship::NothingHidden => None,
                Authorship::Hidden(mark) | Authorship::Unattributed(mark) => Some(*mark),
                Authorship::Shown { written: true, .. } => None,
                // The entry is of a file that is there, and nobody is recorded as having written it.
                Authorship::Shown {
                    written: false,
                    unattributed,
                } => Some(*unattributed),
            },
        })
    }

    /// The title index at a block **without what a hidden change wrote**: the entries of pages whose text is hidden keep their path
    /// and nothing read from the text (its aliases, redirect, id and kind, and a title that is not the path's).
    pub(super) fn titles_without_hidden(
        &self,
        wiki: &WikiRecord,
        at: &BlockPoint,
        whole: Arc<TitleIndex>,
    ) -> Result<Arc<TitleIndex>, ReadError> {
        let reading = self.reading(wiki, Some(at.view()), at.block_id())?;
        if matches!(reading, Reading::NothingHidden) {
            return Ok(whole);
        }
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
        if !changed {
            return Ok(whole);
        }
        Ok(Arc::new(TitleIndex::new(whole.tip().to_owned(), entries)))
    }

    /// The knowledge index at a block **without the claims and sources a hidden change wrote**: their rows, and the relations, links and
    /// places that are theirs, are not in it, and it names them ([`KnowledgeIndex::hidden_files`]).
    pub(super) fn knowledge_without_hidden(
        &self,
        wiki: &WikiRecord,
        at: &BlockPoint,
        whole: Arc<KnowledgeIndex>,
    ) -> Result<Arc<KnowledgeIndex>, ReadError> {
        let reading = self.reading(wiki, Some(at.view()), at.block_id())?;
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
        if hidden.is_empty() {
            return Ok(whole);
        }
        Ok(Arc::new(whole.without(hidden)))
    }
}
