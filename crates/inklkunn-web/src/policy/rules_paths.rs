//! The addresses of the policy page and its form, under the reserved prefix `-/` so that no page title can shadow them. Code,
//! not words.

/// `/<slug>/-/policy`: the rules of the wiki.
pub(crate) fn page(slug: &str) -> String {
    format!("/{slug}/-/policy")
}

/// `/<slug>/-/policy/edit`: the form that proposes a change to them.
pub(crate) fn edit(slug: &str) -> String {
    format!("/{slug}/-/policy/edit")
}
