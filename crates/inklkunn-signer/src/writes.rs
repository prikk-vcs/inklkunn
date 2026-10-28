//! The write sequence (internal design SGN-3): the only code in inklkunn that runs a prikk write verb.
//! Every write runs confined with the repository writable and the key directory read-only, under the
//! server-kept deadline and a memory ceiling; the environment carries seed-file *paths* and key ids, never
//! a seed. Reads the sequence needs (`log` after a seal) run with the repository read-only and no keys.
//! The runner is private: only the named operations exist, no "run any verb".

use std::path::Path;
use std::time::Duration;

use inklkunn_prikk::sandbox::{CallLimits, Mounts, Run, Sandbox};
use inklkunn_prikk::storage::{History, HistoryBlock, Verification};
use inklkunn_prikk::{parse_report, schema};
use serde::Deserialize;

use crate::keys::KeyDir;

/// Why a write did not happen (or did not complete).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignerError {
    /// prikk refused and said why; nothing changed. The text is prikk's (operator text).
    Refused(String),
    /// The call could not run (spawn, a missing tool), or was stopped at its bound. **After a stopped
    /// `commit` the repository's state is not known**: read `log` and `status` before retrying.
    Unavailable(String),
    /// prikk's output could not be read.
    Damaged(String),
    /// `commit` succeeded and `seal` did not: a queued patch awaits recovery (RFC 004).
    Incomplete {
        /// Which step failed.
        step: &'static str,
        /// What prikk said.
        detail: String,
    },
    /// The change is committed **and sealed**; only reading its ids back failed. Nothing needs recovery;
    /// the ids are in `log`.
    SealedButUnread(String),
    /// The call completed, but the temporary directory of decrypted seeds could not be removed. The operator
    /// removes the directories named `inklkunn-seeds-*` under the seed directory.
    SeedsNotRemoved,
}

impl std::fmt::Display for SignerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignerError::Refused(m) => write!(f, "prikk refused the write: {m}"),
            SignerError::Unavailable(m) => write!(f, "the write could not run to completion: {m}"),
            SignerError::Damaged(m) => write!(f, "prikk's output could not be read: {m}"),
            SignerError::Incomplete { step, detail } => {
                write!(
                    f,
                    "the change is committed but `{step}` failed; recovery must complete it: {detail}"
                )
            }
            SignerError::SealedButUnread(m) => {
                write!(
                    f,
                    "the change is sealed but its ids could not be read back: {m}"
                )
            }
            SignerError::SeedsNotRemoved => write!(
                f,
                "the call completed but a temporary directory of decrypted seeds could not be removed; \
                 remove the directories named `inklkunn-seeds-*` under the seed directory"
            ),
        }
    }
}

impl std::error::Error for SignerError {}

/// The ids a sealed change leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedIds {
    /// The sealed block.
    pub block_id: String,
    /// The patches it seals.
    pub patch_ids: Vec<String>,
    /// The view's state id after the seal.
    pub ref_state_id: String,
}

/// The wall-time bound for one write call.
pub const WRITE_DEADLINE: Duration = Duration::from_secs(120);
/// The memory ceiling for one write call (`RLIMIT_DATA`). Writes have no cost table yet; this is a fixed
/// bound against a runaway, generous for any content a proposal may carry. RFC 002's handoff measures
/// the write verbs and replaces it with a costed one.
pub const WRITE_CEILING: u64 = 1 << 30;

/// `status --format json`: the pinned schema name (ledger PK-27).
pub(crate) const STATUS_REPORT: &str = "status-report-v1";
/// `worktree-status --format json`.
pub(crate) const WORKTREE_REPORT: &str = "worktree-status-report-v1";

/// What the write sequence reads from `status`: the current branch and the queue.
#[derive(Debug, Deserialize)]
pub(crate) struct StatusReport {
    /// The repository's current branch (`heads/<name>`), absent in a repository with none.
    pub(crate) current_branch: Option<String>,
    /// Patches committed and not yet sealed.
    pub(crate) queue: QueueReport,
}

