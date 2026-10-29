//! Measurement only (handoff 003 T3; requirements PERF-1): how long the read path takes on a real repository,
//! cold and warm, with the title index built the way the product builds it (one `extract` of the tip, the
//! first bytes of every page read, the index stored) and not by a stand-in.
//!
//! ```text
//! cargo run --release -p inklkunn-core --example measure_index -- REPO [--runs N] [--confined]
//! ```
//!
//! `REPO` is a prikk repository (the wiki-shaped corpora of `tools/corpus` are the ones the cost table was
//! measured on). It is linked, not copied or changed, into a scratch root under the system temp directory
//! and read only. Each run starts with nothing held: a new service and an empty store. Printed, per run, in
//! seconds: the tip (`branch list`), the title index (`extract`, the tree walk, the store), the first page
//! on top of it (`cat`, parse, resolve, render, sanitize), the same page cold in one go on a service of its
//! own, and the percentiles of repeated hits (PERF-1: a cached view under 100 ms at p95, a cold page under
//! 1 s on a wiki of 10,000 pages). `--confined` runs prikk under bubblewrap as the product does.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use inklkunn_core::read::{TransclusionRefusal, entries_from_tree};
use inklkunn_core::{CountingStorage, PageOutcome, Presentation, ReadConfig, ReadService};
use inklkunn_prikk::{
    Admission, ExtractBounds, Point, PrikkCli, Sandbox, SizeHints, Storage, cost,
};
use inklkunn_store::{
    DisplayName, NewWiki, RootLayout, Slug, SqliteStore, WikiId, WikiRecord, WikiStore,
};

struct En;

impl Presentation for En {
    fn lang(&self) -> &str {
        "en"
    }
    fn key(&self) -> String {
        "/w".to_owned()
    }
    fn page_href(&self, title: &str) -> String {
        format!("/w/{}", inklkunn_markup::percent_encode(title))
    }
    fn create_href(&self, title: &str) -> Option<String> {
        Some(format!(
            "/w/-/create?title={}",
            inklkunn_markup::percent_encode(title)
        ))
    }
    fn transclusion_label(&self, title: &str) -> String {
        format!("From {title}")
    }
    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String {
        format!("Not included: {refusal}")
    }
}

