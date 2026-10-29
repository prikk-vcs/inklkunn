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
    // wiki shell
    msg("wiki.nav", "This wiki"),
    msg("wiki.nav_about", "About this wiki"),
    msg("wiki.nav_verification", "Verification"),
    msg("wiki.nav_wikis", "All wikis"),
    msg("wiki.view_in_use", "You are reading the view {view}."),
    msg("title.page", "{page} · {wiki} · {product}"),
    // a page
    msg("page.redirected_from", "Redirected from {title}."),
    msg("page.alias_from", "{title} is another title of this page."),
    msg(
        "page.matched_loosely",
        "You asked for {title}. This is the page with the closest title.",
    ),
    msg(
        "page.stale",
        "This page may be out of date. The server could not ask for the newest version just now, so this is the last version it has.",
    ),
    msg("page.contents", "On this page"),
    msg("page.license", "License: {license}"),
    msg("page.propose_disabled", "Propose a change"),
    msg(
        "page.propose_note",
        "Proposals arrive with the next release.",
    ),
    // a page that is not there, or not one
    msg("missing.heading", "This page does not exist yet"),
    msg("missing.body", "No page in {wiki} has the title {title}."),
    msg(
        "missing.invite",
        "You can propose it when proposals arrive with the next release.",
    ),
    msg("ambiguous.heading", "More than one page fits"),
    msg(
        "ambiguous.body",
        "Several pages could be the one called {title}. Choose one.",
    ),
    msg("redirect.heading", "This redirect does not lead to a page"),
    msg(
        "redirect.dangling",
        "The page {title} sends readers to {target}, and no page has that title.",
    ),
    msg(
        "redirect.loop",
        "The page {title} sends readers round in a circle.",
    ),
    msg(
        "redirect.too_long",
        "The page {title} sends readers through too many other pages.",
    ),
    msg("unreadable.heading", "This page cannot be shown"),
    msg(
        "unreadable.not_text",
        "The file of this page is not text that this site can read.",
    ),
    msg(
        "unreadable.info",
        "The information at the top of this page cannot be read.",
    ),
    msg(
        "unreadable.info_long",
        "The information at the top of this page is too long.",
    ),
    msg(
        "unreadable.advice",
        "The people who write this wiki can fix it.",
    ),
    msg("unavailable.heading", "This page could not be read now"),
    msg(
        "unavailable.body",
        "Try again in a little while. If it keeps happening, tell the person who runs this server.",
    ),
    msg("no_view.heading", "There is no such view"),
    msg("no_view.body", "This wiki has no view called {view}."),
    msg("invalid_title.heading", "That title cannot name a page"),
    msg(
        "invalid_title.body",
        "A title cannot be empty, cannot have an empty part between slashes, and cannot be longer than the wiki can store.",
    ),
    // verification
    msg("assurance.verified", "Verified"),
    msg("assurance.verification_failed", "Verification failed"),
    msg(
        "assurance.could_not_be_checked_now",
        "Could not be checked now",
    ),
    msg("assurance.not_checked_yet", "Not checked yet"),
    msg("badge.sentence", "Verification: {state}"),
    msg("title.verification", "Verification · {wiki} · {product}"),
    msg("verification.heading", "Verification"),
    msg(
        "explain.verification",
        "Verification asks prikk, the program that keeps this wiki's history, to check every part of that history and its signatures. “Verified” means the last check found nothing wrong. It does not mean the pages are correct.",
    ),
    msg("verification.explain_summary", "What does this mean?"),
    msg(
        "verification.state_verified",
        "The last check found nothing wrong.",
    ),
    msg(
        "verification.state_failed",
        "The last check found a problem with this wiki's history. The person who runs this server can look into it.",
    ),
    msg(
        "verification.state_could_not",
        "The last check could not be completed. That says nothing about whether the history is sound.",
    ),
    msg(
        "verification.state_unknown",
        "This wiki has not been checked yet.",
    ),
    msg("verification.checked_at", "Last checked: {when}."),
    msg("verification.check_now", "Check now"),
    msg(
        "verification.recent",
        "A check ran a moment ago, so no new one was started. Wait a little before asking again.",
    ),
    msg("verification.found_heading", "What the check found"),
    msg(
        "verification.found_intro",
        "These parts of the check did not come out well:",
    ),
    msg(
        "verification.now_verified",
        "The check you asked for found nothing wrong.",
    ),
    msg(
        "verification.not_saved",
        "The result could not be saved, so the label on the pages has not changed.",
    ),
    msg(
        "verification.busy",
        "The server is busy with other work, so the check did not run. Try again in a little while.",
    ),
    msg(
        "verification.limit",
        "The check needs more time or memory than this server is set up to give it. Tell the person who runs this server.",
    ),
    msg(
        "verification.unavailable",
        "The check could not be started. Tell the person who runs this server.",
    ),
    // about, and the list of wikis
    msg("title.about", "About {wiki} · {product}"),
    msg("about.heading", "About {wiki}"),
    msg(
        "about.license",
        "Unless a page says otherwise, the pages of this wiki are shared under the license {license}.",
    ),
    msg("about.view", "The view shown when none is named is {view}."),
    msg("about.accessibility_heading", "Accessibility statement"),
    msg(
        "about.accessibility_tools",
        "Automated tests check every kind of page on this site for one main heading, landmarks, a link that skips to the content, labels, text alternatives, language and direction, contrast in the light and dark schemes, visible keyboard focus and large enough targets.",
    ),
    msg(
        "about.accessibility_people",
        "No person who uses assistive technology has tested this site yet. If something does not work for you, tell the person who runs this server.",
    ),
    msg("title.wikis", "Wikis · {product}"),
    msg("wikis.heading", "Wikis"),
    msg("wikis.intro", "These are the wikis you can read here."),
    msg("wikis.none", "There are no wikis you can read here yet."),
    // transclusion (a page that includes another)
    msg("transclusion.from", "From {title}"),
    msg(
        "transclusion.cycle",
        "Not included, it would include itself: {chain}",
    ),
    msg(
        "transclusion.too_deep",
        "Not included, deeper than {limit}: {title}",
    ),
    msg(
        "transclusion.over_budget",
        "Not included, a page includes only so much: {title}",
    ),
    // a request this address does not take
    msg("title.method_not_allowed", "Not possible here · {product}"),
    msg(
        "method_not_allowed.heading",
        "That is not possible at this address",
    ),
    msg(
        "method_not_allowed.body",
        "This address can be opened, but not used in that way. Go back and use the links on the page.",
    ),
    // the read API's problem details (handoff 003 T8)
    msg("problem.not_found.title", "Not found"),
    msg(
        "problem.not_found.detail",
        "There is nothing to show at this address.",
    ),
    msg(
        "problem.not_found.hint",
        "Check the address for typing mistakes.",
    ),
    msg("problem.page_not_found.title", "Page not found"),
    msg(
        "problem.page_not_found.detail",
        "The wiki has no page with this title.",
    ),
    msg(
        "problem.page_not_found.hint",
        "Check the title, or list the pages of the wiki to find it.",
    ),
    msg("problem.ambiguous_title.title", "Several pages match"),
    msg(
        "problem.ambiguous_title.detail",
        "More than one page matches this title, and none is nearer.",
    ),
    msg(
        "problem.ambiguous_title.hint",
        "Ask for one of the pages listed under candidates, by its exact title.",
    ),
    msg(
        "problem.redirect_broken.title",
        "The redirect cannot be followed",
    ),
    msg(
        "problem.redirect_broken.detail",
        "This title is a redirect that does not lead to a page.",
    ),
    msg(
        "problem.redirect_broken.hint",
        "Ask the people who look after the wiki to fix the redirect, or ask for the page it should lead to.",
    ),
    msg("problem.page_unreadable.title", "The page cannot be shown"),
    msg(
        "problem.page_unreadable.detail",
        "The page exists, but its stored text cannot be read as a page.",
    ),
    msg(
        "problem.page_unreadable.hint",
        "Ask the people who write the page to correct it.",
    ),
    msg("problem.invalid_title.title", "Title not accepted"),
    msg(
        "problem.invalid_title.detail",
        "This title cannot name a page.",
    ),
    msg(
        "problem.invalid_title.hint",
        "Use a title that is not empty, not too long, and has no empty part between its slashes.",
    ),
    msg("problem.no_such_view.title", "View not found"),
    msg(
        "problem.no_such_view.detail",
        "The wiki has no open view named {view}.",
    ),
    msg(
        "problem.no_such_view.hint",
        "Leave the view out to use the default view of the wiki.",
    ),
    msg("problem.invalid_parameter.title", "Parameter not accepted"),
    msg(
        "problem.invalid_parameter.detail",
        "The parameter {parameter} is not accepted as it was given.",
    ),
    msg(
        "problem.invalid_parameter.hint",
        "Check the API description for what {parameter} may be.",
    ),
    msg("problem.diff_path_required.title", "A diff needs a path"),
    msg(
        "problem.diff_path_required.detail",
        "A diff is always for one page, so the parameter path is required.",
    ),
    msg(
        "problem.diff_path_required.hint",
        "Give the storage path of the page, as the raw endpoint reports it.",
    ),
    msg("problem.invalid_cursor.title", "Cursor not accepted"),
    msg(
        "problem.invalid_cursor.detail",
        "The cursor is not one this server gave out.",
    ),
    msg(
        "problem.invalid_cursor.hint",
        "Start again from the first page, without a cursor.",
    ),
    msg("problem.not_available.title", "Not available in this build"),
    msg(
        "problem.not_available.detail",
        "This build of the server does not offer this yet.",
    ),
    msg(
        "problem.not_available.hint",
        "Check the API description of this server for what it offers.",
    ),
    msg("problem.unavailable.title", "Cannot be read now"),
    msg(
        "problem.unavailable.detail",
        "The server could not read the wiki just now.",
    ),
    msg("problem.unavailable.hint", "Try again in a little while."),
    msg("problem.failed.title", "The request failed"),
    msg(
        "problem.failed.detail",
        "Something went wrong on the server, so the request was not served.",
    ),
    msg(
        "problem.failed.hint",
        "Try again in a little while, and tell the person who runs this server if it keeps happening.",
    ),
    msg("problem.method_not_allowed.title", "Method not allowed"),
    msg(
        "problem.method_not_allowed.detail",
        "This release of the API only reads, so this address does not take that method.",
    ),
    msg(
        "problem.method_not_allowed.hint",
        "Ask with GET or HEAD instead.",
    ),
];
