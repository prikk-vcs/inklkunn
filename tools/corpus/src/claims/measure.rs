//! `claims measure`: the measures M-1 … M-6 of RFC 005 part 1 from the labels and the replay's facts.
//!
//! Nothing here reads or prints article text: the inputs are the pool (block addresses), the events (metrics) and
//! the labels (item numbers and label names).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::Opts;

use super::events::{Event, Kind, Record, RevisionRec, read_dir};
use super::labels::{Label, Sampling, Scored, load_labels, sample};
use super::pool::PoolItem;
use super::stats::{cohen_kappa, max, mean, median, percentile, round4, share, sorted};
use super::{read_text, write_json};

/// Everything the measures read.
pub struct Input {
    /// The pool, in order.
    pub pool: Vec<PoolItem>,
    /// The replay's records.
    pub records: Vec<Record>,
    /// The first labeller's answers.
    pub first: BTreeMap<u64, super::labels::Judgement>,
    /// The second labeller's answers.
    pub second: BTreeMap<u64, super::labels::Judgement>,
    /// The sample size asked for.
    pub sample: usize,
}

/// A count and its share of a total.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Frac {
    /// The numerator.
    pub n: usize,
    /// The denominator.
    pub of: usize,
    /// `n / of`.
    pub share: Option<f64>,
}

fn frac(n: usize, of: usize) -> Frac {
    Frac {
        n,
        of,
        share: share(n, of),
    }
}

/// M-1.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct M1 {
    /// Sample items whose id survived and both labelled `same-claim`.
    pub both_same_claim: Frac,
    /// The first labeller alone.
    pub first_same_claim: Frac,
    /// The second labeller alone.
    pub second_same_claim: Frac,
}

/// M-2.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct M2 {
    /// How the estimate is made.
    pub formula: String,
    /// Replayed revisions that have a predecessor (the denominator of "per 100 revisions").
    pub edits: usize,
    /// `changed` + `replaced` + `added` + `removed` events (the denominator of "per 100 edited blocks").
    pub edited_blocks: usize,
    /// `changed` events in the replay.
    pub changed_events: usize,
    /// Pool items of kind `changed` both labelled, and the ones of them both marked factual.
    pub factual_share: Frac,
    /// Reconfirmations H-1 point 3 would ask of reviewers.
    pub reconfirmations_estimate: Option<f64>,
    /// Per 100 revisions.
    pub per_100_revisions: Option<f64>,
    /// Per 100 edited blocks.
    pub per_100_edited_blocks: Option<f64>,
}

/// One labeller's (or both's) M-2b.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct MeaningChange {
    /// Sample items labelled with a meaning change, and of them the ones whose id survived.
    pub id_survived: Frac,
}

/// The M-2b check: point 3 keys on the old and the new text differing.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct DifferCheck {
    /// Items with a meaning-change label from both and a surviving id.
    pub items: usize,
    /// Of them, the ones whose old and new text differ.
    pub old_new_differ: usize,
    /// Every one differs (point 3 marks each *judged on an earlier wording*).
    pub all_differ: bool,
}

/// M-2b.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct M2b {
    /// Both labellers gave a meaning-change label.
    pub both: MeaningChange,
    /// The first alone.
    pub first: MeaningChange,
    /// The second alone.
    pub second: MeaningChange,
    /// The check.
    pub check: DifferCheck,
}

/// A share per first-labeller label.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Disagreement {
    /// Items the first labeller gave this label.
    pub first_labelled: usize,
    /// Of them, the ones the second labeller labelled otherwise.
    pub disagreed: usize,
    /// `disagreed / first_labelled`.
    pub share: Option<f64>,
}

/// M-3.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct M3 {
    /// Overall disagreement.
    pub disagreement: Frac,
    /// Per label (as the first labeller's).
    pub per_label: BTreeMap<String, Disagreement>,
    /// First labeller's label to second labeller's label to count.
    pub confusion: BTreeMap<String, BTreeMap<String, usize>>,
    /// Cohen's kappa.
    pub kappa: Option<f64>,
    /// Label pairs (alphabetical `x/y`) the labellers split on.
    pub splits: BTreeMap<String, usize>,
    /// The same, by the event's kind (`changed`, `replaced`).
    pub splits_by_kind: BTreeMap<String, BTreeMap<String, usize>>,
}

/// A distribution.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Dist {
    /// Mean.
    pub mean: Option<f64>,
    /// Median.
    pub median: Option<f64>,
    /// 90th percentile (nearest rank).
    pub p90: Option<f64>,
    /// Maximum.
    pub max: Option<f64>,
}

