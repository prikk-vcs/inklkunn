//! What moderation hides from an export (RFC 006 D-13, D-8's last bullet): the bundle carries the tombstone and never the
//! hidden part.
//!
//! **The marker.** An entry of the bundle that has something hidden says so in two members, written the same way in every
//! file of the bundle that carries one (`history/<page id>.jsonl` and the claim threads):
//!
//! - `suppressed`: the parts that are hidden, a list of `"content"`, `"summary"` and `"author"` in that order;
//! - `reasons`: an object from each of those parts to the reason category it was hidden for, in the store's own spelling
//!   (`personal-data`, `harassment`, `unlawful`, `copyright`, `spam`, `other`). It is an object and not one word because
//!   two hides of one item may have two reasons (the content for `personal-data`, the author for `harassment`).
//!
//! and **omits** what is hidden: a hidden `content` is a thread comment's `body` (a page revision's text is not in a
//! history line at all, so its marker is the whole of what the line says about it); a hidden `summary` is the line's
//! `summary`; a hidden `author` is the line's `authors` (a comment's `author`), and the person is listed in `users.yaml`
//! only for what is not hidden.
//!
//! **What an item is.** A page revision is named by the block that made it; a hide may name one page of a block that changed
//! several. The `content` of a revision is the page's, so a hide that names a page marks that page's lines only. The
//! `summary` and the `authors` of a history line are the *change's* (the patch message is one text for every page it
//! touched), so a hide of either on any page of a block hides it on every line of that block and keeps the person out of
//! `users.yaml` for that block: a summary or a name that is hidden on one page and shown on its neighbour is not hidden.
//! The revision of a proposal that was accepted is the page revision of the block that acceptance sealed (one content, RFC 006
//! D-8), so a hide of it is read as a hide of that block.
//!
//! **An unhide** ends the hide it follows ([`hidden_parts`] reads that); an item with nothing hidden now writes exactly what
//! it wrote before it was hidden.

use std::collections::{BTreeMap, HashMap};

use inklkunn_store::governance::MAX_PAGE;
use inklkunn_store::{
    AcceptanceStore, GovernanceStore, HiddenItem, HiddenItemKind, HiddenParts, ReasonCategory,
    TombstoneAct, TombstoneEvent, WikiId, hidden_parts,
};

use super::ExportError;

/// The parts of one entry that are hidden now, each with the reason it was hidden for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Suppressed {
    /// The content: a thread comment's body, a page revision's text.
    pub content: Option<ReasonCategory>,
    /// The summary of the change.
    pub summary: Option<ReasonCategory>,
    /// The author: who proposed and who accepted.
    pub author: Option<ReasonCategory>,
}

impl Suppressed {
    /// Is nothing hidden?
    pub const fn is_empty(&self) -> bool {
        self.content.is_none() && self.summary.is_none() && self.author.is_none()
    }

    /// The parts that are hidden, in the bundle's order, each with its reason.
    pub fn parts(&self) -> Vec<(&'static str, ReasonCategory)> {
        [
            ("content", self.content),
            ("summary", self.summary),
            ("author", self.author),
        ]
        .into_iter()
        .filter_map(|(name, reason)| reason.map(|r| (name, r)))
        .collect()
    }

    /// What `self` hides and `other` hides, `self`'s reason where both hide a part.
    #[must_use]
    pub fn or(self, other: Suppressed) -> Suppressed {
        Suppressed {
            content: self.content.or(other.content),
            summary: self.summary.or(other.summary),
            author: self.author.or(other.author),
        }
    }

    /// Only the parts that belong to a change as a whole (its summary and its author), not to one page of it.
    #[must_use]
    pub fn of_the_change(self) -> Suppressed {
        Suppressed {
            content: None,
            ..self
        }
    }

    /// The entry's marker members (`suppressed`, `reasons`), written into a JSON line. Nothing is written when nothing is
    /// hidden, so an entry with nothing hidden is the line it always was.
    pub(crate) fn mark(&self, line: &mut serde_json::Map<String, serde_json::Value>) {
        let parts = self.parts();
        if parts.is_empty() {
            return;
        }
        line.insert(
            "suppressed".to_owned(),
            serde_json::Value::Array(
                parts
                    .iter()
                    .map(|(name, _)| serde_json::Value::from(*name))
                    .collect(),
            ),
        );
        line.insert(
            "reasons".to_owned(),
            serde_json::Value::Object(
                parts
                    .iter()
                    .map(|(name, reason)| {
                        ((*name).to_owned(), serde_json::Value::from(reason.as_str()))
                    })
                    .collect(),
            ),
        );
    }
}

/// What is hidden of one block's revisions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BlockHides {
    /// Hidden for the block as a whole (a hide that names no page; a proposal's accepted revision).
    whole: Suppressed,
    /// Hidden for one page of it, by the page's path.
    by_path: BTreeMap<String, Suppressed>,
}

