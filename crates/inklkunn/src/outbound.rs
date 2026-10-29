//! Outbound HTTPS for OpenID Connect, wired (internal design IQ-3, AZ-1; handoff 002 corrections C8). The
//! three parts live in three layers and meet only here, in the binary:
//!
//! - the **egress guard** (`inklkunn-core`) decides whether a URL may be fetched and pins the addresses;
//! - the **fetcher** (`inklkunn-fetch`, a lower layer) runs a confined `curl` at exactly those addresses;
//! - the **seam** (`inklkunn-auth`'s [`HttpsFetcher`]) is what discovery, the key set and the token exchange
//!   call.
//!
//! The fetch crate cannot implement the seam itself: it is a lower layer and may not depend on a core crate
//! (the layering gate), so this small adapter does. Nothing constructs it yet: OpenID Connect is not switched
//! on in a deployment until M2, so the module is compiled and tested but unused.

use std::sync::Arc;

use inklkunn_auth::oidc::{FetchError, FetchRequest, HttpsFetcher};
use inklkunn_core::EgressGuard;
use inklkunn_fetch::{CurlError, CurlFetcher, CurlRequest, FetchTarget};

/// `HttpsFetcher` over the egress guard and the confined `curl`.
pub(crate) struct GuardedCurlFetcher {
    guard: Arc<dyn EgressGuard>,
    transport: CurlFetcher,
}

impl GuardedCurlFetcher {
    /// Over `guard` (which decides and pins) and the confined fetcher (which connects only to what was pinned).
    pub(crate) fn new(guard: Arc<dyn EgressGuard>, transport: CurlFetcher) -> Self {
        GuardedCurlFetcher { guard, transport }
    }
}

impl HttpsFetcher for GuardedCurlFetcher {
    fn fetch(&self, request: &FetchRequest) -> Result<Vec<u8>, FetchError> {
        let pinned = self
            .guard
            .check(&request.url)
            .map_err(|e| FetchError::Refused(e.to_string()))?;
        let target = FetchTarget {
            url: pinned.url,
            host: pinned.host,
            port: pinned.port,
            addrs: pinned.addrs.iter().map(|a| a.ip()).collect(),
        };
        self.transport
            .fetch(&CurlRequest {
                target,
                form: request.form.clone(),
                authorization: request.authorization.clone(),
            })
            .map_err(|e| match e {
                CurlError::TooLarge => FetchError::TooLarge,
                other => FetchError::Transport(other.to_string()),
            })
    }
}

#[cfg(test)]
mod tests;
