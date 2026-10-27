//! Confinement of the prikk subprocess (RFC 001 D-4; threat model INV-3, C-4c). The shape is planeter's,
//! whose investigations settled it (planeter RFC 011 D-2, D-5; its IS-37 and I3), with one difference:
//! inklkunn's driver only reads, so **the repository is bound read-only**; the only writable mount is the
//! call's scratch directory, and only for `extract`.
//!
//! - **`bwrap` is the server's direct child.** `--die-with-parent` ties the whole namespace to the server;
//!   the wall-time bound is kept by the server ([`Sandbox::run_mounts`] sends `SIGKILL` at the deadline).
//! - **Spawn and wait happen on one thread.** `--die-with-parent` is `PR_SET_PDEATHSIG` on the parent
//!   *thread*: a child handed to another thread, or spawned from a thread that then exits, is killed while
//!   healthy (planeter's investigation I3). The runner never moves the child.
//! - **The memory ceiling is `prlimit --data=<N> --` in front of the vector**: set before `bwrap` is
//!   exec'd and inherited by everything under it. A call that hits it is reported as a ceiling hit by the
//!   driver, never as a verdict. bwrap, as pid 1 of its namespace, reports an inner death by signal as exit
//!   `128 + n`; the driver reads that shape.
//! - **The environment is cleared** for prikk and for the wrappers. No `HOME`, so no key directory is ever
//!   visible to a read (the signer crate mounts seeds for a write call on its own terms, RFC 002).
//! - **Every mount is canonical.** Mount paths are resolved on the host before they are handed to bwrap,
//!   so a symlink cannot make a scratch or key directory land inside the repository.
//! - **Output is bounded.** The server reads at most the caller's cap from the child; past it the child is
//!   killed and the call reported as exceeding its output bound, never as a truncated value.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The memory-ceiling wrapper (util-linux): a runtime prerequisite.
pub const PRLIMIT: &str = "prlimit";
/// The sandbox binary: a runtime prerequisite.
pub const BWRAP: &str = "bwrap";
/// How long after stdout closes the runner waits for the exit before treating the call as hung.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// Per-call limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallLimits {
    /// `RLIMIT_DATA` for the call, in bytes; `None` = no ceiling.
    pub data_ceiling: Option<u64>,
    /// The most bytes of stdout the server keeps; past it the child is killed.
    pub max_output: u64,
}

impl Default for CallLimits {
    fn default() -> Self {
        CallLimits {
            data_ceiling: None,
            max_output: 64 << 20,
        }
    }
}

/// What a bounded run produced.
#[derive(Debug)]
pub struct Run {
    /// The child's exit status (a deadline kill shows the `SIGKILL` status; see `timed_out`).
    pub status: ExitStatus,
    /// Captured stdout (empty after a kill).
    pub stdout: Vec<u8>,
    /// Captured stderr (empty after a kill).
    pub stderr: Vec<u8>,
    /// The wall-time bound fired.
    pub timed_out: bool,
    /// The output bound fired.
    pub output_exceeded: bool,
}

/// How the prikk subprocess is confined.
#[derive(Debug, Clone)]
pub enum Sandbox {
    /// Bubblewrap: the production default.
    Bwrap(BwrapConfig),
    /// **Development and tests only:** prikk runs directly, with a cleared environment. A hosting binary
    /// must refuse this policy unless started with an explicit development flag (RFC 003's handoff T1).
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
    /// A path is not absolute or contains `.` or `..` components.
    InvalidPath(PathBuf),
    /// A path does not exist or could not be resolved on the host.
    Unresolvable(PathBuf, String),
    /// Two mounts overlap once resolved (one is inside the other, or they are the same place).
    Overlap(PathBuf, PathBuf),
}

impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountError::InvalidPath(p) => write!(
                f,
                "a sandbox mount must be an absolute path without `.` or `..` (got {})",
                p.display()
            ),
            MountError::Unresolvable(p, e) => {
                write!(
                    f,
                    "a sandbox mount could not be resolved: {} ({e})",
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

fn check_shape(p: &Path) -> Result<(), MountError> {
    if !p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(MountError::InvalidPath(p.to_path_buf()));
    }
    Ok(())
}

fn canonical(p: &Path) -> Result<PathBuf, MountError> {
    check_shape(p)?;
    p.canonicalize()
        .map_err(|e| MountError::Unresolvable(p.to_path_buf(), e.to_string()))
}

/// What a call may see and touch. Built only through the constructors: [`Mounts::read`] for the driver,
/// [`Mounts::write`] and [`Mounts::dir_writable`] for the signer. Nothing else is ever mounted, and the
/// boundary gate keeps the two writable constructors out of every crate but the signer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mounts {
    repo: PathBuf,
    repo_writable: bool,
    scratch: Option<PathBuf>,
    key_dir: Option<PathBuf>,
    env: Vec<(String, String)>,
}

