//! The link graph of a tip (RFC 003 D-9; requirements RD-5, RD-6): who links to whom, derived from the pages
//! of **one extract of the tip** and held in memory per (wiki, tip), built on demand by the first backlinks or
//! health view at that tip and rebuilt (never stored, never authoritative: INV-5) when the tip moves.
//!
//! Every page is read from the extracted files, parsed, its links scanned and resolved against the tip's title
//! index by the same rules the renderer uses (MKUP-5, [`inklkunn_markup::Index::resolve`]), so a backlink
//! list and a rendered page agree about what links where. The graph keeps, for each page, the edges that
//! leave it; and, for each page and each wanted title, the edges that arrive, with **the line of the source
//! that makes the link as its context** (plain text: a surface escapes it).
//!
//! The health views are questions put to the graph and the index:
//!
//! - *wanted*: links whose target is no page, ranked by how many pages make them;
//! - *orphans*: pages nothing links to (a redirect to a page, or an inclusion of it, counts as a link);
//! - *dead ends*: pages that link to no other page;
//! - *redirects*: redirect pages whose target is no page, that loop or run too long, and double redirects;
//! - *wants help*: pages whose status is `wip` or `draft`, or marked as stubs (the tag or status `stub`).
//!
//! A page whose source cannot be read (not UTF-8, frontmatter outside the subset, over the page bound) is a
//! page of the graph with no edges out: it is listed by the views as what it is, never dropped.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, PoisonError};

use inklkunn_markup::render::IMAGE_EXTENSIONS;
use inklkunn_markup::{Page, Via, nfc_trim, normalize};
use inklkunn_store::{IndexEntry, WikiRecord};

use super::index::{MAX_REDIRECT_HOPS, TitleIndex};
use super::{ReadError, ReadService};

/// The most links read from one page.
const MAX_LINKS_PER_PAGE: usize = 1_000;
/// The most characters kept of the line a link stands in.
const CONTEXT_CHARS: usize = 240;
/// The most backlinks kept for one target (the count is of all of them).
pub(super) const MAX_BACKLINKS: usize = 500;
/// The most graphs kept in memory at once, over all wikis.
const MAX_MEMORY_GRAPHS: usize = 8;

/// How a page reaches another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacklinkKind {
    /// A link in the text.
    Link,
    /// An inclusion (`![[Page]]`).
    Include,
    /// A redirect page whose target this is.
    Redirect,
}

/// A page that links to another, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    /// The linking page's path.
    pub from_path: String,
    /// The linking page's title.
    pub from_title: String,
    /// How it links.
    pub kind: BacklinkKind,
    /// The line of its source that makes the link, trimmed and shortened: plain text. Empty for a redirect.
    pub context: String,
}

/// A page of the graph, with what the health views ask of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPage {
    /// The page.
    pub entry: IndexEntry,
    /// Its status (`stable` when it names none).
    pub status: String,
    /// Its tags.
    pub tags: Vec<String>,
    /// Different pages that link to it (redirects and inclusions count).
    pub inbound: usize,
    /// Different other pages it links to (inclusions count).
    pub outbound: usize,
}

impl GraphPage {
    /// Is it a page to read (not a redirect, not unreadable)?
    fn is_content(&self) -> bool {
        self.entry.kind == "page" && self.entry.problem.is_none()
    }

    /// Does it ask for help: work in progress, a draft, or a stub?
    pub fn wants_help(&self) -> bool {
        self.is_content()
            && (matches!(self.status.as_str(), "wip" | "draft" | "stub")
                || self.tags.iter().any(|t| t.eq_ignore_ascii_case("stub")))
    }
}

/// A title that links lead to and no page has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// The title, as the first link to it spelled it.
    pub title: String,
    /// How many different pages link to it.
    pub linked_from: usize,
}

/// What is wrong with a redirect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectIssueKind {
    /// The target is no page.
    Broken,
    /// The chain returns to a page already on it.
    Loop,
    /// The chain is longer than [`MAX_REDIRECT_HOPS`].
    TooLong,
    /// The target is itself a redirect (the chain has two hops or more but ends at a page).
    Double,
}

/// A redirect page and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectIssue {
    /// The redirect page.
    pub entry: IndexEntry,
    /// The target as the page wrote it.
    pub target: String,
    /// The problem.
    pub kind: RedirectIssueKind,
}

/// The link graph of one tip.
#[derive(Debug)]
pub struct LinkGraph {
    tip: String,
    pages: Vec<GraphPage>,
    /// Page path → the pages that link to it.
    inbound: HashMap<String, Vec<Backlink>>,
    /// Normalized wanted title → (first spelling, the pages that link to it).
    wanted: BTreeMap<String, (String, Vec<Backlink>)>,
    redirects: Vec<RedirectIssue>,
}

impl LinkGraph {
    /// The tip's state id.
    pub fn tip(&self) -> &str {
        &self.tip
    }

    /// Every page.
    pub fn pages(&self) -> &[GraphPage] {
        &self.pages
    }

    /// The pages that link to the page at `path`, in path order.
    pub fn backlinks(&self, path: &str) -> &[Backlink] {
        self.inbound.get(path).map_or(&[], Vec::as_slice)
    }

