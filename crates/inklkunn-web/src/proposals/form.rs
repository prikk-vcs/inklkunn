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

use std::collections::{BTreeMap, BTreeSet, HashMap};

use inklkunn_core::knowledge::{Bearing, Link, ReferenceForm, SourceKind, StatementKind};
use inklkunn_core::proposal::files::{changed_owned_keys, front_keys};
use inklkunn_core::proposal::{MAX_PAGE_BYTES, MAX_PROPOSAL_BYTES};
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

/// The source a form attaches to a paragraph (RFC 005 part 2 D-3), as it was written: the texts are checked by the
/// core when the files are made ([`inklkunn_core::knowledge::compose`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachInput {
    pub(crate) relation: Link,
    pub(crate) kind: SourceKind,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) date: String,
    pub(crate) reference_form: ReferenceForm,
    pub(crate) reference: String,
    pub(crate) bearing: Bearing,
    pub(crate) note: String,
    /// What kind of statement the paragraph is, asked only when it is not a claim yet.
    pub(crate) claim_kind: StatementKind,
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
    /// A source is attached to the paragraph the form edits.
    pub(crate) attach: Option<AttachInput>,
    /// The `^id`s of the paragraphs that others rely on whose removal the person confirmed.
    pub(crate) drops: BTreeSet<String>,
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
    let attach = match field(fields, fields::ATTACH) {
        None => None,
        Some(word) => Some(read_attach(fields, word)?),
    };
    let drops = fields
        .iter()
        .filter(|(_, v)| v.as_str() == "1")
        .filter_map(|(k, _)| k.strip_prefix(fields::DROP))
        .filter(|id| {
            !id.is_empty()
                && id.len() <= MAX_BLOCK_ID
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map(str::to_owned)
        .collect();
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
        attach,
        drops,
    })
}

/// The longest `^id` a field name may spell (the markup profile's own bound).
const MAX_BLOCK_ID: usize = 128;

/// The source fields of a form, in the core's words; a field that is missing or says a word the core does not have
/// is a form that was not the one a page offers.
fn read_attach(fields: &HashMap<String, String>, relation: &str) -> Result<AttachInput, FormError> {
    let word = |name: &str| field(fields, name).ok_or(FormError::Incomplete);
    let text = |name: &str| field(fields, name).unwrap_or("").trim().to_owned();
    Ok(AttachInput {
        relation: Link::from_word(relation).ok_or(FormError::Incomplete)?,
        kind: SourceKind::from_word(word(fields::A_KIND)?).ok_or(FormError::Incomplete)?,
        title: text(fields::A_TITLE),
        author: text(fields::A_AUTHOR),
        date: text(fields::A_DATE),
        reference_form: ReferenceForm::from_word(word(fields::A_REF_FORM)?)
            .ok_or(FormError::Incomplete)?,
        reference: text(fields::A_REF),
        bearing: Bearing::from_word(word(fields::A_BEARING)?).ok_or(FormError::Incomplete)?,
        note: text(fields::A_NOTE),
        claim_kind: StatementKind::from_word(
            field(fields, fields::A_CLAIM_KIND).unwrap_or(StatementKind::Unknown.word()),
        )
        .ok_or(FormError::Incomplete)?,
    })
}

/// The sizes, judged **before anything is parsed**: no page over [`MAX_PAGE_BYTES`], and the texts and paths
/// together within [`MAX_PROPOSAL_BYTES`].
pub(crate) fn check_bounds(form: &Submitted) -> Result<(), FormError> {
    if form.pages.iter().any(|p| p.text.len() > MAX_PAGE_BYTES) {
        return Err(FormError::PageTooLarge);
    }
    let attached = form.attach.as_ref().map_or(0, |a| {
        a.title.len() + a.author.len() + a.date.len() + a.reference.len() + a.note.len()
    });
    let total: usize = attached
        + form
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
        front_keys(text)
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
    // The rule is the service's (`changed_owned_keys`, with the ruling that a redirect is ordinary work); this is
    // the first line, which says it before the service is asked.
    let changed = changed_owned_keys(&before, &after);
    if changed.is_empty() || allowed() {
        Ok(())
    } else {
        Err(Front::Owned(changed))
    }
}

#[cfg(test)]
mod tests;
