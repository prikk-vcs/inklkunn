//! Recent changes across a wiki's views (RFC 003 D-7; requirements RD-7): the sealed history of each open view
//! from `history()`, what each change touched from `change()`, who made it from the CT-03 message, and when
//! this service first saw it.
//!
//! **What prikk does not give, and what is done about it** (said on the pages, not hidden):
//!
//! - *No time.* prikk's log and `show` carry no timestamp. The only time there is, is **when this service first
//!   saw the block** ([`inklkunn_store::DerivedStore::first_seen`]), so that is what a row carries, and the
//!   surfaces call it that. For changes the service makes itself it is the time of the change to within the
//!   read that noticed it; for history that was already there when the service met the wiki it is the time of
//!   that meeting, for every one of them alike.
//! - *No touched pages in the log.* A change's pages come from one `show` per block, which is immutable and so
//!   remembered, and which is asked for only as many blocks as a request needs: the rows to be listed, or, when
//!   the request filters by page or namespace, the blocks of the scan window ([`SCAN_WINDOW`] per view).
//! - *No minor flag.* The patch message (CT-03) has no such trailer, so no change is minor; the filter exists
//!   and every change passes it as "not minor". A later message revision can set it.
//!
//! A change whose message is not the service's canonical text (sealed by someone else, INV-1) is listed, with
//! its first line as the summary and no people: [`ChangeKind::Foreign`].

use std::collections::HashSet;

use inklkunn_markup::path::{PAGES_DIR, path_to_title};
use inklkunn_prikk::{HistoryBlock, Point};
use inklkunn_signer::message::{Actor, Kind, Message};
use inklkunn_store::{WikiId, WikiRecord};

use super::{ReadError, ReadService};

/// How many of a view's newest changes a request looks through at most.
pub const SCAN_WINDOW: u64 = 100;

/// The most entries of the remembered per-block page lists before they are dropped and re-asked.
const MAX_REMEMBERED_BLOCKS: usize = 50_000;

/// A three-way filter: everything, only the ones that are, or none of the ones that are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tri {
    /// No filtering.
    #[default]
    Any,
    /// Only the changes that are.
    Only,
    /// Not the changes that are.
    Hide,
}

impl Tri {
    fn admits(self, is: bool) -> bool {
        match self {
            Tri::Any => true,
            Tri::Only => is,
            Tri::Hide => !is,
        }
    }
}

/// What the person who asked wants of the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeFilter {
    /// Only this view, by its name (`main` for `heads/main`); every open view when `None`.
    pub view: Option<String>,
    /// Only changes that touched a page whose title is under this hierarchy prefix.
    pub namespace: Option<String>,
    /// Only changes that touched the page this title leads to.
    pub page: Option<String>,
    /// Changes the service made itself (imports, creating the wiki).
    pub automated: Tri,
    /// Minor changes (none are marked, see the module's note).
    pub minor: Tri,
}

impl ChangeFilter {
    /// Does the filter narrow the list at all?
    pub fn is_active(&self) -> bool {
        self.view.is_some()
            || self.namespace.is_some()
            || self.page.is_some()
            || self.automated != Tri::Any
            || self.minor != Tri::Any
    }

    fn needs_pages(&self) -> bool {
        self.namespace.is_some() || self.page.is_some()
    }
}

/// Who a message names. Names are the text of a person, kept as written: a surface escapes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Person {
    /// An account, by the display name it had at the time.
    Account(String),
    /// An anonymous proposer, by temporary pseudonym.
    Anonymous(String),
    /// The service itself, by its label.
    Service(String),
}

impl From<&Actor> for Person {
    fn from(actor: &Actor) -> Self {
        match actor {
            Actor::Account { name, .. } => Person::Account(name.to_string()),
            Actor::Anonymous(p) => Person::Anonymous(p.as_str().to_owned()),
            Actor::System(l) => Person::Service(l.as_str().to_owned()),
        }
    }
}

/// What kind of change the message says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// An accepted proposal.
    Proposal,
    /// An accepted rollback.
    Rollback,
    /// An import made by the service.
    Import,
    /// Another change the service made itself.
    System,
    /// Sealed by someone else: the message is not the service's.
    Foreign,
}

impl ChangeKind {
    /// Did the service make this change itself, with nobody proposing it?
    pub const fn is_automated(self) -> bool {
        matches!(self, ChangeKind::Import | ChangeKind::System)
    }
}

/// A page a change touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPage {
    /// Its repository path.
    pub path: String,
    /// Its title: the tip's, if the page is still there, else the one its path spells.
    pub title: String,
    /// Is it still there at the view's tip (a link to it leads somewhere)?
    pub exists: bool,
}

