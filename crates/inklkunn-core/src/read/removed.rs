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
//! never the change's summary directly. The tombstones of RFC 006 D-8 (a hidden summary) wrap this one function.

use inklkunn_markup::path::title_to_path;
use inklkunn_store::{FileChange, WikiRecord};

use super::history::{ChangeInfo, PageEvent};
use super::{Lookup, ReadError, ReadService, RollbackOutcome, Tip};
use crate::merge::{PageText, merge};

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
    /// Neither can be made (the text is not text, or is larger than a page may be): the person starts the page anew.
    NotOffered,
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
    /// The way back.
    pub restore: Restore,
}

impl RemovedPage {
    /// **The reason the page was removed**, as it is shown: the one-line summary of the removing change (the proposal's
    /// summary, as the history shows it), or `None` when the change says none. This is the single accessor every surface
    /// of a removed page reads, so that a tombstone over a hidden summary (RFC 006 D-8, G-T4a) wraps it here and no
    /// surface has a second way to the words.
    pub fn reason(&self) -> Option<&str> {
        Some(self.removal.summary.as_str()).filter(|s| !s.is_empty())
    }
}

impl ReadService {
    /// Is the title `title`, which leads to no page at the tip of `view`, a page that was removed? `Ok(None)` when it is not
    /// (it was never a page, was moved, or its removal is older than the history this reads), and when the title leads to a
    /// page after all. The caller asks only after [`ReadService::page`] answered *missing*.
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
        let restore = self.restore_of(wiki, view, &tip, &removal, &path)?;
        Ok(Some(RemovedPage {
            title,
            path,
            tip,
            removal,
            restore,
        }))
    }

    /// The way back for the page at `path` that `removal` removed: a rollback when it merges cleanly over `tip`, else the
    /// last text, else nothing.
    fn restore_of(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        tip: &Tip,
        removal: &ChangeInfo,
        path: &str,
    ) -> Result<Restore, ReadError> {
        if let RollbackOutcome::Plan(plan) = self.rollback_plan(wiki, view, &removal.block_id)?
            && self.merges_over(wiki, tip, &plan.undoes, &plan.files)?
        {
            return Ok(Restore::Rollback {
                undoes: plan.undoes,
            });
        }
        let Some(parent) = removal.parent.as_deref() else {
            return Ok(Restore::NotOffered);
        };
        Ok(match self.text_at(wiki, parent, path)? {
            Ok(Some(_)) => Restore::LastText {
                from: parent.to_owned(),
            },
            _ => Restore::NotOffered,
        })
    }

    /// Would the pages of a rollback plan merge cleanly over the tip (RFC 004 D-5, the merge the acceptance runs): the base
    /// is the pages at the undone block, *theirs* the pages at the tip, *ours* the plan's. Only the pages the plan writes
    /// are asked, so a change anywhere else is no obstacle.
    fn merges_over(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        undoes: &str,
        files: &[FileChange],
    ) -> Result<bool, ReadError> {
        let (mut base, mut theirs, mut ours) = (Vec::new(), Vec::new(), Vec::new());
        for file in files {
            // A page the service cannot read as text at either end is not one it can merge.
            let (Ok(was), Ok(now)) = (
                self.text_at(wiki, undoes, &file.path)?,
                self.text_at(wiki, &tip.block_id, &file.path)?,
            ) else {
                return Ok(false);
            };
            if let Some(text) = was {
                base.push(PageText::new(file.path.clone(), text));
            }
            if let Some(text) = now {
                theirs.push(PageText::new(file.path.clone(), text));
            }
            if let Some(text) = &file.text {
                ours.push(PageText::new(file.path.clone(), text.clone()));
            }
        }
        Ok(merge(&base, &theirs, &ours).is_clean())
    }
}
