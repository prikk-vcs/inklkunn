//! **The list of surfaces** (RFC 006 D-8; INV-13; threat model C-15d): every place inklkunn shows a revision's content,
//! summary or author, or a comment's body or author, as **one table**. Every surface's test iterates it, and a gate fails
//! when a route that is neither in it nor in [`SHOWS_NONE`] exists, the way the route table is held to its shape.
//!
//! A row says which parts of a hidden item the surface can show, the routes and the API members that are the surface, and the
//! test that proves it shows the tombstone and none of the hidden text. **A new surface is a new row, and a row without its
//! test fails a gate**; a route that shows revision text and has no row fails one too.
//!
//! The list is data: a surface's own code does not consult it. What keeps the code honest is the tests (they hide each part of
//! each item and look for the hidden text in every byte of every answer), and the gate that makes sure none is missing.
//!
//! **A row also says what the surface does when the text it needs of a block cannot be checked yet** (correction L-11: no change
//! that wrote it is known, so none of it is given and **no tombstone is shown**, because no hide is the reason). That is
//! [`Unchecked`], one of three things, and a surface that can show a text is never the third: it answers *try again*
//! (`503` and `Retry-After`; the API's `unavailable` with `reason` `not-checked-yet`), or leaves out what needs the text and serves
//! the rest, or reads no text of a block that is not a tip.

/// Which parts of a hidden item a surface can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shows {
    /// A revision's text, or a comment's body.
    pub content: bool,
    /// A revision's summary (the proposal's message, as shown).
    pub summary: bool,
    /// A revision's or a comment's author.
    pub author: bool,
}

impl Shows {
    const fn new(content: bool, summary: bool, author: bool) -> Shows {
        Shows {
            content,
            summary,
            author,
        }
    }

    /// Does it show any part?
    pub const fn any(self) -> bool {
        self.content || self.summary || self.author
    }
}

/// What a surface does when the text it needs of a block **cannot be checked yet** (correction L-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unchecked {
    /// The whole answer is *this text cannot be checked yet; try again in a little while*: `503` with `Retry-After`, and in the API
    /// `unavailable` with the `reason` `not-checked-yet`. No text and no tombstone. The test, by function name, in
    /// `crates/inklkunn-web/tests/tombstones/unchecked.rs`.
    TryAgain(&'static str),
    /// What needs the text is left out, and the rest of the answer is served (nothing of the text, no tombstone, and the page says
    /// that something was left out where it counts). The test, as above.
    LeavesOut(&'static str),
    /// The surface reads **no text of a block that is not a tip** (the reason), so the outcome cannot reach it. Never for a surface
    /// that shows the text of a page or a file at a block (a comment's body is not one).
    NoBlockText(&'static str),
}

/// One surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    /// A stable name, which the test uses to find the surface's addresses.
    pub id: &'static str,
    /// What of a hidden item it can show.
    pub shows: Shows,
    /// The HTML routes, as the route table writes them. A route may belong to two surfaces when one page shows both.
    pub routes: &'static [&'static str],
    /// The API members, as the route table writes them.
    pub api: &'static [&'static str],
    /// The test that shows the tombstone on this surface in place of what is hidden, by function name, in
    /// `crates/inklkunn-web/tests/tombstones/surfaces.rs`.
    pub test: &'static str,
    /// What it does when the text it needs of a block cannot be checked yet.
    pub unchecked: Unchecked,
}