    /// The pages that link to `title`, which is no page.
    pub fn wanted_backlinks(&self, title: &str) -> &[Backlink] {
        self.wanted
            .get(&normalize(title))
            .map_or(&[], |(_, from)| from.as_slice())
    }

    /// Wanted titles, the most linked first, then by title.
    pub fn wanted(&self) -> Vec<Wanted> {
        let mut out: Vec<Wanted> = self
            .wanted
            .values()
            .map(|(title, from)| Wanted {
                title: title.clone(),
                linked_from: distinct_sources(from),
            })
            .collect();
        out.sort_by(|a, b| {
            b.linked_from
                .cmp(&a.linked_from)
                .then_with(|| a.title.cmp(&b.title))
        });
        out
    }

    /// Pages nothing links to, by title.
    pub fn orphans(&self) -> Vec<&GraphPage> {
        self.listed(|p| p.is_content() && p.inbound == 0)
    }

    /// Pages that link to no other page, by title.
    pub fn dead_ends(&self) -> Vec<&GraphPage> {
        self.listed(|p| p.is_content() && p.outbound == 0)
    }

    /// Pages that ask for help, by title.
    pub fn wants_help(&self) -> Vec<&GraphPage> {
        self.listed(GraphPage::wants_help)
    }

    /// Redirects with something wrong, by title.
    pub fn redirect_issues(&self) -> &[RedirectIssue] {
        &self.redirects
    }

    fn listed(&self, keep: impl Fn(&GraphPage) -> bool) -> Vec<&GraphPage> {
        let mut out: Vec<&GraphPage> = self.pages.iter().filter(|p| keep(p)).collect();
        out.sort_by(|a, b| (&a.entry.title, &a.entry.path).cmp(&(&b.entry.title, &b.entry.path)));
        out
    }

    /// Build the graph of `index` from the extracted tree at `root`. Pages bigger than `page_max_bytes` are
    /// pages with no edges.
    pub fn build(index: &TitleIndex, root: &Path, page_max_bytes: u64) -> LinkGraph {
        let mut inbound: HashMap<String, Vec<Backlink>> = HashMap::new();
        let mut wanted: BTreeMap<String, (String, Vec<Backlink>)> = BTreeMap::new();
        let mut outbound: HashMap<String, HashSet<String>> = HashMap::new();
        let mut linked: HashMap<String, HashSet<String>> = HashMap::new();
        let mut status: HashMap<String, (String, Vec<String>)> = HashMap::new();

        for entry in index.entries() {
            let Some(text) = read_page(&root.join(&entry.path), page_max_bytes) else {
                continue;
            };
            let Ok(page) = Page::parse(&text) else {
                continue;
            };
            status.insert(
                entry.path.clone(),
                (page.metadata.status.clone(), page.metadata.tags.clone()),
            );
            let lines: Vec<&str> = text.lines().collect();
            for link in page.scan.links.iter().take(MAX_LINKS_PER_PAGE) {
                let target = nfc_trim(&link.target);
                let resolution = index.resolver().resolve(&target, Some(&entry.path));
                let context = lines
                    .get(link.line.saturating_sub(1))
                    .map(|l| shorten(l))
                    .unwrap_or_default();
                let from = |kind| Backlink {
                    from_path: entry.path.clone(),
                    from_title: entry.title.clone(),
                    kind,
                    context: context.clone(),
                };
                let kind = if link.embed {
                    BacklinkKind::Include
                } else {
                    BacklinkKind::Link
                };
                match resolution.via {
                    Via::Interwiki | Via::SamePage => {}
                    Via::Dangling => {
                        // A file named in an embed is not a page that is missing.
                        let file = link.embed
                            && target.rsplit('.').next().is_some_and(|e| {
                                IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())
                            });
                        if !file && !target.is_empty() {
                            let slot = wanted
                                .entry(normalize(&target))
                                .or_insert_with(|| (target.clone(), Vec::new()));
                            if slot.1.len() < MAX_BACKLINKS {
                                slot.1.push(from(kind));
                            }
                        }
                    }
                    _ => {
                        // An ambiguous link goes to its first candidate, as the renderer does.
                        let Some(to) = resolution.path.as_deref().and_then(|p| index.entry(p))
                        else {
                            continue;
                        };
                        if to.path == entry.path {
                            continue;
                        }
                        outbound
                            .entry(entry.path.clone())
                            .or_default()
                            .insert(to.path.clone());
                        linked
                            .entry(to.path.clone())
                            .or_default()
                            .insert(entry.path.clone());
                        let slot = inbound.entry(to.path.clone()).or_default();
                        if slot.len() < MAX_BACKLINKS {
                            slot.push(from(kind));
                        }
                    }
                }
            }
        }
        // A redirect is a link to its target.
        for entry in index.entries() {
            let Some(target) = entry.redirect.as_deref() else {
                continue;
            };
            let resolution = index.resolver().resolve(target, Some(&entry.path));
            let Some(to) = resolution
                .path
                .as_deref()
                .filter(|_| !matches!(resolution.via, Via::Dangling | Via::Interwiki))
                .and_then(|p| index.entry(p))
            else {
                continue;
            };
            if to.path == entry.path {
                continue;
            }
            outbound
                .entry(entry.path.clone())
                .or_default()
                .insert(to.path.clone());
            linked
                .entry(to.path.clone())
                .or_default()
                .insert(entry.path.clone());
            inbound.entry(to.path.clone()).or_default().push(Backlink {
                from_path: entry.path.clone(),
                from_title: entry.title.clone(),
                kind: BacklinkKind::Redirect,
                context: String::new(),
            });
        }
        for list in inbound.values_mut() {
            list.sort_by(|a, b| {
                (&a.from_path, a.kind as u8, &a.context).cmp(&(
                    &b.from_path,
                    b.kind as u8,
                    &b.context,
                ))
            });
        }
        let pages = index
            .entries()
            .iter()
            .map(|entry| {
                let (status, tags) = status
                    .remove(&entry.path)
                    .unwrap_or_else(|| ("stable".to_owned(), Vec::new()));
                GraphPage {
                    inbound: linked.get(&entry.path).map_or(0, HashSet::len),
                    outbound: outbound.get(&entry.path).map_or(0, HashSet::len),
                    status,
                    tags,
                    entry: entry.clone(),
                }
            })
            .collect();
        let mut redirects: Vec<RedirectIssue> = index
            .entries()
            .iter()
            .filter_map(|e| redirect_issue(index, e))
            .collect();
        redirects
            .sort_by(|a, b| (&a.entry.title, &a.entry.path).cmp(&(&b.entry.title, &b.entry.path)));
        LinkGraph {
            tip: index.tip().to_owned(),
            pages,
            inbound,
            wanted,
            redirects,
        }
    }
}

