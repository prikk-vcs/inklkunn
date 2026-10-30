//! Changes the service makes for itself (RFC 004 D-14; RFC 003 D-12's `import-folder`): **the same path as a
//! proposal's acceptance**, under the system principal, with `Kind: import` or `Kind: system`. There is no
//! second way into a wiki's history (INV-4).
//!
//! The steps are D-4's, less the ones that are about a proposal: `authorize` for the system principal, the
//! wiki's write lease, the guard against an unfinished earlier act, the **record of the act** (it plays the
//! acceptance's part and the intent's, since there is no proposal to accept: the CT-03 message, its digest, the
//! digest of the files and the tip the change is written on), `Accept::accept` with the act's id as the token,
//! the signer's answer read as typed, the end recorded, the caches forgotten, the reconciliation run, the lease
//! released.
//!
//! The signer reads the act back through [`StoreLedger`](super::StoreLedger) as it reads an acceptance: the
//! command line has no way to hand it facts of its own.

use std::path::PathBuf;
use std::sync::Arc;

use inklkunn_prikk::Storage;
use inklkunn_signer::accept::{Accept, AcceptanceToken, PreparedChange, PreparedFile, Unconfirmed};
use inklkunn_signer::message::{ImportSource, Message, Summary, SystemLabel};
use inklkunn_store::{
    AcceptanceOutcome, AcceptanceStore, ActStore, AuditSink, RecoveryFlag, RecoveryReason,
    RootLayout, StoreError, SystemActEnd, SystemActId, SystemActKind, SystemActRecord, WikiRecord,
};

use super::shared::{self, Blocked, TipError};
use super::{AfterSeal, NoFollowUp, Reconciled, Settled, message_digest, settle};
use crate::authorize::{Action, Authorizer, Principal, Resource};
use crate::lease::{Busy, LeaseError, Purpose, WikiLeases};
use crate::read::ReadService;

/// A change the service makes for itself.
#[derive(Debug, Clone)]
pub struct SystemRequest {
    /// `Import` or `System`.
    pub kind: SystemActKind,
    /// The first line of the message (made one line, and cut to the length CT-03 allows).
    pub summary: String,
    /// The files to write or remove.
    pub files: Vec<PreparedFile>,
    /// The renames to declare.
    pub renames: Vec<(PathBuf, PathBuf)>,
    /// Where an import came from (required for `Import`, refused for `System`).
    pub imported_from: Option<ImportSource>,
    /// The block of the tip the change was planned against: the signer writes it only if the view is still
    /// there. `None` pins the tip read under the lease.
    pub planned_against: Option<String>,
}

/// A system act that is in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemSigned {
    /// The record of the act.
    pub act: SystemActId,
    /// The block.
    pub block_id: String,
    /// The patch.
    pub patch_id: String,
    /// The signer could not confirm the seal afterwards (the change **is** sealed); the wiki is flagged.
    pub unconfirmed: Option<Unconfirmed>,
    /// A temporary directory of decrypted seeds could not be removed.
    pub seeds_left: bool,
    /// What the reconciliation found.
    pub reconciliation: Reconciled,
}

/// Why a system act is not in the history. Each variant says what state things are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemActFailure {
    /// The system principal may not do this to this wiki. Nothing was done.
    NotAllowed,
    /// *The wiki is busy* (with a depth and an estimate). Nothing was done.
    Busy(Busy),
    /// *The wiki needs recovery*. Nothing was done by this call.
    NeedsRecovery(RecoveryFlag),
    /// The request cannot be described to the signer. Nothing was done.
    Invalid(String),
    /// Failed **before `commit`**: nothing in prikk changed; the act is recorded as failed with `word`.
    Failed {
        /// The act.
        act: SystemActId,
        /// The fixed word ([`words`]).
        word: &'static str,
        /// What happened, for operators.
        detail: String,
    },
    /// Failed **after** something may have been committed: the act stays open, the wiki is flagged *needs
    /// recovery*, and the change is not made again.
    Incomplete {
        /// The act.
        act: SystemActId,
        /// Why the wiki needs recovery.
        reason: RecoveryReason,
        /// Whether the flag was recorded.
        flagged: bool,
        /// What happened, for operators.
        detail: String,
    },
    /// The repository, the lease or the records could not be had before anything was written. Nothing was done.
    Unavailable(String),
    /// A store failed.
    Store(StoreError),
}

