//! *Why is this here?* (RFC 005 part 2 D-8; handoff 005 K-T7): for each paragraph of a page, the change that last
//! changed it and the change that first wrote it, as far as the page's history can say.
//!
//! ```text
//! why  ─▶ the view's tip ─▶ the title index (which page) ─▶ the page as it is (one `cat`)
//!      ─▶ ONE step of the page's history (`history_step`: the history page's own bounds)
//!      ─▶ the page as it was after each change of the step, newest first (one `cat` each, one at a time)
//!      ─▶ [`trace::Tracer`]: each paragraph followed back from one version to the next
//! ```
//!
//! **Derived from prikk's history and nothing else.** There is no store of where a paragraph came from: the
//! history is the one the history page reads, the texts are the page at the blocks of that history, and the
//! matching is the merge's own ([`crate::merge::alignment`]) over the product's own segmentation
//! ([`inklkunn_markup::blocks`]).
//!
//! **Bounded by the history page's bounds, by construction.** The changes read are the first step of the page's
//! history ([`HISTORY_PAGE_ROWS`](super::history::HISTORY_PAGE_ROWS) changes of the page at most, at most
//! [`MAX_INSPECTED_PER_REQUEST`] blocks inspected, at most
//! [`MAX_SHOWS_PER_REQUEST`](super::history::MAX_SHOWS_PER_REQUEST) `show`s, a log no longer than
//! [`MAX_WINDOW`](super::history::MAX_WINDOW) blocks): nothing is read
//! beyond them, and the answer says that it stopped there ([`Reach::Bounded`]). The versions are read one at a time,
//! two held at once, so the memory is two pages and a small record per paragraph, whatever the history; at most one
//! `cat` per change read, through the same storage (and so the same admission gate, deadlines and ceilings) as every
//! other read. **Bounded in work as well**: the alignments of one request share [`MAX_WHY_WORK`] units of work, and
//! the reading ends at the step that would pass it ([`Reach::Spent`]), the changes it did not read left out of the
//! answer. A paragraph beyond the first [`MAX_WHY_PARAGRAPHS`] of a page is not followed; the answer counts them.
//!
//! **What the answer never claims.** A paragraph with an `^id` that is the one block with it on both sides of every
//! step is followed by its anchor ([`Basis::Anchor`]); any other is followed by matching the text
//! ([`Basis::Text`]) and the surface says so. A paragraph whose beginning was not reached has no first writer, and
//! says that it was written before the changes read; a history that could not be followed to the end is
//! [`Reach::Unfollowed`], never guessed across.

mod trace;

use inklkunn_markup::path::title_to_path;
use inklkunn_store::IndexEntry;

use super::history::{ChangeInfo, HistoryRow, MAX_INSPECTED_PER_REQUEST, PageEvent};
use super::{Lookup, PageProblem, ReadError, ReadService, Tip};
use trace::{Fed, Tracer};

pub use trace::{Basis, Chain, EXCERPT_CHARS, Paragraph};

/// The most paragraphs of one page that are followed: the same figure as the history's bound on the blocks one
/// request inspects, since each paragraph is one more thing the request follows.
pub const MAX_WHY_PARAGRAPHS: usize = MAX_INSPECTED_PER_REQUEST;

/// The most work one request spends aligning the page's versions: one unit is a byte read, a block looked at or a
/// cell of an alignment's table. Twenty alignments of a page of 1 MiB would, without it, do whatever the page
/// makes them do; with it a request is bounded in time as it is in reads. The reading ends at the step that would
/// pass it ([`Reach::Spent`]).
pub const MAX_WHY_WORK: usize = 32_000_000;

/// How far back the paragraphs could be followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Every paragraph was followed to the change that wrote it.
    Whole,
    /// The history page's bounds ended the reading while some paragraph still had an earlier wording: older
    /// changes exist that were not read.
    Bounded,
    /// The reading ended at [`MAX_WHY_WORK`]: the changes listed after those in [`WhyPage::changes`] (older ones)
    /// were not read, and [`WhyPage::changes`] holds only the changes that were. Older changes exist.
    Spent,
    /// The reading ended for another reason (an earlier version could not be read, or the history lists no
    /// change that wrote the page): what lies before is not known.
    Unfollowed,
}

/// A paragraph of the page as it is, and what became known of its past.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhyParagraph {
    /// The paragraph.
    pub paragraph: Paragraph,
    /// Its past. The numbers are places in [`WhyPage::changes`].
    pub chain: Chain,
}

/// The paragraphs of a page, each with the changes that last changed it and first wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhyPage {
    /// The page's title.
    pub title: String,
    /// The page's path.
    pub path: String,
    /// The page's `id` (frontmatter), when it has one: what a claim names it by.
    pub page_id: Option<String>,
    /// The tip everything was read at.
    pub tip: Tip,
    /// The changes that touched the page among those the history page's bounds let be read, newest first, and
    /// that were read: when the reading was [`Reach::Spent`], only those before the step it stopped at.
    pub changes: Vec<ChangeInfo>,
    /// The paragraphs, in the page's order, at most [`MAX_WHY_PARAGRAPHS`].
    pub paragraphs: Vec<WhyParagraph>,
    /// How many more blocks the page has than are followed.
    pub left_out: usize,
    /// How far back the paragraphs could be followed.
    pub reach: Reach,
}

