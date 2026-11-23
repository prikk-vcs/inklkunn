//! Is a change's content a view's tip (RFC 006 D-8)? **The content of a view's tip cannot be hidden**: a page whose current
//! text is hidden is not a page. The question is asked of every open view, not of the default alone, and it is asked of the
//! bytes: a change's content *is* a tip's when a file it wrote is, byte for byte, what a view's tip holds at that path now.
//! (A later change that wrote the same bytes again makes the hidden text the tip's text just the same, so hiding it would hide
//! nothing the page shows.)
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
    /// No view's tip holds what the change wrote: its content may be hidden.
    Clear,
    /// A view's tip holds what the change wrote (the view's short name): it may not be hidden.
    Holds {
        /// The view, by short name.
        view: String,
    },
    /// The change, or a file of it, is too large to compare: it may not be hidden.
    Unchecked,
}

impl ReadService {
    /// Does any open view's tip hold, at any path, the bytes the change `block_id` wrote?
    pub fn tip_check(&self, wiki: &WikiRecord, block_id: &str) -> Result<TipCheck, ReadError> {
        let paths = match self.written_paths(wiki, block_id)? {
            TouchesOf::Paths(paths) => paths,
            TouchesOf::TooLarge => return Ok(TipCheck::Unchecked),
        };
        if paths.is_empty() {
            return Ok(TipCheck::Clear);
        }
        for view in self.open_views(wiki)? {
            let tip = self.tip(wiki, Some(&view))?;
            for path in &paths {
                let theirs =
                    self.bytes_unchecked_at(wiki, &Point::Block(tip.block_id.clone()), path)?;
                let ours =
                    self.bytes_unchecked_at(wiki, &Point::Block(block_id.to_owned()), path)?;
                match (ours, theirs) {
                    (Unchecked::Missing, _) | (_, Unchecked::Missing) => {}
                    (Unchecked::Large, _) | (_, Unchecked::Large) => {
                        return Ok(TipCheck::Unchecked);
                    }
                    (Unchecked::Held(a), Unchecked::Held(b)) if a == b => {
                        return Ok(TipCheck::Holds {
                            view: view.strip_prefix("heads/").unwrap_or(&view).to_owned(),
                        });
                    }
                    (Unchecked::Held(_), Unchecked::Held(_)) => {}
                }
            }
        }
        Ok(TipCheck::Clear)
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
