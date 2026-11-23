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
//! [`ReadService::source_at`], the one reader of a file at a block: the history reads nothing of its own. A file whose text is
//! the same on both sides is not part of the change. The surfaces say each change in sentences, and never show the text.
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
    /// or below a file's place (a repair).
    pub files: Vec<RuleFileChange>,
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
            .history(&repo, &Point::View(tip.view.clone()), Some(WALK_BLOCKS))
            .map_err(|e| self.storage_error(wiki, e))?;
        // The window starts at the tip the service held, as the loader's does.
        let Some(start) = history
            .blocks
            .iter()
            .position(|b| b.block_id == tip.block_id)
        else {
            return Err(ReadError::CouldNotBeReadNow(super::Cause::Scratch(
                "the view moved while its rules were listed".to_owned(),
            )));
        };
        let blocks = history.blocks.get(start..).unwrap_or(&[]);
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
            for file in GovernanceFile::ALL {
                let after = self.source_at(wiki, &block.block_id, file.path())?;
                let before = match parent {
                    Some(parent) => self.source_at(wiki, parent, file.path())?,
                    None => None,
                };
                if before != after {
                    files.push(RuleFileChange {
                        file,
                        before,
                        after,
                    });
                }
            }
            out.changes.push(RuleChange {
                block_id: block.block_id.clone(),
                number: block.update_seq,
                proposal,
                files,
            });
        }
        Ok(out)
    }
}
