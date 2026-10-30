//! Spike B (RFC 001 D-10 b; internal design MRG-4; roadmap R2): wiki-shaped edit histories, and three
//! questions about them.
//!
//! 1. **How often does a base move before acceptance?** A review-latency model: proposals arrive at a rate,
//!    each is accepted after a latency; the base has moved when another acceptance landed in between.
//! 2. **How often is a block-aware three-way merge clean?** Two edits from one baseline, merged by the
//!    product's own function (`inklkunn_core::merge`, RFC 004 D-5): the tip's side is edit A, the proposal's
//!    side is edit B. (Before the handoff 004 T2 the harness held its own block model here; the product
//!    function replaced it and agrees with it on all 120 pairs of the recorded run.)
//! 3. **Would prikk's own span anchors still apply?** The same two edits really sealed, one on `heads/main`
//!    and one on a branch, then `prikk merge --baseline-block` from the baseline: prikk's confluence
//!    verdict, verbatim.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use inklkunn_core::{PageText, merge};
use serde::{Deserialize, Serialize};

use crate::Opts;
use crate::prikk::Prikk;
use crate::wiki::{Rng, page_path, page_text, title};

const PAGES: usize = 40;
const PAGE_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Scenario {
    /// Both sides replace the same paragraph of the same page.
    SameParagraph,
    /// Different paragraphs of the same page.
    SamePageDifferentParagraphs,
    /// Different pages.
    DifferentPages,
    /// One side renames the page (`prikk mv`), the other edits a paragraph of it.
    RenameAndEdit,
    /// One side edits the frontmatter, the other a paragraph of the same page.
    FrontmatterAndBody,
}

impl Scenario {
    fn draw(rng: &mut Rng) -> Scenario {
        match rng.below(100) {
            0..=14 => Scenario::SameParagraph,
            15..=44 => Scenario::SamePageDifferentParagraphs,
            45..=79 => Scenario::DifferentPages,
            80..=89 => Scenario::RenameAndEdit,
            _ => Scenario::FrontmatterAndBody,
        }
    }
}

/// One side's edit, described so both the real repository and the block model can apply it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Edit {
    Paragraph {
        page: usize,
        index: usize,
        text: String,
    },
    Frontmatter {
        page: usize,
        tag: String,
    },
    Rename {
        page: usize,
        new_title: String,
    },
}

#[derive(Debug, Serialize)]
struct Trial {
    n: usize,
    scenario: Scenario,
    baseline: String,
    a: Edit,
    b: Edit,
    block_merge_clean: bool,
    /// Overlaps of this pair that a line-level merge would have made clean (the measurement hook; never applied).
    line_level_clean: usize,
    prikk_outcome: String,
    prikk_detail: String,
}

#[derive(Debug, Default, Serialize)]
struct Tally {
    trials: usize,
    block_clean: usize,
    /// Overlaps a line-level three-way merge of the block would have declared clean (RFC 004 D-5 hook).
    line_level_clean_overlaps: usize,
    prikk_confluent: usize,
    prikk_refusals: BTreeMap<String, usize>,
    /// (block model says clean, prikk says confluent) → count.
    agreement: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
struct LatencyRow {
    proposals_per_day: f64,
    mean_review_hours: f64,
    base_moved_fraction: f64,
}

#[derive(Debug, Serialize)]
struct Report {
    prikk_version: String,
    seed: u64,
    pages: usize,
    per_scenario: BTreeMap<String, Tally>,
    total: Tally,
    latency_model: Vec<LatencyRow>,
    trials: Vec<Trial>,
}

/// The generator's own addressing of a page's paragraphs: block 0 is the frontmatter, then blank-line
/// separated paragraphs and headings. It says which paragraph an edit replaces (`Edit::Paragraph::index`);
/// it is **not** the merge's block model, which is the product's (`inklkunn_markup::blocks`).
fn blocks(text: &str) -> Vec<String> {
    let (fm, body) = match text
        .strip_prefix("---\n")
        .and_then(|r| r.split_once("\n---\n"))
    {
        Some((fm, body)) => (format!("---\n{fm}\n---"), body),
        None => (String::new(), text),
    };
    let mut out = vec![fm.trim().to_owned()];
    out.extend(
        body.split("\n\n")
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_owned),
    );
    out
}

