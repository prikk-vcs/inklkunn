//! `claims convert`: wikitext to Portable Wiki Markdown with wiki-commons' converter, run as a subprocess.
//!
//! The converter (`tools/wikicommons.py`, a Python script of the wiki-commons checkout) is a measurement aid, not a
//! dependency of any crate. It reads a MediaWiki export, so each article's cached wikitext is wrapped in a minimal
//! one: no contributor and no comment element, and **synthetic** timestamps (the real ones are never fetched).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::Opts;

use super::{
    ArticleRevs, cache_root, converted_path, parse_revisions, raw_path, read_text, write_file,
    write_json,
};

/// What the report says about the timestamps in the converter's input.
pub const TIMESTAMP_NOTE: &str = "synthetic: 2001-01-01T00:00:00Z plus the revision's position in the article's list, in seconds; \
     the real times were never fetched";

/// A revision to put in an export.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    /// The revision id.
    pub id: u64,
    /// The parent's id.
    pub parent: u64,
    /// Its position in the article's list (the synthetic time).
    pub position: usize,
    /// The wikitext.
    pub text: &'a str,
}

/// The synthetic timestamp of a position: `2001-01-01T00:00:00Z` plus that many seconds.
pub fn synthetic_timestamp(position: usize) -> String {
    let secs = position as u64;
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 2001-01-01 to a civil date (Howard Hinnant's algorithm, shifted to the Unix epoch).
    let z = days as i64 + 11_323 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// XML-escape text; characters XML 1.0 cannot carry are dropped and counted.
pub fn xml_escape(s: &str) -> (String, usize) {
    let mut out = String::with_capacity(s.len());
    let mut dropped = 0;
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || matches!(c as u32, 0xfffe | 0xffff) => dropped += 1,
            c => out.push(c),
        }
    }
    (out, dropped)
}

/// A minimal MediaWiki export of one page: its title, a page id and its revisions in order. No contributor, no
/// comment. The second value is how many characters XML could not carry.
pub fn export_xml(lang: &str, title: &str, page_id: usize, revs: &[Entry<'_>]) -> (String, usize) {
    let mut dropped = 0;
    let mut x = String::new();
    x.push_str("<mediawiki xmlns=\"http://www.mediawiki.org/xml/export-0.11/\" version=\"0.11\" ");
    x.push_str(&format!("xml:lang=\"{lang}\">\n"));
    x.push_str(&format!(
        "  <siteinfo><sitename>Wikipedia ({lang})</sitename></siteinfo>\n  <page>\n    <title>{}</title>\n    <ns>0</ns>\n    <id>{page_id}</id>\n",
        xml_escape(title).0
    ));
    for r in revs {
        let (text, d) = xml_escape(r.text);
        dropped += d;
        x.push_str(&format!(
            "    <revision>\n      <id>{}</id>\n      <parentid>{}</parentid>\n      <timestamp>{}</timestamp>\n      <text xml:space=\"preserve\">{text}</text>\n    </revision>\n",
            r.id,
            r.parent,
            synthetic_timestamp(r.position)
        ));
    }
    x.push_str("  </page>\n</mediawiki>\n");
    (x, dropped)
}

/// The lines under "Degraded" and "Dropped" of the converter's import report.
pub fn parse_losses(report: &str) -> (Vec<String>, Vec<String>) {
    let (mut degraded, mut dropped) = (Vec::new(), Vec::new());
    let mut into: Option<bool> = None;
    for line in report.lines() {
        if let Some(rest) = line.strip_prefix("- **") {
            into = match rest.split(':').next() {
                Some("Degraded") => Some(true),
                Some("Dropped") => Some(false),
                _ => None,
            };
        } else if let Some(item) = line.strip_prefix("  - ") {
            match into {
                Some(true) => degraded.push(item.trim().to_owned()),
                Some(false) => dropped.push(item.trim().to_owned()),
                None => {}
            }
        } else {
            into = None;
        }
    }
    (degraded, dropped)
}

/// One article's outcome.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Revisions the list pins.
    pub pinned: usize,
    /// Pinned revisions with no cached text (deleted upstream or not fetched yet).
    pub missing_raw: usize,
    /// Revisions converted and cached.
    pub converted: usize,
    /// Revisions given to the converter that came back without text (it could not convert them, or the history
    /// had no entry for them).
    pub skipped: usize,
    /// Characters XML 1.0 cannot carry, dropped from the converter's input.
    pub xml_invalid_chars_dropped: usize,
    /// The converter's "Degraded" lines for this article.
    pub degraded: Vec<String>,
    /// The converter's "Dropped" lines for this article.
    pub dropped: Vec<String>,
}

/// The report.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// `git rev-parse HEAD` of the wiki-commons checkout.
    pub wikicommons_commit: Option<String>,
    /// How the converter's timestamps were made.
    pub timestamps: String,
    /// `lang` to title to counts.
    pub languages: BTreeMap<String, BTreeMap<String, Counts>>,
}

