//! **The one reader of a file's text at a block** (RFC 006 D-8; INV-13; handoff G-T4, correction H-1). Every read of a page's
//! or a knowledge file's text at a block passes here, so that hidden text is hidden wherever it is read: the permalink, both
//! sides of a diff, a proposal's base, a revise, a rollback, *Why is this here?* and the sources a change attached.
//!
//! ```text
//! (block, path) ─▶ the changes of the wiki whose content is hidden now ─▶ none: read the file at the block
//!                                                                    ─▶ some: is this file one of theirs?
//!                  one of theirs: hidden (the tombstone of the change that wrote it)    not: read the file
//! ```
//!
//! **What is hidden.** The text of a path at a block is the text the change that *last wrote the path* gave it, which may be
//! a block long before the one asked for: B writes personal data to a page, C edits another page, D removes the data. At C
//! the page still reads as B wrote it, and hiding B hides it there. So the question is asked of the bytes: the file at the
//! block is **the same bytes** a hidden change wrote (at the path, or at any path the change wrote, so that a page moved
//! with its bytes is still the change's), and no change between the hidden one and the block wrote the path again. A change
//! that wrote the same bytes again, after the hidden one, is its own writer: its text is shown (and a hide covers the change
//! it was made on, not text a later change writes again). What cannot be settled within the bounds is **not** settled in the
//! text's favour: a change too large to list, a file too large to compare, a history that cannot be walked far enough, a block
//! that is not in the view's log: the text is hidden.
//!
//! **Private on purpose.** This module is the only one that asks the storage for the bytes of a file at a block (the write
//! path's own reads in `acceptance` and `knowledge::curation` are the two other places, listed in the gate below). The gate
//! is a test over the crate's sources ([`tests`]): a call to the storage's `read` anywhere else fails it. The three entry
//! points that do not check say in their names what they read and why it may be unchecked:
//!
//! - [`ReadService::bytes_at_tip`]: **a view's tip**. The content of a view's tip cannot be hidden (D-8), and the tip check
//!   (RFC 006 D-8; `tipcheck`) is what keeps it so;
//! - [`ReadService::rules_bytes_at`]: **a governance file**, which is the rules and not a revision's text;
//! - [`ReadService::bytes_unchecked_at`]: the tip check's comparison and the one audited act that shows hidden content;
//! - [`ReadService::file_for_trace`]: *Why is this here?* aligns one version of a page with the next and says which change wrote a
//!   paragraph, never what it said (a hidden change is still a change of the page's history, with its tombstone);
//! - the verdict of whether a rollback would merge, which is a boolean.
//!
//! **Cost.** A wiki in which nothing has had its content hidden pays one store query for a read, nothing else. A wiki in
//! which some change has is asked a digest of the file at the block (one `cat`) and of each file the hidden changes wrote
//! (held, a block's file never changes): the digests are of the bytes and are never served.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, PoisonError};

use inklkunn_prikk::{Point, StorageError};
use inklkunn_store::{WikiId, WikiRecord};
use sha2::{Digest as _, Sha256};

use super::history::{MAX_INSPECTED_PER_REQUEST, MAX_SHOWS_PER_REQUEST, PageEvent, TouchesOf};
use super::{Cause, ReadError, ReadService, Tip};
use crate::merge::{PageText, merge};
use crate::tombstone::Mark;

/// The most files of one hidden change that are compared with the file asked for.
const MAX_COMPARED_FILES: usize = 20;
/// The most digests held.
const MAX_DIGESTS: usize = 2048;

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

/// What the file at a block is, for the comparison of two files: a digest of its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Digest {
    Of([u8; 32]),
    Missing,
    TooLarge,
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

type DigestKey = (WikiId, String, String);

#[derive(Debug, Default)]
struct DigestCache {
    map: HashMap<DigestKey, Digest>,
    order: VecDeque<DigestKey>,
}

/// What the reader holds: the digests of files at blocks. A block's file never changes, so a digest is never stale; what is
/// hidden is asked at every request and nothing here says it.
#[derive(Debug, Default)]
pub(super) struct State {
    digests: Mutex<DigestCache>,
}

impl State {
    fn get(&self, key: &DigestKey) -> Option<Digest> {
        self.digests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map
            .get(key)
            .copied()
    }

