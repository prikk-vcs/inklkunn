//! The top-level blocks of a body (internal design MK-5; RFC 004 D-5): the units the content-level merge
//! compares, and the addresses a comment anchors to.
//!
//! **The parser delimits them.** The block structure is `pulldown-cmark`'s own, with the profile's options, so
//! the merge, the scanner and the renderer agree on what a paragraph, a list or a code block is; this module
//! writes no second Markdown parser. What the parser cannot know is added from the profile's two block-level
//! constructs:
//!
//! - a **container** (`::: name` … `:::`) is one block, however many blocks it holds (CommonMark has no
//!   container syntax; the renderer rewrites the markers before the parser runs, and the same marker rule,
//!   the one in `structure.rs`, is used here). A block the parser starts before a container's opening line ends at
//!   that line;
//! - a **block identifier** (`^id`) names the block it ends: the last line of a paragraph, or a line of its
//!   own after a table, a code block, a list or a quotation (the renderer's rule, `render/blocks.rs`). An
//!   identifier inside a list item names the item, not the list, and an identifier on a heading line is not
//!   one.
//!
//! **Nothing is left between blocks but whitespace.** What the parser consumes without an event (a link
//! reference definition) becomes a block of kind [`BlockKind::Other`], so a merge that carries blocks and the
//! whitespace between them carries every character of the body.
//!
//! One pass over the parser's events and the lines, linear in the body. Pure; never panics on any input.

use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag};

use crate::scan::block_id;
use crate::structure::{ContainerLine, Hidden, Structure, container_marker, options};

/// What a top-level block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// A paragraph.
    Paragraph,
    /// A heading, ATX or setext.
    Heading,
    /// A list, ordered or not.
    List,
    /// A block quotation (a callout is one).
    Quotation,
    /// A table.
    Table,
    /// A fenced or indented code block.
    CodeBlock,
    /// A `::: name` … `:::` container, with everything in it.
    Container,
    /// An HTML block (a comment among them).
    Html,
    /// A thematic break.
    Rule,
    /// A footnote definition.
    Footnote,
    /// Anything else the parser consumed (a link reference definition) or does not delimit.
    Other,
}

/// A top-level block of a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block<'a> {
    /// Where it is in the body, with the whitespace around it left out. Blocks are in order, do not overlap,
    /// and only whitespace lies between them.
    pub range: Range<usize>,
    /// What it is.
    pub kind: BlockKind,
    /// Its `^id` without the caret, when it has one.
    pub id: Option<&'a str>,
    /// The block as written, surrounding whitespace trimmed: what two sides are compared by.
    pub text: &'a str,
}

/// The top-level blocks of `body` (a page's text after its frontmatter), in order.
pub fn blocks(body: &str) -> Vec<Block<'_>> {
    let structure = Structure::of(body);
    let containers = container_spans(body, &structure);
    let mut spans = parser_spans(body);
    spans.extend(
        containers
            .iter()
            .cloned()
            .map(|r| (r, BlockKind::Container)),
    );
    // The parser's blocks lose what a container holds, and keep what lies before and after it.
    let mut pieces: Vec<(Range<usize>, BlockKind)> = Vec::with_capacity(spans.len());
    for (range, kind) in spans {
        if kind == BlockKind::Container {
            pieces.push((range, kind));
            continue;
        }
        let first = containers.partition_point(|c| c.end <= range.start);
        let mut at = range.start;
        for c in containers
            .iter()
            .skip(first)
            .take_while(|c| c.start < range.end)
        {
            if c.start > at {
                pieces.push((at..c.start, kind));
            }
            at = at.max(c.end);
        }
        if at < range.end {
            pieces.push((at..range.end, kind));
        }
    }
    pieces.sort_by_key(|(r, _)| (r.start, r.end));

    let mut out: Vec<Block<'_>> = Vec::with_capacity(pieces.len());
    let mut end = 0usize;
    for (range, kind) in pieces {
        let range = trim(body, range.start.max(end)..range.end);
        if range.is_empty() {
            continue;
        }
        // Whatever lies between the previous block and this one and is not whitespace is blocks of its own.
        other_blocks(body, end..range.start, &mut out);
        end = range.end;
        push(body, range, kind, &mut out);
    }
    other_blocks(body, end..body.len(), &mut out);
    attach_trailers(body, out)
}

/// A block of the given kind over `range` (already trimmed).
fn push<'a>(body: &'a str, range: Range<usize>, kind: BlockKind, out: &mut Vec<Block<'a>>) {
    let text = body.get(range.clone()).unwrap_or("");
    let last_line = text
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .trim_end_matches('\r');
    let id = match kind {
        // The last line of a paragraph, or the line of its own that ends a table.
        BlockKind::Paragraph => block_id(last_line).map(|(id, _)| id),
        BlockKind::Table => lone_id(last_line).filter(|_| text.contains('\n')),
        _ => None,
    };
    out.push(Block {
        range,
        kind,
        id,
        text,
    });
}