/// The command.
pub fn run(opts: &Opts) -> Result<(), String> {
    let revisions = opts.path("revisions")?;
    let wikicommons = opts.path("wikicommons")?;
    let report_path = opts.path("report")?;
    let cache = cache_root(opts)?;
    let articles = parse_revisions(&read_text(&revisions)?)?;
    let report = convert(&articles, &cache, &wikicommons)?;
    write_json(&report_path, &report)?;
    let converted: usize = report
        .languages
        .values()
        .flat_map(BTreeMap::values)
        .map(|c| c.converted)
        .sum();
    println!("converted {converted} revisions");
    Ok(())
}

/// Convert every article's cached revisions with the converter at `wikicommons`.
pub fn convert(
    articles: &[ArticleRevs],
    cache: &Path,
    wikicommons: &Path,
) -> Result<Report, String> {
    let script = wikicommons.join("tools").join("wikicommons.py");
    if !script.is_file() {
        return Err(format!(
            "the wiki-commons converter is not at `{}` (pass `--wikicommons DIR`, a checkout of wiki-commons)",
            script.display()
        ));
    }
    let mut report = Report {
        wikicommons_commit: head_of(wikicommons),
        timestamps: TIMESTAMP_NOTE.to_owned(),
        languages: BTreeMap::new(),
    };
    for a in articles {
        let counts = convert_article(a, cache, &script)?;
        report
            .languages
            .entry(a.lang.clone())
            .or_default()
            .insert(a.title.clone(), counts);
    }
    Ok(report)
}

fn head_of(dir: &Path) -> Option<String> {
    let dir = dir.to_string_lossy();
    match crate::sh(
        Path::new("."),
        &[],
        "git",
        &["-C", dir.as_ref(), "rev-parse", "HEAD"],
    ) {
        Ok(out) => Some(String::from_utf8_lossy(&out).trim().to_owned()),
        Err(e) => {
            eprintln!("inklkunn-corpus: the converter's commit is unknown: {e}");
            None
        }
    }
}

fn convert_article(a: &ArticleRevs, cache: &Path, script: &Path) -> Result<Counts, String> {
    let mut counts = Counts {
        pinned: a.revs.len(),
        ..Counts::default()
    };
    let mut texts = Vec::new();
    for (position, r) in a.revs.iter().enumerate() {
        match std::fs::read_to_string(raw_path(cache, &a.lang, &a.key, r.id)) {
            Ok(t) => texts.push((r.id, r.parent, position, t)),
            Err(_) => counts.missing_raw += 1,
        }
    }
    if texts.is_empty() {
        return Ok(counts);
    }
    let entries: Vec<Entry<'_>> = texts
        .iter()
        .map(|(id, parent, position, t)| Entry {
            id: *id,
            parent: *parent,
            position: *position,
            text: t,
        })
        .collect();
    let (xml, dropped) = export_xml(&a.lang, &a.title, a.index + 1, &entries);
    counts.xml_invalid_chars_dropped = dropped;
    let work = cache.join("convert").join(&a.lang).join(&a.key);
    let xml_path = work.join("export.xml");
    write_file(&xml_path, xml.as_bytes())?;
    let out = work.join("bundle");
    run_converter(script, &xml_path, &out)?;
    let history = read_history(&out)?;
    let wanted: BTreeSet<u64> = texts.iter().map(|(id, ..)| *id).collect();
    for id in &wanted {
        match history.get(id) {
            Some(content) => {
                write_file(
                    &converted_path(cache, &a.lang, &a.key, *id),
                    content.as_bytes(),
                )?;
                counts.converted += 1;
            }
            None => counts.skipped += 1,
        }
    }
    if let Ok(report) = std::fs::read_to_string(out.join("import-report.md")) {
        let (degraded, dropped) = parse_losses(&report);
        counts.degraded = degraded;
        counts.dropped = dropped;
    }
    Ok(counts)
}

fn run_converter(script: &Path, xml: &Path, out: &Path) -> Result<(), String> {
    let result = Command::new("python3")
        .arg(script)
        .args(["mediawiki", "import"])
        .arg(xml)
        .arg(out)
        .stdin(Stdio::null())
        .output();
    let output = match result {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(
                "`python3` is not on PATH (the wiki-commons converter needs it)".to_owned(),
            );
        }
        Err(e) => return Err(format!("python3: {e}")),
    };
    if !output.status.success() {
        return Err(format!(
            "the converter failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// The converted text of each revision in `<bundle>/history/*.jsonl` (revisions with no `content`, which the
/// converter writes for ones deleted upstream, are left out).
pub fn read_history(bundle: &Path) -> Result<BTreeMap<u64, String>, String> {
    let dir = bundle.join("history");
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(out);
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    for f in files {
        for line in read_text(&f)?.lines().filter(|l| !l.trim().is_empty()) {
            let v: serde_json::Value =
                serde_json::from_str(line).map_err(|e| format!("{}: {e}", f.display()))?;
            let id = v
                .get("rev")
                .and_then(|r| r.as_str())
                .and_then(|r| r.strip_prefix('r'))
                .and_then(|r| r.parse::<u64>().ok());
            if let (Some(id), Some(content)) = (id, v.get("content").and_then(|c| c.as_str())) {
                out.insert(id, content.to_owned());
            }
        }
    }
    Ok(out)
}