/// Everything moderation hides that an export can carry, read once for one export: page revisions by the block that made
/// them, and thread comments by their id. Comments on proposals and revisions of proposals that were not accepted are
/// not in a bundle, so a hide of one changes nothing of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suppressions {
    blocks: BTreeMap<String, BlockHides>,
    claim_comments: HashMap<String, Suppressed>,
}

impl Suppressions {
    /// Read every hide and unhide of the wiki and what they leave hidden now. A record that cannot be read fails the
    /// export: a bundle written without it could carry what was hidden.
    pub fn read(
        governance: &dyn GovernanceStore,
        acceptances: &dyn AcceptanceStore,
        wiki: &WikiId,
    ) -> Result<Self, ExportError> {
        let unreadable = || ExportError::Io("the moderation records could not be read".to_owned());
        // The log, newest first, a page at a time; read back to front it is each item's own order.
        let mut log: Vec<TombstoneEvent> = Vec::new();
        loop {
            let page = governance
                .tombstone_log(wiki, log.last().map(|e| &e.id), MAX_PAGE)
                .map_err(|_| unreadable())?;
            let full = page.len() >= MAX_PAGE;
            log.extend(page);
            if !full {
                break;
            }
        }
        log.reverse();
        if log.is_empty() {
            return Ok(Self::default());
        }
        let mut by_item: HashMap<HiddenItem, Vec<TombstoneEvent>> = HashMap::new();
        for event in log {
            by_item.entry(event.item.clone()).or_default().push(event);
        }
        // An accepted proposal's revision is the block its acceptance sealed.
        let sealed: HashMap<(String, String), String> = acceptances
            .sealed_acceptances(wiki)
            .map_err(|_| unreadable())?
            .into_iter()
            .map(|s| {
                (
                    (
                        s.acceptance.proposal.to_string(),
                        s.acceptance.revision.to_string(),
                    ),
                    s.block_id,
                )
            })
            .collect();
        let mut out = Self::default();
        for (item, events) in by_item {
            let hidden = suppressed_now(&events);
            if hidden.is_empty() {
                continue;
            }
            match item.kind {
                HiddenItemKind::PageRevision => {
                    let at = out.blocks.entry(item.id).or_default();
                    match item.reference {
                        Some(path) => {
                            let slot = at.by_path.entry(path).or_default();
                            *slot = slot.or(hidden);
                        }
                        None => at.whole = at.whole.or(hidden),
                    }
                }
                HiddenItemKind::ProposalRevision => {
                    let number = item.reference.unwrap_or_default();
                    if let Some(block) = sealed.get(&(item.id, number)) {
                        let at = out.blocks.entry(block.clone()).or_default();
                        at.whole = at.whole.or(hidden);
                    }
                }
                HiddenItemKind::ClaimComment => {
                    out.claim_comments.insert(item.id, hidden);
                }
                // Not in a bundle (RFC 005 part 2 D-11: comments on proposals are not exported).
                HiddenItemKind::ProposalComment => {}
            }
        }
        Ok(out)
    }

    /// Is nothing hidden that a bundle could carry?
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.claim_comments.is_empty()
    }

    /// What is hidden of the revision of the page at `path` that `block` made.
    pub fn of_revision(&self, block: &str, path: &str) -> Suppressed {
        let Some(at) = self.blocks.get(block) else {
            return Suppressed::default();
        };
        let page = at.by_path.get(path).copied().unwrap_or_default();
        let content = at.whole.or(page);
        // The summary and the author are the change's: any page's hide of either covers every page of the block.
        content.or(self.of_change(block))
    }

    /// What is hidden of the *change* `block` made, whatever page it touched: its summary and its author.
    pub fn of_change(&self, block: &str) -> Suppressed {
        let Some(at) = self.blocks.get(block) else {
            return Suppressed::default();
        };
        at.by_path
            .values()
            .fold(at.whole, |all, page| all.or(*page))
            .of_the_change()
    }

    /// What is hidden of the thread comment `id`.
    pub fn of_claim_comment(&self, id: &str) -> Suppressed {
        self.claim_comments.get(id).copied().unwrap_or_default()
    }
}

/// What the events of one item leave hidden now, with the reasons: the parts [`hidden_parts`] says, each with the reason of
/// the oldest hide that still holds it.
fn suppressed_now(events: &[TombstoneEvent]) -> Suppressed {
    let parts: HiddenParts = hidden_parts(events);
    let holding = |part: fn(&HiddenParts) -> bool| -> Option<ReasonCategory> {
        events
            .iter()
            .filter(|e| e.act == TombstoneAct::Hide && part(&e.parts))
            .filter(|hide| {
                !events
                    .iter()
                    .any(|e| e.act == TombstoneAct::Unhide && e.of_event.as_ref() == Some(&hide.id))
            })
            .find_map(|hide| hide.category)
            // A hide always has a category (the store refuses one without); `other` is the word if it ever did not.
            .or(Some(ReasonCategory::Other))
    };
    Suppressed {
        content: parts.content.then(|| holding(|p| p.content)).flatten(),
        summary: parts.summary.then(|| holding(|p| p.summary)).flatten(),
        author: parts.author.then(|| holding(|p| p.author)).flatten(),
    }
}
