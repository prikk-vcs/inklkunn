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

/// The id of a source's line on a claim's page (a source has no page of its own; its place is on the claims that cite
/// it).
pub(crate) fn source_fragment(source: &str) -> String {
    format!("source-{source}")
}

/// `/<slug>/-/claim/<id>#source-<source>`: a source's line on a claim's page.
pub(crate) fn claim_source(slug: &str, claim: &str, source: &str, view: Option<&str>) -> String {
    format!(
        "{}#{}",
        claim_page(slug, claim, view, None, None),
        source_fragment(source)
    )
}

/// The query parameter that names the comment a page of a claim's thread begins after.
pub(crate) const Q_AFTER: &str = "after";

/// `/<slug>/-/claim/<id>` with the view, the comment a page of the thread begins after and the comment a reply is
/// to, whichever are given: a claim's page at a place in its thread.
pub(crate) fn claim_page(
    slug: &str,
    id: &str,
    view: Option<&str>,
    after: Option<&str>,
    reply: Option<&str>,
) -> String {
    let pairs: Vec<String> = [
        view.map(|v| format!("view={v}")),
        after.map(|a| format!("{Q_AFTER}={a}")),
        reply.map(|r| format!("{}={r}", crate::proposals::fields::Q_REPLY)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if pairs.is_empty() {
        format!("/{slug}/-/claim/{id}")
    } else {
        format!("/{slug}/-/claim/{id}?{}", pairs.join("&"))
    }
}

/// The id of the field that adds a comment to a claim's thread, which a reply link leads to.
pub(crate) const COMMENT_FIELD_ID: &str = "claim-comment";

/// A claim's page at a place in its thread, at the form, for a reply to the comment `to`.
pub(crate) fn reply(
    slug: &str,
    id: &str,
    view: Option<&str>,
    after: Option<&str>,
    to: &str,
) -> String {
    format!(
        "{}#{COMMENT_FIELD_ID}",
        claim_page(slug, id, view, after, Some(to))
    )
}

/// `/<slug>/-/claim/<id>/discuss`: the form that adds a comment to the claim's thread posts here.
pub(crate) fn discuss(slug: &str, id: &str) -> String {
    format!("/{slug}/-/claim/{id}/discuss")
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
