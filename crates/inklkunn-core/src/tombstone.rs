//! Tombstones: moderation that hides without erasing (RFC 006 D-8; handoff G-T4; threat model C-14f, C-15d, C-15f, INV-13,
//! RR-15). **What is hidden of an item is a fact in the store; what shows it is every surface's, and every surface asks the
//! same reader.**
//!
//! - [`surfaces`]: **the list of surfaces** as one table, and the gate over it. Written before the surfaces.
//! - [`Tombstones`]: what is hidden now, of a change, a proposal's revision, a comment, a claim's comment (and what an accepted
//!   proposal's revision and its page's revision share: one content, one record).
//! - [`Hidden`], [`Mark`]: what a surface is given to draw the tombstone: the parts, the category, the rung and the date, never
//!   a name and never the note.
//! - [`ModerationService`]: the three acts of the `moderate` rung, **web-only** (D-12): hide, unhide, and show hidden content;
//!   and the moderation log, which whoever may read the wiki reads.
//!
//! **What a tip is.** The content of a change cannot be hidden when it is what **any open view's tip holds now**, byte for
//! byte, at any path the change wrote ([`ReadService::tip_check`](crate::read::ReadService::tip_check)). A tip's summary and
//! author can be hidden. The moderation form says so and offers the rollback (RFC 004 D-8).
//!
//! **Hiding is not removal** (RR-15): prikk keeps every object. The moderation form says what still holds the content.

mod model;
mod reader;
mod service;
pub mod surfaces;

pub use model::{Hidden, Mark, ProposalHidden};
pub use reader::{
    Tombstones, Unreadable, claim_comment_item, comment_item, page_item, revision_item,
};
pub use service::{
    HideMade, Item, LogRow, MAX_NOTE_CHARS, MAX_REVEALED_FILES, ModerationDecline, ModerationError,
    ModerationService, Preview, Revealed, RevealedFile, Survives, UnhideMade,
};

#[cfg(test)]
mod tests;
