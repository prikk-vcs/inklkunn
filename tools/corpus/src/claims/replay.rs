//! `claims replay`: walk each converted history oldest first through the product's own block functions and
//! record, for each edited block, whether its id survived and how much of its wording changed.
//!
//! The block segmentation is `inklkunn_markup::blocks`; the matching of old blocks to new ones is
//! `inklkunn_core::align_blocks` (the merge's own alignment). The tool mints the ids (the stand-in for CM-5) and
//! carries them through that alignment; it has no matcher of its own.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use inklkunn_core::align_blocks;
use serde::Serialize;

use crate::Opts;

use super::events::{Event, Kind, Record, RevisionRec};
use super::text::{block_texts, body_of, whole, wording_change};
use super::{
    ArticleRevs, cache_root, converted_path, parse_revisions, read_text, write_file, write_json,
};

/// A converted revision.
#[derive(Debug, Clone)]
pub struct Version {
    /// The revision id.
    pub rev: u64,
    /// The converted page (frontmatter included; the replay strips it).
    pub text: String,
}

/// An article's numbers (committed; no text).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Revisions replayed.
    pub revisions: usize,
    /// Of them, the ones that have a predecessor (the denominator of "per 100 revisions").
    pub edits: usize,
    /// Pinned revisions with no converted text (skipped).
    pub revisions_without_converted_text: usize,
    /// Events of kind `changed`.
    pub changed: u64,
    /// Events of kind `replaced`.
    pub replaced: u64,
    /// Events of kind `added`.
    pub added: u64,
    /// Events of kind `removed`.
    pub removed: u64,
    /// Ids minted over the history.
    pub ids_minted: usize,
    /// Ids minted in the first revision that the last revision still carries.
    pub first_ids_alive_at_end: usize,
    /// Revisions whose body repeats an earlier one that is not their parent.
    pub reverts: usize,
}

impl Counts {
    fn add(&mut self, o: &Counts) {
        self.revisions += o.revisions;
        self.edits += o.edits;
        self.revisions_without_converted_text += o.revisions_without_converted_text;
        self.changed += o.changed;
        self.replaced += o.replaced;
        self.added += o.added;
        self.removed += o.removed;
        self.ids_minted += o.ids_minted;
        self.first_ids_alive_at_end += o.first_ids_alive_at_end;
        self.reverts += o.reverts;
    }
}

/// One language's counts.
#[derive(Debug, Default, Serialize)]
pub struct LangCounts {
    /// By title.
    pub articles: BTreeMap<String, Counts>,
    /// The sum.
    pub totals: Counts,
}

/// The committed counts file.
#[derive(Debug, Default, Serialize)]
pub struct CountsFile {
    /// By language.
    pub languages: BTreeMap<String, LangCounts>,
}

/// The result of replaying one article.
#[derive(Debug)]
pub struct Replayed {
    /// The records, in order: each revision's record, then its events.
    pub records: Vec<Record>,
    /// The counts.
    pub counts: Counts,
}

/// The command.
pub fn run(opts: &Opts) -> Result<(), String> {
    let revisions = opts.path("revisions")?;
    let out_dir = opts.path("out-dir")?;
    let counts_path = opts.path("counts")?;
    let cache = cache_root(opts)?;
    let articles = parse_revisions(&read_text(&revisions)?)?;
    let file = replay_all(&articles, &cache, &out_dir)?;
    write_json(&counts_path, &file)?;
    println!("replayed {} articles", articles.len());
    Ok(())
}