impl Mounts {
    /// A read: the repository read-only, an optional scratch directory read-write (`extract`).
    pub fn read(repo: &Path, scratch: Option<&Path>) -> Self {
        Mounts {
            repo: repo.to_path_buf(),
            repo_writable: false,
            scratch: scratch.map(Path::to_path_buf),
            key_dir: None,
            env: Vec::new(),
        }
    }

    /// A write: the repository writable, a key directory read-only, variables for prikk (`PRIKK_*_SEED_FILE`,
    /// `PRIKK_*_KEY_ID`: paths and ids, never a seed).
    pub fn write(repo: &Path, key_dir: &Path, env: Vec<(String, String)>) -> Self {
        Mounts {
            repo: repo.to_path_buf(),
            repo_writable: true,
            scratch: None,
            key_dir: Some(key_dir.to_path_buf()),
            env,
        }
    }

    /// One directory, writable, as the working directory and the only mount (key generation, which needs
    /// no repository).
    pub fn dir_writable(dir: &Path) -> Self {
        Mounts {
            repo: dir.to_path_buf(),
            repo_writable: true,
            scratch: None,
            key_dir: None,
            env: Vec::new(),
        }
    }

    /// The working directory of the call (the repository, or the one directory).
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// Is the working directory bound writable?
    pub fn is_writable(&self) -> bool {
        self.repo_writable
    }

    /// The environment variables set inside the sandbox.
    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    /// Resolve every mount on the host and check the set: absolute, no `.`/`..`, existing, and no two of
    /// the repository, the scratch and the key directory overlapping once resolved. Returns the canonical
    /// set, which is what runs.
    pub fn validate(&self) -> Result<Mounts, MountError> {
        let repo = canonical(&self.repo)?;
        let scratch = self.scratch.as_deref().map(canonical).transpose()?;
        let key_dir = self.key_dir.as_deref().map(canonical).transpose()?;
        let mut seen: Vec<&Path> = vec![repo.as_path()];
        for p in [scratch.as_deref(), key_dir.as_deref()]
            .into_iter()
            .flatten()
        {
            for other in &seen {
                if p.starts_with(other) || other.starts_with(p) {
                    return Err(MountError::Overlap(p.to_path_buf(), other.to_path_buf()));
                }
            }
            seen.push(p);
        }
        Ok(Mounts {
            repo,
            repo_writable: self.repo_writable,
            scratch,
            key_dir,
            env: self.env.clone(),
        })
    }
}

impl Sandbox {
    /// The [`Command`] for `prikk_binary <args>` over `mounts` (already validated), with `limits` applied in
    /// front of the whole vector. The environment of every process in the vector is cleared.
    pub(crate) fn command_mounts(
        &self,
        prikk_binary: &OsStr,
        mounts: &Mounts,
        args: &[&str],
        limits: CallLimits,
    ) -> Command {
        let (inner, prlimit) = match self {
            Sandbox::Unconfined => {
                // The environment is cleared, so a bare name must be resolved here, not by the shell's PATH.
                let mut c = Command::new(resolve_binary(prikk_binary));
                c.current_dir(&mounts.repo).args(args);
                c.env_clear().env("PATH", "/usr/bin:/bin");
                for (k, v) in &mounts.env {
                    c.env(k, v);
                }
                (c, OsString::from(PRLIMIT))
            }
            Sandbox::Bwrap(cfg) => (cfg.command(prikk_binary, mounts, args), cfg.prlimit.clone()),
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
                // `prlimit` passes its environment through; it gets exactly the inner one.
                c.env_clear();
                for (k, v) in inner.get_envs() {
                    if let Some(v) = v {
                        c.env(k, v);
                    }
                }
                c
            }
        }
    }

    /// Run one call to completion over `mounts`: validate and resolve the mounts, spawn, collect stdout and
    /// stderr up to `limits.max_output`, and enforce `deadline` in the server (`SIGKILL`, `timed_out: true`).
    /// Spawn and wait both happen on the calling thread; see the module docs. **Blocks** the caller for up
    /// to the deadline plus a short grace.
    pub fn run_mounts(
        &self,
        prikk_binary: &OsStr,
        mounts: &Mounts,
        args: &[&str],
        limits: CallLimits,
        deadline: Option<Duration>,
    ) -> std::io::Result<Run> {
        let mounts = mounts
            .validate()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        run_bounded(
            self.command_mounts(prikk_binary, &mounts, args, limits),
            deadline,
            limits.max_output,
        )
    }

    /// Is this the confined policy?
    pub fn is_confined(&self) -> bool {
        matches!(self, Sandbox::Bwrap(_))
    }
}

