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
];
