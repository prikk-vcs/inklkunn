//! The command line: which command, with which settings. `main` only hands it the process's arguments,
//! environment, clock and streams, so everything here is exercised in-process.

use std::ffi::OsString;
use std::io::Write;

use inklkunn_prikk::MIN_PRIKK_VERSION;

use crate::config::Settings;
use crate::keys;

/// The commit the release workflow built from (`INKLKUNN_COMMIT` at build time), or `unknown`.
const COMMIT: &str = match option_env!("INKLKUNN_COMMIT") {
    Some(c) => c,
    None => "unknown",
};

/// What `--version` prints.
pub(crate) fn version_text() -> String {
    format!(
        "inklkunn {} (commit {COMMIT}; requires prikk >= {MIN_PRIKK_VERSION}, bubblewrap, prlimit)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Run the command line and return the exit status: 0, 1 for a refusal or failure, 2 for a command line
/// that was not understood.
pub(crate) fn run(
    args: &[String],
    get_env: &dyn Fn(&str) -> Option<OsString>,
    now: i64,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let (dev_unconfined, args) = match args.split_first() {
        Some((first, rest)) if first == "--dev-unconfined" => (true, rest),
        _ => (false, args),
    };
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["--version" | "-V"] | [] if !dev_unconfined => {
            let _ = writeln!(out, "{}", version_text());
            if words.is_empty() {
                let _ = writeln!(
                    out,
                    "no server yet; the `serve` command arrives with RFC 003's handoff"
                );
            }
            0
        }
        ["wiki", "keys", ..] => {
            let rest: Vec<String> = args.iter().skip(2).cloned().collect();
            let command = match keys::parse(&rest) {
                Ok(c) => c,
                Err(keys::Usage(why)) => {
                    let _ = writeln!(err, "inklkunn: {why}");
                    return 2;
                }
            };
            let settings = match Settings::load(get_env, dev_unconfined) {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(err, "inklkunn: {e}");
                    return 1;
                }
            };
            keys::execute(&command, &settings, now, out, err)
        }
        other => {
            let _ = writeln!(
                err,
                "inklkunn: unknown command `{}`; there are `--version` and `wiki keys`\n{}",
                other.join(" "),
                keys::USAGE
            );
            2
        }
    }
}

#[cfg(test)]
mod tests;
