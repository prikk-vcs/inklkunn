//! **The later revisions that still hold the text of a change** (RFC 006 D-8, RR-15; handoff G-T4, correction J-4): what the moderation
//! form lists beside its sentence about a tip, because **hiding a change hides the change and not the text a later change wrote
//! again** (J-1's rule: *who wrote the text* decides whether it is hidden). A later revision of a page that carries most of what the
//! change added still shows it; the moderator decides whether to hide that revision too, and the form says so in plain words.
//!
//! # What *holds* means (exactly)
//!
//! For each path the change wrote, **the lines the change added** are the distinct lines of the change's own text at the path,
//! trimmed, that are not blank, and that the change's parent did not have at the path (trimmed too) — the change's own diff against
//! its parent, the parent's text read at the path the file was *moved from* when the change moved it. A later revision of the path
//! **holds the change's text** when **strictly more than half** of those lines are present in the revision's text at the path,
//! as whole lines (trimmed). Written as a rule on the counts: `2 * present > added`. A change that added no non-blank line to a path
//! (it only removed text, moved the file, or changed its mode) **names nothing** for it.
//!
//! The candidates are the later revisions that **wrote the path** (a write, or a pure rename into it: [`ReadService::later_writers`],
//! one indexed query) and nothing else: a revision that did not touch the path holds what the one before it did. **A file that was
//! moved with its bytes after the change is followed to where it went** (a later revision of the new path is a candidate, and so is
//! the move itself), within a bound ([`super::MAX_FOLLOWED_MOVES`]); a rename that also edits the file in the same block is not followed.
//! **A later revision that may have written the page and cannot be told to have** (an operation whose path was not resolved, a
//! revision too large to list, a kind of revision whose effect is not listed) is **counted and said** ([`LaterHolders::untraced`]),
//! never left out in silence.
//!
//! # What is read, and what is never told
//!
//! Every later revision's text is read through the one reader ([`ReadService::text_at`]). **A revision whose text is itself hidden is
//! skipped**: it is not read, it is not counted as holding anything, and it is not listed; the form says that some were skipped
//! ([`LaterHolders::hidden`]). A revision that cannot be compared (a file larger than a page may be) counts among those that could
//! not be checked. The answer carries a block, a path and the moment this server first saw the block (the history carries no time),
//! **never the text and never a name**.
//!
//! # The bounds, and what is said of them
//!
//! At most [`MAX_HOLDER_PATHS`] paths of one change are looked at, and at most [`MAX_HOLDER_CHECKS`] later revisions of one path are
//! read; at most [`MAX_HOLDERS_LISTED`] holders are listed and the rest are counted. **A case the bounds leave unchecked is counted
//! and said** ([`LaterHolders::unchecked`], [`LaterHolders::paths_unchecked`], and [`LaterHolders::unsettled`] when the index does
//! not reach the change yet, and the moves past the bound): it is never silently absent. The cost is one `show`, the change's own text and its parent's, one
//! indexed query per path and at most `MAX_HOLDER_PATHS * MAX_HOLDER_CHECKS` reads, **whatever the length of the history**.

use std::collections::HashSet;

use std::collections::BTreeSet;

use inklkunn_prikk::Point;
use inklkunn_store::{MAX_LATER_WRITES, WikiRecord};

use super::history::{TouchKind, Touches, TouchesOf};
use super::text::Unchecked;
use super::writes::ParentBlock;
use super::{BlockText, ReadError, ReadService};

/// The most paths of one change whose later revisions are looked for. The paths after them are counted
/// ([`LaterHolders::paths_unchecked`]).
pub const MAX_HOLDER_PATHS: usize = 20;
/// The most later revisions of one path whose text is read. The revisions after them are counted ([`LaterHolders::unchecked`]).
pub const MAX_HOLDER_CHECKS: usize = 50;
/// The most holders listed. The others are counted ([`LaterHolders::more`]).
pub const MAX_HOLDERS_LISTED: usize = 10;

/// A later revision of a page that holds most of the lines a change added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaterHolder {
    /// The revision's block id: the address of its permalink.
    pub block: String,
    /// The path the change wrote and the revision holds the text of.
    pub path: String,
    /// When this server first saw the revision (Unix seconds); `None` when the store could not say. The history carries no time.
    pub first_seen: Option<i64>,
}

