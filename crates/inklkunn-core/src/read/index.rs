//! The title index of a view's tip (RFC 003 D-2; internal design DM-7): which pages the tip has, by title,
//! alias and redirect, and where each lives. It is **derived from the frontmatter of every page** and built
//! from one materialization of the tip, never from one prikk call per page (handoff 003 hold note IS-10:
//! each `cat` replays the whole tree).
//!
//! Two halves. [`entries_from_tree`] reads an extracted tree: the first bytes of every file under `pages/`,
//! the frontmatter parsed, an entry for each, a page whose frontmatter cannot be read listed with the reason
//! (a page that cannot be parsed must still be findable to be repaired). [`TitleIndex`] answers questions
//! about a set of entries: what does this title lead to, by which rule of MKUP-5, through which redirects.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use inklkunn_markup::path::{PAGES_DIR, path_to_title};
use inklkunn_markup::{Index, PageEntry, Via, frontmatter, nfc_trim, parse_metadata};
use inklkunn_store::IndexEntry;

/// How much of a page file is read to find its frontmatter. A frontmatter longer than this is a page whose
/// metadata is not read ([`Problem::FrontmatterTooLong`]).
pub const FRONTMATTER_READ_BYTES: usize = 64 * 1024;

/// How many redirects are followed before the chain is called too long. Chains of more than one are what
/// the health views list (double redirects); the reader is spared the second hop up to this many.
pub const MAX_REDIRECT_HOPS: usize = 5;

/// Why a page's metadata is not in its entry. Stored as [`Problem::code`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The file is not UTF-8.
    NotUtf8,
    /// The frontmatter is outside the plain YAML subset (or malformed).
    Frontmatter,
    /// The frontmatter is longer than [`FRONTMATTER_READ_BYTES`].
    FrontmatterTooLong,
}

impl Problem {
    /// The stable code stored in the index.
    pub const fn code(self) -> &'static str {
        match self {
            Problem::NotUtf8 => "not-utf8",
            Problem::Frontmatter => "frontmatter",
            Problem::FrontmatterTooLong => "frontmatter-too-long",
        }
    }
}

/// Why an extracted tree could not be turned into entries.
#[derive(Debug)]
pub enum TreeError {
    /// More entries under `pages/` than the bound.
    TooManyEntries(u64),
    /// A directory could not be listed.
    Io(std::io::Error),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeError::TooManyEntries(n) => write!(f, "more than {n} entries under `pages/`"),
            TreeError::Io(e) => write!(f, "the extracted tree could not be listed: {e}"),
        }
    }
}

impl std::error::Error for TreeError {}

