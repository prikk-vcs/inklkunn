//! The labellers' answers, and the sample drawn from them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::events::{Event, Kind};
use super::pool::PoolItem;
use super::read_text;

/// What the rule sheet lets a labeller say about a factual item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Label {
    /// The new wording states the old claim.
    SameClaim,
    /// Narrower or more exact.
    Refines,
    /// A correction that replaces the old content.
    Supersedes,
    /// Both cannot hold.
    Contradicts,
    /// A different proposition.
    NewClaim,
    /// The labeller cannot tell.
    CannotTell,
}

impl Label {
    /// Every label, in the order the reports list them.
    pub const ALL: [Label; 6] = [
        Label::SameClaim,
        Label::Refines,
        Label::Supersedes,
        Label::Contradicts,
        Label::NewClaim,
        Label::CannotTell,
    ];

    /// The name used in the files.
    pub fn as_str(self) -> &'static str {
        match self {
            Label::SameClaim => "same-claim",
            Label::Refines => "refines",
            Label::Supersedes => "supersedes",
            Label::Contradicts => "contradicts",
            Label::NewClaim => "new-claim",
            Label::CannotTell => "cannot-tell",
        }
    }

    /// Refines, supersedes, contradicts or a new claim.
    pub fn is_meaning_change(self) -> bool {
        matches!(
            self,
            Label::Refines | Label::Supersedes | Label::Contradicts | Label::NewClaim
        )
    }
}

/// One line of a labels file.
#[derive(Debug, Clone, Deserialize)]
struct Row {
    item: u64,
    factual: bool,
    #[serde(default)]
    label: Option<Label>,
}

/// One labeller's answer for one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judgement {
    /// The item holds a factual statement.
    pub factual: bool,
    /// The label (present for every factual item).
    pub label: Option<Label>,
}

/// Read a labels file (JSON Lines): a duplicate item, or a factual item without a label, is an error.
pub fn load_labels(path: &Path) -> Result<BTreeMap<u64, Judgement>, String> {
    parse_labels(&read_text(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// The labels in a text.
pub fn parse_labels(text: &str) -> Result<BTreeMap<u64, Judgement>, String> {
    let mut out = BTreeMap::new();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let row: Row = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        if row.factual && row.label.is_none() {
            return Err(format!(
                "line {}: item {} is factual and has no label",
                n + 1,
                row.item
            ));
        }
        let j = Judgement {
            factual: row.factual,
            label: row.label.filter(|_| row.factual),
        };
        if out.insert(row.item, j).is_some() {
            return Err(format!(
                "line {}: item {} is labelled twice",
                n + 1,
                row.item
            ));
        }
    }
    Ok(out)
}

/// A sample item: the pool line, the replay's facts and the two labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scored {
    /// The item number.
    pub item: u64,
    /// `en` or `ja`.
    pub lang: String,
    /// `changed` or `replaced`.
    pub kind: Kind,
    /// The replay says the id survived.
    pub id_survived: bool,
    /// The old and the new text differ.
    pub text_differs: bool,
    /// The first labeller's label.
    pub a: Label,
    /// The second labeller's label.
    pub b: Label,
}

/// Items labelled by both, by language and kind, for the estimate of M-2.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct KindShare {
    /// Pool items of kind `changed` that both labellers answered.
    pub labelled_by_both: usize,
    /// Of them, the ones both marked factual.
    pub both_factual: usize,
}

/// Where an event is: language, article, revision, old block, new block.
type EventKey<'a> = (&'a str, &'a str, u64, Option<usize>, Option<usize>);

/// The sample and how it was reached.
#[derive(Debug, Default)]
pub struct Sampling {
    /// The sample, in pool order.
    pub sample: Vec<Scored>,
    /// Pool items visited to fill it.
    pub scanned: usize,
    /// Visited items both labelled but not both factual.
    pub skipped_not_factual: usize,
    /// Visited items one labeller did not answer.
    pub unlabelled: usize,
    /// Visited items the first labeller marked factual.
    pub factual_a: usize,
    /// Visited items the second labeller marked factual.
    pub factual_b: usize,
    /// By language: pool items of kind `changed`, over the whole pool (not only the visited part).
    pub changed_share: BTreeMap<String, KindShare>,
}

/// Draw the sample: the first `limit` pool items, in pool order, that both labellers marked factual. An item
/// that is not in the replay's events is an error (the pool and the events must be of one run).
pub fn sample(
    pool: &[PoolItem],
    events: &[Event],
    a: &BTreeMap<u64, Judgement>,
    b: &BTreeMap<u64, Judgement>,
    limit: usize,
) -> Result<Sampling, String> {
    let known: BTreeSet<u64> = pool.iter().map(|p| p.item).collect();
    for (who, labels) in [("first", a), ("second", b)] {
        if let Some(stray) = labels.keys().find(|k| !known.contains(k)) {
            return Err(format!(
                "the {who} labeller labelled item {stray}, which is not in the pool"
            ));
        }
    }
    let facts: BTreeMap<EventKey<'_>, &Event> = events
        .iter()
        .map(|e| {
            (
                (
                    e.lang.as_str(),
                    e.article.as_str(),
                    e.rev,
                    e.prev_block,
                    e.new_block,
                ),
                e,
            )
        })
        .collect();
    let mut s = Sampling::default();
    let mut full_at: Option<usize> = None;
    for (n, item) in pool.iter().enumerate() {
        let (ja, jb) = (a.get(&item.item), b.get(&item.item));
        if let (Some(ja), Some(jb)) = (ja, jb)
            && item.kind == Kind::Changed
        {
            let share = s.changed_share.entry(item.lang.clone()).or_default();
            share.labelled_by_both += 1;
            share.both_factual += usize::from(ja.factual && jb.factual);
        }
        if full_at.is_some() {
            continue;
        }
        s.scanned += 1;
        s.factual_a += usize::from(ja.is_some_and(|j| j.factual));
        s.factual_b += usize::from(jb.is_some_and(|j| j.factual));
        let (Some(ja), Some(jb)) = (ja, jb) else {
            s.unlabelled += 1;
            continue;
        };
        let (true, true, Some(la), Some(lb)) = (ja.factual, jb.factual, ja.label, jb.label) else {
            s.skipped_not_factual += 1;
            continue;
        };
        let key = (
            item.lang.as_str(),
            item.article.as_str(),
            item.rev,
            item.prev_block,
            item.new_block,
        );
        let ev = facts.get(&key).ok_or_else(|| {
            format!("item {} has no event in the events directory (is it the run the pool was drawn from?)", item.item)
        })?;
        s.sample.push(Scored {
            item: item.item,
            lang: item.lang.clone(),
            kind: item.kind,
            id_survived: ev.id_survived,
            text_differs: ev.text_differs,
            a: la,
            b: lb,
        });
        if s.sample.len() >= limit {
            full_at = Some(n);
        }
    }
    Ok(s)
}