/// What the moderation form tells of the later revisions that hold a change's text (see the module's account).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaterHolders {
    /// The holders, at most [`MAX_HOLDERS_LISTED`]: by path in the order the change wrote them, oldest revision first.
    pub listed: Vec<LaterHolder>,
    /// Holders that were found and are not listed.
    pub more: usize,
    /// Later revisions whose text is hidden itself: skipped, neither read nor counted as holding.
    pub hidden: usize,
    /// Later revisions that were not checked (past [`MAX_HOLDER_CHECKS`] of a path, or too large to compare).
    pub unchecked: usize,
    /// There are at least [`LaterHolders::unchecked`] of them: the list of later writers itself was cut at its bound.
    pub unchecked_at_least: bool,
    /// Paths of the change that were not looked at (past [`MAX_HOLDER_PATHS`], or too large to read).
    pub paths_unchecked: usize,
    /// Later revisions that **may have written** one of the change's pages and that the record of who wrote what cannot say did or did
    /// not (an operation whose path was not resolved, a revision too large to list, a kind of revision whose effect is not listed):
    /// they were not read, and are not listed.
    pub untraced: usize,
    /// There are at least [`LaterHolders::untraced`] of them: the record's list of them was cut at its bound.
    pub untraced_at_least: bool,
    /// A file of the change was moved away with its bytes more often than [`super::MAX_FOLLOWED_MOVES`] follows: the revisions of the paths
    /// it went to after the last one followed are not looked at.
    pub moves_unfollowed: bool,
    /// The index of who wrote what does not reach the change yet, or the change is too large to list: **nothing can be said**, and the
    /// rest of the answer is empty.
    pub unsettled: bool,
}

impl LaterHolders {
    /// There is nothing to tell the moderator: no holder, nothing skipped, nothing left unchecked.
    #[must_use]
    pub fn is_silent(&self) -> bool {
        self.listed.is_empty()
            && self.more == 0
            && self.hidden == 0
            && self.unchecked == 0
            && self.paths_unchecked == 0
            && self.untraced == 0
            && !self.moves_unfollowed
            && !self.unsettled
    }

    /// The answer when nothing can be said: the list is not an answer, and the form says so.
    #[must_use]
    pub fn unsettled() -> LaterHolders {
        LaterHolders {
            unsettled: true,
            ..LaterHolders::default()
        }
    }
}

/// Whether strictly more than half of `added` are lines of `text` (trimmed, whole lines).
fn holds(added: &HashSet<String>, text: &str) -> bool {
    let present: HashSet<&str> = text.lines().map(str::trim).collect();
    let count = added
        .iter()
        .filter(|line| present.contains(line.as_str()))
        .count();
    count.saturating_mul(2) > added.len()
}

