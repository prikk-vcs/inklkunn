//! `inklkunn audit export`: the audit trail as JSON Lines on standard output (RFC 002 D-9), oldest first, one
//! object per line with fixed keys, pages read from the store so a long trail is never held in memory. It
//! reads the store and nothing else: no key, no secret, and (unlike the key commands) no operator secret is
//! needed to read what the service did.

use std::ffi::OsString;
use std::io::Write;

use inklkunn_store::{SqliteStore, export_jsonl};

use crate::config::{ConfigError, absolute_setting};

/// Run `audit export`. Returns the exit status: 0, or 1 with the reason on `err`.
pub(crate) fn export(
    get_env: &dyn Fn(&str) -> Option<OsString>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    match run(get_env, out) {
        Ok(count) => {
            let _ = writeln!(err, "inklkunn: exported {count} audit events");
            0
        }
        Err(why) => {
            let _ = writeln!(err, "inklkunn: {why}");
            1
        }
    }
}

fn run(get_env: &dyn Fn(&str) -> Option<OsString>, out: &mut dyn Write) -> Result<u64, String> {
    let name = "INKLKUNN_DATA_DIR";
    let data_dir = get_env(name)
        .ok_or_else(|| ConfigError(format!("{name} is not set; it is required")).to_string())
        .and_then(|v| absolute_setting(name, &v).map_err(|e| e.to_string()))?;
    let path = data_dir.join("store.sqlite");
    if !path.is_file() {
        return Err(format!(
            "there is no store at {} ({name}); there is no audit trail to export",
            path.display()
        ));
    }
    let store =
        SqliteStore::open(&path).map_err(|e| format!("the store could not be opened: {e}"))?;
    export_jsonl(&store, out).map_err(|e| format!("the audit trail could not be exported: {e}"))
}

#[cfg(test)]
mod tests;
