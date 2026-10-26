//! Free-link resolution by the MKUP-5 order against an index the caller supplies.
//!
//! The order, as the wiki-commons reference resolver states it: an empty target is the page itself;
//! a declared interwiki prefix yields a URL; a `./` or `../` target is a path relative to the linking
//! page; a leading `/` is dropped; then exact title or path, alias, case-insensitive, normalized (NFC,
//! separator runs collapsed, lower-cased), and for a bare title the hierarchy fallback (a unique
//! basename anywhere). Several hits prefer the page in the linking page's directory; otherwise the first
//! in index order is chosen and the ambiguity is reported, never hidden (NAV-15).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::text::{nfc_trim, normalize, percent_encode, relative_target, strip_md};

/// A page as the resolver sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEntry {
    /// Content-root-relative path with `.md`, `/`-separated.
    pub path: String,
    /// The title (frontmatter, or the file name).
    pub title: String,
    /// Other titles that resolve here (META-4).
    pub aliases: Vec<String>,
    /// `page`, `redirect`, `discussion`, … (META-7).
    pub kind: String,
}

impl PageEntry {
    /// A page with no aliases of kind `page`.
    pub fn new(path: impl Into<String>, title: impl Into<String>) -> Self {
        PageEntry {
            path: path.into(),
            title: title.into(),
            aliases: Vec::new(),
            kind: "page".to_owned(),
        }
    }

    fn directory(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(d, _)| d)
    }

    fn basename(&self) -> &str {
        strip_md(
            self.path
                .rsplit_once('/')
                .map_or(self.path.as_str(), |(_, b)| b),
        )
    }
}

/// How a link was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    /// Declared interwiki prefix; see `url`.
    Interwiki,
    /// Exact title or path.
    Exact,
    /// A declared alias.
    Alias,
    /// Case-insensitive title or path.
    Case,
    /// After NFC, separator collapsing and case folding (title, path or alias).
    Normalized,
    /// A bare title that is the basename of exactly one page somewhere in the hierarchy.
    Hierarchy,
    /// A `./` or `../` path from the linking page.
    Relative,
    /// An empty target: the page itself.
    Self_,
    /// Several pages match; `path` is the first and `candidates` lists them all.
    Ambiguous,
    /// No page.
    Dangling,
}

impl Via {
    /// The lower-case name used in reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Via::Interwiki => "interwiki",
            Via::Exact => "exact",
            Via::Alias => "alias",
            Via::Case => "case",
            Via::Normalized => "normalized",
            Via::Hierarchy => "hierarchy",
            Via::Relative => "relative",
            Via::Self_ => "self",
            Via::Ambiguous => "ambiguous",
            Via::Dangling => "dangling",
        }
    }
}

impl std::fmt::Display for Via {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The outcome of resolving one target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolution {
    /// The page path, when a page was found.
    pub path: Option<String>,
    /// The rule that decided.
    pub via: Via,
    /// For interwiki links, the expanded URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// For ambiguous links, every matching path in index order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

impl Resolution {
    /// Does the link lead anywhere?
    pub fn resolved(&self) -> bool {
        self.path.is_some() || self.url.is_some()
    }

    fn page(path: &str, via: Via) -> Self {
        Resolution {
            path: Some(path.to_owned()),
            via,
            url: None,
            candidates: Vec::new(),
        }
    }

    fn dangling() -> Self {
        Resolution {
            path: None,
            via: Via::Dangling,
            url: None,
            candidates: Vec::new(),
        }
    }
}

/// The page index free links resolve against.
#[derive(Debug, Clone, Default)]
pub struct Index {
    pages: Vec<PageEntry>,
    /// MediaWiki-style namespace prefixes that are titles, not interwiki (MKUP-7).
    pub namespaces: BTreeSet<String>,
    /// Interwiki prefix → URL template with `{title}` (MKUP-13).
    pub interwiki: BTreeMap<String, String>,
    pages_prefix: String,
    exact: BTreeMap<String, Vec<usize>>,
    alias: BTreeMap<String, Vec<usize>>,
    case: BTreeMap<String, Vec<usize>>,
    norm: BTreeMap<String, Vec<usize>>,
    base: BTreeMap<String, Vec<usize>>,
}

impl Index {
    /// An empty index. `pages_prefix` names a directory (`pages`) stripped when a path is used as a title,
    /// so that `[[Guides/Getting Started]]` matches `pages/Guides/Getting Started.md`.
    pub fn new(pages_prefix: &str) -> Self {
        let p = pages_prefix.trim_end_matches('/');
        Index {
            pages_prefix: if p.is_empty() {
                String::new()
            } else {
                format!("{p}/")
            },
            ..Index::default()
        }
    }