fn dist(xs: &[f64]) -> Dist {
    let s = sorted(xs);
    Dist {
        mean: mean(xs),
        median: median(&s),
        p90: percentile(&s, 90),
        max: max(xs),
    }
}

/// M-4.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct M4 {
    /// Ids minted over the replay.
    pub ids_minted: usize,
    /// Ids that had at least one `changed` event.
    pub chains_with_changes: usize,
    /// `changed` events per chain, over the chains that have at least one.
    pub changes_per_chain: Dist,
    /// `changed` events per minted id (a chain with none counts 0).
    pub changes_per_id: Option<f64>,
    /// Sample `changed` items both labelled `same-claim`: the changes that touch only wording.
    pub wording_only: Frac,
}

/// M-5, one way of counting.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Notifications {
    /// Chains counted.
    pub chains: usize,
    /// Notifications per 100 revisions, over those chains.
    pub per_100_revisions: Dist,
}

/// M-5.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct M5 {
    /// A notification is a revision in which the chain's block changed.
    pub changed: Notifications,
    /// The same, also counting the revision in which the block was replaced or removed.
    pub changed_or_ended: Notifications,
}

/// An item to place in H-4's fields by hand.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Flagged {
    /// The item number.
    pub item: u64,
    /// The first labeller's label.
    pub first: String,
    /// The second labeller's label.
    pub second: String,
}

/// M-6.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct M6 {
    /// Items the labellers agree on, per label.
    pub agreed: BTreeMap<String, usize>,
    /// Items either labelled `cannot-tell` or `contradicts`.
    pub to_place_by_hand: Vec<Flagged>,
}

/// All the measures of one group (a language, or all).
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Group {
    /// Sample items in the group.
    pub sample_size: usize,
    /// M-1.
    pub m1: M1,
    /// M-2.
    pub m2: M2,
    /// M-2b.
    pub m2b: M2b,
    /// M-3.
    pub m3: M3,
    /// M-4.
    pub m4: M4,
    /// M-5.
    pub m5: M5,
    /// M-6.
    pub m6: M6,
}

/// How the sample was reached.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct SampleAccount {
    /// Items in the pool.
    pub pool_items: usize,
    /// The size asked for.
    pub target: usize,
    /// The size reached.
    pub size: usize,
    /// Pool items visited to reach it.
    pub scanned: usize,
    /// Visited items both labelled but not both factual.
    pub skipped_not_factual: usize,
    /// Visited items one labeller did not answer.
    pub unlabelled: usize,
    /// Visited items the first labeller marked factual.
    pub factual_first: usize,
    /// Visited items the second labeller marked factual.
    pub factual_second: usize,
}

/// The report (committed: numbers and item numbers only).
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Report {
    /// The sample.
    pub sample: SampleAccount,
    /// All languages.
    pub overall: Group,
    /// By language.
    pub languages: BTreeMap<String, Group>,
}

/// The command.
pub fn run(opts: &Opts) -> Result<(), String> {
    let pool_path = opts.path("pool")?;
    let events_dir = opts.path("events-dir")?;
    let out = opts.path("report-json")?;
    let labels = opts.all("labels");
    let [first, second] = labels.as_slice() else {
        return Err("`--labels` is needed twice, once per labeller".to_owned());
    };
    let size = usize::try_from(opts.num("sample", Some(400))?).map_err(|e| e.to_string())?;
    let mut pool = Vec::new();
    for line in read_text(&pool_path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        pool.push(
            serde_json::from_str::<PoolItem>(line)
                .map_err(|e| format!("{}: {e}", pool_path.display()))?,
        );
    }
    let input = Input {
        pool,
        records: read_dir(&events_dir)?,
        first: load_labels(std::path::Path::new(first))?,
        second: load_labels(std::path::Path::new(second))?,
        sample: size,
    };
    let report = compute(&input)?;
    write_json(&out, &report)?;
    print!("{}", render(&report));
    Ok(())
}

