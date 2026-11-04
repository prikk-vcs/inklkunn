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

/// `-/why/<Title>`, with the view: what follows the wiki's slug in the address of the page that shows where a
/// page's paragraphs came from.
pub(crate) fn why_tail(title: &str, view: Option<&str>) -> String {
    format!("-/why/{}{}", encode_title(title), query(view))
}

/// `/<slug>/-/why/<Title>`: the page that shows where a page's paragraphs came from.
pub(crate) fn why(slug: &str, view: Option<&str>, title: &str) -> String {
    format!("/{slug}/{}", why_tail(title, view))
}

/// `#<id>`: the fragment of a paragraph on that page. A paragraph with an `^id` is named by it, as it is on the
/// page itself; any other by its place among the page's blocks. The two cannot be confused: the first has a caret.
pub(crate) fn why_paragraph_id(ordinal: usize, block: Option<&str>) -> String {
    match block {
        Some(id) => format!("w-^{id}"),
        None => format!("w-{ordinal}"),
    }
}

/// `#c-<number>`: the fragment of a change on that page, which lists each change once.
pub(crate) fn why_change_id(number: u64) -> String {
    format!("c-{number}")
}
