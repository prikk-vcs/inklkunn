//! The parts of a page a person can propose a change to (RFC 004 D-2): the whole page, a **section** (a heading
//! and what follows it, up to the next heading of the same or a higher level) or one **part** (a top-level
//! block of `inklkunn_markup::blocks`, the unit the merge and the comments use). A part is named by its
//! position, and is found again by that position in the **base** text the form was opened on, so the text a
//! person edited is put back into exactly the place it came from, however far the page has moved since.

use std::ops::Range;

use inklkunn_markup::{Block, BlockKind, blocks, frontmatter};

/// The most parts or sections a chooser lists.
pub(crate) const MAX_LISTED: usize = 200;

/// The longest excerpt shown for a part, in characters.
const EXCERPT_CHARS: usize = 80;

/// Which part of a page a form edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selection {
    /// The whole page.
    Whole,
    /// The `n`-th heading of the page, and what follows it.
    Section(usize),
    /// The `n`-th top-level block of the body.
    Block(usize),
}

/// A part, found in a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    /// Where it is in the whole text, in bytes.
    pub(crate) range: Range<usize>,
    /// What it begins with, one line.
    pub(crate) excerpt: String,
}

/// The offset of the body in a page's text.
fn body_offset(text: &str) -> usize {
    let (_, body, _) = frontmatter::split(text);
    text.len() - body.len()
}

/// The first line of `text`, cut to a length a list can hold.
pub(crate) fn excerpt(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(EXCERPT_CHARS).collect();
    if line.chars().count() > EXCERPT_CHARS {
        out.push('…');
    }
    out
}

/// The heading level of a heading block: the `#`s of an ATX heading, 1 or 2 for a setext one.
fn level(block: &Block<'_>) -> usize {
    let first = block.text.lines().next().unwrap_or("");
    let hashes = first.chars().take_while(|c| *c == '#').count();
    if hashes > 0 {
        return hashes.min(6);
    }
    match block.text.lines().nth(1).map(str::trim_start) {
        Some(under) if under.starts_with('=') => 1,
        _ => 2,
    }
}

/// The heading text without its marks, for a section's excerpt.
fn heading_text(block: &Block<'_>) -> String {
    let first = block.text.lines().next().unwrap_or("");
    excerpt(first.trim_start_matches('#').trim())
}

/// The sections of a page's text, in order: each heading with its range.
pub(crate) fn sections(text: &str) -> Vec<Found> {
    let offset = body_offset(text);
    let (_, body, _) = frontmatter::split(text);
    let all = blocks(body);
    let mut out = Vec::new();
    for (i, block) in all.iter().enumerate() {
        if block.kind != BlockKind::Heading {
            continue;
        }
        let depth = level(block);
        let last = all
            .iter()
            .skip(i + 1)
            .take_while(|b| !(b.kind == BlockKind::Heading && level(b) <= depth))
            .last()
            .unwrap_or(block);
        out.push(Found {
            range: offset + block.range.start..offset + last.range.end,
            excerpt: heading_text(block),
        });
    }
    out
}

/// The top-level parts of a page's body, in order.
pub(crate) fn parts(text: &str) -> Vec<Found> {
    let offset = body_offset(text);
    let (_, body, _) = frontmatter::split(text);
    blocks(body)
        .into_iter()
        .map(|b| Found {
            range: offset + b.range.start..offset + b.range.end,
            excerpt: excerpt(b.text),
        })
        .collect()
}

/// The selected part of `text`, or `None` when the text has no such part (the base is the same text the form
/// was opened on, so this means a form that was tampered with).
pub(crate) fn find(text: &str, selection: Selection) -> Option<Found> {
    match selection {
        Selection::Whole => Some(Found {
            range: 0..text.len(),
            excerpt: excerpt(text),
        }),
        Selection::Section(n) => sections(text).into_iter().nth(n),
        Selection::Block(n) => parts(text).into_iter().nth(n),
    }
}

/// `text` with the range of `found` replaced by `new`.
pub(crate) fn splice(text: &str, found: &Found, new: &str) -> Option<String> {
    let before = text.get(..found.range.start)?;
    let after = text.get(found.range.end..)?;
    Some(format!("{before}{new}{after}"))
}

#[cfg(test)]
mod tests;
