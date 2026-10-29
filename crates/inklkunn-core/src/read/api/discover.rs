//! The reads that look across a wiki, for the API (handoff 003 T8): the pages that link to a page, the health
//! lists and the recent changes, each in bounded pages. They are the page views' own reads (the link graph,
//! the stale list, the changes) with a cursor; nothing is recomputed that the page views compute.
//!
//! The backlinks and the health lists are computed whole for one tip and walked by an **offset stamped with
//! that tip**: a walk that outlives its tip is told to start again (`invalid-cursor`) instead of being handed
//! a page of a different list. The changes are walked by the last change seen.

use std::sync::Arc;

use inklkunn_store::{IndexEntry, WikiRecord};

use super::{ApiError, Cursor, MAX_PAGE_SIZE, Position, change_key, list_stamp};
use crate::read::graph::MAX_BACKLINKS;
use crate::read::{
    Backlink, ChangeFilter, ChangeRow, GraphPage, LinkGraph, Lookup, ReadError, ReadService,
    RedirectIssue, StalePage, Tip, Wanted,
};

/// One page of an offset-walked list: the items from the cursor's offset, at most `limit`, and the cursor of
/// the next page. A cursor that is not an offset, or is of another tip, is [`ApiError::InvalidCursor`].
fn offset_page<T: Clone>(
    items: &[T],
    stamp: &str,
    after: Option<&Cursor>,
    limit: usize,
) -> Result<(Vec<T>, Option<Cursor>), ApiError> {
    let start = match after.map(Cursor::position) {
        None => 0,
        Some(Some(Position::Offset { at, stamp: held })) if held == stamp && at <= items.len() => {
            at
        }
        Some(_) => return Err(ApiError::InvalidCursor),
    };
    let end = start
        .saturating_add(limit.clamp(1, MAX_PAGE_SIZE))
        .min(items.len());
    let page = items.get(start..end).unwrap_or_default().to_vec();
    let next = (end < items.len()).then(|| Cursor::at_offset(end, stamp));
    Ok((page, next))
}

// ------------------------------------------------------------------------------------ backlinks

/// One page of the pages that link to a title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkPage {
    /// The tip the links were read at.
    pub tip: Tip,
    /// The page's title; for a title that leads to no page, the title as asked (trimmed).
    pub title: String,
    /// Does the title lead to a page? When it does not, these are the pages that link to it anyway (a wanted
    /// title).
    pub exists: bool,
    /// The links of this page of the list, in the graph's order (the linking page's path).
    pub items: Vec<Backlink>,
    /// All the links the graph holds for the title (at most 500).
    pub total: usize,
    /// The graph holds 500 links for the title and keeps no more: there may be others.
    pub cut: bool,
    /// Where the next page begins; `None` on the last.
    pub next: Option<Cursor>,
}

/// What a request for backlinks came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BacklinkAnswer {
    /// The links.
    Found(Box<BacklinkPage>),
    /// No page has the title and no page links to it.
    NotFound,
    /// Several pages match the title, none is nearer, and no page links to it as a missing title.
    Ambiguous {
        /// Every candidate.
        candidates: Vec<IndexEntry>,
    },
}

// ------------------------------------------------------------------------------------ health

/// The six health lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthList {
    /// Titles that links lead to and no page has.
    Wanted,
    /// Pages nothing links to.
    Orphans,
    /// Pages that link to no other page.
    DeadEnds,
    /// Redirects with something wrong.
    Redirects,
    /// Pages marked as work in progress, draft or stub.
    WantsHelp,
    /// Pages that have gone longest without a change.
    Stale,
}

impl HealthList {
    /// Every list, in the order the pages show them.
    pub const ALL: [HealthList; 6] = [
        HealthList::Wanted,
        HealthList::Orphans,
        HealthList::DeadEnds,
        HealthList::Redirects,
        HealthList::WantsHelp,
        HealthList::Stale,
    ];

    /// The stable word the web address and the API's `list` parameter end with.
    pub const fn slug(self) -> &'static str {
        match self {
            HealthList::Wanted => "wanted",
            HealthList::Orphans => "orphans",
            HealthList::DeadEnds => "dead-ends",
            HealthList::Redirects => "redirects",
            HealthList::WantsHelp => "wants-help",
            HealthList::Stale => "stale",
        }
    }

    /// The list a slug names.
    pub fn from_slug(slug: &str) -> Option<HealthList> {
        HealthList::ALL.into_iter().find(|l| l.slug() == slug)
    }
}

