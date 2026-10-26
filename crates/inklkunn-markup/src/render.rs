//! Rendering to HTML with the MKUP-12 conventions: `id` on headings and identified blocks,
//! `class="wikilink"` on internal links and `wikilink wikilink-missing` on dangling ones, a
//! `data-wiki-target` with the title as written, `rel="nofollow ugc"` on external links, callouts as
//! `<aside class="callout">`, directives as `data-directive` elements whose fallback content renders.
//!
//! **No sanitization here.** The output may carry whatever raw HTML the source had; `inklkunn-web`
//! sanitizes before anything reaches a browser (threat model C-1). This crate only has to be faithful.
//!
//! How it works: the body goes through a line-level pre-pass (container directives become `<div>`/`<aside>`
//! wrappers, since CommonMark has no container syntax), then through the scanner's masked view so that
//! free links, embeds, inline tags, inline directives and trailing block identifiers are replaced by
//! inline HTML **in the source**, where the scanner has already excluded code, comments and escapes; the
//! result is parsed by `pulldown-cmark`, whose event stream is then rewritten for heading ids, callouts
//! and external-link attributes before `pulldown-cmark`'s own HTML writer emits the page.

use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, html};

use crate::anchor::{Anchors, github_anchor};
use crate::scan::{self, FragmentKind, Link, ScanOptions};
use crate::text::{escape_href, escape_html, sl};

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
    /// Decide a free link's destination.
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
    /// The HTML fragment (no `<html>`/`<body>`; the web layer wraps it in `<article>`).
    pub html: String,
    /// Headings in order.
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
pub fn render_with(body: &str, links: &dyn RenderLinks, options: ScanOptions) -> Rendered {
    let pre = containers_pre_pass(body);
    let source = inline_pre_pass(&pre, links, options);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let events: Vec<Event<'_>> = Parser::new_ext(&source, opts).collect();
    let events = rewrite_callouts(events);
    let (events, toc) = rewrite_headings(events);
    let events = rewrite_external_links(events);
    let mut out = String::with_capacity(source.len() * 2);
    html::push_html(&mut out, events.into_iter());
    Rendered { html: out, toc }
}

// ---------------------------------------------------------------- pre-passes

/// `::: name key=value` … `:::` → wrappers. Fenced code is left alone.
fn containers_pre_pass(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 64);
    let mut fence: Option<char> = None;
    let mut stack: Vec<&'static str> = Vec::new();
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let trimmed = content.trim_start_matches([' ', '\t']);
        let fence_here: Option<char> = if trimmed.len() + 3 >= content.len()
            && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
        {
            trimmed.chars().next()
        } else {
            None
        };
        match fence {
            Some(ch) => {
                if fence_here == Some(ch) && trimmed.trim().chars().all(|c| c == ch) {
                    fence = None;
                }
                out.push_str(line);
                continue;
            }
            None => {
                if let Some(ch) = fence_here {
                    fence = Some(ch);
                    out.push_str(line);
                    continue;
                }
            }
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
                    // The title is the free text after the name, when no attributes were written.
                    let title = if attrs.is_empty() { attrs_text } else { "" };
                    out.push_str(&callout_open(
                        &lname.to_uppercase(),
                        None,
                        (!title.is_empty()).then_some(title),
                    ));
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

/// Replace inline constructs with HTML in the source, using the scanner's masked positions.
fn inline_pre_pass(body: &str, links: &dyn RenderLinks, options: ScanOptions) -> String {
    let report = scan::scan_body(body, 0, options);
    // (start, end, replacement) for every construct, then applied back to front.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for l in &report.links {
        edits.push((l.start, l.end, render_link(l, links)));
    }
    for (start, end, tag) in tag_spans(body, &report) {
        let inner = escape_html(&format!("#{tag}"));
        let html = match links.tag(&tag) {
            Some(href) => format!(
                "<a class=\"tag\" href=\"{}\" data-tag=\"{}\">{}</a>",
                escape_href(&href),
                escape_html(&tag),
                inner
            ),
            None => format!(
                "<span class=\"tag\" data-tag=\"{}\">{}</span>",
                escape_html(&tag),
                inner
            ),
        };
        edits.push((start, end, html));
    }
    for (start, end, d) in inline_directive_spans(body, &report) {
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
        edits.push((start, end, html));
    }
    for (start, end, id) in block_id_spans(body, &report) {
        edits.push((
            start,
            end,
            format!(
                "<span class=\"block-id\" id=\"^{}\"></span>",
                escape_html(&id)
            ),
        ));
    }
    edits.sort_by_key(|(s, _, _)| *s);
    // Drop overlaps (a tag inside a link cannot happen after masking, but be safe).
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
    out
}