/// Every surface that shows a revision's content, summary or author, or a comment's body or author.
pub const SURFACES: &[Surface] = &[
    Surface {
        id: "history-row",
        shows: Shows::new(false, true, true),
        routes: &["/{slug}/-/history/{*title}"],
        api: &["/{slug}/-/api/v1/history/{*title}"],
        test: "surface_history_row",
        unchecked: Unchecked::NoBlockText(
            "a row of the log names a change by its summary and its author; no text of any block is read",
        ),
    },
    Surface {
        id: "change-view",
        shows: Shows::new(false, true, true),
        routes: &["/{slug}/-/change/{block}"],
        api: &["/{slug}/-/api/v1/revisions/{id}"],
        test: "surface_change_view",
        unchecked: Unchecked::TryAgain("unchecked_change_view"),
    },
    Surface {
        id: "diff",
        shows: Shows::new(true, false, false),
        routes: &["/{slug}/-/diff/{from}/{to}/{*title}"],
        api: &["/{slug}/-/api/v1/diff"],
        test: "surface_diff",
        unchecked: Unchecked::TryAgain("unchecked_diff"),
    },
    Surface {
        // The page as of the revision, its permalink: one address. Its raw source is the tip's, which cannot be hidden, and
        // the API gives the page as of a revision as a member of the change (`revisions/{id}?title=`).
        id: "page-as-of",
        shows: Shows::new(true, true, true),
        routes: &["/{slug}/-/at/{block}/{*title}"],
        api: &[],
        test: "surface_page_as_of",
        unchecked: Unchecked::TryAgain("unchecked_page_as_of"),
    },
    Surface {
        id: "why-is-this-here",
        shows: Shows::new(false, true, true),
        routes: &["/{slug}/-/why/{*title}"],
        api: &["/{slug}/-/api/v1/provenance/{*title}"],
        test: "surface_why_is_this_here",
        unchecked: Unchecked::LeavesOut("unchecked_why_is_this_here"),
    },
    Surface {
        // Two views' tips against each other. A tip's content cannot be hidden, in any view, so what it can show of a hidden
        // item is nothing: the test shows that a change hidden elsewhere is not among what it shows, and that the tip of
        // either view refuses to be hidden.
        id: "compare-views",
        shows: Shows::new(false, false, false),
        routes: &[
            "/{slug}/-/views/compare",
            "/{slug}/-/views/compare/{*title}",
        ],
        api: &["/{slug}/-/api/v1/views/compare"],
        test: "surface_compare_views",
        unchecked: Unchecked::NoBlockText(
            "the difference of two views' tips, whose content cannot be hidden and is read as a tip",
        ),
    },
    Surface {
        id: "recent-changes",
        shows: Shows::new(false, true, true),
        routes: &["/{slug}/-/changes", "/{slug}/-/changes.atom"],
        api: &["/{slug}/-/api/v1/changes"],
        test: "surface_recent_changes",
        unchecked: Unchecked::NoBlockText(
            "a list of changes by summary and author; no text of any block is read",
        ),
    },
    Surface {
        id: "proposal-page",
        shows: Shows::new(true, true, true),
        routes: &[
            "/{slug}/-/proposals/{id}",
            "/{slug}/-/proposals/{id}/edit",
            "/{slug}/-/proposals/{id}/comment",
            "/{slug}/-/proposals/{id}/decide",
            "/{slug}/-/proposals/{id}/accept",
            "/{slug}/-/proposals/{id}/reconfirm",
            "/{slug}/-/proposals/{id}/decline",
            "/{slug}/-/proposals/{id}/ask",
            "/{slug}/-/proposals/{id}/withdraw",
        ],
        api: &[
            "/{slug}/-/api/v1/proposals",
            "/{slug}/-/api/v1/proposals/{id}",
            "/{slug}/-/api/v1/proposals/{id}/draft",
            "/{slug}/-/api/v1/proposals/{id}/submission",
            "/{slug}/-/api/v1/proposals/{id}/revisions",
            "/{slug}/-/api/v1/proposals/{id}/comments",
            "/{slug}/-/api/v1/proposals/{id}/decisions",
            "/{slug}/-/api/v1/proposals/{id}/acceptances",
            "/{slug}/-/api/v1/proposals/{id}/declines",
            "/{slug}/-/api/v1/proposals/{id}/withdrawals",
        ],
        test: "surface_proposal_page",
        unchecked: Unchecked::TryAgain("unchecked_proposal_page"),
    },
    Surface {
        id: "proposal-comments",
        shows: Shows::new(true, false, true),
        routes: &["/{slug}/-/proposals/{id}"],
        api: &[],
        test: "surface_proposal_comments",
        unchecked: Unchecked::NoBlockText(
            "a comment's body and author; no text of a block is read",
        ),
    },
    Surface {
        id: "proposal-list",
        shows: Shows::new(false, true, true),
        routes: &["/{slug}/-/proposals", "/{slug}/-/discussion/{*title}"],
        api: &[],
        test: "surface_proposal_list",
        unchecked: Unchecked::NoBlockText(
            "a list of proposals by summary and author; no text of a block is read",
        ),
    },
    Surface {
        id: "claim-thread",
        shows: Shows::new(true, false, true),
        routes: &["/{slug}/-/claim/{id}", "/{slug}/-/claim/{id}/discuss"],
        // The API gives a claim's file and its links, not its thread: the thread has no API member.
        api: &[],
        test: "surface_claim_thread",
        unchecked: Unchecked::NoBlockText(
            "a claim's thread: comments only; no text of a block is read",
        ),
    },
    Surface {
        // A title that leads to no page may be a page that was removed: the answer is `410 Gone` and says by which change and
        // why, the removing change's summary (G-T6). The HTML page, the raw source on the content origin (a fixed sentence: it
        // reads none of the change) and the API's `page-gone` problem are the same fact asked three ways; the page at the tip
        // is the part that cannot be hidden, and is not what this row is about. The author is never named by the answer.
        id: "removed-page",
        shows: Shows::new(false, true, false),
        routes: &["/{slug}/{*title}", "/{slug}/-/raw/{*title}"],
        api: &[
            "/{slug}/-/api/v1/pages/{*title}",
            "/{slug}/-/api/v1/raw/{*title}",
            "/{slug}/-/api/v1/fragment/{*title}",
        ],
        test: "surface_removed_page",
        unchecked: Unchecked::NoBlockText(
            "the removing change's summary from the log; the text before the removal is the restore offer's",
        ),
    },
    Surface {
        // The offer to restore a removed page: a rollback of the removing change or a draft with the page's last text. It copies
        // the text that would come back (never from a revision whose content is hidden) and the removal's summary (the rollback
        // draft's own, which is the masked one).
        id: "restore-offer",
        shows: Shows::new(true, true, false),
        routes: &["/{slug}/-/restore/{*title}"],
        api: &[],
        test: "surface_restore_offer",
        unchecked: Unchecked::TryAgain("unchecked_restore_offer"),
    },
    Surface {
        id: "inbox",
        shows: Shows::new(false, true, true),
        routes: &["/-/inbox"],
        api: &[],
        test: "surface_inbox",
        unchecked: Unchecked::NoBlockText(
            "a list of items by summary and author; no text of a block is read",
        ),
    },
    Surface {
        // The page of the rules names, for each change of them, the proposal that made it by its summary (RFC 006 D-11, G-T7):
        // a hidden summary is the tombstone's words there, and no author is named. The sentences of what a change did are the
        // rules' own, said from the files; the API's members of the rules name no proposal.
        id: "rules-history",
        shows: Shows::new(false, true, false),
        routes: &["/{slug}/-/policy"],
        api: &[],
        test: "surface_rules_history",
        unchecked: Unchecked::LeavesOut("unchecked_rules_history"),
    },
];

