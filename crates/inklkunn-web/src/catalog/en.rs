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
        "You can propose it. A proposal needs an account, and the page exists once someone who is allowed to takes the proposal into the history.",
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
    msg(
        "verification.explained_by_retirement",
        "The only failures are those the retirement of key {key} recorded. That key was removed from this wiki's trust, so what it signed can no longer be checked; nothing else failed.",
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
        "This build of the server does not offer this.",
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
        "This address of the API does not take that method.",
    ),
    msg(
        "problem.method_not_allowed.hint",
        "The API description of this server lists the methods each address takes.",
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
    msg("health.propose_page", "Propose this page ({title})"),
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
    msg(
        "health.knowledge_name",
        "Knowledge files that cannot be read",
    ),
    msg(
        "health.knowledge_desc",
        "Claim and source files that are not in the form the wiki reads. Nothing is read from them, and the pages are not affected.",
    ),
    msg("health.col_file", "File"),
    msg("health.col_rule", "What is wrong"),
    msg(
        "health.knowledge_action",
        "Propose a corrected file, or propose to remove it.",
    ),
    msg("health.none_knowledge", "Every knowledge file can be read."),
    msg(
        "health.rule_path",
        "The file is not in knowledge/claims or knowledge/sources, or its name is not a well formed id followed by .yaml.",
    ),
    msg("health.rule_not_text", "The file is not UTF-8 text."),
    msg(
        "health.rule_too_long",
        "The file is longer than a knowledge file may be.",
    ),
    msg(
        "health.rule_syntax",
        "The text is not in the plain YAML subset the wiki reads.",
    ),
    msg(
        "health.rule_missing",
        "A key the format requires is missing.",
    ),
    msg(
        "health.rule_unknown",
        "The file has a key the format does not have.",
    ),
    msg(
        "health.rule_type",
        "A value is not of the shape its key needs.",
    ),
    msg(
        "health.rule_number",
        "A number appears where the format has none. Write dates and years as text.",
    ),
    msg(
        "health.rule_value",
        "A value is outside the words, or the form, its key allows.",
    ),
    msg("health.rule_id", "An identifier is not well formed."),
    msg("health.rule_duplicate", "The same entry appears twice."),
    msg(
        "health.rule_bounds",
        "A list or a text is longer than the format allows.",
    ),
    msg(
        "health.rule_reference",
        "The file names a source or a claim the wiki does not have.",
    ),
    msg(
        "health.rule_anchor",
        "An expression names a page, or a paragraph of a page, that the wiki does not have.",
    ),
    msg(
        "health.rule_in_use",
        "The file is removed while another file still names it.",
    ),
    msg(
        "health.rule_owned",
        "The change also changes a page key that only the service writes.",
    ),
    msg(
        "health.rule_status",
        "The change sets a community status, a review date or an availability, changes or removes a source link a claim already has, changes a source that was checked in its title, author, date, reference or the source that replaces it, removes a claim or a source that holds any of these, or replaces or removes a file that cannot be read, outside a curation proposal.",
    ),
    msg(
        "health.rule_judged",
        "A judgement the change sets (a source link's or the community status's) is judged on a wording that no paragraph of its claim has in the result.",
    ),
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
    // the write API's problems (handoff 004 T7)
    msg("problem.authentication_required.title", "Sign-in needed"),
    msg(
        "problem.authentication_required.detail",
        "This request carries no credential, and writing needs an account.",
    ),
    msg(
        "problem.authentication_required.hint",
        "Send an API token in an Authorization header, as Bearer followed by the token.",
    ),
    msg("problem.invalid_token.title", "Token not accepted"),
    msg(
        "problem.invalid_token.detail",
        "The token in the Authorization header is not one this server accepts: it may be unknown, expired or revoked, or its account may be disabled.",
    ),
    msg(
        "problem.invalid_token.hint",
        "Issue a new token and send that one.",
    ),
    msg("problem.forbidden.title", "Not allowed"),
    msg(
        "problem.forbidden.detail",
        "This credential may not do this: the account's role in this wiki or the scope the token was issued with does not allow it, or a request made with a session cookie came from another site.",
    ),
    msg(
        "problem.forbidden.hint",
        "Ask a maintainer of the wiki for the role, or use a token issued with a wider scope.",
    ),
    msg("problem.base_required.title", "Base needed"),
    msg(
        "problem.base_required.detail",
        "A write names the state it was written against, and this request has no If-Match header.",
    ),
    msg(
        "problem.base_required.hint",
        "Send the ETag of the proposal's revision as If-Match; for a new proposal, send the state id of the view's tip.",
    ),
    msg("problem.stale_base.title", "The base is out of date"),
    msg(
        "problem.stale_base.detail",
        "What this write was made against is no longer the current state: someone else got there first. Nothing was recorded.",
    ),
    msg(
        "problem.stale_base.hint",
        "Read the current state, check your change against it, and send again with the current value in If-Match. This problem carries that value.",
    ),
    msg("problem.invalid_body.title", "Body not accepted"),
    msg(
        "problem.invalid_body.detail",
        "The body is not a JSON object of the shape this endpoint takes: the member {member} is missing, repeated, unknown or has a value it may not have.",
    ),
    msg(
        "problem.invalid_body.hint",
        "Send the members the description of this endpoint lists, and no others.",
    ),
    msg("problem.body_too_large.title", "Body too large"),
    msg(
        "problem.body_too_large.detail",
        "The body is larger than this endpoint takes: at most {limit} bytes. Nothing was read of it.",
    ),
    msg(
        "problem.body_too_large.hint",
        "Send less: split the change, or shorten the text.",
    ),
    msg("problem.unsupported_media_type.title", "Not sent as JSON"),
    msg(
        "problem.unsupported_media_type.detail",
        "The body must be JSON, sent with the content type application/json.",
    ),
    msg(
        "problem.unsupported_media_type.hint",
        "Set the Content-Type header to application/json.",
    ),
    msg("problem.unacceptable.title", "Not acceptable as sent"),
    msg("problem.unacceptable.detail", "{explanation}"),
    msg(
        "problem.unacceptable.hint",
        "Correct what the reason names and send again. Nothing was recorded.",
    ),
    msg("problem.not_in_this_state.title", "Not possible now"),
    msg(
        "problem.not_in_this_state.detail",
        "This cannot be done while the proposal is {state}.",
    ),
    msg(
        "problem.not_in_this_state.hint",
        "Read the proposal: what is possible depends on its state.",
    ),
    msg("problem.already_decided.title", "Already decided"),
    msg(
        "problem.already_decided.detail",
        "You have decided on this revision already, and a decision is not changed.",
    ),
    msg(
        "problem.already_decided.hint",
        "Decide again on the next revision, when there is one.",
    ),
    msg("problem.own_proposal.title", "Your own proposal"),
    msg(
        "problem.own_proposal.detail",
        "A proposer cannot approve or ask for changes on their own proposal.",
    ),
    msg(
        "problem.own_proposal.hint",
        "Ask a reviewer to decide on it. You can still comment on your own proposal.",
    ),
    msg("problem.refused_limit.title", "Refused: a limit"),
    msg(
        "problem.refused_limit.detail",
        "A limit was reached: {limit}. The members allowed, current and asked give the numbers, and reset says when it will be possible again.",
    ),
    msg(
        "problem.refused_limit.hint",
        "Where waiting helps, Retry-After says how long. Otherwise close a proposal of yours, or ask for less.",
    ),
    msg("problem.busy.title", "The wiki is busy"),
    msg(
        "problem.busy.detail",
        "Another act on this wiki is running or waiting. Acts ahead of this one: {waiting}. Nothing was done.",
    ),
    msg(
        "problem.busy.hint",
        "Send it again after the time Retry-After gives.",
    ),
    msg("problem.needs_recovery.title", "The wiki needs recovery"),
    msg(
        "problem.needs_recovery.detail",
        "A write to this wiki did not finish. The wiki accepts no more changes until recovery has looked at it, and reading still works.",
    ),
    msg(
        "problem.needs_recovery.hint",
        "Tell the person who runs this server. This request did nothing.",
    ),
    msg("problem.overlap.title", "Changes overlap"),
    msg(
        "problem.overlap.detail",
        "Stopped: the proposal and the wiki changed the same places, {overlaps} of them. The proposal now needs a revision, and the wiki is as it was.",
    ),
    msg(
        "problem.overlap.hint",
        "Read the overlap on the proposal, write a revision that takes both texts into account, and send it.",
    ),
    msg("problem.nothing_to_change.title", "Nothing to change"),
    msg(
        "problem.nothing_to_change.detail",
        "The wiki already has everything this revision asks for.",
    ),
    msg(
        "problem.nothing_to_change.hint",
        "Decline or withdraw the proposal, or revise it.",
    ),
    msg(
        "problem.acceptance_in_progress.title",
        "Acceptance already underway",
    ),
    msg(
        "problem.acceptance_in_progress.detail",
        "An earlier acceptance of this revision has not finished.",
    ),
    msg(
        "problem.acceptance_in_progress.hint",
        "Wait and ask again. Recovery settles it if the earlier one was interrupted.",
    ),
    msg("problem.acceptance_failed.title", "Acceptance failed"),
    msg(
        "problem.acceptance_failed.detail",
        "The change was not made: {word}. The proposal stays open and the history is as it was.",
    ),
    msg(
        "problem.acceptance_failed.hint",
        "Try again, or tell the person who runs this server.",
    ),
    msg(
        "problem.signing_not_configured.title",
        "Signing is not set up",
    ),
    msg(
        "problem.signing_not_configured.detail",
        "This server is not set up to sign changes into the history, so it takes no proposal into the history; reading, proposing and commenting still work.",
    ),
    msg(
        "problem.signing_not_configured.hint",
        "Tell the person who runs this server. This request did nothing.",
    ),
    msg(
        "problem.retirement_unfinished.title",
        "A change to the wiki's keys is not finished",
    ),
    msg(
        "problem.retirement_unfinished.detail",
        "A change to this wiki's keys is not finished, so nothing new can be signed into its history until the person who runs this wiki finishes or gives up a change to its keys. The change was not made, and the proposal is kept.",
    ),
    msg(
        "problem.retirement_unfinished.hint",
        "Tell the person who runs this wiki. When they have finished or given up the change, accept the proposal again.",
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
    msg(
        "outcome.knowledge_invalid",
        "Not signed: a claim or source file is not acceptable",
    ),
    msg(
        "outcome_note.knowledge_invalid",
        "A claim or source file in this proposal is not in the form the wiki reads, or the change would leave another file pointing at something that is not there. Nothing was changed, and the proposal is still held for review. Correct the file and accept again.",
    ),
    msg("outcome.too_large", "Too large to sign now"),
    msg(
        "outcome_note.too_large",
        "This change needs more memory than one signing may use. Nothing was changed, and the proposal is still held for review.",
    ),
    msg(
        "outcome.retirement_unfinished",
        "Not signed: a change to this wiki's keys is not finished",
    ),
    msg(
        "outcome_note.retirement_unfinished",
        "A change to this wiki's keys is not finished, so nothing new can be signed into its history until the person who runs this wiki finishes or gives up a change to its keys. Nothing was changed, and the proposal is kept. Tell that person; once it is finished or given up, accept the proposal again.",
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
    msg("accept.heading", "Accepting is not available in this build"),
    msg(
        "accept.not_served",
        "This build of the server does not take proposals into the history.",
    ),
    msg(
        "accept.not_served_kept",
        "The proposal stays held for review and nothing was changed.",
    ),
    msg(
        "accept.not_served_note",
        "Accepting is not available on this server.",
    ),
    msg(
        "accept.unconfigured_heading",
        "Accepting is not set up on this server",
    ),
    msg(
        "accept.unconfigured",
        "This server is not set up to sign changes into the history, so nothing was changed and the proposal stays held for review; tell the person who runs this server.",
    ),
    msg(
        "accept.unconfigured_note",
        "Accepting is not set up on this server.",
    ),
    msg("overlap.heading", "Where the changes overlap"),
    msg(
        "overlap.intro",
        "The wiki's current text and this proposal both changed the places below. Nothing was changed. Revise the proposal with both texts in front of you.",
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
        "quota.comments_hour_person",
        "too many comments in the last hour",
    ),
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
        "Ask the person who runs this wiki how to get an account.",
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
        "refused.own_proposal",
        "You cannot approve or ask for changes on your own proposal. Ask a reviewer to decide on it; you can still comment.",
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
        "invalid.path_collides",
        "A page whose name differs from this one only in capital letters, or the page you are moving onto, exists already. Choose another title.",
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
    // sign-in and accounts (the sign-in handoff U1, U2): begin
    msg("header.account_nav", "Account"),
    msg("header.signed_in", "Signed in as {name}"),
    msg("header.sign_out", "Sign out"),
    msg("header.sign_in", "Sign in"),
    msg("title.form_expired", "Form expired · {product}"),
    msg("form_expired.heading", "The form has expired"),
    msg(
        "form_expired.body",
        "This form could not be matched to your sign-in, so nothing was changed. It may be old, or you may have signed in or out since you opened it. Go back, reload the page and try again.",
    ),
    msg(
        "problem.unavailable",
        "That did not work this time, and nothing was changed. Try again in a while; if it keeps happening, tell the person who runs this server.",
    ),
    msg("title.sign_in", "Sign in · {product}"),
    msg("sign_in.heading", "Sign in"),
    msg("sign_in.login_label", "Sign-in name"),
    msg("sign_in.password_label", "Password"),
    msg("sign_in.button", "Sign in"),
    msg(
        "sign_in.failed",
        "That sign-in name and password did not match. Check both and try again.",
    ),
    msg(
        "sign_in.locked",
        "There have been too many attempts. Wait a while before trying again.",
    ),
    msg(
        "sign_in.busy",
        "The server is busy right now. Try again in a moment.",
    ),
    msg("sign_in.already", "You are signed in as {name}."),
    msg("sign_in.to_account", "Go to your account"),
    msg(
        "sign_in.invitation_only",
        "Ask the person who runs this wiki for an invitation.",
    ),
    msg("sign_in.join", "Join this wiki"),
    msg("title.sign_out", "Sign out · {product}"),
    msg("sign_out.heading", "Sign out"),
    msg(
        "sign_out.body",
        "This signs this browser out. Other browsers that are signed in to your account stay signed in.",
    ),
    msg("sign_out.not_signed_in", "You are not signed in."),
    msg("title.join", "Join · {product}"),
    msg("join.heading", "Join this wiki"),
    msg(
        "join.intro",
        "Choose a sign-in name and a password to make your account. Nobody, including the person who runs this server, can read your password.",
    ),
    msg("join.button", "Join"),
    msg("join_closed.heading", "Joining is not open"),
    msg(
        "join_closed.body",
        "Accounts here are made from invitations. Ask the person who runs this wiki for an invitation.",
    ),
    msg("title.invite", "Invitation · {product}"),
    msg("invite.heading", "You are invited"),
    msg(
        "invite.intro",
        "Choose a sign-in name and a password to make your account. This link works once. Nobody, including the person who runs this server, can read your password.",
    ),
    msg("invite.button", "Make my account"),
    msg("invite_unusable.heading", "This invitation cannot be used"),
    msg(
        "invite_unusable.body",
        "It may have been used already, or it may have ended. Ask the person who runs this wiki for a new one.",
    ),
    msg("account_form.name_label", "Your name"),
    msg(
        "account_form.name_hint",
        "The name other people see next to what you write. Any script is fine.",
    ),
    msg(
        "account_form.login_hint",
        "3 to 64 characters: letters a to z, digits, dots, underscores and hyphens. Capital letters count as small ones.",
    ),
    msg("account_form.password_label", "Choose a password"),
    msg(
        "account_form.password_hint",
        "At least {min} characters. There are no other rules; a long sentence makes a good password.",
    ),
    msg(
        "account_form.login_taken",
        "That sign-in name is taken. Choose another one.",
    ),
    msg(
        "account_form.login_length",
        "A sign-in name has 3 to 64 characters.",
    ),
    msg(
        "account_form.login_characters",
        "A sign-in name uses letters a to z, digits, dots, underscores and hyphens, and starts with a letter or a digit.",
    ),
    msg(
        "account_form.name_length",
        "A name has 1 to 120 characters.",
    ),
    msg(
        "account_form.name_control",
        "A name cannot contain line breaks or control characters.",
    ),
    msg(
        "account_form.name_space",
        "A name cannot start or end with a space.",
    ),
    msg(
        "account_form.password_short",
        "A password needs at least {min} characters.",
    ),
    msg("account_form.password_long", "That password is too long."),
    msg("title.account", "Your account · {product}"),
    msg("account.heading", "Your account"),
    msg("account.name_heading", "Your name"),
    msg("account.name_label", "Name"),
    msg(
        "account.name_hint",
        "The name other people see next to what you write.",
    ),
    msg("account.name_button", "Save name"),
    msg("account.name_saved", "Your name was saved."),
    msg("account.sessions_heading", "Where you are signed in"),
    msg(
        "account.sessions_intro",
        "Each line is a browser that is signed in to your account now. Ending a session signs that browser out.",
    ),
    msg("account.session_line", "Started {started}; ends {ends}"),
    msg(
        "account.session_current",
        "This is the browser you are using.",
    ),
    msg("account.session_end", "End this session"),
    msg("account.session_ended", "That session was ended."),
    msg("account.tokens_heading", "API tokens"),
    msg(
        "account.tokens_intro",
        "An API token lets a program act as you through the write API. It can do no more than the scope you give it, and no more than your roles allow. It is shown once, when you make it.",
    ),
    msg("account.tokens_none", "You have no tokens."),
    msg("account.token_unlabelled", "A token with no note"),
    msg("account.token_scope", "Scope: {scope}"),
    msg("account.token_live", "Works until {when}"),
    msg("account.token_expired", "Ended {when}"),
    msg("account.token_revoked", "Revoked {when}"),
    msg("account.token_revoke", "Revoke"),
    msg("account.token_revoked_note", "That token was revoked."),
    msg("account.token_new_heading", "Make a token"),
    msg("account.token_label", "What is it for?"),
    msg(
        "account.token_label_hint",
        "A short note to recognise it by, up to 80 characters.",
    ),
    msg("account.token_scope_label", "Scope"),
    msg("account.scope_read", "Read: pages, history and proposals"),
    msg(
        "account.scope_propose",
        "Propose: also comment and propose changes",
    ),
    msg("account.scope_review", "Review: also review proposals"),
    msg("account.scope_accept", "Accept: also accept proposals"),
    msg("account.scope_admin", "Admin: everything your roles allow"),
    msg("account.token_lifetime_label", "Works for"),
    msg("account.lifetime_day", "1 day"),
    msg("account.lifetime_week", "7 days"),
    msg("account.lifetime_month", "30 days"),
    msg("account.lifetime_quarter", "90 days"),
    msg("account.lifetime_year", "1 year"),
    msg("account.token_button", "Make token"),
    msg("account.token_made_heading", "Your new token"),
    msg(
        "account.token_made_body",
        "Copy it now. It is shown only this once and cannot be shown again.",
    ),
    msg(
        "account.token_made_use",
        "A program sends it in the Authorization header, as Bearer followed by the token.",
    ),
    msg(
        "account.token_too_many",
        "You already have {max} live tokens. Revoke one first.",
    ),
    msg(
        "account.token_label_long",
        "The note is too long: at most 80 characters.",
    ),
    // sign-in and accounts (the sign-in handoff U1, U2): end
    // sign-in through a provider (the OpenID Connect handoff U3): begin
    msg("sign_in.provider", "Sign in with {provider}"),
    msg(
        "sign_in.provider_joins",
        "The first time you sign in with {provider}, an account is made for you.",
    ),
    msg(
        "sign_in.provider_failed",
        "The sign-in with {provider} did not work, and nothing was changed. Try again; if it keeps happening, tell the person who runs this server.",
    ),
    msg(
        "sign_in.provider_state",
        "This sign-in did not start in this browser, or it has been used already, or it took too long. Start it again from this page.",
    ),
    msg(
        "sign_in.provider_not_linked",
        "The provider knows you, but no account here is linked to your sign-in there. Ask the person who runs this wiki for an invitation.",
    ),
    msg(
        "sign_in.provider_unreachable",
        "The provider could not be reached right now, and nothing was changed. Try again in a while.",
    ),
    msg(
        "invite.provider_intro",
        "Or, instead of a password, let your provider vouch for you. Your account is then tied to that sign-in, and there is no password to remember.",
    ),
    msg(
        "invite.provider_link",
        "Use {provider} instead of a password",
    ),
    msg(
        "invite.provider_taken",
        "Your sign-in with {provider} already has an account here, so this invitation was not used. Sign in with {provider} instead.",
    ),
    msg("title.landing", "Signing in · {product}"),
    msg(
        "landing.body",
        "Signing you in. If this page does not move on by itself, {link}.",
    ),
    msg("landing.continue", "continue to the page you asked for"),
    msg(
        "sign_in.provider_not_navigation",
        "A sign-in with a provider starts only when you open its address yourself, from this site's sign-in page. Nothing was started. Use the button on this page.",
    ),
    msg("account.token_password_label", "Your password"),
    msg(
        "account.token_password_hint",
        "Asked again because a token keeps working after you sign out.",
    ),
    msg(
        "account.token_password_wrong",
        "The password was not right, so no token was made.",
    ),
    msg(
        "account.token_provider_hint",
        "This account signs in through a provider and has no password. A token can be made only within ten minutes of signing in.",
    ),
    msg(
        "account.token_sign_in_again",
        "Your last sign-in was too long ago, so no token was made. Sign out, sign in again, and make the token within ten minutes.",
    ),
    msg("account.password_heading", "Password"),
    msg(
        "account.password_intro",
        "Changing your password ends every other session of this account and every API token it has. This browser stays signed in.",
    ),
    msg(
        "account.password_none",
        "This account has no password: it signs in through a provider.",
    ),
    msg("account.password_current_label", "Current password"),
    msg("account.password_new_label", "New password"),
    msg("account.password_button", "Change password"),
    msg(
        "account.password_wrong",
        "The current password was not right. Nothing was changed.",
    ),
    msg(
        "account.password_changed",
        "Your password was changed. Every other session and every API token of this account has ended.",
    ),
    // sign-in through a provider (the OpenID Connect handoff U3): end
    // the knowledge layer's reader (K-T6)
    msg("page.link_why", "Why is this here?"),
    msg("claim.link", "Sources and views"),
    msg("claim.link_name", "Sources and views: {paragraph}"),
    msg(
        "claim.link_name_nth",
        "Sources and views, claim {n} of {total}: {paragraph}",
    ),
    msg("claim.paragraph_start", "paragraph beginning “{text}”"),
    msg("claim.paragraph_empty", "paragraph without text"),
    msg("title.claim", "Sources and views · {wiki} · {product}"),
    msg("claim.heading", "Sources and views"),
    msg("claim.kind", "What kind of statement it is: {kind}."),
    msg("claim.kind_observation", "an observation"),
    msg("claim.kind_interpretation", "an interpretation"),
    msg("claim.kind_hypothesis", "a hypothesis"),
    msg("claim.kind_opinion", "an opinion"),
    msg("claim.kind_decision", "a decision"),
    msg("claim.kind_policy", "a policy"),
    msg("claim.kind_question", "a question"),
    msg("claim.kind_unknown", "not said"),
    msg("claim.said_heading", "Where it is said"),
    msg(
        "claim.said_nowhere",
        "No page has the paragraph this claim is said in, as the pages are now.",
    ),
    msg("claim.sources_heading", "Sources"),
    msg("claim.sources_none", "No source is attached to this claim."),
    msg("claim.source_line", "Source {id} ({kind}): {how}."),
    msg("claim.source_primary", "primary"),
    msg("claim.source_secondary", "secondary"),
    msg("claim.source_observational", "observational"),
    msg("claim.source_computational", "computational"),
    msg("claim.source_anecdotal", "anecdotal"),
    msg("claim.source_other", "another kind"),
    msg("claim.source_unreadable", "its file cannot be read"),
    msg("claim.source_line_named", "{name} ({kind}): {how}."),
    msg("claim.source_titled", "“{title}”"),
    msg("claim.source_untitled", "A source without a title"),
    msg("claim.source_by", "by {author}"),
    msg("claim.source_dated", "dated {date}"),
    msg("claim.source_join", "{first}, {second}"),
    msg("claim.source_found_at", "Where it is found: {reference}."),
    msg("claim.source_identifier", "Its identifier is {id}."),
    msg("claim.how_supports_direct", "supports, directly"),
    msg("claim.how_supports_partial", "supports, in part"),
    msg("claim.how_supports_indirect", "supports, indirectly"),
    msg("claim.how_challenges_direct", "challenges, directly"),
    msg("claim.how_challenges_partial", "challenges, in part"),
    msg("claim.how_challenges_indirect", "challenges, indirectly"),
    msg("claim.judged_earlier", "Judged on an earlier wording."),
    msg("claim.status_heading", "Status in this view"),
    msg(
        "claim.status_accepted",
        "This claim is accepted in this view.",
    ),
    msg(
        "claim.status_contested",
        "This claim is contested in this view.",
    ),
    msg(
        "claim.status_unresolved",
        "This claim is unresolved in this view.",
    ),
    msg(
        "claim.status_unknown",
        "This claim has not been decided in this view.",
    ),
    msg("claim.decided_by", "Decided by {proposal}."),
    msg("claim.decided_link", "the proposal that decided it"),
    msg(
        "claim.decided_unlisted",
        "Decided by a proposal that this wiki's records do not hold.",
    ),
    msg("claim.none_heading", "No such claim"),
    msg(
        "claim.none_body",
        "This wiki has no claim at that address, or it cannot be read now.",
    ),
    msg("why.heading", "Why is this here?"),
    msg("why.back", "Back to {page}"),
    // views: creating and comparing (K-T8)
    msg("title.views", "Views · {wiki} · {product}"),
    msg("views.heading", "Views"),
    msg(
        "views.intro",
        "A view is a named version of the whole wiki. Readers see the default view unless they choose another one.",
    ),
    msg("views.default_name", "the default view"),
    msg("views.named", "the view “{view}”"),
    msg("views.default_note", "Shown when no view is named."),
    msg("views.kind_experimental", "Experimental"),
    msg("views.kind_interpretation", "Interpretation"),
    msg(
        "views.kind_experimental_note",
        "A working state. Its changes may be taken back into the view it was forked from, or dropped.",
    ),
    msg(
        "views.kind_interpretation_note",
        "A competing understanding, kept beside the default view.",
    ),
    msg("views.forked_from", "Forked from {view}."),
    msg(
        "views.no_origin",
        "The wiki did not record where this view came from.",
    ),
    msg("views.read_link", "Read {view}"),
    msg(
        "views.compare_with_default",
        "Compare {view} with the default view",
    ),
    msg(
        "views.omitted",
        "The first {shown} views are listed here; the other {omitted} are not shown.",
    ),
    msg("views.only_default", "This wiki has only its default view."),
    msg("views.create_link", "Create a view"),
    msg("views.compare_link", "Compare two views"),
    msg("views.all_link", "All views of this wiki"),
    msg("title.views_new", "Create a view · {wiki} · {product}"),
    msg("views_new.heading", "Create a view"),
    msg(
        "views_new.intro",
        "A view is a named version of the whole wiki, forked from a view that is already there. Anyone who may read it can propose a change in it. Creating a view is recorded: who made it, when, and what it was forked from.",
    ),
    msg("views_new.name_label", "Name"),
    msg(
        "views_new.name_hint",
        "Lower-case letters, digits, dots, hyphens and underscores, starting with a letter or a digit; at most 64 characters.",
    ),
    msg("views_new.kind_legend", "What the view is for"),
    msg("views_new.from_label", "Fork it from"),
    msg("views_new.submit", "Create the view"),
    msg("views_new.problem_name_empty", "Give the view a name."),
    msg(
        "views_new.problem_name_start",
        "A name starts with a lower-case letter or a digit.",
    ),
    msg(
        "views_new.problem_name_chars",
        "A name has only lower-case letters, digits, dots, hyphens and underscores, and neither two dots in a row nor a dot at the end.",
    ),
    msg(
        "views_new.problem_name_long",
        "A name has at most 64 characters.",
    ),
    msg(
        "views_new.problem_name_retired",
        "That name cannot be given to a view: it would say that this view is better or more settled than the others. Choose a name for what the view is about.",
    ),
    msg("views_new.problem_kind", "Choose what the view is for."),
    msg(
        "views_new.problem_from",
        "Choose a view that is there to fork it from.",
    ),
    msg(
        "views_new.problem_taken",
        "A view with that name is already there. Choose another name.",
    ),
    msg(
        "views_new.not_allowed",
        "Your role in this wiki does not include creating views. Nothing was changed.",
    ),
    msg(
        "views_new.busy",
        "Another change is being made to the wiki. Nothing was changed; try again in a moment.",
    ),
    msg("views_new.too_many_heading", "Too many views"),
    msg(
        "views_new.too_many",
        "This wiki already has {bound} open views, the most it keeps. Nothing was changed.",
    ),
    msg(
        "views_new.retirement_heading",
        "Not created: a change to this wiki's keys is not finished",
    ),
    msg(
        "views_new.retirement",
        "A change to this wiki's keys is not finished, so nothing new can be made in its history until the person who runs this wiki finishes or gives up that change. Nothing was changed. Tell that person; once it is finished or given up, create the view again.",
    ),
    msg("views_new.failed_heading", "The view was not created"),
    msg(
        "views_new.failed",
        "The history did not accept the new view. Nothing was changed.",
    ),
    msg(
        "views_new.uncertain",
        "It is not known whether the view was made: the wiki's views could not be read afterwards, or the view is listed and the repository did not say that making it was finished. It may or may not be there. Look at the list of views before you try again, and tell the person who runs this server: the wiki may take no change until that person has looked at it.",
    ),
    msg(
        "views_new.not_kept_heading",
        "Creating views is not available on this server",
    ),
    msg(
        "views_new.not_kept",
        "This server does not keep the record a new view needs, so no view can be created here yet. Nothing was changed.",
    ),
    msg(
        "views_new.unconfigured_heading",
        "Creating views is not set up on this server",
    ),
    msg(
        "views_new.unconfigured",
        "This server is not set up to sign changes into the history, so no view can be created; tell the person who runs this server. Nothing was changed.",
    ),
    msg("title.compare", "Compare views · {wiki} · {product}"),
    msg("compare.heading", "Compare views"),
    msg(
        "compare.intro",
        "This lists where two views differ. It does not say which one is right.",
    ),
    msg("compare.a_label", "First view"),
    msg("compare.b_label", "Second view"),
    msg("compare.go", "Compare"),
    msg("compare.pick_two", "Choose two different views to compare."),
    msg("compare.pair", "Comparing {a} and {b}"),
    msg("compare.same", "The two views hold the same content."),
    msg("compare.claims_heading", "Claims that differ"),
    msg(
        "compare.claims_none",
        "No claim differs between the two views.",
    ),
    msg("compare.claim_said_in", "A claim said in {page}"),
    msg("compare.claim_unplaced", "A claim that no page says"),
    msg("compare.diff_wording", "It is worded differently."),
    msg(
        "compare.diff_status",
        "Its status: {a} in {va}, {b} in {vb}.",
    ),
    msg("compare.diff_sources", "Its sources differ."),
    msg("compare.only_in", "Only in {view}."),
    msg(
        "compare.unreadable_in",
        "Its file in {view} cannot be read, so nothing is said of it there.",
    ),
    msg("compare.status_accepted", "accepted"),
    msg("compare.status_contested", "contested"),
    msg("compare.status_unresolved", "unresolved"),
    msg("compare.status_unknown", "not decided"),
    msg(
        "compare.claims_omitted",
        "{shown} claims are listed; {omitted} more also differ and are not shown.",
    ),
    msg("compare.pages_heading", "Pages that differ"),
    msg(
        "compare.pages_none",
        "No page differs between the two views.",
    ),
    msg("compare.page_differs", "The page differs."),
    msg("compare.page_link", "How {page} differs"),
    msg(
        "compare.pages_omitted",
        "{shown} pages are listed; {omitted} more also differ and are not shown.",
    ),
    msg("compare.back", "All views"),
    msg(
        "title.compare_page",
        "{page}: how it differs · {wiki} · {product}",
    ),
    msg(
        "compare.page_heading",
        "{title}: how it differs between two views",
    ),
    msg(
        "compare.page_range",
        "Lines marked removed are in {a} only; lines marked added are in {b} only.",
    ),
    msg("compare.page_read", "Read {page} in {view}"),
    msg("compare.page_back", "Back to the comparison"),
    msg("compare.page_absent", "Neither view has this page."),
    msg("claim.others_heading", "Other views"),
    msg(
        "claim.others_none",
        "No other view reads this claim differently.",
    ),
    msg("claim.others_compare", "Compare with {view}"),
    msg(
        "claim.others_limit",
        "Only {shown} other views are checked on this page; compare views to see the others.",
    ),
    msg(
        "claim.others_unchecked",
        "Some views could not be read just now, so they are not listed here.",
    ),
    // views: creating and comparing (K-T8): end
    // the knowledge layer's reader (K-T6): end
    // Why is this here? (K-T7)
    msg("title.why", "Why is this here? {page} · {wiki} · {product}"),
    msg(
        "why.intro",
        "Where each paragraph of {page} came from, as far as the page's history says. Every paragraph here can be linked to.",
    ),
    msg("why.history_link", "The whole history of {page}"),
    plural_msg(
        "why.bounded",
        "Only the latest {count} change to this page was read here. There are older changes, and what came before is not shown.",
        "Only the latest {count} changes to this page were read here. There are older changes, and what came before them is not shown.",
    ),
    msg(
        "why.bounded_none",
        "No change to this page was found among the latest changes of this wiki that one request can read. There are older changes, and they are not shown.",
    ),
    msg(
        "why.spent_none",
        "The work one request may spend on this page was used up before any change to it could be read. There are changes, and they are not shown.",
    ),
    msg(
        "why.unfollowed",
        "The history of this page could not be followed to its start: an earlier version could not be read, or the history lists no change that wrote the page. What is said below holds only for the changes that were read.",
    ),
    plural_msg(
        "why.left_out",
        "{count} more paragraph is not followed: only the first {limit} paragraphs of a page are.",
        "{count} more paragraphs are not followed: only the first {limit} paragraphs of a page are.",
    ),
    msg("why.paragraphs_heading", "Paragraphs"),
    msg("why.no_paragraphs", "This page has no paragraphs."),
    msg("why.paragraph_label", "Paragraph {n}"),
    msg(
        "why.written_unchanged",
        "Written in {change}. Not changed since.",
    ),
    msg("why.last_changed", "Last changed in {change}."),
    msg("why.first_written", "First written in {change}."),
    msg(
        "why.first_before",
        "First written before the oldest change read here.",
    ),
    msg(
        "why.unchanged_read",
        "Not changed in the changes read here.",
    ),
    msg(
        "why.found_written_unchanged",
        "Earliest wording found in {change}. No change to it was found since.",
    ),
    msg("why.found_last", "Latest change found in {change}."),
    msg(
        "why.found_first",
        "Earliest wording found in {change}. No earlier wording matches it.",
    ),
    msg(
        "why.basis_anchor",
        "Followed through its anchor: in every change read, the one paragraph with this anchor is this paragraph.",
    ),
    msg(
        "why.basis_text",
        "Traced by matching the text. Nothing carries this paragraph through every change but its words, so each earlier wording was found by comparing text. If several paragraphs were reworded at once, it may be a different paragraph.",
    ),
    msg("why.changes_heading", "Changes read"),
    msg("why.changes_none", "No change to this page was read."),
    msg("why.change_open", "See change {number} in the history"),
    msg("why.proposal", "Made from the proposal {proposal}."),
    msg(
        "why.proposal_named",
        "The message of this change names the proposal {proposal}.",
    ),
    msg(
        "why.proposal_unlisted",
        "The message of this change names a proposal that is not among the records of this wiki that you may read.",
    ),
    msg(
        "why.proposal_unconfirmed",
        "This wiki's records do not show that this proposal was accepted as this change, so its review is not shown here.",
    ),
    msg("why.review_heading", "Review"),
    msg(
        "why.review_none",
        "Nobody commented or decided on this proposal.",
    ),
    msg("why.decision_words", "They wrote: “{text}”"),
    msg(
        "why.comment",
        "{author} commented, on revision {number}: “{text}”",
    ),
    plural_msg(
        "why.review_more",
        "{count} more item of the review is on the proposal's page.",
        "{count} more items of the review are on the proposal's page.",
    ),
    msg("why.sources_heading", "Sources given with the proposal"),
    msg("why.sources_none", "No source was given with the proposal."),
    // Why is this here? (K-T7): end
    // knowledge: claims and sources in the proposal path (handoff 005 K-T5): start
    msg("parts.attach", "Attach a source"),
    msg("parts.challenge", "Challenge this"),
    msg("parts.attach_for", "Attach a source to part {number}"),
    msg("parts.challenge_for", "Challenge this: part {number}"),
    msg(
        "title.attach",
        "Attach a source to {page} · {wiki} · {product}",
    ),
    msg(
        "title.challenge",
        "Challenge a part of {page} · {wiki} · {product}",
    ),
    msg("attach.heading_supports", "Attach a source to {page}"),
    msg("attach.heading_challenges", "Challenge a part of {page}"),
    msg(
        "attach.intro_supports",
        "A source is what a statement rests on: a book, an article, a record, a person's account. Attaching one records this paragraph as a claim, so that others can see what it rests on. You can also change the words of the paragraph; the change and the source go in one proposal.",
    ),
    msg(
        "attach.intro_challenges",
        "A challenging source speaks against a statement, or against part of it. Adding one records this paragraph as a claim and shows what is said against it. You can also change the words of the paragraph; the change and the source go in one proposal.",
    ),
    msg(
        "attach.claim_exists",
        "This paragraph is a claim already. The source is added to it.",
    ),
    msg("attach.legend", "The source"),
    msg("attach.label_kind", "What kind of source it is"),
    msg("attach.kind_primary", "A first-hand source"),
    msg("attach.kind_secondary", "A source about first-hand sources"),
    msg("attach.kind_observational", "A record of an observation"),
    msg("attach.kind_computational", "The result of a computation"),
    msg(
        "attach.kind_anecdotal",
        "A person's account of what they saw or heard",
    ),
    msg("attach.kind_other", "Another kind"),
    msg("attach.label_title", "Title"),
    msg("attach.label_author", "Author or publisher"),
    msg("attach.label_date", "Date, as written"),
    msg(
        "attach.hint_date",
        "A year, a season or a day, as the source gives it.",
    ),
    msg("attach.label_ref_form", "How you refer to it"),
    msg("attach.ref_url", "A web address"),
    msg("attach.ref_doi", "A DOI"),
    msg("attach.ref_isbn", "An ISBN"),
    msg("attach.ref_page", "A page in this wiki"),
    msg("attach.ref_citation", "A citation in words"),
    msg("attach.label_ref", "The reference"),
    msg(
        "attach.hint_ref",
        "Leave it empty if the title says enough. A web address starts with https or http and has no spaces in it.",
    ),
    msg(
        "attach.label_bearing",
        "How directly the source speaks to this paragraph",
    ),
    msg("attach.bearing_direct", "It speaks to the whole of it"),
    msg("attach.bearing_partial", "It speaks to part of it"),
    msg(
        "attach.bearing_indirect",
        "It speaks to it only by implication",
    ),
    msg("attach.label_note", "A note (optional)"),
    msg(
        "attach.hint_note",
        "Where to look in the source, or what it says.",
    ),
    msg(
        "attach.label_claim_kind",
        "What kind of statement this paragraph is",
    ),
    msg(
        "attach.claim_kind_hint",
        "Say what you can. “Not said” is fine.",
    ),
    msg("attach.claim_kind_observation", "An observation"),
    msg("attach.claim_kind_interpretation", "An interpretation"),
    msg("attach.claim_kind_hypothesis", "A hypothesis"),
    msg("attach.claim_kind_opinion", "An opinion"),
    msg("attach.claim_kind_decision", "A decision"),
    msg("attach.claim_kind_policy", "A policy"),
    msg("attach.claim_kind_question", "A question"),
    msg("attach.claim_kind_unknown", "Not said"),
    msg(
        "attach.no_paragraph",
        "The part you edited has no paragraph a source can be attached to.",
    ),
    msg(
        "attach.page_unreadable",
        "The information at the top of this page cannot be read, so the page cannot be given the identifier a claim needs.",
    ),
    msg(
        "attach.several_claims",
        "More than one claim is recorded for this paragraph, and this form cannot tell which the source is for.",
    ),
    msg(
        "attach.full",
        "This claim already has as many sources as one claim may have.",
    ),
    msg(
        "attach.says_nothing",
        "A source needs a title or a reference.",
    ),
    msg(
        "attach.too_long",
        "A text in the source is longer than {limit} characters.",
    ),
    msg(
        "attach.control",
        "A text in the source holds a character that cannot be kept, such as a line break.",
    ),
    msg(
        "attach.not_web",
        "A web address has to start with https or http and have no spaces in it.",
    ),
    msg(
        "attach.unwritable",
        "The source could not be written down. Nothing was changed; try again.",
    ),
    msg(
        "curate.not_a_day",
        "A date has to be written year, month, day, like 2026-11-04.",
    ),
    msg(
        "curate.unknown_source",
        "The claim does not cite one of the sources in the form. Open the form again.",
    ),
    msg(
        "curate.no_wording",
        "The claim is not said in any paragraph now, so there is no wording to judge a source or a status on.",
    ),
    msg(
        "propose.claim_held_one",
        "Others rely on this paragraph: sources are attached to it. Its marker is kept for you, so just edit the words.",
    ),
    msg(
        "propose.claim_held_many",
        "Some paragraphs here have sources attached, and others rely on them. Their markers are kept for you as long as you leave their words as they are. To reword one of them, change that part on its own.",
    ),
    msg(
        "propose.claim_lost",
        "The paragraph that begins “{excerpt}” has sources attached, and this change leaves it out. To reword it, change that part on its own. To remove it, tick the box below.",
    ),
    msg("propose.claim_drop_legend", "Paragraphs you are removing"),
    msg(
        "propose.claim_drop",
        "Remove “{excerpt}”. It stops being where its claim is said, and the reviewer will see that.",
    ),
    msg("know.heading", "Claims and sources in this proposal"),
    plural_msg(
        "know.claim_made",
        "Records the claim {claim} ({kind}), said in {count} place. Its community status: {standing}.",
        "Records the claim {claim} ({kind}), said in {count} places. Its community status: {standing}.",
    ),
    msg(
        "know.cite",
        "Cites the source {source}: it {relation} the claim {bearing}, judged on the wording it has in this proposal's result.",
    ),
    msg(
        "know.cite_unjudged",
        "Cites the source {source}: it {relation} the claim {bearing}, judged on a wording the claim does not have in this proposal's result.",
    ),
    msg(
        "know.source_recorded",
        "Records the source {source} ({kind}).",
    ),
    msg("know.claim_changed", "Changes the claim {claim}:"),
    msg("know.source_changed", "Changes the source {source}:"),
    msg("know.removed_claim", "Removes the claim {claim}."),
    msg("know.removed_source", "Removes the source {source}."),
    msg(
        "know.unreadable",
        "The file {file} in this proposal is not in the form the wiki reads. {reason}",
    ),
    msg("know.change_kind", "The kind of statement becomes {kind}."),
    msg(
        "know.change_lifecycle",
        "Where it stands in its life becomes: {lifecycle}.",
    ),
    msg(
        "know.change_standing",
        "Its community status changes from {from} to {to}.",
    ),
    msg(
        "know.change_said",
        "It is said in one more paragraph: {paragraph}.",
    ),
    msg(
        "know.change_unsaid",
        "It is no longer said in the paragraph {paragraph}: the paragraph is removed, or the claim lets it go.",
    ),
    msg(
        "know.change_related",
        "A relation to the claim {other} is added: {relation}.",
    ),
    msg(
        "know.change_unrelated",
        "A relation to the claim {other} is removed: {relation}.",
    ),
    msg(
        "know.change_uncited",
        "It no longer cites the source {source}.",
    ),
    msg(
        "know.change_recited",
        "The source {source}, which it cites, now {relation} it {bearing}.",
    ),
    msg(
        "know.change_noted",
        "The note on the source {source}, which it cites, changes.",
    ),
    msg("know.source_change_kind", "Its kind becomes {kind}."),
    msg(
        "know.source_change_identity",
        "Who or what it is, as written, becomes {identity}.",
    ),
    msg(
        "know.source_change_reference",
        "Where it is found becomes ({form}): {reference}.",
    ),
    msg(
        "know.source_change_snapshot",
        "The copy kept of it changes.",
    ),
    msg("know.source_change_reviewed", "Last checked on {day}."),
    msg(
        "know.source_change_unreviewed",
        "It is no longer marked as checked.",
    ),
    msg(
        "know.source_change_availability",
        "Whether it can be had becomes: {availability}.",
    ),
    msg(
        "know.source_change_superseded",
        "It is now replaced by the source {source}.",
    ),
    msg(
        "know.is_curation",
        "This is a curation proposal: it changes claims and sources and no page.",
    ),
    msg(
        "know.not_curation",
        "This proposal sets a community status, a review date or an availability, changes or removes a source link a claim already has, changes a source that was checked in its title, author, date, reference or the source that replaces it, removes a claim or a source that holds any of these, or replaces or removes a file that cannot be read, which only a curation proposal may do. It cannot be accepted as it is.",
    ),
    msg(
        "know.not_judged",
        "This proposal sets or changes a judgement on a claim (a source link's or the community status), judged on a wording that no paragraph of that claim has once the proposal is made. It cannot be accepted as it is.",
    ),
    msg("know.kind_observation", "an observation"),
    msg("know.kind_interpretation", "an interpretation"),
    msg("know.kind_hypothesis", "a hypothesis"),
    msg("know.kind_opinion", "an opinion"),
    msg("know.kind_decision", "a decision"),
    msg("know.kind_policy", "a policy"),
    msg("know.kind_question", "a question"),
    msg("know.kind_unknown", "kind not said"),
    msg("know.source_primary", "first-hand"),
    msg("know.source_secondary", "about first-hand sources"),
    msg("know.source_observational", "a record of an observation"),
    msg("know.source_computational", "the result of a computation"),
    msg("know.source_anecdotal", "a person's account"),
    msg("know.source_other", "of another kind"),
    msg("know.supports", "supports"),
    msg("know.challenges", "challenges"),
    msg("know.directly", "directly"),
    msg("know.in_part", "in part"),
    msg("know.by_implication", "by implication"),
    msg("know.life_proposed", "put forward"),
    msg("know.life_adopted", "taken up"),
    msg("know.life_retired", "retired"),
    msg("know.life_unknown", "not said"),
    msg("know.standing_accepted", "accepted"),
    msg("know.standing_contested", "contested"),
    msg("know.standing_unresolved", "unresolved"),
    msg("know.standing_unknown", "not decided"),
    msg("know.available", "it can be had"),
    msg("know.unavailable", "it cannot be had"),
    msg("know.availability_unknown", "not known"),
    msg("know.rel_refines", "refines"),
    msg("know.rel_supersedes", "replaces"),
    msg("know.rel_contradicts", "is at odds with"),
    msg("know.rel_split_from", "came out of"),
    msg("know.rel_merged_into", "was joined into"),
    // knowledge: claims and sources in the proposal path (handoff 005 K-T5): end
    // knowledge: the curation proposal (handoff 005 K-T5): start
    msg("title.curate", "Curate a claim · {wiki} · {product}"),
    msg("curate.heading", "Curate a claim"),
    msg(
        "curate.intro",
        "A curation proposal changes how a claim stands, how its sources bear on it, and when its sources were last checked. It changes no page. Nothing changes until someone who is allowed to takes it into the history; a community status is set this way and no other.",
    ),
    msg("curate.wording", "It is said as: “{words}”"),
    msg(
        "curate.wording_none",
        "It is not said in any paragraph now.",
    ),
    msg("curate.label_status", "Community status"),
    msg(
        "curate.status_hint",
        "What the community has decided about this claim. It is set by this proposal, once someone accepts it.",
    ),
    msg(
        "curate.earlier",
        "This was judged on an earlier wording of the claim.",
    ),
    msg("curate.source_legend", "Source: {title}"),
    msg("curate.source_untitled", "a source without a title"),
    msg("curate.label_relation", "How it relates to the claim"),
    msg(
        "curate.label_bearing",
        "How directly it speaks to the claim",
    ),
    msg("curate.label_reviewed", "Last checked on (year-month-day)"),
    msg(
        "curate.reviewed_hint",
        "Set by a person who checked it. Leave it empty if nobody has.",
    ),
    msg("curate.label_availability", "Whether the source can be had"),
    msg(
        "curate.availability_hint",
        "As the person who checked says. Nothing checks it for you.",
    ),
    msg(
        "curate.confirm",
        "Judge the sources and the status again on the wording the claim has now",
    ),
    msg(
        "curate.confirm_hint",
        "Use this when the claim was reworded and you have read its sources against the new wording.",
    ),
    msg("curate.submit", "Send for review"),
    msg(
        "curate.nothing",
        "Nothing in the form differs from what the wiki has now. Change something first.",
    ),
    // knowledge: the curation proposal (handoff 005 K-T5): end
    // knowledge: the claims a change touches (handoff 005 K-T5): start
    msg("touch.heading", "Claims this change touches"),
    msg(
        "touch.intro",
        "These claims are said in paragraphs this change rewords or removes. The wiki shows what it can see; it does not say whether a reworded paragraph still means the same.",
    ),
    msg(
        "touch.reworded",
        "The paragraph “{words}” is reworded. It stays the same claim, because its marker stays.",
    ),
    msg(
        "touch.removed",
        "The paragraph “{words}” is removed. The claim is no longer said there.",
    ),
    plural_msg(
        "touch.links_earlier",
        "{count} source was judged on the earlier wording. It will be shown as judged on an earlier wording.",
        "{count} sources were judged on the earlier wording. They will be shown as judged on an earlier wording.",
    ),
    msg(
        "touch.standing_earlier",
        "Its community status ({status}) was decided on the earlier wording. It will be shown as judged on an earlier wording.",
    ),
    msg(
        "touch.nothing_earlier",
        "Nothing was judged on the earlier wording.",
    ),
    msg(
        "touch.reconfirm_hint",
        "One proposal for all of them: it judges their sources, and their status where one was decided, on the wording they have now. It changes no page, and someone has to accept it.",
    ),
    msg("touch.reconfirm_button", "Propose judging them again"),
    msg(
        "touch.reconfirm_later",
        "Once this proposal is accepted, a person can propose judging these claims again here, in one step.",
    ),
    msg(
        "touch.reconfirm_summary",
        "Judge the claims again after: {summary}",
    ),
    msg(
        "touch.nothing_to_judge",
        "Nothing needs judging again: each of these claims is judged on the wording it has now.",
    ),
    msg(
        "touch.reconfirm_open",
        "Judging these claims again is already proposed: {proposal}. That proposal is still open.",
    ),
    msg(
        "touch.reconfirm_done",
        "Each of these claims was judged again on the wording it has now, by a proposal that was accepted.",
    ),
    msg(
        "touch.reconfirm_of",
        "This proposal judges again the claims that the change made by {proposal} left judged on an earlier wording.",
    ),
    // knowledge: the claims a change touches (handoff 005 K-T5): end
    msg(
        "invalid.knowledge_not_shown",
        "A change to a claim or a source in this proposal is one this page would not describe, so nobody reviewing it could see what it does. It is not sent.",
    ),
    msg(
        "invalid.knowledge_unjudged",
        "A source link this proposal adds to a claim or changes is judged on a wording that no paragraph of that claim has once the proposal is made. It is not sent.",
    ),
    msg(
        "invalid.knowledge_status_unjudged",
        "The community status this proposal sets on a claim is judged on a wording that no paragraph of that claim has once the proposal is made. It is not sent.",
    ),
    msg(
        "know.change_rejudged",
        "What {subject} was judged on becomes this wording: “{words}”",
    ),
    msg(
        "know.change_rejudged_unseen",
        "What {subject} was judged on becomes a wording that none of the claim's paragraphs has once this proposal is made.",
    ),
    msg(
        "know.change_rejudged_cleared",
        "It is no longer recorded which wording {subject} was judged on.",
    ),
    msg("know.rejudged_status", "its community status"),
    msg("know.rejudged_source", "the source {source} that it cites"),
    msg(
        "know.nothing_changes",
        "The claim and source files in this proposal are the files the wiki already has: they change nothing.",
    ),
    // knowledge: re-judgements shown (handoff 005 corrections K5): end
    msg(
        "attach.direction",
        "A text in the source holds a character that changes the direction of text or is not shown, which cannot be kept.",
    ),
    // knowledge: direction characters refused (handoff 005 corrections K14): end
    msg(
        "know.replaced_unreadable_claim",
        "Replaces or removes a claim file that cannot be read ({id}). What it held is not known.",
    ),
    msg(
        "know.replaced_unreadable_source",
        "Replaces or removes a source file that cannot be read ({id}). What it held is not known.",
    ),
    msg(
        "know.made_lifecycle",
        "Where it stands in its life: {lifecycle}.",
    ),
    msg(
        "know.made_related",
        "It has a relation to the claim {other}: {relation}.",
    ),
    msg(
        "know.source_made_availability",
        "Whether it can be had: {availability}.",
    ),
    msg(
        "know.source_where",
        "Where it is found ({form}): {reference}.",
    ),
    msg("know.source_copy", "A copy of it is kept."),
    msg(
        "know.source_change_unsuperseded",
        "It is no longer marked as replaced.",
    ),
    msg(
        "know.source_change_reference_cleared",
        "It no longer says where it is found.",
    ),
    msg(
        "know.source_nameless",
        "A source without a title, author or date",
    ),
    // knowledge: a file that cannot be read is replaced (handoff 005 corrections, second round K16): end
    msg("know.subject_claim", "{id}"),
    msg("know.subject_claim_id", "“{excerpt}” [claim {id}]"),
    msg("know.subject_source_id", "{name} [source {id}]"),
    msg("know.subject_words", "“{words}”"),
    msg("know.subject_paragraph", "“{words}” on the page {page}"),
    msg(
        "know.subject_paragraph_block",
        "“{words}” (the paragraph marked {block}) on the page {page}",
    ),
    // knowledge: subjects told apart (handoff 005 corrections, third round K24): end
    msg(
        "know.removed_had_standing",
        "Its community status goes with it: {standing}.",
    ),
    msg(
        "know.removed_had_link",
        "Its link to the source {source} goes with it: it {relation} the claim {bearing}.",
    ),
    msg(
        "know.removed_had_reviewed",
        "The day it was last checked goes with it: {day}.",
    ),
    msg(
        "know.removed_had_availability",
        "What is recorded of whether it can be had goes with it: {availability}.",
    ),
    // knowledge: what leaves with a removal (handoff 005 corrections, third round K27): end
    msg("health.sources_name", "Sources to review"),
    msg(
        "health.sources_desc",
        "Sources that were last reviewed long ago or never, and sources a person marked as unavailable, each with the claims that cite it. The wiki checks nothing and fetches nothing: a person looks at the source and says so in a proposal.",
    ),
    msg("health.col_source", "Source"),
    msg("health.col_why", "Why it is listed"),
    msg("health.col_claims", "Claims that cite it"),
    msg(
        "health.sources_action",
        "Look at the source, then propose the day you looked and whether it can be had.",
    ),
    msg("health.none_sources", "No source needs a look."),
    msg(
        "health.sources_unavailable",
        "A person who checked marked it as unavailable.",
    ),
    msg("health.sources_never", "No one has recorded looking at it."),
    msg("health.sources_last", "Last reviewed on {day}."),
    msg(
        "health.sources_overdue",
        "Last reviewed on {day}, more than {limit} days ago.",
    ),
    plural_msg(
        "health.sources_claims",
        "{count} claim cites it:",
        "{count} claims cite it:",
    ),
    msg("health.sources_cut", "The first {limit} are named."),
    msg("health.sources_no_claims", "No claim cites it."),
    msg("health.sources_claim", "{where} ({how})"),
    msg(
        "health.sources_claim_nowhere",
        "A claim said on no page now, identifier {id}",
    ),
    msg(
        "health.sources_setting",
        "A source is listed when it was last reviewed more than {days} days ago. The person who runs this server sets the number.",
    ),
    msg(
        "health.sources_future",
        "Recorded as reviewed on {day}, a day that has not come.",
    ),
    msg(
        "health.sources_showing",
        "Showing sources {from} to {to} of {total}.",
    ),
    msg("health.sources_more", "Show the next sources"),
    msg("health.sources_first", "Show the first sources"),
    msg(
        "know.curate_link",
        "Propose a change to its status or sources",
    ),
    msg(
        "know.curate_link_of",
        "Propose a change to the status or sources of the claim {claim}",
    ),
    msg("claim_thread.heading", "Discussion of this claim"),
    msg(
        "claim_thread.never_changes",
        "A discussion never changes a claim: only a proposal that people accept does.",
    ),
    plural_msg("claim_thread.total", "{count} comment", "{count} comments"),
    msg("claim_thread.none", "Nobody has discussed this claim yet."),
    msg("claim_thread.meta", "Comment {number} · {when}"),
    msg("claim_thread.in_reply_to", "In reply to comment {number}."),
    msg("claim_thread.more", "Show the next comments"),
    msg("claim_thread.first", "Show the first comments"),
    msg(
        "claim_thread.unavailable",
        "The discussion of this claim could not be read now.",
    ),
    msg(
        "claim_thread.form_label",
        "Add a comment to this discussion",
    ),
    msg("claim_thread.form_reply", "Replying to comment {number}"),
    msg("claim_thread.back", "Back to the claim"),
];
