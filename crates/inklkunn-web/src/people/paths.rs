//! The addresses of the people pages, all under the reserved prefix `-/` so that no page title can shadow them. Code, not
//! words.

use super::fields;

/// `/<slug>/-/people`: the wiki's people.
pub(crate) fn page(slug: &str) -> String {
    format!("/{slug}/-/people")
}

/// `/<slug>/-/people?done=<what>`: the people, with a note of what was just done (one of a fixed set of words).
pub(crate) fn done(slug: &str, what: &str) -> String {
    format!("/{slug}/-/people?{}={what}", fields::DONE)
}

fn form(slug: &str, act: &str, account: Option<&str>) -> String {
    match account {
        Some(account) => format!("/{slug}/-/people/{act}?{}={account}", fields::ACCOUNT),
        None => format!("/{slug}/-/people/{act}"),
    }
}

/// `/<slug>/-/people/grant`: the form that changes a role, opened on a person when one is named.
pub(crate) fn grant(slug: &str, account: Option<&str>) -> String {
    form(slug, "grant", account)
}

/// `/<slug>/-/people/restrict`: the form that restricts a person.
pub(crate) fn restrict(slug: &str, account: Option<&str>) -> String {
    form(slug, "restrict", account)
}

/// `/<slug>/-/people/renew`: the form that renews a restriction.
pub(crate) fn renew(slug: &str, account: Option<&str>) -> String {
    form(slug, "renew", account)
}

/// `/<slug>/-/people/lift`: the form that lifts a restriction.
pub(crate) fn lift(slug: &str, account: Option<&str>) -> String {
    form(slug, "lift", account)
}