/// Compute every measure.
pub fn compute(input: &Input) -> Result<Report, String> {
    let events: Vec<Event> = input
        .records
        .iter()
        .filter_map(|r| match r {
            Record::Event(e) => Some(e.clone()),
            Record::Revision(_) => None,
        })
        .collect();
    let revisions: Vec<&RevisionRec> = input
        .records
        .iter()
        .filter_map(|r| match r {
            Record::Revision(v) => Some(v),
            Record::Event(_) => None,
        })
        .collect();
    let s = sample(
        &input.pool,
        &events,
        &input.first,
        &input.second,
        input.sample,
    )?;
    let langs: BTreeSet<&str> = input
        .records
        .iter()
        .map(|r| match r {
            Record::Revision(v) => v.lang.as_str(),
            Record::Event(e) => e.lang.as_str(),
        })
        .collect();
    let mut report = Report {
        sample: SampleAccount {
            pool_items: input.pool.len(),
            target: input.sample,
            size: s.sample.len(),
            scanned: s.scanned,
            skipped_not_factual: s.skipped_not_factual,
            unlabelled: s.unlabelled,
            factual_first: s.factual_a,
            factual_second: s.factual_b,
        },
        ..Report::default()
    };
    for lang in &langs {
        let g = group(Some(lang), &s, &events, &revisions);
        report.languages.insert((*lang).to_owned(), g);
    }
    report.overall = group(None, &s, &events, &revisions);
    // The estimate of M-2 is made per language (the share of factual `changed` items differs by script); the
    // overall figure is the sum of the languages' estimates.
    let estimates: Vec<Option<f64>> = report
        .languages
        .values()
        .map(|g| g.m2.reconfirmations_estimate)
        .collect();
    if !estimates.is_empty() && estimates.iter().all(Option::is_some) {
        let total: f64 = estimates.iter().flatten().sum();
        let m2 = &mut report.overall.m2;
        m2.reconfirmations_estimate = Some(round4(total));
        m2.per_100_revisions = ratio_per_100(total, m2.edits);
        m2.per_100_edited_blocks = ratio_per_100(total, m2.edited_blocks);
    } else {
        let m2 = &mut report.overall.m2;
        m2.reconfirmations_estimate = None;
        m2.per_100_revisions = None;
        m2.per_100_edited_blocks = None;
    }
    Ok(report)
}

fn ratio_per_100(x: f64, of: usize) -> Option<f64> {
    (of > 0).then(|| round4(x / of as f64 * 100.0))
}

fn pair_name(a: Label, b: Label) -> String {
    let (x, y) = if a.as_str() <= b.as_str() {
        (a, b)
    } else {
        (b, a)
    };
    format!("{}/{}", x.as_str(), y.as_str())
}

