//! Rendering to HTML with the MKUP-12 conventions: `id` on headings and identified blocks,
//! `class="wikilink"` on internal links and `wikilink wikilink-missing` on dangling ones, a
//! `data-wiki-target` with the title as written, `rel="nofollow ugc"` on external links, callouts as
//! `<aside class="callout">`, directives as `data-directive` elements whose fallback content renders.
//!
//! **The output is not sanitized** and its field says so ([`Rendered::unsanitized_html`]): it may carry
//! whatever raw HTML the source had. `inklkunn-web` sanitizes before anything reaches a browser (threat
//! model C-1; RFC 003 D-3). This crate is faithful, and defends in depth where it writes an `href` itself:
//! a scheme outside [`crate::text::ALLOWED_SCHEMES`] is never emitted as a link.
//!
//! How it works: the body goes through a line-level pre-pass (container directives become `<div>`/`<aside>`
//! wrappers, since CommonMark has no container syntax; lines inside code or HTML comments are left alone),
//! then the scanner finds free links, embeds, inline tags, inline directives and trailing block identifiers
//! **with their byte spans**, which are replaced by inline HTML in the source; the result is parsed by
//! `pulldown-cmark`, whose event stream is then rewritten for heading ids (the scanner's anchors, so links
//! and ids agree), callouts and link attributes before `pulldown-cmark`'s own HTML writer emits the page.

use std::collections::BTreeMap;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, Parser, Tag, TagEnd, html};

use crate::anchor::{Anchors, github_anchor};
use crate::scan::{self, FragmentKind, Link, ScanOptions};
use crate::structure::{Structure, level_u8, options};
use crate::text::{LineIndex, escape_href, escape_html, href_is_allowed, sl};

/// Where a link should go, decided by the caller (the web layer knows the URL scheme; this crate does not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkHref {
    /// An existing page: its href and, when known, its title (the default label).
    Page {
        /// The href of the page.
        href: String,
        /// The page title, used as the label when none is written.
        title: Option<String>,
    },
    /// No such page: an optional href (to create it) and the `wikilink-missing` class.
    Missing {
        /// Where a click leads, usually the create-page form; `None` renders a non-link span.
        href: Option<String>,
    },
    /// Outside this wiki (interwiki): `rel="nofollow ugc"`.
    External(String),
    /// An attachment (an image embed resolves here).
    Attachment(String),
}

/// The caller's knowledge of where links and tags lead.
pub trait RenderLinks {
    /// Decide a free link's destination. `link.start`/`link.end` are offsets into the text the renderer
    /// scanned (the body after the container pre-pass), not into the caller's body.
    fn link(&self, link: &Link) -> LinkHref;
    /// The href of a tag page, if tags have one.
    fn tag(&self, _tag: &str) -> Option<String> {
        None
    }
}

/// Every link is missing and nothing has an href: the renderer's own tests and the fallback when no
/// index exists.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoLinks;

impl RenderLinks for NoLinks {
    fn link(&self, _link: &Link) -> LinkHref {
        LinkHref::Missing { href: None }
    }
}

/// A heading of the rendered page, for tables of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// 1–6.
    pub level: u8,
    /// The `id` attribute.
    pub id: String,
    /// The plain text.
    pub text: String,
}

/// The rendered page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// The HTML fragment (no `<html>`/`<body>`), **not sanitized**: it must pass through the web layer's
    /// sanitizer before any browser sees it.
    pub unsanitized_html: String,
    /// Headings in order, with the ids written into the HTML.
    pub toc: Vec<TocEntry>,
}

