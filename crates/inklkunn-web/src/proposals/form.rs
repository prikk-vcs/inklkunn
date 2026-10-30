//! What a person sent from the proposal form, read and checked (RFC 004 D-2; threat model C-4a … C-4d).
//!
//! **Order matters.** The form is read as plain fields; the **sizes are checked next, before any text is
//! parsed** (a page over 1 MiB or a proposal over 2 MiB is refused without a byte of it reaching the markup
//! crate's parser: [`check_bounds`] takes no parser, and a test counts the parser's calls); only then are the
//! pages' information blocks parsed, to refuse the keys the service owns ([`check_owned_keys`]).
//!
//! **Line endings.** A browser sends every line break of a text area as CR LF, whatever the page had, so the
//! line endings of a page are the page's own: a page whose line breaks were all CR LF keeps them, and every
//! other page is written with LF. A text a person never touched is therefore the page, byte for byte.

use std::collections::{BTreeMap, HashMap};

use inklkunn_core::proposal::{MAX_PAGE_BYTES, MAX_PROPOSAL_BYTES};
use inklkunn_markup::frontmatter;
use inklkunn_store::{Base, Rename};

use super::fields;
use super::parts::Selection;

/// The most pages one form carries.
pub(crate) const MAX_FORM_PAGES: usize = 20;
/// The most renames one form carries.
pub(crate) const MAX_FORM_RENAMES: usize = 20;

/// Which button was pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Preview,
    Changes,
    Draft,
    Submit,
}

impl Op {
    fn of(value: &str) -> Option<Op> {
        match value {
            fields::OP_PREVIEW => Some(Op::Preview),
            fields::OP_CHANGES => Some(Op::Changes),
            fields::OP_DRAFT => Some(Op::Draft),
            fields::OP_SUBMIT => Some(Op::Submit),
            _ => None,
        }
    }
}

/// One page of a form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubmittedPage {
    /// The page's repository path; `None` for a page that does not exist yet (its path comes from `title`).
    pub(crate) path: Option<String>,
    /// The page's title (for a page that exists, only to label it).
    pub(crate) title: String,
    /// The proposal removes the page.
    pub(crate) removed: bool,
    /// The text the person edited, with the line breaks the browser sent.
    pub(crate) text: String,
}

/// What a form says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Submitted {
    pub(crate) op: Op,
    pub(crate) view: Option<String>,
    pub(crate) base: Option<Base>,
    pub(crate) summary: String,
    pub(crate) sources: Vec<String>,
    pub(crate) pages: Vec<SubmittedPage>,
    pub(crate) renames: Vec<Rename>,
    pub(crate) selection: Selection,
}

/// Why a form could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormError {
    /// A field is missing or does not mean anything: the form was not the one a page offers.
    Incomplete,
    /// A page is over the bound.
    PageTooLarge,
    /// The pages together are over the bound.
    TooLarge,
}

fn field<'a>(fields: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    fields.get(name).map(String::as_str)
}

/// The lines of a text, trimmed, without the empty ones.
pub(crate) fn lines_of(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Read the fields of a form. Nothing is parsed as markup here and no size is judged: [`check_bounds`] is next.
pub(crate) fn read(fields: &HashMap<String, String>) -> Result<Submitted, FormError> {
    let op = field(fields, fields::OP)
        .and_then(Op::of)
        .ok_or(FormError::Incomplete)?;
    let count = field(fields, fields::PAGES)
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| (1..=MAX_FORM_PAGES).contains(n))
        .ok_or(FormError::Incomplete)?;
    let mut pages = Vec::with_capacity(count);
    for i in 0..count {
        let get = |what: &str| field(fields, &fields::page_field(i, what));
        let new_page = get(fields::F_NEW) == Some("1");
        let path = get(fields::F_PATH)
            .filter(|p| !p.is_empty())
            .map(str::to_owned);
        if !new_page && path.is_none() {
            return Err(FormError::Incomplete);
        }
        pages.push(SubmittedPage {
            path: if new_page { None } else { path },
            title: get(fields::F_TITLE).unwrap_or("").to_owned(),
            removed: get(fields::F_REMOVED) == Some("1"),
            text: get(fields::F_TEXT).unwrap_or("").to_owned(),
        });
    }
    let renames = field(fields, fields::RENAMES)
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0)
        .min(MAX_FORM_RENAMES);
    let renames = (0..renames)
        .filter_map(|i| {
            let from = field(fields, &fields::rename_field(i, fields::F_FROM))?;
            let to = field(fields, &fields::rename_field(i, fields::F_TO))?;
            Some(Rename {
                from: from.to_owned(),
                to: to.to_owned(),
            })
        })
        .collect();
    let selection = match (
        field(fields, fields::PART).unwrap_or(fields::PART_WHOLE),
        field(fields, fields::ORDINAL).and_then(|n| n.parse::<usize>().ok()),
    ) {
        (fields::PART_WHOLE, _) => Selection::Whole,
        (fields::PART_SECTION, Some(n)) => Selection::Section(n),
        (fields::PART_BLOCK, Some(n)) => Selection::Block(n),
        _ => return Err(FormError::Incomplete),
    };
    let base = match (
        field(fields, fields::BASE_STATE),
        field(fields, fields::BASE_BLOCK),
    ) {
        (Some(state), Some(block)) => Some(Base::new(state, block).ok_or(FormError::Incomplete)?),
        _ => None,
    };
    Ok(Submitted {
        op,
        view: field(fields, fields::VIEW)
            .filter(|v| !v.is_empty())
            .map(str::to_owned),
        base,
        summary: field(fields, fields::SUMMARY)
            .unwrap_or("")
            .trim()
            .to_owned(),
        sources: lines_of(field(fields, fields::SOURCES).unwrap_or("")),
        pages,
        renames,
        selection,
    })
}

