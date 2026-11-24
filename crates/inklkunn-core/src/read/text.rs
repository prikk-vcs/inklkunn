//! **The one reader of a file's text at a block** (RFC 006 D-8; INV-13; handoff G-T4, corrections H-1 and J-1 – J-3). Every read of a
//! page's or a knowledge file's text at a block passes here, so that hidden text is hidden wherever it is read: the permalink, the
//! pages it includes, both sides of a diff, a proposal's base, a revise, a rollback, *Why is this here?* and the sources a change
//! attached.
//!
//! ```text
//! (view, block, path) ─▶ any change of the wiki with its content hidden? ─▶ none: read the file at the block
//!                                                                          ─▶ some: who last wrote the path at or before the block?
//!                        that change has its content hidden: hidden (its tombstone)      not: read the file
//! ```
//!
//! **What is hidden is decided by who wrote the text, never by what it says** (J-1; the rule, what *wrote* means for a path in
//! prikk's block model, and what is done with every case that cannot be settled, are the account of [`super::writes`]): the text of
//! a path at a block is hidden exactly when the change that last wrote the path at or before the block has its content hidden,
//! following a pure rename back to the change that wrote the text it moved. B writes personal data to a page, C edits another page,
//! D removes the data: at C the page still reads as B wrote it, and hiding B hides it there. Bytes another change wrote
//! independently are not hidden, and **no bound answers *not hidden***. The decision is a lookup in the index of who wrote each
//! path (J-2), the same for every number of changes and of hidden changes.
//!
//! **Private on purpose.** This module is the only one that asks the storage for the bytes of a file at a block (the write
//! path's own reads in `acceptance` and `knowledge::curation` are the two other places, listed in the gate below). The gate is a
//! test over the crate's sources ([`tests`]): a call to the storage's `read` anywhere else fails it, and so does a call of
//! `materialize`, `with_tree`, `with_block_tree`, `diff` or a `Tip {` literal outside the places that may make it, each with its
//! reason (J-3). The entry points that do not check say in their names what they read and why it may be unchecked:
//!
//! - [`ReadService::bytes_at_tip`]: **a view's tip**, which only [`ReadService::tip`] makes. The content of a view's tip cannot be
//!   hidden (D-8), and the tip check (RFC 006 D-8; `tipcheck`) is what keeps it so;
//! - [`ReadService::rules_bytes_at`]: **a governance file**, which is the rules and not a revision's text;
//! - [`ReadService::bytes_unchecked_at`]: the tip check's comparison and the one audited act that shows hidden content;
//! - [`ReadService::file_for_trace`]: *Why is this here?* aligns one version of a page with the next and says which change wrote a
//!   paragraph, never what it said (a hidden change is still a change of the page's history, with its tombstone);
//! - the verdict of whether a rollback would merge, which is a boolean.
//!
//! **Cost.** A wiki in which nothing has had its content hidden pays one store query for a read, nothing else. A wiki in which
//! something has pays a few indexed queries and no call to prikk, **whatever the history and the number of changes hidden**.

use inklkunn_prikk::{Point, StorageError};
use inklkunn_store::WikiRecord;

use super::writes::Authorship;
use super::{ReadError, ReadService, Tip};
use crate::merge::{PageText, merge};
use crate::tombstone::Mark;

/// What the text of a file at a block is, as a reader may be given it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockText {
    /// The file's text.
    Text(String),
    /// There was no such file at the block.
    Missing,
    /// The file is not text (not UTF-8).
    NotText,
    /// The file is larger than a page may be.
    TooLarge,
    /// The text is hidden by moderation (RFC 006 D-8): the mark of the hiding of the change that wrote it. **Nothing of the
    /// text is given.**
    Hidden(Box<Mark>),
}

/// What reading the bytes of a file at a block gave.
pub(super) enum FileAt {
    /// The bytes.
    Held(Vec<u8>),
    /// There was no such file at the block.
    Missing,
    /// The file is larger than the bound asked for (the refusal the storage gave).
    TooLarge(StorageError),
    /// The text is hidden by moderation.
    Hidden(Box<Mark>),
}

/// What the tip check and the audited reveal read: the bytes, unchecked.
pub(super) enum Unchecked {
    /// The bytes.
    Held(Vec<u8>),
    /// There was no such file at the point.
    Missing,
    /// The file is larger than a page may be.
    Large,
}

impl ReadService {
    /// The text of the file at `path` at the block `block_id`, **unless it is hidden**: the one function the text of a page
    /// at a block is read through (see the module's account). `view` is the view whose log orders the blocks (the default
    /// view for `None`).
    pub fn text_at(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
    ) -> Result<BlockText, ReadError> {
        Ok(
            match self.file_at(wiki, view, block_id, path, self.config.page_max_bytes)? {
                FileAt::Held(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => BlockText::Text(text),
                    Err(_) => BlockText::NotText,
                },
                FileAt::Missing => BlockText::Missing,
                FileAt::TooLarge(_) => BlockText::TooLarge,
                FileAt::Hidden(mark) => BlockText::Hidden(mark),
            },
        )
    }

