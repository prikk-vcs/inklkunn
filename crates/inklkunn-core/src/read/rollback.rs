//! The inverse of a change, at content level (RFC 004 D-8; handoff 004 T6): *Propose a rollback* begins here.
//!
//! ```text
//! the change (a block of the view) ─▶ the pages it touched (one `show`)
//!   ─▶ each page's text at the block and at the block before it (`cat`) ─▶ the pages that differ
//!   ─▶ the plan: those pages as the parent had them, written against the undone block
//! ```
//!
//! **The plan restores the pages as they were before the change** — their text exactly, a page the change
//! created removed, a page it removed written again, a page it moved put back where it was (a move is the
//! removal of the page at its new path and the page at its old one; the merge matches them by page id).
//! History is never rewritten: the plan is the text of a proposal, and the proposal goes through review and
//! acceptance like any other.
//!
//! **The base is the undone block itself**, not the tip. The merge (D-5) reads a proposal's base as "the pages
//! the proposer saw", takes the tip as *theirs* and the proposal as *ours*; with the undone block as the base,
//! a rollback is the three-way edit *"take back the change this block made"*. Pages that later changes edited
//! in other places keep those edits; a later change to the very text the undone change made is an overlap,
//! which a person decides (MRG-2), instead of an older text silently written over newer work. A rollback of a
//! change that is still the tip is the same thing with nothing in between. The base also becomes the
//! message's `Base:` trailer, so the history names the undone block (CT-03).
//!
//! **A rollback of a rollback restores the change**, because an accepted rollback is a block like any other:
//! its inverse is the pages as they were before it, which are the pages the first change left.

use std::collections::BTreeSet;

use inklkunn_markup::path::{PAGES_DIR, path_to_title};
use inklkunn_prikk::{Change as PrikkChange, StorageError};
use inklkunn_store::{Base, FileChange, WikiRecord};

use super::history::ChangeInfo;
use super::writes::RequestBudget;
use super::{BlockText, ReadError, ReadService};
use crate::message::SUMMARY_MAX_CHARS;

/// The most pages one rollback restores. A change that touched more is left to a person to undo by hand.
pub const MAX_ROLLBACK_PAGES: usize = 200;

/// The proposal that undoes a change: what the pages should say, and what it is made against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackPlan {
    /// The block it undoes (64 lower-case hex).
    pub undoes: String,
    /// The undone change's number in the view's history.
    pub number: u64,
    /// The undone change's summary line, as the history gives it (untrusted text).
    pub undone_summary: String,
    /// What the proposal is written against: the undone block's state and the block itself.
    pub base: Base,
    /// The pages, in path order: the text they had before the change, or their removal when the change
    /// created them.
    pub files: Vec<FileChange>,
    /// The proposal's summary line: *Undo: change {number}*, which quotes nothing of the undone change (H-7).
    pub summary: String,
}

/// Why a change could not be turned into a rollback. Each is a plain fact a surface chooses its words by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoRollback {
    /// The view's history has no such block.
    NoSuchChange,
    /// The first change of a view has nothing before it to go back to.
    FirstChange,
    /// The change did something that is not an edit of a page's text (a file that is not a page, a link, a
    /// binary file, a change of mode): it cannot be undone by writing pages.
    NotOnlyPages,
    /// A page the change touched is not text, or is larger than a page may be.
    NotText,
    /// The change touched more than [`MAX_ROLLBACK_PAGES`] pages, or prikk cannot describe it within its
    /// bounds.
    TooLarge,
    /// The pages are the same before and after the change (it edited and put back): nothing to undo.
    NothingToUndo,
    /// The block's parent is not within the part of the log this service reads.
    ParentNotKnown,
    /// **The text the rollback would write again is hidden by moderation** (RFC 006 D-8; INV-13; decision 22): the pages whose text
    /// as it was before the change is hidden, by title (never any of the text). A rollback of a change that wrote several pages is
    /// refused whole when any one is hidden. The text of the change being undone being hidden is no obstacle: it is taken away,
    /// never written.
    TextHidden {
        /// The titles of the pages whose text is hidden.
        titles: Vec<String>,
    },
}

/// What planning a rollback came to, before the pages whose text is hidden are made a refusal.
pub(super) enum Planned {
    /// The plan.
    Plan(Box<RollbackPlan>),
    /// There is none, and why.
    No(NoRollback),
    /// The text of some pages that the rollback would write again is hidden: no plan is made. What is kept is what asking whether
    /// it would have merged needs (the paths and the blocks, never a text).
    Hidden {
        titles: Vec<String>,
        undoes: String,
        parent: String,
        /// Every page the rollback writes, the hidden ones too.
        paths: Vec<String>,
    },
}

/// What asking for a rollback came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackOutcome {
    /// The plan.
    Plan(Box<RollbackPlan>),
    /// There is none, and why.
    No(NoRollback),
}

/// A page path: under `pages/`, a Markdown file.
fn is_page_path(path: &str) -> bool {
    path.strip_prefix(PAGES_DIR)
        .is_some_and(|rest| rest.starts_with('/'))
        && path.ends_with(inklkunn_markup::path::EXTENSION)
}

/// The pages a change touched, or why it cannot be undone as pages. Only the operations that write a page's
/// text, create it, remove it or move it are undone; anything else names no inverse.
fn touched(change: &PrikkChange) -> Result<BTreeSet<String>, NoRollback> {
    let mut paths = BTreeSet::new();
    for patch in &change.patches {
        for op in &patch.operations {
            if !matches!(
                op.kind.as_str(),
                "create-file" | "edit-text" | "delete-node" | "rename-path"
            ) {
                return Err(NoRollback::NotOnlyPages);
            }
            for reference in &op.paths {
                // A path prikk could not name is a path this service cannot read back.
                let Some(path) = reference.path.as_deref() else {
                    return Err(NoRollback::NotOnlyPages);
                };
                if !is_page_path(path) {
                    return Err(NoRollback::NotOnlyPages);
                }
                paths.insert(path.to_owned());
            }
        }
    }
    Ok(paths)
}