/// The five shared callout types (MKUP-15); other types render as a generic callout.
pub const SHARED_CALLOUTS: &[&str] = &["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];

/// Container-directive names that are admonitions and render as callouts (MKUP-15).
const ADMONITION_NAMES: &[&str] = &[
    "note",
    "tip",
    "important",
    "warning",
    "caution",
    "info",
    "hint",
    "danger",
    "error",
    "attention",
    "question",
    "example",
    "quote",
    "abstract",
    "summary",
    "todo",
];

/// Image file extensions an embed `![[name.ext]]` renders as `<img>` (MKUP-10).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg", "webp", "avif", "bmp"];

/// Render a page body (frontmatter already removed) to HTML.
pub fn render_html(body: &str, links: &dyn RenderLinks) -> Rendered {
    render_with(body, links, ScanOptions::default())
}

/// Render with scan options.
pub fn render_with(body: &str, links: &dyn RenderLinks, options_: ScanOptions) -> Rendered {
    let pre = containers_pre_pass(body);
    let (source, report) = inline_pre_pass(&pre, links, options_);
    let scan_anchors: BTreeMap<usize, String> = report
        .headings
        .iter()
        .map(|h| (h.line, h.anchor.clone()))
        .collect();
    let lines = LineIndex::new(&source);
    let with_ranges: Vec<(Event<'_>, Range<usize>)> = Parser::new_ext(&source, options())
        .into_offset_iter()
        .collect();
    let (events, toc) = rewrite_headings(with_ranges, &lines, &scan_anchors);
    let events = rewrite_callouts(events);
    let events = rewrite_links(events);
    let mut out = String::with_capacity(source.len() * 2);
    html::push_html(&mut out, events.into_iter());
    Rendered {
        unsanitized_html: out,
        toc,
    }
}

// ---------------------------------------------------------------- pre-passes

/// `::: name key=value` … `:::` → wrappers. Lines inside code or HTML (comments) are left alone.
fn containers_pre_pass(body: &str) -> String {
    let structure = Structure::of(body);
    let mut hidden: Vec<Range<usize>> = structure
        .code
        .iter()
        .chain(&structure.html)
        .cloned()
        .collect();
    hidden.sort_by_key(|r| (r.start, r.end));
    // Lines come in offset order, so one cursor over the sorted ranges answers every line in all linear time.
    let mut cursor = 0usize;
    let mut is_hidden = |offset: usize| {
        while hidden.get(cursor).is_some_and(|r| r.end <= offset) {
            cursor += 1;
        }
        hidden.get(cursor).is_some_and(|r| r.contains(&offset))
    };
    let mut out = String::with_capacity(body.len() + 64);
    let mut stack: Vec<&'static str> = Vec::new();
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        let first_non_space = line_start + (content.len() - content.trim_start().len());
        if is_hidden(line_start) || is_hidden(first_non_space) {
            out.push_str(line);
            continue;
        }
        if let Some(rest) = content.strip_prefix(":::") {
            let rest_trim = rest.trim_start_matches([' ', '\t']);
            if rest_trim.trim().is_empty() {
                if let Some(closer) = stack.pop() {
                    out.push('\n');
                    out.push_str(closer);
                    out.push('\n');
                    continue;
                }
            } else if rest_trim.starts_with(|c: char| c.is_ascii_alphabetic()) {
                let name_end = rest_trim
                    .char_indices()
                    .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == '-'))
                    .map_or(rest_trim.len(), |(i, _)| i);
                let name = sl(rest_trim, 0, name_end);
                let attrs_text = rest_trim.get(name_end..).unwrap_or("").trim();
                let attrs = scan::parse_attrs(attrs_text);
                let lname = name.to_ascii_lowercase();
                if ADMONITION_NAMES.contains(&lname.as_str()) {
                    // The title is the free text before the first `key=value`, if any.
                    let title = title_before_attrs(attrs_text);
                    out.push_str(&callout_open(&lname.to_uppercase(), None, title));
                    out.push_str("\n\n");
                    stack.push("</aside>");
                } else {
                    out.push_str(&format!(
                        "<div class=\"directive directive-{}\" data-directive=\"{}\"{}>\n",
                        escape_html(&lname),
                        escape_html(name),
                        data_attrs(&attrs)
                    ));
                    out.push('\n');
                    stack.push("</div>");
                }
                continue;
            }
        }
        out.push_str(line);
    }
    for closer in stack.into_iter().rev() {
        out.push('\n');
        out.push_str(closer);
        out.push('\n');
    }
    out
}