/// An entry for every page file under `<root>/pages`, by path. `max_entries` bounds the files and
/// directories visited. Only regular files and directories are followed; a link, a device or a name that is
/// not UTF-8 is not a page (prikk stores none of them, and none could have a path of ours).
pub fn entries_from_tree(root: &Path, max_entries: u64) -> Result<Vec<IndexEntry>, TreeError> {
    let mut found: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut visited: u64 = 0;
    let mut stack = vec![(PAGES_DIR.to_owned(), root.join(PAGES_DIR))];
    while let Some((logical, dir)) = stack.pop() {
        let listing = match std::fs::read_dir(&dir) {
            Ok(l) => l,
            // A tree without a `pages/` directory has no pages.
            Err(e) if logical == PAGES_DIR && e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(TreeError::Io(e)),
        };
        for item in listing {
            let item = item.map_err(TreeError::Io)?;
            visited += 1;
            if visited > max_entries {
                return Err(TreeError::TooManyEntries(max_entries));
            }
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(kind) = item.file_type() else {
                continue;
            };
            let path = format!("{logical}/{name}");
            if kind.is_dir() {
                stack.push((path, item.path()));
            } else if kind.is_file() && name.ends_with(inklkunn_markup::path::EXTENSION) {
                found.push((path, item.path()));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    // A file that cannot be read from the tree fails the build instead of becoming an entry: it says nothing
    // about the page (a full disk, a failing device), and an index that is stored carries it until the tip
    // moves. The next request tries again.
    found
        .into_iter()
        .map(|(path, file)| entry_of(path, &file).map_err(TreeError::Io))
        .collect()
}

/// The entry for the page file at `file`, which the repository calls `path`. An error only for a file that
/// could not be read at all; a file that can be read and is not a page is an entry with its [`Problem`].
fn entry_of(path: String, file: &Path) -> std::io::Result<IndexEntry> {
    let from_path = || path_to_title(&path).unwrap_or_else(|| path.clone());
    let unreadable = |problem: Problem, path: String, title: String| IndexEntry {
        path,
        title,
        aliases: Vec::new(),
        redirect: None,
        page_id: None,
        kind: "page".to_owned(),
        problem: Some(problem.code().to_owned()),
    };
    let head = read_head(file)?;
    let text = match head.text() {
        Some(t) => t,
        None => {
            let title = from_path();
            return Ok(unreadable(Problem::NotUtf8, path, title));
        }
    };
    if head.truncated && frontmatter_is_open(text) {
        let title = from_path();
        return Ok(unreadable(Problem::FrontmatterTooLong, path, title));
    }
    Ok(match parse_metadata(text) {
        Ok(meta) => {
            let title = meta
                .title
                .as_deref()
                .map(nfc_trim)
                .filter(|t| !t.is_empty())
                .unwrap_or_else(from_path);
            IndexEntry {
                title,
                aliases: meta.aliases.iter().map(|a| nfc_trim(a)).collect(),
                redirect: meta
                    .redirect
                    .as_deref()
                    .map(nfc_trim)
                    .filter(|r| !r.is_empty()),
                page_id: meta.id.clone().filter(|id| !id.is_empty()),
                kind: meta.kind,
                problem: None,
                path,
            }
        }
        Err(_) => {
            let title = from_path();
            unreadable(Problem::Frontmatter, path, title)
        }
    })
}

/// The beginning of a file.
struct Head {
    bytes: Vec<u8>,
    /// The file goes on past what was read.
    truncated: bool,
}

impl Head {
    /// The head as text: all of it, or, for a truncated file, up to the last whole character.
    fn text(&self) -> Option<&str> {
        match std::str::from_utf8(&self.bytes) {
            Ok(t) => Some(t),
            // The cut fell inside a character: the text before it is the text.
            Err(e) if self.truncated && e.error_len().is_none() => {
                std::str::from_utf8(self.bytes.get(..e.valid_up_to())?).ok()
            }
            Err(_) => None,
        }
    }
}

fn read_head(file: &Path) -> std::io::Result<Head> {
    let mut bytes = Vec::new();
    File::open(file)?
        .take(FRONTMATTER_READ_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() > FRONTMATTER_READ_BYTES;
    bytes.truncate(FRONTMATTER_READ_BYTES);
    Ok(Head { bytes, truncated })
}

/// Does the text open a frontmatter that it does not close?
fn frontmatter_is_open(text: &str) -> bool {
    let first = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .lines()
        .next()
        .unwrap_or("");
    first.trim_end() == "---" && frontmatter::split(text).0.is_none()
}

// ---------------------------------------------------------------------------------------------------------

/// Where a title leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// A page.
    Found(Found),
    /// A redirect page whose chain cannot be followed to a page.
    BrokenRedirect {
        /// The redirect page the chain began at.
        from: IndexEntry,
        /// Why it ends.
        problem: RedirectProblem,
    },
    /// Several pages match and none is nearer (NAV-15).
    Ambiguous {
        /// Every candidate, in index order.
        candidates: Vec<IndexEntry>,
    },
    /// No page: the 404 that invites the page (NAV-3).
    Missing,
}

/// A title that led to a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The page.
    pub entry: IndexEntry,
    /// The rule of MKUP-5 that matched the title the reader gave (the first hop, for a redirect). Anything
    /// but `Exact` is a tolerant match: the surface may send the reader to the page's own address.
    pub via: Via,
    /// Set when the reader did not ask for this page by its own title: an alias led here, or a redirect.
    pub redirected_from: Option<RedirectedFrom>,
}

/// How a reader came to a page other than by its title (D-1: *redirected from*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectedFrom {
    /// The title the reader gave. Plain text: a surface escapes it.
    pub title: String,
    /// Through what.
    pub by: RedirectedBy,
}

/// Through what a reader was redirected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectedBy {
    /// The title is an alias of the page (META-4).
    Alias,
    /// The title is a redirect page, followed to its target.
    Redirect,
}

