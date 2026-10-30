//! What the pages are built on: the stores and services a request asks (RFC 003 D-2, D-5). A handler holds
//! these and nothing else; the binary builds them, a test builds them over doubles.
//!
//! The web crate **never makes a principal**: [`Identify`] is the one seam through which a request becomes one
//! (the binary implements it over the sign-in), and [`AnonymousOnly`] is the answer until it does. A handler
//! gets a [`inklkunn_core::Principal`] and asks [`inklkunn_core::Authorizer::authorize`] about it; it cannot
//! build an account principal (the compiler and the boundary gate agree).

use std::sync::Arc;

use axum::http::HeaderMap;
use inklkunn_core::{Authorizer, Inbox, Principal, ReadService, Verifier};
use inklkunn_store::WikiStore;

/// Who a request is. The only way the web layer learns a principal.
pub trait Identify: Send + Sync {
    /// The principal of the request with these headers: the account of its session, else anonymous. Never a
    /// system principal.
    fn principal(&self, headers: &HeaderMap) -> Principal;
}

/// Everyone is anonymous: a deployment before sign-in is served, and every test of a public wiki.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnonymousOnly;

impl Identify for AnonymousOnly {
    fn principal(&self, _: &HeaderMap) -> Principal {
        Principal::Anonymous
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
    /// Where a cause meant for the operator goes (a page never shows it).
    pub log: Arc<dyn Fn(&str) + Send + Sync>,
    /// The clock, in Unix seconds.
    pub clock: fn() -> i64,
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Services")
    }
}