/// `Heads up depth=2` → `Heads up`: the words before the first `key=` are the title.
fn title_before_attrs(attrs_text: &str) -> Option<&str> {
    let mut end = attrs_text.len();
    for (i, word) in attrs_text.match_indices(|c: char| !c.is_whitespace()) {
        let _ = word;
        let rest = attrs_text.get(i..).unwrap_or("");
        let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let token = sl(rest, 0, token_end);
        if token.contains('=') && token.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            end = i;
            break;
        }
    }
    let t = sl(attrs_text, 0, end).trim();
    (!t.is_empty()).then_some(t)
}

fn data_attrs(attrs: &scan::Attrs) -> String {
    let mut s = String::new();
    for (k, v) in attrs {
        if k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            s.push_str(&format!(
                " data-attr-{}=\"{}\"",
                k.to_ascii_lowercase(),
                escape_html(v)
            ));
        }
    }
    s
}

fn callout_open(kind: &str, fold: Option<&str>, title: Option<&str>) -> String {
    let mut s = format!(
        "<aside class=\"callout callout-{}\" data-callout=\"{}\"",
        escape_html(&kind.to_ascii_lowercase()),
        escape_html(kind)
    );
    if let Some(f) = fold {
        s.push_str(&format!(" data-fold=\"{}\"", escape_html(f)));
    }
    s.push('>');
    if let Some(t) = title {
        s.push_str(&format!(
            "<p class=\"callout-title\">{}</p>",
            escape_html(t)
        ));
    }
    s
}

/// Replace inline constructs with HTML in the source, at the byte spans the scanner reported. Returns the
/// new source and the scan it was built from (the heading anchors come from it).
fn inline_pre_pass(
    body: &str,
    links: &dyn RenderLinks,
    options_: ScanOptions,
) -> (String, scan::ScanReport) {
    let report = scan::scan_body(body, 0, options_);
    // (start, end, replacement) for every construct, then applied front to back.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for l in &report.links {
        edits.push((l.start, l.end, render_link(l, links)));
    }
    for (tag, (start, end)) in report.tags.inline.iter().zip(&report.tag_spans) {
        let inner = escape_html(&format!("#{tag}"));
        let html = match links.tag(tag).filter(|h| href_is_allowed(h)) {
            Some(href) => format!(
                "<a class=\"tag\" href=\"{}\" data-tag=\"{}\">{}</a>",
                escape_href(&href),
                escape_html(tag),
                inner
            ),
            None => format!(
                "<span class=\"tag\" data-tag=\"{}\">{}</span>",
                escape_html(tag),
                inner
            ),
        };
        edits.push((*start, *end, html));
    }
    for d in report.directives.iter().filter(|d| d.inline) {
        let text = escape_html(d.text.as_deref().unwrap_or(""));
        let html = if d.name == "kbd" {
            format!("<kbd{}>{text}</kbd>", data_attrs(&d.attrs))
        } else {
            format!(
                "<span class=\"directive directive-{}\" data-directive=\"{}\"{}>{text}</span>",
                escape_html(&d.name.to_ascii_lowercase()),
                escape_html(&d.name),
                data_attrs(&d.attrs)
            )
        };
        edits.push((d.start, d.end, html));
    }
    for b in &report.block_ids {
        edits.push((
            b.start,
            b.end,
            format!(
                "<span class=\"block-id\" id=\"^{}\"></span>",
                escape_html(&b.id)
            ),
        ));
    }
    edits.sort_by_key(|(s, _, _)| *s);
    let mut out = String::with_capacity(body.len() + 256);
    let mut pos = 0;
    for (s, e, html) in edits {
        if s < pos {
            continue;
        }
        out.push_str(sl(body, pos, s));
        out.push_str(&html);
        pos = e;
    }
    out.push_str(body.get(pos..).unwrap_or(""));
    (out, report)
}