/// The rows of one page of a health list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthRows {
    /// Wanted titles, the most linked first.
    Wanted(Vec<Wanted>),
    /// Pages nothing links to, by title.
    Orphans(Vec<GraphPage>),
    /// Pages that link nowhere, by title.
    DeadEnds(Vec<GraphPage>),
    /// Redirects with something wrong, by title.
    Redirects(Vec<RedirectIssue>),
    /// Pages that ask for help, by title.
    WantsHelp(Vec<GraphPage>),
    /// Pages untouched in the changes looked through first, then the oldest touch first.
    Stale(Vec<StalePage>),
}

/// How far the stale list looked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleScan {
    /// How many changes were looked through (the newest [`SCAN_WINDOW`](crate::read::SCAN_WINDOW) of the view).
    pub looked_through: usize,
    /// The whole history was looked through: a page with no touch has none at all, rather than none lately.
    pub complete: bool,
}

/// One page of a health list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthPage {
    /// The tip the list was read at.
    pub tip: Tip,
    /// Which list.
    pub list: HealthList,
    /// The rows of this page.
    pub rows: HealthRows,
    /// How many rows the whole list has.
    pub total: usize,
    /// Where the next page begins; `None` on the last.
    pub next: Option<Cursor>,
    /// Set for the stale list only.
    pub scan: Option<StaleScan>,
}

// ------------------------------------------------------------------------------------ changes

/// One page of the recent changes. The rows are **not yet restricted to what the reader may see**: the surface
/// does that against the principal and says how many it left out, as the changes page does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangePage {
    /// The rows, newest first, at most the size asked for.
    pub rows: Vec<ChangeRow>,
    /// Where the next page begins; `None` on the last. Made from the last row of this page, whether or not the
    /// surface leaves it out.
    pub next: Option<Cursor>,
    /// The wiki's open views by name, the default first.
    pub views: Vec<String>,
    /// Some changes are listed without their pages because `show` could not be asked just now.
    pub pages_missing: bool,
    /// The list is the newest part of the history: the changes looked through
    /// ([`SCAN_WINDOW`](crate::read::SCAN_WINDOW) of each view) are not all of it.
    pub beyond_window: bool,
}