/// The routes and API members that show **no** revision text, summary or author, each with the reason: a route is here only
/// when it reads the tip (whose content cannot be hidden, and whose summary and author the surfaces above replace where the
/// page names its last change), names no proposal and no comment, or writes and answers with a surface above.
pub const SHOWS_NONE: &[(&str, &str)] = &[
    // The tip's own pages: the content of a view's tip cannot be hidden (D-8). (The page route and the raw route also answer
    // for a title that is no page, `410` with the removal's words: they are the surface `removed-page`.)
    (
        "/{slug}",
        "the wiki's front page: a page at the tip, and the wiki's own facts",
    ),
    ("/{slug}/-/search", "searches the tip's text only"),
    (
        "/{slug}/-/opensearch.xml",
        "the description of the search: no wiki text",
    ),
    ("/{slug}/-/links/{*title}", "the link graph of the tip"),
    (
        "/{slug}/-/health",
        "the health lists of the tip: titles and counts",
    ),
    (
        "/{slug}/-/health/{list}",
        "the health lists of the tip: titles and counts",
    ),
    (
        "/{slug}/-/site.json",
        "the wiki's description: no revision and no comment",
    ),
    ("/{slug}/-/describe/{*title}", "the tip page's description"),
    (
        "/{slug}/-/sitemap.xml",
        "the titles of the tip and the times a change was first seen",
    ),
    ("/{slug}/-/about", "the wiki's own facts"),
    (
        "/{slug}/-/verification",
        "the verification record: counts and words, no wiki text",
    ),
    ("/-/wikis", "the list of the wikis a person may read"),
    (
        "/{slug}/-/watch/{*title}",
        "follows a page: a write that answers with a redirect",
    ),
    // Writing: they show the tip, or a form of the person's own text.
    (
        "/{slug}/-/proposals/new",
        "a form over the tip's page; the person's own text",
    ),
    (
        "/{slug}/-/proposals/curate",
        "a form over the tip's knowledge files; the person's own text",
    ),
    // The views, their list and their forms.
    (
        "/{slug}/-/views",
        "the list of views: their names and where they came from",
    ),
    ("/{slug}/-/views/new", "the form that makes a view"),
    // The people and the moderation: people and rungs, never a revision.
    ("/{slug}/-/people", "the people of the wiki and their rungs"),
    ("/{slug}/-/people/grant", "the form that changes a rung"),
    (
        "/{slug}/-/people/restrict",
        "the form that restricts a person",
    ),
    (
        "/{slug}/-/people/renew",
        "the form that renews a restriction",
    ),
    ("/{slug}/-/people/lift", "the form that lifts a restriction"),
    // Appeals: what a person wrote of a grievance, to the person and the rung that decides, and a link to a proposal; never a
    // revision's content, summary or author, and never anything hidden.
    (
        "/{slug}/-/appeals",
        "the appeals, for the rung that decides them: what the appellants wrote, and the decisions",
    ),
    (
        "/{slug}/-/appeals/mine",
        "a person's own appeals and what they may contest",
    ),
    (
        "/{slug}/-/appeals/new",
        "the form that appeals a decision, and the act",
    ),
    ("/{slug}/-/appeals/decide", "the act that decides an appeal"),
    (
        "/{slug}/-/protect",
        "the form that protects a page: titles and reasons",
    ),
    (
        "/{slug}/-/moderation",
        "the moderation log: the item's address, the category, the role and the date, never the hidden content and never a name",
    ),
    (
        "/{slug}/-/moderate",
        "the form that hides: it names the item by its address and says what is hidden of it, never the content",
    ),
    (
        "/{slug}/-/moderate/unhide",
        "restores what a hide hid, and answers with a redirect",
    ),
    // The rules of the wiki (RFC 006 D-11, D-12; handoff G-T7): sentences of the rules in force, never a revision. (The page of the
    // rules also names the proposals that changed them: it is the surface `rules-history`.)
    (
        "/{slug}/-/policy/edit",
        "the form that proposes a change to the rules: the rules in force, in controls, and the person's own text",
    ),
    (
        "/{slug}/-/api/v1/policy",
        "the rules in force as data: no revision, no comment, no proposal",
    ),
    (
        "/{slug}/-/api/v1/protections",
        "the protections in force: titles, reasons and ends",
    ),
    (
        "/{slug}/-/api/v1/moderation-log",
        "the moderation log as data: the item's identifier, the category, the role and the date, never the hidden content and never a name",
    ),
    (
        "/{slug}/-/api/v1/appeals/mine",
        "the caller's own appeals, and nobody else's",
    ),
    // The knowledge files that cannot be read: the form is aimed at one file as the tip holds it.
    (
        "/{slug}/-/knowledge/replace",
        "a form aimed at one unreadable knowledge file as the view's tip holds it, which shows the file's name and the person's own text and reads no earlier revision; the content of a tip cannot be hidden",
    ),
    // The API's own: the tip, or the description of the API. (The members of a page and its raw and fragment also answer
    // `page-gone` for a removed page: they are the surface `removed-page`.)
    ("/{slug}/-/api/v1/pages", "the list of the tip's pages"),
    (
        "/{slug}/-/api/v1/backlinks/{*title}",
        "the links of the tip",
    ),
    ("/{slug}/-/api/v1/health", "the health lists of the tip"),
    ("/{slug}/-/api/v1/search", "searches the tip's text only"),
    ("/{slug}/-/api/v1/sources/{id}", "a source file at the tip"),
    (
        "/{slug}/-/api/v1/claims/{id}",
        "a claim's file at the tip: no thread, no comment",
    ),
    (
        "/{slug}/-/api/v1/proposals/curation",
        "a write that answers with the proposal as the proposal API gives it",
    ),
    (
        "/{slug}/-/api/{*rest}",
        "an address under the API that is not an endpoint",
    ),
    (
        "/-/api/{*rest}",
        "an address under the API that is not an endpoint",
    ),
];

