//! The in-memory derived cache: the reference the SQLite store must agree with.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::PoisonError;

use crate::Result;
use crate::derived::{
    Appended, BlockWrites, DerivedPage, DerivedStore, IndexEntry, IndexedBlock, KnowledgeRows,
    LastWrite, LaterMove, LaterWrite, MAX_ABSENT_BLOCKS, MAX_LATER_MOVES, MAX_LATER_WRITES,
    MAX_MAYBE_WRITERS, MaybeWriter, PageKey, SearchDoc, SearchHit, WriteKind, Writers, WritesState,
};
use crate::governance::{GovernanceSnapshot, GovernanceSnapshotStore};
use crate::record::WikiId;

use super::MemoryStore;

/// One view's writes index in memory: the reference the SQLite tables must agree with.
#[derive(Debug, Default)]
pub(super) struct ViewWrites {
    state: Option<WritesState>,
    /// number → the block.
    blocks: BTreeMap<u64, IndexedBlock>,
    /// block id → number.
    by_id: HashMap<String, u64>,
    /// `(path, number)` → what the block did (a block has one row per path).
    rows: BTreeMap<(String, u64), WriteKind>,
}

/// Does the row settle who wrote the path (it is not a *may have*)?
fn is_certain(kind: &WriteKind) -> bool {
    matches!(
        kind,
        WriteKind::Wrote | WriteKind::Deleted | WriteKind::Carried { .. }
    )
}

/// `(wiki, tip, path, lang, presentation)`.
type PageId = (WikiId, String, String, String, String);

/// Everything derived, behind one lock so that a replacement is one step.
#[derive(Debug, Default)]
pub(super) struct Derived {
    pages: HashMap<PageId, DerivedPage>,
    /// `(wiki, tip)` → the complete index, by path.
    indexes: HashMap<(WikiId, String), BTreeMap<String, IndexEntry>>,
    /// `(wiki, tip)` → the complete search index, in the order stored.
    searches: HashMap<(WikiId, String), Vec<SearchDoc>>,
    /// `(wiki, tip)` → the complete knowledge index.
    knowledge: HashMap<(WikiId, String), KnowledgeRows>,
    /// `(wiki, tip)` → the governance snapshot, with the order it was stored in (the newest has the highest).
    governance: HashMap<(WikiId, String), (u64, GovernanceSnapshot)>,
    /// How many governance snapshots were stored: the order of the next.
    governance_stored: u64,
    /// `(wiki, block)` → when the block was first seen. Kept when the cache is cleared.
    seen: HashMap<(WikiId, String), i64>,
    /// `(wiki, view)` → the writes index of the view's log.
    writes: HashMap<(WikiId, String), ViewWrites>,
    /// `(wiki, view)` → how many times the view's index was dropped (its generation), kept when it is.
    generations: HashMap<(WikiId, String), u64>,
    /// `(wiki, view)` → `(block, the view's tip)` that were found not in the view's log, oldest first.
    absent: HashMap<(WikiId, String), VecDeque<(String, String)>>,
}

fn id_of(key: &PageKey<'_>) -> PageId {
    (
        key.wiki.clone(),
        key.tip.to_owned(),
        key.path.to_owned(),
        key.lang.to_owned(),
        key.presentation.to_owned(),
    )
}

impl MemoryStore {
    fn derived(&self) -> std::sync::MutexGuard<'_, Derived> {
        self.derived.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl DerivedStore for MemoryStore {
    fn page(&self, key: &PageKey<'_>) -> Result<Option<DerivedPage>> {
        Ok(self.derived().pages.get(&id_of(key)).cloned())
    }

    fn put_page(&self, key: &PageKey<'_>, page: &DerivedPage) -> Result<()> {
        self.derived().pages.insert(id_of(key), page.clone());
        Ok(())
    }

    fn title_index(&self, wiki: &WikiId, tip: &str) -> Result<Option<Vec<IndexEntry>>> {
        Ok(self
            .derived()
            .indexes
            .get(&(wiki.clone(), tip.to_owned()))
            .map(|entries| entries.values().cloned().collect()))
    }

    fn put_title_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        entries: &[IndexEntry],
        _built_at: i64,
    ) -> Result<()> {
        let by_path = entries
            .iter()
            .map(|e| (e.path.clone(), e.clone()))
            .collect::<BTreeMap<_, _>>();
        self.derived()
            .indexes
            .insert((wiki.clone(), tip.to_owned()), by_path);
        Ok(())
    }

