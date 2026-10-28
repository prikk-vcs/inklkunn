//! Transclusion (NAV-16; RFC 003 D-3; handoff 003 T3): `![[Page]]` on a line of its own brings the page's
//! rendered body into this one, **to a depth of two**, with its provenance shown, and a page that would
//! include itself is **refused with the cycle named**, in place, while the rest of the page renders.
//!
//! The crate is pure and does not read pages: the caller supplies the page ([`crate::RenderLinks::transclusion`])
//! and, because it knows the language, the words ([`Transclusion::label`], [`crate::RenderLinks::transclusion_notice`]).
//! This module writes structure and data and no sentence of its own.
//!
//! **What is a transclusion.** An embed that is alone on its line, names no heading or block (`![[Page]]`,
//! not `![[Page#Heading]]`), is not an image, and that the caller says is a page. Anywhere else (in a
//! sentence, a list item, a heading) an embed stays what it was: a link.
//!
//! **What the included page looks like.** A `<div class="transclusion">` holding a provenance line (a link to
//! the page, the caller's label) and the page's body, rendered as any body is, with three differences that keep
//! the host page well formed: its headings sit one level lower per level of nesting (so it never adds a second
//! `h1`), every `id` it writes carries a prefix of its own (so two inclusions of one page, or a heading both
//! pages have, never share an `id`), and its same-page links follow its ids.
//!
//! **How the HTML gets in.** Pages are CommonMark and a block of HTML cannot be handed to the parser as text
//! without being parsed again (a blank line ends it; a line indented four spaces becomes code). So the
//! pre-pass writes a *token* in the embed's place and this module swaps the token for the finished HTML in the
//! parser's event stream. Tokens are private-use characters, which are removed from every body first, so a
//! page cannot forge one.

use std::borrow::Cow;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, Tag, TagEnd};

use crate::text::{escape_href, escape_html, href_is_allowed};

use super::{Ctx, Level, PageRef, render_body};

/// How many pages deep inclusion goes: the page itself includes one (depth 1), which includes one (depth
/// 2), which includes nothing. A deeper embed is refused.
pub const MAX_TRANSCLUSION_DEPTH: usize = 2;

/// How many pages one rendered page may include, at every depth together. Each inclusion costs the caller a
/// read of the page and a render, and inclusions nest (a page that includes a page 30 times, which includes
/// a page 30 times, would render 900), so the work a page can ask for is bounded by this and by
/// [`MAX_TRANSCLUDED_BYTES`].
pub const MAX_TRANSCLUSIONS: usize = 32;

/// How many bytes of source the pages included in one rendered page may have in all.
pub const MAX_TRANSCLUDED_BYTES: usize = 4 << 20;

const OPEN: char = '\u{E000}';
const CLOSE: char = '\u{E001}';

/// A page that an embed brings in, as the caller found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transclusion {
    /// The page's identity: its path. Two embeds of one path are one page, and a path already being
    /// rendered is a cycle.
    pub path: String,
    /// The page's title, shown in the provenance line, in the chain of a refusal and as `data-transclusion`.
    pub title: String,
    /// Where the page is: the provenance line links here.
    pub href: String,
    /// The provenance line's words, in the caller's language ("From ‘Glossary’"). Plain text. Empty means
    /// the title alone.
    pub label: String,
}

/// Why an embed that could have been included was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransclusionRefusal {
    /// The page, directly or through the pages it includes, would include itself. `chain` is the titles from
    /// the first page of the cycle to the page that repeats it: `[A, B, A]`.
    Cycle {
        /// The pages of the cycle, in order, ending with the one it returns to.
        chain: Vec<String>,
    },
    /// The embed is deeper than [`MAX_TRANSCLUSION_DEPTH`].
    TooDeep {
        /// The page that was not included.
        title: String,
        /// The depth allowed.
        limit: usize,
    },
    /// The page already includes as much as a page may ([`MAX_TRANSCLUSIONS`] pages, [`MAX_TRANSCLUDED_BYTES`]
    /// of their source). Noted once, at the first embed that was not included for it; the embeds after it
    /// stay links.
    OverBudget {
        /// The first page that was not included.
        title: String,
    },
}

impl TransclusionRefusal {
    /// A stable code for the refusal (`cycle`, `too-deep`), written as `data-transclusion-refusal`.
    pub fn code(&self) -> &'static str {
        match self {
            TransclusionRefusal::Cycle { .. } => "cycle",
            TransclusionRefusal::TooDeep { .. } => "too-deep",
            TransclusionRefusal::OverBudget { .. } => "over-budget",
        }
    }
}

