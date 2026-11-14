//! The rows behind a [`KnowledgeIndex`](super::KnowledgeIndex), which the index reads only through these methods.
//!
//! In test builds every link and every anchor that is read is counted (K40, maintenance M30), whichever way it is
//! read: through a position the index's maps hold (`link`, `anchor`), through a pass over the anchors (`anchors`),
//! or by taking all the rows (`whole`, which counts every link and every anchor as read). The rows are a private
//! field of this module, so a lookup rewritten as a scan of them (quadratic over a crowd of sources) cannot hide
//! from the count: it has to take them all, and it is counted for what it takes. In a release build nothing is
//! counted and the methods are the slices'.

#[cfg(test)]
use std::sync::Arc;

use inklkunn_store::{
    KnowledgeAnchorRow, KnowledgeClaimRow, KnowledgeInvalidRow, KnowledgeLinkRow,
    KnowledgeRelationRow, KnowledgeRows, KnowledgeSourceRow,
};

/// What was read through the rows so far (test builds only): units of work, never calls and never a clock.
#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct Work {
    /// Links read: one for each link a lookup returned or a pass went over.
    links: std::sync::atomic::AtomicUsize,
    /// Anchors read, the same way.
    anchors: std::sync::atomic::AtomicUsize,
    /// Calls of [`Rows::whole`]: the whole of the rows handed out, to be read as the caller likes.
    whole_reads: std::sync::atomic::AtomicUsize,
}

/// The rows of an index, readable only through the methods below.
#[derive(Debug, Clone)]
pub(super) struct Rows {
    inner: KnowledgeRows,
    /// Shared by the clones of an index, which are of one tip.
    #[cfg(test)]
    work: Arc<Work>,
}

impl Rows {
    /// These rows, nothing read yet.
    pub(super) fn new(inner: KnowledgeRows) -> Self {
        Rows {
            inner,
            #[cfg(test)]
            work: Arc::new(Work::default()),
        }
    }

    /// `(links read, anchors read, whole reads)` so far (test builds only).
    #[cfg(test)]
    pub(super) fn work_done(&self) -> (usize, usize, usize) {
        use std::sync::atomic::Ordering::Relaxed;
        (
            self.work.links.load(Relaxed),
            self.work.anchors.load(Relaxed),
            self.work.whole_reads.load(Relaxed),
        )
    }

    /// All the rows as the store keeps them: the caller may then read any of them, so every link and every anchor
    /// is counted as read, and the call as one whole read. (A pass over the links is possible only here, and it
    /// is paid for here.)
    pub(super) fn whole(&self) -> &KnowledgeRows {
        #[cfg(test)]
        {
            use std::sync::atomic::Ordering::Relaxed;
            self.work.whole_reads.fetch_add(1, Relaxed);
            self.work.links.fetch_add(self.inner.links.len(), Relaxed);
            self.work
                .anchors
                .fetch_add(self.inner.anchors.len(), Relaxed);
        }
        &self.inner
    }

    /// The claims, in order.
    pub(super) fn claims(&self) -> &[KnowledgeClaimRow] {
        &self.inner.claims
    }

    /// The sources, in order.
    pub(super) fn sources(&self) -> &[KnowledgeSourceRow] {
        &self.inner.sources
    }

    /// The claim-to-claim relations, in order.
    pub(super) fn relations(&self) -> &[KnowledgeRelationRow] {
        &self.inner.relations
    }

    /// The files that cannot be read.
    pub(super) fn invalid(&self) -> &[KnowledgeInvalidRow] {
        &self.inner.invalid
    }

    /// The link at a position the index holds, counted as one read.
    pub(super) fn link(&self, at: usize) -> Option<&KnowledgeLinkRow> {
        let link = self.inner.links.get(at);
        #[cfg(test)]
        if link.is_some() {
            self.work
                .links
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        link
    }

    /// The anchor at a position the index holds, counted as one read.
    pub(super) fn anchor(&self, at: usize) -> Option<&KnowledgeAnchorRow> {
        let anchor = self.inner.anchors.get(at);
        #[cfg(test)]
        if anchor.is_some() {
            self.work
                .anchors
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        anchor
    }

    /// Every anchor in order; in test builds each one passed is counted as a read.
    pub(super) fn anchors(&self) -> impl Iterator<Item = &KnowledgeAnchorRow> {
        #[cfg(test)]
        let work = Arc::clone(&self.work);
        self.inner.anchors.iter().inspect(move |_| {
            #[cfg(test)]
            work.anchors
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
    }
}
