//! `claims pool`: draw the items the two labellers see, and write them as packets.
//!
//! The eligible events are the `changed` and the `replaced` ones (both have an old and a new text). The draw is
//! seeded, balanced between the two languages and gives each article of a language an equal share (an article
//! with fewer events than its share gives the surplus to the others). The committed pool holds item numbers and
//! block addresses, never text; the packets, which hold the two texts of each item and nothing else, stay in the
//! cache.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Opts;
use crate::wiki::Rng;

use super::events::{Event, Kind, Record, read_dir};
use super::text::block_texts;
use super::{cache_root, converted_path, slug, write_file};

/// One line of the committed pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolItem {
    /// 1-based, in the order the labellers see.
    pub item: u64,
    /// `en` or `ja`.
    pub lang: String,
    /// The article's title.
    pub article: String,
    /// The revision.
    pub rev: u64,
    /// The block number in the new revision.
    pub new_block: Option<usize>,
    /// The block number in the previous revision.
    pub prev_block: Option<usize>,
    /// `changed` or `replaced`.
    pub kind: Kind,
}

/// An item with the revision before it, which only the packets need (the committed pool does not carry it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawn {
    /// The pool line.
    pub item: PoolItem,
    /// The previous replayed revision.
    pub parent_rev: u64,
}

/// The command.
pub fn run(opts: &Opts) -> Result<(), String> {
    let events_dir = opts.path("events-dir")?;
    let seed = opts.num("seed", None)?;
    let size = usize::try_from(opts.num("size", None)?).map_err(|e| e.to_string())?;
    let out = opts.path("out")?;
    let packets = opts.path("packets-dir")?;
    let per_packet =
        usize::try_from(opts.num("packet-size", Some(40))?).map_err(|e| e.to_string())?;
    let shuffle = match opts.values.get("shuffle-order").map(String::as_str) {
        None | Some("yes") => true,
        Some("no") => false,
        Some(other) => return Err(format!("`--shuffle-order` is `yes` or `no`, not `{other}`")),
    };
    let cache = cache_root(opts)?;
    let events: Vec<Event> = read_dir(&events_dir)?
        .into_iter()
        .filter_map(|r| match r {
            Record::Event(e) => Some(e),
            Record::Revision(_) => None,
        })
        .collect();
    let pool = draw(&events, seed, size, shuffle)?;
    let mut lines = String::new();
    for d in &pool {
        lines.push_str(&serde_json::to_string(&d.item).map_err(|e| e.to_string())?);
        lines.push('\n');
    }
    write_file(&out, lines.as_bytes())?;
    write_packets(&pool, &cache, &packets, per_packet)?;
    println!("drew {} items into {}", pool.len(), out.display());
    Ok(())
}

/// Equal shares of `total` over articles with `available` events each: water-filling, so an article with fewer
/// than its share gives the surplus to the others; the remainder of a division goes to the first articles (in
/// the order given) that can still take one.
pub fn shares(available: &[usize], total: usize) -> Vec<usize> {
    let mut quota = vec![0usize; available.len()];
    let mut left = total;
    while left > 0 {
        let open: Vec<usize> = (0..available.len())
            .filter(|i| quota.get(*i) < available.get(*i))
            .collect();
        if open.is_empty() {
            break;
        }
        let each = left / open.len();
        if each == 0 {
            for i in open.iter().take(left) {
                if let Some(q) = quota.get_mut(*i) {
                    *q += 1;
                }
            }
            break;
        }
        for i in open {
            let room = available.get(i).copied().unwrap_or(0) - quota.get(i).copied().unwrap_or(0);
            let give = each.min(room);
            if let Some(q) = quota.get_mut(i) {
                *q += give;
            }
            left -= give;
        }
    }
    quota
}

fn shuffle<T>(rng: &mut Rng, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = usize::try_from(rng.below(i as u64 + 1)).unwrap_or(0);
        items.swap(i, j);
    }
}

/// Draw the pool: `size / 2` per language (the remainder to `en`), equal shares per article, seeded.
pub fn draw(
    events: &[Event],
    seed: u64,
    size: usize,
    shuffle_order: bool,
) -> Result<Vec<Drawn>, String> {
    let mut by_lang: BTreeMap<&str, BTreeMap<&str, Vec<&Event>>> = BTreeMap::new();
    for e in events {
        if matches!(e.kind, Kind::Changed | Kind::Replaced) {
            by_lang
                .entry(e.lang.as_str())
                .or_default()
                .entry(e.article.as_str())
                .or_default()
                .push(e);
        }
    }
    let mut rng = Rng::new(seed);
    let mut chosen: Vec<&Event> = Vec::new();
    for (lang, want) in [("en", size / 2 + size % 2), ("ja", size / 2)] {
        let articles = by_lang.get(lang).cloned().unwrap_or_default();
        let available: Vec<usize> = articles.values().map(Vec::len).collect();
        let have: usize = available.iter().sum();
        if have < want {
            return Err(format!(
                "language `{lang}` has {have} eligible events, fewer than the {want} the pool needs"
            ));
        }
        let quota = shares(&available, want);
        for ((_, mut list), q) in articles.into_iter().zip(quota) {
            shuffle(&mut rng, &mut list);
            chosen.extend(list.into_iter().take(q));
        }
    }
    if shuffle_order {
        shuffle(&mut rng, &mut chosen);
    }
    Ok(chosen
        .into_iter()
        .enumerate()
        .map(|(n, e)| Drawn {
            item: PoolItem {
                item: n as u64 + 1,
                lang: e.lang.clone(),
                article: e.article.clone(),
                rev: e.rev,
                new_block: e.new_block,
                prev_block: e.prev_block,
                kind: e.kind,
            },
            parent_rev: e.parent_rev,
        })
        .collect())
}

/// A code fence longer than any run of backticks in `text`.
fn fence_for(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

/// One item as a packet shows it: its number, the old text and the new text, and nothing else.
pub fn render_item(item: u64, old: &str, new: &str) -> String {
    let (fo, fn_) = (fence_for(old), fence_for(new));
    format!("## Item {item}\n\nOLD:\n\n{fo}\n{old}\n{fo}\n\nNEW:\n\n{fn_}\n{new}\n{fn_}\n\n")
}

fn block_of(
    cache: &Path,
    item: &PoolItem,
    rev: u64,
    block: Option<usize>,
) -> Result<String, String> {
    let path = converted_path(cache, &item.lang, &slug(&item.article), rev);
    let page = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let blocks = block_texts(&page);
    block
        .and_then(|b| blocks.get(b))
        .map(|t| (*t).to_owned())
        .ok_or_else(|| {
            format!(
                "item {}: block {block:?} is not in {}",
                item.item,
                path.display()
            )
        })
}

/// Write `packet-001.md` … with `per_packet` items each, from the converted cache.
pub fn write_packets(
    pool: &[Drawn],
    cache: &Path,
    dir: &Path,
    per_packet: usize,
) -> Result<(), String> {
    if per_packet == 0 {
        return Err("`--packet-size` must be at least 1".to_owned());
    }
    for (n, chunk) in pool.chunks(per_packet).enumerate() {
        let mut body = String::new();
        for d in chunk {
            let old = block_of(cache, &d.item, d.parent_rev, d.item.prev_block)?;
            let new = block_of(cache, &d.item, d.item.rev, d.item.new_block)?;
            body.push_str(&render_item(d.item.item, &old, &new));
        }
        write_file(
            &dir.join(format!("packet-{:03}.md", n + 1)),
            body.as_bytes(),
        )?;
    }
    Ok(())
}
