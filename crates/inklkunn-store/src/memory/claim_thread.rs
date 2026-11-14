//! The in-memory claim threads: the reference the SQLite store must agree with ([`crate::claim_thread`] says what
//! they are).

use std::sync::{MutexGuard, PoisonError};

use super::MemoryStore;
use crate::claim_thread::{
    ClaimCommentRecord, ClaimCommentRow, ClaimThreadPage, ClaimThreadStore, MAX_PAGE, well_formed,
};
use crate::record::{AccountId, CommentId, WikiId};
use crate::{Result, StoreError};

/// Every comment of every thread, in the order written.
#[derive(Debug, Default)]
pub(super) struct ClaimThreads {
    comments: Vec<ClaimCommentRecord>,
}

impl ClaimThreads {
    /// The comments of one thread, in order, with their places.
    fn thread(&self, wiki: &WikiId, claim: &str) -> Vec<ClaimCommentRow> {
        let in_thread: Vec<&ClaimCommentRecord> = self
            .comments
            .iter()
            .filter(|c| c.wiki == *wiki && c.claim == claim)
            .collect();
        let place = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
        in_thread
            .iter()
            .enumerate()
            .map(|(i, c)| ClaimCommentRow {
                position: place(i),
                parent_position: c
                    .parent
                    .as_ref()
                    .and_then(|parent| in_thread.iter().position(|p| p.id == *parent).map(place)),
                comment: (*c).clone(),
            })
            .collect()
    }
}

impl MemoryStore {
    fn claim_threads_lock(&self) -> MutexGuard<'_, ClaimThreads> {
        self.claim_threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn has_wiki(&self, id: &WikiId) -> bool {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|r| r.id == *id)
    }
}

impl ClaimThreadStore for MemoryStore {
    fn add_claim_comment(&self, comment: &ClaimCommentRecord) -> Result<()> {
        if !well_formed(&comment.claim) {
            return Err(StoreError::Backend(
                "a claim thread is named by the claim's id".to_owned(),
            ));
        }
        if comment.body.is_empty() {
            return Err(StoreError::Backend(
                "a comment in a claim thread has a text".to_owned(),
            ));
        }
        if !self.has_wiki(&comment.wiki) {
            return Err(StoreError::NotFound);
        }
        if !self.has_account(&comment.author) {
            return Err(StoreError::AccountNotFound);
        }
        let mut d = self.claim_threads_lock();
        if let Some(parent) = &comment.parent
            && !d
                .comments
                .iter()
                .any(|c| c.id == *parent && c.wiki == comment.wiki && c.claim == comment.claim)
        {
            return Err(StoreError::NotFound);
        }
        if d.comments.iter().any(|c| c.id == comment.id) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.comments.push(comment.clone());
        Ok(())
    }

    fn claim_comments(
        &self,
        wiki: &WikiId,
        claim: &str,
        after: Option<&CommentId>,
        limit: usize,
    ) -> Result<ClaimThreadPage> {
        let limit = limit.clamp(1, MAX_PAGE);
        let d = self.claim_threads_lock();
        let all = d.thread(wiki, claim);
        let start = match after {
            None => 0,
            Some(id) => {
                all.iter()
                    .position(|r| r.comment.id == *id)
                    .ok_or(StoreError::NotFound)?
                    + 1
            }
        };
        let total = u64::try_from(all.len()).unwrap_or(u64::MAX);
        let mut rows: Vec<ClaimCommentRow> = all.into_iter().skip(start).take(limit + 1).collect();
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = more
            .then(|| rows.last().map(|r| r.comment.id.clone()))
            .flatten();
        Ok(ClaimThreadPage { rows, total, next })
    }

    fn claim_comments_since(
        &self,
        wiki: &WikiId,
        author: &AccountId,
        since: i64,
    ) -> Result<Vec<i64>> {
        let d = self.claim_threads_lock();
        let mut times: Vec<i64> = d
            .comments
            .iter()
            .filter(|c| c.wiki == *wiki && c.author == *author && c.created_at >= since)
            .map(|c| c.created_at)
            .collect();
        times.sort_unstable();
        Ok(times)
    }

    fn claim_comments_in_wiki(&self, wiki: &WikiId) -> Result<Vec<ClaimCommentRow>> {
        let d = self.claim_threads_lock();
        let mut claims: Vec<&str> = d
            .comments
            .iter()
            .filter(|c| c.wiki == *wiki)
            .map(|c| c.claim.as_str())
            .collect();
        claims.sort_unstable();
        claims.dedup();
        Ok(claims
            .into_iter()
            .flat_map(|claim| d.thread(wiki, claim))
            .collect())
    }
}
