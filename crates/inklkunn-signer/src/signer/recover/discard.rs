//! Taking a sound queued patch back, on purpose (maintenance M3; prikk 0.57.0, ledger PK-43).
//!
//! Until 0.57.0 prikk had no verb that removes a sound queued patch (`doctor --discard-damaged-commits` acts on a
//! commit the log no longer holds soundly and refuses over anything else), so recovery stopped on a queued patch that no
//! recorded act describes (`foreign-queued-patch`) and left it to a person. 0.57.0 has `prikk uncommit <patch>
//! [--plan-only]`: it takes the **newest** queued commit out of the queue, keeps it in its recovery log (`doctor
//! --recovery-restore <run>` puts it back), and leaves the files of the worktree alone.
//!
//! **This is the only place recovery uses it, and only for a patch a person confirmed by its id.** The plan records the
//! id; the signer does the step only if the recorded act names that id, the queue holds exactly that patch, and
//! prikk's own plan for it reads. It is never taken on recovery's own judgement: a patch that is the change of a
//! recorded act is sealed (or stopped on), never discarded.

use inklkunn_store::WikiId;

use super::{RecoverError, RecoveryOp, RecoveryToken, Signer, mismatch};
use crate::recover::text::Discarded;

impl Signer {
    /// See [`crate::recover::Recover::discard_queued`].
    pub(super) fn discard_queued_op(
        &self,
        wiki: &WikiId,
        act: &RecoveryToken,
        patch_id: &str,
    ) -> Result<Discarded, RecoverError> {
        let op = RecoveryOp::DiscardQueued {
            patch: patch_id.to_owned(),
        };
        let _held = self.recovering(wiki, act, &op)?;
        let repo = self.root.repo_dir(wiki);
        let w = &self.writes;
        // What the repository holds: this one patch queued, nothing torn, nothing interrupted.
        let status = w.status_full(&repo)?;
        if status.trailing_partial_wal_bytes != 0
            || status.provisional_worktree.is_some()
            || status.interrupted_materialization.is_some()
        {
            return mismatch(
                "the repository has a torn log or an interrupted state besides the queue",
            );
        }
        let [patch] = status.queue.patches.as_slice() else {
            return mismatch("the queue does not hold exactly one patch");
        };
        if patch.patch_id != patch_id {
            return mismatch("the queued patch is not the one that was confirmed");
        }
        // prikk's own plan for it must read, and name this patch on the ref the queue targets.
        let Some(plan) = w.uncommit_plan(&repo, patch_id)? else {
            return Err(RecoverError::Unavailable(
                "this prikk has no `uncommit` (it came with 0.57.0)".to_owned(),
            ));
        };
        if plan.patch_id != patch_id
            || status.queue.target_ref.as_deref() != Some(plan.target.as_str())
        {
            return mismatch("prikk's plan is not for this patch on the ref the queue targets");
        }
        let run = w.uncommit(&repo, patch_id)?;
        // It did what was meant: nothing is queued now.
        let after = w.status_full(&repo)?;
        if !after.queue.patches.is_empty() {
            return mismatch("prikk still reports a queued patch after the step");
        }
        Ok(Discarded {
            patch_id: Some(patch_id.to_owned()),
            recovery_run: Some(run),
            worktree_may_hold_content: true,
        })
    }
}