/// The refusal without words: the titles it names, `A → B → A` for a cycle and the title for a depth. A
/// caller that wants sentences writes its own ([`crate::RenderLinks::transclusion_notice`]).
impl std::fmt::Display for TransclusionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransclusionRefusal::Cycle { chain } => f.write_str(&chain.join(" → ")),
            TransclusionRefusal::TooDeep { title, .. }
            | TransclusionRefusal::OverBudget { title } => f.write_str(title),
        }
    }
}

/// An embed that might become a block: the link as scanned, and the HTML of the link it stays when it does
/// not. Whether it becomes a block is decided where the parser has spoken (in [`splice`]): only a paragraph
/// that holds nothing but the embed is one, and only then is the caller asked for the page.
pub(super) struct Fragment {
    pub(super) link: crate::scan::Link,
    pub(super) inline: String,
}

/// The token that stands for fragment `index` in the pre-pass's source.
pub(super) fn token(index: usize) -> String {
    format!("{OPEN}{index}{CLOSE}")
}

/// `body` without the characters tokens are made of (replaced by U+FFFD), so no page can write a token.
pub(super) fn strip_sentinels(body: &str) -> Cow<'_, str> {
    if body.contains([OPEN, CLOSE]) {
        Cow::Owned(body.replace([OPEN, CLOSE], "\u{FFFD}"))
    } else {
        Cow::Borrowed(body)
    }
}

/// Is `link` an embed that could be a transclusion: `![[Page]]` with no fragment, not an image, alone on its
/// line of `source`?
pub(super) fn is_candidate(link: &crate::scan::Link, source: &str) -> bool {
    if !link.embed || link.fragment.is_some() || link.target.is_empty() {
        return false;
    }
    let image =
        link.target.rsplit('.').next().is_some_and(|ext| {
            super::IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
        });
    if image {
        return false;
    }
    let before = source.get(..link.start).unwrap_or("");
    let after = source.get(link.end..).unwrap_or("");
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line_end = after.find('\n').unwrap_or(after.len());
    before
        .get(line_start..)
        .is_some_and(|s| s.trim().is_empty())
        && after.get(..line_end).is_some_and(|s| s.trim().is_empty())
}

/// The block for an embed the caller says is `page`: the included page rendered one level down, or the
/// refusal in its place; `None` when the page turns out not to be readable as a page, so that the embed stays
/// a link. The page's body is asked for only after the refusals, which cost the caller nothing.
pub(super) fn block(page: &Transclusion, ctx: &mut Ctx<'_>, level: &Level) -> Option<String> {
    // A cycle first: it is the more useful thing to say when both hold.
    if let Some(at) = ctx.stack.iter().position(|p| p.path == page.path) {
        let mut chain: Vec<String> = ctx
            .stack
            .get(at..)
            .unwrap_or(&[])
            .iter()
            .map(|p| p.title.clone())
            .collect();
        chain.push(page.title.clone());
        return Some(refused(TransclusionRefusal::Cycle { chain }, ctx));
    }
    if level.depth >= MAX_TRANSCLUSION_DEPTH {
        return Some(refused(
            TransclusionRefusal::TooDeep {
                title: page.title.clone(),
                limit: MAX_TRANSCLUSION_DEPTH,
            },
            ctx,
        ));
    }
    // The budget: the count before the page is read (a read is the cost), the bytes after. Once it has run
    // out it stays out: the embeds after the first that did not fit are not read to see whether they would.
    if ctx.included >= MAX_TRANSCLUSIONS || ctx.budget_noted {
        return over_budget(page, ctx);
    }
    let body = ctx.links.transclusion_body(page)?;
    if ctx.included_bytes.saturating_add(body.len()) > MAX_TRANSCLUDED_BYTES {
        return over_budget(page, ctx);
    }
    ctx.included += 1;
    ctx.included_bytes += body.len();
    if !ctx.transcluded.contains(&page.path) {
        ctx.transcluded.push(page.path.clone());
    }
    ctx.counter += 1;
    let inner_level = Level {
        depth: level.depth + 1,
        id_prefix: format!("t{}-", ctx.counter),
        demote: level.demote + 1,
    };
    ctx.stack.push(PageRef {
        path: page.path.clone(),
        title: page.title.clone(),
    });
    let inner = render_body(&body, ctx, &inner_level);
    ctx.stack.pop();
    let label = if page.label.is_empty() {
        page.title.as_str()
    } else {
        page.label.as_str()
    };
    let source = if href_is_allowed(&page.href) {
        format!(
            "<a class=\"wikilink\" href=\"{}\">{}</a>",
            escape_href(&page.href),
            escape_html(label)
        )
    } else {
        escape_html(label)
    };
    Some(format!(
        "<div class=\"transclusion\" data-transclusion=\"{}\" data-transclusion-depth=\"{}\">\n\
         <p class=\"transclusion-source\">{source}</p>\n{}</div>\n",
        escape_html(&page.title),
        inner_level.depth,
        inner.html
    ))
}