/// Positions of the inline tags the scan reported: re-found in order on their lines.
fn tag_spans(body: &str, report: &scan::ScanReport) -> Vec<(usize, usize, String)> {
    // The scanner reports tag names without offsets; find each in turn from the previous one.
    let mut out = Vec::new();
    let mut from = 0;
    for tag in &report.tags.inline {
        let needle = format!("#{tag}");
        let mut search = from;
        while let Some(off) = body.get(search..).and_then(|rest| rest.find(&needle)) {
            let s = search + off;
            let e = s + needle.len();
            let prev_ok = s == 0 || sl(body, 0, s).ends_with([' ', '\t', '\n', '(']);
            if prev_ok && !overlaps(report, s, e) {
                out.push((s, e, tag.clone()));
                from = e;
                break;
            }
            search = s + 1;
        }
    }
    out
}

fn overlaps(report: &scan::ScanReport, s: usize, e: usize) -> bool {
    report.links.iter().any(|l| s < l.end && l.start < e)
}

fn inline_directive_spans(
    body: &str,
    report: &scan::ScanReport,
) -> Vec<(usize, usize, scan::Directive)> {
    let mut out = Vec::new();
    let mut from = 0;
    for d in report.directives.iter().filter(|d| d.inline) {
        let needle = format!(":{}[{}]{{", d.name, d.text.as_deref().unwrap_or(""));
        if let Some(off) = body.get(from..).and_then(|rest| rest.find(&needle)) {
            let s = from + off;
            if let Some(brace) = body.get(s + needle.len()..).and_then(|rest| rest.find('}')) {
                let e = s + needle.len() + brace + 1;
                out.push((s, e, d.clone()));
                from = e;
            }
        }
    }
    out
}

fn block_id_spans(body: &str, report: &scan::ScanReport) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut line_starts = vec![0usize];
    for (i, b) in body.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }
    for b in &report.block_ids {
        let Some(&ls) = line_starts.get(b.line - 1) else {
            continue;
        };
        let le = line_starts.get(b.line).map_or(body.len(), |n| n - 1);
        let line = sl(body, ls, le).trim_end_matches(['\r', ' ', '\t']);
        let Some(caret) = line.rfind('^') else {
            continue;
        };
        // Take the separating space with the token so no stray space remains.
        let start = if caret > 0 { caret - 1 } else { caret };
        out.push((ls + start, ls + line.len(), b.id.clone()));
    }
    out
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
    match links.link(l) {
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
            format!(
                "<img class=\"wikilink-embed\" src=\"{}\" alt=\"{}\"{width}>",
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
            match href {
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
            format!(
                "<a class=\"wikilink wikilink-interwiki\" href=\"{}\" rel=\"nofollow ugc\"{target_attr}>{}</a>",
                escape_href(&url),
                escape_html(l.label.as_deref().unwrap_or(rest))
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

/// `> [!TYPE]± Title` block quotes become `<aside class="callout">`.
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
                        m.title.as_deref(),
                    ))));
                    quotes.push(true);
                    if m.keep_paragraph {
                        out.push(Event::Start(Tag::Paragraph));
                    }
                    i += 1 + m.consumed;
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

