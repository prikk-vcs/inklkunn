//! The `^id` of a claim's paragraph, held by a form and never shown in its text box (RFC 005 part 1 D-1 b).
//!
//! A person who edits a paragraph others rely on edits **words**: the marker that makes the paragraph where a claim
//! is said is the form's to keep, and it is put back when the form is sent. What is put back, and where, is decided
//! by construction and never by guessing which new paragraph "is" an old one (D-1 a: the merge's pairing is not the
//! identity signal):
//!
//! - a form that edits **one paragraph** ([`reattach_part`]) hands the marker to the first paragraph of what the
//!   person wrote, because that paragraph is the one the form was opened on, after the whitespace it had before it;
//! - a form that edits **several** ([`reattach_all`]) hands a marker back to a paragraph that is exactly the text
//!   it was shown with, **the same occurrence** of those words when several paragraphs have them; a paragraph a
//!   person changed there, or one of several with equal words taken out or added, is *lost* (its claim loses this
//!   expression unless the person says so, and the form says that they must), and the way to reword a claim's
//!   paragraph is the form for that paragraph.
//!
//! Pure, and bounded by the text it is given. Only a **paragraph** can hold a claim (the prose block).

use std::collections::BTreeSet;
use std::ops::Range;

use inklkunn_markup::{Block, BlockKind, blocks, frontmatter};

/// The offset of the body in `text` and the body (the whole text when it is not a whole page).
fn body_of(text: &str, whole_page: bool) -> (usize, &str) {
    if whole_page {
        let (_, body, _) = frontmatter::split(text);
        (text.len().saturating_sub(body.len()), body)
    } else {
        (0, text)
    }
}

/// Where the marker of a paragraph is in the body, with the whitespace before it: the range to cut to leave the
/// paragraph's words. `None` for a block that is not a paragraph with an `^id`, or whose `^id` is all it says.
fn marker_range(block: &Block<'_>) -> Option<Range<usize>> {
    let id = block.id?;
    if block.kind != BlockKind::Paragraph {
        return None;
    }
    let head = block.text.strip_suffix(&format!("^{id}"))?;
    let words = head.trim_end();
    if words.is_empty() {
        return None;
    }
    Some(block.range.start + words.len()..block.range.end)
}

/// The words of a paragraph without its marker, as the form shows them.
fn words_of<'a>(block: &Block<'a>) -> &'a str {
    marker_range(block)
        .and_then(|cut| {
            block
                .text
                .get(..cut.start.saturating_sub(block.range.start))
        })
        .unwrap_or(block.text)
}

/// `text` without the markers of the paragraphs whose `^id` is in `ids`, which is what a form shows.
/// `whole_page` says whether `text` begins with a page's information block (a section or a part does not).
pub fn hide_markers(text: &str, ids: &BTreeSet<String>, whole_page: bool) -> String {
    let (offset, body) = body_of(text, whole_page);
    let cuts: Vec<Range<usize>> = blocks(body)
        .iter()
        .filter(|b| b.id.is_some_and(|id| ids.contains(id)))
        .filter_map(marker_range)
        .map(|r| offset + r.start..offset + r.end)
        .collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    for cut in cuts {
        out.push_str(text.get(at..cut.start).unwrap_or(""));
        at = cut.end;
    }
    out.push_str(text.get(at..).unwrap_or(""));
    out
}

/// The `^id` a form that edits this one block holds: the block is a paragraph whose `^id` is a claim's.
pub fn held_part(block_text: &str, ids: &BTreeSet<String>) -> Option<String> {
    let found = blocks(block_text);
    let [only] = found.as_slice() else {
        return None;
    };
    let id = only.id.filter(|id| ids.contains(*id))?;
    marker_range(only)?;
    Some(id.to_owned())
}

/// The whitespace a held paragraph has between its words and its marker in `started`: a space, or a line break when
/// the marker is on a line of its own. Putting the marker back with another would be an edit nobody made.
fn separator(started: &str, id: &str) -> String {
    blocks(started)
        .iter()
        .find(|b| b.id == Some(id))
        .and_then(|b| b.text.strip_suffix(&format!("^{id}")))
        .and_then(|head| head.get(head.trim_end().len()..))
        .filter(|sep| !sep.is_empty())
        .map_or_else(|| " ".to_owned(), str::to_owned)
}

/// Append the marker `id` to the last line of `paragraph`'s words, after `sep`.
fn with_marker(words: &str, sep: &str, id: &str) -> String {
    format!("{}{sep}^{id}", words.trim_end())
}

/// What a person wrote in the form for one paragraph, with the marker put back: on the first paragraph of the text,
/// **after the whitespace it had before it in `started`** (the block the form was opened on), so a text nobody touched
/// is the block byte for byte. `None` when it cannot be put back: nothing was written, the first block is not a
/// paragraph, it already carries another `^id`, or the marker does not take (read again, the paragraph does not hold
/// it). The caller then asks the person whether they mean to remove the paragraph.
pub fn reattach_part(started: &str, posted: &str, id: &str) -> Option<String> {
    let found = blocks(posted);
    let first = found.first()?;
    // A first block that is not a paragraph (a heading, a list) cannot hold a marker: the read below says so.
    match first.id {
        // The person left it, or wrote it themselves.
        Some(had) if had == id => return Some(posted.to_owned()),
        Some(_) => return None,
        None => {}
    }
    let end = first.range.end;
    let changed = format!(
        "{}{}{}",
        posted.get(..first.range.start)?,
        with_marker(first.text, &separator(started, id), id),
        posted.get(end..)?
    );
    // Read it again: the same blocks, the first one holding the marker.
    let after = blocks(&changed);
    let ok = after.len() == found.len()
        && after
            .first()
            .is_some_and(|b| b.id == Some(id) && words_of(b) == first.text);
    ok.then_some(changed)
}

