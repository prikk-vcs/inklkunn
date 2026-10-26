//! Confinement of the prikk subprocess (RFC 001 D-4; threat model INV-3, C-4c). The shape is planeter's,
//! whose investigations settled it (planeter RFC 011 D-2, D-5; its IS-37 and I3), with one difference:
//! inklkunn's driver only reads, so **the repository is bound read-only**; the only writable mount is the
//! call's scratch directory, and only for `extract`.
//!
//! - **`bwrap` is the server's direct child.** `--die-with-parent` ties the whole namespace to the server;
//!   the wall-time bound is kept by the server ([`Sandbox::run`] sends `SIGKILL` at the deadline).
//! - **Spawn and wait happen on one thread.** `--die-with-parent` is `PR_SET_PDEATHSIG` on the parent
//!   *thread*: a child handed to another thread, or spawned from a thread that then exits, is killed while
//!   healthy (planeter's investigation I3). [`Sandbox::run`] never moves the child.
//! - **The memory ceiling is `prlimit --data=<N> --` in front of the vector**: set before `bwrap` is
//!   exec'd and inherited by everything under it. A call that hits it is reported as a ceiling hit by the
//!   driver, never as a verdict.
//! - **The environment is cleared.** No `HOME`, so no key directory is ever visible to a read (the signer
//!   crate mounts seeds for a write call on its own terms, RFC 002).

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The memory-ceiling wrapper (util-linux): a runtime prerequisite.
pub const PRLIMIT: &str = "prlimit";
/// The sandbox binary: a runtime prerequisite.
pub const BWRAP: &str = "bwrap";

/// Per-call limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallLimits {
    /// `RLIMIT_DATA` for the call, in bytes; `None` = no ceiling.
    pub data_ceiling: Option<u64>,
}

/// What a bounded run produced.
#[derive(Debug)]
pub struct Run {
    /// The child's exit status (a deadline kill shows the `SIGKILL` status; see `timed_out`).
    pub status: ExitStatus,
    /// Captured stdout (empty after a deadline kill).
    pub stdout: Vec<u8>,
    /// Captured stderr (empty after a deadline kill).
    pub stderr: Vec<u8>,
    /// The wall-time bound fired.
    pub timed_out: bool,
}

/// How the prikk subprocess is confined.
#[derive(Debug, Clone)]
pub enum Sandbox {
    /// Bubblewrap: the production default.
    Bwrap(BwrapConfig),
    /// **Tests and development only:** prikk runs directly. Never for hosting.
    Unconfined,
}

impl Default for Sandbox {
    fn default() -> Self {
        Sandbox::Bwrap(BwrapConfig::default())
    }
}

/// Tunables for bubblewrap.
#[derive(Debug, Clone)]
pub struct BwrapConfig {
    /// The `bwrap` binary.
    pub bwrap: OsString,
    /// The `prlimit` binary.
    pub prlimit: OsString,
}

impl Default for BwrapConfig {
    fn default() -> Self {
        BwrapConfig {
            bwrap: OsString::from(BWRAP),
            prlimit: OsString::from(PRLIMIT),
        }
    }
}

/// Why a mount set was refused before anything ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountError {
    /// A path is not absolute or contains `..`.
    NotAbsolute(PathBuf),
    /// The scratch directory and the repository overlap.
    Overlap(PathBuf, PathBuf),
}

impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountError::NotAbsolute(p) => {
                write!(
                    f,
                    "a sandbox mount must be an absolute path without `..` (got {})",
                    p.display()
                )
            }
            MountError::Overlap(a, b) => write!(
                f,
                "sandbox mounts must not overlap ({} and {})",
                a.display(),
                b.display()
            ),
        }
    }
}

impl std::error::Error for MountError {}

fn check_path(p: &Path) -> Result<(), MountError> {
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(MountError::NotAbsolute(p.to_path_buf()));
    }
    Ok(())
}

/// Check the repository and the optional scratch mount: absolute, no `..`, no overlap.
pub fn validate_mounts(repo: &Path, scratch: Option<&Path>) -> Result<(), MountError> {
    check_path(repo)?;
    if let Some(s) = scratch {
        check_path(s)?;
        if s.starts_with(repo) || repo.starts_with(s) {
            return Err(MountError::Overlap(s.to_path_buf(), repo.to_path_buf()));
        }
    }
    Ok(())
}

impl Sandbox {
    /// The [`Command`] that runs `prikk_binary <args>` in `repo` under this policy, with `scratch` (if any)
    /// bound read-write and `limits` applied in front of the whole vector.
    pub fn command(
        &self,
        prikk_binary: &OsStr,
        repo: &Path,
        scratch: Option<&Path>,
        args: &[&str],
        limits: CallLimits,
    ) -> Command {
        let (inner, prlimit) = match self {
            Sandbox::Unconfined => {
                let mut c = Command::new(prikk_binary);
                c.current_dir(repo).args(args);
                (c, OsString::from(PRLIMIT))
            }
            Sandbox::Bwrap(cfg) => (
                cfg.command(prikk_binary, repo, scratch, args),
                cfg.prlimit.clone(),
            ),
        };
        match limits.data_ceiling {
            None => inner,
            Some(bytes) => {
                let mut c = Command::new(prlimit);
                c.arg(format!("--data={bytes}"))
                    .arg("--")
                    .arg(inner.get_program())
                    .args(inner.get_args());
                if let Some(dir) = inner.get_current_dir() {
                    c.current_dir(dir);
                }
                c
            }
        }
    }