impl BwrapConfig {
    fn command(&self, prikk_binary: &OsStr, mounts: &Mounts, args: &[&str]) -> Command {
        let prikk = resolve_binary(prikk_binary);
        let repo = mounts.repo.as_path();
        let mut c = Command::new(&self.bwrap);
        c.env_clear().env("PATH", "/usr/bin:/bin");
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
        // The repository: read-only for every verb of this driver (STO-5); writable only for the signer.
        c.arg(if mounts.repo_writable {
            "--bind"
        } else {
            "--ro-bind"
        })
        .arg(repo)
        .arg(repo);
        // The scratch directory, read-write, only when the call has one (`extract`).
        if let Some(s) = &mounts.scratch {
            c.arg("--bind").arg(s).arg(s);
        }
        // The key directory, read-only, only for the signer's write calls.
        if let Some(k) = &mounts.key_dir {
            c.arg("--ro-bind").arg(k).arg(k);
        }
        c.arg("--chdir").arg(repo);
        c.arg("--unshare-all");
        c.arg("--die-with-parent");
        c.arg("--new-session");
        c.arg("--clearenv");
        c.arg("--setenv").arg("PATH").arg("/usr/bin");
        for (k, v) in &mounts.env {
            c.arg("--setenv").arg(k).arg(v);
        }
        c.arg("--").arg(&prikk).args(args);
        c
    }
}

/// What a reader thread reports when it stops reading.
enum ReadEnd {
    Eof,
    Exceeded,
}

/// The child, killed and reaped if the runner leaves while it is still armed.
struct Reap {
    child: Child,
    armed: bool,
}

impl Drop for Reap {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A short, unconfined probe (`prikk --version` at open): bounded in time and output.
pub(crate) fn run_probe(cmd: Command, deadline: Duration) -> std::io::Result<Run> {
    run_bounded(cmd, Some(deadline), 64 << 10)
}

/// Spawn `cmd`, collect its output up to `max_output`, kill it at `deadline` or when the output bound is
/// passed. Spawn and wait on this thread (module docs).
fn run_bounded(
    mut cmd: Command,
    deadline: Option<Duration>,
    max_output: u64,
) -> std::io::Result<Run> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut guard = Reap {
        child: cmd.spawn()?,
        armed: true,
    };
    let started = Instant::now();
    let (Some(out), Some(err)) = (guard.child.stdout.take(), guard.child.stderr.take()) else {
        return Err(std::io::Error::other(
            "stdout and stderr were requested as pipes",
        ));
    };
    let (end_tx, end_rx) = mpsc::channel::<ReadEnd>();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut limited = out.take(max_output.saturating_add(1));
        let _ = limited.read_to_end(&mut buf);
        let end = if u64::try_from(buf.len()).unwrap_or(u64::MAX) > max_output {
            ReadEnd::Exceeded
        } else {
            ReadEnd::Eof
        };
        let _ = end_tx.send(end);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut limited = err.take(max_output.saturating_add(1));
        let _ = limited.read_to_end(&mut buf);
        buf
    });
    // 1. Wait for stdout to end (or the deadline, or the output bound).
    let end = match deadline {
        None => end_rx.recv().ok(),
        Some(d) => match end_rx.recv_timeout(d) {
            Ok(end) => Some(end),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => Some(ReadEnd::Eof),
        },
    };
    match end {
        Some(ReadEnd::Eof) => {
            // 2. The exit normally follows at once; wait for it up to the deadline plus a short grace.
            let limit = deadline.map(|d| d + EXIT_GRACE);
            let mut pause = Duration::from_micros(50);
            loop {
                if let Some(status) = guard.child.try_wait()? {
                    guard.armed = false;
                    let stdout = out_reader.join().unwrap_or_default();
                    let stderr = err_reader.join().unwrap_or_default();
                    return Ok(Run {
                        status,
                        stdout,
                        stderr,
                        timed_out: false,
                        output_exceeded: false,
                    });
                }
                if limit.is_some_and(|l| started.elapsed() >= l) {
                    break;
                }
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_millis(5));
            }
            Ok(killed(&mut guard, true, false))
        }
        Some(ReadEnd::Exceeded) => Ok(killed(&mut guard, false, true)),
        None => Ok(killed(&mut guard, true, false)),
    }
}

/// SIGKILL the direct child (`--die-with-parent` takes the namespace with it) and reap it. The reader
/// threads finish by themselves when the pipes close; nothing waits on them.
fn killed(guard: &mut Reap, timed_out: bool, output_exceeded: bool) -> Run {
    let _ = guard.child.kill();
    let status = guard.child.wait().unwrap_or_else(|_| {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(9)
    });
    guard.armed = false;
    Run {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out,
        output_exceeded,
    }
}

/// `<binary> --version` runs: the check for `prlimit` at start.
pub fn probe_prlimit(binary: &OsStr) -> Result<(), String> {
    match Command::new(binary)
        .arg("--version")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
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
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
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

/// Resolve a binary to an absolute path: a bare name through `PATH` (bwrap must bind it), a relative path
/// against the current directory; absolute paths pass through.
pub fn resolve_binary(binary: &OsStr) -> PathBuf {
    let p = Path::new(binary);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    if p.components().count() > 1 {
        return std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(binary);
            if candidate.is_file() {
                return std::path::absolute(&candidate).unwrap_or(candidate);
            }
        }
    }
    p.to_path_buf()
}

#[cfg(test)]
mod tests;