/// The wiki's pages as the merge sees them: path and text, in page order.
fn wiki_pages(titles: &[String], pages: &[String]) -> Vec<PageText> {
    titles
        .iter()
        .zip(pages)
        .map(|(t, p)| PageText::new(page_path(t).to_string_lossy(), p.clone()))
        .collect()
}

/// The three sides of the merge of two edits from one baseline: the baseline, the tip's (edit `a`) and the
/// proposal's (edit `b`).
fn sides(
    titles: &[String],
    pages: &[String],
    a: &Edit,
    b: &Edit,
) -> (Vec<PageText>, Vec<PageText>, Vec<PageText>) {
    let after = |edit: &Edit| {
        let mut t = titles.to_vec();
        let mut p = pages.to_vec();
        apply_in_memory(&mut p, edit);
        if let Edit::Rename { page, new_title } = edit
            && let Some(slot) = t.get_mut(*page)
        {
            *slot = new_title.clone();
        }
        wiki_pages(&t, &p)
    };
    (wiki_pages(titles, pages), after(a), after(b))
}
/// Apply an edit to page texts held in memory (`pages[i]` is page i's text).
fn apply_in_memory(pages: &mut [String], edit: &Edit) {
    match edit {
        Edit::Paragraph { page, index, text } => {
            if let Some(p) = pages.get_mut(*page) {
                let mut bs = blocks(p);
                if let Some(slot) = bs.get_mut(*index) {
                    *slot = text.clone();
                }
                *p = rejoin(&bs);
            }
        }
        Edit::Frontmatter { page, tag } => {
            if let Some(p) = pages.get_mut(*page) {
                *p = p.replacen("tags: [", &format!("tags: [{tag}, "), 1);
            }
        }
        Edit::Rename { .. } => {}
    }
}

fn rejoin(bs: &[String]) -> String {
    let mut out = String::new();
    for (i, b) in bs.iter().enumerate() {
        if i == 0 {
            out.push_str(b);
            out.push('\n');
        } else {
            out.push('\n');
            out.push_str(b);
            out.push('\n');
        }
    }
    out
}

/// The paragraph (non-heading) block indices of a page.
fn paragraph_indices(text: &str) -> Vec<usize> {
    blocks(text)
        .iter()
        .enumerate()
        .filter(|(i, b)| *i > 0 && !b.starts_with('#'))
        .map(|(i, _)| i)
        .collect()
}

fn new_paragraph(rng: &mut Rng, side: &str, n: usize, keep_id: &str) -> String {
    let words = 30 + rng.below(30);
    let mut s = format!("Revised by {side} in trial {n}:");
    for k in 0..words {
        s.push_str(if k % 2 == 0 { " knowledge" } else { " shared" });
    }
    s.push('.');
    if let Some(id) = keep_id
        .rsplit(" ^")
        .next()
        .filter(|_| keep_id.contains(" ^"))
    {
        s.push_str(&format!(" ^{id}"));
    }
    s
}