fn render_link(l: &Link, links: &dyn RenderLinks) -> String {
    let fragment = match (&l.fragment, l.fragment_kind) {
        (Some(f), Some(FragmentKind::Heading)) => format!("#{}", github_anchor(f)),
        (Some(f), Some(FragmentKind::Block)) => format!("#^{f}"),
        _ => String::new(),
    };
    let target_attr = format!(" data-wiki-target=\"{}\"", escape_html(&l.target));
    if l.target.is_empty() {
        // Same-page link.
        let label = l
            .label
            .clone()
            .or_else(|| l.fragment.clone())
            .unwrap_or_default();
        return format!(
            "<a class=\"wikilink wikilink-self\" href=\"{}\">{}</a>",
            escape_href(&fragment),
            escape_html(&label)
        );
    }
    let last = l.target.rsplit('/').next().unwrap_or(&l.target).to_owned();
    let is_image = l.embed
        && l.target
            .rsplit('.')
            .next()
            .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
    let blocked = |label: &str| {
        format!(
            "<span class=\"wikilink wikilink-blocked\"{target_attr}>{}</span>",
            escape_html(label)
        )
    };
    match links.link(l) {
        LinkHref::Attachment(href) if !href_is_allowed(&href) => blocked(&last),
        LinkHref::Attachment(href) if is_image => {
            let alt = l
                .label
                .clone()
                .filter(|s| !s.chars().all(|c| c.is_ascii_digit() || c == 'x'))
                .unwrap_or(last);
            let width = l
                .label
                .as_deref()
                .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
                .map(|w| format!(" width=\"{w}\""))
                .unwrap_or_default();
            // Wrapped so that an embed alone on a line is not an HTML block that swallows the lines after it.
            format!(
                "<span class=\"embed\"><img class=\"wikilink-embed\" src=\"{}\" alt=\"{}\"{width}></span>",
                escape_href(&href),
                escape_html(&alt)
            )
        }
        LinkHref::Attachment(href) => format!(
            "<a class=\"wikilink wikilink-attachment\" href=\"{}\"{target_attr}>{}</a>",
            escape_href(&href),
            escape_html(l.label.as_deref().unwrap_or(&last))
        ),
        LinkHref::Page { href, title } => {
            let label = default_label(l, title.as_deref().unwrap_or(&last));
            if !href_is_allowed(&href) {
                return blocked(&label);
            }
            let class = if l.embed {
                "wikilink wikilink-embed"
            } else {
                "wikilink"
            };
            format!(
                "<a class=\"{class}\" href=\"{}{}\"{target_attr}>{}</a>",
                escape_href(&href),
                escape_href(&fragment),
                escape_html(&label)
            )
        }
        LinkHref::Missing { href } => {
            let label = default_label(l, &last);
            match href.filter(|h| href_is_allowed(h)) {
                Some(h) => format!(
                    "<a class=\"wikilink wikilink-missing\" href=\"{}\"{target_attr}>{}</a>",
                    escape_href(&h),
                    escape_html(&label)
                ),
                None => format!(
                    "<span class=\"wikilink wikilink-missing\"{target_attr}>{}</span>",
                    escape_html(&label)
                ),
            }
        }
        LinkHref::External(url) => {
            let rest = l
                .target
                .split_once(':')
                .map_or(l.target.as_str(), |(_, r)| r.trim());
            let label = l.label.as_deref().unwrap_or(rest);
            if !href_is_allowed(&url) {
                return blocked(label);
            }
            format!(
                "<a class=\"wikilink wikilink-interwiki\" href=\"{}\" rel=\"nofollow ugc\"{target_attr}>{}</a>",
                escape_href(&url),
                escape_html(label)
            )
        }
    }
}

fn default_label(l: &Link, title: &str) -> String {
    if let Some(label) = &l.label {
        return label.clone();
    }
    match (&l.fragment, l.fragment_kind) {
        (Some(f), Some(FragmentKind::Heading)) => format!("{title} › {f}"),
        _ => title.to_owned(),
    }
}

// ---------------------------------------------------------------- event rewrites