    fn retain_tips(&self, wiki: &WikiId, keep: &[String]) -> Result<u64> {
        let mut d = self.derived();
        let keeps = |tip: &String| keep.contains(tip);
        let count = |d: &Derived| {
            d.pages.len()
                + d.indexes.len()
                + d.searches.len()
                + d.knowledge.len()
                + d.governance.len()
        };
        let before = count(&d);
        d.pages.retain(|(w, tip, ..), _| w != wiki || keeps(tip));
        d.indexes.retain(|(w, tip), _| w != wiki || keeps(tip));
        d.searches.retain(|(w, tip), _| w != wiki || keeps(tip));
        d.knowledge.retain(|(w, tip), _| w != wiki || keeps(tip));
        d.governance.retain(|(w, tip), _| w != wiki || keeps(tip));
        let removed = before - count(&d);
        Ok(u64::try_from(removed).unwrap_or(u64::MAX))
    }

    fn clear(&self, wiki: &WikiId) -> Result<u64> {
        let mut d = self.derived();
        let count = |d: &Derived| {
            d.pages.len()
                + d.indexes.len()
                + d.searches.len()
                + d.knowledge.len()
                + d.governance.len()
                + d.writes.len()
        };
        let before = count(&d);
        d.pages.retain(|(w, ..), _| w != wiki);
        d.indexes.retain(|(w, _), _| w != wiki);
        d.searches.retain(|(w, _), _| w != wiki);
        d.knowledge.retain(|(w, _), _| w != wiki);
        d.governance.retain(|(w, _), _| w != wiki);
        // The index of each view is dropped, and the view's generation moves with it.
        let dropped: Vec<(WikiId, String)> = d
            .writes
            .keys()
            .filter(|(w, _)| w == wiki)
            .cloned()
            .collect();
        for key in dropped {
            *d.generations.entry(key).or_default() += 1;
        }
        d.writes.retain(|(w, _), _| w != wiki);
        d.absent.retain(|(w, _), _| w != wiki);
        let removed = before - count(&d);
        Ok(u64::try_from(removed).unwrap_or(u64::MAX))
    }