/// One change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRow {
    /// The block that sealed it.
    pub block_id: String,
    /// The view it was listed from, by full name (`heads/main`).
    pub view: String,
    /// The block before it in that view, if the window shows one (a diff needs both).
    pub parent_block: Option<String>,
    /// The monotonic sequence within the view.
    pub update_seq: u64,
    /// The first line of the message: plain text.
    pub summary: String,
    /// What the message says it is.
    pub kind: ChangeKind,
    /// Who proposed it, if the message says.
    pub proposed_by: Option<Person>,
    /// Who accepted it, if the message says.
    pub accepted_by: Option<Person>,
    /// Is it an undo?
    pub rollback: bool,
    /// Marked minor (never, see the module's note).
    pub minor: bool,
    /// When this service first saw the block, in Unix seconds.
    pub first_seen: i64,
    /// What it touched; `None` when `show` could not be asked just now.
    pub pages: Option<Vec<ChangedPage>>,
}

impl ChangeRow {
    /// Did the service make this change itself?
    pub fn automated(&self) -> bool {
        self.kind.is_automated()
    }
}

/// The changes a request listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeList {
    /// The rows, newest first (by when they were first seen, then by sequence within a view).
    pub rows: Vec<ChangeRow>,
    /// The wiki's open views by name (`main`), the default first: what the view filter can name.
    pub views: Vec<String>,
    /// More changes lie beyond the [`SCAN_WINDOW`] looked through (or beyond the limit): the list is the
    /// newest part of the history, not all of it.
    pub more: bool,
    /// Some changes are listed without their pages because `show` could not be asked.
    pub pages_missing: bool,
}

/// The message of a block as a row's head: kind, summary and people.
struct Described {
    kind: ChangeKind,
    summary: String,
    proposed_by: Option<Person>,
    accepted_by: Option<Person>,
}

/// The longest summary taken from a message that is not the service's.
const FOREIGN_SUMMARY_CHARS: usize = 120;

