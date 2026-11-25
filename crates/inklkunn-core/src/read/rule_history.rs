//! The history of the rules of a wiki: the changes of the default view that changed a governance file, newest first, each with
//! the proposal that made it and what it did to each file (RFC 006 D-11, *why the rules are what they are*; handoff G-T7).
//!
//! **It is the loader's own walk, read once more.** The rules in force are found by walking the default view's newest
//! [`WALK_BLOCKS`] blocks for the one that last changed a governance file ([`ReadService::rules_of`]); this walks the same
//! window, with the same question put to each block (*did it change one of the two files, or something at or below their place?*,
//! answered once and remembered), and keeps every block that did instead of stopping at the first. So its bound is the loader's,
//! it asks prikk nothing the loader does not, and **older changes than the window are not listed**
//! ([`RuleHistory::window_ended`] says the window was spent before the history was).
//!
//! **What a change did** is the two files' text before it and after it, read at the block and at the block before it with
//! [`ReadService::source_or_hidden`], the reader of a file at a block that says *hidden* instead of failing: the history reads
//! nothing of its own. A file whose text is the same on both sides is not part of the change. The surfaces say each change in
//! sentences, and never show the text.
//!
//! **Moderation hides one change, not the history.** A change whose own text of a rules file is hidden, or whose text before it
//! is (the file as the change before it left it), is listed with [`RuleChange::hidden`] and no sentences; the others are listed
//! as they were. An error of the store that says what is hidden is still an error of the whole history (*could not be read
//! now*): nothing is said when it cannot be known whether it may be.
//!
//! **The proposal** is the one the block's message names (the message is the service's record, CT-03). A change whose message is
//! not in that form (an import, a block sealed outside inklkunn) names none.

use inklkunn_prikk::Point;
use inklkunn_store::WikiRecord;

use super::rules::WALK_BLOCKS;
use super::{ReadError, ReadService};
use crate::governance::GovernanceFile;
use crate::message::Message;

/// The most changes one history lists; the rest of the window is counted ([`RuleHistory::more`]).
pub const MAX_RULE_CHANGES: usize = 20;

/// What one change did to one governance file: the file's text before it and after it. `None` is *no file* (or one that is not
/// text): a file the change wrote for the first time has no `before`, one it removed has no `after`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFileChange {
    /// Which file.
    pub file: GovernanceFile,
    /// Its text before the change.
    pub before: Option<String>,
    /// Its text after the change.
    pub after: Option<String>,
}

/// Which text of a change of the rules moderation hides, so that what the change did cannot be said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleHidden {
    /// The text of a rules file as this change wrote it.
    ThisChange,
    /// The text of a rules file as the change before this one left it (and this change's own text is not hidden).
    EarlierChange,
}

/// One change of the default view that changed the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleChange {
    /// The block.
    pub block_id: String,
    /// The block's number in its view (prikk's update sequence).
    pub number: u64,
    /// The proposal that made it, when the block's message names one.
    pub proposal: Option<String>,
    /// What it did to each governance file whose text it changed. **Empty** for a change that only cleared or made something at
    /// or below a file's place (a repair), and for a change whose text is hidden ([`RuleChange::hidden`]).
    pub files: Vec<RuleFileChange>,
    /// Moderation hides text that this change's sentences would say: none of it is read into the change, and the surfaces say
    /// so in the change's place.
    pub hidden: Option<RuleHidden>,
}

/// The changes of the rules, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuleHistory {
    /// At most [`MAX_RULE_CHANGES`] changes, newest first.
    pub changes: Vec<RuleChange>,
    /// How many more changes of the rules the window holds than are listed.
    pub more: usize,
    /// The window of the newest [`WALK_BLOCKS`] blocks ended before the view's history did: there may be older changes that are not
    /// listed or counted.
    pub window_ended: bool,
}

impl ReadService {
    /// The changes of the default view's newest [`WALK_BLOCKS`] blocks that changed a governance file, newest first, at most
    /// [`MAX_RULE_CHANGES`] of them with what each did, and the count of the rest of the window.
    ///
    /// A wiki that has never had a governance file has none. An error is *could not be read now*: the page that asks says so and
    /// is otherwise whole.
    pub fn rule_history(&self, wiki: &WikiRecord) -> Result<RuleHistory, ReadError> {
        let tip = self.tip(wiki, None)?;
        let repo = self.layout.repo_dir(&wiki.id);
        let history = self
            .storage
            .history(
                &repo,
                &Point::View(tip.view().to_owned()),
                Some(WALK_BLOCKS),
            )
            .map_err(|e| self.storage_error(wiki, e))?;
        // The window starts at the tip the service held, as the loader's does.
        let Some(start) = history
            .blocks
            .iter()
            .position(|b| b.block_id == tip.block_id())
        else {
            return Err(ReadError::CouldNotBeReadNow(super::Cause::Scratch(
                "the view moved while its rules were listed".to_owned(),
            )));
        };
        let blocks = history.blocks.get(start..).unwrap_or(&[]);
        // One request: every file of every change draws from the one budget (correction Q-3).
        let budget = self.request_budget();
        let mut out = RuleHistory {
            window_ended: u64::try_from(blocks.len()).is_ok_and(|n| n >= WALK_BLOCKS),
            ..RuleHistory::default()
        };
        for (n, block) in blocks.iter().enumerate() {
            if !self.changed_governance(wiki, &repo, &block.block_id)? {
                continue;
            }
            if out.changes.len() >= MAX_RULE_CHANGES {
                out.more += 1;
                continue;
            }
            // The block before it, in the view: none for the first block.
            let first = block.kind == "Root" || block.previous_ref_state_id.is_none();
            let parent = if first {
                None
            } else {
                blocks.get(n + 1).map(|b| b.block_id.as_str())
            };
            let proposal = block
                .patch_messages
                .iter()
                .find_map(|pm| Message::parse(&pm.message).ok())
                .and_then(|m| m.proposal_id().map(|p| p.as_str().to_owned()));
            let mut files = Vec::new();
            let mut hidden = None;
            for file in GovernanceFile::ALL {
                let after =
                    self.source_or_hidden_within(wiki, &block.block_id, file.path(), &budget)?;
                let before = match parent {
                    Some(parent) => {
                        self.source_or_hidden_within(wiki, parent, file.path(), &budget)?
                    }
                    None => Ok(None),
                };
                match (before, after) {
                    (_, Err(_)) => hidden = Some(RuleHidden::ThisChange),
                    (Err(_), Ok(_)) => hidden = hidden.or(Some(RuleHidden::EarlierChange)),
                    (Ok(before), Ok(after)) if before != after => files.push(RuleFileChange {
                        file,
                        before,
                        after,
                    }),
                    (Ok(_), Ok(_)) => {}
                }
            }
            // Nothing of a change whose text is hidden is said: not even the file that is not.
            if hidden.is_some() {
                files.clear();
            }
            out.changes.push(RuleChange {
                block_id: block.block_id.clone(),
                number: block.update_seq,
                proposal,
                files,
                hidden,
            });
        }
        Ok(out)
    }
}