    fn put_search_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        docs: &[SearchDoc],
        _built_at: i64,
    ) -> Result<()> {
        self.derived()
            .searches
            .insert((wiki.clone(), tip.to_owned()), docs.to_vec());
        Ok(())
    }

    fn search(
        &self,
        wiki: &WikiId,
        tip: &str,
        folded_query: &str,
        limit: usize,
    ) -> Result<Option<Vec<SearchHit>>> {
        Ok(self
            .derived()
            .searches
            .get(&(wiki.clone(), tip.to_owned()))
            .map(|docs| {
                docs.iter()
                    .filter(|d| d.folded.contains(folded_query))
                    .take(limit)
                    .map(|d| SearchHit {
                        path: d.path.clone(),
                        kind: d.kind,
                        text: d.text.clone(),
                    })
                    .collect()
            }))
    }

    fn knowledge_index(&self, wiki: &WikiId, tip: &str) -> Result<Option<KnowledgeRows>> {
        Ok(self
            .derived()
            .knowledge
            .get(&(wiki.clone(), tip.to_owned()))
            .cloned())
    }

    fn put_knowledge_index(
        &self,
        wiki: &WikiId,
        tip: &str,
        rows: &KnowledgeRows,
        _built_at: i64,
    ) -> Result<()> {
        self.derived()
            .knowledge
            .insert((wiki.clone(), tip.to_owned()), rows.clone().sorted());
        Ok(())
    }

    fn first_seen(&self, wiki: &WikiId, blocks: &[String], now: i64) -> Result<Vec<i64>> {
        let mut d = self.derived();
        Ok(blocks
            .iter()
            .map(|b| *d.seen.entry((wiki.clone(), b.clone())).or_insert(now))
            .collect())
    }

    fn writes_state(&self, wiki: &WikiId, view: &str) -> Result<Option<WritesState>> {
        Ok(self
            .derived()
            .writes
            .get(&(wiki.clone(), view.to_owned()))
            .and_then(|v| v.state.clone()))
    }

    fn append_writes(
        &self,
        wiki: &WikiId,
        view: &str,
        blocks: &[BlockWrites],
        state: &WritesState,
        expected_generation: u64,
    ) -> Result<Appended> {
        blocks.iter().try_for_each(BlockWrites::validate)?;
        let mut d = self.derived();
        // Under the one lock a reset takes too: the index is the one the builder began with, or nothing is appended.
        let key = (wiki.clone(), view.to_owned());
        if d.generations.get(&key).copied().unwrap_or(0) != expected_generation {
            return Ok(Appended::Dropped);
        }
        let index = d.writes.entry(key).or_default();
        for block in blocks {
            if index.blocks.contains_key(&block.number) {
                continue;
            }
            index.blocks.insert(
                block.number,
                IndexedBlock {
                    number: block.number,
                    block: block.block.clone(),
                    proposal: block.proposal.clone(),
                },
            );
            index.by_id.insert(block.block.clone(), block.number);
            for write in &block.writes {
                index
                    .rows
                    .insert((write.path.clone(), block.number), write.kind.clone());
            }
        }
        // The state only moves forward: a second reader that built the same blocks leaves it where the first put it.
        if index
            .state
            .as_ref()
            .is_none_or(|held| held.head_number <= state.head_number)
        {
            index.state = Some(state.clone());
        }
        Ok(Appended::Done)
    }

    fn reset_writes(&self, wiki: &WikiId, view: &str) -> Result<()> {
        let mut d = self.derived();
        let key = (wiki.clone(), view.to_owned());
        d.writes.remove(&key);
        d.absent.remove(&key);
        *d.generations.entry(key).or_default() += 1;
        Ok(())
    }

    fn writes_generation(&self, wiki: &WikiId, view: &str) -> Result<u64> {
        Ok(self
            .derived()
            .generations
            .get(&(wiki.clone(), view.to_owned()))
            .copied()
            .unwrap_or(0))
    }

    fn block_absent(&self, wiki: &WikiId, view: &str, block: &str, view_tip: &str) -> Result<bool> {
        Ok(self
            .derived()
            .absent
            .get(&(wiki.clone(), view.to_owned()))
            .is_some_and(|kept| kept.iter().any(|(b, tip)| b == block && tip == view_tip)))
    }

    fn note_block_absent(
        &self,
        wiki: &WikiId,
        view: &str,
        block: &str,
        view_tip: &str,
    ) -> Result<()> {
        let mut d = self.derived();
        let kept = d.absent.entry((wiki.clone(), view.to_owned())).or_default();
        kept.retain(|(b, _)| b != block);
        kept.push_back((block.to_owned(), view_tip.to_owned()));
        while kept.len() > MAX_ABSENT_BLOCKS {
            kept.pop_front();
        }
        Ok(())
    }

    fn indexed_block(
        &self,
        wiki: &WikiId,
        view: &str,
        block: &str,
    ) -> Result<Option<IndexedBlock>> {
        let d = self.derived();
        Ok(d.writes
            .get(&(wiki.clone(), view.to_owned()))
            .and_then(|v| v.by_id.get(block).and_then(|n| v.blocks.get(n)))
            .cloned())
    }

    fn indexed_block_at(
        &self,
        wiki: &WikiId,
        view: &str,
        number: u64,
    ) -> Result<Option<IndexedBlock>> {
        let d = self.derived();
        Ok(d.writes
            .get(&(wiki.clone(), view.to_owned()))
            .and_then(|v| v.blocks.get(&number))
            .cloned())
    }

    fn last_writes(&self, wiki: &WikiId, view: &str, path: &str, at: u64) -> Result<Writers> {
        let d = self.derived();
        let Some(index) = d.writes.get(&(wiki.clone(), view.to_owned())) else {
            return Ok(Writers::default());
        };
        let of_path = |p: &str| {
            index
                .rows
                .range((p.to_owned(), 0)..=(p.to_owned(), at))
                .map(|((_, number), kind)| (*number, kind.clone()))
                .collect::<Vec<_>>()
        };
        let own = of_path(path);
        let last = own
            .iter()
            .rev()
            .find(|(_, kind)| is_certain(kind))
            .map(|(number, kind)| LastWrite {
                number: *number,
                kind: kind.clone(),
            });
        let after = last.as_ref().map_or(0, |l| l.number);
        let mut maybes: Vec<MaybeWriter> = own
            .iter()
            .chain(of_path("").iter())
            .filter(|(number, kind)| *number >= after && !is_certain(kind))
            .map(|(number, kind)| MaybeWriter {
                number: *number,
                opaque: *kind == WriteKind::Opaque,
            })
            .collect();
        // A block that is both (a path row and a block row) is listed once, opaque when either is.
        maybes.sort_by(|a, b| a.number.cmp(&b.number).then(b.opaque.cmp(&a.opaque)));
        maybes.dedup_by_key(|m| m.number);
        let more = maybes.len() > MAX_MAYBE_WRITERS;
        maybes.truncate(MAX_MAYBE_WRITERS);
        Ok(Writers { last, maybes, more })
    }

    fn later_writes(
        &self,
        wiki: &WikiId,
        view: &str,
        path: &str,
        after: u64,
        limit: usize,
    ) -> Result<Vec<LaterWrite>> {
        let d = self.derived();
        let Some(index) = d.writes.get(&(wiki.clone(), view.to_owned())) else {
            return Ok(Vec::new());
        };
        let limit = limit.clamp(1, MAX_LATER_WRITES).saturating_add(1);
        Ok(index
            .rows
            .range((path.to_owned(), after.saturating_add(1))..=(path.to_owned(), u64::MAX))
            .filter(|(_, kind)| matches!(kind, WriteKind::Wrote | WriteKind::Carried { .. }))
            .filter_map(|((_, number), kind)| {
                index.blocks.get(number).map(|b| LaterWrite {
                    number: *number,
                    block: b.block.clone(),
                    kind: kind.clone(),
                })
            })
            .take(limit)
            .collect())
    }

    fn later_maybes(
        &self,
        wiki: &WikiId,
        view: &str,
        path: &str,
        after: u64,
    ) -> Result<Vec<MaybeWriter>> {
        let d = self.derived();
        let Some(index) = d.writes.get(&(wiki.clone(), view.to_owned())) else {
            return Ok(Vec::new());
        };
        let mut maybes: Vec<MaybeWriter> = [path, ""]
            .iter()
            .flat_map(|of| {
                index.rows.range(
                    ((*of).to_owned(), after.saturating_add(1))..=((*of).to_owned(), u64::MAX),
                )
            })
            .filter(|(_, kind)| !is_certain(kind))
            .map(|((_, number), kind)| MaybeWriter {
                number: *number,
                opaque: *kind == WriteKind::Opaque,
            })
            .collect();
        // A block that is both (a row of the path and a row of every path) is listed once, opaque when either is.
        maybes.sort_by(|a, b| a.number.cmp(&b.number).then(b.opaque.cmp(&a.opaque)));
        maybes.dedup_by_key(|m| m.number);
        maybes.truncate(MAX_MAYBE_WRITERS + 1);
        Ok(maybes)
    }

    fn later_moves(
        &self,
        wiki: &WikiId,
        view: &str,
        path: &str,
        after: u64,
        limit: usize,
    ) -> Result<Vec<LaterMove>> {
        let d = self.derived();
        let Some(index) = d.writes.get(&(wiki.clone(), view.to_owned())) else {
            return Ok(Vec::new());
        };
        let limit = limit.clamp(1, MAX_LATER_MOVES).saturating_add(1);
        let mut moves: Vec<LaterMove> = index
            .rows
            .iter()
            .filter(|((_, number), _)| *number > after)
            .filter_map(|((to, number), kind)| match kind {
                WriteKind::Carried { from } if from == path => {
                    index.blocks.get(number).map(|b| LaterMove {
                        number: *number,
                        block: b.block.clone(),
                        to: to.clone(),
                    })
                }
                _ => None,
            })
            .collect();
        moves.sort_by(|a, b| a.number.cmp(&b.number).then_with(|| a.to.cmp(&b.to)));
        moves.truncate(limit);
        Ok(moves)
    }
}

impl GovernanceSnapshotStore for MemoryStore {
    fn governance_snapshot(&self, wiki: &WikiId, tip: &str) -> Result<Option<GovernanceSnapshot>> {
        Ok(self
            .derived()
            .governance
            .get(&(wiki.clone(), tip.to_owned()))
            .map(|(_, snapshot)| snapshot.clone()))
    }

    fn latest_governance_snapshot(
        &self,
        wiki: &WikiId,
    ) -> Result<Option<(String, GovernanceSnapshot)>> {
        Ok(self
            .derived()
            .governance
            .iter()
            .filter(|((w, _), _)| w == wiki)
            .max_by_key(|(_, (order, _))| *order)
            .map(|((_, tip), (_, snapshot))| (tip.clone(), snapshot.clone())))
    }

    fn put_governance_snapshot(
        &self,
        wiki: &WikiId,
        tip: &str,
        snapshot: &GovernanceSnapshot,
        _built_at: i64,
    ) -> Result<()> {
        snapshot.validate()?;
        let mut derived = self.derived();
        derived.governance_stored += 1;
        let order = derived.governance_stored;
        derived
            .governance
            .insert((wiki.clone(), tip.to_owned()), (order, snapshot.clone()));
        Ok(())
    }
}
