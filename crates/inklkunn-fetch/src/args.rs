//! The `curl` argument vector and the configuration it reads from standard input. **The vector carries no
//! secret**: the `Authorization` header and the form body go in the configuration, because an argument
//! vector is readable by every process of the host (`/proc/<pid>/cmdline`) and appears in error texts.

use std::fmt;
use std::time::Duration;

use crate::error::CurlError;
use crate::fetcher::FetchLimits;
use crate::request::{CurlRequest, MAX_SECRET_FIELD};

/// What `--write-out` appends after the body: a newline and the three-digit status. The body cannot forge
/// it: it is read from the end of what `curl` wrote.
pub(crate) const TRAILER_LEN: usize = 4;
/// The `--write-out` format that produces the trailer.
const WRITE_OUT: &str = "\\n%{http_code}";

/// What to run `curl` with: the arguments (no secret in any) and the configuration for its standard input
/// (the secrets; never shown by `Debug`).
pub(crate) struct Invocation {
    pub(crate) args: Vec<String>,
    pub(crate) stdin: Vec<u8>,
}

impl fmt::Debug for Invocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invocation")
            .field("args", &self.args)
            .field("stdin", &format_args!("{} bytes", self.stdin.len()))
            .finish()
    }
}

/// Whole seconds, at least one (curl takes decimals, but a bound of zero means "none").
fn seconds(d: Duration) -> u64 {
    d.as_secs()
        .saturating_add(u64::from(d.subsec_nanos() > 0))
        .max(1)
}

/// A value for a double-quoted parameter of curl's configuration syntax: `\` and `"` escaped; any control
/// byte (a newline would end the line and let a value add parameters of its own) is refused. The refusal
/// names the field, never its value.
fn quoted(field: &str, value: &str) -> Result<String, CurlError> {
    if value.len() > MAX_SECRET_FIELD {
        return Err(CurlError::InvalidRequest(format!(
            "the {field} is too long"
        )));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(CurlError::InvalidRequest(format!(
            "the {field} contains a control character"
        )));
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Ok(out)
}

/// The vector's `curl` invocation for `request`. `ca_path` is where `curl` finds the CA bundle (a path
/// inside the vector).
pub(crate) fn invocation(
    request: &CurlRequest,
    limits: &FetchLimits,
    ca_path: &str,
) -> Result<Invocation, CurlError> {
    let checked = request.target.check()?;
    let mut args: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(|s| (*s).to_owned()));
    // `-q` first: no `.curlrc`. No `-n`: no netrc.
    push(&["-q", "--silent", "--show-error", "--globoff"]);
    // https only, for the first request and for any redirect; and no redirect is followed at all.
    push(&[
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        "0",
    ]);
    // No proxy, whatever the environment says (it is cleared anyway), and a modern TLS.
    push(&["--noproxy", "*", "--tlsv1.2"]);
    push(&["--cacert", ca_path]);
    let (connect, total, max_bytes) = (
        seconds(limits.connect_timeout).to_string(),
        seconds(limits.max_time).to_string(),
        limits.max_bytes.to_string(),
    );
    push(&["--connect-timeout", &connect]);
    push(&["--max-time", &total]);
    push(&["--max-filesize", &max_bytes]);
    if let Some(entry) = &checked.resolve {
        // The pinned addresses; curl does not resolve this name.
        push(&["--resolve", entry]);
    }
    push(&["--user-agent", "inklkunn-fetch"]);
    push(&["--header", "Accept: application/json"]);
    push(&["--write-out", WRITE_OUT]);
    // The secrets come on standard input.
    push(&["--config", "-"]);
    push(&["--url", &checked.url]);

    let mut config = String::new();
    if let Some(auth) = &request.authorization {
        let value = quoted("authorization header", &format!("Authorization: {auth}"))?;
        config.push_str(&format!("header = {value}\n"));
    }
    if let Some(form) = &request.form {
        let value = quoted("form body", form)?;
        config.push_str(&format!("data-raw = {value}\n"));
    }
    Ok(Invocation {
        args,
        stdin: config.into_bytes(),
    })
}

#[cfg(test)]
mod tests;
