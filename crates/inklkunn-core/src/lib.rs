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
//! - [`bundle`] and [`wiki`]: the repository layout (external design CT-02) and *create wiki*.

pub mod accept;
pub mod assurance;
pub mod authorize;
pub mod bundle;
pub mod egress;
pub mod reconcile;
pub mod routes;
pub mod wiki;

pub use inklkunn_signer::message;

pub use accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ExpectedMessage, LedgerError,
    PreparedChange, PreparedFile, SealedChange, ViewCreationAct, ViewCreationFacts,
};
pub use assurance::Assurance;
pub use authorize::{
    Action, Authorizer, AuthzSource, Decision, Principal, Resource, Snapshot, SourceError,
    StoreAuthz, SystemPrincipal, decide,
};
pub use egress::{EgressGuard, EgressRefusal, PinnedTarget, Resolver, StdEgressGuard, StdResolver};
pub use inklkunn_signer::{KeyStatus, Signer, SignerConfig, SignerError};
pub use routes::{Guard, RouteEntry, RouteError, RouteTable};
pub use wiki::{
    Cleanup, CreateWikiError, CreatedWiki, RepositoryBootstrap, create_wiki, creation_message,
    system_message,
};
