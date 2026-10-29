//! The typed failures of a fetch. Every variant's text is for operators and is built from fixed words, curl's
//! exit code and a *scrubbed* excerpt of its standard error; none can carry the `Authorization` value or the
//! form body.

use std::fmt;

/// Why a fetch gave nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurlError {
    /// The request was refused before anything ran: a URL that is not `https`, that carries userinfo or
    /// a control byte, that disagrees with the pinned target, a target with no address, or a header or form
    /// that cannot be passed safely. The text never repeats a secret.
    InvalidRequest(String),
    /// The `curl` program was not found.
    CurlMissing(String),
    /// No system CA bundle was found to mount.
    NoCaBundle,
    /// `bwrap` is missing or cannot confine a process here (user namespaces refused, a mount failed).
    SandboxUnavailable(String),
    /// The vector could not be started for another reason.
    Spawn(String),
    /// The time bound passed (curl's own `--max-time`, exit 28, or the server's deadline).
    TimedOut,
    /// The answer was longer than the byte cap (curl's `--max-filesize`, exit 63, or the server's read cap).
    TooLarge,
    /// The connection could not be made (curl exit 7).
    Connect(String),
    /// The TLS handshake or certificate check failed (curl's TLS exit codes).
    Tls {
        /// curl's exit code.
        code: i32,
        /// Scrubbed detail.
        detail: String,
    },
    /// The server answered with a redirect, which is never followed.
    Redirect(u16),
    /// The server answered with a status other than 200.
    Status(u16),
    /// curl failed in another way.
    Curl {
        /// curl's exit code.
        code: i32,
        /// Scrubbed detail.
        detail: String,
    },
    /// The vector was killed (a signal, or a memory ceiling).
    Killed(String),
    /// curl exited successfully but its output did not have the shape asked for.
    Malformed(String),
}

impl fmt::Display for CurlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurlError::InvalidRequest(e) => write!(f, "the request was refused before it ran: {e}"),
            CurlError::CurlMissing(p) => {
                write!(f, "`{p}` was not found; outbound HTTPS needs curl")
            }
            CurlError::NoCaBundle => f.write_str(
                "no system CA bundle was found (looked for /etc/ssl/certs/ca-certificates.crt, \
                 /etc/pki/tls/certs/ca-bundle.crt, /etc/ssl/cert.pem)",
            ),
            CurlError::SandboxUnavailable(e) => {
                write!(f, "the sandbox for curl is unavailable: {e}")
            }
            CurlError::Spawn(e) => write!(f, "curl could not be started: {e}"),
            CurlError::TimedOut => f.write_str("the exchange did not finish in time"),
            CurlError::TooLarge => f.write_str("the response was larger than the byte cap"),
            CurlError::Connect(d) => write!(f, "the connection could not be made: {d}"),
            CurlError::Tls { code, detail } => {
                write!(f, "the TLS exchange failed (curl exit {code}): {detail}")
            }
            CurlError::Redirect(c) => {
                write!(
                    f,
                    "the server answered {c}, a redirect; redirects are not followed"
                )
            }
            CurlError::Status(c) => write!(f, "the server answered {c}, not 200"),
            CurlError::Curl { code, detail } => write!(f, "curl failed (exit {code}): {detail}"),
            CurlError::Killed(e) => write!(f, "curl was killed: {e}"),
            CurlError::Malformed(e) => {
                write!(f, "curl's output was not in the expected shape: {e}")
            }
        }
    }
}

impl std::error::Error for CurlError {}