fn describe(block: &HistoryBlock) -> Described {
    for patch in &block.patch_messages {
        if let Ok(message) = Message::parse(&patch.message) {
            return Described {
                kind: match message.kind() {
                    Kind::Proposal => ChangeKind::Proposal,
                    Kind::Rollback => ChangeKind::Rollback,
                    Kind::Import => ChangeKind::Import,
                    Kind::System => ChangeKind::System,
                },
                summary: message.summary().as_str().to_owned(),
                proposed_by: Some(Person::from(message.proposed_by())),
                accepted_by: Some(Person::from(message.accepted_by())),
            };
        }
    }
    let first_line = block
        .patch_messages
        .iter()
        .flat_map(|p| p.message.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut summary: String = first_line
        .chars()
        .filter(|c| !c.is_control())
        .take(FOREIGN_SUMMARY_CHARS)
        .collect();
    if first_line.chars().count() > FOREIGN_SUMMARY_CHARS {
        summary.push('…');
    }
    Described {
        kind: ChangeKind::Foreign,
        summary,
        proposed_by: None,
        accepted_by: None,
    }
}

/// A repository path that is a page.
fn is_page_path(path: &str) -> bool {
    path.strip_prefix(PAGES_DIR)
        .is_some_and(|rest| rest.starts_with('/'))
        && path.ends_with(inklkunn_markup::path::EXTENSION)
}

/// Is `title` in the hierarchy `namespace` (`Guides` holds `Guides/Setup`)? Compared as the page links are.
fn in_namespace(title: &str, namespace: &str) -> bool {
    let prefix = inklkunn_markup::normalize(namespace);
    let prefix = prefix.trim_end_matches('/');
    if prefix.is_empty() {
        return true;
    }
    let normalized = inklkunn_markup::normalize(title);
    normalized
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

impl ReadService {
    /// The wiki's open views by full name (`heads/main`), the default view first. Asks prikk unless the
    /// views are held.
    pub fn open_views(&self, wiki: &WikiRecord) -> Result<Vec<String>, ReadError> {
        let repo = self.layout.repo_dir(&wiki.id);
        let views = self
            .storage
            .views(&repo)
            .map_err(|e| self.storage_error(wiki, e))?;
        self.tips.hold(&wiki.id, &repo, &views);
        let mut names: Vec<String> = views
            .views
            .iter()
            .filter(|v| !v.closed)
            .map(|v| v.name.clone())
            .collect();
        names.sort();
        if let Some(at) = names.iter().position(|n| *n == wiki.default_view) {
            let default = names.remove(at);
            names.insert(0, default);
        }
        Ok(names)
    }

    /// The paths a block touched, from `show`, remembered (a block never changes).
    fn pages_of_block(
        &self,
        wiki: &WikiId,
        repo: &std::path::Path,
        block: &str,
    ) -> Option<Vec<String>> {
        let key = (wiki.clone(), block.to_owned());
        if let Some(known) = self
            .block_pages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return Some(known.clone());
        }
        let change = self.storage.change(repo, block).ok()?;
        let mut paths: Vec<String> = Vec::new();
        for patch in &change.patches {
            for op in &patch.operations {
                for p in op.paths.iter().filter_map(|r| r.path.as_deref()) {
                    if is_page_path(p) && !paths.iter().any(|q| q == p) {
                        paths.push(p.to_owned());
                    }
                }
            }
        }
        let mut held = self
            .block_pages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if held.len() >= MAX_REMEMBERED_BLOCKS {
            held.clear();
        }
        held.insert(key, paths.clone());
        Some(paths)
    }

    /// The changes of the wiki's open views that pass `filter`, newest first, at most `limit` of them.
    ///
    /// Looks through the newest [`SCAN_WINDOW`] changes of each view: `more` says there is history beyond it.
    pub fn changes(
        &self,
        wiki: &WikiRecord,
        filter: &ChangeFilter,
        limit: usize,
    ) -> Result<ChangeList, ReadError> {
        let repo = self.layout.repo_dir(&wiki.id);
        let all_views = self.open_views(wiki)?;
        let views: Vec<&String> = match &filter.view {
            Some(name) => all_views
                .iter()
                .filter(|v| v.strip_prefix("heads/") == Some(name.as_str()) || *v == name)
                .collect(),
            None => all_views.iter().collect(),
        };
        // The newest blocks of each view, a block listed once (a view branched from another shares its past).
        struct Candidate {
            view: String,
            rank: usize,
            block: HistoryBlock,
            parent: Option<String>,
        }
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut more = false;
        for (rank, view) in views.iter().enumerate() {
            let history = self
                .storage
                .history(&repo, &Point::View((*view).clone()), Some(SCAN_WINDOW + 1))
                .map_err(|e| self.storage_error(wiki, e))?;
            let mut blocks = history.blocks;
            if u64::try_from(blocks.len()).unwrap_or(u64::MAX) > SCAN_WINDOW {
                more = true;
                blocks.truncate(usize::try_from(SCAN_WINDOW).unwrap_or(usize::MAX));
            }
            let parents: Vec<Option<String>> = blocks
                .iter()
                .skip(1)
                .map(|b| Some(b.block_id.clone()))
                .chain(std::iter::once(None))
                .collect();
            for (block, parent) in blocks.into_iter().zip(parents) {
                if seen.insert(block.block_id.clone()) {
                    candidates.push(Candidate {
                        view: (*view).clone(),
                        rank,
                        block,
                        parent,
                    });
                }
            }
        }
        // When this service first saw each (one transaction); a store that cannot answer leaves them unseen,
        // and the rows say 0 rather than a time that was made up.
        let ids: Vec<String> = candidates
            .iter()
            .map(|c| c.block.block_id.clone())
            .collect();
        let seen_at = self
            .derived
            .first_seen(&wiki.id, &ids, inklkunn_store::record::now_secs())
            .unwrap_or_else(|_| {
                self.store_failures
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                vec![0; ids.len()]
            });
        let mut ordered: Vec<(i64, Candidate)> = seen_at.into_iter().zip(candidates).collect();
        ordered.sort_by(|a, b| {
            (
                std::cmp::Reverse(a.0),
                a.1.rank,
                std::cmp::Reverse(a.1.block.update_seq),
            )
                .cmp(&(
                    std::cmp::Reverse(b.0),
                    b.1.rank,
                    std::cmp::Reverse(b.1.block.update_seq),
                ))
        });

        // The page the `page` filter names, as a path.
        let page_path = match filter.page.as_deref() {
            Some(title) => match self.lookup(wiki, None, title) {
                Ok(super::index::Lookup::Found(f)) => Some(Some(f.entry.path)),
                Ok(_) => Some(None),
                Err(_) => Some(None),
            },
            None => None,
        };
        // Titles of the pages, from the tip's index where it can be read.
        let mut indexes: Vec<(String, Option<std::sync::Arc<super::index::TitleIndex>>)> =
            Vec::new();

        let mut rows = Vec::new();
        let mut pages_missing = false;
        let mut cut = false;
        for (first_seen, c) in ordered {
            let head = describe(&c.block);
            let rollback = c.block.rollback_block || head.kind == ChangeKind::Rollback;
            if !filter.automated.admits(head.kind.is_automated()) || !filter.minor.admits(false) {
                continue;
            }
            let wants_pages = filter.needs_pages() || rows.len() < limit;
            let touched = if wants_pages {
                self.pages_of_block(&wiki.id, &repo, &c.block.block_id)
            } else {
                None
            };
            if filter.needs_pages() && touched.is_none() {
                pages_missing = true;
                continue;
            }
            let titled = touched.map(|paths| {
                let index = match indexes.iter().find(|(v, _)| *v == c.view) {
                    Some((_, ix)) => ix.clone(),
                    None => {
                        let ix = self.title_index(wiki, Some(&c.view)).ok();
                        indexes.push((c.view.clone(), ix.clone()));
                        ix
                    }
                };
                paths
                    .into_iter()
                    .map(|path| {
                        let entry = index.as_ref().and_then(|ix| ix.entry(&path));
                        ChangedPage {
                            title: entry
                                .map(|e| e.title.clone())
                                .or_else(|| path_to_title(&path))
                                .unwrap_or_else(|| path.clone()),
                            exists: entry.is_some(),
                            path,
                        }
                    })
                    .collect::<Vec<_>>()
            });
            if let Some(pages) = &titled {
                if let Some(Some(wanted)) = &page_path {
                    if !pages.iter().any(|p| p.path == *wanted) {
                        continue;
                    }
                } else if let Some(None) = &page_path {
                    continue;
                }
                if let Some(ns) = &filter.namespace
                    && !pages.iter().any(|p| in_namespace(&p.title, ns))
                {
                    continue;
                }
            }
            if titled.is_none() && wants_pages {
                pages_missing = true;
            }
            if rows.len() >= limit {
                cut = true;
                break;
            }
            rows.push(ChangeRow {
                block_id: c.block.block_id.clone(),
                view: c.view,
                parent_block: c.parent,
                update_seq: c.block.update_seq,
                summary: head.summary,
                kind: head.kind,
                proposed_by: head.proposed_by,
                accepted_by: head.accepted_by,
                rollback,
                minor: false,
                first_seen,
                pages: titled,
            });
        }
        Ok(ChangeList {
            rows,
            views: all_views
                .iter()
                .map(|v| v.strip_prefix("heads/").unwrap_or(v).to_owned())
                .collect(),
            more: more || cut,
            pages_missing,
        })
    }
}

/// A page, and when it was last touched in the changes looked through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StalePage {
    /// The page.
    pub entry: inklkunn_store::IndexEntry,
    /// When this service first saw the newest change that touched it; `None` when none of the changes looked
    /// through touched it.
    pub last_touched: Option<i64>,
}

