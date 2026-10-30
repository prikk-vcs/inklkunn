//! A comment's anchor and where it lands on a later revision (RFC 004 D-3; COLL-5).
//!
//! A comment is about one part of one page of the revision it was made on. The anchor stored with it names the
//! part by its `^id` when it has one, and by a **quote selector** besides: the part's own text, with a little
//! of the text before it and after it, and the part's position. It is not changed afterwards. Where it lands on
//! a later revision is **derived each time the proposal is read**:
//!
//! 1. the part with the same `^id`, if the page still has one;
//! 2. else the parts whose text holds the quoted text; among them the one whose surroundings agree with the
//!    quote's prefix and suffix, and then the one nearest to the old position;
//! 3. else nothing matches: the comment is **orphaned**. It is kept and shown with the text it was about, and
//!    nothing is guessed.

use inklkunn_markup::{blocks, frontmatter};
use inklkunn_store::{Anchor, Quote};

/// The most bytes of a part's text a quote holds.
const QUOTE_BYTES: usize = 1024;
/// The most characters of the text before and after a part a quote holds.
const CONTEXT_CHARS: usize = 64;

/// Where an anchor lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Landing {
    /// On the part at this position of the page's body.
    At(usize),
    /// Nowhere: no part has the anchor's `^id` or holds its quote.
    Orphaned,
}

/// `text` cut to at most `max` bytes at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or("")
}

/// The last `n` characters of `text`.
fn tail(text: &str, n: usize) -> &str {
    let skip = text.chars().count().saturating_sub(n);
    text.char_indices()
        .nth(skip)
        .and_then(|(at, _)| text.get(at..))
        .unwrap_or("")
}

/// The first `n` characters of `text`.
fn head(text: &str, n: usize) -> &str {
    text.char_indices()
        .nth(n)
        .and_then(|(at, _)| text.get(..at))
        .unwrap_or(text)
}

/// The anchor of the `ordinal`-th part of the body of `text`, the page at `path`. `None` when the page has no
/// such part.
pub(crate) fn make(path: &str, text: &str, ordinal: usize) -> Option<Anchor> {
    let (_, body, _) = frontmatter::split(text);
    let all = blocks(body);
    let block = all.get(ordinal)?;
    let before = body.get(..block.range.start).unwrap_or("").trim_end();
    let after = body.get(block.range.end..).unwrap_or("").trim_start();
    Some(Anchor {
        page: path.to_owned(),
        block_id: block.id.map(str::to_owned),
        quote: Some(Quote {
            exact: cut(block.text, QUOTE_BYTES).to_owned(),
            prefix: tail(before, CONTEXT_CHARS).to_owned(),
            suffix: head(after, CONTEXT_CHARS).to_owned(),
        }),
        ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
    })
}

/// Where `anchor` lands in `text`, the current text of the page it names.
pub(crate) fn land(anchor: &Anchor, text: &str) -> Landing {
    let (_, body, _) = frontmatter::split(text);
    let all = blocks(body);
    if let Some(id) = anchor.block_id.as_deref()
        && let Some(i) = all.iter().position(|b| b.id == Some(id))
    {
        return Landing::At(i);
    }
    let Some(quote) = &anchor.quote else {
        return Landing::Orphaned;
    };
    let old = usize::try_from(anchor.ordinal).unwrap_or(usize::MAX);
    all.iter()
        .enumerate()
        .filter(|(_, b)| !quote.exact.is_empty() && b.text.contains(quote.exact.as_str()))
        .map(|(i, b)| {
            let before = body.get(..b.range.start).unwrap_or("").trim_end();
            let after = body.get(b.range.end..).unwrap_or("").trim_start();
            let context = u8::from(before.ends_with(quote.prefix.as_str()))
                + u8::from(after.starts_with(quote.suffix.as_str()));
            (i, context)
        })
        .min_by_key(|(i, context)| (std::cmp::Reverse(*context), i.abs_diff(old), *i))
        .map_or(Landing::Orphaned, |(i, _)| Landing::At(i))
}

#[cfg(test)]
mod tests;
