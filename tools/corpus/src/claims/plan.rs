//! `claims plan`: pin the newest N revisions of each listed article (ids and parent ids only).

use std::path::Path;

use crate::Opts;

use super::wire::{self, Fetcher};
use super::{Article, cache_root, parse_articles, read_text, slug, write_file};

/// The command.
pub fn run(opts: &Opts) -> Result<(), String> {
    let articles = opts.path("articles")?;
    let out = opts.path("out")?;
    let window = opts.num("window", None)?;
    let cache = cache_root(opts)?;
    let list = parse_articles(&read_text(&articles)?)?;
    let mut fetcher = wire::live_fetcher();
    let text = plan(
        &list,
        usize::try_from(window).map_err(|e| e.to_string())?,
        &cache,
        &mut fetcher,
    )?;
    write_file(&out, text.as_bytes())?;
    println!("planned {} articles into {}", list.len(), out.display());
    Ok(())
}

/// The `revisions.txt` text for the articles: per article its newest `window` revisions, oldest first. An answer
/// already in the cache is reused, so a repeated plan asks nothing and a pinned list does not drift.
pub fn plan(
    articles: &[Article],
    window: usize,
    cache: &Path,
    fetcher: &mut impl Fetcher,
) -> Result<String, String> {
    if window == 0 {
        return Err("`--window` must be at least 1".to_owned());
    }
    let mut out = String::new();
    for a in articles {
        let cached = cache
            .join("plan")
            .join(&a.lang)
            .join(format!("{}-{window}.tsv", slug(&a.title)));
        let revs = match std::fs::read_to_string(&cached) {
            Ok(text) => read_pairs(&text)?,
            Err(_) => {
                let revs = newest(a, window, fetcher)?;
                write_file(&cached, write_pairs(&revs).as_bytes())?;
                revs
            }
        };
        for (id, parent) in revs {
            out.push_str(&format!("{}\t{}\t{id}\t{parent}\n", a.lang, a.title));
        }
    }
    Ok(out)
}

/// The article's newest `window` revisions, oldest first.
fn newest(
    a: &Article,
    window: usize,
    fetcher: &mut impl Fetcher,
) -> Result<Vec<(u64, u64)>, String> {
    let mut got: Vec<(u64, u64)> = Vec::new();
    let mut cont: Vec<(String, String)> = Vec::new();
    while got.len() < window {
        let want = window - got.len();
        let url = wire::api_url(&a.lang, &wire::plan_params(&a.title, want, &cont))?;
        let page = wire::parse_plan_page(&fetcher.fetch(&url)?)
            .map_err(|e| format!("{} ({}): {e}", a.title, a.lang))?;
        if page.missing {
            return Err(format!(
                "{} ({}): the article does not exist",
                a.title, a.lang
            ));
        }
        got.extend(page.revisions);
        match page.next {
            Some(next) => cont = next,
            None => break,
        }
    }
    got.truncate(window);
    got.reverse();
    if got.len() < window {
        eprintln!(
            "inklkunn-corpus: {} ({}) has only {} revisions (window {window})",
            a.title,
            a.lang,
            got.len()
        );
    }
    Ok(got)
}

fn write_pairs(revs: &[(u64, u64)]) -> String {
    revs.iter().map(|(i, p)| format!("{i}\t{p}\n")).collect()
}

fn read_pairs(text: &str) -> Result<Vec<(u64, u64)>, String> {
    text.lines()
        .map(|l| {
            let (i, p) = l.split_once('\t').ok_or("a cached plan line has no tab")?;
            Ok((
                i.parse().map_err(|_| "bad cached revision id")?,
                p.parse().map_err(|_| "bad cached parent id")?,
            ))
        })
        .collect()
}
