//! The command line: which command, with which settings. `main` only hands it the process's arguments,
//! environment, clock and streams, so everything here is exercised in-process.

use std::ffi::OsString;
use std::io::Write;

use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::{Admission, MIN_PRIKK_VERSION, PrikkCli};

use crate::config::{self, Settings, WebSettings};
use crate::{admin, audit, keys, serve};

/// The commit the release workflow built from (`INKLKUNN_COMMIT` at build time), or `unknown`.
const COMMIT: &str = match option_env!("INKLKUNN_COMMIT") {
    Some(c) => c,
    None => "unknown",
};

/// The prikk this host would use and what it says, for `--version`: the version found, or why none is.
fn found_prikk(get_env: &dyn Fn(&str) -> Option<OsString>) -> String {
    let prikk = get_env("INKLKUNN_PRIKK").unwrap_or_else(|| OsString::from("prikk"));
    match PrikkCli::open(
        &prikk,
        Sandbox::Unconfined,
        Admission::new(1 << 30, 64 << 20, std::time::Duration::from_millis(200)),
    ) {
        Ok(cli) => format!("found {}", cli.version()),
        Err(e) => format!("no usable prikk: {e}"),
    }
}

/// What `--version` prints: the build, the floor it needs, and the prikk found here.
pub(crate) fn version_text(get_env: &dyn Fn(&str) -> Option<OsString>) -> String {
    format!(
        "inklkunn {} (commit {COMMIT}; requires prikk >= {MIN_PRIKK_VERSION}, bubblewrap, prlimit; {})",
        env!("CARGO_PKG_VERSION"),
        found_prikk(get_env)
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
            let _ = writeln!(out, "{}", version_text(get_env));
            0
        }
        ["serve", rest @ ..] => serve_command(rest, get_env, dev_unconfined, err),
        ["audit", "export"] => audit::export(get_env, out, err),
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
        ["wiki", "create" | "import-folder" | "export", ..] => {
            let rest: Vec<String> = args.iter().skip(1).cloned().collect();
            let command = match admin::parse(&rest) {
                Ok(c) => c,
                Err(admin::Usage(why)) => {
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
            admin::execute(&command, &settings, now, out, err)
        }
        other => {
            let _ = writeln!(
                err,
                "inklkunn: unknown command `{}`; there are `--version`, `serve`, `audit export`, `wiki keys`, `wiki create`, `wiki import-folder` and `wiki export`\n{}\n{}\n{}",
                other.join(" "),
                SERVE_USAGE,
                keys::USAGE,
                admin::USAGE
            );
            2
        }
    }
}

const SERVE_USAGE: &str = "usage: inklkunn [--dev-unconfined] serve [--config FILE]";

/// `serve [--config FILE]`: the settings (the file, then the environment over it), the checks, the server.
fn serve_command(
    rest: &[&str],
    get_env: &dyn Fn(&str) -> Option<OsString>,
    dev_unconfined: bool,
    err: &mut dyn Write,
) -> i32 {
    let config_path = match rest {
        [] => None,
        ["--config", path] => Some(*path),
        _ => {
            let _ = writeln!(
                err,
                "inklkunn: serve takes only --config FILE\n{SERVE_USAGE}"
            );
            return 2;
        }
    };
    let fail = |err: &mut dyn Write, why: &dyn std::fmt::Display| {
        let _ = writeln!(err, "inklkunn: {why}");
        1
    };
    let file = match config_path {
        None => std::collections::HashMap::new(),
        Some(path) => match std::fs::read_to_string(path)
            .map_err(|e| {
                config::ConfigError(format!(
                    "the configuration file {path} could not be read ({})",
                    e.kind()
                ))
            })
            .and_then(|text| {
                config::read_file(&text).map_err(|e| config::ConfigError(format!("{path}: {e}")))
            }) {
            Ok(f) => f,
            Err(e) => return fail(err, &e),
        },
    };
    let get = config::layered(get_env, &file);
    let settings = match Settings::load(&get, dev_unconfined) {
        Ok(s) => s,
        Err(e) => return fail(err, &e),
    };
    let web = match WebSettings::load(&get) {
        Ok(w) => w,
        Err(e) => return fail(err, &e),
    };
    if dev_unconfined {
        let _ = writeln!(err, "{}", config::UNCONFINED_BANNER);
    }
    let prepared = match serve::prepare(&settings, &web) {
        Ok(p) => p,
        Err(e) => return fail(err, &e),
    };
    match serve::run(
        prepared,
        inklkunn_web::serve::Shutdown::on_signal(),
        err,
        |_| {},
    ) {
        Ok(()) => 0,
        Err(e) => fail(err, &e),
    }
}

#[cfg(test)]
mod tests;
