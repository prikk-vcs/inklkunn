//! The measurement hook (RFC 004 D-5): for an overlap, would a line-level three-way merge of the same text
//! have been clean? **Counted and reported; never applied.** The merge itself does not merge within a block,
//! and this answer changes nothing it does.
//!
//! The check is the textbook one (`diff3`, as `git merge-file` applies it): the lines are aligned with the
//! base by content and order only, and the texts are clean when no stretch between the lines all three have in
//! common was changed by both sides differently. Two changes that touch or share a line are a conflict, as
//! there; nothing is paired by position, so the answer is the conservative one. It is made with the block
//! merge's own engine and no further dependency.

use super::engine::{Piece, Unit, Verdict, pieces, slice, verdict};
use super::{LineLevel, Place};

/// The answer for an overlap at `place`.
pub(super) fn line_level(
    place: &Place,
    base: Option<&str>,
    theirs: Option<&str>,
    ours: Option<&str>,
) -> LineLevel {
    match place {
        Place::Frontmatter { .. } | Place::Block { .. } => {
            if would_be_clean(
                base.unwrap_or_default(),
                theirs.unwrap_or_default(),
                ours.unwrap_or_default(),
            ) {
                LineLevel::WouldBeClean
            } else {
                LineLevel::WouldConflict
            }
        }
        Place::Page | Place::Path => LineLevel::NotText,
    }
}

fn lines(text: &str) -> Vec<Unit<'_>> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|line| Unit {
            id: None,
            text: line,
        })
        .collect()
}

/// Would a line-level three-way merge of these three texts be clean?
pub(super) fn would_be_clean(base: &str, theirs: &str, ours: &str) -> bool {
    let (b, t, o) = (lines(base), lines(theirs), lines(ours));
    pieces(&b, &t, &o, false).iter().all(|piece| match piece {
        Piece::Sync { .. } => true,
        Piece::Chunk { base, theirs, ours } => {
            verdict(&slice(&b, base), &slice(&t, theirs), &slice(&o, ours)) != Verdict::Overlap
        }
    })
}