/// Give every heading its `id`: the scanner's anchor for the ATX heading on that line (so `[[Page#Heading]]`
/// lands on it), a fresh unique one for a setext heading the scanner does not report. Collects the table
/// of contents.
fn rewrite_headings<'a>(
    events: Vec<(Event<'a>, Range<usize>)>,
    lines: &LineIndex,
    scan_anchors: &BTreeMap<usize, String>,
) -> (Vec<Event<'a>>, Vec<TocEntry>) {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut toc = Vec::new();
    let mut anchors = Anchors::new();
    for a in scan_anchors.values() {
        anchors.reserve(a);
    }
    let mut i = 0;
    while i < events.len() {
        if let Some((
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }),
            range,
        )) = events.get(i)
        {
            // Collect the text up to the matching end.
            let mut text = String::new();
            let mut j = i + 1;
            while let Some((e, _)) = events.get(j) {
                match e {
                    Event::End(TagEnd::Heading(_)) => break,
                    Event::Text(t) | Event::Code(t) => text.push_str(t),
                    Event::SoftBreak | Event::HardBreak => text.push(' '),
                    _ => {}
                }
                j += 1;
            }
            let line = lines.line_of(range.start);
            let anchor = match id {
                Some(explicit) => explicit.to_string(),
                None => match scan_anchors.get(&line) {
                    Some(a) => a.clone(),
                    None => anchors.next(&text),
                },
            };
            toc.push(TocEntry {
                level: level_u8(*level),
                id: anchor.clone(),
                text: text.trim().to_owned(),
            });
            out.push(Event::Start(Tag::Heading {
                level: *level,
                id: Some(CowStr::from(anchor)),
                classes: classes.clone(),
                attrs: attrs.clone(),
            }));
            i += 1;
            continue;
        }
        if let Some((e, _)) = events.get(i) {
            out.push(e.clone());
        }
        i += 1;
    }
    (out, toc)
}

/// `> [!TYPE]± Title` block quotes become `<aside class="callout">`; the title may carry inline markup.
fn rewrite_callouts<'a>(events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    // For each open block quote: was it turned into an aside?
    let mut quotes: Vec<bool> = Vec::new();
    let mut i = 0;
    while i < events.len() {
        match events.get(i) {
            Some(Event::Start(Tag::BlockQuote(_))) => {
                if let Some(m) = callout_marker(&events, i + 1) {
                    out.push(Event::Html(CowStr::from(callout_open(
                        &m.kind,
                        m.fold.as_deref(),
                        None,
                    ))));
                    if m.has_title {
                        out.push(Event::Html(CowStr::from("<p class=\"callout-title\">")));
                        if let Some(t) = m.title_text {
                            out.push(Event::Text(CowStr::from(t)));
                        }
                        for e in events.get(m.title_events.clone()).unwrap_or(&[]) {
                            out.push(e.clone());
                        }
                        out.push(Event::Html(CowStr::from("</p>")));
                    }
                    quotes.push(true);
                    if m.keep_paragraph {
                        out.push(Event::Start(Tag::Paragraph));
                    }
                    i = m.resume;
                    continue;
                }
                quotes.push(false);
                out.push(Event::Start(Tag::BlockQuote(None)));
            }
            Some(Event::End(TagEnd::BlockQuote(_))) => {
                if quotes.pop().unwrap_or(false) {
                    out.push(Event::Html(CowStr::from("</aside>")));
                } else {
                    out.push(Event::End(TagEnd::BlockQuote(None)));
                }
            }
            Some(e) => out.push(e.clone()),
            None => break,
        }
        i += 1;
    }
    out
}

/// A recognized callout marker and how to continue.
struct Marker {
    kind: String,
    fold: Option<String>,
    /// The title's leading text (after the marker), if any.
    title_text: Option<String>,
    /// The events of the rest of the marker line (inline markup in the title).
    title_events: Range<usize>,
    has_title: bool,
    /// Resume rendering at this event index.
    resume: usize,
    /// A paragraph continues after the marker line.
    keep_paragraph: bool,
}

