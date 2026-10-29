//! Measurement only (handoff 003 T6; internal design IQ-2; requirements PERF-3): what the search index costs
//! on a wiki of the size PERF-3 names (10,000 pages), the way the product builds and asks it: every title and
//! alias folded, stored in the SQLite store's FTS5 table with the trigram tokenizer in one transaction (the
//! cost paid once per tip change), and queries of the lengths that take the two paths (three characters or
//! more through the index, fewer by a scan of the tip's texts), in English and in Japanese.
//!
//! ```text
//! cargo run --release -p inklkunn-core --example measure_search -- [--pages N] [--wikis N]
//! ```
//!
//! `--wikis` stores that many tips' indexes in the same database first (the table is shared by every wiki and
//! tip the server holds), so the figures include the filtering by wiki and tip a real server does.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use inklkunn_core::read::fold;
use inklkunn_store::{
    DerivedStore, DisplayName, NewWiki, SearchDoc, SearchKind, Slug, SqliteStore, WikiStore,
};

const WORDS: &[&str] = &[
    "Getting",
    "Started",
    "Guide",
    "Handbook",
    "Café",
    "Société",
    "Release",
    "Notes",
    "Policy",
    "Review",
    "Archive",
    "Glossary",
    "Roadmap",
    "Design",
    "Install",
    "Upgrade",
    "Backup",
    "Restore",
    "Security",
    "Network",
    "Storage",
    "Account",
    "Billing",
    "Support",
    "Contact",
    "History",
    "Changes",
    "Search",
];
const KANJI: &[char] = &[
    '日', '本', '語', '入', '門', '設', '計', '運', '用', '手', '順', '書', '管', '理', '者', '向',
    'け', 'の', 'ウ', 'ィ', 'キ', 'ガ', 'イ', 'ド',
];

fn title(i: usize) -> String {
    if matches!(i % 5, 0) {
        // A Japanese title of four to ten characters.
        let len = 4 + i % 7;
        (0..len)
            .map(|k| {
                KANJI
                    .get((i * 7 + k * 13) % KANJI.len())
                    .copied()
                    .unwrap_or('日')
            })
            .collect()
    } else {
        let w = |k: usize| WORDS.get(k % WORDS.len()).copied().unwrap_or("Page");
        format!("{} {} {i}", w(i), w(i / WORDS.len() + i))
    }
}

fn docs(pages: usize) -> Vec<SearchDoc> {
    let mut out = Vec::new();
    for i in 0..pages {
        let t = title(i);
        out.push(SearchDoc {
            path: format!("pages/{i}.md"),
            kind: SearchKind::Title,
            folded: fold(&t),
            text: t,
        });
        if matches!(i % 10, 0) {
            let alias = format!("Ａｌｉａｓ {i}");
            out.push(SearchDoc {
                path: format!("pages/{i}.md"),
                kind: SearchKind::Alias,
                folded: fold(&alias),
                text: alias,
            });
        }
    }
    out
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let at = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted.get(at).copied().unwrap_or_default()
}

fn run() -> Result<(), String> {
    let mut pages = 10_000usize;
    let mut wikis = 1usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--pages" => {
                pages = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--pages N")?
            }
            "--wikis" => {
                wikis = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--wikis N")?
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let dir = std::env::temp_dir().join(format!("inklkunn-measure-search-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db = dir.join("store.sqlite");
    let store = SqliteStore::open(&db).map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for n in 0..wikis {
        let record = store
            .create(NewWiki::new(
                Slug::new(&format!("wiki-{n}")).map_err(|e| e.to_string())?,
                DisplayName::new("Wiki").map_err(|e| e.to_string())?,
            ))
            .map_err(|e| e.to_string())?;
        ids.push(record.id);
    }
    let texts = docs(pages);
    let tip = |n: usize| format!("{n:064x}");
    println!(
        "{pages} pages, {} texts (titles and aliases), {wikis} wiki(s) in one database",
        texts.len()
    );

    // Folding.
    let t0 = Instant::now();
    let folded = docs(pages).len();
    println!("fold + generate {folded} texts: {:?}", t0.elapsed());

    // Building: the cost of one tip change.
    let mut builds = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        let t0 = Instant::now();
        store
            .put_search_index(id, &tip(n), &texts, 1)
            .map_err(|e| e.to_string())?;
        builds.push(t0.elapsed());
    }
    builds.sort();
    println!(
        "build (one tip, one transaction): median {:?}, max {:?}",
        percentile(&builds, 0.5),
        percentile(&builds, 1.0)
    );
    let size = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0)
        + std::fs::metadata(dir.join("store.sqlite-wal"))
            .map(|m| m.len())
            .unwrap_or(0);
    println!(
        "database after the builds: {:.1} MiB",
        size as f64 / 1_048_576.0
    );

    // Replacing the index of a tip that moved: the old one is dropped in the same transaction.
    let t0 = Instant::now();
    if let Some(id) = ids.first() {
        store
            .put_search_index(id, &tip(0), &texts, 2)
            .map_err(|e| e.to_string())?;
    }
    println!("rebuild over an existing index: {:?}", t0.elapsed());

    // Queries.
    let Some(id) = ids.last() else {
        return Err("no wiki".into());
    };
    let at = tip(ids.len() - 1);
    // Parts of a Japanese title that exists in the corpus (the titles are generated).
    let jp: Vec<char> = title(5).chars().collect();
    let part = |from: usize, len: usize| -> String { jp.iter().skip(from).take(len).collect() };
    let (jp3, jp2, jp1) = (part(1, 3), part(1, 2), part(1, 1));
    for (label, query) in [
        ("3+ characters, English  ", "start"),
        ("3+ characters, folded   ", "cafe soc"),
        ("3+ characters, Japanese ", jp3.as_str()),
        ("3+ characters, no match ", "zzzzzz"),
        ("1 character, Japanese   ", jp1.as_str()),
        ("2 characters, Japanese  ", jp2.as_str()),
        ("2 characters, common    ", "ar"),
    ] {
        let q = fold(query);
        let mut times = Vec::new();
        let mut hits = 0;
        for _ in 0..200 {
            let t0 = Instant::now();
            let found = store
                .search(id, &at, &q, 400)
                .map_err(|e| e.to_string())?
                .map_or(0, |h| h.len());
            times.push(t0.elapsed());
            hits = found;
        }
        times.sort();
        println!(
            "query {label} {query:<10} hits {hits:>4}  p50 {:>9?}  p95 {:>9?}  max {:>9?}",
            percentile(&times, 0.5),
            percentile(&times, 0.95),
            percentile(&times, 1.0)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("measure_search: {e}");
            ExitCode::FAILURE
        }
    }
}
