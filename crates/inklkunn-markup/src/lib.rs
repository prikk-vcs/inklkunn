#![forbid(unsafe_code)]
//! `inklkunn-markup` — the Portable Wiki Markdown profile (wiki-commons chapter 08) for inklkunn:
//! frontmatter in the plain YAML subset (chapter 09), the profile's Layer 2 constructs, link
//! resolution by the MKUP-5 order, rendering to HTML with the MKUP-12 conventions, lint with the
//! guideline each finding comes from, and the title-to-path function of RFC 001 D-6.
//!
//! **Pure by rule** (RFC 001 D-2 d): no I/O, no clock, no randomness, no process. The boundary gate
//! enforces it. Everything here is a function of its inputs, which is what lets the wiki-commons
//! conformance corpus be this crate's acceptance test (`tests/corpus.rs`).
//!
//! The crate is organized by the question it answers:
//! - [`frontmatter`]: what metadata does this page carry?
//! - [`scan`](mod@scan): which profile constructs does this page contain, and where?
//! - [`resolve`](mod@resolve): where does a free link go, by which rule?
//! - [`anchor`]: what is the fragment of a heading?
//! - [`path`]: where does a page with this title live under `pages/`?
//! - [`render`]: what HTML does this page become? (Block identifiers as `id` attributes, transclusion to
//!   depth two with cycles refused, and the vocabulary of everything it may write.)
//! - [`lint`](mod@lint): what could be better, as a suggestion, never a blocker (AUTH-13)?

pub mod anchor;
pub mod frontmatter;
pub mod lint;
pub mod path;
pub mod render;
pub mod resolve;
pub mod scan;
pub(crate) mod structure;
pub(crate) mod text;

pub use frontmatter::{Metadata, Value, YamlError};
pub use lint::{Context as LintContext, Finding, Level, lint};
pub use path::{PathError, collision_key, path_to_title, title_to_path, with_suffix};
pub use render::{
    AuthorHtml, LinkHref, MAX_TRANSCLUDED_BYTES, MAX_TRANSCLUSION_DEPTH, MAX_TRANSCLUSIONS,
    NoLinks, RenderLinks, RenderOptions, Rendered, TocEntry, Transclusion, TransclusionRefusal,
    render_html, render_page, render_page_with, render_page_with_html, render_with,
};
pub use resolve::{Index, PageEntry, Resolution, Via, resolve};
pub use scan::{Link, ScanOptions, ScanReport, scan, scan_with};
pub use text::{
    ALLOWED_SCHEMES, escape_href, escape_html, href_is_allowed, nfc_trim, normalize, percent_encode,
};

/// A page: its frontmatter (if any), its typed metadata, its body and the scan of its constructs.
#[derive(Debug, Clone)]
pub struct Page {
    /// The frontmatter as parsed, `None` when the file has none.
    pub frontmatter: Option<Value>,
    /// The vocabulary of chapter 09 read out of the frontmatter; defaults when absent.
    pub metadata: Metadata,
    /// The body text (everything after the frontmatter).
    pub body: String,
    /// The 1-based line on which the body starts in the file.
    pub body_start_line: usize,
    /// Every construct found in the file (frontmatter and body), with file line numbers.
    pub scan: ScanReport,
}

/// Why a page could not be parsed. Only the frontmatter can fail: a body is always readable text
/// (Cunningham's *Tolerant*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The frontmatter is not in the plain YAML subset (an anchor, a tag, a malformed line).
    Frontmatter(YamlError),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Frontmatter(e) => write!(f, "frontmatter: {e}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Read only the metadata of a page file: the frontmatter parsed against the plain subset and the vocabulary
/// read out of it, without scanning the body. What the title index needs of every page at a tip, where
/// [`Page::parse`] would scan thousands of bodies for nothing.
pub fn parse_metadata(text: &str) -> Result<Metadata, ParseError> {
    let (yaml, _, _) = frontmatter::split(text);
    let frontmatter = match yaml {
        Some(y) => Some(frontmatter::parse_yaml(y).map_err(ParseError::Frontmatter)?),
        None => None,
    };
    Ok(Metadata::from_value(frontmatter.as_ref()))
}

impl Page {
    /// Parse a page file. The frontmatter is validated against the plain subset; everything else is
    /// accepted and scanned.
    pub fn parse(text: &str) -> Result<Page, ParseError> {
        let (yaml, body, body_start_line) = frontmatter::split(text);
        let frontmatter = match yaml {
            Some(y) => Some(frontmatter::parse_yaml(y).map_err(ParseError::Frontmatter)?),
            None => None,
        };
        let metadata = Metadata::from_value(frontmatter.as_ref());
        // The same scan a caller gets from `scan()`: frontmatter fields included.
        let scan = scan::scan_with(text, ScanOptions::default());
        Ok(Page {
            frontmatter,
            metadata,
            body: body.to_owned(),
            body_start_line,
            scan,
        })
    }
}