/// What a request for a page's *Why is this here?* came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhyOutcome {
    /// The page's paragraphs.
    Page(Box<WhyPage>),
    /// No page has this title at the tip.
    Missing {
        /// The title asked for, trimmed.
        title: String,
    },
    /// Several pages match the title and none is nearer.
    Ambiguous {
        /// Every candidate.
        candidates: Vec<IndexEntry>,
    },
}

/// Is the page absent after a change with these events (deleted, or moved away)?
fn absent_after(events: &[PageEvent]) -> bool {
    matches!(
        events.last(),
        Some(PageEvent::Deleted | PageEvent::MovedTo { .. })
    )
}

/// The path the page had before a change: its own, unless the change moved it here.
fn path_before(row: &HistoryRow) -> String {
    row.events
        .iter()
        .find_map(|event| match event {
            PageEvent::MovedFrom { title } => title_to_path(title).ok(),
            _ => None,
        })
        .unwrap_or_else(|| row.path.clone())
}

impl ReadService {
    /// The paragraphs of the page titled `title` at the view's tip, each with the change that last changed it and
    /// the one that first wrote it, within the bounds of the page's history (see the module's account).
    pub fn page_why(
        &self,
        wiki: &inklkunn_store::WikiRecord,
        view: Option<&str>,
        title: &str,
    ) -> Result<WhyOutcome, ReadError> {
        title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let tip = self.tip(wiki, view)?;
        let entry = match self.index_at(wiki, &tip)?.lookup(title) {
            Lookup::Found(found) => found.entry,
            Lookup::BrokenRedirect { from, .. } => from,
            Lookup::Ambiguous { candidates } => return Ok(WhyOutcome::Ambiguous { candidates }),
            Lookup::Missing => {
                return Ok(WhyOutcome::Missing {
                    title: title.trim().to_owned(),
                });
            }
        };
        // The page as it is, at the very block the history is read from.
        let current = self
            .source_at(wiki, &tip.block_id, &entry.path)?
            .ok_or(ReadError::PageUnreadable(PageProblem::NotUtf8))?;
        let history = self.history_step(
            wiki,
            tip.clone(),
            entry.path.clone(),
            entry.title.clone(),
            None,
        )?;
        let mut tracer = Tracer::begin(&current, MAX_WHY_PARAGRAPHS, MAX_WHY_WORK);
        let rows = &history.rows;
        let mut unfollowed = false;
        // The step the work budget ended the reading at: the changes from there on were not read.
        let mut spent_at: Option<usize> = None;
        for (step, row) in rows.iter().enumerate() {
            if !tracer.is_following() {
                break;
            }
            // The page after the newest change is the page as it is, unless the newest change is not the tip's
            // own block: then the two are compared, and a difference no listed change explains ends the reading.
            if step == 0 && row.change.block_id != tip.block_id {
                let after = self.source_at(wiki, &row.change.block_id, &row.path)?;
                if after.as_deref() != Some(current.as_str()) {
                    unfollowed = true;
                    break;
                }
            }
            let older = if row.events.contains(&PageEvent::Created) {
                None
            } else if let Some(before) = rows.get(step + 1) {
                match self.source_at(wiki, &before.change.block_id, &before.path)? {
                    Some(text) => Some(text),
                    // Absent, or not text: only a page that was deleted or moved away is absent.
                    None if absent_after(&before.events) => None,
                    None => {
                        unfollowed = true;
                        break;
                    }
                }
            } else {
                // The oldest change read: the page before it is the page at the block before it, one read more
                // (so that what this change did to each paragraph is known). A first block, or a block the log
                // window does not hold the one before, has none to read.
                let Some(parent) = &row.change.parent else {
                    break;
                };
                match self.source_at(wiki, parent, &path_before(row))? {
                    Some(text) => Some(text),
                    None => {
                        unfollowed = true;
                        break;
                    }
                }
            };
            if tracer.feed(step, older.as_deref()) == Fed::OutOfWork {
                spent_at = Some(step);
                break;
            }
        }
        let reach = if !tracer.is_following() {
            Reach::Whole
        } else if spent_at.is_some() {
            Reach::Spent
        } else if !unfollowed && (history.older.is_some() || history.beyond_window) {
            Reach::Bounded
        } else {
            Reach::Unfollowed
        };
        let paragraphs = tracer
            .paragraphs()
            .iter()
            .cloned()
            .zip(tracer.chains())
            .map(|(paragraph, chain)| WhyParagraph { paragraph, chain })
            .collect();
        Ok(WhyOutcome::Page(Box::new(WhyPage {
            title: entry.title,
            path: entry.path,
            page_id: entry.page_id,
            tip: history.tip,
            changes: history
                .rows
                .into_iter()
                .take(spent_at.unwrap_or(usize::MAX))
                .map(|row| row.change)
                .collect(),
            paragraphs,
            left_out: tracer.left_out(),
            reach,
        })))
    }
}
