//! A page that was removed (RFC 006 D-10; handoff G-T6): what a title that leads to no page at a view's tip says when the
//! page's own history says its last change removed it.
//!
//! ```text
//! a title that is no page at the tip ─▶ the page's history at that tip (the path the title spells)
//!   ─▶ the newest change that touched the path: did it remove the page?
//!   ─▶ yes: the removal (its change, its proposal, its summary), and the way back
//!        a rollback of that change, when it merges cleanly over the tip
//!        else the page's last text, as a proposal that writes the page again
//! ```
//!
//! **The difference is read from the history and nowhere else, so it cannot be forged**: a title that was never a page has
//! no change that touched its path (the answer stays *no such page*), a page that was removed has the change that did it,
//! and a page that was moved away is not a removed page (its last change is the move). The history is read as the page's
//! history pages read it, in steps of the same bounds ([`MAX_REMOVAL_STEPS`] of them, so that a title that is not a page
//! costs a bounded number of `show` calls the first time and none after, the blocks being held by id), and a removal older
//! than that is answered as before, *no such page*.
//!
//! **One place says why.** What the removal's reason is, is [`RemovedPage::reason`] and nothing else: the surfaces ask it,
//! never the change's summary directly. It wraps the tombstones of RFC 006 D-8: a hidden summary is [`Reason::Hidden`] with
//! the mark of the hiding (the surface shows the tombstone), never the words. The removing change is read through the same
//! masked history as every other surface reads, so a hidden summary is already empty in it and a hidden author is already
//! [`super::history::Who::Hidden`].
//!
//! **A removed page's old text can be hidden, and a way back never brings it back** (INV-13; D-8, D-10). The page's last text
//! is not a tip, so moderation may hide it; the way back would copy it into a draft, then into the tip. So the way back is
//! offered only when the text to be written again is not hidden, which the one reader of a text at a block says
//! ([`ReadService::text_at`]: the change that last wrote the page there has its content hidden, whichever change that is):
//! [`Restore::TextHidden`] says which pages, and the form that restores refuses. Only the *content* matters: a hidden summary or author of that
//! revision leaves the text free to come back (the draft's own summary is the service's).

use inklkunn_markup::path::{path_to_title, title_to_path};
use inklkunn_store::WikiRecord;

use super::history::{ChangeInfo, PageEvent};
use super::rollback::Planned;
use super::{BlockText, Cause, Lookup, ReadError, ReadService, Tip};
use crate::tombstone::Mark;

/// The most steps of a page's history read to find out that a title is a removed page. A step inspects at most
/// [`super::MAX_INSPECTED_PER_REQUEST`] blocks and asks prikk for at most [`super::MAX_SHOWS_PER_REQUEST`] it does not
/// hold.
pub const MAX_REMOVAL_STEPS: usize = 3;

/// The way back for a removed page, as it is offered now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restore {
    /// A rollback of the change that removed the page (RFC 004 D-8): it merges cleanly over the tip, so it can be
    /// accepted as it stands. It restores what the change took (every page it removed, as the parent had them).
    Rollback {
        /// The block it undoes (64 lower-case hex): the removal.
        undoes: String,
    },
    /// Later changes block a rollback (the change did more than write pages, or something it touched was changed again),
    /// so the offer is a proposal that writes the page again with its last text.
    LastText {
        /// The block the text is read at (the removal's parent: the last state the page had).
        from: String,
    },
    /// **The last text cannot be read as text** (it is not UTF-8, or it is larger than a page may be): no way back is
    /// offered, and the person starts the page anew.
    NotText,
    /// **The text that would come back was hidden by moderation** (RFC 006 D-8; INV-13): no way back is offered, and a request
    /// for one is refused. The person starts the page anew. For a rollback of a change that wrote several pages, any one hidden
    /// refuses the whole.
    TextHidden {
        /// The titles of the pages whose text is hidden: the titles only, never any of the text.
        titles: Vec<String>,
    },
}

/// **Why a page was removed**, as it may be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason<'a> {
    /// The words of the removing change's summary.
    Given(&'a str),
    /// The summary is hidden by moderation: the surface shows the tombstone of this hiding in place of the words.
    Hidden(&'a Mark),
    /// The change gives none.
    NotGiven,
}

/// A page whose history ends with its removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedPage {
    /// The title asked for, trimmed.
    pub title: String,
    /// The path the title spells: the page's storage path.
    pub path: String,
    /// The tip the question was asked at.
    pub tip: Tip,
    /// The change that removed it: its number, its block, the proposal its message names and its summary.
    pub removal: ChangeInfo,
}

