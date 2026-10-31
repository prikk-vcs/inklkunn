//! `claims get`: fetch exactly the pinned revisions' wikitext into the cache.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::Opts;

use super::wire::{self, Fetcher, MAX_BATCH};
use super::{
    ArticleRevs, cache_root, deleted_path, parse_revisions, raw_path, read_text, write_file,
    write_json,
};

/// One article's counts.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Revisions the list pins.
    pub requested: usize,
    /// Of them, the ones whose text is in the cache.
    pub fetched: usize,
    /// Of them, the ones the API reported missing or hidden (skipped, never replaced).
    pub deleted_upstream: usize,
}

/// The report: per language and article.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// `lang` to title to counts.
    pub languages: BTreeMap<String, BTreeMap<String, Counts>>,
}

/// The command.
pub fn run(opts: &Opts) -> Result<(), String> {
    let revisions = opts.path("revisions")?;
    let report = opts.path("report")?;
    let cache = cache_root(opts)?;
    let articles = parse_revisions(&read_text(&revisions)?)?;
    let mut fetcher = wire::live_fetcher();
    let result = get(&articles, &cache, &mut fetcher)?;
    write_json(&report, &result)?;
    let total = result.languages.values().flat_map(BTreeMap::values);
    let (req, got, del) = total.fold((0, 0, 0), |(a, b, c), n| {
        (a + n.requested, b + n.fetched, c + n.deleted_upstream)
    });
    println!("requested {req}, fetched {got}, deleted upstream {del}");
    Ok(())
}

/// Fetch every pinned revision not yet in the cache (or marked deleted there), in batches of up to 50 per
/// language, and count the outcome. A revision the API does not return text for is marked deleted and skipped.
pub fn get(
    articles: &[ArticleRevs],
    cache: &Path,
    fetcher: &mut impl Fetcher,
) -> Result<Report, String> {
    // Pending revisions per language, with where each belongs: revision ids are the wiki's, so unique per language.
    let mut pending: BTreeMap<&str, Vec<(u64, &ArticleRevs)>> = BTreeMap::new();
    for a in articles {
        for r in &a.revs {
            let done = raw_path(cache, &a.lang, &a.key, r.id).exists()
                || deleted_path(cache, &a.lang, &a.key, r.id).exists();
            if !done {
                pending.entry(a.lang.as_str()).or_default().push((r.id, a));
            }
        }
    }
    for (lang, list) in &pending {
        for chunk in list.chunks(MAX_BATCH) {
            let ids: Vec<u64> = chunk.iter().map(|(id, _)| *id).collect();
            let batch = wire::fetch_batch(fetcher, lang, &ids)?;
            for (id, a) in chunk {
                match batch.texts.get(id) {
                    Some(text) => write_file(&raw_path(cache, lang, &a.key, *id), text.as_bytes())?,
                    // Missing, hidden, or simply not returned: deleted upstream, never replaced.
                    None => write_file(&deleted_path(cache, lang, &a.key, *id), b"")?,
                }
            }
        }
    }
    let mut report = Report::default();
    for a in articles {
        let mut c = Counts {
            requested: a.revs.len(),
            ..Counts::default()
        };
        for r in &a.revs {
            if raw_path(cache, &a.lang, &a.key, r.id).exists() {
                c.fetched += 1;
            } else if deleted_path(cache, &a.lang, &a.key, r.id).exists() {
                c.deleted_upstream += 1;
            }
        }
        report
            .languages
            .entry(a.lang.clone())
            .or_default()
            .insert(a.title.clone(), c);
    }
    Ok(report)
}
