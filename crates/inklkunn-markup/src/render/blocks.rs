//! Block identifiers as `id` attributes (MKUP-12; handoff 003 T3): `Paragraph text. ^p1` renders as
//! `<p id="^p1">Paragraph text.</p>`, so `[[Page#^p1]]` and a permalink land on the block itself and no empty
//! element sits beside it.
//!
//! The inline pre-pass cannot know the block an identifier ends, so it leaves a marker (an empty
//! `<span class="block-id" id="^p1">`) and this pass, which sees the parser's block structure, moves each
//! marker onto the block that holds it:
//!
//! - an identifier **inside** a paragraph or a list item names that paragraph or item (the innermost one: a
//!   quotation's identifier lands on the paragraph in it, a loose list item's on the paragraph in the item);
//! - a paragraph that holds **nothing but** an identifier is a *trailer*, and names the block just before it
//!   (`^id` on a line of its own after a table, a code block, a list or a quotation: the forms the profile
//!   gives those blocks, MKUP-9). When the line follows a table directly the parser makes it a table row of
//!   one cell, which this pass recognizes and removes;
//! - an identifier that cannot be placed (inside a heading, in a table cell, a second one in a block that
//!   has one, a trailer with nothing before it) keeps its marker, so the anchor still exists.
//!
//! One pass over the events with a stack: linear in the page.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

use crate::text::escape_html;

const MARKER_OPEN: &str = "<span class=\"block-id\" id=\"";
const MARKER_CLOSE: &str = "\">";

/// What the inline pre-pass writes in place of a trailing `^id`: an empty span carrying the id, with the
/// transclusion prefix of the page it is on (`""` for the page itself).
pub(super) fn marker(prefix: &str, id: &str) -> String {
    format!(
        "{MARKER_OPEN}{}{MARKER_CLOSE}</span>",
        escape_html(&format!("{prefix}^{id}"))
    )
}

/// The id of the marker whose opening tag is `events[i]` (its `</span>` is `events[i + 1]`).
fn marker_at<'e>(events: &'e [Event<'_>], i: usize) -> Option<&'e str> {
    let Some(Event::InlineHtml(open)) = events.get(i) else {
        return None;
    };
    let Some(Event::InlineHtml(close)) = events.get(i + 1) else {
        return None;
    };
    if &**close != "</span>" {
        return None;
    }
    open.strip_prefix(MARKER_OPEN)?.strip_suffix(MARKER_CLOSE)
}

/// A table row that holds only a marker (and empty cells): `(id, events in the row)`.
fn marker_row<'e>(events: &'e [Event<'_>], i: usize) -> Option<(&'e str, usize)> {
    if !matches!(events.get(i), Some(Event::Start(Tag::TableRow)))
        || !matches!(events.get(i + 1), Some(Event::Start(Tag::TableCell)))
    {
        return None;
    }
    let id = marker_at(events, i + 2)?;
    if !matches!(events.get(i + 4), Some(Event::End(TagEnd::TableCell))) {
        return None;
    }
    let mut j = i + 5;
    loop {
        match (events.get(j), events.get(j + 1)) {
            (Some(Event::Start(Tag::TableCell)), Some(Event::End(TagEnd::TableCell))) => j += 2,
            (Some(Event::End(TagEnd::TableRow)), _) => return Some((id, j + 1 - i)),
            _ => return None,
        }
    }
}

/// The opening tag of a block that carries `id`, written as the parser's HTML writer would write it. `None`
/// for a tag that is not a block an identifier can name.
fn open_tag(tag: &Tag<'_>, id: &str) -> Option<String> {
    Some(match tag {
        Tag::Paragraph => format!("<p id=\"{id}\">"),
        Tag::Item => format!("<li id=\"{id}\">"),
        Tag::BlockQuote(_) => format!("<blockquote id=\"{id}\">\n"),
        Tag::List(None) => format!("<ul id=\"{id}\">\n"),
        Tag::List(Some(1)) => format!("<ol id=\"{id}\">\n"),
        Tag::List(Some(start)) => format!("<ol start=\"{start}\" id=\"{id}\">\n"),
        Tag::Table(_) => format!("<table id=\"{id}\">"),
        Tag::CodeBlock(CodeBlockKind::Fenced(info)) => {
            let language = info.split(' ').next().unwrap_or("");
            if language.is_empty() {
                format!("<pre id=\"{id}\"><code>")
            } else {
                format!(
                    "<pre id=\"{id}\"><code class=\"language-{}\">",
                    escape_html(language)
                )
            }
        }
        Tag::CodeBlock(CodeBlockKind::Indented) => format!("<pre id=\"{id}\"><code>"),
        _ => return None,
    })
}

/// Does the tag open something inside a line (emphasis, a link) rather than a block?
fn is_inline(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
    )
}