    /// The file at `path` at the block, as bytes of at most `max` (the bound the caller reads a file by), **unless its text is
    /// hidden**.
    pub(super) fn file_at(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
        max: u64,
    ) -> Result<FileAt, ReadError> {
        // What is there and whose it is are two questions; the second is asked first, so that a hidden text is never read.
        let unattributed = match self.authorship(wiki, view, block_id, path)? {
            Authorship::Hidden(mark) | Authorship::Unattributed(mark) => {
                return Ok(FileAt::Hidden(mark));
            }
            Authorship::NothingHidden | Authorship::Shown { written: true, .. } => None,
            // Nobody is recorded as having written the path: a text there cannot be attributed.
            Authorship::Shown {
                written: false,
                unattributed,
            } => Some(unattributed),
        };
        let point = Point::Block(block_id.to_owned());
        match self
            .storage
            .read(&self.layout.repo_dir(&wiki.id), &point, path, max)
        {
            Ok(bytes) => Ok(match unattributed {
                Some(mark) => FileAt::Hidden(mark),
                None => FileAt::Held(bytes),
            }),
            Err(refused @ StorageError::Refused { .. }) => {
                // Either there was no such file then, or it is over the size read: `stat` tells.
                match self
                    .storage
                    .stat(&self.layout.repo_dir(&wiki.id), &point, path)
                {
                    Err(StorageError::Refused { .. }) => Ok(FileAt::Missing),
                    Ok(_) => Ok(match unattributed {
                        Some(mark) => FileAt::Hidden(mark),
                        None => FileAt::TooLarge(refused),
                    }),
                    Err(e) => Err(self.storage_error(wiki, e)),
                }
            }
            Err(e) => Err(self.storage_error(wiki, e)),
        }
    }

    /// The text of `path` as a view's tip holds it, as the storage gave it. **Unchecked**: the content of a view's tip cannot
    /// be hidden (RFC 006 D-8), and the check that keeps it so is [`ReadService::tip_check`]. Only a [`Tip`] can be asked, and only
    /// [`ReadService::tip`] makes one.
    pub(super) fn bytes_at_tip(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        path: &str,
    ) -> Result<Vec<u8>, ReadError> {
        self.storage
            .read(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(tip.block_id().to_owned()),
                path,
                self.config.page_max_bytes,
            )
            .map_err(|e| self.storage_error(wiki, e))
    }

    /// A governance file at a point, as bytes of at most `max`. **Unchecked**: the rules are not a revision's text, and what a
    /// wiki's rules are does not depend on what moderation hides.
    pub(super) fn rules_bytes_at(
        &self,
        wiki: &WikiRecord,
        point: &Point,
        path: &str,
        max: u64,
    ) -> Result<Vec<u8>, ReadError> {
        self.storage
            .read(&self.layout.repo_dir(&wiki.id), point, path, max)
            .map_err(|e| self.storage_error(wiki, e))
    }

    /// A page's text at a block **for *Why is this here?* to align**, whatever moderation hides of it: the reading compares one
    /// version of the page with the next to say which change wrote each paragraph, and says only *which change* (a number). What
    /// it prints of a paragraph is the paragraph of the tip, which cannot be hidden, and what it prints of a change is the change as
    /// every surface gives it (masked). **No text read here leaves the comparison.**
    pub(super) fn file_for_trace(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
        path: &str,
    ) -> Result<FileAt, ReadError> {
        let point = Point::Block(block_id.to_owned());
        match self.bytes_unchecked_at(wiki, &point, path)? {
            Unchecked::Held(bytes) => Ok(FileAt::Held(bytes)),
            Unchecked::Missing => Ok(FileAt::Missing),
            Unchecked::Large => match self.storage.read(
                &self.layout.repo_dir(&wiki.id),
                &point,
                path,
                self.config.page_max_bytes,
            ) {
                Err(refused) => Ok(FileAt::TooLarge(refused)),
                Ok(bytes) => Ok(FileAt::Held(bytes)),
            },
        }
    }

    /// The raw bytes of a knowledge file at a block, **unchecked**, to decide only whether the wiki can read it
    /// ([`super::preview`]'s `knowledge_readability_at`, G-T6 correction S-2): at most `bound` bytes, and the verdict is all
    /// that leaves the caller, never the bytes. A knowledge file is judged where it stands at a view's tip (the content of a
    /// tip cannot be hidden), and a file the service cannot read at all is the very case this is asked about.
    pub(super) fn bytes_for_readability(
        &self,
        repo: &std::path::Path,
        point: &Point,
        path: &str,
        bound: u64,
    ) -> Result<Vec<u8>, StorageError> {
        self.storage.read(repo, point, path, bound)
    }