/// An embed over the budget: the first one is a note in the page and a refusal for the caller, the rest stay
/// links (`None`), so that a page cannot make its own HTML as long as it has embeds.
fn over_budget(page: &Transclusion, ctx: &mut Ctx<'_>) -> Option<String> {
    if ctx.budget_noted {
        return None;
    }
    ctx.budget_noted = true;
    Some(refused(
        TransclusionRefusal::OverBudget {
            title: page.title.clone(),
        },
        ctx,
    ))
}

/// The refusal as a note in the page, recorded for the caller.
fn refused(refusal: TransclusionRefusal, ctx: &mut Ctx<'_>) -> String {
    let notice = ctx.links.transclusion_notice(&refusal);
    let html = format!(
        "<p class=\"transclusion-refused\" data-transclusion-refusal=\"{}\" role=\"note\">{}</p>\n",
        refusal.code(),
        escape_html(&notice)
    );
    if !ctx.refusals.contains(&refusal) {
        ctx.refusals.push(refusal);
    }
    html
}

/// `t` is exactly one token (surrounding whitespace aside): which fragment?
fn whole_token(t: &str) -> Option<usize> {
    t.trim()
        .strip_prefix(OPEN)?
        .strip_suffix(CLOSE)?
        .parse()
        .ok()
}

/// Swap tokens for their HTML in the event stream: a paragraph that is only a token becomes the block, if
/// the caller says the embed is a page, and is the link otherwise; a token anywhere else becomes the link it
/// would have been.
pub(super) fn splice<'a>(
    events: Vec<(Event<'a>, Range<usize>)>,
    fragments: &[Fragment],
    ctx: &mut Ctx<'_>,
    level: &Level,
) -> Vec<(Event<'a>, Range<usize>)> {
    if fragments.is_empty() {
        return events;
    }
    let mut out = Vec::with_capacity(events.len());
    let mut i = 0;
    while let Some((event, range)) = events.get(i) {
        if matches!(event, Event::Start(Tag::Paragraph))
            && let Some((Event::Text(t), _)) = events.get(i + 1)
            && matches!(events.get(i + 2), Some((Event::End(TagEnd::Paragraph), _)))
            && let Some(fragment) = whole_token(t).and_then(|k| fragments.get(k))
        {
            let included = ctx
                .links
                .transclusion(&fragment.link)
                .and_then(|page| block(&page, ctx, level));
            match included {
                Some(html) => out.push((Event::Html(CowStr::from(html)), range.clone())),
                // Not a page after all, or not readable as one: the paragraph stays, holding the link.
                None => {
                    out.push((Event::Start(Tag::Paragraph), range.clone()));
                    out.push((
                        Event::InlineHtml(CowStr::from(fragment.inline.clone())),
                        range.clone(),
                    ));
                    out.push((Event::End(TagEnd::Paragraph), range.clone()));
                }
            }
            i += 3;
            continue;
        }
        match event {
            Event::Text(t) if t.contains(OPEN) => {
                for piece in split_text(t, fragments) {
                    out.push((piece, range.clone()));
                }
            }
            other => out.push((other.clone(), range.clone())),
        }
        i += 1;
    }
    out
}

/// A text with tokens in it, as text and inline HTML.
fn split_text<'a>(text: &str, fragments: &[Fragment]) -> Vec<Event<'a>> {
    let mut pieces: Vec<Event<'a>> = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(OPEN) {
        plain.push_str(rest.get(..open).unwrap_or(""));
        let after = rest.get(open + OPEN.len_utf8()..).unwrap_or("");
        let token = after
            .find(CLOSE)
            .and_then(|close| Some((after.get(..close)?.parse::<usize>().ok()?, close)))
            .and_then(|(k, close)| Some((fragments.get(k)?, close)));
        match token {
            Some((fragment, close)) => {
                if !plain.is_empty() {
                    pieces.push(Event::Text(CowStr::from(std::mem::take(&mut plain))));
                }
                pieces.push(Event::InlineHtml(CowStr::from(fragment.inline.clone())));
                rest = after.get(close + CLOSE.len_utf8()..).unwrap_or("");
            }
            // Not a token of ours (the sentinels were removed from the page, so this cannot be reached
            // from page text): keep the character and go on.
            None => {
                plain.push(OPEN);
                rest = after;
            }
        }
    }
    plain.push_str(rest);
    if !plain.is_empty() {
        pieces.push(Event::Text(CowStr::from(plain)));
    }
    pieces
}

#[cfg(test)]
mod tests;
