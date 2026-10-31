//! The English messages: the language the catalog is written in. The order is [`super::Key`]'s, and the
//! Japanese table follows it, so a reviewer's sheet maps back to the keys one to one. The words are the
//! glossary's (`docs/glossary.md`, section 2); a test holds the two equal.

use super::{Entry, msg, plural_msg};

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
    msg(
        "verification.attempt_at",
        "A later check could not be completed ({when}). That says nothing about whether the history is sound, and the result above still stands.",
    ),
    msg(
        "verification.running",
        "A check of this wiki is already running. Wait for it to finish, then look at this page again.",
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
    // history, change, diff and permalinks (handoff 003 T5)
    msg("page.link_history", "History of this page"),
    msg("title.history", "History of {page} · {wiki} · {product}"),
    msg("history.heading", "History of {title}"),
    msg(
        "history.intro",
        "These changes touched this page. The newest is first.",
    ),
    msg("history.current", "The current version of {title}"),
    msg("history.none", "No change to this page was found."),
    msg(
        "history.none_so_far",
        "No change to this page was found among those read so far. Older changes may have touched it.",
    ),
    msg("history.older", "Look at older changes"),
    msg(
        "history.beyond",
        "This wiki has more changes than are read at once. The oldest are not listed here.",
    ),
    msg("history.change_label", "Change {number}"),
    msg("history.link_diff", "What change {number} did to this page"),
    msg("history.link_version", "The page after change {number}"),
    msg("people.proposed_by", "Proposed by {name}"),
    msg("people.accepted_by", "Accepted by {name}"),
    msg("people.made_by", "Made by {name}"),
    msg("people.service", "the wiki service"),
    msg("people.anonymous", "an anonymous contributor ({token})"),
    msg(
        "people.unrecorded",
        "The record of this change does not say who made it.",
    ),
    msg("flag.undo", "Undoes earlier changes"),
    msg("flag.import", "Import"),
    msg("flag.service", "Made by the wiki service itself"),
    msg("flag.first", "The first change of this wiki"),
    msg("event.created", "The page was created"),
    msg("event.edited", "The page was edited"),
    msg("event.deleted", "The page was removed"),
    msg("event.moved_from", "The page was moved here from {title}"),
    msg("event.moved_to", "The page was moved to {title}"),
    msg(
        "event.not_inspected",
        "This change is too large to list what it did, so it may have touched this page.",
    ),
    msg("title.change", "Change {number} · {wiki} · {product}"),
    msg("change.heading", "Change {number}"),
    msg("change.signed", "Signed into the history"),
    msg(
        "change.signed_by",
        "Signed by the wiki service on behalf of {proposer}, accepted by {acceptor}.",
    ),
    msg("change.signed_service", "Signed by the wiki service."),
    msg("change.did_heading", "What this change did"),
    msg("change.did_none", "This change has nothing to list."),
    msg("change.did_added", "Added the page {title}"),
    msg("change.did_edited", "Edited the page {title}"),
    msg("change.did_removed", "Removed the page {title}"),
    msg("change.did_moved", "Moved the page {from} to {to}"),
    msg(
        "change.did_file_added",
        "Added an attachment or another file that is not a page",
    ),
    msg(
        "change.did_file_edited",
        "Edited an attachment or another file that is not a page",
    ),
    msg(
        "change.did_file_removed",
        "Removed an attachment or another file that is not a page",
    ),
    msg(
        "change.did_file_moved",
        "Moved an attachment or another file that is not a page",
    ),
    plural_msg(
        "change.did_more",
        "{count} more item is not listed.",
        "{count} more items are not listed.",
    ),
    msg(
        "change.too_large",
        "This change is too large to list what it did.",
    ),
    msg("change.link_diff", "What this change did to {title}"),
    msg("change_none.heading", "There is no such change"),
    msg(
        "change_none.body",
        "This wiki has no change at this address. Check the address, or open the history of a page and follow a link from there.",
    ),
    msg("title.diff", "Changes to {page} · {wiki} · {product}"),
    msg("diff.heading", "Changes to {title}"),
    msg("diff.from_to", "From {from} to {to}."),
    msg("diff.summary_heading", "Summary"),
    plural_msg(
        "diff.summary_added",
        "{count} line added",
        "{count} lines added",
    ),
    plural_msg(
        "diff.summary_removed",
        "{count} line removed",
        "{count} lines removed",
    ),
    msg("diff.summary_none", "No line differs."),
    msg(
        "diff.status_added",
        "The page did not exist at the first point and exists at the second.",
    ),
    msg(
        "diff.status_removed",
        "The page existed at the first point and does not exist at the second.",
    ),
    msg(
        "diff.status_unchanged",
        "The page is the same at both points.",
    ),
    msg(
        "diff.no_lines",
        "The lines of this page cannot be shown. It may not be text, or it could not be compared.",
    ),
    msg("diff.truncated", "Only the first {limit} lines are shown."),
    msg(
        "diff.refinement",
        "Marked words are found by inklkunn, which compares the changed lines word by word. The lines themselves come from prikk.",
    ),
    msg("diff.hunk_near", "Changes near line {line}"),
    msg("diff.hunk_plain", "Changes"),
    msg(
        "diff.hunk_raw",
        "These lines could not be read as a list of changes. They are shown as they are.",
    ),
    msg("diff.added_label", "Added:"),
    msg("diff.removed_label", "Removed:"),
    msg(
        "diff.no_newline",
        "The text ends here without a line break.",
    ),
    msg(
        "title.at",
        "{page} after change {number} · {wiki} · {product}",
    ),
    msg("at.banner", "This is the page as it was after {change}."),
    msg("at.current", "Go to {current}."),
    msg("at.current_label", "the current version of this page"),
    msg(
        "at.links",
        "The links in this page lead to the pages as they are now, not as they were then.",
    ),
    msg("at.missing_heading", "The page did not exist then"),
    msg(
        "at.missing_body",
        "After {change}, this wiki had no page titled {title}.",
    ),
    // handoff 003 T6: changes, feeds, search, health, backlinks, the head of a page
    msg("wiki.nav_changes", "Recent changes"),
    msg("wiki.nav_search", "Search"),
    msg("wiki.nav_health", "Wiki health"),
    msg("title.changes", "Recent changes · {wiki} · {product}"),
    msg("changes.heading", "Recent changes"),
    msg(
        "changes.intro",
        "The changes recorded in the history of {wiki}, newest first.",
    ),
    msg(
        "changes.seen_note",
        "The history does not record when a change was made, so each change shows when this server first saw it.",
    ),
    msg("changes.filter_legend", "Choose which changes to show"),
    msg("changes.filter_namespace", "Only pages under this title"),
    msg(
        "changes.filter_namespace_hint",
        "For example Guides shows changes to Guides/Setup. Leave it empty for all pages.",
    ),
    msg("changes.filter_view", "View"),
    msg("changes.all_views", "All views"),
    msg(
        "changes.filter_automated",
        "Changes made by the wiki service itself or with the help of a machine",
    ),
    msg("changes.opt_show", "Show them"),
    msg("changes.opt_hide", "Hide them"),
    msg("changes.opt_only", "Show only them"),
    msg("changes.filter_apply", "Show these changes"),
    msg("changes.active_heading", "The list is filtered"),
    msg("changes.active_namespace", "Only pages under {namespace}"),
    msg("changes.active_page", "Only changes to the page {page}"),
    msg("changes.active_view", "Only the view {view}"),
    msg(
        "changes.active_hide",
        "Changes made by the wiki service itself or with the help of a machine are hidden",
    ),
    msg(
        "changes.active_only",
        "Only changes made by the wiki service itself or with the help of a machine",
    ),
    msg("changes.clear", "Show all changes"),
    msg("changes.caption", "Changes, newest first"),
    msg("changes.col_change", "Change"),
    msg("changes.col_people", "People"),
    msg("changes.col_pages", "Pages"),
    msg("changes.col_seen", "First seen here"),
    msg("changes.proposed_by", "Proposed by {person}"),
    msg("changes.accepted_by", "Accepted by {person}"),
    msg("changes.made_by", "Made by {person}"),
    msg("changes.person_anonymous", "an anonymous visitor ({token})"),
    msg("changes.person_service", "{label}, the wiki service"),
    msg(
        "changes.foreign",
        "Recorded outside this service, so the people are not known.",
    ),
    msg("changes.rollback", "Undoes earlier changes"),
    msg("changes.no_pages", "No pages"),
    msg(
        "changes.pages_unavailable",
        "The pages of this change could not be read just now.",
    ),
    msg("changes.page_gone", "{title} (no longer there)"),
    msg("changes.none", "There are no changes to show."),
    msg(
        "changes.more",
        "Older changes exist than these. Only the newest changes were looked through.",
    ),
    msg(
        "changes.pages_missing",
        "Some changes could not be matched to their pages just now, so this list may miss changes.",
    ),
    msg(
        "changes.hidden",
        "Changes to pages you may not read are not shown.",
    ),
    msg(
        "changes.feed",
        "Follow these changes in a feed reader (Atom)",
    ),
    msg("feed.title_wiki", "Recent changes · {wiki}"),
    msg(
        "feed.title_namespace",
        "Recent changes under {namespace} · {wiki}",
    ),
    msg("feed.title_page", "Changes to {page} · {wiki}"),
    msg("feed.pages", "Pages: {pages}"),
    msg("title.search", "Search · {wiki} · {product}"),
    msg("search.heading", "Search"),
    msg("search.label", "Search the titles and aliases of pages"),
    msg("search.button", "Search"),
    msg(
        "search.scope",
        "Search finds pages by their titles and aliases. Searching the text of pages arrives in a later release.",
    ),
    msg(
        "search.empty",
        "Type a title, or part of one, to search for.",
    ),
    msg("search.results_heading", "Pages found for {query}"),
    msg("search.matched_title", "Title:"),
    msg("search.matched_alias", "Alias:"),
    msg(
        "search.none",
        "No page has {query} in its title or aliases.",
    ),
    msg(
        "search.cut",
        "Only the best matches are listed. Type more of the title to narrow the search.",
    ),
    msg("search.exact", "A page with this title exists already."),
    msg("search.similar_heading", "Pages with similar titles"),
    msg("search.create_heading", "Create this page"),
    msg("search.create", "Create “{title}”"),
    msg(
        "opensearch.description",
        "Search the titles and aliases of {wiki}",
    ),
    msg("title.health", "Wiki health · {wiki} · {product}"),
    msg("health.heading", "Health of {wiki}"),
    msg(
        "health.intro",
        "These lists show where the wiki can be improved. Each row says what would resolve it.",
    ),
    msg("health.wanted_name", "Wanted pages"),
    msg(
        "health.wanted_desc",
        "A wanted page is a title that pages link to and that no page has yet.",
    ),
    msg("health.orphans_name", "Orphan pages"),
    msg(
        "health.orphans_desc",
        "An orphan page is a page that no other page links to.",
    ),
    msg("health.dead_ends_name", "Dead-end pages"),
    msg(
        "health.dead_ends_desc",
        "A dead-end page is a page that links to no other page.",
    ),
    msg("health.redirects_name", "Redirect problems"),
    msg(
        "health.redirects_desc",
        "Redirects that lead nowhere, go round in a circle, run too long, or lead to another redirect.",
    ),
    msg("health.help_name", "Pages that want help"),
    msg(
        "health.help_desc",
        "Pages marked as work in progress, draft or stub.",
    ),
    msg("health.stale_name", "Pages unchanged longest"),
    msg(
        "health.stale_desc",
        "Pages whose latest change is the oldest among the recent changes.",
    ),
    msg("health.back", "All health lists"),
    msg("health.col_page", "Page"),
    msg("health.col_wanted", "Wanted title"),
    msg("health.col_linked", "Pages linking to it"),
    msg("health.col_action", "What would resolve it"),
    msg("health.col_target", "Points to"),
    msg("health.col_problem", "Problem"),
    msg("health.col_mark", "Marked as"),
    msg("health.col_changed", "Last change"),
    msg("health.see_links", "Pages that link to {title}"),
    msg(
        "health.wanted_action",
        "Add a page with this title, or change the links that point to it.",
    ),
    msg(
        "health.orphans_action",
        "Link to this page from a related page, so that readers can find it.",
    ),
    msg(
        "health.dead_ends_action",
        "Add links from this page to related pages.",
    ),
    msg("health.problem_broken", "The target is not a page."),
    msg(
        "health.problem_loop",
        "The redirects lead round in a circle.",
    ),
    msg("health.problem_long", "The chain of redirects is too long."),
    msg("health.problem_double", "The target is itself a redirect."),
    msg(
        "health.action_broken",
        "Point it at a page that exists, or remove the redirect.",
    ),
    msg(
        "health.action_loop",
        "Make one of the redirects point at a real page.",
    ),
    msg(
        "health.action_long",
        "Point the first redirect straight at the final page.",
    ),
    msg(
        "health.action_double",
        "Point this redirect straight at the final page.",
    ),
    msg("health.mark_wip", "Work in progress"),
    msg("health.mark_draft", "Draft"),
    msg("health.mark_stub", "Stub"),
    msg(
        "health.help_action",
        "Finish or improve the page, then remove the mark.",
    ),
    msg(
        "health.stale_never",
        "Not changed in the recent changes looked through",
    ),
    msg("health.stale_seen", "In a change first seen {when}"),
    msg(
        "health.stale_action",
        "Read the page, and update it if it is out of date.",
    ),
    msg(
        "health.stale_window",
        "The history does not say when a page last changed, so this list looks only through the newest {limit} changes.",
    ),
    msg(
        "health.none_wanted",
        "No page is wanted: every link leads to a page.",
    ),
    msg(
        "health.none_orphans",
        "Every page has a page that links to it.",
    ),
    msg("health.none_dead_ends", "Every page links to another page."),
    msg("health.none_redirects", "Every redirect leads to a page."),
    msg("health.none_help", "No page is marked as needing help."),
    msg("health.none_stale", "There are no pages to list."),
    msg("title.links", "Links to {page} · {wiki} · {product}"),
    msg("links.heading", "Pages that link to {page}"),
    msg(
        "links.missing",
        "No page has this title yet. These pages link to it.",
    ),
    msg("links.go_to", "Go to {page}"),
    msg("links.none", "No page links here."),
    msg("links.kind_include", "Includes this page"),
    msg("links.kind_redirect", "Redirects here"),
    msg("links.cut", "Only the first {limit} are listed."),
    msg("head.feed_wiki", "Recent changes to {wiki}"),
    msg("head.feed_page", "Changes to {page}"),
    msg("head.markdown", "{page} as Markdown"),
    msg("head.search", "Search {wiki}"),
    // the API's problem for a change that is not in the history (handoff 003 T8, completion)
    msg("problem.change_not_found.title", "Change not found"),
    msg(
        "problem.change_not_found.detail",
        "This wiki has no change with this id in this view.",
    ),
    msg(
        "problem.change_not_found.hint",
        "Take the id of a change from the history of a page or from the recent changes.",
    ),
    // the inbox, following a page, and the flags of changes (handoff 004 T6)
    msg("title.inbox", "Inbox · {product}"),
    msg("inbox.heading", "Inbox"),
    msg(
        "inbox.intro",
        "What has happened to your proposals and to the pages you follow, newest first.",
    ),
    msg("inbox.none", "Nothing has happened yet that involves you."),
    msg("inbox.new", "New"),
    msg("inbox.mark_all_read", "Mark everything as read"),
    msg("inbox.someone", "Someone"),
    msg(
        "inbox.event_reviewed",
        "{person} reviewed your proposal {proposal} in {wiki}.",
    ),
    msg(
        "inbox.event_accepted",
        "Your proposal {proposal} in {wiki} was accepted and signed into the history.",
    ),
    msg(
        "inbox.event_declined",
        "{person} declined your proposal {proposal} in {wiki}.",
    ),
    msg(
        "inbox.event_needs_revision",
        "{person} asked for a revision of your proposal {proposal} in {wiki}.",
    ),
    msg(
        "inbox.event_withdrawn",
        "{person} withdrew the proposal {proposal} in {wiki}.",
    ),
    msg(
        "inbox.event_superseded",
        "{person} replaced your proposal {proposal} in {wiki} with a later one.",
    ),
    msg(
        "inbox.event_replied",
        "{person} commented in the discussion of {proposal} in {wiki}.",
    ),
    msg(
        "inbox.event_mentioned",
        "{person} mentioned you in the discussion of {proposal} in {wiki}.",
    ),
    msg(
        "inbox.event_watched",
        "A page you follow in {wiki} changed. The change came from the proposal {proposal}.",
    ),
    msg(
        "inbox.event_unreadable",
        "Something happened that involves you, in a wiki you can no longer read.",
    ),
    msg("inbox.account_heading", "An account is needed"),
    msg(
        "inbox.account_body",
        "Your inbox belongs to an account. Sign in to see what has happened to your proposals and to the pages you follow.",
    ),
    msg("inbox.following_heading", "Pages you follow"),
    msg("inbox.following_none", "You do not follow any page yet."),
    msg("inbox.following_page", "{page} in {wiki}"),
    msg(
        "inbox.following_namespace",
        "The pages under {page} in {wiki}",
    ),
    msg("inbox.unfollow", "Stop following {what}"),
    msg("watch.heading_followed", "Following"),
    msg("watch.heading_stopped", "No longer following"),
    msg("watch.heading_full", "You follow as many as you can"),
    msg(
        "watch.page_followed",
        "You follow the page {title}. When a change to it is signed into the history, you will be told in your inbox.",
    ),
    msg(
        "watch.namespace_followed",
        "You follow the pages under {title}. When a change to one of them is signed into the history, you will be told in your inbox.",
    ),
    msg(
        "watch.page_stopped",
        "You no longer follow the page {title}.",
    ),
    msg(
        "watch.namespace_stopped",
        "You no longer follow the pages under {title}.",
    ),
    msg(
        "watch.account_body",
        "Following a page belongs to an account. Sign in, then follow the page again.",
    ),
    msg(
        "watch.full_body",
        "You follow as many pages as you can. Stop following one of them, then try again.",
    ),
    msg("watch.back", "Back to {title}"),
    msg("watch.to_inbox", "Your inbox"),
    msg("changes.filter_minor", "Minor changes"),
    msg("changes.active_minor_hide", "Minor changes are hidden"),
    msg("changes.active_minor_only", "Only minor changes"),
    msg("changes.minor_label", "Minor change"),
    msg(
        "changes.machine_assisted",
        "Machine-assisted content, accepted by {person}",
    ),
    msg("changes.machine_assisted_only", "Machine-assisted content"),
    // proposals (handoff 004 T3): begin
    msg("wiki.nav_proposals", "Proposals"),
    msg("page.propose_change", "Propose a change"),
    msg(
        "page.propose_part",
        "Propose a change to a section or a part",
    ),
    msg("page.discussion", "Discussion"),
    msg("propose.new_page_link", "Propose this page"),
    msg("state.draft", "Draft"),
    msg("state.held", "Held for review"),
    msg("state.needs_revision", "Needs a revision"),
    msg("state.accepted", "Accepted"),
    msg("state.declined", "Declined"),
    msg("state.withdrawn", "Withdrawn"),
    msg("state.superseded", "Superseded"),
    msg(
        "state_note.draft",
        "A draft. Only the person who started it can see it, and nobody has been asked to review it.",
    ),
    msg(
        "state_note.held",
        "The wiki service holds this proposal for review. Nothing becomes part of a page until someone who is allowed to takes the proposal into the history.",
    ),
    msg(
        "state_note.needs_revision",
        "This proposal needs a revision from {author} before it can be taken into the history.",
    ),
    msg(
        "state_note.accepted",
        "Accepted: someone who is allowed to took this proposal into the history.",
    ),
    msg(
        "state_note.declined",
        "Declined: someone who is allowed to decided not to take this proposal into the history.",
    ),
    msg(
        "state_note.withdrawn",
        "Withdrawn by its proposer. It is kept here for the record.",
    ),
    msg(
        "state_note.superseded",
        "Superseded: a later proposal takes its place.",
    ),
    msg("proposal.superseded_by", "It was replaced by {proposal}."),
    msg("proposal.another", "another proposal"),
    msg(
        "proposal.reason_declined",
        "{person} declined this proposal. Reason: {reason}",
    ),
    msg(
        "proposal.reason_withdrawn",
        "{person} withdrew this proposal. Reason: {reason}",
    ),
    msg(
        "proposal.reason_asked",
        "{person} asked for a revision. Reason: {reason}",
    ),
    msg("outcome.signed", "Signed into the history"),
    msg("outcome.overlap", "Stopped: changes overlap"),
    msg("outcome.not_allowed", "Not allowed"),
    msg("outcome.busy", "The wiki is busy"),
    msg("outcome.recovery", "The wiki needs recovery"),
    msg("outcome.refused", "Refused: {limit}"),
    msg("outcome.refused_heading", "Refused"),
    msg(
        "outcome_note.signed",
        "The proposal is part of the history now.",
    ),
    msg(
        "outcome_note.overlap",
        "Another change touched the same text. Nothing was changed. The proposer is asked for a revision, and both texts are shown on the proposal.",
    ),
    msg(
        "outcome_note.not_allowed",
        "Your role in this wiki does not include accepting proposals. Nothing was changed.",
    ),
    msg(
        "outcome_note.busy",
        "Another change is being signed into the history. Nothing was changed; try again in a moment.",
    ),
    plural_msg(
        "outcome_note.busy_depth",
        "One acceptance is waiting ahead of yours.",
        "{count} acceptances are waiting ahead of yours.",
    ),
    msg(
        "outcome_note.recovery",
        "The wiki is being checked after an interruption. It can be read, but nothing can be changed until the check is done.",
    ),
    msg(
        "outcome.unchanged_refused",
        "Not signed: the history refused it",
    ),
    msg(
        "outcome_note.unchanged_refused",
        "The history did not accept this change as it was prepared. Nothing was changed, and the proposal is still held for review.",
    ),
    msg("outcome.too_large", "Too large to sign now"),
    msg(
        "outcome_note.too_large",
        "This change needs more memory than one signing may use. Nothing was changed, and the proposal is still held for review.",
    ),
    msg("outcome.unavailable", "Could not be done now"),
    msg(
        "outcome_note.unavailable",
        "The records or the history could not be used just now. Nothing was changed. Try again in a little while; if it keeps happening, tell the person who runs this server.",
    ),
    msg("outcome.in_progress", "This revision is being signed"),
    msg(
        "outcome_note.in_progress",
        "An earlier attempt to take this revision into the history has not finished. Nothing was done now. Wait a little and read the proposal again.",
    ),
    msg("outcome.nothing_to_change", "Nothing to change"),
    msg(
        "outcome_note.nothing_to_change",
        "The history already has everything this proposal asks for. Nothing was changed.",
    ),
    msg("outcome.store", "The result is not certain"),
    msg(
        "outcome_note.store",
        "The records could not be used while the change was being taken in. It may or may not be in the history now: read the history before you try again, and tell the person who runs this server.",
    ),
    msg("accept.heading", "Accepting is not available yet"),
    msg(
        "accept.not_yet",
        "This server does not take proposals into the history yet.",
    ),
    msg(
        "accept.not_yet_kept",
        "The proposal stays held for review and nothing was changed.",
    ),
    msg(
        "accept.not_yet_note",
        "Accepting is not available yet on this server.",
    ),
    msg("overlap.heading", "Where the changes overlap"),
    msg(
        "overlap.intro",
        "The wiki's current text and this proposal both changed the places below. Nothing was merged. Revise the proposal with both texts in front of you.",
    ),
    msg(
        "overlap.caption",
        "Places changed both in the wiki and in this proposal",
    ),
    msg("overlap.col_where", "Where"),
    msg("overlap.col_theirs", "In the wiki now"),
    msg("overlap.col_ours", "In this proposal"),
    msg("overlap.nothing", "(removed: nothing is there)"),
    msg("overlap.row", "{page}: {place}"),
    msg("overlap.place_top", "Information at the top of the page"),
    msg(
        "overlap.place_key",
        "Information at the top of the page: {key}",
    ),
    msg("overlap.place_part", "A part of the page"),
    msg("overlap.place_number", "Part {number}"),
    msg("overlap.place_named", "The part named {id}"),
    msg("overlap.place_page", "The whole page"),
    msg("overlap.place_path", "The page's address"),
    msg("quota.open_proposals", "too many open proposals"),
    msg("quota.revisions", "too many revisions of one proposal"),
    msg("quota.accepted_bytes", "too much accepted text today"),
    msg(
        "quota.hour_person",
        "too many proposals opened in the last hour",
    ),
    msg(
        "quota.hour_wiki",
        "too many proposals opened in this wiki in the last hour",
    ),
    msg("quota.queue", "too many acceptances waiting"),
    msg(
        "quota.counts",
        "The limit is {allowed}. The count now is {current}, and this asked for {asked}.",
    ),
    msg("quota.reset_at", "It resets at {when}."),
    msg(
        "quota.reset_proposal_closes",
        "It resets when one of your open proposals is accepted, declined, withdrawn or superseded.",
    ),
    msg(
        "quota.reset_queue",
        "It resets when a waiting acceptance gets its turn or gives up.",
    ),
    msg(
        "quota.reset_never_revisions",
        "Waiting does not help: this proposal has all the revisions it can have. Open a new proposal instead.",
    ),
    msg(
        "quota.reset_never_too_big",
        "Waiting does not help: this is more than the limit allows at any time. Make it smaller.",
    ),
    msg("size.mib", "{amount} MiB"),
    msg("size.kib", "{amount} KiB"),
    msg("size.bytes", "{amount} bytes"),
    msg("needs_account.heading", "Proposing needs an account"),
    msg(
        "needs_account.body",
        "You are not signed in, and proposing a change, commenting or reviewing needs an account.",
    ),
    msg(
        "needs_account.advice",
        "Ask the person who runs this server how to get an account.",
    ),
    msg(
        "refused.not_allowed",
        "Your role in this wiki does not include this. Nothing was changed.",
    ),
    msg("refused.not_now_heading", "Not possible now"),
    msg("refused.invalid_heading", "This could not be saved"),
    msg(
        "refused.not_in_this_state",
        "This proposal is {state}, so this cannot be done now.",
    ),
    msg(
        "refused.already_decided",
        "You have already decided on this revision. A decision is not changed; decide again on the next revision.",
    ),
    msg(
        "refused.stale_revision",
        "This proposal has a newer revision, number {latest}. Read it again and decide on that one.",
    ),
    msg(
        "refused.superseder",
        "A proposal that is {state} cannot replace another one.",
    ),
    msg(
        "refused.changed",
        "The proposal changed while you were working on it. Read it again, then try again.",
    ),
    msg(
        "invalid.incomplete",
        "The form was not complete. Open it again from the page and try again.",
    ),
    msg(
        "invalid.form_too_large",
        "What you sent is larger than a form can carry.",
    ),
    msg(
        "invalid.summary_missing",
        "Say in a few words what you changed and why.",
    ),
    msg(
        "invalid.summary_too_long",
        "The summary is longer than {limit} characters.",
    ),
    msg(
        "invalid.summary_not_one_line",
        "The summary must be one line of plain text.",
    ),
    msg(
        "invalid.too_many_sources",
        "At most {limit} sources can be named.",
    ),
    msg(
        "invalid.bad_source",
        "Each source must be one line, not empty and not longer than {limit} bytes.",
    ),
    msg(
        "invalid.nothing_proposed",
        "The text is the same as the page: change it first, then send it.",
    ),
    msg(
        "invalid.too_large",
        "A proposal can hold at most {limit} of text in all.",
    ),
    msg(
        "invalid.page_too_large",
        "A page can hold at most {limit} of text.",
    ),
    msg(
        "invalid.reason_missing",
        "A reason is needed. Say it kindly and say what could happen next.",
    ),
    msg(
        "invalid.reason_too_long",
        "The reason is longer than {limit}.",
    ),
    msg("invalid.comment_empty", "A comment needs some words."),
    msg(
        "invalid.comment_too_long",
        "The comment is longer than {limit}.",
    ),
    msg("title.proposals", "Proposals · {wiki} · {product}"),
    msg("proposals.heading", "Proposals"),
    msg(
        "proposals.intro",
        "Changes to {wiki} are proposed here, read by others, and taken into the history by someone who is allowed to.",
    ),
    msg("proposals.empty", "Nobody has proposed anything yet."),
    msg("proposals.new_page", "Propose a new page"),
    msg("proposals.drafts", "Your drafts"),
    msg("proposals.open", "Open proposals"),
    msg("proposals.closed", "Decided proposals"),
    msg(
        "proposals.row_by",
        "Proposed by {author} · last changed {when}",
    ),
    msg("proposal.no_summary", "(no summary yet)"),
    msg("proposals.back", "Back to the proposals"),
    msg("proposal.back", "Back to the proposal"),
    msg("person.unknown", "someone"),
    msg(
        "title.propose_parts",
        "Choose what to change in {page} · {wiki} · {product}",
    ),
    msg("parts.heading", "Choose what to change in {page}"),
    msg(
        "parts.intro",
        "You can propose a change to the whole page, to one section, or to one part.",
    ),
    msg("parts.whole", "The whole page"),
    msg("parts.sections_heading", "Sections"),
    msg("parts.parts_heading", "Parts"),
    msg("parts.no_sections", "This page has no headings."),
    msg("parts.section_label", "Section: {heading}"),
    msg("parts.part_label", "Part {number}: {excerpt}"),
    msg("parts.cut", "Only the first {limit} are listed."),
    msg("parts.back", "Back to {page}"),
    msg(
        "title.propose_page",
        "Propose a change to {page} · {wiki} · {product}",
    ),
    msg(
        "title.propose_new",
        "Propose a new page · {wiki} · {product}",
    ),
    msg(
        "title.propose_revise",
        "Revise a proposal · {wiki} · {product}",
    ),
    msg("propose.heading_page", "Propose a change to {page}"),
    msg("propose.heading_new", "Propose a new page"),
    msg("propose.heading_revise", "Revise this proposal"),
    msg(
        "propose.intro",
        "Edit the text, say what you changed and why, and send it for review. Nothing changes in the wiki until someone who is allowed to takes your proposal into the history.",
    ),
    msg("propose.scope_whole", "You are changing the whole page."),
    msg(
        "propose.scope_section",
        "You are changing one section: {part}",
    ),
    msg("propose.scope_part", "You are changing one part: {part}"),
    msg("propose.choose", "Choose a different section or part"),
    msg("propose.label_text", "Text of {title}"),
    msg("propose.label_section", "Text of this section of {title}"),
    msg("propose.label_part", "Text of this part of {title}"),
    msg("propose.label_title", "Title of the new page"),
    msg(
        "propose.title_hint",
        "A slash makes a page part of a group, for example Guides/Setup.",
    ),
    msg("propose.new_page_name", "the new page"),
    msg(
        "propose.label_summary",
        "What and why (up to {limit} characters)",
    ),
    msg(
        "propose.summary_hint",
        "One line, for example: Correct the date of the first meeting.",
    ),
    msg("propose.label_sources", "Sources (one per line, optional)"),
    msg(
        "propose.sources_hint",
        "Links or references that support the change.",
    ),
    msg("propose.preview", "Preview"),
    msg("propose.show_changes", "Show changes"),
    msg("propose.save_draft", "Save as draft"),
    msg("propose.submit", "Submit"),
    msg("propose.submit_revision", "Submit the revision"),
    msg("propose.saved", "Your draft is saved. Only you can see it."),
    msg("propose.problems", "Please fix this first"),
    msg("propose.continue_draft", "Continue with your draft"),
    msg("propose.removed_note", "This proposal removes {title}."),
    msg("propose.preview_heading", "Preview of {title}"),
    msg(
        "propose.preview_note",
        "This is how your text will look. Nothing is saved yet.",
    ),
    msg("propose.suggestions", "Suggestions"),
    msg(
        "propose.front_unreadable",
        "The information at the top of the page cannot be read. Each line needs a name, a colon and a value.",
    ),
    msg(
        "propose.front_owned",
        "The lines {keys} at the top of the page can only be changed by an administrator of this wiki.",
    ),
    msg(
        "propose.title_empty",
        "A title cannot be empty, and no part of it between slashes can be empty.",
    ),
    msg(
        "propose.title_reserved",
        "A part of a title cannot be a single dot or two dots.",
    ),
    msg("propose.title_too_long", "A part of the title is too long."),
    msg(
        "propose.title_too_deep",
        "The title is too long, or has too many parts.",
    ),
    msg(
        "propose.title_taken",
        "A page titled {title} exists already. Propose a change to it instead.",
    ),
    msg("changes.proposal_heading", "What it changes"),
    msg(
        "changes.proposal_none",
        "The text is the same as the page now: there is nothing to propose.",
    ),
    msg("changes.page_heading", "Page: {title}"),
    msg("changes.status_added", "A new page."),
    msg("changes.status_removed", "The page is removed."),
    msg("changes.status_modified", "The page is changed."),
    msg("changes.info_heading", "Information at the top of the page"),
    msg("changes.part_changed", "Part {number} is changed"),
    msg("changes.part_added", "Part {number} is added"),
    msg("changes.part_removed", "Part {number} is removed"),
    plural_msg(
        "changes.unchanged_parts",
        "{count} part is the same on both sides.",
        "{count} parts are the same on both sides.",
    ),
    msg("title.proposal", "Proposal: {summary} · {wiki} · {product}"),
    msg("proposal.proposed_by", "Proposed by {author}, {when}."),
    plural_msg(
        "proposal.pages",
        "It changes {count} page.",
        "It changes {count} pages.",
    ),
    msg("proposal.revision", "This is revision {number} of {total}."),
    msg("proposal.view", "It is for the view {view}."),
    msg("proposal.sources_heading", "Sources"),
    msg("proposal.signed_change", "See the change"),
    msg("proposal.actions_heading", "What you can do"),
    msg("action.continue", "Continue editing"),
    msg("action.revise", "Revise this proposal"),
    msg("action.withdraw", "Withdraw this proposal"),
    msg("action.ask", "Ask for a revision"),
    msg("action.decline", "Decline this proposal"),
    msg("action.accept", "Accept"),
    msg("thread.heading", "Discussion"),
    msg("thread.none", "Nobody has commented yet."),
    msg("comment.meta", "{when} · on revision {number}"),
    msg("comment.on_part", "About part {number} of {title}"),
    msg(
        "comment.orphaned",
        "Orphaned: the part of {title} this comment was about is no longer in the proposal. It was about this text:",
    ),
    msg("comment.on_this_part", "Comment on part {number}"),
    msg("comment.reply", "Reply to {author}"),
    msg("comment_form.label", "Add a comment"),
    msg("comment_form.button", "Comment"),
    msg(
        "comment_form.about_part",
        "Your comment is about part {number} of {title}.",
    ),
    msg(
        "comment_form.reply_to",
        "Your comment is a reply to {author}.",
    ),
    msg("decisions.heading", "Decisions"),
    msg("decisions.none", "No reviewer has decided yet."),
    msg(
        "decision.approved",
        "{reviewer} approved, on revision {number}.",
    ),
    msg(
        "decision.requested",
        "{reviewer} asked for changes, on revision {number}.",
    ),
    msg(
        "decision.older",
        "A newer revision was written after this. The decision no longer counts toward accepting it.",
    ),
    msg("decide.label", "A note with your decision (optional)"),
    msg("decide.approve", "Approve this revision"),
    msg("decide.request", "Request changes"),
    msg("title.decline", "Decline a proposal · {wiki} · {product}"),
    msg("title.withdraw", "Withdraw a proposal · {wiki} · {product}"),
    msg("title.ask", "Ask for a revision · {wiki} · {product}"),
    msg("reason.decline_heading", "Decline this proposal"),
    msg(
        "reason.decline_intro",
        "Thank the proposer for the effort, say why the proposal is not taken into the history, and say what could be done next. The proposer is told, and everyone who reads the proposal sees your reason.",
    ),
    msg("reason.decline_button", "Decline"),
    msg("reason.withdraw_heading", "Withdraw this proposal"),
    msg(
        "reason.withdraw_intro",
        "Say why you are taking the proposal back. It stays here for the record, and everyone who reads it sees your reason.",
    ),
    msg("reason.withdraw_button", "Withdraw"),
    msg("reason.ask_heading", "Ask for a revision"),
    msg(
        "reason.ask_intro",
        "Say what needs to change, and why, so that the proposer knows what to do. The proposer is told and can send a new revision.",
    ),
    msg("reason.ask_button", "Ask for a revision"),
    msg("reason.label", "Reason"),
    msg(
        "title.discussion",
        "Discussion of {page} · {wiki} · {product}",
    ),
    msg("discussion.heading", "Discussion of {page}"),
    msg(
        "discussion.intro",
        "What has been said about this page so far: the proposals that change it, each with its discussion.",
    ),
    msg("discussion.empty", "No proposal changes this page yet."),
    msg(
        "discussion.cut",
        "Only the newest {limit} proposals were looked through.",
    ),
    msg("discussion.page", "Read {page}"),
    msg("discussion.propose", "Propose a change to this page"),
    msg(
        "lint.bom",
        "{guideline}: The text starts with a byte-order mark.",
    ),
    msg(
        "lint.crlf",
        "{guideline}: The page uses CR LF line endings; LF is recommended.",
    ),
    msg(
        "lint.newline",
        "{guideline}: The text does not end with a line break.",
    ),
    msg(
        "lint.tabs",
        "{guideline}: A tab character is used outside code; spaces are recommended.",
    ),
    msg(
        "lint.no_info",
        "{guideline}: The page has no information at the top. A title there makes the page portable.",
    ),
    msg(
        "lint.no_title",
        "{guideline}: The information at the top of the page has no title.",
    ),
    msg(
        "lint.coerced",
        "{guideline}: A value at the top of the page was read as another kind of value than you may mean.",
    ),
    msg(
        "lint.list",
        "{guideline}: A value at the top of the page that should be a list of texts is not one.",
    ),
    msg(
        "lint.status",
        "{guideline}: The status at the top of the page is not one of the recommended ones.",
    ),
    msg(
        "lint.redirect",
        "{guideline}: The page is a redirect but names no page to redirect to.",
    ),
    msg(
        "lint.visibility",
        "{guideline}: The visibility at the top of the page is not public, internal or restricted.",
    ),
    msg(
        "lint.secret",
        "{guideline}: A name at the top of the page looks like a secret. A page is not the place for secrets.",
    ),
    msg(
        "lint.duplicate_id",
        "{guideline}: Two parts of the page have the same identifier.",
    ),
    msg(
        "lint.unclosed",
        "{guideline}: A container is opened and never closed.",
    ),
    msg(
        "lint.callout",
        "{guideline}: A callout is not one of the usual kinds.",
    ),
    msg(
        "lint.container",
        "{guideline}: A container has a name this wiki does not know.",
    ),
    msg(
        "lint.heading_top",
        "{guideline}: The page uses a top-level heading, or skips a heading level.",
    ),
    msg(
        "lint.heading_dup",
        "{guideline}: Two headings have the same text, so their links share a name.",
    ),
    msg(
        "lint.tags",
        "{guideline}: A tag is not written the usual way.",
    ),
    msg(
        "lint.empty_label",
        "{guideline}: A link has an empty label.",
    ),
    msg("lint.empty_target", "{guideline}: A link has no target."),
    msg(
        "lint.missing_part_ref",
        "{guideline}: A link to a part of this page points to a part that does not exist.",
    ),
    msg(
        "lint.prefix",
        "{guideline}: A link names a place with a prefix the wiki does not use.",
    ),
    msg(
        "lint.order",
        "{guideline}: A link could mean more than one page; the nearest one is taken.",
    ),
    msg(
        "lint.dangling",
        "{guideline}: A link leads to a page that does not exist yet.",
    ),
    msg(
        "lint.ambiguous",
        "{guideline}: A link matches several pages and none is nearer.",
    ),
    msg(
        "lint.other",
        "{guideline}: The linter has a suggestion for this text.",
    ),
    // proposals (handoff 004 T3): end
];