    /// The pages in index order.
    pub fn pages(&self) -> &[PageEntry] {
        &self.pages
    }

    /// The path without `.md` and without the pages prefix.
    pub fn logical_path<'a>(&self, path: &'a str) -> &'a str {
        let p = strip_md(path);
        if !self.pages_prefix.is_empty() {
            p.strip_prefix(self.pages_prefix.as_str()).unwrap_or(p)
        } else {
            p
        }
    }

    /// Add a page.
    pub fn add(&mut self, page: PageEntry) {
        let i = self.pages.len();
        let mut keys: Vec<String> = vec![
            nfc_trim(&page.title),
            nfc_trim(self.logical_path(&page.path)),
            nfc_trim(strip_md(&page.path)),
        ];
        keys.sort();
        keys.dedup();
        for key in keys {
            self.case.entry(key.to_lowercase()).or_default().push(i);
            self.norm.entry(normalize(&key)).or_default().push(i);
            self.exact.entry(key).or_default().push(i);
        }
        for a in &page.aliases {
            self.alias.entry(nfc_trim(a)).or_default().push(i);
            self.norm.entry(normalize(a)).or_default().push(i);
        }
        self.base
            .entry(normalize(page.basename()))
            .or_default()
            .push(i);
        self.pages.push(page);
    }

    /// The page at `path`.
    pub fn by_path(&self, path: &str) -> Option<&PageEntry> {
        self.pages.iter().find(|p| p.path == path)
    }

    /// Resolve `target` as written in a link on `from_path`.
    pub fn resolve(&self, target: &str, from_path: Option<&str>) -> Resolution {
        let mut target = nfc_trim(target);
        if target.is_empty() {
            return Resolution {
                path: from_path.map(str::to_owned),
                via: Via::Self_,
                url: None,
                candidates: Vec::new(),
            };
        }
        if !target.starts_with('/')
            && !target.starts_with('.')
            && let Some((prefix, rest)) = target.split_once(':')
            && let Some(template) = self.interwiki.get(prefix)
        {
            return Resolution {
                path: None,
                via: Via::Interwiki,
                url: Some(template.replace("{title}", &percent_encode(rest.trim()))),
                candidates: Vec::new(),
            };
        }
        if (target.starts_with("./") || target.starts_with("../"))
            && let Some(from) = from_path
        {
            let cand = relative_target(from, &target);
            return match self
                .pages
                .iter()
                .find(|p| strip_md(&p.path) == cand || self.logical_path(&p.path) == cand)
            {
                Some(p) => Resolution::page(&p.path, Via::Relative),
                None => Resolution::dangling(),
            };
        }
        if let Some(t) = target.strip_prefix('/') {
            target = t.to_owned();
        }
        if let Some(hits) = self.exact.get(&target) {
            return self.pick(hits, Via::Exact, from_path);
        }
        if let Some(hits) = self.alias.get(&target) {
            return self.pick(hits, Via::Alias, from_path);
        }
        if let Some(hits) = self.case.get(&target.to_lowercase()) {
            return self.pick(hits, Via::Case, from_path);
        }
        if let Some(hits) = self.norm.get(&normalize(&target)) {
            return self.pick(hits, Via::Normalized, from_path);
        }
        if !target.contains('/')
            && let Some(hits) = self.base.get(&normalize(&target))
        {
            return self.pick(hits, Via::Hierarchy, from_path);
        }
        Resolution::dangling()
    }

    fn pick(&self, hits: &[usize], via: Via, from_path: Option<&str>) -> Resolution {
        let mut uniq: Vec<&PageEntry> = Vec::new();
        for &h in hits {
            if let Some(p) = self.pages.get(h)
                && !uniq.iter().any(|u| u.path == p.path)
            {
                uniq.push(p);
            }
        }
        let [only] = uniq.as_slice() else {
            if let Some(from) = from_path {
                let here = from.rsplit_once('/').map_or("", |(d, _)| d);
                let same: Vec<&&PageEntry> =
                    uniq.iter().filter(|p| p.directory() == here).collect();
                if let [one] = same.as_slice() {
                    return Resolution::page(&one.path, via);
                }
            }
            return match uniq.first() {
                Some(first) => Resolution {
                    path: Some(first.path.clone()),
                    via: Via::Ambiguous,
                    url: None,
                    candidates: uniq.iter().map(|p| p.path.clone()).collect(),
                },
                None => Resolution::dangling(),
            };
        };
        Resolution::page(&only.path, via)
    }
}

/// Resolve with a bare index (convenience for callers holding one).
pub fn resolve(index: &Index, target: &str, from_path: Option<&str>) -> Resolution {
    index.resolve(target, from_path)
}

#[cfg(test)]
mod tests;
