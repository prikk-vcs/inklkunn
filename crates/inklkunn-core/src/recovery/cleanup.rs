//! The witness for a publication a killed `seal` left unfinished (handoff 004, P15).
//!
//! A `seal` publishes its block and then drains the queue. Killed between the two, it leaves the block in the history
//! **and** the patch in the queue: `prikk status --format json` refuses with an integrity error (it replays the queued
//! patch on a tip that already holds it), `prikk worktree-status` refuses the same way, and `prikk doctor` reports one
//! error, `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING`, recommending `prikk seal --allow-no-audit` for the ref. prikk's own
//! answer is taken as a recovery step ([`super::plan::Step::CompletePublication`]), but only on this witness: `doctor`
//! names that error and, as the plan then requires, nothing else; `status` either reads or refuses with an integrity
//! error (any other refusal, or a call that could not run, is not this state and stays a witness error).

use std::path::Path;

use inklkunn_prikk::{Point, RepoStatus, Storage, StorageError, schema, storage::Queue};
use inklkunn_signer::recover::text::DoctorReport;

use super::plan::{ActWitness, CleanupPending, Published};
use super::{RecoveryError, witness_error};
use crate::acceptance::message_digest;

/// Is `status` a refusal of the kind this state makes: `prikk status` itself failing an integrity check.
fn is_integrity_refusal(status: &Result<RepoStatus, StorageError>) -> Option<bool> {
    match status {
        Ok(_) => Some(false),
        Err(StorageError::Refused {
            verb: "status",
            message,
            ..
        }) if message.starts_with("integrity error") => Some(true),
        Err(_) => None,
    }
}

/// Read the state, when it is the state: `doctor` names one pending cleanup, and `status` is readable or refuses with
/// an integrity error. The newest block of the view of the one unfinished act is read from the history. (That
/// `doctor` names **nothing else** is the plan's to judge, as a stop with a name; a `status` that refuses with no
/// pending cleanup to explain it is a witness that cannot be read.)
pub(super) fn read(
    storage: &dyn Storage,
    repo: &Path,
    status: &Result<RepoStatus, StorageError>,
    doctor: &DoctorReport,
    acts: &[ActWitness],
) -> Result<Option<CleanupPending>, RecoveryError> {
    if doctor
        .diagnostics
        .iter()
        .filter(|d| d.is_cleanup_pending())
        .count()
        != 1
    {
        return Ok(None);
    }
    let Some(status_refused) = is_integrity_refusal(status) else {
        return Ok(None);
    };
    let published = match acts {
        [act] => {
            let history = storage
                .history(repo, &Point::View(act.view.clone()), Some(2))
                .map_err(|e| witness_error("prikk log", e))?;
            let mut blocks = history.blocks.iter();
            blocks.next().map(|tip| {
                let parent = blocks
                    .next()
                    .filter(|p| tip.previous_ref_state_id.as_ref() == Some(&p.ref_state_id))
                    .map(|p| p.block_id.clone());
                Published {
                    view: act.view.clone(),
                    block_id: tip.block_id.clone(),
                    parent,
                    message_digests: tip
                        .patch_messages
                        .iter()
                        .map(|m| message_digest(&m.message))
                        .collect(),
                }
            })
        }
        _ => None,
    };
    Ok(Some(CleanupPending {
        status_refused,
        published,
    }))
}

/// What stands in for `status` when it refused: no queue is read, no torn byte and no interrupted state is claimed.
/// The witness line says the queue could not be read; nothing decides on these fields.
pub(super) fn unread_status(view: Option<&str>) -> RepoStatus {
    RepoStatus {
        schema_version: schema::STATUS_REPORT.to_owned(),
        current_branch: view.map(str::to_owned),
        active_wal_records: 0,
        trailing_partial_wal_bytes: 0,
        provisional_worktree: None,
        interrupted_materialization: None,
        queue: Queue {
            count: 0,
            target_ref: None,
            target_ref_status: None,
            patches: Vec::new(),
        },
    }
}