/// One line for a summary: at most 120 characters, a cut one ending in an ellipsis.
pub(super) fn fit(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| {
            if inklkunn_store::record::is_line_or_control(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() <= SUMMARY_MAX_CHARS {
        return flat.to_owned();
    }
    let mut cut: String = flat.chars().take(SUMMARY_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// The summary of a rollback: **it names the change by its number and quotes nothing** (RFC 006 D-8; H-7). The summary is
/// written into the rollback's own message, and what prikk holds is never taken back (RR-15), so a summary that is hidden
/// later must not have been copied there. (Rollbacks made before this keep the words they quoted; see [`quoted_summary`].)
fn summary_of(info: &ChangeInfo) -> String {
    format!("Undo: change {}", info.number)
}

/// Is `summary` what a rollback made before H-7 wrote for a change whose summary was `hidden`: *Undo: * and those words, cut to the
/// line the message allows?
pub(crate) fn quoted_summary(summary: &str, hidden: &str) -> bool {
    !hidden.trim().is_empty() && summary == fit(&format!("Undo: {hidden}"))
}

impl ReadService {
    /// The plan that undoes the change `block_id` of the view (the default view for `None`): the pages it
    /// touched, as they were before it. Reads the change once (`show`) and each touched page twice (`cat`). **One request, one budget**
    /// for the part it has in building the index of who wrote each page, whatever the number of pages (correction Q-3).
    pub fn rollback_plan(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
    ) -> Result<RollbackOutcome, ReadError> {
        Ok(
            match self.plan_of(wiki, view, block_id, &self.request_budget())? {
                Planned::Plan(plan) => RollbackOutcome::Plan(plan),
                Planned::No(why) => RollbackOutcome::No(why),
                Planned::Hidden { titles, .. } => {
                    RollbackOutcome::No(NoRollback::TextHidden { titles })
                }
            },
        )
    }

    /// What planning the rollback of the change came to, **with the pages whose text is hidden told apart**: what the offer of a
    /// way back for a removed page decides by (it asks whether the rollback would have merged, to say which way back is refused).
    pub(super) fn plan_of(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        block_id: &str,
        budget: &RequestBudget,
    ) -> Result<Planned, ReadError> {
        let no = |why| Ok(Planned::No(why));
        let Some(info) = self.change_info(wiki, view, block_id)? else {
            return no(NoRollback::NoSuchChange);
        };
        if info.first {
            return no(NoRollback::FirstChange);
        }
        let Some(parent) = info.parent.clone() else {
            return no(NoRollback::ParentNotKnown);
        };
        let shown = self
            .storage
            .change(&self.layout.repo_dir(&wiki.id), block_id);
        let change = match shown {
            Ok(change) => change,
            Err(StorageError::Unavailable {
                reason: inklkunn_prikk::Unavailable::OutputExceeded,
                ..
            }) => return no(NoRollback::TooLarge),
            Err(e) => return Err(self.storage_error(wiki, e)),
        };
        let paths = match touched(&change) {
            Ok(paths) if paths.len() > MAX_ROLLBACK_PAGES => return no(NoRollback::TooLarge),
            Ok(paths) => paths,
            Err(why) => return no(why),
        };
        let mut files = Vec::new();
        let mut hidden = Vec::new();
        let mut withheld = Vec::new();
        for path in paths {
            // A path whose title the wiki could not spell is not a page it can name.
            let Some(title) = path_to_title(&path) else {
                return no(NoRollback::NotOnlyPages);
            };
            // The text after the change is compared and never given: when it is hidden (the change being undone is the one
            // moderation hid) the page is taken as one the change changed, which the plan then writes as it was before.
            let after = match self.text_at_within(wiki, view, block_id, &path, budget)? {
                BlockText::Text(text) => Some(Some(text)),
                BlockText::Missing => Some(None),
                BlockText::NotText | BlockText::TooLarge => return no(NoRollback::NotText),
                BlockText::Hidden(_) => None,
            };
            // The text before it is what the plan writes: it is never hidden text.
            let before = match self.text_at_within(wiki, view, &parent, &path, budget)? {
                BlockText::Text(text) => Some(text),
                BlockText::Missing => None,
                BlockText::NotText | BlockText::TooLarge => return no(NoRollback::NotText),
                BlockText::Hidden(_) => {
                    hidden.push(title);
                    withheld.push(path);
                    continue;
                }
            };
            if after.as_ref() != Some(&before) {
                files.push(FileChange { path, text: before });
            }
        }
        if !hidden.is_empty() {
            return Ok(Planned::Hidden {
                titles: hidden,
                undoes: info.block_id,
                parent,
                paths: files
                    .into_iter()
                    .map(|file| file.path)
                    .chain(withheld)
                    .collect(),
            });
        }
        if files.is_empty() {
            return no(NoRollback::NothingToUndo);
        }
        let Some(base) = Base::new(&info.state_id, &info.block_id) else {
            return no(NoRollback::NoSuchChange);
        };
        Ok(Planned::Plan(Box::new(RollbackPlan {
            undoes: info.block_id.clone(),
            number: info.number,
            summary: summary_of(&info),
            undone_summary: info.summary,
            base,
            files,
        })))
    }
}
