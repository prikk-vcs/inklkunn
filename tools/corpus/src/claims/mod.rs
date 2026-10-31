//! Spike C (RFC 005 part 1): the claim-identity measurement. See `CLAIMS.md` for the pipeline.
//!
//! `plan` pins revision ids, `get` fetches exactly those, `convert` turns their wikitext into Portable Wiki
//! Markdown with wiki-commons' converter, `replay` walks each history through the product's own block functions,
//! `pool` draws the items the two labellers see, `measure` computes M-1 … M-6 from their labels.
//!
//! **What is committed and what is not.** Revision ids, counts, the pool (item, article, revision, block numbers),
//! the labels and the report are measurements and may be committed. Article text lives only in the cache
//! (`raw/`, `converted/`, the packets); the cache is refused when it is inside a repository. No editor identity is
//! requested, read or kept.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::Opts;

mod convert;
mod events;
mod get;
mod labels;
mod measure;
mod plan;
mod pool;
mod replay;
mod stats;
mod text;
mod wire;

#[cfg(test)]
mod tests;

/// Run one `claims` verb.
pub fn run(verb: &str, opts: &Opts) -> Result<(), String> {
    match verb {
        "plan" => plan::run(opts),
        "get" => get::run(opts),
        "convert" => convert::run(opts),
        "replay" => replay::run(opts),
        "pool" => pool::run(opts),
        "measure" => measure::run(opts),
        other => Err(format!("unknown `claims` verb `{other}`")),
    }
}

/// The languages the spike reads.
pub const LANGS: [&str; 2] = ["en", "ja"];

/// One line of the article list: `lang<TAB>title<TAB>reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    /// `en` or `ja`.
    pub lang: String,
    /// The title as the wiki spells it.
    pub title: String,
    /// Why the article was chosen (committed with the list).
    pub reason: String,
}

/// Read the article list (`#` comments and blank lines allowed).
pub fn parse_articles(text: &str) -> Result<Vec<Article>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let at = format!("article list line {}", n + 1);
        let mut f = line.split('\t');
        let (Some(lang), Some(title), Some(reason), None) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            return Err(format!("{at}: expected `lang<TAB>title<TAB>reason`"));
        };
        check_lang(lang).map_err(|e| format!("{at}: {e}"))?;
        if title.trim().is_empty() || reason.trim().is_empty() {
            return Err(format!("{at}: the title and the reason are required"));
        }
        out.push(Article {
            lang: lang.to_owned(),
            title: title.trim().to_owned(),
            reason: reason.trim().to_owned(),
        });
    }
    Ok(out)
}

/// `en` or `ja`, nothing else (the language names the host the fetcher talks to).
pub fn check_lang(lang: &str) -> Result<(), String> {
    if LANGS.contains(&lang) {
        Ok(())
    } else {
        Err(format!("language `{lang}` is not `en` or `ja`"))
    }
}

/// A revision a run is pinned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rev {
    /// The wiki's revision id.
    pub id: u64,
    /// Its parent's id (0 for the first revision of a page).
    pub parent: u64,
}

/// The pinned revisions of one article, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleRevs {
    /// The article's position in the file (the replay's id prefix).
    pub index: usize,
    /// `en` or `ja`.
    pub lang: String,
    /// The title.
    pub title: String,
    /// The filesystem-safe key of the title.
    pub key: String,
    /// The revisions, in file order.
    pub revs: Vec<Rev>,
}

