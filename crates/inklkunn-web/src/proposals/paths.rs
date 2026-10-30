//! The addresses of a wiki's proposals (RFC 004 D-2; RFC 003 D-1): every one is under `/<slug>/-/`, so no page
//! title can shadow them. Code, not words; scoped in `tests/literals.allow`.

use super::fields;
use crate::url::encode_title;

/// `/<slug>/-/proposals`: the list.
pub(crate) fn list(slug: &str) -> String {
    format!("/{slug}/-/proposals")
}

/// `/<slug>/-/proposals/new` with a query: the form, or the chooser of what to change.
fn new_with(slug: &str, query: &[(&str, &str)]) -> String {
    let mut out = format!("/{slug}/-/proposals/new");
    for (i, (k, v)) in query.iter().enumerate() {
        out.push(if i == 0 { '?' } else { '&' });
        out.push_str(k);
        out.push('=');
        out.push_str(&inklkunn_markup::percent_encode(v));
    }
    out
}

/// The form to change a page as a whole, in the view named (`None`: the default view).
pub(crate) fn change_page(slug: &str, title: &str, view: Option<&str>) -> String {
    let mut q = vec![(fields::Q_PAGE, title)];
    if let Some(v) = view {
        q.push((fields::VIEW, v));
    }
    new_with(slug, &q)
}

/// The form to change one section (a heading and what follows it) or one part of a page.
pub(crate) fn change_part(
    slug: &str,
    title: &str,
    view: Option<&str>,
    kind: &str,
    ordinal: usize,
) -> String {
    let ordinal = ordinal.to_string();
    let mut q = vec![
        (fields::Q_PAGE, title),
        (fields::PART, kind),
        (fields::ORDINAL, ordinal.as_str()),
    ];
    if let Some(v) = view {
        q.push((fields::VIEW, v));
    }
    new_with(slug, &q)
}

/// The chooser of what to change in a page.
pub(crate) fn choose_part(slug: &str, title: &str, view: Option<&str>) -> String {
    let mut q = vec![(fields::Q_PAGE, title), (fields::Q_PARTS, "1")];
    if let Some(v) = view {
        q.push((fields::VIEW, v));
    }
    new_with(slug, &q)
}

/// The form for a page that does not exist yet; with a title, the title is filled in.
pub(crate) fn new_page(slug: &str, title: Option<&str>) -> String {
    match title {
        Some(t) if !t.trim().is_empty() => {
            new_with(slug, &[(fields::Q_NEW, "1"), (fields::Q_TITLE, t.trim())])
        }
        _ => new_with(slug, &[(fields::Q_NEW, "1")]),
    }
}

/// The form that opens a new proposal.
pub(crate) fn new_action(slug: &str) -> String {
    format!("/{slug}/-/proposals/new")
}

/// `/<slug>/-/proposals/<id>`.
pub(crate) fn proposal(slug: &str, id: &str) -> String {
    format!("/{slug}/-/proposals/{id}")
}

/// The proposal's page with a fragment.
pub(crate) fn proposal_at(slug: &str, id: &str, fragment: &str) -> String {
    format!("/{slug}/-/proposals/{id}#{fragment}")
}

/// The proposal's page with a query and a fragment: which part a comment is about, which comment it answers.
pub(crate) fn proposal_with(
    slug: &str,
    id: &str,
    key: &str,
    value: &str,
    fragment: &str,
) -> String {
    format!("/{slug}/-/proposals/{id}?{key}={value}#{fragment}")
}

/// An act on a proposal: the last part of its address.
pub(crate) fn act(slug: &str, id: &str, act: &str) -> String {
    format!("/{slug}/-/proposals/{id}/{act}")
}
pub(crate) const ACT_EDIT: &str = "edit";
pub(crate) const ACT_COMMENT: &str = "comment";
pub(crate) const ACT_DECIDE: &str = "decide";
pub(crate) const ACT_ACCEPT: &str = "accept";
pub(crate) const ACT_DECLINE: &str = "decline";
pub(crate) const ACT_WITHDRAW: &str = "withdraw";
pub(crate) const ACT_ASK: &str = "ask";

/// The form that continues a draft, with the sign that it was just saved.
pub(crate) fn edit_saved(slug: &str, id: &str) -> String {
    format!("/{slug}/-/proposals/{id}/edit?{}=1", fields::Q_SAVED)
}

/// A page's discussion.
pub(crate) fn discussion(slug: &str, title: &str) -> String {
    format!("/{slug}/-/discussion/{}", encode_title(title))
}