fn distinct_sources(from: &[Backlink]) -> usize {
    from.iter()
        .map(|b| b.from_path.as_str())
        .collect::<HashSet<_>>()
        .len()
}

/// The page's text if it can be read under the bound and is UTF-8.
fn read_page(path: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// One line of a page's source, for a backlink's context: whitespace collapsed, cut at a length.
fn shorten(line: &str) -> String {
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= CONTEXT_CHARS {
        return collapsed;
    }
    let mut cut: String = collapsed.chars().take(CONTEXT_CHARS).collect();
    cut.push('…');
    cut
}

/// What is wrong with the redirect `entry` makes, if it is one and something is.
fn redirect_issue(index: &TitleIndex, entry: &IndexEntry) -> Option<RedirectIssue> {
    entry.redirect.as_ref()?;
    let mut seen = vec![entry.path.as_str()];
    let mut at = entry;
    let mut hops = 0usize;
    let issue = |kind| RedirectIssue {
        entry: entry.clone(),
        target: entry.redirect.clone().unwrap_or_default(),
        kind,
    };
    while let Some(target) = at.redirect.as_deref() {
        let resolution = index.resolver().resolve(target, Some(&at.path));
        let next = match resolution.via {
            Via::Dangling | Via::Interwiki | Via::SamePage => None,
            _ => resolution.path.as_deref().and_then(|p| index.entry(p)),
        };
        let Some(next) = next else {
            return Some(issue(RedirectIssueKind::Broken));
        };
        if seen.contains(&next.path.as_str()) {
            return Some(issue(RedirectIssueKind::Loop));
        }
        if seen.len() > MAX_REDIRECT_HOPS {
            return Some(issue(RedirectIssueKind::TooLong));
        }
        seen.push(next.path.as_str());
        hops += 1;
        at = next;
    }
    (hops >= 2).then(|| issue(RedirectIssueKind::Double))
}

impl ReadService {
    /// The link graph of the view's tip (the default view when `None`), built from one extract the first time
    /// it is asked for at a tip.
    pub fn link_graph(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
    ) -> Result<Arc<LinkGraph>, ReadError> {
        let tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        if let Some(graph) = self.remembered_graph(wiki, &tip.state_id) {
            return Ok(graph);
        }
        let lock = self.build_lock(&wiki.id);
        let _building = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(graph) = self.remembered_graph(wiki, &tip.state_id) {
            return Ok(graph);
        }
        let max = self.config.page_max_bytes;
        let graph =
            Arc::new(self.with_tree(wiki, &tip, |root| Ok(LinkGraph::build(&index, root, max)))?);
        let mut held = self.graphs.lock().unwrap_or_else(PoisonError::into_inner);
        held.retain(|(w, g)| !(*w == wiki.id && g.tip() == graph.tip()));
        held.push((wiki.id.clone(), Arc::clone(&graph)));
        let excess = held.len().saturating_sub(MAX_MEMORY_GRAPHS);
        held.drain(..excess);
        Ok(graph)
    }

    fn remembered_graph(&self, wiki: &WikiRecord, tip: &str) -> Option<Arc<LinkGraph>> {
        self.graphs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(w, g)| *w == wiki.id && g.tip() == tip)
            .map(|(_, g)| Arc::clone(g))
    }
}

#[cfg(test)]
mod tests;