/// Replay every article from the converted cache, writing one events file per article into `out_dir`.
pub fn replay_all(
    articles: &[ArticleRevs],
    cache: &Path,
    out_dir: &Path,
) -> Result<CountsFile, String> {
    let mut file = CountsFile::default();
    for a in articles {
        let mut versions = Vec::new();
        let mut missing = 0;
        for r in &a.revs {
            match std::fs::read_to_string(converted_path(cache, &a.lang, &a.key, r.id)) {
                Ok(text) => versions.push(Version { rev: r.id, text }),
                Err(_) => missing += 1,
            }
        }
        let mut done = replay_article(a.index, &a.lang, &a.title, &versions);
        done.counts.revisions_without_converted_text = missing;
        let mut lines = String::new();
        for rec in &done.records {
            lines.push_str(&rec.to_line()?);
            lines.push('\n');
        }
        write_file(
            &out_dir.join(format!("{}-{}.jsonl", a.lang, a.key)),
            lines.as_bytes(),
        )?;
        let lang = file.languages.entry(a.lang.clone()).or_default();
        lang.totals.add(&done.counts);
        lang.articles.insert(a.title.clone(), done.counts);
    }
    Ok(file)
}

/// What one revision step produced.
struct Step {
    events: Vec<Event>,
    ids: Vec<String>,
    minted: usize,
}

/// Replay one article's converted revisions, oldest first. Ids are `<article index>-<n>`.
pub fn replay_article(index: usize, lang: &str, article: &str, versions: &[Version]) -> Replayed {
    let mut records = Vec::new();
    let mut counts = Counts::default();
    let mut next_id = 0usize;
    let mut earlier: HashSet<String> = HashSet::new();
    let mut first_ids: HashSet<String> = HashSet::new();
    let mut prev: Option<(&Version, Vec<String>)> = None;
    let mut last_ids: Vec<String> = Vec::new();
    for (position, v) in versions.iter().enumerate() {
        let body = body_of(&v.text);
        let (events, ids, minted, parent) = match &prev {
            None => {
                let n = block_texts(&v.text).len();
                let ids: Vec<String> = (0..n).map(|_| mint(index, &mut next_id)).collect();
                first_ids = ids.iter().cloned().collect();
                (Vec::new(), ids, n, None)
            }
            Some((p, prev_ids)) => {
                let s = step(index, &mut next_id, p, v, prev_ids);
                (s.events, s.ids, s.minted, Some(p.rev))
            }
        };
        let is_revert = match &prev {
            Some((p, _)) => body_of(&p.text) != body && earlier.contains(body),
            None => false,
        };
        if let Some((p, _)) = &prev {
            earlier.insert(body_of(&p.text).to_owned());
            counts.edits += 1;
        }
        let tally = |k: Kind| events.iter().filter(|e| e.kind == k).count() as u64;
        let rec = RevisionRec {
            kind: "revision".to_owned(),
            lang: lang.to_owned(),
            article: article.to_owned(),
            rev: v.rev,
            parent_rev: parent,
            position,
            blocks: ids.len(),
            minted,
            changed: tally(Kind::Changed),
            replaced: tally(Kind::Replaced),
            added: tally(Kind::Added),
            removed: tally(Kind::Removed),
            is_revert,
        };
        counts.changed += rec.changed;
        counts.replaced += rec.replaced;
        counts.added += rec.added;
        counts.removed += rec.removed;
        counts.ids_minted += minted;
        counts.reverts += usize::from(is_revert);
        let others = (events.len() as u64).saturating_sub(1);
        records.push(Record::Revision(rec));
        for mut e in events {
            e.lang = lang.to_owned();
            e.article = article.to_owned();
            e.others_changed_in_revision = others;
            e.is_revert = is_revert;
            records.push(Record::Event(e));
        }
        last_ids.clone_from(&ids);
        prev = Some((v, ids));
    }
    counts.revisions = versions.len();
    counts.first_ids_alive_at_end = last_ids.iter().filter(|i| first_ids.contains(*i)).count();
    Replayed { records, counts }
}

fn mint(index: usize, next: &mut usize) -> String {
    let id = format!("{index}-{next}");
    *next += 1;
    id
}

