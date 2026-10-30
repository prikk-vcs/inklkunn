//! What the two orchestrators of the acceptance path ([`super::ProposalAcceptor`] and [`super::SystemActs`])
//! share, so that there is one way to do each of these things and not two (RFC 004 D-14: no second write path).

use std::path::Path;
use std::sync::Arc;

use inklkunn_prikk::{Point, Storage};
use inklkunn_store::{
    AcceptanceStore, ActStore, AuditSink, RecoveryFlag, RecoveryReason, RootLayout, StoreError,
    WikiRecord,
};

use super::{Reconciled, StoreActs};
use crate::lease::{Lease, Purpose};
use crate::reconcile::{ReadBounds, reconcile_with, record_findings};

/// Why a write was not started.
#[derive(Debug)]
pub(crate) enum Blocked {
    /// The wiki is flagged *needs recovery* (it was, or this check just flagged it).
    Flagged(RecoveryFlag),
    /// The check could not be made, or the flag could not be recorded (the text is for operators).
    Unavailable(String),
    /// A store failed.
    Store(StoreError),
}

impl From<StoreError> for Blocked {
    fn from(e: StoreError) -> Self {
        Blocked::Store(e)
    }
}

/// Record the wiki's flag; `true` when it is recorded (now, or already).
pub(crate) fn flag(
    acts: &dyn ActStore,
    wiki: &WikiRecord,
    reason: RecoveryReason,
    act: Option<String>,
    at: i64,
) -> bool {
    acts.set_recovery_flag(&RecoveryFlag {
        wiki: wiki.id.clone(),
        reason,
        act,
        since: at,
    })
    .is_ok()
}

/// A write starts only on a wiki whose last write finished (RFC 004 D-6): the wiki must not be flagged *needs
/// recovery*, must hold no open intent and no open system act, and the lease must not have replaced the record
/// of a holder that died in the middle of an act. Any of the last three flags the wiki now (*interrupted*), so
/// that the answer is the same ever after, until recovery clears it.
pub(crate) fn recovery_guard(
    acts: &dyn ActStore,
    acceptances: &dyn AcceptanceStore,
    wiki: &WikiRecord,
    lease: &Lease,
    at: i64,
) -> Result<(), Blocked> {
    if let Some(flag) = acts.recovery_flag(&wiki.id)? {
        return Err(Blocked::Flagged(flag));
    }
    let open_acceptance = acceptances.open_intents(&wiki.id)?.into_iter().next();
    let open_act = acts
        .system_acts(&wiki.id)?
        .into_iter()
        .find(|e| e.end.is_none());
    // An export or a refresh that died leaves nothing in the repository to recover.
    let died = lease.replaced().filter(|h| {
        h.purpose != Purpose::Export.as_str() && h.purpose != Purpose::Refresh.as_str()
    });
    let interrupted = open_acceptance.is_some() || open_act.is_some() || died.is_some();
    if !interrupted {
        return Ok(());
    }
    let act = open_acceptance
        .map(|i| i.intent.acceptance.to_string())
        .or_else(|| open_act.map(|a| a.act.id.to_string()))
        .or_else(|| died.and_then(|h| h.acceptance.as_ref().map(ToString::to_string)));
    flag(acts, wiki, RecoveryReason::Interrupted, act, at);
    match acts.recovery_flag(&wiki.id)? {
        Some(flag) => Err(Blocked::Flagged(flag)),
        None => Err(Blocked::Unavailable(
            "an earlier act did not finish and the wiki could not be flagged".to_owned(),
        )),
    }
}

/// Why the tip could not be read.
#[derive(Debug)]
pub(crate) enum TipError {
    /// The wiki has no open view of that name.
    NoSuchView,
    /// prikk could not be asked (the text is for operators).
    Unavailable(String),
}

/// The block the view points at, read now: `Storage::views` for the view, `log` for its newest block. Never a
/// held answer.
pub(crate) fn tip_block(
    storage: &dyn Storage,
    repo: &Path,
    view: &str,
) -> Result<String, TipError> {
    let views = storage
        .views(repo)
        .map_err(|e| TipError::Unavailable(e.to_string()))?;
    if !views.views.iter().any(|v| v.name == view && !v.closed) {
        return Err(TipError::NoSuchView);
    }
    let log = storage
        .history(repo, &Point::View(view.to_owned()), Some(1))
        .map_err(|e| TipError::Unavailable(e.to_string()))?;
    log.blocks
        .into_iter()
        .next()
        .map(|b| b.block_id)
        .ok_or_else(|| TipError::Unavailable("the view's log lists no block".to_owned()))
}

/// The reconciliation (RFC 002 D-7) of the wiki's history with the records, reported to the audit trail when it
/// finds something. Run with the lease held, so that no act of this process is between its record and its seal
/// while the records are read.
pub(crate) fn reconcile_wiki(
    storage: &dyn Storage,
    layout: &RootLayout,
    wiki: &WikiRecord,
    acceptances: &Arc<dyn AcceptanceStore>,
    acts: &Arc<dyn ActStore>,
    audit: Option<&Arc<dyn AuditSink>>,
    at: i64,
) -> Reconciled {
    let records = StoreActs::new(Arc::clone(acceptances), Arc::clone(acts));
    let repo = layout.repo_dir(&wiki.id);
    match reconcile_with(&wiki.id, &repo, storage, &records, ReadBounds::default()) {
        Ok(report) if report.is_clean() => Reconciled::Clean,
        Ok(report) => {
            if let Some(audit) = audit {
                let _ = record_findings(&report, &[], &**audit, at);
            }
            Reconciled::Findings(report.findings)
        }
        Err(e) => Reconciled::NotRun(e.to_string()),
    }
}
