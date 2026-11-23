//! The addresses of the protection form, under the reserved prefix `-/` so that no page title can shadow them. Code, not words.

use super::fields;

/// `/<slug>/-/protect`: the form that puts a protection in place, renews it or lifts it.
pub(crate) fn form(slug: &str) -> String {
    format!("/{slug}/-/protect")
}

/// The form for one page (`page`) or for the pages below a title (`namespace`), the title prefilled.
pub(crate) fn form_for(slug: &str, namespace: bool, title: &str) -> String {
    let key = if namespace {
        fields::Q_NAMESPACE
    } else {
        fields::Q_PAGE
    };
    format!(
        "/{slug}/-/protect?{key}={}",
        inklkunn_markup::percent_encode(title)
    )
}

/// `/<slug>`: the wiki's front page.
pub(crate) fn front(slug: &str) -> String {
    format!("/{slug}")
}

/// `/<slug>/<Title>`: a page, in the default view.
pub(crate) fn page(slug: &str, title: &str) -> String {
    format!("/{slug}/{}", crate::url::encode_title(title))
}