/// The unsealed queue.
#[derive(Debug, Deserialize)]
pub(crate) struct QueueReport {
    /// How many patches wait for a seal.
    pub(crate) count: u64,
}

/// What the write sequence reads from `worktree-status`: whether the worktree matches the baseline.
#[derive(Debug, Deserialize)]
pub(crate) struct WorktreeReport {
    /// No change and no declaration against the baseline.
    pub(crate) clean: bool,
}

/// The write-verb runner over a sandbox and a key directory.
#[derive(Debug, Clone)]
pub(crate) struct Writes {
    binary: std::ffi::OsString,
    sandbox: Sandbox,
}

/// The outcome of a confined call, read in the signer's terms.
fn outcome(verb: &str, run: Run) -> Result<Vec<u8>, SignerError> {
    if run.timed_out {
        return Err(SignerError::Unavailable(format!(
            "`{verb}` was stopped at its {} s bound; the repository's state must be read before a retry",
            WRITE_DEADLINE.as_secs()
        )));
    }
    if run.output_exceeded {
        return Err(SignerError::Unavailable(format!(
            "`{verb}` produced more output than the server keeps"
        )));
    }
    if !run.status.success() {
        let stderr = String::from_utf8_lossy(&run.stderr).trim().to_owned();
        if let Some(first) = stderr.lines().next()
            && (first.starts_with("bwrap:") || first.starts_with("prlimit:"))
        {
            return Err(SignerError::Unavailable(first.to_owned()));
        }
        let text = stderr
            .strip_prefix("error:")
            .map_or(stderr.as_str(), str::trim)
            .to_owned();
        return Err(SignerError::Refused(if text.is_empty() {
            format!("`{verb}` exited {} without a reason", run.status)
        } else {
            text
        }));
    }
    Ok(run.stdout)
}

/// Like [`outcome`], for a verb whose **report is the answer even when it exits 1**: `worktree-status` exits
/// 1 on a dirty tree and `verify` on a failing verdict, each still printing its JSON report on stdout. A
/// non-zero exit with a report on stdout is returned as the report; one without is the refusal it says.
fn report_outcome(verb: &str, run: Run) -> Result<Vec<u8>, SignerError> {
    let has_report = run.stdout.first() == Some(&b'{');
    if has_report && !run.timed_out && !run.output_exceeded {
        return Ok(run.stdout);
    }
    outcome(verb, run)
}

impl Writes {
    /// A runner for `binary` under `sandbox`.
    pub(crate) fn new(binary: impl Into<std::ffi::OsString>, sandbox: Sandbox) -> Self {
        Writes {
            // Resolved once, like the driver does at `open`: the sandbox clears the environment.
            binary: inklkunn_prikk::sandbox::resolve_binary(&binary.into()).into_os_string(),
            sandbox,
        }
    }

    fn limits() -> CallLimits {
        CallLimits {
            data_ceiling: Some(WRITE_CEILING),
            ..CallLimits::default()
        }
    }

    fn run(&self, mounts: &Mounts, args: &[&str]) -> Result<Vec<u8>, SignerError> {
        let verb = args.first().copied().unwrap_or("");
        let run = self
            .sandbox
            .run_mounts(
                self.binary.as_os_str(),
                mounts,
                args,
                Self::limits(),
                Some(WRITE_DEADLINE),
            )
            .map_err(|e| SignerError::Unavailable(e.to_string()))?;
        outcome(verb, run)
    }

    /// A write verb: the repository writable, the key directory read-only, prikk's seed variables set.
    fn write(&self, repo: &Path, keys: &KeyDir, args: &[&str]) -> Result<Vec<u8>, SignerError> {
        self.run(&Mounts::write(repo, keys.path(), keys.env()), args)
    }

    /// A read the sequence needs: the repository read-only, no keys in sight.
    fn read(&self, repo: &Path, args: &[&str]) -> Result<Vec<u8>, SignerError> {
        self.run(&Mounts::read(repo, None), args)
    }