fn group(lang: Option<&str>, s: &Sampling, events: &[Event], revisions: &[&RevisionRec]) -> Group {
    let keep = |l: &str| lang.is_none_or(|w| w == l);
    let items: Vec<&Scored> = s.sample.iter().filter(|i| keep(&i.lang)).collect();
    let events: Vec<&Event> = events.iter().filter(|e| keep(&e.lang)).collect();
    let revisions: Vec<&&RevisionRec> = revisions.iter().filter(|r| keep(&r.lang)).collect();
    let mut g = Group {
        sample_size: items.len(),
        ..Group::default()
    };

    // M-1: of the items whose id survived, the share both labelled same-claim.
    let survived: Vec<&&Scored> = items.iter().filter(|i| i.id_survived).collect();
    let same = |f: fn(&Scored) -> bool| survived.iter().filter(|i| f(i)).count();
    g.m1 = M1 {
        both_same_claim: frac(
            same(|i| i.a == Label::SameClaim && i.b == Label::SameClaim),
            survived.len(),
        ),
        first_same_claim: frac(same(|i| i.a == Label::SameClaim), survived.len()),
        second_same_claim: frac(same(|i| i.b == Label::SameClaim), survived.len()),
    };

    // M-2: reconfirmations. `changed` events x the share of `changed` pool items (both labelled) that both
    // marked factual.
    let kinds = |k: Kind| events.iter().filter(|e| e.kind == k).count();
    let edits = revisions.iter().filter(|r| r.parent_rev.is_some()).count();
    let changed_share = s
        .changed_share
        .iter()
        .filter(|(l, _)| keep(l))
        .fold((0, 0), |(n, d), (_, k)| {
            (n + k.both_factual, d + k.labelled_by_both)
        });
    let edited_blocks: usize = Kind::ALL.iter().map(|k| kinds(*k)).sum();
    let estimate = (changed_share.1 > 0)
        .then(|| kinds(Kind::Changed) as f64 * changed_share.0 as f64 / changed_share.1 as f64);
    g.m2 = M2 {
        formula: "reconfirmations = (changed events in the replay) x (pool items of kind changed that both labellers \
                  marked factual / pool items of kind changed that both labelled); overall = the sum over languages"
            .to_owned(),
        edits,
        edited_blocks,
        changed_events: kinds(Kind::Changed),
        factual_share: frac(changed_share.0, changed_share.1),
        reconfirmations_estimate: estimate.map(round4),
        per_100_revisions: estimate.and_then(|e| ratio_per_100(e, edits)),
        per_100_edited_blocks: estimate.and_then(|e| ratio_per_100(e, edited_blocks)),
    };

    // M-2b: of the items labelled a change of meaning, the share whose id survived.
    let meaning = |f: fn(&Scored) -> bool| {
        let all: Vec<&&Scored> = items.iter().filter(|i| f(i)).collect();
        MeaningChange {
            id_survived: frac(all.iter().filter(|i| i.id_survived).count(), all.len()),
        }
    };
    let both: Vec<&&Scored> = items
        .iter()
        .filter(|i| i.a.is_meaning_change() && i.b.is_meaning_change() && i.id_survived)
        .collect();
    let differ = both
        .iter()
        .filter(|i| i.kind == Kind::Changed && i.text_differs)
        .count();
    g.m2b = M2b {
        both: meaning(|i| i.a.is_meaning_change() && i.b.is_meaning_change()),
        first: meaning(|i| i.a.is_meaning_change()),
        second: meaning(|i| i.b.is_meaning_change()),
        check: DifferCheck {
            items: both.len(),
            old_new_differ: differ,
            all_differ: differ == both.len(),
        },
    };

    // M-3: agreement between the labellers.
    let pairs: Vec<(Label, Label)> = items.iter().map(|i| (i.a, i.b)).collect();
    let mut m3 = M3 {
        disagreement: frac(pairs.iter().filter(|(a, b)| a != b).count(), pairs.len()),
        kappa: cohen_kappa(&pairs),
        ..M3::default()
    };
    for a in Label::ALL {
        let row = m3.confusion.entry(a.as_str().to_owned()).or_default();
        for b in Label::ALL {
            row.insert(
                b.as_str().to_owned(),
                pairs.iter().filter(|p| **p == (a, b)).count(),
            );
        }
        let first = pairs.iter().filter(|(x, _)| *x == a).count();
        let dis = pairs.iter().filter(|(x, y)| *x == a && *y != a).count();
        m3.per_label.insert(
            a.as_str().to_owned(),
            Disagreement {
                first_labelled: first,
                disagreed: dis,
                share: share(dis, first),
            },
        );
    }
    for i in items.iter().filter(|i| i.a != i.b) {
        let name = pair_name(i.a, i.b);
        *m3.splits.entry(name.clone()).or_default() += 1;
        *m3.splits_by_kind
            .entry(i.kind.as_str().to_owned())
            .or_default()
            .entry(name)
            .or_default() += 1;
    }
    g.m3 = m3;

    // M-4 and M-5: chains. A chain is an id; its changes are the `changed` events that carry it.
    let mut changed: BTreeMap<(&str, &str, &str), usize> = BTreeMap::new();
    let mut ended: BTreeMap<(&str, &str, &str), usize> = BTreeMap::new();
    for e in &events {
        let Some(id) = e.id_before.as_deref() else {
            continue;
        };
        let key = (e.lang.as_str(), e.article.as_str(), id);
        match e.kind {
            Kind::Changed => *changed.entry(key).or_default() += 1,
            Kind::Replaced | Kind::Removed => *ended.entry(key).or_default() += 1,
            Kind::Added => {}
        }
    }
    let minted: usize = revisions.iter().map(|r| r.minted).sum();
    let counts: Vec<f64> = changed.values().map(|c| *c as f64).collect();
    let changed_sample: Vec<&&Scored> = items.iter().filter(|i| i.kind == Kind::Changed).collect();
    g.m4 = M4 {
        ids_minted: minted,
        chains_with_changes: changed.len(),
        changes_per_chain: dist(&counts),
        changes_per_id: (minted > 0).then(|| round4(counts.iter().sum::<f64>() / minted as f64)),
        wording_only: frac(
            changed_sample
                .iter()
                .filter(|i| i.a == Label::SameClaim && i.b == Label::SameClaim)
                .count(),
            changed_sample.len(),
        ),
    };
    let mut edits_of: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for r in &revisions {
        if r.parent_rev.is_some() {
            *edits_of
                .entry((r.lang.as_str(), r.article.as_str()))
                .or_default() += 1;
        }
    }
    let rates = |with_ends: bool| {
        let xs: Vec<f64> = changed
            .iter()
            .filter_map(|(k, c)| {
                let n = c + if with_ends {
                    ended.get(k).copied().unwrap_or(0)
                } else {
                    0
                };
                let edits = edits_of.get(&(k.0, k.1)).copied().unwrap_or(0);
                (edits > 0).then(|| n as f64 / edits as f64 * 100.0)
            })
            .collect();
        Notifications {
            chains: xs.len(),
            per_100_revisions: dist(&xs),
        }
    };
    g.m5 = M5 {
        changed: rates(false),
        changed_or_ended: rates(true),
    };

    // M-6: the agreed labels, and the items to place by hand.
    for l in Label::ALL {
        g.m6.agreed.insert(
            l.as_str().to_owned(),
            items.iter().filter(|i| i.a == l && i.b == l).count(),
        );
    }
    let flag = |l: Label| matches!(l, Label::CannotTell | Label::Contradicts);
    g.m6.to_place_by_hand = items
        .iter()
        .filter(|i| flag(i.a) || flag(i.b))
        .map(|i| Flagged {
            item: i.item,
            first: i.a.as_str().to_owned(),
            second: i.b.as_str().to_owned(),
        })
        .collect();
    g
}