impl std::fmt::Display for SystemActFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemActFailure::NotAllowed => write!(f, "not allowed"),
            SystemActFailure::Busy(b) => write!(
                f,
                "the wiki is busy ({} ahead, about {} s)",
                b.depth,
                b.estimate.as_secs()
            ),
            SystemActFailure::NeedsRecovery(flag) => {
                write!(f, "the wiki needs recovery ({})", flag.reason.as_str())
            }
            SystemActFailure::Invalid(why) => write!(f, "{why}"),
            SystemActFailure::Failed { word, detail, .. } => {
                write!(f, "failed ({word}); nothing was committed: {detail}")
            }
            SystemActFailure::Incomplete { reason, detail, .. } => write!(
                f,
                "incomplete ({}); the wiki needs recovery: {detail}",
                reason.as_str()
            ),
            SystemActFailure::Unavailable(why) => write!(f, "could not be done now: {why}"),
            SystemActFailure::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SystemActFailure {}

impl From<StoreError> for SystemActFailure {
    fn from(e: StoreError) -> Self {
        SystemActFailure::Store(e)
    }
}

/// The path of the service's own changes.
pub struct SystemActs {
    authorizer: Authorizer,
    acts: Arc<dyn ActStore>,
    acceptances: Arc<dyn AcceptanceStore>,
    storage: Arc<dyn Storage>,
    layout: RootLayout,
    signer: Arc<dyn Accept>,
    leases: Arc<WikiLeases>,
    reads: Option<Arc<ReadService>>,
    follow_up: Arc<dyn AfterSeal>,
    audit: Option<Arc<dyn AuditSink>>,
    reconcile: bool,
    clock: fn() -> i64,
}

impl std::fmt::Debug for SystemActs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SystemActs")
    }
}