/// A block that has been closed: where its start event is in the output, and whether it has an id.
struct Done<'a> {
    tag: Tag<'a>,
    at: usize,
    has_id: bool,
}

/// An open block.
struct Frame<'a> {
    /// `None` for the document.
    tag: Option<Tag<'a>>,
    /// Where its start event went in the output.
    at: usize,
    has_id: bool,
    /// The child block that closed last, while nothing else has come between.
    last: Option<Done<'a>>,
}

/// Move the markers onto their blocks.
pub(super) fn attach<'a>(events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut stack: Vec<Frame<'a>> = vec![Frame {
        tag: None,
        at: 0,
        has_id: false,
        last: None,
    }];
    let mut i = 0;
    while let Some(event) = events.get(i) {
        match event {
            Event::Start(tag) => {
                // A paragraph that is only an identifier names the block before it.
                if matches!(tag, Tag::Paragraph)
                    && let Some(id) = marker_at(&events, i + 1)
                    && matches!(events.get(i + 3), Some(Event::End(TagEnd::Paragraph)))
                    && let Some(done) = stack.last_mut().and_then(|f| f.last.as_mut())
                    && !done.has_id
                    && let Some(open) = open_tag(&done.tag, id)
                {
                    done.has_id = true;
                    if let Some(slot) = out.get_mut(done.at) {
                        *slot = Event::Html(CowStr::from(open));
                    }
                    i += 4;
                    continue;
                }
                // A table row that is only an identifier names the table.
                if let Some((id, used)) = marker_row(&events, i)
                    && let Some(table) = stack.last_mut()
                    && let Some(tag) = table.tag.as_ref()
                    && matches!(tag, Tag::Table(_))
                    && !table.has_id
                    && let Some(open) = open_tag(tag, id)
                {
                    table.has_id = true;
                    if let Some(slot) = out.get_mut(table.at) {
                        *slot = Event::Html(CowStr::from(open));
                    }
                    i += used;
                    continue;
                }
                stack.push(Frame {
                    tag: Some(tag.clone()),
                    at: out.len(),
                    has_id: false,
                    last: None,
                });
                out.push(event.clone());
            }
            Event::End(_) => {
                out.push(event.clone());
                if stack.len() > 1
                    && let Some(closed) = stack.pop()
                    && let Some(parent) = stack.last_mut()
                {
                    parent.last = closed.tag.map(|tag| Done {
                        tag,
                        at: closed.at,
                        has_id: closed.has_id,
                    });
                }
            }
            // Something that is neither a block of ours nor inline text separates what came before from
            // what comes after.
            Event::Rule | Event::Html(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.last = None;
                }
                out.push(event.clone());
            }
            Event::InlineHtml(_) if marker_at(&events, i).is_some() => {
                let id = marker_at(&events, i).unwrap_or_default();
                // The nearest block around the marker, passing over emphasis and links.
                let holder = stack
                    .iter_mut()
                    .rev()
                    .find(|f| f.tag.as_ref().is_none_or(|t| !is_inline(t)));
                let placed = match holder {
                    Some(frame)
                        if !frame.has_id
                            && frame
                                .tag
                                .as_ref()
                                .is_some_and(|t| matches!(t, Tag::Paragraph | Tag::Item)) =>
                    {
                        let open = frame.tag.as_ref().and_then(|t| open_tag(t, id));
                        match (open, out.get_mut(frame.at)) {
                            (Some(open), Some(slot)) => {
                                *slot = Event::Html(CowStr::from(open));
                                frame.has_id = true;
                                true
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                };
                if placed {
                    i += 2;
                    continue;
                }
                out.push(event.clone());
            }
            _ => out.push(event.clone()),
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests;