impl RemovedPage {
    /// **The reason the page was removed**, as it is shown: the one-line summary of the removing change (the proposal's
    /// summary, as the history shows it), the mark of the hiding when moderation hid that summary (the words are then never
    /// given), or none when the change says none. This is the single accessor every surface of a removed page reads, so that
    /// the tombstone over a hidden summary (RFC 006 D-8, G-T4) is made here and no surface has a second way to the words.
    pub fn reason(&self) -> Reason<'_> {
        if let Some(mark) = self.removal.hidden.summary() {
            return Reason::Hidden(mark);
        }
        match Some(self.removal.summary.as_str()).filter(|s| !s.is_empty()) {
            Some(words) => Reason::Given(words),
            None => Reason::NotGiven,
        }
    }
}

impl ReadService {
    /// Is the title `title`, which leads to no page at the tip of `view`, a page that was removed? `Ok(None)` when it is not
    /// (it was never a page, was moved, or its removal is older than the history this reads), and when the title leads to a
    /// page after all. The caller asks only after [`ReadService::page`] answered *missing*. **Only the removal is read**: the
    /// calls are a constant of the history's steps, however many pages the removing change removed (the way back is
    /// [`ReadService::way_back`]).
    pub fn removed_page(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
    ) -> Result<Option<RemovedPage>, ReadError> {
        let path = title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        if !matches!(index.lookup(title), Lookup::Missing) {
            return Ok(None);
        }
        let title = title.trim().to_owned();
        let mut after = None;
        let mut removal = None;
        for _ in 0..MAX_REMOVAL_STEPS {
            let step = self.history_step(wiki, tip.clone(), path.clone(), title.clone(), after)?;
            if let Some(row) = step.rows.first() {
                if row.events.last() == Some(&PageEvent::Deleted) {
                    removal = Some(row.change.clone());
                }
                // The newest change that touched the path decides: nothing older is looked at.
                break;
            }
            match step.older {
                Some(older) => after = Some(older),
                None => break,
            }
        }
        let Some(removal) = removal else {
            return Ok(None);
        };
        Ok(Some(RemovedPage {
            title,
            path,
            tip,
            removal,
        }))
    }

    /// **The way back for a removed page**, as it is offered now: a rollback of the removing change when it merges cleanly
    /// over the tip, else the page's last text, else nothing; and **nothing but a refusal when the text that would be written
    /// again is hidden**. It is the costly half of a removed page (see the module's account), so it is asked on the form that
    /// restores, behind the `propose` guard, and by no answer that a visitor gets for the asking. The texts are read through
    /// the one reader ([`ReadService::text_at`]), which says which of them moderation hides, whichever change wrote them.
    pub fn way_back(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        removed: &RemovedPage,
    ) -> Result<Restore, ReadError> {
        let (tip, removal, path) = (&removed.tip, &removed.removal, removed.path.as_str());
        // Whether the rollback would merge over the tip is asked of the files (the verdict is all that comes back, never a text);
        // when it would and the text it would write is hidden, the way back is refused whole, naming the pages.
        match self.plan_of(wiki, view, &removal.block_id)? {
            Planned::Plan(plan) => {
                let paths: Vec<String> = plan.files.iter().map(|f| f.path.clone()).collect();
                if let Some(parent) = &removal.parent
                    && self.rollback_merges(wiki, &plan.undoes, parent, &tip.block_id, &paths)?
                {
                    return Ok(Restore::Rollback {
                        undoes: plan.undoes,
                    });
                }
            }
            Planned::Hidden {
                titles,
                undoes,
                parent,
                paths,
            } => {
                if self.rollback_merges(wiki, &undoes, &parent, &tip.block_id, &paths)? {
                    return Ok(Restore::TextHidden { titles });
                }
            }
            Planned::No(_) => {}
        }
        // A removal is never the first block of a history (it removes what an earlier block wrote), so one without a parent is
        // not a history this reads: it is a read that failed, not a sentence for a reader.
        let Some(parent) = removal.parent.as_deref() else {
            return Err(ReadError::CouldNotBeReadNow(Cause::Scratch(format!(
                "the change {} removed a page and has no parent block",
                removal.block_id
            ))));
        };
        Ok(match self.text_at(wiki, view, parent, path)? {
            BlockText::Text(_) => Restore::LastText {
                from: parent.to_owned(),
            },
            BlockText::Hidden(_) => Restore::TextHidden {
                titles: vec![path_to_title(path).unwrap_or_else(|| path.to_owned())],
            },
            BlockText::Missing | BlockText::NotText | BlockText::TooLarge => Restore::NotText,
        })
    }
}
