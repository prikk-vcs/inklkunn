//! The addresses of the views' pages, all under the reserved prefix `-/` so that no page title can shadow them.
//! Code, not words. A view's name is a plain name (`url::view_ref`), so it needs no escaping in a query.

use crate::url::encode_title;

/// `/<slug>/-/views`: the wiki's views.
pub(crate) fn list(slug: &str) -> String {
    format!("/{slug}/-/views")
}

/// `/<slug>/-/views#view-<name>`: the wiki's views, at one of them.
pub(crate) fn list_at(slug: &str, view: &str) -> String {
    format!("/{slug}/-/views#view-{view}")
}

/// `/<slug>/-/views/new`: the form that creates a view.
pub(crate) fn new(slug: &str) -> String {
    format!("/{slug}/-/views/new")
}

/// `/<slug>/-/views/compare`, with the two views when they are given.
pub(crate) fn compare(slug: &str, a: Option<&str>, b: Option<&str>) -> String {
    let mut query = Vec::new();
    if let Some(a) = a {
        query.push(format!("a={a}"));
    }
    if let Some(b) = b {
        query.push(format!("b={b}"));
    }
    if query.is_empty() {
        format!("/{slug}/-/views/compare")
    } else {
        format!("/{slug}/-/views/compare?{}", query.join("&"))
    }
}

/// `/<slug>/-/views/compare/<Title>?a=…&b=…`: how one page differs between two views.
pub(crate) fn compare_page(slug: &str, a: &str, b: &str, title: &str) -> String {
    format!(
        "/{slug}/-/views/compare/{}?a={a}&b={b}",
        encode_title(title)
    )
}

/// `/<slug>/Home` in the default view, `/<slug>/~<view>/Home` in another: where a view is read from.
pub(crate) fn read(slug: &str, view: Option<&str>) -> String {
    match view {
        None => format!("/{slug}"),
        Some(view) => format!("/{slug}/~{view}/Home"),
    }
}
