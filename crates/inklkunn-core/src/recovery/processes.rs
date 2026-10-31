//! The foreign-process witness (RFC 004 D-7; handoff 004 T5): is a `prikk` or `bwrap` of somebody else working in this
//! wiki's repository? Recovery clears a lock only when nothing of the kind is.
//!
//! A process is looked at through `/proc`: its command line, its parent and its working directory. It **names** the
//! repository when a word of its command line is the repository's directory (or something under it) or when it is
//! working in it; it is a **tool** when a word of its command line is `prikk` or `bwrap` (a confined prikk is
//! `bwrap … prikk …`, and an unconfined one run by a script is `sh …/prikk …`). A process of this process's own (a
//! descendant of it: the reads the server is making) is not foreign. A process whose command line could not be read
//! is not skipped: it is counted as *unreadable* and is treated as foreign when it could be a tool, because not being
//! able to see is not seeing nothing.

use std::path::Path;

/// One process of the host, as far as it could be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    /// The process id.
    pub pid: u32,
    /// Its parent's process id (0 when unknown).
    pub parent: u32,
    /// Its command line, word by word (empty when it could not be read).
    pub argv: Vec<String>,
    /// Where it is working (`/proc/<pid>/cwd`), when that could be read.
    pub cwd: Option<String>,
    /// Everything that was asked of `/proc` could be read. `false` for a process of another user that would not say.
    pub readable: bool,
}

/// The host's processes, behind a trait so that a test can put a process on the table.
pub trait ProcessTable: Send + Sync {
    /// Every process that could be listed.
    fn processes(&self) -> Vec<ProcessInfo>;
}

/// `/proc`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcFs;

impl ProcessTable for ProcFs {
    fn processes(&self) -> Vec<ProcessInfo> {
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in dir.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let at = entry.path();
            let argv: Vec<String> = match std::fs::read(at.join("cmdline")) {
                Ok(raw) => raw
                    .split(|b| *b == 0)
                    .filter(|a| !a.is_empty())
                    .map(|a| String::from_utf8_lossy(a).into_owned())
                    .collect(),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    out.push(ProcessInfo {
                        pid,
                        parent: 0,
                        argv: Vec::new(),
                        cwd: None,
                        readable: false,
                    });
                    continue;
                }
                Err(_) => continue, // gone while it was looked at
            };
            if argv.is_empty() {
                continue; // a kernel thread, or one that is exiting
            }
            let cwd = std::fs::read_link(at.join("cwd"))
                .ok()
                .map(|p| p.display().to_string());
            out.push(ProcessInfo {
                pid,
                parent: parent_of(&at).unwrap_or(0),
                argv,
                readable: cwd.is_some(),
                cwd,
            });
        }
        out
    }
}

/// The parent's id from `/proc/<pid>/stat` (`pid (comm) state ppid …`; the command name may hold spaces and
/// brackets, so the fields are read after the last `)`).
fn parent_of(at: &Path) -> Option<u32> {
    let stat = std::fs::read_to_string(at.join("stat")).ok()?;
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(1)?.parse().ok()
}

/// A process that is somebody else's and may be working in the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreign {
    /// The process id.
    pub pid: u32,
    /// The tool it runs (`prikk`, `bwrap`) or, for a process that could not be read, `unreadable`.
    pub tool: String,
}

fn base(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn is_tool(word: &str) -> Option<&'static str> {
    match base(word) {
        "prikk" => Some("prikk"),
        "bwrap" => Some("bwrap"),
        _ => None,
    }
}

fn under(path: &str, repo: &str) -> bool {
    path == repo
        || path
            .strip_prefix(repo)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The processes of `table` that are not `ours` (this process and its descendants) and are a tool working in
/// `repo`, or could be.
pub fn foreign(table: &[ProcessInfo], repo: &Path, ours: u32) -> Vec<Foreign> {
    let repo = repo.display().to_string();
    // The descendants of this process: walk parents until the chain ends or meets it.
    let parent: std::collections::HashMap<u32, u32> =
        table.iter().map(|p| (p.pid, p.parent)).collect();
    let is_ours = |pid: u32| {
        let mut at = pid;
        for _ in 0..4096 {
            if at == ours {
                return true;
            }
            match parent.get(&at) {
                Some(&up) if up != 0 && up != at => at = up,
                _ => return false,
            }
        }
        false
    };
    let mut out = Vec::new();
    for p in table {
        if is_ours(p.pid) {
            continue;
        }
        if !p.readable && p.argv.is_empty() {
            out.push(Foreign {
                pid: p.pid,
                tool: "unreadable".to_owned(),
            });
            continue;
        }
        let Some(tool) = p.argv.iter().find_map(|w| is_tool(w)) else {
            continue;
        };
        let names_repo = p.argv.iter().any(|w| under(w, &repo))
            || p.cwd.as_deref().is_some_and(|c| under(c, &repo));
        // A tool whose working directory could not be read might be working here.
        if names_repo || p.cwd.is_none() {
            out.push(Foreign {
                pid: p.pid,
                tool: tool.to_owned(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