    /// The file at a point, **unchecked**, for the two things that must see what is hidden: the comparison of a change's files
    /// with the views' tips (what the content of a tip is) and the one audited act that shows hidden content. Nothing else
    /// may read through it.
    pub(super) fn bytes_unchecked_at(
        &self,
        wiki: &WikiRecord,
        point: &Point,
        path: &str,
    ) -> Result<Unchecked, ReadError> {
        let repo = self.layout.repo_dir(&wiki.id);
        match self
            .storage
            .read(&repo, point, path, self.config.page_max_bytes)
        {
            Ok(bytes) => Ok(Unchecked::Held(bytes)),
            Err(refused @ StorageError::Refused { .. }) => {
                // Either the file is not there at that point, or it is over the size read: `stat` tells.
                match self.storage.stat(&repo, point, path) {
                    Err(StorageError::Refused { .. }) => Ok(Unchecked::Missing),
                    Ok(_) => Ok(Unchecked::Large),
                    Err(_) => Err(self.storage_error(wiki, refused)),
                }
            }
            Err(e) => Err(self.storage_error(wiki, e)),
        }
    }

    // ------------------------------------------------------------------------------------ what is hidden

    /// Is the text of the file at `path` at the block hidden, and by which hiding? `Ok(None)` when it is not (and when the
    /// service has no tombstones). **A store that cannot be read is an error**, never *nothing hidden*; a text that cannot be
    /// attributed is hidden (see the account in `read/writes.rs`).
    pub fn hidden_text(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
    ) -> Result<Option<Mark>, ReadError> {
        Ok(match self.authorship(wiki, view, block_id, path)? {
            Authorship::NothingHidden | Authorship::Shown { written: true, .. } => None,
            Authorship::Hidden(mark) | Authorship::Unattributed(mark) => Some(*mark),
            // Nobody is recorded as having written the path: nothing is hidden if nothing is there, and a text that is there
            // cannot be attributed. One `stat` tells which.
            Authorship::Shown {
                written: false,
                unattributed,
            } => match self.storage.stat(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(block_id.to_owned()),
                path,
            ) {
                Ok(_) => Some(*unattributed),
                Err(StorageError::Refused { .. }) => None,
                Err(e) => return Err(self.storage_error(wiki, e)),
            },
        })
    }

    /// **Would undoing a change merge cleanly over the tip?** The pages `paths` as they were at `parent` (the change's parent), written
    /// against `undoes` (the change), over the tip's: the three-way merge the acceptance runs (RFC 004 D-5). Only the pages the
    /// rollback writes are asked, so a change anywhere else is no obstacle. **The answer is a verdict and nothing else**: the
    /// texts are read here, whatever moderation hides of them, because a way back that is refused (its text is hidden) is
    /// refused for the reason it would have been offered, and that is asked of the files; no text leaves this function. A page
    /// that cannot be read as text at any of the three is not one that can be merged.
    pub(super) fn rollback_merges(
        &self,
        wiki: &WikiRecord,
        undoes: &str,
        parent: &str,
        tip_block: &str,
        paths: &[String],
    ) -> Result<bool, ReadError> {
        let (mut base, mut theirs, mut ours) = (Vec::new(), Vec::new(), Vec::new());
        for path in paths {
            let (Some(was), Some(now), Some(before)) = (
                self.merge_text(wiki, undoes, path)?,
                self.merge_text(wiki, tip_block, path)?,
                self.merge_text(wiki, parent, path)?,
            ) else {
                return Ok(false);
            };
            if let Some(text) = was {
                base.push(PageText::new(path.clone(), text));
            }
            if let Some(text) = now {
                theirs.push(PageText::new(path.clone(), text));
            }
            if let Some(text) = before {
                ours.push(PageText::new(path.clone(), text));
            }
        }
        Ok(merge(&base, &theirs, &ours).is_clean())
    }

    /// The text of a file at a block for the merge's verdict: `Some(None)` when there was no such file, `None` when it is not
    /// text or too large.
    fn merge_text(
        &self,
        wiki: &WikiRecord,
        block: &str,
        path: &str,
    ) -> Result<Option<Option<String>>, ReadError> {
        Ok(
            match self.bytes_unchecked_at(wiki, &Point::Block(block.to_owned()), path)? {
                Unchecked::Held(bytes) => String::from_utf8(bytes).ok().map(Some),
                Unchecked::Missing => Some(None),
                Unchecked::Large => None,
            },
        )
    }
}

#[cfg(test)]
mod tests;