impl ReadService {
    /// The later revisions of the pages the change `block_id` wrote that hold most of the lines it added: what the moderation form
    /// lists beside its sentence about a tip (see the module's account for what *holds* means and for every bound). `view` is the
    /// view whose history orders the revisions (the default view for `None`).
    pub fn later_holders(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
    ) -> Result<LaterHolders, ReadError> {
        // The index first: a block it does not reach (or that is no block of the view) is asked nothing else.
        let parent = match self.indexed_parent(wiki, view, block_id)? {
            ParentBlock::Unsettled => return Ok(LaterHolders::unsettled()),
            ParentBlock::First => None,
            ParentBlock::Block(parent) => Some(parent),
        };
        let paths = match self.written_paths(wiki, block_id)? {
            TouchesOf::Paths(paths) => paths,
            // A change that may have written a path nothing names cannot be followed by the paths it names.
            TouchesOf::TooLarge | TouchesOf::Unresolved(_) => {
                return Ok(LaterHolders::unsettled());
            }
        };
        if paths.is_empty() {
            return Ok(LaterHolders::default());
        }
        let mut out = LaterHolders {
            paths_unchecked: paths.len().saturating_sub(MAX_HOLDER_PATHS),
            ..LaterHolders::default()
        };
        let mut found: Vec<(String, String)> = Vec::new();
        // The later revisions that may have written a page of the change and could not be told apart, by number (one for all pages).
        let mut untraced: BTreeSet<u64> = BTreeSet::new();
        for path in paths.iter().take(MAX_HOLDER_PATHS) {
            let added = match self.added_lines(wiki, block_id, parent.as_deref(), path)? {
                Some(added) => added,
                None => {
                    // A file too large to compare is a path that was not looked at.
                    out.paths_unchecked += 1;
                    continue;
                }
            };
            if added.is_empty() {
                continue;
            }
            let later = self.later_writers(wiki, view, block_id, path, MAX_LATER_WRITES)?;
            if !later.settled {
                return Ok(LaterHolders::unsettled());
            }
            out.unchecked += later.writers.len().saturating_sub(MAX_HOLDER_CHECKS);
            out.unchecked_at_least |= later.more;
            untraced.extend(later.untraced.iter().copied());
            out.untraced_at_least |= later.untraced_at_least;
            out.moves_unfollowed |= later.moves_unfollowed;
            for writer in later.writers.iter().take(MAX_HOLDER_CHECKS) {
                let text = match self.text_at(wiki, view, &writer.block, &writer.path) {
                    Ok(text) => text,
                    // A text that cannot be checked yet is one that was not looked at (and no hide is said of it).
                    Err(ReadError::NotCheckedYet) => {
                        out.unchecked += 1;
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                // A revision of the path the file was moved to is read at that path (the lines the change added are its own).
                match text {
                    BlockText::Text(text) => {
                        if holds(&added, &text) {
                            found.push((writer.block.clone(), writer.path.clone()));
                        }
                    }
                    BlockText::Hidden(_) => out.hidden += 1,
                    BlockText::TooLarge => out.unchecked += 1,
                    BlockText::Missing | BlockText::NotText => {}
                }
            }
        }
        out.untraced = untraced.len();
        out.more = found.len().saturating_sub(MAX_HOLDERS_LISTED);
        found.truncate(MAX_HOLDERS_LISTED);
        let blocks: Vec<String> = found.iter().map(|(block, _)| block.clone()).collect();
        let seen = if blocks.is_empty() {
            Vec::new()
        } else {
            self.derived
                .first_seen(&wiki.id, &blocks, inklkunn_store::record::now_secs())
                .unwrap_or_else(|_| {
                    self.store_failures
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Vec::new()
                })
        };
        out.listed = found
            .into_iter()
            .enumerate()
            .map(|(at, (block, path))| LaterHolder {
                block,
                path,
                first_seen: seen.get(at).copied().filter(|t| *t > 0),
            })
            .collect();
        Ok(out)
    }

    /// The lines the change added to `path` (see the module's account): `None` when the file is too large to compare. A file that
    /// is not text adds none.
    fn added_lines(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
        parent: Option<&str>,
        path: &str,
    ) -> Result<Option<HashSet<String>>, ReadError> {
        let ours = match self.bytes_unchecked_at(wiki, &Point::Block(block_id.to_owned()), path)? {
            Unchecked::Held(bytes) => bytes,
            Unchecked::Missing => return Ok(Some(HashSet::new())),
            Unchecked::Large => return Ok(None),
        };
        let Ok(text) = std::str::from_utf8(&ours) else {
            return Ok(Some(HashSet::new()));
        };
        let before: HashSet<String> = match parent {
            Some(parent) => {
                let from = self.moved_from(wiki, block_id, path)?;
                match self.bytes_unchecked_at(
                    wiki,
                    &Point::Block(parent.to_owned()),
                    from.as_deref().unwrap_or(path),
                )? {
                    Unchecked::Held(old) => String::from_utf8_lossy(&old)
                        .lines()
                        .map(|l| l.trim().to_owned())
                        .collect(),
                    Unchecked::Missing | Unchecked::Large => HashSet::new(),
                }
            }
            None => HashSet::new(),
        };
        Ok(Some(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !before.contains(*l))
                .map(str::to_owned)
                .collect(),
        ))
    }

    /// The path the change moved the file at `path` from, when it did.
    fn moved_from(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
        path: &str,
    ) -> Result<Option<String>, ReadError> {
        Ok(match self.touches_of(wiki, block_id)?.0 {
            Touches::Known(list) => list.iter().find_map(|t| match &t.kind {
                TouchKind::Rename { from } if t.path == path => Some(from.clone()),
                _ => None,
            }),
            Touches::TooLarge => None,
        })
    }
}

#[cfg(test)]
mod tests;