fn next_db() -> usize {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let repo: PathBuf = args
        .next()
        .ok_or("usage: measure_index REPO [--runs N] [--confined]")?
        .into();
    let mut runs = 3usize;
    let mut sandbox = Sandbox::Unconfined;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => {
                runs = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or("--runs needs a number")?;
            }
            "--confined" => sandbox = Sandbox::default(),
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    let repo = repo
        .canonicalize()
        .map_err(|e| format!("{}: {e}", repo.display()))?;

    // A root of our own whose one wiki's directory is a link to the repository.
    let root = std::env::temp_dir().join(format!("inklkunn-measure-{}", std::process::id()));
    let layout = RootLayout::new(&root);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    // The product's store: SQLite in a file, WAL, so that the cost of writing 10,000 index rows and of reading
    // a cached page back is in the numbers.
    let store = Arc::new(SqliteStore::open(&root.join("store.sqlite")).map_err(|e| e.to_string())?);
    let record = WikiRecord::from_new(
        WikiId::new(),
        NewWiki::new(
            Slug::new("measured").map_err(|e| e.to_string())?,
            DisplayName::new("Measured").map_err(|e| e.to_string())?,
        ),
        0,
    )
    .map_err(|e| e.to_string())?;
    let record = store.insert(record).map_err(|e| e.to_string())?;
    let linked = layout.repo_dir(&record.id);
    std::fs::create_dir_all(linked.parent().ok_or("no parent")?).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&repo, &linked).map_err(|e| e.to_string())?;

    let hints = SizeHints::new();
    let driver = PrikkCli::open(
        "prikk",
        sandbox,
        // A budget that holds the largest corpus: the admission arithmetic is not what is measured here.
        Admission::new(16 << 30, 256 << 20, Duration::from_secs(5)),
    )
    .map_err(|e| e.to_string())?
    .with_size_hints(hints.clone());
    println!("{}", driver.version());
    let counting = Arc::new(CountingStorage::new(driver));
    let store_bytes = cost::store_size(&repo).map_err(|e| e.to_string())?;
    let service = |ttl: Duration| {
        // Every run's services start with an empty derived cache: a fresh database file each time.
        let fresh = Arc::new(
            SqliteStore::open(&root.join(format!("derived-{}.sqlite", next_db())))
                .unwrap_or_else(|e| panic!("{e}")),
        );
        ReadService::new(
            counting.clone() as Arc<dyn Storage>,
            fresh,
            store.clone(),
            layout.clone(),
            ReadConfig {
                tip_ttl: ttl,
                ..ReadConfig::default()
            },
            Some(hints.clone()),
        )
    };

    println!(
        "repository {}  store {:.1} MiB",
        repo.display(),
        store_bytes as f64 / 1_048_576.0
    );
    println!(
        "run  pages  extract(s)  walk+parse(s)  tip(s)  index(s)  first-page(s)  cold-page(s)  hit p50(ms)  hit p95(ms)  hit max(ms)  calls"
    );
    for run in 1..=runs {
        // The two halves of the index build on their own: prikk's `extract` of the tip, then reading the first
        // bytes of every page file from the extracted tree.
        let scratch = layout.scratch_dir(&record.id);
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        let name = format!("parts-{run}");
        let t = Instant::now();
        counting
            .materialize(
                &linked,
                &Point::View(record.default_view.clone()),
                &scratch,
                &name,
                ExtractBounds {
                    max_bytes: None,
                    max_files: None,
                },
            )
            .map_err(|e| e.to_string())?;
        let extract_time = t.elapsed();
        let t = Instant::now();
        let parsed =
            entries_from_tree(&scratch.join(&name), u64::MAX).map_err(|e| e.to_string())?;
        let parse_time = t.elapsed();
        let _ = std::fs::remove_dir_all(scratch.join(&name));
        if parsed.is_empty() {
            return Err("the extracted tree has no pages".to_owned());
        }
        // The parts, on a service with nothing in it.
        let s = service(Duration::from_secs(3600));
        let t = Instant::now();
        s.tip(&record, None).map_err(|e| e.to_string())?;
        let tip_time = t.elapsed();
        let t = Instant::now();
        let index = s.title_index(&record, None).map_err(|e| e.to_string())?;
        let index_time = t.elapsed();
        let pages = index.entries().len();
        let title = index
            .entries()
            .get(pages / 2)
            .map(|e| e.title.clone())
            .ok_or("the repository has no pages")?;
        let t = Instant::now();
        let first = s
            .page(&record, None, &title, &En)
            .map_err(|e| e.to_string())?;
        let first_time = t.elapsed();
        if !matches!(first, PageOutcome::Found(_)) {
            return Err(format!("`{title}` was not found"));
        }

        // The same page cold in one go: nothing held, nothing cached, the store empty.
        let before = counting.calls();
        let cold = service(Duration::from_secs(3600));
        let t = Instant::now();
        let outcome = cold
            .page(&record, None, &title, &En)
            .map_err(|e| e.to_string())?;
        let cold_time = t.elapsed();
        let calls = counting.calls().since(&before);
        if !matches!(outcome, PageOutcome::Found(_)) {
            return Err(format!("`{title}` was not found cold"));
        }

        // Repeated hits.
        let mut hits: Vec<Duration> = Vec::new();
        for _ in 0..400 {
            let t = Instant::now();
            let _ = cold
                .page(&record, None, &title, &En)
                .map_err(|e| e.to_string())?;
            hits.push(t.elapsed());
        }
        hits.sort();
        let at = |q: f64| -> f64 {
            let i = ((hits.len() as f64 - 1.0) * q).round() as usize;
            hits.get(i).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
        };
        println!(
            "{run:>3}  {pages:>5}  {:>10.3}  {:>13.3}  {:>6.3}  {:>8.3}  {:>13.3}  {:>12.3}  {:>11.2}  {:>11.2}  {:>11.2}  views {} extract {} cat {}",
            secs(extract_time),
            secs(parse_time),
            secs(tip_time),
            secs(index_time),
            secs(first_time),
            secs(cold_time),
            at(0.5),
            at(0.95),
            at(1.0),
            calls.views,
            calls.materialize,
            calls.read,
        );
    }
    // Handoff 003 T6: what the surfaces built on the same tip cost. Each on a service with nothing in it but
    // the title index, so the first of each pays for itself (an extract for the link graph; the search index
    // is built from the title index in memory; the changes are the log and one `show` for each change listed).
    {
        let s = service(Duration::from_secs(3600));
        s.title_index(&record, None).map_err(|e| e.to_string())?;
        let time =
            |what: &str, f: &mut dyn FnMut() -> Result<String, String>| -> Result<(), String> {
                let t = Instant::now();
                let note = f()?;
                println!("T6 {what:<44} {:>8.3} s  {note}", secs(t.elapsed()));
                Ok(())
            };
        time("search, first query (builds the index)", &mut || {
            let o = s
                .search(&record, None, "page 12")
                .map_err(|e| e.to_string())?;
            Ok(format!("{} hits", o.hits.len()))
        })?;
        time("search, next query", &mut || {
            let o = s.search(&record, None, "ab").map_err(|e| e.to_string())?;
            Ok(format!("{} hits", o.hits.len()))
        })?;
        time("link graph, first use (one extract)", &mut || {
            let g = s.link_graph(&record, None).map_err(|e| e.to_string())?;
            Ok(format!(
                "{} pages, {} wanted, {} orphans, {} dead ends",
                g.pages().len(),
                g.wanted().len(),
                g.orphans().len(),
                g.dead_ends().len()
            ))
        })?;
        time("link graph, next use", &mut || {
            let g = s.link_graph(&record, None).map_err(|e| e.to_string())?;
            Ok(format!("{} pages", g.pages().len()))
        })?;
        time("recent changes, first (log + a show per row)", &mut || {
            let c = s
                .changes(&record, &inklkunn_core::read::ChangeFilter::default(), 50)
                .map_err(|e| e.to_string())?;
            Ok(format!("{} rows", c.rows.len()))
        })?;
        time("recent changes, next (shows remembered)", &mut || {
            let c = s
                .changes(&record, &inklkunn_core::read::ChangeFilter::default(), 50)
                .map_err(|e| e.to_string())?;
            Ok(format!("{} rows", c.rows.len()))
        })?;
        time("stale list (the whole window of shows)", &mut || {
            let c = s.stale_pages(&record).map_err(|e| e.to_string())?;
            Ok(format!(
                "{} pages, {} changes looked through",
                c.pages.len(),
                c.looked_through
            ))
        })?;
        time("raw source of one page", &mut || {
            let title = s
                .title_index(&record, None)
                .map_err(|e| e.to_string())?
                .entries()
                .first()
                .map(|e| e.title.clone())
                .ok_or("no page")?;
            let r = s.raw(&record, None, &title).map_err(|e| e.to_string())?;
            Ok(format!("{} bytes", r.map_or(0, |r| r.bytes.len())))
        })?;
    }
    let _ = std::fs::remove_file(&linked);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("measure_index: {e}");
            ExitCode::FAILURE
        }
    }
}
