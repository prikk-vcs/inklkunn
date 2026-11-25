//! A derived store that watches what an export asks of it and can be told to refuse to grow the writes index: it answers every
//! question as the memory store does. `first_seen` is recorded by the size of each call (I-T1 correction X-6), and
//! `append_writes` says the index was dropped under the builder when it is told to (so the index never moves).
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use inklkunn_store::{
    Appended, BlockWrites, DerivedPage, DerivedStore, IndexEntry, IndexedBlock, KnowledgeRows,
    LaterMove, LaterWrite, MaybeWriter, MemoryStore, PageKey, SearchDoc, SearchHit, WikiId,
    Writers, WritesState,
};

/// The memory store, watched.
pub struct Spy {
    pub inner: std::sync::Arc<MemoryStore>,
    /// The number of blocks of each `first_seen` call, in order.
    pub seen_calls: Mutex<Vec<usize>>,
    /// While set, an append to the writes index changes nothing and says the index was dropped.
    pub drop_appends: AtomicBool,
    /// While set, every read of the writes index's generation says another number: an index dropped under every reader.
    pub flap_generation: AtomicBool,
    flaps: std::sync::atomic::AtomicU64,
}

impl Spy {
    pub fn over(inner: std::sync::Arc<MemoryStore>) -> Self {
        Spy {
            inner,
            seen_calls: Mutex::new(Vec::new()),
            drop_appends: AtomicBool::new(false),
            flap_generation: AtomicBool::new(false),
            flaps: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn seen_calls(&self) -> Vec<usize> {
        self.seen_calls.lock().unwrap().clone()
    }
}

impl DerivedStore for Spy {
    fn page(&self, a0: &PageKey<'_>) -> inklkunn_store::Result<Option<DerivedPage>> {
        self.inner.page(a0)
    }
    fn put_page(&self, a0: &PageKey<'_>, a1: &DerivedPage) -> inklkunn_store::Result<()> {
        self.inner.put_page(a0, a1)
    }

    fn title_index(
        &self,
        a0: &WikiId,
        a1: &str,
    ) -> inklkunn_store::Result<Option<Vec<IndexEntry>>> {
        self.inner.title_index(a0, a1)
    }

    fn put_title_index(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &[IndexEntry],
        a3: i64,
    ) -> inklkunn_store::Result<()> {
        self.inner.put_title_index(a0, a1, a2, a3)
    }

    fn retain_tips(&self, a0: &WikiId, a1: &[String]) -> inklkunn_store::Result<u64> {
        self.inner.retain_tips(a0, a1)
    }

    fn clear(&self, a0: &WikiId) -> inklkunn_store::Result<u64> {
        self.inner.clear(a0)
    }

    fn put_search_index(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &[SearchDoc],
        a3: i64,
    ) -> inklkunn_store::Result<()> {
        self.inner.put_search_index(a0, a1, a2, a3)
    }

    fn search(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
        a3: usize,
    ) -> inklkunn_store::Result<Option<Vec<SearchHit>>> {
        self.inner.search(a0, a1, a2, a3)
    }

    fn knowledge_index(
        &self,
        a0: &WikiId,
        a1: &str,
    ) -> inklkunn_store::Result<Option<KnowledgeRows>> {
        self.inner.knowledge_index(a0, a1)
    }

    fn put_knowledge_index(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &KnowledgeRows,
        a3: i64,
    ) -> inklkunn_store::Result<()> {
        self.inner.put_knowledge_index(a0, a1, a2, a3)
    }

    fn writes_state(&self, a0: &WikiId, a1: &str) -> inklkunn_store::Result<Option<WritesState>> {
        self.inner.writes_state(a0, a1)
    }

    fn reset_writes(&self, a0: &WikiId, a1: &str) -> inklkunn_store::Result<()> {
        self.inner.reset_writes(a0, a1)
    }

    fn writes_generation(&self, a0: &WikiId, a1: &str) -> inklkunn_store::Result<u64> {
        let real = self.inner.writes_generation(a0, a1)?;
        if self.flap_generation.load(Ordering::SeqCst) {
            return Ok(real + 1 + self.flaps.fetch_add(1, Ordering::SeqCst));
        }
        Ok(real)
    }

    fn block_absent(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
        a3: &str,
    ) -> inklkunn_store::Result<bool> {
        self.inner.block_absent(a0, a1, a2, a3)
    }

    fn note_block_absent(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
        a3: &str,
    ) -> inklkunn_store::Result<()> {
        self.inner.note_block_absent(a0, a1, a2, a3)
    }

    fn indexed_block(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
    ) -> inklkunn_store::Result<Option<IndexedBlock>> {
        self.inner.indexed_block(a0, a1, a2)
    }

    fn indexed_block_at(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: u64,
    ) -> inklkunn_store::Result<Option<IndexedBlock>> {
        self.inner.indexed_block_at(a0, a1, a2)
    }

    fn last_writes(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
        a3: u64,
    ) -> inklkunn_store::Result<Writers> {
        self.inner.last_writes(a0, a1, a2, a3)
    }

    fn later_writes(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
        a3: u64,
        a4: usize,
    ) -> inklkunn_store::Result<Vec<LaterWrite>> {
        self.inner.later_writes(a0, a1, a2, a3, a4)
    }

    fn later_maybes(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
        a3: u64,
    ) -> inklkunn_store::Result<Vec<MaybeWriter>> {
        self.inner.later_maybes(a0, a1, a2, a3)
    }

    fn later_moves(
        &self,
        a0: &WikiId,
        a1: &str,
        a2: &str,
        a3: u64,
        a4: usize,
    ) -> inklkunn_store::Result<Vec<LaterMove>> {
        self.inner.later_moves(a0, a1, a2, a3, a4)
    }

    fn first_seen(
        &self,
        wiki: &WikiId,
        blocks: &[String],
        now: i64,
    ) -> inklkunn_store::Result<Vec<i64>> {
        self.seen_calls.lock().unwrap().push(blocks.len());
        self.inner.first_seen(wiki, blocks, now)
    }

    fn append_writes(
        &self,
        wiki: &WikiId,
        view: &str,
        blocks: &[BlockWrites],
        state: &WritesState,
        expected_generation: u64,
    ) -> inklkunn_store::Result<Appended> {
        if self.drop_appends.load(Ordering::SeqCst) {
            return Ok(Appended::Dropped);
        }
        self.inner
            .append_writes(wiki, view, blocks, state, expected_generation)
    }
}