/// Read `revisions.txt` (`lang<TAB>title<TAB>revid<TAB>parentid`, oldest first) into articles in order of first
/// appearance. Two titles with one key in one language are refused.
pub fn parse_revisions(text: &str) -> Result<Vec<ArticleRevs>, String> {
    let mut out: Vec<ArticleRevs> = Vec::new();
    let mut seen_ids: BTreeSet<(String, u64)> = BTreeSet::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let at = format!("revisions line {}", n + 1);
        let mut f = line.split('\t');
        let (Some(lang), Some(title), Some(id), Some(parent), None) =
            (f.next(), f.next(), f.next(), f.next(), f.next())
        else {
            return Err(format!(
                "{at}: expected `lang<TAB>title<TAB>revid<TAB>parentid`"
            ));
        };
        check_lang(lang).map_err(|e| format!("{at}: {e}"))?;
        let id: u64 = id.parse().map_err(|_| format!("{at}: bad revision id"))?;
        let parent: u64 = parent.parse().map_err(|_| format!("{at}: bad parent id"))?;
        if !seen_ids.insert((lang.to_owned(), id)) {
            return Err(format!("{at}: revision {id} is listed twice"));
        }
        let pos = match out.iter().position(|a| a.lang == lang && a.title == title) {
            Some(p) => p,
            None => {
                let key = slug(title);
                if out.iter().any(|a| a.lang == lang && a.key == key) {
                    return Err(format!("{at}: two titles share the key `{key}`"));
                }
                out.push(ArticleRevs {
                    index: out.len(),
                    lang: lang.to_owned(),
                    title: title.to_owned(),
                    key,
                    revs: Vec::new(),
                });
                out.len() - 1
            }
        };
        if let Some(a) = out.get_mut(pos) {
            a.revs.push(Rev { id, parent });
        }
    }
    Ok(out)
}

/// A filesystem-safe key of a title: letters and digits (any script) and `-` stay, every other run becomes one
/// `_`, at most 80 characters, never empty. It cannot be `.`, `..` or contain a separator.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() || c == '-' {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out: String = out.trim_matches('_').chars().take(80).collect();
    if out.is_empty() { "_".to_owned() } else { out }
}

/// The cache directory when none is given: `$XDG_CACHE_HOME/inklkunn-corpus/claims`, else
/// `~/.cache/inklkunn-corpus/claims`. A relative `XDG_CACHE_HOME` is ignored, as the XDG specification says.
pub fn default_cache(xdg: Option<&str>, home: Option<&str>) -> Result<PathBuf, String> {
    let base = match (xdg, home) {
        (Some(x), _) if Path::new(x).is_absolute() => PathBuf::from(x),
        (_, Some(h)) if Path::new(h).is_absolute() => Path::new(h).join(".cache"),
        _ => return Err("no cache directory: pass `--cache DIR`".to_owned()),
    };
    Ok(base.join("inklkunn-corpus").join("claims"))
}

/// Refuse a cache inside a repository: the nearest existing ancestor and every one above it must hold no `.git`
/// (a directory in a clone, a file in a worktree). Article text must never be able to reach a commit.
pub fn refuse_in_repository(cache: &Path) -> Result<(), String> {
    let mut dir = Some(cache);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return Err(format!(
                "the cache `{}` is inside a repository (`{}`); article text must stay outside any repository",
                cache.display(),
                d.display()
            ));
        }
        dir = d.parent();
    }
    Ok(())
}

/// The cache directory of a run: `--cache`, else the default; refused when inside a repository.
pub fn cache_root(opts: &Opts) -> Result<PathBuf, String> {
    let path = match opts.opt_path("cache") {
        Some(p) if p.is_absolute() => p,
        Some(_) => return Err("`--cache` must be absolute".to_owned()),
        None => default_cache(
            std::env::var("XDG_CACHE_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        )?,
    };
    refuse_in_repository(&path)?;
    Ok(path)
}

/// Where a revision's wikitext is cached.
pub fn raw_path(cache: &Path, lang: &str, key: &str, rev: u64) -> PathBuf {
    cache
        .join("raw")
        .join(lang)
        .join(key)
        .join(format!("{rev}.txt"))
}

/// The marker that a revision was reported missing or deleted upstream.
pub fn deleted_path(cache: &Path, lang: &str, key: &str, rev: u64) -> PathBuf {
    cache
        .join("raw")
        .join(lang)
        .join(key)
        .join(format!("{rev}.deleted"))
}

/// Where a revision's converted text is cached.
pub fn converted_path(cache: &Path, lang: &str, key: &str, rev: u64) -> PathBuf {
    cache
        .join("converted")
        .join(lang)
        .join(key)
        .join(format!("{rev}.md"))
}

/// Write a file (and its directories) by writing a sibling first and renaming it, so a stopped run leaves no
/// half-written file for the next run to trust.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{}: no parent directory", path.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Write a value as pretty JSON with a final newline (committed reports: sorted maps, so deterministic).
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let mut s = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    s.push('\n');
    write_file(path, s.as_bytes())
}

/// Read a whole file as text.
pub fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}
