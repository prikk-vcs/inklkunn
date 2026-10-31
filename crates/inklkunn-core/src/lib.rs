#![forbid(unsafe_code)]
//! `inklkunn-core` — inklkunn's domain. M0 (RFC 001 D-5, D-9) establishes the **seams** later RFCs fill
//! and creates a wiki on disk as a Portable Wiki Bundle:
//!
//! - [`authorize`](mod@authorize): the one default-deny decision (RFC 002 D-2): the pure rule [`decide`] and the
//!   [`Authorizer`] that loads the facts itself, fails closed and audits consequential decisions.
//! - [`reconcile`]: the sealed history against the service's records (RFC 002 D-7), reported and never repaired.
//! - [`routes`]: the table of entry points, each guarded by an action or public with a reason.
//! - [`accept`]: the `Accept` trait (RFC 002 D-4), defined in `inklkunn-signer`, which implements it, and
//!   re-exported here; nothing else may implement it.
//! - [`assurance`]: the honesty view-model values (RFC 002 D-6).
//! - [`message`]: the patch message, the attribution contract written into every signed change (CT-03;
//!   RFC 002 D-5). The grammar lives in `inklkunn-signer`, which enforces it, and is re-exported here.
//! - [`egress`]: the one egress guard for every outbound fetch (threat model C-8).
//! - [`read`]: the read service (RFC 003 D-2): the view's tip, the title index of the tip, pages re-derived
//!   from prikk and cached by prikk's ids, the sanitizer pass every page goes through, and the counting
//!   harness that holds the number of prikk calls to a bound.
//! - [`check`]: the verification check (RFC 003 D-5): `verify` run on request, its outcome recorded as the
//!   wiki's last verification, and a check that could not run kept apart from a verdict.
//! - [`proposal`]: the proposal as a record with a life (RFC 004 D-1, D-3, D-8, D-10, D-11): the service, the
//!   state machine with every transition named, the quotas and the participation metrics (handoff 004 T1).
//! - [`lease`] and [`acceptance`]: the per-wiki write lease (a fair bounded queue over the signer's `flock`, with a
//!   durable holder record) and **acceptance under it** (RFC 004 D-4, D-6, D-14; handoff 004 T4): authorize,
//!   lease, re-read the tip, re-author a moved base, record the acceptance and its intent, `Accept`, read the
//!   outcome as typed, flag the wiki *needs recovery* when it is incomplete; the same path for the service's own
//!   changes (import, system).
//! - [`bundle`] and [`wiki`]: the repository layout (external design CT-02) and *create wiki*, which is
//!   kill-safe (a creation mark and a start-up sweep).
//! - [`merge`](mod@merge): the content-level three-way merge (RFC 004 D-5): frontmatter key by key, body block by block,
//!   pages matched by page id, an overlap never resolved by the code. Pure.
//! - [`import`] and [`export`]: `import-folder` (one accepted change under the system principal) and the
//!   Portable Wiki Bundle export at fidelity level 3 (handoff 003 T7; RFC 003 D-11, D-12).

pub mod accept;
pub mod acceptance;
pub mod assurance;
pub mod authorize;
pub mod bundle;
pub mod check;
pub mod egress;
pub mod export;
pub mod import;
pub mod inbox;
pub mod lease;
pub mod merge;
pub mod notify;
pub mod proposal;
pub mod read;
pub mod reconcile;
pub mod routes;
pub mod wiki;

pub use inklkunn_signer::message;

pub use accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ChangeDigest, ExpectedMessage,
    LedgerError, PreparedChange, PreparedFile, SealedChange, Unconfirmed, ViewCreationAct,
    ViewCreationFacts,
};
pub use acceptance::{
    AcceptFailure, AcceptanceParts, Accepted, AfterSeal, NoFollowUp, NoLedger, ProposalAcceptor,
    Reconciled, Signed, StoreActs, StoreLedger, SystemActFailure, SystemActs, SystemRequest,
};
pub use assurance::Assurance;
pub use authorize::{
    AccountPrincipal, Action, Authorizer, AuthzSource, Decision, IdentityAuthority, Principal,
    Resource, Snapshot, SourceError, StoreAuthz, SystemPrincipal, decide,
};
pub use check::{
    CONCURRENT_CHECKS, CheckOutcome, Checked, Declined, MIN_RECHECK_SECS, NotChecked, StageFinding,
    Verifier,
};
pub use egress::{EgressGuard, EgressRefusal, PinnedTarget, Resolver, StdEgressGuard, StdResolver};
pub use inbox::Inbox;
pub use inklkunn_signer::{
    ArchiveExported, KeyAuthority, KeyStatus, LateSeal, Maintenance, OldKey, OldKeyOutcome,
    Recover, RecoverError, RecoveryFacts, RecoveryOp, RecoveryToken, Rotation, Signer,
    SignerConfig, SignerError, SupersededKey, Witnesses, WriteLeases,
};
pub use lease::{Busy, BusyWhy, Lease, LeaseConfig, LeaseError, Leased, Purpose, WikiLeases};
pub use merge::{LineLevel, MergeOutcome, MergedPage, Overlap, PageRef, PageText, Place, merge};
pub use notify::{Channel, InApp, Notifier};
pub use proposal::{
    BaseSource, Invalid, Metrics, NewComment, Period, ProposalEdit, ProposalError, ProposalService,
    QuotaConfig, QuotaLimit, QuotaRefusal, Quotas, Reset,
};
pub use read::{
    Calls, Cause, CountingStorage, Found, Lookup, PageOutcome, PageView, Presentation, ReadConfig,
    ReadError, ReadService, SafeHtml, Sanitizer, Tip, TitleIndex,
};
pub use routes::{Guard, RouteEntry, RouteError, RouteTable};
pub use wiki::{
    Cleanup, CreateWikiError, CreatedWiki, CreationStep, NoSweepAudit, RepositoryBootstrap,
    SweepAudit, SweepReport, create_wiki, create_wiki_observed, creation_message,
    sweep_unfinished_creations, system_message,
};