/// A recognized callout marker: type, fold, title, how many events it spans, whether a paragraph remains.
struct Marker {
    kind: String,
    fold: Option<String>,
    title: Option<String>,
    consumed: usize,
    keep_paragraph: bool,
}

/// If `events[at..]` is `Start(Paragraph) Text… (SoftBreak|End(Paragraph))` whose text is a callout
/// marker, return it.
fn callout_marker(events: &[Event<'_>], at: usize) -> Option<Marker> {
    if !matches!(events.get(at), Some(Event::Start(Tag::Paragraph))) {
        return None;
    }
    let mut text = String::new();
    let mut j = at + 1;
    while let Some(Event::Text(t)) = events.get(j) {
        text.push_str(t);
        j += 1;
    }
    let first_line = format!("> {text}");
    let (kind, fold, title) = callout_parts_public(&first_line)?;
    let keep_paragraph = match events.get(j) {
        Some(Event::SoftBreak) | Some(Event::HardBreak) => true,
        Some(Event::End(TagEnd::Paragraph)) => false,
        _ => return None,
    };
    Some(Marker {
        kind,
        fold,
        title,
        consumed: j + 1 - at,
        keep_paragraph,
    })
}

fn callout_parts_public(line: &str) -> Option<(String, Option<String>, Option<String>)> {
    let r = scan::scan_body(line, 0, ScanOptions::default());
    let c = r.callouts.into_iter().next()?;
    Some((c.kind, c.fold, c.title))
}

/// Give every heading an `id` (GitHub style, duplicates suffixed) and collect the table of contents.
fn rewrite_headings<'a>(events: Vec<Event<'a>>) -> (Vec<Event<'a>>, Vec<TocEntry>) {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut toc = Vec::new();
    let mut anchors = Anchors::new();
    let mut i = 0;
    while i < events.len() {
        if let Some(Event::Start(Tag::Heading {
            level,
            id,
            classes,
            attrs,
        })) = events.get(i)
        {
            // Collect the text up to the matching end.
            let mut text = String::new();
            let mut j = i + 1;
            while let Some(e) = events.get(j) {
                match e {
                    Event::End(TagEnd::Heading(_)) => break,
                    Event::Text(t) | Event::Code(t) => text.push_str(t),
                    Event::InlineHtml(h) | Event::Html(h) => text.push_str(&strip_tags(h)),
                    Event::SoftBreak | Event::HardBreak => text.push(' '),
                    _ => {}
                }
                j += 1;
            }
            let anchor = match id {
                Some(explicit) => explicit.to_string(),
                None => anchors.next(&text),
            };
            toc.push(TocEntry {
                level: heading_level(*level),
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
        if let Some(e) = events.get(i) {
            out.push(e.clone());
        }
        i += 1;
    }
    (out, toc)
}

fn heading_level(level: pulldown_cmark::HeadingLevel) -> u8 {
    use pulldown_cmark::HeadingLevel::*;
    match level {
        H1 => 1,
        H2 => 2,
        H3 => 3,
        H4 => 4,
        H5 => 5,
        H6 => 6,
    }
}

/// Remove `<…>` tags and decode the few entities this crate emits.
fn strip_tags(h: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in h.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
}

/// External Markdown links get `rel="nofollow ugc"` (MKUP-12) by emitting the anchor directly.
fn rewrite_external_links<'a>(events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut open_external = false;
    for e in events {
        match e {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let external = dest_url.starts_with("http://")
                    || dest_url.starts_with("https://")
                    || dest_url.starts_with("//");
                if external {
                    open_external = true;
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
                    out.push(Event::Start(Tag::Link {
                        link_type: match link_type {
                            LinkType::Autolink => LinkType::Autolink,
                            other => other,
                        },
                        dest_url,
                        title,
                        id,
                    }));
                }
            }
            Event::End(TagEnd::Link) if open_external => {
                open_external = false;
                out.push(Event::Html(CowStr::from("</a>")));
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests;
