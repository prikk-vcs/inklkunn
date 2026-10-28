//! The English messages: the language the catalog is written in. The order is [`super::Key`]'s, and the
//! Japanese table follows it, so a reviewer's sheet maps back to the keys one to one. The words are the
//! glossary's (`docs/glossary.md`, section 2); a test holds the two equal.

use super::{Entry, msg};

/// Every message, in English.
pub const CATALOG: &[Entry] = &[
    // layout
    msg("layout.skip", "Skip to the main content"),
    msg("layout.language", "Language"),
    msg("layout.footer", "{product} version {version}"),
    // the language switcher and its page
    msg("title.language", "Language · {product}"),
    msg("language.heading", "Language"),
    msg(
        "language.intro",
        "Choose the language in which pages are shown. Your choice is kept in a cookie on this device.",
    ),
    msg("language.in_use", "In use now."),
    msg("language.preview_choice", "{name} (preview)"),
    msg(
        "language.preview_note",
        "This translation is a preview. Fluent readers have not checked it yet, so some wording may be wrong.",
    ),
    msg("language.back", "Go back to the page you were on"),
    // not found
    msg("title.not_found", "Not found · {product}"),
    msg("not_found.heading", "Not found"),
    msg(
        "not_found.body",
        "There is nothing to show at this address. Check the address for typing mistakes.",
    ),
    // a switch that was refused
    msg("title.language_refused", "Language not changed · {product}"),
    msg("language_refused.heading", "The language was not changed"),
    msg(
        "language_refused.body",
        "The form did not name a language that is available, so nothing was changed. Choose a language on the language page.",
    ),
    msg("language_refused.link", "Go to the language page"),
    msg("title.form_refused", "Form not accepted · {product}"),
    msg("form_refused.heading", "The form was not accepted"),
    msg(
        "form_refused.body",
        "The form could not be confirmed as coming from a page of this site, so nothing was changed. Go back, reload the page and try again.",
    ),
    // answers that are not pages
    msg(
        "page.render_failed",
        "This page could not be shown. Try again in a little while. If it keeps happening, tell the person who runs this server.",
    ),
    msg(
        "misdirected.body",
        "This server does not serve pages at that address. Check the address you typed, or tell the person who runs this server.",
    ),
];