impl ReadService {
    /// One page of the pages that link to the page the title leads to, or, for a title that leads to no page,
    /// of the pages that link to it anyway. As the page view finds them: through the link graph of the tip.
    pub fn api_backlinks(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<BacklinkAnswer, ApiError> {
        let lookup = self.lookup(wiki, view, title)?;
        let graph = self.link_graph(wiki, view)?;
        let tip = self.tip(wiki, view)?;
        let (shown, links, exists) = match &lookup {
            Lookup::Found(found) => (
                found.entry.title.clone(),
                graph.backlinks(&found.entry.path),
                true,
            ),
            Lookup::BrokenRedirect { from, .. } => {
                (from.title.clone(), graph.backlinks(&from.path), true)
            }
            Lookup::Ambiguous { .. } | Lookup::Missing => {
                let wanted = graph.wanted_backlinks(title);
                if wanted.is_empty() {
                    return Ok(match lookup {
                        Lookup::Ambiguous { candidates } => {
                            BacklinkAnswer::Ambiguous { candidates }
                        }
                        _ => BacklinkAnswer::NotFound,
                    });
                }
                (title.trim().to_owned(), wanted, false)
            }
        };
        let stamp = list_stamp(
            graph.tip(),
            &format!("backlinks:{exists}:{}", shown.to_lowercase()),
        );
        let (items, next) = offset_page(links, &stamp, after, limit)?;
        Ok(BacklinkAnswer::Found(Box::new(BacklinkPage {
            tip,
            title: shown,
            exists,
            items,
            total: links.len(),
            cut: links.len() >= MAX_BACKLINKS,
            next,
        })))
    }

    /// One page of a health list of the view's tip, from the link graph (and, for the stale list, the
    /// changes), as the health pages read them.
    pub fn api_health(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        list: HealthList,
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<HealthPage, ApiError> {
        let tip = self.tip(wiki, view)?;
        let page = |rows: HealthRows, total, next, scan| HealthPage {
            tip: tip.clone(),
            list,
            rows,
            total,
            next,
            scan,
        };
        if list == HealthList::Stale {
            let stale = self.stale_pages_in(wiki, view)?;
            let stamp = list_stamp(&tip.state_id, "health:stale");
            let (rows, next) = offset_page(&stale.pages, &stamp, after, limit)?;
            let scan = StaleScan {
                looked_through: stale.looked_through,
                complete: stale.complete,
            };
            return Ok(page(
                HealthRows::Stale(rows),
                stale.pages.len(),
                next,
                Some(scan),
            ));
        }
        let graph: Arc<LinkGraph> = self.link_graph(wiki, view)?;
        let stamp = |name: &str| list_stamp(graph.tip(), &format!("health:{name}"));
        let pages =
            |found: Vec<&GraphPage>| -> Vec<GraphPage> { found.into_iter().cloned().collect() };
        let (rows, total, next) = match list {
            HealthList::Wanted => {
                let all = graph.wanted();
                let (rows, next) = offset_page(&all, &stamp("wanted"), after, limit)?;
                (HealthRows::Wanted(rows), all.len(), next)
            }
            HealthList::Redirects => {
                let all = graph.redirect_issues();
                let (rows, next) = offset_page(all, &stamp("redirects"), after, limit)?;
                (HealthRows::Redirects(rows), all.len(), next)
            }
            HealthList::Orphans => {
                let all = pages(graph.orphans());
                let (rows, next) = offset_page(&all, &stamp("orphans"), after, limit)?;
                (HealthRows::Orphans(rows), all.len(), next)
            }
            HealthList::DeadEnds => {
                let all = pages(graph.dead_ends());
                let (rows, next) = offset_page(&all, &stamp("dead-ends"), after, limit)?;
                (HealthRows::DeadEnds(rows), all.len(), next)
            }
            HealthList::WantsHelp | HealthList::Stale => {
                let all = pages(graph.wants_help());
                let (rows, next) = offset_page(&all, &stamp("wants-help"), after, limit)?;
                (HealthRows::WantsHelp(rows), all.len(), next)
            }
        };
        Ok(page(rows, total, next, None))
    }

    /// One page of the recent changes of the wiki's open views that pass `filter`, newest first, after the
    /// change the cursor names. The list is the one the changes page shows (the newest
    /// [`SCAN_WINDOW`](crate::read::SCAN_WINDOW) changes of each view); a cursor that names a change no longer
    /// in it is [`ApiError::InvalidCursor`] (start again). A `filter.view` that is not one of the wiki's open
    /// views is [`ReadError::NoSuchView`].
    pub fn api_changes(
        &self,
        wiki: &WikiRecord,
        filter: &ChangeFilter,
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<ChangePage, ApiError> {
        let after = match after.map(Cursor::position) {
            Some(Some(Position::Key(key))) => Some(key),
            Some(_) => return Err(ApiError::InvalidCursor),
            None => None,
        };
        let list = self.changes(wiki, filter, usize::MAX)?;
        if let Some(name) = &filter.view {
            let name = name.strip_prefix("heads/").unwrap_or(name);
            if !list.views.iter().any(|v| v == name) {
                return Err(ReadError::NoSuchView(format!("heads/{name}")).into());
            }
        }
        let start = match after {
            None => 0,
            Some(key) => {
                let at = list
                    .rows
                    .iter()
                    .position(|row| change_key(&row.block_id) == key)
                    .ok_or(ApiError::InvalidCursor)?;
                at + 1
            }
        };
        let end = start
            .saturating_add(limit.clamp(1, MAX_PAGE_SIZE))
            .min(list.rows.len());
        let rows: Vec<ChangeRow> = list.rows.get(start..end).unwrap_or_default().to_vec();
        let next = (end < list.rows.len())
            .then(|| rows.last().map(|row| Cursor::after_change(&row.block_id)))
            .flatten();
        Ok(ChangePage {
            rows,
            next,
            views: list.views,
            pages_missing: list.pages_missing,
            beyond_window: list.more,
        })
    }
}
