//! The addresses of the knowledge layer's pages: both under the reserved prefix `-/`, so no page title can
//! shadow them. Code, not words; scoped in `tests/literals.allow`. A view other than the default is named by
//! `?view=`, as the pages about a page's past do, so a link in a view stays in it.

use inklkunn_core::read::HEADING_ID_PREFIX;

use crate::url::encode_title;

/// `?view=<name>` for a view other than the default, else nothing. A view's name is a plain name
/// (`url::view_ref`), so it needs no escaping.
fn query(view: Option<&str>) -> String {
    view.map_or_else(String::new, |v| format!("?view={v}"))
}

/// `-/claim/<id>`, with the view: what follows the wiki's slug in a claim's address.
pub(crate) fn claim_tail(id: &str, view: Option<&str>) -> String {
    format!("-/claim/{id}{}", query(view))
}

/// `/<slug>/-/claim/<id>`: a claim's page.
pub(crate) fn claim(slug: &str, id: &str, view: Option<&str>) -> String {
    format!("/{slug}/{}", claim_tail(id, view))
}

/// A paragraph of a page: the page's address and the fragment the renderer's id for the block makes
/// (the page's id prefix, then `^`, then the block's id).
pub(crate) fn paragraph(page_href: &str, block: &str) -> String {
    format!("{page_href}#{HEADING_ID_PREFIX}^{block}")
}

/// `/<slug>/-/why/<Title>`: the page that shows where a page's paragraphs came from.
pub(crate) fn why(slug: &str, view: Option<&str>, title: &str) -> String {
    format!("/{slug}/-/why/{}{}", encode_title(title), query(view))
}