/// The one place hidden content is served: to the `moderate` rung, by an explicit act that is audited, never to anyone else
/// (INV-13, Q-5).
pub const REVEALS: &[(&str, &str)] = &[(
    "/{slug}/-/moderate/show",
    "shows the hidden parts of one item to the `moderate` rung, after an event of the audit trail (one line, never content), as the answer of a POST that no cache stores",
)];

/// What a route is, for the gate that holds every route to this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A surface of the table: it shows a hidden item's tombstone in place of the hidden part.
    Surface,
    /// It shows no revision text, summary or author, for the stated reason.
    ShowsNone,
    /// The one act that shows hidden content to the `moderate` rung.
    Reveals,
    /// In no list: **a route that shows revision content and is not in the table fails the gate** (INV-13).
    Unlisted,
}

/// Which list a route is in.
pub fn classify(path: &str) -> Class {
    if is_surface_route(path) {
        Class::Surface
    } else if shows_none(path).is_some() {
        Class::ShowsNone
    } else if REVEALS.iter().any(|(p, _)| *p == path) {
        Class::Reveals
    } else {
        Class::Unlisted
    }
}

/// The surface with this id.
pub fn surface(id: &str) -> Option<&'static Surface> {
    SURFACES.iter().find(|s| s.id == id)
}

/// Is this route (or API member) in the table of surfaces?
pub fn is_surface_route(path: &str) -> bool {
    SURFACES
        .iter()
        .any(|s| s.routes.contains(&path) || s.api.contains(&path))
}

/// The reason a route shows no hidden part, when it is one of those that do not.
pub fn shows_none(path: &str) -> Option<&'static str> {
    SHOWS_NONE
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, reason)| *reason)
}

#[cfg(test)]
mod tests;
