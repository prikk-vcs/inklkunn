//! What the pages are built on: the stores and services a request asks (RFC 003 D-2, D-5). A handler holds
//! these and nothing else; the binary builds them, a test builds them over doubles.
//!
//! The web crate **never makes a principal**: [`Identify`] is the one seam through which a request becomes one
//! (the binary builds the real one over the sign-in, [`crate::identity::AuthIdentity`]), and [`AnonymousOnly`]
//! is the answer where no sign-in is served. A handler
//! gets a [`inklkunn_core::Principal`] and asks [`inklkunn_core::Authorizer::authorize`] about it; it cannot
//! build an account principal (the compiler and the boundary gate agree).

use std::sync::Arc;

use axum::http::HeaderMap;
use inklkunn_core::proposal::ProposalService;
use inklkunn_core::{
    AcceptFailure, Accepted, Authorizer, Inbox, Principal, ProposalAcceptor, ReadService, Verifier,
};
use inklkunn_store::{AcceptanceStore, AccountStore, ActStore, ProposalId, WikiRecord, WikiStore};

/// A request's live session, for drawing a page: whose it is and the token every form of it must carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    /// The account the session is of. The account page acts on this one and no other.
    pub account: inklkunn_store::AccountId,
    /// The name the header shows (the account's display name).
    pub display_name: String,
    /// The session's CSRF synchronizer token: the hidden field of every form a signed-in person can submit.
    pub csrf: String,
}

/// Who a request that changes something is (handoff 004 P6; the sign-in handoff's U4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteIdentity {
    /// The request's principal: anonymous when it presents no credential (or a session that is over), the
    /// token's account when it presents a bearer token that authenticates, the session's account when it
    /// presents a session **and the session's CSRF token**.
    Principal(Principal),
    /// A live session, and the token that must come with it is missing or is another session's: refuse the
    /// write and say the form expired.
    CsrfMismatch,
    /// The request presents a bearer token that does not authenticate (or one that cannot be read as a token):
    /// `invalid-token`, never a fallback to a cookie it may also carry.
    InvalidToken,
}

/// Who a request is. The only way the web layer learns a principal.
pub trait Identify: Send + Sync {
    /// The principal of the request with these headers, **for reading**: the account of its session, or of the
    /// API token it presents as `Authorization: Bearer …` (RFC 004 D-13); else anonymous. **A request that
    /// presents a token is its token's, whatever cookies it also carries**: a stale session must not widen or
    /// narrow what a token may do, and the write API tells a request that presented a credential that did not
    /// authenticate (it answers `invalid-token`) from one that presented none by the header, so an
    /// implementation answers anonymous for a token that is unknown, expired, revoked or of a disabled account.
    /// The principal of a token carries the token's scope, which the authorizer intersects with the account's
    /// role (AZ-3). Never a system principal.
    fn principal(&self, headers: &HeaderMap) -> Principal;

    /// A cheap answer, with no lookup, to "could this request have a live session?": `false` only when it
    /// certainly has none (no session cookie, or a bearer token that judges it alone). A page and the CSRF layer
    /// ask [`Identify::session`] only when this is `true`. The conservative default is `true`.
    fn may_be_signed_in(&self, _headers: &HeaderMap) -> bool {
        true
    }

    /// The live session of the request, for drawing a page. `None` for a request that presents a bearer token
    /// (the token alone judges it), and for one with no live session.
    fn session(&self, headers: &HeaderMap) -> Option<SessionView>;

    /// Who a request that **changes something** is. `csrf` is the token the request presented (the form's
    /// hidden field, or the header of a client that is not a form). A session is never believed on its cookie
    /// alone: the cookie and the token are checked together. The caller checks this **before it looks at any
    /// wiki**, so a refusal says nothing about which wikis exist.
    fn for_write(&self, headers: &HeaderMap, csrf: Option<&str>) -> WriteIdentity;
}

/// Everyone is anonymous: a deployment before sign-in is served, and every test of a public wiki.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnonymousOnly;

impl Identify for AnonymousOnly {
    fn principal(&self, _: &HeaderMap) -> Principal {
        Principal::Anonymous
    }