impl SystemActs {
    /// The service's own path into `signer`'s wikis, authorizing through `authorizer` (which records the
    /// consequential decision), writing its records to `acts` and `acceptances`, under `leases` (which must be
    /// the ones `signer` was opened over, and whose store is `acceptances`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        authorizer: Authorizer,
        acts: Arc<dyn ActStore>,
        acceptances: Arc<dyn AcceptanceStore>,
        storage: Arc<dyn Storage>,
        layout: RootLayout,
        signer: Arc<dyn Accept>,
        leases: Arc<WikiLeases>,
    ) -> Self {
        SystemActs {
            authorizer,
            acts,
            acceptances,
            storage,
            layout,
            signer,
            leases,
            reads: None,
            follow_up: Arc::new(NoFollowUp),
            audit: None,
            reconcile: true,
            clock: inklkunn_store::record::now_secs,
        }
    }

    /// Forget this read service's caches of the wiki after a seal (a process that serves pages has one; the
    /// command line has none).
    #[must_use]
    pub fn with_reads(mut self, reads: Arc<ReadService>) -> Self {
        self.reads = Some(reads);
        self
    }

    /// Ask `follow_up` for the title index and the verification after a seal.
    #[must_use]
    pub fn with_follow_up(mut self, follow_up: Arc<dyn AfterSeal>) -> Self {
        self.follow_up = follow_up;
        self
    }

    /// Append the reconciliation's findings to this audit trail.
    #[must_use]
    pub fn with_audit(mut self, audit: Arc<dyn AuditSink>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Run the reconciliation after a seal (the default) or not.
    #[must_use]
    pub fn with_reconciliation(mut self, on: bool) -> Self {
        self.reconcile = on;
        self
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    /// Make `request`'s change to `wiki` as the service itself, named `label` in the history. Blocks the
    /// calling thread.
    pub fn run(
        &self,
        wiki: &WikiRecord,
        label: &SystemLabel,
        request: SystemRequest,
    ) -> Result<SystemSigned, SystemActFailure> {
        // The message first: a request that cannot be described is refused before anything else is touched.
        let described =
            |e: inklkunn_signer::message::MessageError| SystemActFailure::Invalid(e.to_string());
        let summary = Summary::from_text(&request.summary).map_err(described)?;
        let message = match (request.kind, &request.imported_from) {
            (SystemActKind::Import, Some(source)) => {
                Message::import(summary, label.clone(), source.clone())
            }
            (SystemActKind::Import, None) => {
                return Err(SystemActFailure::Invalid(
                    "an import names where it came from".to_owned(),
                ));
            }
            (SystemActKind::System, None) => Message::system(summary, label.clone()),
            (SystemActKind::System, Some(_)) => {
                return Err(SystemActFailure::Invalid(
                    "a system change comes from nowhere but the service".to_owned(),
                ));
            }
        };
        let text = message.to_text();

        // 0. Authorization, first: the system principal, for the act's own action.
        let principal = Principal::system(label.clone());
        let action = match request.kind {
            SystemActKind::Import => Action::Import,
            SystemActKind::System => Action::Accept,
        };
        let resource = Resource::Wiki {
            wiki: wiki.id.clone(),
        };
        if !self
            .authorizer
            .authorize(&principal, action, &resource)
            .is_allowed()
        {
            return Err(SystemActFailure::NotAllowed);
        }

        // 1. The lease.
        let lease = self
            .leases
            .acquire(&wiki.id, Purpose::System, None)
            .map_err(|e| match e {
                LeaseError::Busy(busy) => SystemActFailure::Busy(busy),
                LeaseError::Unavailable(why) => SystemActFailure::Unavailable(why),
            })?;
        shared::recovery_guard(
            &*self.acts,
            &*self.acceptances,
            wiki,
            &lease,
            (self.clock)(),
        )
        .map_err(|b| match b {
            Blocked::Flagged(flag) => SystemActFailure::NeedsRecovery(flag),
            Blocked::Unavailable(why) => SystemActFailure::Unavailable(why),
            Blocked::Store(e) => SystemActFailure::Store(e),
        })?;

        // 2. The tip the change is written on.
        let repo = self.layout.repo_dir(&wiki.id);
        let tip =
            shared::tip_block(&*self.storage, &repo, &wiki.default_view).map_err(|e| match e {
                TipError::NoSuchView => {
                    SystemActFailure::Unavailable("the wiki has no open default view".to_owned())
                }
                TipError::Unavailable(why) => SystemActFailure::Unavailable(why),
            })?;
        let pinned = request.planned_against.clone().unwrap_or(tip);

        // 4. The record of the act, before the signer is asked.
        let change = PreparedChange {
            files: request.files,
            renames: request.renames,
            message: text.clone(),
        };
        let bytes = super::prepare::change_bytes(&change.files, &change.renames);
        let act = SystemActRecord {
            id: SystemActId::new(),
            wiki: wiki.id.clone(),
            view: wiki.default_view.clone(),
            kind: request.kind,
            base_block: pinned,
            message_digest: message_digest(&text),
            message: text,
            digest: change.digest().to_hex(),
            bytes,
            created_at: (self.clock)(),
        };
        self.acts.begin_system_act(&act)?;

        // 5. The signer, on this thread, inside the lease.
        let token = AcceptanceToken::new(act.id.as_str());
        let answer = {
            let _scope = lease.scope();
            self.signer
                .accept(&wiki.id, &wiki.default_view, &token, &change)
        };
        match settle(answer) {
            Settled::Failed { word, detail } => {
                let _ = self.acts.finish_system_act(&SystemActEnd {
                    act: act.id.clone(),
                    outcome: AcceptanceOutcome::Failed {
                        word: word.to_owned(),
                    },
                    at: (self.clock)(),
                });
                Err(SystemActFailure::Failed {
                    act: act.id,
                    word,
                    detail,
                })
            }
            Settled::Incomplete { reason, detail } => {
                let flagged = shared::flag(
                    &*self.acts,
                    wiki,
                    reason,
                    Some(act.id.to_string()),
                    (self.clock)(),
                );
                Err(SystemActFailure::Incomplete {
                    act: act.id,
                    reason,
                    flagged,
                    detail,
                })
            }
            Settled::Sealed {
                block_id,
                patch_id,
                unconfirmed,
                seeds_left,
            } => {
                let ended = self.acts.finish_system_act(&SystemActEnd {
                    act: act.id.clone(),
                    outcome: AcceptanceOutcome::Sealed {
                        block_id: block_id.clone(),
                        patch_id: patch_id.clone(),
                    },
                    at: (self.clock)(),
                });
                if unconfirmed.is_some() || ended.is_err() {
                    // Sealed, and a person looks at why it could not be confirmed (or recorded).
                    shared::flag(
                        &*self.acts,
                        wiki,
                        RecoveryReason::SealedUnconfirmed,
                        Some(act.id.to_string()),
                        (self.clock)(),
                    );
                }
                if let Err(e) = ended {
                    return Err(SystemActFailure::Incomplete {
                        act: act.id,
                        reason: RecoveryReason::SealedUnconfirmed,
                        flagged: true,
                        detail: format!(
                            "the change is sealed (block {block_id}) and its record could not be completed: {e}"
                        ),
                    });
                }
                if let Some(reads) = &self.reads {
                    reads.forget_wiki(wiki);
                }
                let reconciliation = if self.reconcile {
                    shared::reconcile_wiki(
                        &*self.storage,
                        &self.layout,
                        wiki,
                        &self.acceptances,
                        &self.acts,
                        self.audit.as_ref(),
                        (self.clock)(),
                    )
                } else {
                    Reconciled::Skipped
                };
                drop(lease);
                self.follow_up.warm_title_index(wiki);
                self.follow_up.enqueue_verification(wiki);
                Ok(SystemSigned {
                    act: act.id,
                    block_id,
                    patch_id,
                    unconfirmed,
                    seeds_left,
                    reconciliation,
                })
            }
        }
    }
}
