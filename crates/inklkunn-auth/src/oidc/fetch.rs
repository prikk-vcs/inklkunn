//! The one way this crate reaches the network: the [`HttpsFetcher`] seam. Discovery, the provider's keys and
//! the token exchange all go through it, and an implementation sends every request through the egress guard
//! (threat model C-8: https only, no private, loopback or link-local destination, the address checked is the
//! address connected to). **This crate holds no HTTP client and spawns no process**; what implements the seam
//! in a deployment is the binary's wiring, and a test implements it with a map.

use std::fmt;

/// The largest response any implementation may hand back: a discovery document, a key set or a token
/// response is a few kilobytes.
pub const MAX_RESPONSE_BYTES: usize = 1 << 20;

/// One outbound request: a `GET`, or a form `POST` when `form` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    /// An `https://` URL.
    pub url: String,
    /// An `application/x-www-form-urlencoded` body; the request is a `POST` when present.
    pub form: Option<String>,
    /// The `Authorization` header's value, when the request needs one.
    pub authorization: Option<String>,
}

impl FetchRequest {
    /// A `GET`.
    pub fn get(url: impl Into<String>) -> Self {
        FetchRequest {
            url: url.into(),
            form: None,
            authorization: None,
        }
    }
}

/// Why a fetch gave nothing. The text is for operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The egress guard refused the destination.
    Refused(String),
    /// The connection or the exchange failed, or the answer was not a success.
    Transport(String),
    /// The answer was longer than [`MAX_RESPONSE_BYTES`].
    TooLarge,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Refused(e) => write!(f, "the egress guard refused: {e}"),
            FetchError::Transport(e) => write!(f, "the fetch failed: {e}"),
            FetchError::TooLarge => f.write_str("the response was too large"),
        }
    }
}

impl std::error::Error for FetchError {}

/// Fetch a URL over HTTPS through the egress guard.
pub trait HttpsFetcher: Send + Sync {
    /// Make the request and return the body of a successful answer.
    fn fetch(&self, request: &FetchRequest) -> Result<Vec<u8>, FetchError>;
}
