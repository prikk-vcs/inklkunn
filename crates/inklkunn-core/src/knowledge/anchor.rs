//! What a page's block is now: the other half of a claim's expression.

use inklkunn_markup::{blocks, frontmatter};

use super::{Digest, block_digest};

/// What an expression's block is in a page's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorState {
    /// The page has the block once, and this is the digest of its text.
    Present(Digest),
    /// The page has no block with that `^id`.
    BlockMissing,
    /// The page has more than one block with that `^id`: the anchor does not say which.
    BlockRepeated,
}

/// The state of the block `block` (a `^id`, without the caret) in the page whose whole text, frontmatter
/// included, is `page_text`.
pub fn anchor_state(page_text: &str, block: &str) -> AnchorState {
    let (_, body, _) = frontmatter::split(page_text);
    let found = blocks(body);
    let mut named = found.iter().filter(|b| b.id == Some(block));
    match (named.next(), named.next()) {
        (Some(b), None) => AnchorState::Present(block_digest(b)),
        (None, _) => AnchorState::BlockMissing,
        (Some(_), Some(_)) => AnchorState::BlockRepeated,
    }
}
