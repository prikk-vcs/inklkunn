//! Is a change's content a view's tip (RFC 006 D-8)? **The content of a view's tip cannot be hidden**: a page whose current
//! text is hidden is not a page. The question is asked of every open view, not of the default alone, and it is asked of the
//! bytes: a change's content *is* a tip's when a file it wrote is, byte for byte, what a view's tip holds **at any path** now (a
//! page that was moved with its bytes is still the tip's text; H-5). (A later change that wrote the same bytes again makes the
//! hidden text the tip's text just the same, so hiding it would hide nothing the page shows.)
//!
//! **What the rule can promise, and what it cannot.** When no tip holds a file of the change whole, the hide is allowed, and the
//! rule says what it does *not* do ([`TipCheck::Survives`]): a later change that fixed a typo and nothing else leaves the tip holding
//! most of the lines the change added, and hiding the change hides the change, not those lines. The count is of the lines the
//! change added (those its files have and its parent's did not) that the tip's page still has as lines; it is bounded, and a page
//! so large that the bound ended the count says *at least*. A hide covers **this** change, not text that a later change writes
//! again.
//!
//! A change too large to list, or a file too large to compare, is **refused**, not allowed: what cannot be checked is not
//! hidden. Reverting or removing the content first, by proposal, is the way (soft security first).

use inklkunn_prikk::Point;
use inklkunn_store::WikiRecord;

use super::history::{ChangeInfo, TouchesOf};
use super::text::Unchecked;
use super::{ReadError, ReadService};

/// The most files of a change that *show hidden content* lists.
pub const MAX_REVEALED_FILES: usize = 20;
/// The most files of a tip that one file of a change is compared with when the page may have been moved (those of the same size
/// that are not at its own path). A change with more of them than this is not compared: it may not be hidden.
pub const MAX_MOVE_CANDIDATES: usize = 8;
/// The most added lines the count of surviving lines looks at, over a change's files. Past it the count stops and says *at least*.
pub const MAX_SURVIVING_LINES: usize = 20_000;

/// A change as it was written, **hidden or not**: what the `moderate` rung is shown by the one audited act that shows hidden
/// content. Nothing but that act asks for one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revealed {
    /// The change as the history says it: the summary and the people unhidden.
    pub info: ChangeInfo,
    /// The files it wrote: path and text (lossy UTF-8), at most [`MAX_REVEALED_FILES`].
    pub files: Vec<(String, String)>,
}

/// What the check of a change's content found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TipCheck {
    /// No view's tip holds what the change wrote, and no tip's page holds a line it added: its content may be hidden.
    Clear,
    /// No view's tip holds what the change wrote whole, so its content may be hidden, **but a tip's page still holds lines the change
    /// added**: hiding the change does not hide them, and the moderation form says so (an edit that removes them is the way).
    Survives {
        /// How many lines (counted over the pages the change wrote, in the view that holds the most of each).
        lines: usize,
        /// The count stopped at [`MAX_SURVIVING_LINES`] lines looked at: there are at least this many.
        at_least: bool,
    },
    /// A view's tip holds what the change wrote (the view's short name): it may not be hidden.
    Holds {
        /// The view, by short name.
        view: String,
    },
    /// The change, or a file of it, is too large to compare: it may not be hidden.
    Unchecked,
}

impl ReadService {
    /// Does any open view's tip hold, at any path, the bytes the change `block_id` wrote? And when none does, does a tip's page
    /// still hold lines the change added (see the module's account)?
    pub fn tip_check(&self, wiki: &WikiRecord, block_id: &str) -> Result<TipCheck, ReadError> {
        let paths = match self.written_paths(wiki, block_id)? {
            TouchesOf::Paths(paths) => paths,
            TouchesOf::TooLarge => return Ok(TipCheck::Unchecked),
        };
        if paths.is_empty() {
            return Ok(TipCheck::Clear);
        }
        let mut ours: Vec<(String, Vec<u8>)> = Vec::new();
        for path in &paths {
            match self.bytes_unchecked_at(wiki, &Point::Block(block_id.to_owned()), path)? {
                Unchecked::Held(bytes) => ours.push((path.clone(), bytes)),
                Unchecked::Missing => {}
                Unchecked::Large => return Ok(TipCheck::Unchecked),
            }
        }
        let mut tips: Vec<String> = Vec::new();
        for view in self.open_views(wiki)? {
            let tip = self.tip(wiki, Some(&view))?;
            let listing = self
                .storage
                .tree(
                    &self.layout.repo_dir(&wiki.id),
                    &Point::Block(tip.block_id.clone()),
                    None,
                )
                .map_err(|e| self.storage_error(wiki, e))?;
            for (path, bytes) in &ours {
                // The file at its own path first, then a file of the same size anywhere (a page moved with its bytes).
                let mut elsewhere: Vec<&str> = listing
                    .entries
                    .iter()
                    .filter(|e| e.kind == "file" && e.path != *path)
                    .filter(|e| e.size.is_none_or(|size| size == bytes.len() as u64))
                    .map(|e| e.path.as_str())
                    .collect();
                if elsewhere.len() > MAX_MOVE_CANDIDATES {
                    return Ok(TipCheck::Unchecked);
                }
                let mut candidates: Vec<&str> = vec![path.as_str()];
                candidates.append(&mut elsewhere);
                for candidate in candidates {
                    match self.bytes_unchecked_at(
                        wiki,
                        &Point::Block(tip.block_id.clone()),
                        candidate,
                    )? {
                        Unchecked::Missing => {}
                        Unchecked::Large => return Ok(TipCheck::Unchecked),
                        Unchecked::Held(theirs) if theirs == *bytes => {
                            return Ok(TipCheck::Holds {
                                view: view.strip_prefix("heads/").unwrap_or(&view).to_owned(),
                            });
                        }
                        Unchecked::Held(_) => {}
                    }
                }
            }
            tips.push(tip.block_id);
        }
        Ok(match self.surviving_lines(wiki, block_id, &ours, &tips)? {
            (0, _) => TipCheck::Clear,
            (lines, at_least) => TipCheck::Survives { lines, at_least },
        })
    }