    /// Run one call to completion: spawn, collect stdout and stderr, and enforce `deadline` in the server
    /// (`SIGKILL`, `timed_out: true`). Spawn and wait both happen on the calling thread; see the module docs.
    pub fn run(
        &self,
        prikk_binary: &OsStr,
        repo: &Path,
        scratch: Option<&Path>,
        args: &[&str],
        limits: CallLimits,
        deadline: Option<Duration>,
    ) -> std::io::Result<Run> {
        if matches!(self, Sandbox::Bwrap(_)) {
            validate_mounts(repo, scratch)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        }
        run_bounded(
            self.command(prikk_binary, repo, scratch, args, limits),
            deadline,
        )
    }

    /// Is this the confined policy?
    pub fn is_confined(&self) -> bool {
        matches!(self, Sandbox::Bwrap(_))
    }
}

impl BwrapConfig {
    fn command(
        &self,
        prikk_binary: &OsStr,
        repo: &Path,
        scratch: Option<&Path>,
        args: &[&str],
    ) -> Command {
        let prikk = resolve_binary(prikk_binary);
        let mut c = Command::new(&self.bwrap);
        // A read-only view of the system, with the usr-merge symlinks a dynamically linked binary needs.
        c.arg("--ro-bind").arg("/usr").arg("/usr");
        for (target, link) in [
            ("usr/lib", "/lib"),
            ("usr/lib", "/lib64"),
            ("usr/bin", "/bin"),
            ("usr/bin", "/sbin"),
        ] {
            c.arg("--symlink").arg(target).arg(link);
        }
        // The prikk binary itself, wherever it lives.
        c.arg("--ro-bind").arg(&prikk).arg(&prikk);
        c.arg("--proc").arg("/proc");
        c.arg("--dev").arg("/dev");
        // The repository, read-only: every verb of this driver is a read (STO-5).
        c.arg("--ro-bind").arg(repo).arg(repo);
        // The scratch directory, read-write, only when the call has one (`extract`).
        if let Some(s) = scratch {
            c.arg("--bind").arg(s).arg(s);
        }
        c.arg("--chdir").arg(repo);
        c.arg("--unshare-all");
        c.arg("--die-with-parent");
        c.arg("--new-session");
        c.arg("--clearenv");
        c.arg("--setenv").arg("PATH").arg("/usr/bin");
        c.arg("--").arg(&prikk).args(args);
        c
    }
}

/// Spawn `cmd`, collect its output, kill it at `deadline`. Spawn and wait on this thread (module docs).
fn run_bounded(mut cmd: Command, deadline: Option<Duration>) -> std::io::Result<Run> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    let (Some(mut out), Some(mut err)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::other(
            "stdout and stderr were requested as pipes",
        ));
    };
    let (eof_tx, eof_rx) = mpsc::channel::<()>();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        let _ = eof_tx.send(());
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err.read_to_end(&mut buf);
        buf
    });
    // 1. Wait for stdout to end (or the deadline).
    let mut timed_out = match deadline {
        None => {
            let _ = eof_rx.recv();
            false
        }
        Some(d) => matches!(eof_rx.recv_timeout(d), Err(mpsc::RecvTimeoutError::Timeout)),
    };
    // 2. The exit normally follows at once; poll briefly if it lags, still bounded.
    if !timed_out {
        let mut pause = Duration::from_micros(50);
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break Some(st);
            }
            if let Some(d) = deadline
                && started.elapsed() >= d
            {
                timed_out = true;
                break None;
            }
            std::thread::sleep(pause);
            pause = (pause * 2).min(Duration::from_millis(5));
        };
        if let Some(status) = status {
            let stdout = out_reader.join().unwrap_or_default();
            let stderr = err_reader.join().unwrap_or_default();
            return Ok(Run {
                status,
                stdout,
                stderr,
                timed_out: false,
            });
        }
    }
    // Deadline: SIGKILL the direct child; `--die-with-parent` takes the namespace with it. The readers
    // finish by themselves when the pipes close; nothing waits on them.
    let _ = child.kill();
    let status = child.wait()?;
    drop((out_reader, err_reader));
    Ok(Run {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out,
    })
}

/// `<binary> --version` runs: the check for `prlimit` at start.
pub fn probe_prlimit(binary: &OsStr) -> Result<(), String> {
    match Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(st) if st.success() => Ok(()),
        Ok(st) => Err(format!(
            "`{} --version` exited with {st}",
            binary.to_string_lossy()
        )),
        Err(e) => Err(format!("`{}` could not run: {e}", binary.to_string_lossy())),
    }
}

/// A minimal confined `true` runs: the check for `bwrap` (and for user namespaces being permitted) at start.
pub fn probe_bwrap(binary: &OsStr) -> Result<(), String> {
    match Command::new(binary)
        .args([
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
        ])
        .args([
            "--symlink",
            "usr/bin",
            "/bin",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
        ])
        .args([
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--",
            "/usr/bin/true",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "`{}` cannot confine a process here: {}",
            binary.to_string_lossy(),
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Err(format!("`{}` could not run: {e}", binary.to_string_lossy())),
    }
}

/// Resolve a bare binary name through `PATH` to an absolute path (bwrap must bind it); paths pass through.
pub fn resolve_binary(binary: &OsStr) -> PathBuf {
    let p = Path::new(binary);
    if p.components().count() > 1 || p.is_absolute() {
        return p.to_path_buf();
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(binary);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    p.to_path_buf()
}

#[cfg(test)]
mod tests;
