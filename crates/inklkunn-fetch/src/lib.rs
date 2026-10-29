#![forbid(unsafe_code)]
//! `inklkunn-fetch` — outbound HTTPS as a confined `curl` (internal design IQ-3, LAY-3; handoff 002
//! corrections C8). A lower-layer crate: it depends on nothing above the driver, resolves no name and
//! judges no destination. **The egress guard (in `inklkunn-core`) decides and pins the address**; the caller
//! hands this crate the guard's answer as a [`FetchTarget`] and the crate connects to exactly those
//! addresses (`curl --resolve`) and to nothing else.
//!
//! The vector (threat model C-8; the exact argument list is in `vector.rs` and `args.rs`):
//!
//! - `curl` runs under its own bubblewrap: **network shared, everything else unshared**, `/usr` and the
//!   system CA bundle read-only, a fresh `tmpfs` for `/tmp`, a cleared environment, `--die-with-parent`. No
//!   repository, no key directory, no seed is mounted, so a compromised `curl` finds none of them.
//! - `curl` is told `--proto =https` (and for redirects), `--max-redirs 0` (an answer that redirects is an
//!   error, never followed), `--resolve host:port:<pinned addresses>`, `--noproxy '*'`, `-q` (no
//!   configuration file), `--globoff`, a minimum of TLS 1.2, `--max-time`, `--connect-timeout`, and
//!   `--max-filesize`. The server also kills the child at a deadline and **reads at most the byte cap** from
//!   its standard output, whatever `curl` believes about the length.
//! - **Secrets never travel in an argument vector** (visible to every process of the host in `/proc`): the
//!   `Authorization` header and the form body go to `curl` on its standard input as a configuration
//!   (`--config -`). Nor do they appear in an error, a `Debug` rendering or a log: errors are typed, carry
//!   text for operators built from `curl`'s exit code, and add a *scrubbed* excerpt of its standard error
//!   (secrets removed, control characters dropped, length bounded).
//! - No TLS crate is in the dependency tree: TLS is `curl`'s, confined.
//!
//! The crate spawns two programs and no others: `bwrap` (whose only job is to run `curl`) and, in tests,
//! `curl` itself unconfined. The boundary gate checks both facts (`tools/boundary-check`).
//!
//! The seam is [`CurlFetcher::fetch`]. `inklkunn-auth`'s `HttpsFetcher` is implemented one layer up, by the
//! binary's wiring, because this crate is a lower layer and may not name a core crate (the layering gate).

mod args;
mod error;
mod fetcher;
mod request;
mod scrub;
mod vector;

pub use error::CurlError;
pub use fetcher::{CurlFetcher, FetchLimits};
pub use request::{CurlRequest, FetchTarget};

/// The program the vector runs.
pub const CURL: &str = "curl";

/// The default cap on a response body, in bytes: a discovery document, a key set or a token response is a
/// few kilobytes. (`inklkunn-auth` calls the same number `MAX_RESPONSE_BYTES`; the binary's wiring test
/// asserts they are equal.)
pub const DEFAULT_MAX_BYTES: usize = 1 << 20;
