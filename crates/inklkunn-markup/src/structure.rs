//! The CommonMark structure of a body, from `pulldown-cmark` itself, so that the scanner and the renderer
//! agree on one truth about what is code, what is HTML and where the headings are. The hand-written
//! tokenizers this replaces disagreed with the parser on indented code, multi-line code spans and fence
//! lengths; the parser's own token ranges cannot.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// The parser options the profile uses, shared by the structure pass and the renderer.
pub(crate) fn options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// A heading as the parser sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedHeading {
    /// The byte range of the whole heading (ATX line, or setext text and underline).
    pub range: Range<usize>,
    /// 1–6.
    pub level: u8,
    /// The plain text of the inline content: text and code joined, HTML dropped, breaks as spaces.
    pub text: String,
}

/// Byte ranges of the regions in which Layer 2 syntax is not syntax.
#[derive(Debug, Clone, Default)]
pub(crate) struct Structure {
    /// Fenced and indented code blocks, inline code spans (fences and backticks included).
    pub code: Vec<Range<usize>>,
    /// HTML blocks and inline HTML, comments among them.
    pub html: Vec<Range<usize>>,
    /// Markdown links, images and autolinks (whole construct), for the tag scan to skip.
    pub links: Vec<Range<usize>>,
    /// Headings in document order.
    pub headings: Vec<ParsedHeading>,
}

impl Structure {
    /// The structure of `body`.
    pub(crate) fn of(body: &str) -> Self {
        let mut s = Structure::default();
        let mut heading: Option<(Range<usize>, u8, String)> = None;
        let mut link_depth = 0usize;
        for (event, range) in Parser::new_ext(body, options()).into_offset_iter() {
            match event {
                Event::Start(Tag::CodeBlock(_)) => s.code.push(range),
                Event::Code(_) => s.code.push(range),
                Event::Html(_) | Event::InlineHtml(_) => s.html.push(range),
                Event::Start(Tag::Link { .. }) | Event::Start(Tag::Image { .. }) => {
                    if link_depth == 0 {
                        s.links.push(range);
                    }
                    link_depth += 1;
                }
                Event::End(TagEnd::Link) | Event::End(TagEnd::Image) => {
                    link_depth = link_depth.saturating_sub(1);
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    heading = Some((range, level_u8(level), String::new()));
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some((range, level, text)) = heading.take() {
                        s.headings.push(ParsedHeading {
                            range,
                            level,
                            text: text.trim().to_owned(),
                        });
                    }
                }
                Event::Text(t) => {
                    if let Some((_, _, text)) = &mut heading {
                        text.push_str(&t);
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some((_, _, text)) = &mut heading {
                        text.push(' ');
                    }
                }
                _ => {}
            }
        }
        s
    }
}

pub(crate) fn level_u8(level: pulldown_cmark::HeadingLevel) -> u8 {
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
