//! Views: listing them, creating one, comparing two (RFC 005 part 2 D-9; handoff 005 K-T8).
//!
//! A view is a named version of the whole wiki. Readers see **the default view** (the glossary's word, shipped
//! with the about page) unless they choose another; **no view is called canonical, adopted or official**, and
//! none is ranked: the list is the default view and then the others by name, which says nothing about them.
//!
//! - [`list`]: `GET /{slug}/-/views`, the wiki's open views with the kind and origin the service recorded.
//! - [`new`]: `GET|POST /{slug}/-/views/new`, the form that creates an alternative view (`create-view`, reviewers
//!   and above by default; no script; signed in; the session's token like every write), and the page that says how
//!   a creation ended, for each way it can.
//! - [`compare`]: `GET /{slug}/-/views/compare` (claims and pages that differ, claim by claim and page by page) and
//!   `…/compare/{title}` (one page's own diff between the two views, as RFC 003's diff is).
//! - `paths`: their addresses. `fields`: the names of the form's fields.
//!
//! **Authorized as pages are, and existence-neutral**: every handler begins as a handler of a wiki does (a wiki
//! that is absent and one the visitor may not read are one page), and the creation asks `create-view` of the
//! authorizer before it shows a form or acts. The web layer never makes a principal and never touches a key: the
//! act goes through the acceptor, which records it and asks the signer (INV-4).

pub(crate) mod compare;
mod fields;
pub(crate) mod list;
pub mod new;
pub(crate) mod paths;

use inklkunn_core::views::short_name;
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, T};

pub use new::{ViewOutcome, render_view_outcome};

/// A view named in words inside a sentence: the default view is *the default view* and has no other name; the
/// others are named by their names.
pub(crate) fn named(t: T, wiki: &WikiRecord, view: &str) -> String {
    if view == short_name(&wiki.default_view) {
        t.get(Key::ViewsDefaultName)
    } else {
        t.msg(Key::ViewsNamed).with("view", view).text()
    }
}

/// The view as an address spells it: `None` for the default view, else its name.
pub(crate) fn address_of<'a>(wiki: &WikiRecord, view: &'a str) -> Option<&'a str> {
    (view != short_name(&wiki.default_view)).then_some(view)
}
