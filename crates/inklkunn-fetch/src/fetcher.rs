//! [`CurlFetcher`]: build the vector, run it with the driver's bounded runner, and read the answer.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use inklkunn_prikk::sandbox::{BWRAP, probe_bwrap, resolve_binary, run_command};

use crate::args::{TRAILER_LEN, invocation};
use crate::error::CurlError;
use crate::request::CurlRequest;
use crate::scrub::scrub;
use crate::vector::{VECTOR_CA_PATH, bwrap_command, find_ca_bundle};
use crate::{CURL, DEFAULT_MAX_BYTES};

/// The bounds of one exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchLimits {
    /// curl's `--max-time`: the whole exchange.
    pub max_time: Duration,
    /// curl's `--connect-timeout`.
    pub connect_timeout: Duration,
    /// The most response-body bytes accepted: curl's `--max-filesize`, and the server's own read cap.
    pub max_bytes: usize,
    /// How long past `max_time` the server waits before killing the child (curl should have stopped it).
    pub kill_slack: Duration,
}

impl Default for FetchLimits {
    fn default() -> Self {
        FetchLimits {
            max_time: Duration::from_secs(15),
            connect_timeout: Duration::from_secs(5),
            max_bytes: DEFAULT_MAX_BYTES,
            kill_slack: Duration::from_secs(3),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confinement {
    Bwrap,
    #[cfg(any(test, feature = "test-knobs"))]
    Unconfined,
}

/// Fetches over HTTPS by running `curl` in its own bubblewrap vector, at the addresses the egress guard
/// pinned. Holds no secret; cheap to clone and share.
#[derive(Debug, Clone)]
pub struct CurlFetcher {
    bwrap: OsString,
    curl: OsString,
    ca_bundle: Option<PathBuf>,
    limits: FetchLimits,
    confinement: Confinement,
}

impl Default for CurlFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl CurlFetcher {
    /// `bwrap` and `curl` from `PATH`, the system CA bundle, the default limits.
    pub fn new() -> Self {
        CurlFetcher {
            bwrap: OsString::from(BWRAP),
            curl: OsString::from(CURL),
            ca_bundle: None,
            limits: FetchLimits::default(),
            confinement: Confinement::Bwrap,
        }
    }

    /// Other bounds.
    #[must_use]
    pub fn with_limits(mut self, limits: FetchLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Other programs (an absolute path, or a name found on `PATH`).
    #[must_use]
    pub fn with_programs(mut self, bwrap: impl Into<OsString>, curl: impl Into<OsString>) -> Self {
        self.bwrap = bwrap.into();
        self.curl = curl.into();
        self
    }

    /// A CA bundle other than the system's.
    #[must_use]
    pub fn with_ca_bundle(mut self, path: impl Into<PathBuf>) -> Self {
        self.ca_bundle = Some(path.into());
        self
    }

    /// **Tests only:** run `curl` directly (cleared environment), with no sandbox, so that a stand-in script
    /// can exercise the failure shapes where bubblewrap does not work.
    #[cfg(any(test, feature = "test-knobs"))]
    #[must_use]
    pub fn unconfined(mut self) -> Self {
        self.confinement = Confinement::Unconfined;
        self
    }

    /// The bounds in force.
    pub fn limits(&self) -> FetchLimits {
        self.limits
    }

    /// Everything the vector needs is there: `curl` found, a CA bundle found, and `bwrap` able to confine a
    /// process here. For the start-up probe; a failure says what to install or permit.
    pub fn probe(&self) -> Result<(), CurlError> {
        self.curl_path()?;
        self.ca_path()?;
        match self.confinement {
            Confinement::Bwrap => probe_bwrap(&self.bwrap)
                .map_err(|e| CurlError::SandboxUnavailable(scrub(e.as_bytes(), &[]))),
            #[cfg(any(test, feature = "test-knobs"))]
            Confinement::Unconfined => Ok(()),
        }
    }

    fn curl_path(&self) -> Result<PathBuf, CurlError> {
        let path = resolve_binary(&self.curl);
        if path.is_file() {
            Ok(path)
        } else {
            Err(CurlError::CurlMissing(
                self.curl.to_string_lossy().into_owned(),
            ))
        }
    }

    fn ca_path(&self) -> Result<PathBuf, CurlError> {
        match &self.ca_bundle {
            Some(p) => p
                .canonicalize()
                .ok()
                .filter(|p| p.is_file())
                .ok_or(CurlError::NoCaBundle),
            None => find_ca_bundle().ok_or(CurlError::NoCaBundle),
        }
    }

    /// Make the request and return the body of a `200`. **Blocks** the caller for up to `max_time` plus
    /// `kill_slack`. The target's addresses are the only ones connected to; the name is never resolved.
    pub fn fetch(&self, request: &CurlRequest) -> Result<Vec<u8>, CurlError> {
        let curl = self.curl_path()?;
        let secrets: Vec<&str> = [request.authorization.as_deref(), request.form.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        let (command, input) = match self.confinement {
            Confinement::Bwrap => {
                let ca = self.ca_path()?;
                let inv = invocation(request, &self.limits, VECTOR_CA_PATH)?;
                (bwrap_command(&self.bwrap, &curl, &ca, &inv.args), inv.stdin)
            }
            #[cfg(any(test, feature = "test-knobs"))]
            Confinement::Unconfined => {
                let ca = self
                    .ca_path()
                    .unwrap_or_else(|_| PathBuf::from(VECTOR_CA_PATH));
                let inv = invocation(request, &self.limits, &ca.to_string_lossy())?;
                (
                    crate::vector::unconfined_command(&curl, &inv.args),
                    inv.stdin,
                )
            }
        };
        let program = command.get_program().to_string_lossy().into_owned();
        let cap =
            u64::try_from(self.limits.max_bytes.saturating_add(TRAILER_LEN)).unwrap_or(u64::MAX);
        let run = run_command(
            command,
            Some(&input),
            Some(self.limits.max_time + self.limits.kill_slack),
            cap,
        )
        .map_err(|e| spawn_error(&program, &e))?;
        if run.timed_out {
            return Err(CurlError::TimedOut);
        }
        if run.output_exceeded {
            return Err(CurlError::TooLarge);
        }
        let excerpt = || scrub(&run.stderr, &secrets);
        let Some(code) = run.status.code() else {
            use std::os::unix::process::ExitStatusExt;
            return Err(CurlError::Killed(match run.status.signal() {
                Some(n) => format!("by signal {n}"),
                None => "without an exit code".to_owned(),
            }));
        };
        if code != 0 {
            // bubblewrap's own refusals come in its own voice, whatever the inner exit code.
            if run.stderr.starts_with(b"bwrap:") {
                return Err(CurlError::SandboxUnavailable(excerpt()));
            }
            return Err(classify(code, excerpt()));
        }
        read_answer(run.stdout, self.limits.max_bytes)
    }
}

/// A spawn failure: the program (`bwrap`) missing, or not runnable.
fn spawn_error(name: &str, e: &std::io::Error) -> CurlError {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            CurlError::SandboxUnavailable(format!("`{name}` was not found"))
        }
        _ => CurlError::Spawn(format!("`{name}` could not be started: {e}")),
    }
}

/// curl's exit code as a typed error. `detail` is already scrubbed.
fn classify(code: i32, detail: String) -> CurlError {
    match code {
        // 128 + n: the vector's pid 1 reporting that curl died by signal n (a ceiling, an OOM kill).
        129..=192 => CurlError::Killed(format!("by signal {}", code - 128)),
        7 => CurlError::Connect(detail),
        28 => CurlError::TimedOut,
        63 => CurlError::TooLarge,
        // The TLS family: handshake, certificate, cipher, CA file, pinned key, ...
        35 | 51 | 53 | 54 | 58 | 59 | 60 | 64 | 66 | 77 | 80 | 82 | 83 | 90 | 91 | 98 => {
            CurlError::Tls { code, detail }
        }
        _ => CurlError::Curl { code, detail },
    }
}

/// Split `body ++ "\n" ++ status` and judge the status.
fn read_answer(mut stdout: Vec<u8>, max_bytes: usize) -> Result<Vec<u8>, CurlError> {
    let malformed = || CurlError::Malformed("no status trailer".to_owned());
    let split = stdout
        .len()
        .checked_sub(TRAILER_LEN)
        .ok_or_else(malformed)?;
    let trailer = stdout.get(split..).ok_or_else(malformed)?;
    let (newline, digits) = trailer.split_first().ok_or_else(malformed)?;
    if *newline != b'\n' || digits.len() != 3 || !digits.iter().all(u8::is_ascii_digit) {
        return Err(malformed());
    }
    let status = std::str::from_utf8(digits)
        .ok()
        .and_then(|d| d.parse::<u16>().ok())
        .ok_or_else(malformed)?;
    match status {
        200 => {
            stdout.truncate(split);
            if stdout.len() > max_bytes {
                return Err(CurlError::TooLarge);
            }
            Ok(stdout)
        }
        300..=399 => Err(CurlError::Redirect(status)),
        0 => Err(CurlError::Malformed("curl reported no status".to_owned())),
        s => Err(CurlError::Status(s)),
    }
}

#[cfg(test)]
mod tests;
