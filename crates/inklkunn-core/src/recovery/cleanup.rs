//! The witness for a publication a killed `seal` left unfinished (handoff 004, P15; maintenance M61).
//!
//! Two states are read here. A `seal` killed after it moved the ref's pointer and before it appended the ref log leaves
//! `doctor` naming exactly one error, `PRIKK-VERIFY-REF-POINTER-LEADS-LOG` for the act's view, with the block in the history
//! and the patch still queued ([`CleanupPending::lead`]); the plan finishes that publication (`ref complete`) and then the
//! cleanup. A `seal` killed after the log and before the queue was drained leaves `doctor` naming the pending cleanup
//! (the rest of this text). Anything else is not read as either: the plan stops on it.
//!
//! A `seal` publishes its block and then drains the queue. Killed between the two, it leaves the block in the history
//! **and** the patch in the queue. Up to prikk 0.57.0 `prikk status --format json` refuses with an integrity error (it
//! replays the queued patch on a tip that already holds it); from 0.58.0 it reads, lists the queue and says
//! `queue.already_sealed` (the block the queue is already in; maintenance M51). `prikk worktree-status` refuses the
//! same way on both, and `prikk doctor` reports one error, `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING`, recommending
//! `prikk seal --allow-no-audit` for the ref. prikk's own answer is taken as a recovery step
//! ([`super::plan::Step::CompletePublication`]), but only on this witness: `doctor` names that error and, as the plan
//! then requires, nothing else; `status` either reads or refuses with an integrity error (any other refusal, or a call
//! that could not run, is not this state and stays a witness error). **The decision is the same on both:** the queue
//! is the tip block's patches (ledger PK-44); where 0.58.0 lists it, recovery checks that it is, and that the block
//! `status` says it is already sealed in is the block found in the history. Where it is not, that is a witness error
//! and nothing is done (a stop, not a step); the queue `status` lists decides nothing else.

use std::path::Path;

use inklkunn_prikk::{Point, RepoStatus, Storage, StorageError, schema, storage::Queue};
use inklkunn_signer::recover::text::{DoctorReport, Severity};

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

/// Read the state, when it is the state: `doctor` names one pending cleanup (or, for a `seal` killed before it appended
/// the ref log, exactly one interrupted publication of the ref of the one unfinished act's view: maintenance M61), and
/// `status` is readable or refuses with an integrity error. The newest block of the view of the one unfinished act is
/// read from the history. (That `doctor` names **nothing else** is the plan's to judge, as a stop with a name; a
/// `status` that refuses with no pending cleanup to explain it is a witness that cannot be read.)
pub(super) fn read(
    storage: &dyn Storage,
    repo: &Path,
    status: &Result<RepoStatus, StorageError>,
    doctor: &DoctorReport,
    acts: &[ActWitness],
) -> Result<Option<CleanupPending>, RecoveryError> {
    let named = doctor
        .diagnostics
        .iter()
        .filter(|d| d.is_cleanup_pending())
        .count();
    // A `seal` killed after it moved the ref's pointer and before it appended the ref log (M61): `doctor` names exactly
    // one thing, prikk's own `PRIKK-VERIFY-REF-POINTER-LEADS-LOG` for the ref of the one unfinished act's view. Any
    // other lead (another ref, a second error, another wording) is not this state, and the plan stops on it.
    let lead = match acts {
        [act] if named == 0 => doctor.only_ref_lead().filter(|r| *r == act.view),
        _ => None,
    };
    // prikk 0.58.0 says the queue is already sealed while `doctor` names no error at all: two answers of prikk's that
    // disagree (maintenance M51), and nothing is concluded from either. (Beside a pending cleanup or an interrupted
    // publication that a `seal` left it is the state, not a contradiction; beside any other error it is that error's
    // business, which the plan stops on.)
    if !doctor
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
        && let Ok(read) = status
        && let Some(said) = &read.queue.already_sealed
    {
        return Err(witness_error(
            "prikk status",
            format!(
                "it says the queue is already sealed in block {}, while prikk doctor names no error at all",
                said.block_id
            ),
        ));
    }
    if named != 1 && lead.is_none() {
        return Ok(None);
    }
    let Some(status_refused) = is_integrity_refusal(status) else {
        return Ok(None);
    };
    // An interrupted publication beside a `status` that reads an ordinary queue is not a state recovery acts on: the
    // queue is not sealed in the block the pointer leads to. The plan stops on the lead.
    if lead.is_some()
        && let Ok(read) = status
        && read.queue.already_sealed.is_none()
    {
        return Ok(None);
    }
    let mut tip_patches: Vec<String> = Vec::new();
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
                tip_patches = tip
                    .patch_messages
                    .iter()
                    .map(|m| m.patch_id.clone())
                    .collect();
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
    // prikk 0.58.0 reads the state, and what it says must agree with the history (maintenance M51; up to 0.57.0
    // `status` refuses, and the signer's step rests on prikk's own guarantee, ledger PK-44). The block it says the
    // queue is already sealed in is the block the history shows at the view's tip; the queue it lists is that block's
    // patches and no others; and `status` does not read an ordinary queue while `doctor` names a pending cleanup.
    // Where two witnesses of prikk's disagree nothing is concluded from either. Nothing else of `already_sealed` is
    // read (not its `route`).
    let already_sealed = match (status, &published) {
        (Ok(read), Some(tip)) => {
            let said = match &read.queue.already_sealed {
                Some(said) if said.block_id == tip.block_id => said,
                Some(said) => {
                    return Err(witness_error(
                        "prikk status",
                        format!(
                            "it says the queue is already sealed in block {}, but the newest block of the view is {}",
                            said.block_id, tip.block_id
                        ),
                    ));
                }
                None => {
                    return Err(witness_error(
                        "prikk status",
                        "it reads an ordinary queue, while prikk doctor names a pending cleanup of a publication",
                    ));
                }
            };
            let mut queued: Vec<&str> = read
                .queue
                .patches
                .iter()
                .map(|p| p.patch_id.as_str())
                .collect();
            let mut sealed: Vec<&str> = tip_patches.iter().map(String::as_str).collect();
            queued.sort_unstable();
            sealed.sort_unstable();
            if queued != sealed {
                return Err(witness_error(
                    "prikk status",
                    "the queue it lists is not the patches of the newest block of the view",
                ));
            }
            Some(said.block_id.clone())
        }
        (Ok(read), None) => read
            .queue
            .already_sealed
            .as_ref()
            .map(|said| said.block_id.clone()),
        (Err(_), _) => None,
    };
    Ok(Some(CleanupPending {
        status_refused,
        already_sealed,
        published,
        lead: lead.map(str::to_owned),
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
            already_sealed: None,
        },
    }
}