fn draw_edits(
    rng: &mut Rng,
    scenario: Scenario,
    pages: &[String],
    titles: &[String],
    n: usize,
) -> (Edit, Edit) {
    let page_a = usize::try_from(rng.below(pages.len() as u64)).unwrap_or(0);
    let text_a = pages.get(page_a).cloned().unwrap_or_default();
    let paras = paragraph_indices(&text_a);
    let pick = |rng: &mut Rng, exclude: Option<usize>| -> usize {
        let candidates: Vec<usize> = paras
            .iter()
            .copied()
            .filter(|i| Some(*i) != exclude)
            .collect();
        rng.pick(&candidates).copied().unwrap_or(1)
    };
    let block_text = |p: &str, i: usize| blocks(p).get(i).cloned().unwrap_or_default();
    match scenario {
        Scenario::SameParagraph => {
            let i = pick(rng, None);
            let old = block_text(&text_a, i);
            (
                Edit::Paragraph {
                    page: page_a,
                    index: i,
                    text: new_paragraph(rng, "A", n, &old),
                },
                Edit::Paragraph {
                    page: page_a,
                    index: i,
                    text: new_paragraph(rng, "B", n, &old),
                },
            )
        }
        Scenario::SamePageDifferentParagraphs => {
            let i = pick(rng, None);
            let j = pick(rng, Some(i));
            let (oi, oj) = (block_text(&text_a, i), block_text(&text_a, j));
            (
                Edit::Paragraph {
                    page: page_a,
                    index: i,
                    text: new_paragraph(rng, "A", n, &oi),
                },
                Edit::Paragraph {
                    page: page_a,
                    index: j,
                    text: new_paragraph(rng, "B", n, &oj),
                },
            )
        }
        Scenario::DifferentPages => {
            let mut page_b = usize::try_from(rng.below(pages.len() as u64)).unwrap_or(0);
            if page_b == page_a {
                page_b = (page_b + 1) % pages.len().max(1);
            }
            let text_b = pages.get(page_b).cloned().unwrap_or_default();
            let i = pick(rng, None);
            let pb = paragraph_indices(&text_b);
            let j = rng.pick(&pb).copied().unwrap_or(1);
            (
                Edit::Paragraph {
                    page: page_a,
                    index: i,
                    text: new_paragraph(rng, "A", n, &block_text(&text_a, i)),
                },
                Edit::Paragraph {
                    page: page_b,
                    index: j,
                    text: new_paragraph(rng, "B", n, &block_text(&text_b, j)),
                },
            )
        }
        Scenario::RenameAndEdit => {
            let i = pick(rng, None);
            let new_title = format!(
                "{} (renamed in trial {n})",
                titles.get(page_a).map_or("", String::as_str)
            );
            (
                Edit::Rename {
                    page: page_a,
                    new_title,
                },
                Edit::Paragraph {
                    page: page_a,
                    index: i,
                    text: new_paragraph(rng, "B", n, &block_text(&text_a, i)),
                },
            )
        }
        Scenario::FrontmatterAndBody => {
            let i = pick(rng, None);
            (
                Edit::Frontmatter {
                    page: page_a,
                    tag: format!("trial{n}"),
                },
                Edit::Paragraph {
                    page: page_a,
                    index: i,
                    text: new_paragraph(rng, "B", n, &block_text(&text_a, i)),
                },
            )
        }
    }
}

/// Write the edit into the worktree (and declare renames with `prikk mv`).
fn apply_to_worktree(
    prikk: &Prikk,
    repo: &Path,
    titles: &mut [String],
    pages: &mut [String],
    edit: &Edit,
) -> Result<(), String> {
    match edit {
        Edit::Rename { page, new_title } => {
            let Some(old) = titles.get(*page).cloned() else {
                return Ok(());
            };
            let from = page_path(&old);
            let to = page_path(new_title);
            if let Some(parent) = repo.join(&to).parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let from_s = from.to_string_lossy().into_owned();
            let to_s = to.to_string_lossy().into_owned();
            prikk.run(repo, &["mv", &from_s, &to_s])?;
            if let Some(t) = titles.get_mut(*page) {
                *t = new_title.clone();
            }
            Ok(())
        }
        Edit::Paragraph { page, .. } | Edit::Frontmatter { page, .. } => {
            apply_in_memory(pages, edit);
            let Some(t) = titles.get(*page) else {
                return Ok(());
            };
            let text = pages.get(*page).cloned().unwrap_or_default();
            std::fs::write(repo.join(page_path(t)), text).map_err(|e| e.to_string())
        }
    }
}