/// What [`reattach_all`] made of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reattached {
    /// The text with the markers put back where they could be.
    pub text: String,
    /// The `^id`s of the paragraphs that were held and are not in the text any more (changed or removed).
    pub lost: Vec<String>,
}

/// Put back the markers of the paragraphs of `started` (the text the form was opened on, markers in it) whose `^id`
/// is in `ids`, into `posted` (what the person sent back, markers hidden). A paragraph of `posted` that is exactly
/// the words a held paragraph was shown with gets its marker, **as `started` had it**, so a text nobody touched is
/// the page byte for byte; the held paragraphs that match nothing are `lost`.
///
/// **Equal words are told apart by their place** (handoff 005 corrections K10), never by taking the first: the form
/// showed `Same.` twice (the second was the claim's, `Same. ^c1`), so the marker goes back to the paragraph that is the
/// same *occurrence* of those words in what was sent, the k-th among the paragraphs with equal words as the form showed
/// them. And only when the number of such paragraphs is what the form showed: if a paragraph with those words was taken
/// out or added, the text no longer says which of them the claim was, and the marker is `lost`, which asks the person
/// to confirm a removal instead of moving a claim onto a paragraph that may be another. Words that were shown once
/// and are sent once are matched as they always were.
pub fn reattach_all(
    started: &str,
    posted: &str,
    ids: &BTreeSet<String>,
    whole_page: bool,
) -> Reattached {
    let (_, started_body) = body_of(started, whole_page);
    let shown = blocks(started_body);
    let is_held =
        |b: &Block<'_>| b.id.is_some_and(|id| ids.contains(id)) && marker_range(b).is_some();
    // The text each paragraph had in the form: a held one without its marker, any other as it is.
    fn showed<'a>(b: &Block<'a>, held: bool) -> &'a str {
        if held { words_of(b) } else { b.text }
    }
    let paragraphs: Vec<&Block<'_>> = shown
        .iter()
        .filter(|b| b.kind == BlockKind::Paragraph)
        .collect();
    // For each held paragraph: its id, its words, its text with the marker, which occurrence of those words it
    // was among the paragraphs the form showed, and how many the form showed.
    let mut held: Vec<(String, String, String, usize, usize)> = Vec::new();
    for (at, b) in paragraphs.iter().enumerate() {
        let (true, Some(id)) = (is_held(b), b.id) else {
            continue;
        };
        let words = words_of(b);
        let before = paragraphs
            .iter()
            .take(at)
            .filter(|p| showed(p, is_held(p)) == words)
            .count();
        let all = paragraphs
            .iter()
            .filter(|p| showed(p, is_held(p)) == words)
            .count();
        held.push((
            id.to_owned(),
            words.to_owned(),
            b.text.to_owned(),
            before,
            all,
        ));
    }
    if held.is_empty() {
        return Reattached {
            text: posted.to_owned(),
            lost: Vec::new(),
        };
    }
    let (offset, posted_body) = body_of(posted, whole_page);
    let found = blocks(posted_body);
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut lost = Vec::new();
    for (id, words, whole, occurrence, shown_count) in &held {
        // A paragraph that already carries the marker (the form was opened before the claim was made).
        if found.iter().any(|b| b.id == Some(id.as_str())) {
            continue;
        }
        let same: Vec<&Block<'_>> = found
            .iter()
            .filter(|b| b.kind == BlockKind::Paragraph && b.text == words.as_str())
            .collect();
        match same.get(*occurrence).filter(|_| same.len() == *shown_count) {
            Some(b) => edits.push((offset + b.range.start..offset + b.range.end, whole.clone())),
            None => lost.push(id.clone()),
        }
    }
    edits.sort_by_key(|(r, _)| r.start);
    let mut text = String::with_capacity(posted.len());
    let mut at = 0usize;
    for (range, replacement) in &edits {
        text.push_str(posted.get(at..range.start).unwrap_or(""));
        text.push_str(replacement);
        at = range.end;
    }
    text.push_str(posted.get(at..).unwrap_or(""));
    Reattached { text, lost }
}

/// The words of the paragraph whose `^id` is `id` in `text` (marker left out), when the text has it once.
/// `whole_page` says whether `text` begins with a page's information block.
pub fn words_with_id(text: &str, id: &str, whole_page: bool) -> Option<String> {
    let (_, body) = body_of(text, whole_page);
    let found = blocks(body);
    let mut named = found.iter().filter(|b| b.id == Some(id));
    match (named.next(), named.next()) {
        (Some(b), None) => Some(words_of(b).to_owned()),
        _ => None,
    }
}
