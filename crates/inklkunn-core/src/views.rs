//! Views (RFC 005 part 2 D-9; RFC 002 D-2's `create-view`; handoff 005 K-T8): a view is a prikk branch, and an
//! **alternative view** is one the community made on purpose, with a name, a kind (*experimental* or
//! *interpretation*) and a record of what it was forked from.
//!
//! - [`new_name`]: what a new view may be called. Stricter than what is read (lower-case letters, digits, `.`,
//!   `-`, `_`), so that two names never differ only by case and no name can be mistaken for another.
//! - [`list`]: the wiki's open views, **the default view first**, each with its recorded origin when the service
//!   made it. A view made some other way (by the command line, by an import) has no recorded origin, and the list
//!   says nothing of one rather than invent it. Bounded by [`MAX_VIEWS`]; what is left out is counted.
//! - The creation itself is [`ProposalAcceptor::create_view`](crate::ProposalAcceptor::create_view), which runs
//!   the same steps as an acceptance (authorize, the write lease, a recorded act before the signer is asked).
//!
//! **No view here is called canonical, adopted or official**, and none is ranked: the list is the default view and
//! then the others in name order, which says nothing about them.

use inklkunn_store::{ActStore, StoreError, ViewActEntry, WikiRecord};

pub use inklkunn_store::ViewKind;

use crate::read::{Cause, ReadError, ReadService};

/// The most views a wiki lists, compares among and holds open at once. A bound against a runaway, generous for
/// any community: a wiki with more has to close some before a reviewer makes another.
pub const MAX_VIEWS: usize = 100;

/// The most characters a new view's name has.
pub const MAX_NAME_CHARS: usize = 64;

/// Why a new view's name is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameFault {
    /// There is none.
    Empty,
    /// It has more than [`MAX_NAME_CHARS`] characters.
    TooLong,
    /// It holds a character other than a lower-case ASCII letter, a digit, `.`, `-` or `_`, or has `..` in it,
    /// or ends with `.`.
    Characters,
    /// It is one of the [`RETIRED_NAMES`], whole: a name that would call a view better, more settled or more
    /// authoritative than another (handoff 005 K12).
    Retired,
    /// It starts with something other than a lower-case ASCII letter or a digit.
    Start,
}

/// The words no view is named, whole and in any case: the words the knowledge layer retires (RFC 005 part 2 D-7:
/// nothing in the wiki says that a claim is true, or that a view is the official, canonical or adopted one). A name
/// that only contains one (`official-draft`) is a name; a name that is one says what no view may say of itself.
pub const RETIRED_NAMES: [&str; 7] = [
    "official",
    "canonical",
    "adopted",
    "true",
    "false",
    "verified",
    "proven",
];

/// A new view's name, if it is one. The name is never trimmed or changed: what was typed is what is checked.
pub fn new_name(raw: &str) -> Result<&str, NameFault> {
    if raw.is_empty() {
        return Err(NameFault::Empty);
    }
    if RETIRED_NAMES.iter().any(|w| w.eq_ignore_ascii_case(raw)) {
        return Err(NameFault::Retired);
    }
    if raw.chars().count() > MAX_NAME_CHARS {
        return Err(NameFault::TooLong);
    }
    if !raw
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err(NameFault::Start);
    }
    let plain = raw
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'));
    if !plain || raw.contains("..") || raw.ends_with('.') {
        return Err(NameFault::Characters);
    }
    Ok(raw)
}

/// The short name of a view: `draft` for `heads/draft`.
pub fn short_name(view: &str) -> &str {
    view.strip_prefix("heads/").unwrap_or(view)
}

/// Where an alternative view came from, as the service recorded it when it made the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewOrigin {
    /// What the view is for.
    pub kind: ViewKind,
    /// The short name of the view it was forked from.
    pub forked_from: String,
    /// The block that view's tip was when the fork was made.
    pub from_block: String,
}

/// One view in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewRow {
    /// The short name.
    pub name: String,
    /// Is it the wiki's default view?
    pub is_default: bool,
    /// Where it came from, when the service made it.
    pub origin: Option<ViewOrigin>,
}

/// The wiki's open views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewList {
    /// At most [`MAX_VIEWS`] views, the default view first, then the others by name.
    pub rows: Vec<ViewRow>,
    /// How many open views the list leaves out.
    pub omitted: usize,
}

impl ViewList {
    /// The row of the view with this short name.
    pub fn view(&self, name: &str) -> Option<&ViewRow> {
        self.rows.iter().find(|r| r.name == name)
    }
}

/// The origin each open view has on record: the act that stands for its name. A view that no act made has none.
fn origin_of(acts: &[ViewActEntry], name: &str) -> Option<ViewOrigin> {
    acts.iter()
        .rev()
        .find(|e| e.stands() && e.act.name == name)
        .map(|e| ViewOrigin {
            kind: e.act.kind,
            forked_from: e.act.forked_from.clone(),
            from_block: e.act.from_block.clone(),
        })
}

/// The wiki's open views with their recorded origins. Asks prikk unless the views are held; a record store that
/// cannot answer is [`ReadError::CouldNotBeReadNow`] (the list is not shown without origins as if it had none).
pub fn list(
    reads: &ReadService,
    acts: &dyn ActStore,
    wiki: &WikiRecord,
) -> Result<ViewList, ReadError> {
    let open = reads.open_views(wiki)?;
    let recorded = acts
        .view_acts(&wiki.id)
        .map_err(|e: StoreError| ReadError::CouldNotBeReadNow(Cause::Store(e)))?;
    let default = short_name(&wiki.default_view);
    let omitted = open.len().saturating_sub(MAX_VIEWS);
    let rows = open
        .iter()
        .take(MAX_VIEWS)
        .map(|full| {
            let name = short_name(full);
            ViewRow {
                name: name.to_owned(),
                is_default: name == default,
                origin: origin_of(&recorded, name),
            }
        })
        .collect();
    Ok(ViewList { rows, omitted })
}

#[cfg(test)]
mod tests;