/// Why a chain of redirects does not end at a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectProblem {
    /// The target names no page. Plain text: a surface escapes it.
    Dangling {
        /// The target as the redirect page wrote it.
        target: String,
    },
    /// The chain returns to a page already on it.
    Loop,
    /// The chain is longer than [`MAX_REDIRECT_HOPS`].
    TooLong,
}

/// The pages of one tip, ready to answer.
#[derive(Debug)]
pub struct TitleIndex {
    tip: String,
    entries: Vec<IndexEntry>,
    by_path: HashMap<String, usize>,
    resolver: Index,
}

impl TitleIndex {
    /// The index of `entries` at the tip `tip`. The entries are taken in path order (the order the stores
    /// give them), which is the order candidates are listed in and the first of several equal matches is
    /// chosen in.
    pub fn new(tip: impl Into<String>, mut entries: Vec<IndexEntry>) -> Self {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries.dedup_by(|a, b| a.path == b.path);
        let mut resolver = Index::new(PAGES_DIR);
        let mut by_path = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            by_path.insert(e.path.clone(), i);
            resolver.add(PageEntry {
                path: e.path.clone(),
                title: e.title.clone(),
                aliases: e.aliases.clone(),
                kind: e.kind.clone(),
            });
        }
        TitleIndex {
            tip: tip.into(),
            entries,
            by_path,
            resolver,
        }
    }

    /// The tip's state id.
    pub fn tip(&self) -> &str {
        &self.tip
    }

    /// Every page, by path.
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    /// The page at `path`.
    pub fn entry(&self, path: &str) -> Option<&IndexEntry> {
        self.by_path.get(path).and_then(|i| self.entries.get(*i))
    }

    /// The resolver links are resolved with (MKUP-5), over these pages.
    pub fn resolver(&self) -> &Index {
        &self.resolver
    }

    /// Where `requested` leads: the page, through an alias or a redirect when it is one, or why not.
    pub fn lookup(&self, requested: &str) -> Lookup {
        let requested = nfc_trim(requested);
        let first = self.resolver.resolve(&requested, None);
        let via = first.via;
        let mut at = match self.resolved_entry(&first) {
            Ok(Some(e)) => e,
            Ok(None) => return Lookup::Missing,
            Err(candidates) => return Lookup::Ambiguous { candidates },
        };
        let first_hop = at.clone();
        let mut redirected = via == Via::Alias;
        let mut seen = vec![at.path.as_str()];
        while let Some(target) = at.redirect.as_deref() {
            let r = self.resolver.resolve(target, Some(&at.path));
            let next = match self.resolved_entry(&r) {
                Ok(Some(e)) => e,
                Ok(None) | Err(_) => {
                    return Lookup::BrokenRedirect {
                        from: first_hop.clone(),
                        problem: RedirectProblem::Dangling {
                            target: target.to_owned(),
                        },
                    };
                }
            };
            if seen.contains(&next.path.as_str()) {
                return Lookup::BrokenRedirect {
                    from: first_hop.clone(),
                    problem: RedirectProblem::Loop,
                };
            }
            if seen.len() > MAX_REDIRECT_HOPS {
                return Lookup::BrokenRedirect {
                    from: first_hop.clone(),
                    problem: RedirectProblem::TooLong,
                };
            }
            seen.push(next.path.as_str());
            redirected = true;
            at = next;
        }
        let by = if first_hop.path == at.path {
            RedirectedBy::Alias
        } else {
            RedirectedBy::Redirect
        };
        Lookup::Found(Found {
            entry: at.clone(),
            via,
            redirected_from: redirected.then(|| RedirectedFrom {
                title: requested.clone(),
                by,
            }),
        })
    }

    /// The entry a resolution points at: `Ok(None)` when it points at none, `Err` with the candidates when
    /// it is ambiguous.
    fn resolved_entry(
        &self,
        r: &inklkunn_markup::Resolution,
    ) -> Result<Option<&IndexEntry>, Vec<IndexEntry>> {
        match r.via {
            Via::Dangling | Via::Interwiki | Via::SamePage => Ok(None),
            Via::Ambiguous => Err(r
                .candidates
                .iter()
                .filter_map(|p| self.entry(p).cloned())
                .collect()),
            _ => Ok(r.path.as_deref().and_then(|p| self.entry(p))),
        }
    }
}

#[cfg(test)]
mod tests;