    /// `prikk init` in `repo` (which must exist and be empty of a repository).
    pub(crate) fn init(&self, repo: &Path, keys: &KeyDir) -> Result<(), SignerError> {
        self.write(repo, keys, &["init"]).map(|_| ())
    }

    /// `prikk trust maintainer add` for the wiki's maintainer key.
    pub(crate) fn trust_maintainer(
        &self,
        repo: &Path,
        keys: &KeyDir,
        key_id: &str,
        public_key_hex: &str,
    ) -> Result<(), SignerError> {
        self.write(
            repo,
            keys,
            &[
                "trust",
                "maintainer",
                "add",
                "--key-id",
                key_id,
                "--public-key",
                public_key_hex,
            ],
        )
        .map(|_| ())
    }

    /// `prikk commit -m <message>` under the author seed, then `prikk seal --allow-no-audit` under the
    /// maintainer seed, then read the tip from `log` (read-only, no keys) and return its ids. A failure
    /// after `commit` is [`SignerError::Incomplete`]; a failure after `seal` is
    /// [`SignerError::SealedButUnread`].
    pub(crate) fn commit_and_seal(
        &self,
        repo: &Path,
        keys: &KeyDir,
        message: &str,
    ) -> Result<SealedIds, SignerError> {
        self.write(repo, keys, &["commit", "-m", message])?;
        self.write(repo, keys, &["seal", "--allow-no-audit"])
            .map_err(|e| SignerError::Incomplete {
                step: "seal",
                detail: e.to_string(),
            })?;
        let stdout = self
            .read(repo, &["log", "--limit", "1", "--format", "json"])
            .map_err(|e| SignerError::SealedButUnread(e.to_string()))?;
        let history: History = parse_report("log", &stdout, schema::LOG_REPORT)
            .map_err(|e| SignerError::SealedButUnread(e.to_string()))?;
        let Some(tip) = history.blocks.first() else {
            return Err(SignerError::SealedButUnread(
                "log listed no block after the seal".to_owned(),
            ));
        };
        Ok(SealedIds {
            block_id: tip.block_id.clone(),
            patch_ids: tip
                .patch_messages
                .iter()
                .map(|m| m.patch_id.clone())
                .collect(),
            ref_state_id: tip.ref_state_id.clone(),
        })
    }

    /// `prikk key generate --out <path>` (writes the seed, mode 0600, and its `.key-id` file). The key
    /// directory is the only mount, writable; no repository is involved.
    pub(crate) fn generate_key(&self, keys: &KeyDir, role: &str) -> Result<(), SignerError> {
        let out = keys.seed_file_text(role);
        self.run(
            &Mounts::dir_writable(keys.path()),
            &["key", "generate", "--out", &out],
        )
        .map(|_| ())
    }

