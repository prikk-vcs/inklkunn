//! The extracts of the read path in a wiki's scratch directory, and who may remove them (handoff 005 corrections, fifth
//! round, K51).
//!
//! More than one process extracts into one wiki's scratch directory: the server, and the command line (`wiki
//! import-folder` builds a read service of its own, a dry run takes no lease). The build lock of a [`ReadService`]
//! orders the extracts of one service only. So an extract is named for the process that made it
//! (`index-<pid>-<start time>-<n>`, the pid and start time as the lease records name a holder) and the sweep removes
//! the directories of **processes that are gone** and nothing else: a live process's extract, in this process or in
//! another, is not touched.
//!
//! [`ReadService`]: super::ReadService

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::lease::{start_time, start_time_of};

/// The prefix of the read path's extracts.
const PREFIX: &str = "index-";

/// One count for the whole process, so that two read services in it (a test's, a command line's) never choose the same
/// name.
static EXTRACTS: AtomicU64 = AtomicU64::new(0);

/// A name for a new extract: `index-<pid>-<start time>-<n>`.
pub(super) fn extract_name() -> String {
    format!(
        "{PREFIX}{}-{}-{}",
        std::process::id(),
        start_time(),
        EXTRACTS.fetch_add(1, Ordering::Relaxed)
    )
}

/// The process an extract's name says it is the extract of: its pid and, when the name records it, its start time. The
/// shape of an earlier build (`index-<pid>-<n>`) has a pid and no start time; any other name is no extract's.
fn made_by(name: &str) -> Option<(u32, Option<u64>)> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = name.strip_prefix(PREFIX)?.split('-').collect();
    match parts.as_slice() {
        [pid, start, n] if digits(pid) && digits(start) && digits(n) => {
            Some((pid.parse().ok()?, Some(start.parse().ok()?)))
        }
        [pid, n] if digits(pid) && digits(n) => Some((pid.parse().ok()?, None)),
        _ => None,
    }
}

/// Is the process that made an extract gone? Never on a guess: not where `/proc` cannot say, not for a process that is
/// there with the start time the name records (this one, among them), and for a pid that another process has been given
/// since, only when the name records a start time to tell them by.
fn is_gone(pid: u32, start: Option<u64>) -> bool {
    if !Path::new("/proc/self").exists() {
        return false;
    }
    if !Path::new(&format!("/proc/{pid}")).exists() {
        return true;
    }
    match (start, start_time_of(&pid.to_string())) {
        // A start time of `0` is "`/proc` could not say" when the name was made.
        (Some(recorded), Some(now)) => recorded != 0 && recorded != now,
        _ => false,
    }
}

/// Remove the extracts that processes which are gone left in `scratch`. Only directories of exactly the shape of
/// [`extract_name`] (or of the earlier build's), nothing is followed, and a live process's extract stays.
pub(super) fn sweep_leftovers(scratch: &Path) {
    let Ok(listing) = std::fs::read_dir(scratch) else {
        return;
    };
    for item in listing.flatten() {
        let left = item
            .file_name()
            .to_str()
            .and_then(made_by)
            .is_some_and(|(pid, start)| is_gone(pid, start));
        if left && item.file_type().is_ok_and(|t| t.is_dir()) {
            remove_tree(&item.path());
        }
    }
}

pub(super) fn remove_tree(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}