fn pct(x: Option<f64>) -> String {
    x.map_or_else(|| "n/a".to_owned(), |v| format!("{:.1} %", v * 100.0))
}

fn num(x: Option<f64>) -> String {
    x.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.2}"))
}

/// The human-readable table (no text of any article).
pub fn render(r: &Report) -> String {
    let mut cols: Vec<(&str, &Group)> = vec![("overall", &r.overall)];
    cols.extend(r.languages.iter().map(|(l, g)| (l.as_str(), g)));
    let mut out = String::new();
    let a = &r.sample;
    out.push_str(&format!(
        "sample {} of {} asked (pool {}, scanned {}, skipped not factual by both {}, unanswered {}, factual: first {} second {})\n\n",
        a.size, a.target, a.pool_items, a.scanned, a.skipped_not_factual, a.unlabelled, a.factual_first, a.factual_second
    ));
    out.push_str(&format!("{:<44}", "measure"));
    for (name, _) in &cols {
        out.push_str(&format!("{name:>18}"));
    }
    out.push('\n');
    let mut row = |label: &str, f: &dyn Fn(&Group) -> String| {
        out.push_str(&format!("{label:<44}"));
        for (_, g) in &cols {
            out.push_str(&format!("{:>18}", f(g)));
        }
        out.push('\n');
    };
    row("sample items", &|g| g.sample_size.to_string());
    row("M-1 both same-claim (id survived)", &|g| {
        pct(g.m1.both_same_claim.share)
    });
    row("M-1 first labeller alone", &|g| {
        pct(g.m1.first_same_claim.share)
    });
    row("M-1 second labeller alone", &|g| {
        pct(g.m1.second_same_claim.share)
    });
    row("M-2 reconfirmations per 100 revisions", &|g| {
        num(g.m2.per_100_revisions)
    });
    row("M-2 reconfirmations per 100 edited blocks", &|g| {
        num(g.m2.per_100_edited_blocks)
    });
    row("M-2b meaning change, id survived (both)", &|g| {
        pct(g.m2b.both.id_survived.share)
    });
    row("M-2b first labeller alone", &|g| {
        pct(g.m2b.first.id_survived.share)
    });
    row("M-2b second labeller alone", &|g| {
        pct(g.m2b.second.id_survived.share)
    });
    row("M-3 disagreement", &|g| pct(g.m3.disagreement.share));
    row("M-3 Cohen's kappa", &|g| num(g.m3.kappa));
    row("M-4 changes per chain (mean)", &|g| {
        num(g.m4.changes_per_chain.mean)
    });
    row("M-4 changes per chain (median / p90 / max)", &|g| {
        let d = &g.m4.changes_per_chain;
        format!("{}/{}/{}", num(d.median), num(d.p90), num(d.max))
    });
    row("M-4 wording only (sample changed items)", &|g| {
        pct(g.m4.wording_only.share)
    });
    row("M-5 notifications per 100 revisions (mean)", &|g| {
        num(g.m5.changed.per_100_revisions.mean)
    });
    row("M-5 (p90)", &|g| num(g.m5.changed.per_100_revisions.p90));
    row("M-5 (max)", &|g| num(g.m5.changed.per_100_revisions.max));
    row("M-6 items to place by hand", &|g| {
        g.m6.to_place_by_hand.len().to_string()
    });
    out
}