    /// `prikk key public --seed-file <path>` → the public key hex (a public value). prikk prints
    /// `public key: <64 hex>`; anything else is refused rather than guessed at, since a seed is also 64 hex.
    pub(crate) fn public_key(&self, keys: &KeyDir, role: &str) -> Result<String, SignerError> {
        let seed = keys.seed_file_text(role);
        let stdout = self.run(
            &Mounts::read(keys.path(), None),
            &["key", "public", "--seed-file", &seed],
        )?;
        let text = String::from_utf8_lossy(&stdout);
        let mut found: Option<String> = None;
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("public key:") else {
                continue;
            };
            let hex = rest.trim();
            if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                if found.is_some() {
                    return Err(SignerError::Damaged(
                        "`key public` printed more than one public key".to_owned(),
                    ));
                }
                found = Some(hex.to_owned());
            }
        }
        found.ok_or_else(|| {
            SignerError::Damaged("`key public` printed no `public key: <hex>` line".to_owned())
        })
    }

    /// A read whose report is the answer even on exit 1 (see [`report_outcome`]); no keys in sight.
    fn read_report(&self, repo: &Path, args: &[&str]) -> Result<Vec<u8>, SignerError> {
        let verb = args.first().copied().unwrap_or("");
        let run = self
            .sandbox
            .run_mounts(
                self.binary.as_os_str(),
                &Mounts::read(repo, None),
                args,
                Self::limits(),
                Some(WRITE_DEADLINE),
            )
            .map_err(|e| SignerError::Unavailable(e.to_string()))?;
        report_outcome(verb, run)
    }

    /// `prikk status`: the current branch and the unsealed queue.
    pub(crate) fn status(&self, repo: &Path) -> Result<StatusReport, SignerError> {
        let out = self.read_report(repo, &["status", "--format", "json"])?;
        parse_report("status", &out, STATUS_REPORT).map_err(|e| SignerError::Damaged(e.to_string()))
    }

    /// Is the worktree exactly the baseline (no change, no declared rename)?
    pub(crate) fn worktree_clean(&self, repo: &Path) -> Result<bool, SignerError> {
        let out = self.read_report(repo, &["worktree-status", "--format", "json"])?;
        let report: WorktreeReport = parse_report("worktree-status", &out, WORKTREE_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))?;
        Ok(report.clean)
    }

    /// `prikk branch switch heads/<view>`: the worktree becomes the view's tip. prikk refuses a worktree that
    /// is not clean, before changing anything.
    pub(crate) fn switch(&self, repo: &Path, keys: &KeyDir, view: &str) -> Result<(), SignerError> {
        self.write(repo, keys, &["branch", "switch", view])
            .map(|_| ())
    }

    /// `prikk mv <from> <to>`: moves the file and declares the rename for the next commit. A move back
    /// cancels the declaration (ledger PK-29).
    pub(crate) fn mv(
        &self,
        repo: &Path,
        keys: &KeyDir,
        from: &str,
        to: &str,
    ) -> Result<(), SignerError> {
        self.write(repo, keys, &["mv", from, to]).map(|_| ())
    }

    /// `prikk commit --ref <view> -m <message>` under the author seed.
    pub(crate) fn commit(
        &self,
        repo: &Path,
        keys: &KeyDir,
        view: &str,
        message: &str,
    ) -> Result<(), SignerError> {
        self.write(repo, keys, &["commit", "--ref", view, "-m", message])
            .map(|_| ())
    }

    /// `prikk seal --allow-no-audit --ref <view>` under the maintainer seed.
    pub(crate) fn seal(&self, repo: &Path, keys: &KeyDir, view: &str) -> Result<(), SignerError> {
        self.write(repo, keys, &["seal", "--allow-no-audit", "--ref", view])
            .map(|_| ())
    }

    /// The newest block of a view, read back (read-only, no keys).
    pub(crate) fn tip(&self, repo: &Path, view: &str) -> Result<Option<HistoryBlock>, SignerError> {
        let stdout = self.read(
            repo,
            &["log", "--ref", view, "--limit", "1", "--format", "json"],
        )?;
        let history: History = parse_report("log", &stdout, schema::LOG_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))?;
        Ok(history.blocks.into_iter().next())
    }

    /// The newest block of the repository's **current** branch, or `None` when it has none yet (a fresh
    /// repository). Read-only, no keys.
    pub(crate) fn head(&self, repo: &Path) -> Result<Option<HistoryBlock>, SignerError> {
        let stdout = self.read(repo, &["log", "--limit", "1", "--format", "json"])?;
        let history: History = parse_report("log", &stdout, schema::LOG_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))?;
        Ok(history.blocks.into_iter().next())
    }

    /// `prikk verify`: the verdict of the repository's own checks (read-only, no keys). A failing verdict
    /// is a report, not an error.
    pub(crate) fn verify(&self, repo: &Path) -> Result<Verification, SignerError> {
        let out = self.read_report(repo, &["verify", "--format", "json"])?;
        parse_report("verify", &out, schema::VERIFY_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))
    }

    /// `prikk branch create heads/<name> --from <ref>`, published under the maintainer seed.
    pub(crate) fn branch_create(
        &self,
        repo: &Path,
        keys: &KeyDir,
        name: &str,
        from: &str,
    ) -> Result<(), SignerError> {
        self.write(repo, keys, &["branch", "create", name, "--from", from])
            .map(|_| ())
    }
}