/// The pages of the default view ordered by how long ago they were last touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleList {
    /// Pages untouched in the changes looked through first (by title), then the oldest touch first.
    pub pages: Vec<StalePage>,
    /// How many changes were looked through.
    pub looked_through: usize,
    /// The whole history was looked through: a page with no touch has none at all, rather than none lately.
    pub complete: bool,
}

impl ReadService {
    /// The pages that have gone longest without a change, from the newest [`SCAN_WINDOW`] changes of the
    /// default view. prikk's log does not say which pages a change touched, so a page's last change is known
    /// only inside the window; the list says how far it looked ([`StaleList::complete`]).
    pub fn stale_pages(&self, wiki: &WikiRecord) -> Result<StaleList, ReadError> {
        let window = usize::try_from(SCAN_WINDOW).unwrap_or(usize::MAX);
        let filter = ChangeFilter {
            view: Some(wiki.default_view.clone()),
            ..ChangeFilter::default()
        };
        let list = self.changes(wiki, &filter, window)?;
        let mut touched: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
        for row in &list.rows {
            for page in row.pages.iter().flatten() {
                touched.entry(page.path.as_str()).or_insert(row.first_seen);
            }
        }
        let index = self.title_index(wiki, None)?;
        let mut pages: Vec<StalePage> = index
            .entries()
            .iter()
            .filter(|e| e.kind == "page" && e.problem.is_none())
            .map(|e| StalePage {
                last_touched: touched.get(e.path.as_str()).copied(),
                entry: e.clone(),
            })
            .collect();
        pages.sort_by(|a, b| {
            (a.last_touched, &a.entry.title, &a.entry.path).cmp(&(
                b.last_touched,
                &b.entry.title,
                &b.entry.path,
            ))
        });
        Ok(StaleList {
            pages,
            looked_through: list.rows.len(),
            complete: !list.more,
        })
    }
}

#[cfg(test)]
mod tests;