    fn may_be_signed_in(&self, _: &HeaderMap) -> bool {
        false
    }

    fn session(&self, _: &HeaderMap) -> Option<SessionView> {
        None
    }

    fn for_write(&self, _: &HeaderMap, _: Option<&str>) -> WriteIdentity {
        WriteIdentity::Principal(Principal::Anonymous)
    }
}

/// What takes a proposal into the history (RFC 004 D-4): the one seam through which the web layer accepts. It
/// blocks the calling thread (the lease waits its turn, prikk is asked); a handler calls it on a blocking
/// thread. [`ProposalAcceptor`] is the implementation; a test stands a double behind it to show each outcome's
/// page.
pub trait Acceptor: Send + Sync {
    /// Accept revision `revision` of the proposal, as `principal`.
    fn accept(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        revision: u32,
    ) -> Result<Accepted, AcceptFailure>;

    /// Can this acceptor take a proposal into the history at all? `false` for [`NoSigner`]: the server was
    /// started without the means to sign, so a handler says that, in one plain sentence, after the checks an
    /// acceptance makes first and without asking [`Acceptor::accept`].
    fn is_configured(&self) -> bool {
        true
    }
}

/// The acceptor of a server that was started **without** the means to sign (no operator secret file, or a key
/// store that could not be opened): it serves every read and refuses every write with one plain sentence, and
/// the operator was told why once, at start. It asks nothing of anyone, and it never asks for the secret.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSigner;

impl Acceptor for NoSigner {
    fn accept(
        &self,
        _: &Principal,
        _: &WikiRecord,
        _: &ProposalId,
        _: u32,
    ) -> Result<Accepted, AcceptFailure> {
        // The handlers do not get here (`is_configured` is `false`); if something else does, nothing happens.
        Err(AcceptFailure::Unavailable(
            "this server was started without the means to sign".to_owned(),
        ))
    }

    fn is_configured(&self) -> bool {
        false
    }
}

impl Acceptor for ProposalAcceptor {
    fn accept(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        revision: u32,
    ) -> Result<Accepted, AcceptFailure> {
        ProposalAcceptor::accept(self, principal, wiki, id, revision)
    }
}

/// Everything a wiki page needs, shared by every handler.
#[derive(Clone)]
pub struct Services {
    /// The wiki records. Read for every request, so the badge is the record's, never a cache's.
    pub wikis: Arc<dyn WikiStore>,
    /// The read service: tips, indexes, pages.
    pub read: Arc<ReadService>,
    /// Runs `verify` on request and records the outcome.
    pub verifier: Arc<Verifier>,
    /// May this principal read this wiki?
    pub authorizer: Arc<Authorizer>,
    /// The inbox and the watches of accounts (RFC 004 D-9): what the person is told, read as the person.
    pub inbox: Arc<Inbox>,
    /// Who a request is.
    pub identity: Arc<dyn Identify>,
    /// The proposal service: every call authorizes first, and a draft is its author's alone (RFC 004 D-1).
    pub proposals: Arc<ProposalService>,
    /// The accounts, to name the people a proposal's page mentions.
    pub accounts: Arc<dyn AccountStore>,
    /// The acceptance records, to link an accepted proposal to its change.
    pub acceptances: Arc<dyn AcceptanceStore>,
    /// The overlaps an acceptance found, kept on the proposal: shown where the proposal needs a revision (D-5).
    pub acts: Arc<dyn ActStore>,
    /// What accepts a proposal. `None`: accepting is not served by this build, and the page says so. [`NoSigner`]:
    /// it is served, but this server was started without the means to sign, and the page says that.
    pub acceptor: Option<Arc<dyn Acceptor>>,
    /// Where a cause meant for the operator goes (a page never shows it).
    pub log: Arc<dyn Fn(&str) + Send + Sync>,
    /// The clock, in Unix seconds.
    pub clock: fn() -> i64,
    /// Sign-in, sessions and tokens (the sign-in handoff's U1, U2). `None`: no sign-in is served, and no page
    /// offers it.
    pub auth: Option<Arc<inklkunn_auth::Authenticator>>,
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Services")
    }
}