    fn put(&self, key: DigestKey, digest: Digest) {
        let mut held = self.digests.lock().unwrap_or_else(PoisonError::into_inner);
        if held.map.insert(key.clone(), digest).is_none() {
            held.order.push_back(key);
        }
        while held.map.len() > MAX_DIGESTS {
            let Some(oldest) = held.order.pop_front() else {
                break;
            };
            held.map.remove(&oldest);
        }
    }
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
        if let Some(mark) = self.hidden_text(wiki, view, block_id, path)? {
            return Ok(FileAt::Hidden(Box::new(mark)));
        }
        let point = Point::Block(block_id.to_owned());
        match self
            .storage
            .read(&self.layout.repo_dir(&wiki.id), &point, path, max)
        {
            Ok(bytes) => Ok(FileAt::Held(bytes)),
            Err(refused @ StorageError::Refused { .. }) => {
                // Either there was no such file then, or it is over the size read: `stat` tells.
                match self
                    .storage
                    .stat(&self.layout.repo_dir(&wiki.id), &point, path)
                {
                    Err(StorageError::Refused { .. }) => Ok(FileAt::Missing),
                    Ok(_) => Ok(FileAt::TooLarge(refused)),
                    Err(e) => Err(self.storage_error(wiki, e)),
                }
            }
            Err(e) => Err(self.storage_error(wiki, e)),
        }
    }

    /// The text of `path` as a view's tip holds it, as the storage gave it. **Unchecked**: the content of a view's tip cannot
    /// be hidden (RFC 006 D-8), and the check that keeps it so is [`ReadService::tip_check`].
    pub(super) fn bytes_at_tip(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        path: &str,
    ) -> Result<Vec<u8>, ReadError> {
        self.storage
            .read(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(tip.block_id.clone()),
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
    /// service has no tombstones). **A store that cannot be read is an error**, never *nothing hidden*.
    pub fn hidden_text(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        path: &str,
    ) -> Result<Option<Mark>, ReadError> {
        let Some(tombstones) = &self.tombstones else {
            return Ok(None);
        };
        let hidden = tombstones.content_hidden_revisions(&wiki.id).map_err(|e| {
            self.store_failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ReadError::CouldNotBeReadNow(Cause::Store(inklkunn_store::StoreError::Backend(e.0)))
        })?;
        if hidden.is_empty() {
            return Ok(None);
        }
        let ours = match self.digest_at(wiki, block_id, path)? {
            Digest::Of(digest) => digest,
            // No file at the block: nothing is read, so nothing is shown.
            Digest::Missing => return Ok(None),
            // A file too large to compare: it cannot be said to be none of a hidden change's, so it is theirs when a hidden
            // change wrote the path.
            Digest::TooLarge => {
                for (hidden, mark) in &hidden {
                    let wrote = match self.written_paths(wiki, hidden)? {
                        TouchesOf::Paths(paths) => paths.iter().any(|p| p == path),
                        TouchesOf::TooLarge => true,
                    };
                    if wrote {
                        return Ok(Some(mark.clone()));
                    }
                }
                return Ok(None);
            }
        };
        for (hidden, mark) in &hidden {
            // The files the hidden change wrote: a change too large to list is compared at the path asked for alone.
            let wrote = match self.written_paths(wiki, hidden)? {
                TouchesOf::Paths(paths) => paths,
                TouchesOf::TooLarge => vec![path.to_owned()],
            };
            for file in wrote.iter().take(MAX_COMPARED_FILES) {
                if self.digest_at(wiki, hidden, file)? != Digest::Of(ours) {
                    continue;
                }
                // The same bytes: the hidden change wrote what the block holds, unless a change between them wrote the path
                // again (or the hidden change is later than the block).
                if file == path && self.is_not_the_writer(wiki, view, hidden, block_id, path)? {
                    continue;
                }
                return Ok(Some(mark.clone()));
            }
        }
        Ok(None)
    }

    /// Is it **certain** that the hidden change `hidden` is not the one whose text the file `path` has at `block`: the change is
    /// later than the block, or a change after it and up to the block wrote the path (a created or an edited page; a page only
    /// moved here carries its text with it)? What cannot be settled within the bounds is `false`: hidden.
    fn is_not_the_writer(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        hidden: &str,
        block: &str,
        path: &str,
    ) -> Result<bool, ReadError> {
        let tip = self.tip(wiki, view)?;
        let Some((window, at_block)) = self.find_block(wiki, &tip, block)? else {
            return Ok(false);
        };
        let Some(at_hidden) = window.block_position(hidden) else {
            return Ok(false);
        };
        // The log is newest first: a change at a lower place is a later one.
        if at_hidden < at_block {
            return Ok(true);
        }
        let mut tracked = path.to_owned();
        let (mut inspected, mut shown) = (0_usize, 0_usize);
        for at in at_block..at_hidden {
            if inspected >= MAX_INSPECTED_PER_REQUEST || shown >= MAX_SHOWS_PER_REQUEST {
                return Ok(false);
            }
            inspected += 1;
            let Some(between) = window.block_id_at(at) else {
                return Ok(false);
            };
            let (events, fetched) = match self.events_of(wiki, between, &mut tracked)? {
                Some(read) => read,
                None => return Ok(false),
            };
            shown += usize::from(fetched);
            if events
                .iter()
                .any(|e| matches!(e, PageEvent::Created | PageEvent::Edited))
            {
                return Ok(true);
            }
        }
        Ok(false)
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

    /// The digest of the file at the block, held.
    fn digest_at(&self, wiki: &WikiRecord, block: &str, path: &str) -> Result<Digest, ReadError> {
        let key = (wiki.id.clone(), block.to_owned(), path.to_owned());
        if let Some(held) = self.texts.get(&key) {
            return Ok(held);
        }
        let digest = match self.bytes_unchecked_at(wiki, &Point::Block(block.to_owned()), path)? {
            Unchecked::Held(bytes) => Digest::Of(Sha256::digest(&bytes).into()),
            Unchecked::Missing => Digest::Missing,
            Unchecked::Large => Digest::TooLarge,
        };
        self.texts.put(key, digest);
        Ok(digest)
    }
}

#[cfg(test)]
mod tests;