/// The sizes, judged **before anything is parsed**: no page over [`MAX_PAGE_BYTES`], and the texts and paths
/// together within [`MAX_PROPOSAL_BYTES`].
pub(crate) fn check_bounds(form: &Submitted) -> Result<(), FormError> {
    if form.pages.iter().any(|p| p.text.len() > MAX_PAGE_BYTES) {
        return Err(FormError::PageTooLarge);
    }
    let total: usize = form
        .pages
        .iter()
        .map(|p| p.text.len() + p.path.as_ref().map_or(0, String::len) + p.title.len())
        .sum::<usize>()
        + form
            .renames
            .iter()
            .map(|r| r.from.len() + r.to.len())
            .sum::<usize>();
    if u64::try_from(total).unwrap_or(u64::MAX) > MAX_PROPOSAL_BYTES {
        return Err(FormError::TooLarge);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------- line breaks

/// How a page's lines end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Eol {
    Lf,
    CrLf,
}

impl Eol {
    /// The endings of a page: CR LF when it has line breaks and every one is CR LF, else LF.
    pub(crate) fn of(text: &str) -> Eol {
        let breaks = text.matches('\n').count();
        if breaks > 0 && text.matches("\r\n").count() == breaks {
            Eol::CrLf
        } else {
            Eol::Lf
        }
    }

    /// `text` with every line break written the way this page writes them.
    pub(crate) fn apply(self, text: &str) -> String {
        let lf = text.replace("\r\n", "\n").replace('\r', "\n");
        match self {
            Eol::Lf => lf,
            Eol::CrLf => lf.replace('\n', "\r\n"),
        }
    }
}

/// A text as a text area shows it: line breaks as LF (the browser decides how to send them back).
pub(crate) fn for_display(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

// ---------------------------------------------------------------------------------------------- owned keys

/// Something that can say which top-level frontmatter keys a page's text has and what they are set to. The
/// real one is the markup crate's parser; a test counts its calls.
pub(crate) trait Frontmatter {
    /// The keys and their values (as JSON text), or `None` when the information at the top of the page is
    /// not in the subset the wiki reads.
    fn keys(&self, text: &str) -> Option<BTreeMap<String, String>>;
}

/// The markup crate's parser.
pub(crate) struct Markup;

impl Frontmatter for Markup {
    fn keys(&self, text: &str) -> Option<BTreeMap<String, String>> {
        let (yaml, _, _) = frontmatter::split(text);
        let Some(yaml) = yaml else {
            return Some(BTreeMap::new());
        };
        match frontmatter::parse_yaml(yaml).ok()? {
            frontmatter::Value::Map(pairs) => Some(
                pairs
                    .into_iter()
                    .map(|(k, v)| (k, v.to_json().to_string()))
                    .collect(),
            ),
            _ => Some(BTreeMap::new()),
        }
    }
}

/// Why the information at the top of a page was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Front {
    /// It is not readable.
    Unreadable,
    /// It adds, changes or removes keys only the service writes (they are named).
    Owned(Vec<&'static str>),
}

/// Check the information at the top of `new` against that of `old` (the page the proposal was written against,
/// `None` for a new page): unreadable is refused, and so is any change to a key the service owns. A key left
/// as it was is not a change. `allowed` says whether the principal's role may write the keys; it is asked only
/// when a key did change.
pub(crate) fn check_owned_keys(
    parser: &dyn Frontmatter,
    old: Option<&str>,
    new: &str,
    allowed: impl FnOnce() -> bool,
) -> Result<(), Front> {
    let after = parser.keys(new).ok_or(Front::Unreadable)?;
    // A page that is already unreadable is not made worse by being read as having no keys.
    let before = old.and_then(|t| parser.keys(t)).unwrap_or_default();
    let changed: Vec<&'static str> = fields::OWNED_KEYS
        .into_iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .collect();
    if changed.is_empty() || allowed() {
        Ok(())
    } else {
        Err(Front::Owned(changed))
    }
}

#[cfg(test)]
mod tests;