/// Bring the worktree's `pages/` to `reference`'s tip by extracting it and copying over.
fn resync_worktree(
    prikk: &Prikk,
    repo: &Path,
    reference: &str,
    scratch: &Path,
) -> Result<(), String> {
    let out = scratch.join(format!("resync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let out_s = out.to_string_lossy().into_owned();
    prikk.run(repo, &["extract", "--ref", reference, &out_s])?;
    let _ = std::fs::remove_dir_all(repo.join("pages"));
    copy_dir(&out.join("pages"), &repo.join("pages"))?;
    let _ = std::fs::remove_dir_all(&out);
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let target = to.join(e.file_name());
        let ft = e.file_type().map_err(|e| e.to_string())?;
        if ft.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn classify(err: &str) -> String {
    let l = err.to_ascii_lowercase();
    for (needle, name) in [
        ("conflict", "conflict"),
        ("deferred", "deferred"),
        ("not confluent", "not-confluent"),
        ("not_confluent", "not-confluent"),
        ("unsupported", "unsupported"),
        ("ordered", "ordered-dependency"),
        ("evidence", "evidence-failure"),
        ("invalid", "invalid-candidate"),
    ] {
        if l.contains(needle) {
            return name.to_owned();
        }
    }
    "other".to_owned()
}

/// The review-latency model: exponential inter-arrivals at `rate` per day, exponential review time with
/// mean `mean_hours`; a proposal's base has moved when another proposal was accepted between its arrival
/// and its own acceptance.
fn latency_model(rng: &mut Rng, rate_per_day: f64, mean_hours: f64, n: usize) -> f64 {
    let exp = |rng: &mut Rng, mean: f64| -> f64 {
        let u = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        -mean * (1.0 - u).max(1e-12).ln()
    };
    let mut t = 0.0;
    let mut arrivals = Vec::with_capacity(n);
    let mut acceptances = Vec::with_capacity(n);
    for _ in 0..n {
        t += exp(rng, 24.0 / rate_per_day);
        let accepted = t + exp(rng, mean_hours);
        arrivals.push(t);
        acceptances.push(accepted);
    }
    let mut sorted = acceptances.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut moved = 0usize;
    for (arr, acc) in arrivals.iter().zip(acceptances.iter()) {
        let before = sorted.partition_point(|x| *x < *acc);
        let from = sorted.partition_point(|x| *x <= *arr);
        // acceptances strictly inside (arr, acc), excluding this proposal's own.
        if before.saturating_sub(from).saturating_sub(1) > 0 {
            moved += 1;
        }
    }
    moved as f64 / n as f64
}

/// The `histories` subcommand.
pub fn run(opts: &Opts) -> Result<(), String> {
    let out = opts.path("out")?;
    let keys = opts.path("keys")?;
    let trials = usize::try_from(opts.num("trials", Some(100))?).unwrap_or(100);
    let seed = opts.num("seed", Some(7))?;
    let report_path: Option<PathBuf> = opts.opt_path("report");
    if out.exists() {
        return Err(format!("{} exists; refusing to overwrite", out.display()));
    }
    let repo = out.join("repo");
    let scratch = out.join("scratch");
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let prikk = Prikk::with_home(&keys)?;
    prikk.setup(&repo)?;
    let version = String::from_utf8_lossy(&prikk.run(&repo, &["--version"])?)
        .trim()
        .to_owned();

    let mut rng = Rng::new(seed);
    let mut titles: Vec<String> = (0..PAGES).map(|i| title(i, 0)).collect();
    let mut pages: Vec<String> = Vec::with_capacity(PAGES);
    for i in 0..PAGES {
        let t = titles.get(i).cloned().unwrap_or_default();
        let text = page_text(&mut rng, i, &t, PAGE_BYTES, &titles);
        let p = repo.join(page_path(&t));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, &text).map_err(|e| e.to_string())?;
        pages.push(text);
    }
    prikk.commit_and_seal(&repo, "Base wiki for the content-merge spike")?;

    let mut report = Report {
        prikk_version: version,
        seed,
        pages: PAGES,
        per_scenario: BTreeMap::new(),
        total: Tally::default(),
        latency_model: Vec::new(),
        trials: Vec::new(),
    };

    for n in 0..trials {
        let scenario = Scenario::draw(&mut rng);
        let baseline = prikk.tip(&repo, Some("heads/main"))?;
        let (a, b) = draw_edits(&mut rng, scenario, &pages, &titles, n);

        // The product's merge: A is the tip's side, B the proposal's.
        let (base_pages, tip_pages, mine_pages) = sides(&titles, &pages, &a, &b);
        let merged = merge(&base_pages, &tip_pages, &mine_pages);
        let block_merge_clean = merged.is_clean();
        // The measurement hook: of the overlaps, how many a line-level merge would have made clean.
        let line_level_clean = merged.line_level_clean();

        // prikk: B on a branch, A on main, then merge from the baseline.
        let branch = format!("heads/t{n}");
        prikk.run(
            &repo,
            &["branch", "create", &branch, "--from", "heads/main"],
        )?;
        prikk.run(&repo, &["branch", "switch", &branch])?;
        let mut titles_b = titles.clone();
        let mut pages_b = pages.clone();
        apply_to_worktree(&prikk, &repo, &mut titles_b, &mut pages_b, &b)?;
        prikk.commit_and_seal(&repo, &format!("Trial {n} side B"))?;
        prikk.run(&repo, &["branch", "switch", "heads/main"])?;
        resync_worktree(&prikk, &repo, "heads/main", &scratch)?;
        apply_to_worktree(&prikk, &repo, &mut titles, &mut pages, &a)?;
        prikk.commit_and_seal(&repo, &format!("Trial {n} side A"))?;
        let merged = prikk.run(
            &repo,
            &[
                "merge",
                "--allow-no-audit",
                "--baseline-block",
                &baseline,
                "--into",
                "heads/main",
                "--from",
                &branch,
            ],
        );
        let (outcome, detail) = match merged {
            Ok(_) => {
                // Both edits are now in history; the model's view of the pages must follow.
                resync_worktree(&prikk, &repo, "heads/main", &scratch)?;
                for (i, t) in titles.iter().enumerate() {
                    if let Ok(text) = std::fs::read_to_string(repo.join(page_path(t)))
                        && let Some(p) = pages.get_mut(i)
                    {
                        *p = text;
                    }
                }
                ("confluent".to_owned(), String::new())
            }
            Err(e) => (classify(&e), e.chars().take(200).collect()),
        };
        let tally_for = |t: &mut Tally| {
            t.trials += 1;
            if block_merge_clean {
                t.block_clean += 1;
            }
            t.line_level_clean_overlaps += line_level_clean;
            if outcome == "confluent" {
                t.prikk_confluent += 1;
            } else {
                *t.prikk_refusals.entry(outcome.clone()).or_default() += 1;
            }
            let key = format!(
                "block_clean={block_merge_clean},prikk_confluent={}",
                outcome == "confluent"
            );
            *t.agreement.entry(key).or_default() += 1;
        };
        tally_for(
            report
                .per_scenario
                .entry(format!("{scenario:?}"))
                .or_default(),
        );
        tally_for(&mut report.total);
        eprintln!("trial {n} {scenario:?}: block-clean={block_merge_clean} prikk={outcome}");
        report.trials.push(Trial {
            n,
            scenario,
            baseline,
            a,
            b,
            block_merge_clean,
            line_level_clean,
            prikk_outcome: outcome,
            prikk_detail: detail,
        });
    }

    for rate in [1.0, 5.0, 20.0, 100.0] {
        for mean in [1.0, 24.0, 168.0] {
            report.latency_model.push(LatencyRow {
                proposals_per_day: rate,
                mean_review_hours: mean,
                base_moved_fraction: latency_model(&mut rng, rate, mean, 20_000),
            });
        }
    }

    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(p) = report_path {
        std::fs::write(&p, &json).map_err(|e| e.to_string())?;
        eprintln!("report written to {}", p.display());
    }
    let summary = serde_json::json!({
        "total": report.total,
        "per_scenario": report.per_scenario,
        "latency_model": report.latency_model,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
    );
    Ok(())
}

// ------------------------------------------------------------------------------------- the merge fixture

/// What the merge fixture needs of a recorded trial.
#[derive(Debug, Deserialize)]
struct RecordedTrial {
    n: usize,
    scenario: Scenario,
    a: Edit,
    b: Edit,
    block_merge_clean: bool,
    prikk_outcome: String,
}

/// What the merge fixture needs of a recorded run (the spike's report).
#[derive(Debug, Deserialize)]
struct Recorded {
    seed: u64,
    pages: usize,
    trials: Vec<RecordedTrial>,
}

/// A page on one side of a pair. `text` is absent when it is the base's.
#[derive(Debug, Serialize)]
struct FixturePage {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Debug, Serialize)]
struct FixtureBase {
    path: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct FixtureEntry {
    base: FixtureBase,
    theirs: FixturePage,
    ours: FixturePage,
}

#[derive(Debug, Serialize)]
struct FixturePair {
    n: usize,
    scenario: Scenario,
    /// What the spike's own block model said about this pair (recorded in the report).
    spike_block_clean: bool,
    pages: Vec<FixtureEntry>,
}

#[derive(Debug, Serialize)]
struct Fixture {
    generator: &'static str,
    source: String,
    seed: u64,
    pairs: Vec<FixturePair>,
}

fn edited_page(edit: &Edit) -> usize {
    match edit {
        Edit::Paragraph { page, .. }
        | Edit::Frontmatter { page, .. }
        | Edit::Rename { page, .. } => *page,
    }
}

/// The `merge-fixture` subcommand (handoff 004 T2): rebuild, without prikk, the pairs of a recorded
/// `histories` run as the three sides of a merge, and write them as a committed test fixture.
///
/// The recorded report holds the edits but not the page texts. The texts are a pure function of the seed, so
/// the generator is replayed: the same draws, in the same order, with the wiki's state carried from trial to
/// trial as the run carried it (edit A always landed; edit B landed when prikk's merge was confluent). The
/// replay is checked against the report: every drawn pair of edits must equal the recorded one, or the
/// fixture is refused.
pub fn merge_fixture(opts: &Opts) -> Result<(), String> {
    let report_path = opts.path("report")?;
    let out = opts.path("out")?;
    if out.exists() {
        return Err(format!("{} exists; refusing to overwrite", out.display()));
    }
    let recorded: Recorded =
        serde_json::from_slice(&std::fs::read(&report_path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", report_path.display()))?;
    let mut rng = Rng::new(recorded.seed);
    let mut titles: Vec<String> = (0..recorded.pages).map(|i| title(i, 0)).collect();
    let mut pages: Vec<String> = Vec::with_capacity(recorded.pages);
    for i in 0..recorded.pages {
        let t = titles.get(i).cloned().unwrap_or_default();
        pages.push(page_text(&mut rng, i, &t, PAGE_BYTES, &titles));
    }

    let mut pairs = Vec::with_capacity(recorded.trials.len());
    for trial in &recorded.trials {
        let scenario = Scenario::draw(&mut rng);
        let (a, b) = draw_edits(&mut rng, scenario, &pages, &titles, trial.n);
        if scenario != trial.scenario || a != trial.a || b != trial.b {
            return Err(format!(
                "the replay diverged from the report at trial {}: drawn {scenario:?}, recorded {:?}",
                trial.n, trial.scenario
            ));
        }
        let (base, theirs, ours) = sides(&titles, &pages, &a, &b);
        let mut touched = vec![edited_page(&a), edited_page(&b)];
        touched.sort_unstable();
        touched.dedup();
        let mut entries = Vec::new();
        for i in touched {
            let (Some(x), Some(y), Some(z)) = (base.get(i), theirs.get(i), ours.get(i)) else {
                return Err(format!("trial {}: page {i} is missing", trial.n));
            };
            let differs = |p: &PageText| (p.text != x.text).then(|| p.text.clone());
            entries.push(FixtureEntry {
                base: FixtureBase {
                    path: x.path.clone(),
                    text: x.text.clone(),
                },
                theirs: FixturePage {
                    path: y.path.clone(),
                    text: differs(y),
                },
                ours: FixturePage {
                    path: z.path.clone(),
                    text: differs(z),
                },
            });
        }
        pairs.push(FixturePair {
            n: trial.n,
            scenario,
            spike_block_clean: trial.block_merge_clean,
            pages: entries,
        });
        // Carry the state as the run did: A landed on the tip; B landed too when prikk merged them.
        advance(&mut titles, &mut pages, &a);
        if trial.prikk_outcome == "confluent" {
            advance(&mut titles, &mut pages, &b);
        }
    }

    let fixture = Fixture {
        generator: "inklkunn-corpus merge-fixture (handoff 004 T2)",
        source: report_path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        seed: recorded.seed,
        pairs,
    };
    let json = serde_json::to_string(&fixture).map_err(|e| e.to_string())?;
    std::fs::write(&out, json + "\n").map_err(|e| e.to_string())?;
    eprintln!("{} pairs written to {}", fixture.pairs.len(), out.display());
    Ok(())
}

/// Land an edit on the in-memory wiki.
fn advance(titles: &mut [String], pages: &mut [String], edit: &Edit) {
    apply_in_memory(pages, edit);
    if let Edit::Rename { page, new_title } = edit
        && let Some(slot) = titles.get_mut(*page)
    {
        *slot = new_title.clone();
    }
}
