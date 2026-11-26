#![allow(clippy::expect_used, clippy::unwrap_used)]
//! RFC 007 acceptance criterion 12 (handoff I-T3): **the cost of an import with its history, measured.** Not a check: a measurement, so the test
//! is `#[ignore]`d (no `skipping:` line in a normal run, no `EXPECTED_SKIPS`), and it prints its numbers.
//!
//! ```text
//! PATH=$HOME/.pgtmp/rel-0.58.0/x:$PATH TMPDIR=<a directory on a disk, not a tmpfs> \
//!   INKLKUNN_MEASURE_SIZES=500,2000 \
//!   cargo test -p inklkunn-core --locked --test import_measure -- --ignored --nocapture an_import_of_a_long_history
//! ```
//!
//! `INKLKUNN_MEASURE_SIZES` (revisions to replay, comma separated; default `500`). The bundle is made by this file from synthetic pages
//! (twenty pages of about 2 KiB, edited in turn, each revision a block, two authors alternating, minutes apart), so the plan reads it and
//! the replay writes it exactly as it does a converter's output. **What is printed** is the time of the plan, the time per block (the mean of
//! the first twenty-five blocks, of each hundred, of the last twenty-five), the time the end takes (the verification of the whole repository,
//! the report, the record and every act stored), and the total: the cost of a block grows with the log (PK-50), which is the point.

#[path = "support/replay.rs"]
mod replay;

use std::cell::RefCell;
use std::path::Path;
use std::time::{Duration, Instant};

use inklkunn_core::import::bundle::{BundleOptions, ImportStep, plan_bundle};
use inklkunn_core::read::ReadConfig;
use replay::{Dir, dir_in, prikk_on_path, run, world};

const PAGES: usize = 20;

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn body(page: usize, revision: usize) -> String {
    let mut text = format!(
        "---\nid: p-{page:02}\ntitle: Page {page:02}\n---\n\nPage {page:02}, revision {revision}.\n\n"
    );
    for line in 0..30 {
        text.push_str(&format!(
            "Line {line} of page {page:02}: a sentence of about this length, so that a page is a couple of kilobytes. ({revision})\n"
        ));
    }
    text
}

/// A bundle of `n` revisions over [`PAGES`] pages: no checksums, a licence in the manifest.
fn bundle(n: usize) -> Dir {
    let d = dir_in(&std::env::temp_dir(), &format!("measure-bundle-{n}"));
    put(
        &d.0,
        "wiki-bundle.yaml",
        "format: portable-wiki-bundle\nversion: '0.4'\nlicense: CC0-1.0\nsource:\n  name: Measured Wiki\n",
    );
    // By page: its lines, the revision before, and its text now.
    let mut pages: std::collections::BTreeMap<usize, (Vec<String>, Option<usize>, String)> =
        std::collections::BTreeMap::new();
    for revision in 0..n {
        let page = revision % PAGES;
        let text = body(page, revision);
        let minutes = revision;
        let at = format!(
            "2020-{:02}-{:02}T{:02}:{:02}:00Z",
            1 + (minutes / (60 * 24 * 28)) % 12,
            1 + (minutes / (60 * 24)) % 28,
            (minutes / 60) % 24,
            minutes % 60
        );
        let entry = pages.entry(page).or_default();
        let parent = entry.1.map(|p| format!("r{p}"));
        let line = serde_json::json!({
            "rev": format!("r{revision}"),
            "parent": parent,
            "type": if parent.is_none() { "create" } else { "edit" },
            "at": at,
            "author": {"name": if revision % 2 == 0 { "Ada" } else { "Bo" }, "kind": "person"},
            "summary": format!("edit {revision}"),
            "content": text,
        });
        entry.0.push(line.to_string());
        entry.1 = Some(revision);
        entry.2 = text;
    }
    for (page, (lines, _, text)) in pages {
        put(
            &d.0,
            &format!("history/p-{page:02}.jsonl"),
            &(lines.join("\n") + "\n"),
        );
        put(&d.0, &format!("pages/Page {page:02}.md"), &text);
    }
    d
}

fn mean(times: &[Duration]) -> f64 {
    if times.is_empty() {
        return 0.0;
    }
    times.iter().map(Duration::as_secs_f64).sum::<f64>() / times.len() as f64
}

#[test]
#[ignore = "a measurement, not a check: run it by name with --ignored"]
fn an_import_of_a_long_history_costs_per_block_what_it_costs() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let sizes: Vec<usize> = std::env::var("INKLKUNN_MEASURE_SIZES")
        .unwrap_or_else(|_| "500".to_owned())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    for n in sizes {
        let source = bundle(n);
        let started = Instant::now();
        let options = BundleOptions {
            max_revisions: 1_000_000,
            ..BundleOptions::default()
        };
        let plan = plan_bundle(&source.0, &ReadConfig::default(), &options).expect("plan");
        assert!(plan.is_importable(), "{}", plan.render());
        let planned = started.elapsed();
        let w = world(&format!("measure-{n}"));
        let times: RefCell<Vec<(ImportStep, Instant)>> = RefCell::new(Vec::new());
        let begun = Instant::now();
        let done = run(&w, &plan, "measured", "CC0-1.0", &|s| {
            times.borrow_mut().push((s, Instant::now()));
        })
        .expect("imported");
        let total = begun.elapsed();
        let times = times.into_inner();
        // The time of each block: from the step before it to the step after it.
        let mut blocks: Vec<Duration> = Vec::new();
        let mut before = times
            .iter()
            .find(|(s, _)| *s == ImportStep::Staged)
            .map(|(_, t)| *t)
            .unwrap_or(begun);
        for (s, t) in &times {
            if let ImportStep::Revision(_) = s {
                blocks.push(t.duration_since(before));
                before = *t;
            }
        }
        let last_step = times
            .iter()
            .rev()
            .find(|(s, _)| matches!(s, ImportStep::Revision(_)))
            .map(|(_, t)| *t)
            .unwrap_or(begun);
        let end = times
            .iter()
            .find(|(s, _)| *s == ImportStep::Stored)
            .map(|(_, t)| t.duration_since(last_step));
        println!(
            "MEASURE n={n} revisions={} blocks={} plan={:.2}s total={:.1}s end(after the last block: final state, verify, report, store)={:.1}s",
            done.revisions,
            blocks.len(),
            planned.as_secs_f64(),
            total.as_secs_f64(),
            end.map_or(0.0, |d| d.as_secs_f64()),
        );
        println!(
            "MEASURE n={n} per block: first 25 = {:.3}s, last 25 = {:.3}s, mean = {:.3}s",
            mean(blocks.get(..25).unwrap_or(&blocks)),
            mean(blocks.get(blocks.len().saturating_sub(25)..).unwrap_or(&[])),
            mean(&blocks)
        );
        // One line per tenth of the way, to see the growth.
        let step = (blocks.len() / 10).max(1);
        let mut at = 0;
        while at < blocks.len() {
            let end = (at + step).min(blocks.len());
            println!(
                "MEASURE n={n}   blocks {}..{}: {:.3}s per block",
                at + 1,
                end,
                mean(blocks.get(at..end).unwrap_or(&[]))
            );
            at = end;
        }
        assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
    }
}