/// If `events[at..]` is `Start(Paragraph) Text(marker…) … (SoftBreak|HardBreak|End(Paragraph))` whose
/// first text begins with a callout marker, return it.
fn callout_marker(events: &[Event<'_>], at: usize) -> Option<Marker> {
    if !matches!(events.get(at), Some(Event::Start(Tag::Paragraph))) {
        return None;
    }
    // pulldown may split `[!NOTE]` into several Text events (`[`, `!NOTE`, `]`): join the leading texts.
    let mut first_text = String::new();
    let mut j = at + 1;
    while let Some(Event::Text(t)) = events.get(j) {
        first_text.push_str(t);
        j += 1;
    }
    let (kind, fold, _) = scan::callout_marker(&first_text)?;
    let marker_len = scan::callout_marker_len(&first_text)?;
    // The rest of the first line: events up to the first break or the paragraph's end.
    let mut k = j;
    let mut depth = 0usize;
    let end_kind = loop {
        match events.get(k) {
            Some(Event::SoftBreak) | Some(Event::HardBreak) if depth == 0 => break Some(true),
            Some(Event::End(TagEnd::Paragraph)) if depth == 0 => break Some(false),
            Some(Event::Start(_)) => depth += 1,
            Some(Event::End(_)) => depth = depth.saturating_sub(1),
            Some(_) => {}
            None => break None,
        }
        k += 1;
    };
    let keep_paragraph = end_kind?;
    // The title's leading text: what followed the marker in the same text run. Its inner spacing is kept
    // when inline events continue the title; a lone title is trimmed.
    let rest = first_text.get(marker_len..).unwrap_or("").trim_start();
    let rest = if k > j { rest } else { rest.trim_end() };
    let title_text = (!rest.is_empty()).then(|| rest.to_owned());
    let has_title = title_text.is_some() || k > j;
    Some(Marker {
        kind,
        fold,
        title_text,
        title_events: j..k,
        has_title,
        resume: k + 1,
        keep_paragraph,
    })
}

/// What was done with an open link or image, so the end is matched.
enum Opened {
    /// Passed through as the parser's own tag.
    Kept,
    /// Emitted as a raw `<a …>`; closes with `</a>`.
    Anchor,
    /// Emitted as a `<span …>`; closes with `</span>`.
    Span,
}

/// External Markdown links get `rel="nofollow ugc"` (MKUP-12) by emitting the anchor directly; a link or
/// image whose destination has a scheme outside [`crate::text::ALLOWED_SCHEMES`] is emitted as a plain
/// span, its text kept.
fn rewrite_links<'a>(events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut opened: Vec<Opened> = Vec::new();
    for e in events {
        match e {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if !href_is_allowed(&dest_url) {
                    opened.push(Opened::Span);
                    out.push(Event::Html(CowStr::from("<span class=\"link-blocked\">")));
                    continue;
                }
                let scheme = dest_url.split(':').next().unwrap_or("");
                let external = dest_url.starts_with("//")
                    || (dest_url
                        .get(scheme.len()..)
                        .is_some_and(|r| r.starts_with("://"))
                        && (scheme.eq_ignore_ascii_case("http")
                            || scheme.eq_ignore_ascii_case("https")));
                if external {
                    opened.push(Opened::Anchor);
                    let mut a = format!(
                        "<a class=\"external\" href=\"{}\" rel=\"nofollow ugc\"",
                        escape_href(&dest_url)
                    );
                    if !title.is_empty() {
                        a.push_str(&format!(" title=\"{}\"", escape_html(&title)));
                    }
                    a.push('>');
                    out.push(Event::Html(CowStr::from(a)));
                } else {
                    opened.push(Opened::Kept);
                    out.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                }
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if !href_is_allowed(&dest_url) {
                    opened.push(Opened::Span);
                    out.push(Event::Html(CowStr::from("<span class=\"image-blocked\">")));
                } else {
                    opened.push(Opened::Kept);
                    out.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                }
            }
            Event::End(TagEnd::Link) => match opened.pop() {
                Some(Opened::Anchor) => out.push(Event::Html(CowStr::from("</a>"))),
                Some(Opened::Span) => out.push(Event::Html(CowStr::from("</span>"))),
                _ => out.push(Event::End(TagEnd::Link)),
            },
            Event::End(TagEnd::Image) => match opened.pop() {
                Some(Opened::Anchor) => out.push(Event::Html(CowStr::from("</a>"))),
                Some(Opened::Span) => out.push(Event::Html(CowStr::from("</span>"))),
                _ => out.push(Event::End(TagEnd::Image)),
            },
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests;