/// One revision against its predecessor.
fn step(
    index: usize,
    next_id: &mut usize,
    prev: &Version,
    next: &Version,
    prev_ids: &[String],
) -> Step {
    let pb = block_texts(&prev.text);
    let nb = block_texts(&next.text);
    let align = align_blocks(&prev.text, &next.text);
    // `owner[j]`: the old block the new block j is the same block as. One old block per new block.
    let mut owner: Vec<Option<usize>> = vec![None; nb.len()];
    let mut matched_prev = vec![false; pb.len()];
    let mut anchors: Vec<(usize, usize)> = Vec::new();
    for (i, a) in align.iter().enumerate().take(pb.len()) {
        if let Some(j) = *a
            && let Some(slot) = owner.get_mut(j)
            && slot.is_none()
        {
            *slot = Some(i);
            if let Some(m) = matched_prev.get_mut(i) {
                *m = true;
            }
            anchors.push((i, j));
        }
    }
    let mut minted = 0;
    let ids: Vec<String> = owner
        .iter()
        .map(|o| match o.and_then(|i| prev_ids.get(i)) {
            Some(id) => id.clone(),
            None => {
                minted += 1;
                mint(index, next_id)
            }
        })
        .collect();
    let make = |kind: Kind, p: Option<usize>, n: Option<usize>| -> Event {
        let old = p.and_then(|i| pb.get(i)).copied();
        let new = n.and_then(|j| nb.get(j)).copied();
        let change = match (kind, old, new) {
            (Kind::Changed | Kind::Replaced, Some(o), Some(nw)) => wording_change(o, nw),
            (_, o, nw) => whole(o.unwrap_or(""), nw.unwrap_or(""), false),
        };
        Event {
            kind,
            lang: String::new(),
            article: String::new(),
            rev: next.rev,
            parent_rev: prev.rev,
            prev_block: p,
            new_block: n,
            id_before: p.and_then(|i| prev_ids.get(i)).cloned(),
            id_after: n.and_then(|j| ids.get(j)).cloned(),
            id_survived: kind == Kind::Changed,
            chars_changed: change.chars,
            words_changed: change.words,
            unrefined: change.unrefined,
            text_differs: old != new,
            others_changed_in_revision: 0,
            is_revert: false,
        }
    };
    let mut events = Vec::new();
    let mut emitted_new = vec![false; nb.len()];
    let (mut pc, mut nc) = (0usize, 0usize);
    let sentinel = [(pb.len(), nb.len())];
    for (&(i, j), is_anchor) in anchors
        .iter()
        .map(|a| (a, true))
        .chain(sentinel.iter().map(|a| (a, false)))
    {
        let old_run: Vec<usize> = (pc..i)
            .filter(|k| matched_prev.get(*k) == Some(&false))
            .collect();
        let new_run: Vec<usize> = if j > nc {
            (nc..j).filter(|k| owner.get(*k) == Some(&None)).collect()
        } else {
            Vec::new()
        };
        for k in &new_run {
            if let Some(e) = emitted_new.get_mut(*k) {
                *e = true;
            }
        }
        let pairs = old_run.len().min(new_run.len());
        for (p, n) in old_run.iter().zip(&new_run) {
            events.push(make(Kind::Replaced, Some(*p), Some(*n)));
        }
        for p in old_run.iter().skip(pairs) {
            events.push(make(Kind::Removed, Some(*p), None));
        }
        for n in new_run.iter().skip(pairs) {
            events.push(make(Kind::Added, None, Some(*n)));
        }
        if is_anchor {
            if pb.get(i) != nb.get(j) {
                events.push(make(Kind::Changed, Some(i), Some(j)));
            }
            pc = i + 1;
            nc = nc.max(j + 1);
        }
    }
    // A new block left over by crossing anchors is still an addition.
    for (n, done) in emitted_new.iter().enumerate() {
        if !done && owner.get(n) == Some(&None) {
            events.push(make(Kind::Added, None, Some(n)));
        }
    }
    Step {
        events,
        ids,
        minted,
    }
}
