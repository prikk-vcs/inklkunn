//! What prikk's `error: unsupported format version: N` means (maintenance M11, M17).
//!
//! The sentence is prikk's whole answer when it cannot open a repository by the number in `.prikk/FORMAT`. Two things
//! can be behind it, and only one of them is a case for the operator's mismatch:
//!
//! * **A mismatch**: the running prikk is *known* not to read format N, because it is older than the first prikk that
//!   does. The repository was made by a newer prikk and nothing else is implied.
//! * **A damaged or unknown marker**: anything else. A prikk that reads N and still says so is looking at a marker that
//!   is not what the repository was made with (`8` written without its newline was measured on 0.57.0, which reads
//!   format 8); a number no table here knows (`9`) is a marker this build cannot judge. Nothing in the repository can be
//!   called sound then.
//!
//! The driver and the signer ask the same classifier with the same conditions, so a surface cannot be told one thing by
//! one and another by the other.

use crate::storage::unsupported_format_version;

/// A prikk version, `(major, minor, patch)`.
pub type Version = (u64, u64, u64);

/// The first prikk that reads each format whose first reader is known: **the one table**. Format 8 (a repository made
/// under prikk 0.57.0) is read from 0.57.0 on; prikk 0.55.0 and 0.56.0 refuse it. A format that is not listed is one
/// this build knows nothing about, and a refusal of it is never taken for a mismatch.
const FIRST_READER: &[(u64, Version)] = &[(8, (0, 57, 0))];

/// The first prikk that reads `format`, when the table knows it.
pub fn first_reader(format: u64) -> Option<Version> {
    FIRST_READER
        .iter()
        .find(|(f, _)| *f == format)
        .map(|(_, v)| *v)
}

/// What an `unsupported format version: N` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    /// The running prikk is known not to read format `format`: an operator's mismatch, not damage.
    Mismatch {
        /// The format prikk named.
        format: u64,
    },
    /// The format marker is damaged or unknown: the running prikk should read `format` or cannot be shown to refuse
    /// it. The repository is flagged and nothing is called intact.
    Marker {
        /// The format prikk named.
        format: u64,
    },
}

/// Whether a failed run of prikk is its `unsupported format version: N`, and what that is.
///
/// The conditions are the same for every caller: the exit code is 1, stdout holds no `refusal-v1` report (a refusal
/// about the thing asked is not this), and `stderr` is, whole, prikk's sentence with a decimal number of at most nine
/// digits and no leading zero ([`unsupported_format_version`]). `prikk` is the running prikk's version when it is
/// known; **an unknown version is never read as one that refuses**.
pub fn classify_format_error(
    code: Option<i32>,
    stdout: &[u8],
    stderr: &str,
    prikk: Option<Version>,
) -> Option<FormatError> {
    if code != Some(1) || is_refusal_report(stdout) {
        return None;
    }
    let format = unsupported_format_version(stderr)?;
    let refuses = match (prikk, first_reader(format)) {
        (Some(running), Some(first)) => running < first,
        _ => false,
    };
    Some(if refuses {
        FormatError::Mismatch { format }
    } else {
        FormatError::Marker { format }
    })
}

/// Is `stdout` prikk's `refusal-v1` report?
pub(crate) fn is_refusal_report(stdout: &[u8]) -> bool {
    #[derive(serde::Deserialize)]
    struct Head {
        schema_version: String,
    }
    serde_json::from_slice::<Head>(stdout).is_ok_and(|h| h.schema_version == "refusal-v1")
}

/// What an operator is told of a repository a prikk that is known not to read it was asked to open. It names the
/// version prikk gave and the first prikk that reads it.
pub fn unsupported_format_sentence(format: u64) -> String {
    let first = match first_reader(format) {
        Some((major, minor, patch)) => {
            format!(" (prikk {major}.{minor}.{patch} is the first that does)")
        }
        None => String::new(),
    };
    format!(
        "it is format {format}, which this prikk does not read{first}; run the prikk that made it, or restore the wiki from a backup made before"
    )
}

/// What an operator is told of a format marker that is damaged or unknown. It names the version prikk gave, and claims
/// nothing about the rest of the repository.
pub fn format_marker_sentence(format: u64) -> String {
    format!(
        "prikk says the repository is format {format} and will not open it, but this prikk is not known to refuse that format: the format marker (`.prikk/FORMAT`) may be damaged, or may be one this build does not know; compare it with a backup, and do not take the rest of the repository to be intact"
    )
}

#[cfg(test)]
mod tests;