    /// How many lines the change added that a tip's page still has, and whether the count stopped at its bound. A line is a line of
    /// text with something in it; it is **added** when the change's file has it and the file the change's parent had at the same
    /// path did not; it **survives** when the tip's page at that path has it as a line. Per file, the view that holds most counts.
    fn surviving_lines(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
        ours: &[(String, Vec<u8>)],
        tips: &[String],
    ) -> Result<(usize, bool), ReadError> {
        let Some(info) = self.change_info_raw(wiki, None, block_id)? else {
            return Ok((0, false));
        };
        let (mut total, mut looked, mut capped) = (0_usize, 0_usize, false);
        for (path, bytes) in ours {
            let Ok(text) = std::str::from_utf8(bytes) else {
                continue;
            };
            let before: std::collections::HashSet<String> = match &info.parent {
                Some(parent) => {
                    match self.bytes_unchecked_at(wiki, &Point::Block(parent.clone()), path)? {
                        Unchecked::Held(old) => String::from_utf8_lossy(&old)
                            .lines()
                            .map(|l| l.trim().to_owned())
                            .collect(),
                        Unchecked::Missing | Unchecked::Large => std::collections::HashSet::new(),
                    }
                }
                None => std::collections::HashSet::new(),
            };
            let mut added: Vec<&str> = Vec::new();
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                if looked >= MAX_SURVIVING_LINES {
                    capped = true;
                    break;
                }
                looked += 1;
                if !before.contains(line) {
                    added.push(line);
                }
            }
            if added.is_empty() {
                continue;
            }
            let mut best = 0;
            for tip in tips {
                if let Unchecked::Held(now) =
                    self.bytes_unchecked_at(wiki, &Point::Block(tip.clone()), path)?
                {
                    let held: std::collections::HashSet<&str> = std::str::from_utf8(&now)
                        .unwrap_or_default()
                        .lines()
                        .map(str::trim)
                        .collect();
                    best = best.max(added.iter().filter(|line| held.contains(**line)).count());
                }
            }
            total += best;
        }
        Ok((total, capped))
    }

    /// The change `block_id` of the view as it was written, with nothing hidden: `Ok(None)` when the view's history has no such
    /// change. **The one function that gives hidden text back**; the moderation service calls it after the audit event of
    /// *show hidden content* is written, and nothing else may.
    pub fn reveal_change(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
    ) -> Result<Option<Revealed>, ReadError> {
        let Some(info) = self.change_info_raw(wiki, view, block_id)? else {
            return Ok(None);
        };
        let paths = match self.written_paths(wiki, block_id)? {
            TouchesOf::Paths(paths) => paths,
            TouchesOf::TooLarge => Vec::new(),
        };
        let mut files = Vec::new();
        for path in paths.into_iter().take(MAX_REVEALED_FILES) {
            if let Unchecked::Held(bytes) =
                self.bytes_unchecked_at(wiki, &Point::Block(block_id.to_owned()), &path)?
            {
                files.push((path, String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
        Ok(Some(Revealed { info, files }))
    }

    /// The repository paths the change wrote (not the ones it deleted): `None` when it is too large to list. What a hiding of
    /// the change's content is recorded against, one path at a time (the export names the page by it).
    pub fn written_paths_of(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
    ) -> Result<Option<Vec<String>>, ReadError> {
        Ok(match self.written_paths(wiki, block_id)? {
            TouchesOf::Paths(paths) => Some(paths),
            TouchesOf::TooLarge => None,
        })
    }
}