/// The identifier of a line that holds nothing else.
fn lone_id(line: &str) -> Option<&str> {
    block_id(line).and_then(|(id, caret)| (line.get(..caret)?.trim().is_empty()).then_some(id))
}

/// The top-level blocks the parser reports, with the kind its tag gives them.
fn parser_spans(body: &str) -> Vec<(Range<usize>, BlockKind)> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(body, options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    out.push((range, kind_of(&tag)));
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            other if depth == 0 => out.push((
                range,
                if matches!(other, Event::Rule) {
                    BlockKind::Rule
                } else {
                    BlockKind::Other
                },
            )),
            _ => {}
        }
    }
    out
}

fn kind_of(tag: &Tag<'_>) -> BlockKind {
    match tag {
        Tag::Paragraph => BlockKind::Paragraph,
        Tag::Heading { .. } => BlockKind::Heading,
        Tag::List(_) => BlockKind::List,
        Tag::BlockQuote(_) => BlockKind::Quotation,
        Tag::Table(_) => BlockKind::Table,
        Tag::CodeBlock(_) => BlockKind::CodeBlock,
        Tag::HtmlBlock => BlockKind::Html,
        Tag::FootnoteDefinition(_) => BlockKind::Footnote,
        _ => BlockKind::Other,
    }
}

/// The outermost containers of `body`: from the line that opens one to the line that closes it (the end of
/// the body when it is never closed), by the renderer's rule: lines inside code or HTML are not markers, and
/// a closer with nothing open is text.
fn container_spans(body: &str, structure: &Structure) -> Vec<Range<usize>> {
    let mut hidden = Hidden::of(structure);
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut opened = 0usize;
    let mut offset = 0usize;
    for line in body.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        let first_non_space = line_start + (content.len() - content.trim_start().len());
        if hidden.contains(line_start) || hidden.contains(first_non_space) {
            continue;
        }
        match container_marker(content) {
            Some((ContainerLine::Open, _)) => {
                if depth == 0 {
                    opened = line_start;
                }
                depth += 1;
            }
            Some((ContainerLine::Close, _)) if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push(opened..offset);
                }
            }
            _ => {}
        }
    }
    if depth > 0 {
        out.push(opened..body.len());
    }
    out
}

/// `range` without the whitespace around it.
fn trim(body: &str, range: Range<usize>) -> Range<usize> {
    let text = body.get(range.clone()).unwrap_or("");
    let lead = text.len() - text.trim_start().len();
    let kept = text.trim().len();
    if kept == 0 {
        return range.start..range.start;
    }
    range.start + lead..range.start + lead + kept
}

/// The text of `range` that no block holds, as blocks of kind [`BlockKind::Other`]: its runs of lines
/// between blank lines.
fn other_blocks<'a>(body: &'a str, range: Range<usize>, out: &mut Vec<Block<'a>>) {
    let Some(text) = body.get(range.clone()) else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let mut run: Option<Range<usize>> = None;
    let mut offset = range.start;
    for line in text.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        if line.trim().is_empty() {
            if let Some(r) = run.take() {
                push(body, trim(body, r), BlockKind::Other, out);
            }
        } else {
            run = Some(run.map_or(at..offset, |r| r.start..offset));
        }
    }
    if let Some(r) = run {
        push(body, trim(body, r), BlockKind::Other, out);
    }
}

/// A paragraph that holds nothing but `^id` names the block before it, when that block can carry one and has
/// none: the two become one block.
fn attach_trailers<'a>(body: &'a str, blocks: Vec<Block<'a>>) -> Vec<Block<'a>> {
    let mut out: Vec<Block<'a>> = Vec::with_capacity(blocks.len());
    for block in blocks {
        let trailer = (block.kind == BlockKind::Paragraph && !block.text.contains('\n'))
            .then(|| lone_id(block.text))
            .flatten();
        if let Some(id) = trailer
            && let Some(prev) = out.last_mut()
            && prev.id.is_none()
            && matches!(
                prev.kind,
                BlockKind::Paragraph
                    | BlockKind::List
                    | BlockKind::Quotation
                    | BlockKind::Table
                    | BlockKind::CodeBlock
            )
        {
            prev.range.end = block.range.end;
            prev.text = body.get(prev.range.clone()).unwrap_or(prev.text);
            prev.id = Some(id);
        } else {
            out.push(block);
        }
    }
    out
}

#[cfg(test)]
mod tests;
