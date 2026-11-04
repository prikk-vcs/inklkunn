//! The claims a change touches and what it does to what they were judged on (RFC 005 part 1 D-1 c; part 2 D-3).
//!
//! A claim is *touched* by a change when a paragraph it is said in is reworded or goes. The service states the
//! observable facts and never suggests whether the claim is "the same": a reworded paragraph keeps its `^id`, and
//! so its claim, by the marker alone (D-1 a). What it can say is the **consequence** (D-1 point 3): the sources
//! and the status that were judged on the wording the paragraph had, and that the new wording is not, will be shown
//! as *judged on an earlier wording* until people judge them again.
//!
//! Pure: the pages as they were and as the change leaves them, the claims, and the digests the rest of the view
//! has. Which claims to ask about, and the reading, are [`super::onpage`]'s.

use std::collections::BTreeSet;

use super::compose::page_id_of;
use super::marker::words_with_id;
use super::{AnchorState, Claim, ClaimId, Digest, Expression, SourceId, Standing, anchor_state};

/// A page a change writes: where, and its text before and after (`None`: not there).
#[derive(Debug, Clone, Copy)]
pub struct PageChange<'a> {
    /// The page's path.
    pub path: &'a str,
    /// Its text at the base.
    pub old: Option<&'a str>,
    /// Its text as the change leaves it.
    pub new: Option<&'a str>,
}

impl PageChange<'_> {
    /// The page's `id`, from either side.
    pub fn page_id(&self) -> Option<String> {
        self.old
            .and_then(page_id_of)
            .or(self.new.and_then(page_id_of))
    }
}

/// What happens to the paragraph a claim is said in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// Its words change; its marker stays, and so does the claim.
    Reworded,
    /// It is not in the page any more.
    Removed,
}

/// A claim a change touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Touch {
    /// The claim.
    pub claim: ClaimId,
    /// What happens to the paragraph.
    pub how: How,
    /// The page's path.
    pub path: String,
    /// The paragraph's `^id`.
    pub block: String,
    /// The paragraph's words after the change (before it, when removed), without its marker.
    pub words: String,
    /// The sources judged on the wording the paragraph had and not on the one it will have.
    pub links: Vec<SourceId>,
    /// The community status, when it was decided on the wording the paragraph had and not on the new one.
    pub standing: Option<Standing>,
}

/// The blocks of `change` that are in `blocks` and whose words change or go.
pub fn changed_blocks(change: &PageChange<'_>, blocks: &BTreeSet<String>) -> Vec<String> {
    blocks
        .iter()
        .filter(|block| {
            let before = change.old.map(|t| anchor_state(t, block));
            let after = change.new.map(|t| anchor_state(t, block));
            matches!(&before, Some(AnchorState::Present(a)) if !matches!(&after, Some(AnchorState::Present(b)) if a == b))
        })
        .cloned()
        .collect()
}

/// The digest of an expression's paragraph in the pages a change writes, or the view's (`rest`) when it does not
/// write the page.
fn digest_in(
    e: &Expression,
    changes: &[PageChange<'_>],
    after: bool,
    rest: &dyn Fn(&Expression) -> Option<Digest>,
) -> Option<Digest> {
    match changes
        .iter()
        .find(|c| c.page_id().as_deref() == Some(e.page_id.as_str()))
    {
        Some(c) => {
            let text = if after { c.new } else { c.old }?;
            match anchor_state(text, &e.block) {
                AnchorState::Present(d) => Some(d),
                _ => None,
            }
        }
        None => rest(e),
    }
}

/// What the change does to `claim`, if it touches it: `None` when every paragraph the claim is said in keeps its
/// words. `rest` gives the digest of an expression in a page the change does not write.
pub fn judge(
    claim: &Claim,
    changes: &[PageChange<'_>],
    rest: &dyn Fn(&Expression) -> Option<Digest>,
) -> Option<Touch> {
    let (before, after): (BTreeSet<Digest>, BTreeSet<Digest>) = (
        claim
            .expressions
            .iter()
            .filter_map(|e| digest_in(e, changes, false, rest))
            .collect(),
        claim
            .expressions
            .iter()
            .filter_map(|e| digest_in(e, changes, true, rest))
            .collect(),
    );
    // The first expression, in the claim's order, whose paragraph changes its words or goes.
    let (e, how, change) = claim.expressions.iter().find_map(|e| {
        let change = changes
            .iter()
            .find(|c| c.page_id().as_deref() == Some(e.page_id.as_str()))?;
        let was = digest_in(e, changes, false, rest)?;
        match digest_in(e, changes, true, rest) {
            Some(now) if now == was => None,
            Some(_) => Some((e, How::Reworded, change)),
            None => Some((e, How::Removed, change)),
        }
    })?;
    let was_current = |d: &Digest| before.contains(d) && !after.contains(d);
    let text = match how {
        How::Reworded => change.new,
        How::Removed => change.old,
    };
    Some(Touch {
        claim: claim.id.clone(),
        how,
        path: change.path.to_owned(),
        block: e.block.clone(),
        words: text
            .and_then(|t| words_with_id(t, &e.block, true))
            .unwrap_or_default(),
        links: claim
            .sources
            .iter()
            .filter(|l| was_current(&l.judged_on))
            .map(|l| l.source.clone())
            .collect(),
        standing: (claim.community_status.status != Standing::Unknown
            && claim
                .community_status
                .judged_on
                .as_ref()
                .is_some_and(was_current))
        .then_some(claim.community_status.status),
    })
}
