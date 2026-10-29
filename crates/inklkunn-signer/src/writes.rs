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

use crate::cost::{self, WriteVerb};
use crate::keys::{KeyDir, Role};

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
    /// Another act is writing this wiki (another thread, or another process such as the command line beside a
    /// running server), and holds its write lock. **Nothing was done.** Try again when it is over.
    Busy {
        /// The wiki.
        wiki: String,
        /// The process that holds the lock, when it could be read (a courtesy for the operator).
        holder: Option<u32>,
    },
    /// The call was stopped by its memory ceiling (`RLIMIT_DATA`; threat model C-9a). **Not a refusal and not a
    /// failed check**: nothing is wrong with what was asked, the call needed more memory than its ceiling
    /// allowed. Where the repository is depends on the step the caller was at.
    CeilingExceeded {
        /// The prikk verb that was stopped.
        verb: String,
        /// The ceiling it ran under, in bytes.
        bytes: u64,
    },
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
            SignerError::Busy { wiki, holder } => match holder {
                Some(pid) => write!(
                    f,
                    "wiki {wiki} is being written by process {pid}; nothing was done, try again when it is finished"
                ),
                None => write!(
                    f,
                    "wiki {wiki} is being written by another process; nothing was done, try again when it is finished"
                ),
            },
            SignerError::CeilingExceeded { verb, bytes } => write!(
                f,
                "`{verb}` was stopped by its memory ceiling ({bytes} bytes); it is not refused and not failed, it needs more memory"
            ),
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
/// `status --format json`: the pinned schema name (ledger PK-27).
pub(crate) const STATUS_REPORT: &str = "status-report-v1";
/// `worktree-status --format json`.
pub(crate) const WORKTREE_REPORT: &str = "worktree-status-report-v1";

/// `archive export --format json`: the pinned schema name (ledger PK-23; measured on prikk 0.55.0).
pub(crate) const ARCHIVE_REPORT: &str = "prepo-export-report-v1";

/// What the signer reads from `archive export`'s report.
#[derive(Debug, Deserialize)]
pub(crate) struct ArchiveReport {
    /// The export completed.
    pub(crate) ok: bool,
    /// The archive's size in bytes, which the signer checks against the file it finds.
    pub(crate) total_bytes: u64,
    /// The repository format the archive carries.
    pub(crate) repository_format: u64,
    /// The prikk that wrote it.
    pub(crate) tool_version: String,
}

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

/// Where a signing call gets its seed from. A call asks for **one role** and is handed a key directory that
/// holds that role's seed only (RFC 002 C-1a, C-1c: `commit` parses attacker-influenced paths and content with
/// the repository writable, and must not be able to read the maintainer seed). The key store's implementation
/// decrypts the seed for exactly the call and removes it afterwards; the bootstrap signer's hands out its plain
/// directory restricted to the role.
pub(crate) trait SeedAccess {
    /// Run `f` with the key directory of `role`.
    fn with_role<T>(
        &self,
        role: Role,
        f: impl FnOnce(&KeyDir) -> Result<T, SignerError>,
    ) -> Result<T, SignerError>;
}

/// What a call's memory ceiling is sized by: the repository whose store a fresh walk measures, and the bytes of
/// the change the verb reads whole (`commit`, `seal`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sizing<'a> {
    pub(crate) repo: Option<&'a Path>,
    pub(crate) change_bytes: u64,
}

impl<'a> Sizing<'a> {
    /// Sized by the repository's store alone.
    pub(crate) fn repo(repo: &'a Path) -> Self {
        Sizing {
            repo: Some(repo),
            change_bytes: 0,
        }
    }

    /// Sized by the repository's store and a change of `change_bytes`.
    pub(crate) fn change(repo: &'a Path, change_bytes: u64) -> Self {
        Sizing {
            repo: Some(repo),
            change_bytes,
        }
    }

    /// No repository: a flat verb.
    pub(crate) fn none() -> Self {
        Sizing {
            repo: None,
            change_bytes: 0,
        }
    }
}

/// The write-verb runner over a sandbox and a key directory.
#[derive(Debug, Clone)]
pub(crate) struct Writes {
    binary: std::ffi::OsString,
    sandbox: Sandbox,
}

/// prikk's words with every path into a call's seed directory replaced: `error: no seed at
/// /dev/shm/…/inklkunn-seeds-<bits>/author.seed` names where keys are, and an error must never (RFC 002 D-3).
fn scrub(text: &str) -> String {
    use crate::key_store::DIR_PREFIX;
    let path_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(DIR_PREFIX) {
        let (before, after) = rest.split_at(at);
        // Back to where the path starts, forward to where it ends.
        let start = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| path_char(*c))
            .last()
            .map_or(before.len(), |(i, _)| i);
        out.push_str(before.get(..start).unwrap_or_default());
        out.push_str("<seed directory>");
        let mut end = after
            .char_indices()
            .find(|(_, c)| !path_char(*c))
            .map_or(after.len(), |(i, _)| i);
        // A full stop that ends a sentence is not part of the path.
        while end > 0 && after.get(..end).is_some_and(|p| p.ends_with('.')) {
            end -= 1;
        }
        rest = after.get(end..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// The outcome of a confined call, read in the signer's terms.
fn outcome(verb: &str, run: Run, ceiling: u64) -> Result<Vec<u8>, SignerError> {
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
    if !run.status.success() && run.died_of_ceiling() {
        // The driver's classifier (PK-9): a death of the memory ceiling is not a refusal.
        return Err(SignerError::CeilingExceeded {
            verb: verb.to_owned(),
            bytes: ceiling,
        });
    }
    if !run.status.success() {
        let stderr = scrub(String::from_utf8_lossy(&run.stderr).trim());
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

/// Does a `verify` report say a stage failed for want of memory? The message after the exact prefix `i/o error:`
/// must be, whole, `out of memory` (the measured shape) or `Cannot allocate memory (os error 12)`. **Never a
/// substring match**: a message can embed a name the repository's contributors chose (a page titled "out of
/// memory"), and that must not turn damage into a ceiling hit.
pub(crate) fn resource_failure_in(report: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(report) else {
        return false;
    };
    value
        .get("stages")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|stages| {
            stages.iter().any(|stage| {
                stage.get("status").and_then(serde_json::Value::as_str) == Some("failed")
                    && stage
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|m| m.trim().strip_prefix("i/o error:"))
                        .is_some_and(|rest| {
                            matches!(
                                rest.trim(),
                                "out of memory" | "Cannot allocate memory (os error 12)"
                            )
                        })
            })
        })
}

/// The `(block id, maintainer key id)` pairs of the `sealed-block <id>: <key id>` lines of `prikk verify`'s text.
/// A line that is not exactly that shape (a 64-character lower-case hex block id, a colon, a space, a key id
/// prikk accepts) is not one; a block with no key id listed has an empty one.
pub(crate) fn parse_sealed_blocks(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("sealed-block ")?;
            let (block, key) = rest.split_once(':')?;
            let key = key.trim();
            let block_ok = block.len() == 64
                && block
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
            let key_ok = key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            (block_ok && key_ok && key.len() <= 128).then(|| (block.to_owned(), key.to_owned()))
        })
        .collect()
}

/// Like [`outcome`], for a verb whose **report is the answer even when it exits 1**: `worktree-status` exits
/// 1 on a dirty tree and `verify` on a failing verdict, each still printing its JSON report on stdout. A
/// non-zero exit with a report on stdout is returned as the report; one without is the refusal it says.
fn report_outcome(verb: &str, run: Run, ceiling: u64) -> Result<Vec<u8>, SignerError> {
    let has_report = run.stdout.first() == Some(&b'{');
    if has_report && !run.timed_out && !run.output_exceeded {
        return Ok(run.stdout);
    }
    outcome(verb, run, ceiling)
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

    /// Run `args` over `mounts` under the ceiling the verb's cost rule gives for `sized_by` (the repository
    /// whose store size the rule needs; `None` for a verb that has none).
    fn run(
        &self,
        mounts: &Mounts,
        size: Sizing<'_>,
        args: &[&str],
    ) -> Result<Vec<u8>, SignerError> {
        let name = args.first().copied().unwrap_or("");
        let run = self.run_raw(mounts, size, args)?;
        outcome(name, run.0, run.1)
    }

    /// The run itself, and the ceiling it had.
    fn run_raw(
        &self,
        mounts: &Mounts,
        size: Sizing<'_>,
        args: &[&str],
    ) -> Result<(Run, u64), SignerError> {
        // A verb with no cost rule is never run (fail closed): the table is the list of what the signer runs.
        let verb = WriteVerb::of(args).ok_or_else(|| {
            SignerError::Refused(format!(
                "`{}` has no memory rule in the signer's cost table",
                args.first().copied().unwrap_or("")
            ))
        })?;
        let ceiling = cost::ceiling_in(verb, size.repo, size.change_bytes)?;
        let run = self
            .sandbox
            .run_mounts(
                self.binary.as_os_str(),
                mounts,
                args,
                CallLimits {
                    data_ceiling: Some(ceiling),
                    ..CallLimits::default()
                },
                Some(WRITE_DEADLINE),
            )
            .map_err(|e| SignerError::Unavailable(e.to_string()))?;
        Ok((run, ceiling))
    }

    /// A write verb that signs: the repository writable and **the one seed it signs with** (`role`) in a key
    /// directory mounted read-only, with that role's variables set and the other's absent.
    fn write_signed(
        &self,
        repo: &Path,
        seeds: &impl SeedAccess,
        role: Role,
        change_bytes: u64,
        args: &[&str],
    ) -> Result<Vec<u8>, SignerError> {
        seeds.with_role(role, |keys| {
            self.run(
                &Mounts::write(repo, Some(keys.path()), keys.env()),
                Sizing::change(repo, change_bytes),
                args,
            )
        })
    }

    /// A write verb that signs nothing (`init`, `trust`, `mv`, `branch switch`): the repository writable and no
    /// seed anywhere in sight, no key variable, no key directory.
    fn write_keyless(&self, repo: &Path, args: &[&str]) -> Result<Vec<u8>, SignerError> {
        self.run(
            &Mounts::write(repo, None, Vec::new()),
            Sizing::repo(repo),
            args,
        )
    }

    /// A read the sequence needs: the repository read-only, no keys in sight.
    fn read(&self, repo: &Path, args: &[&str]) -> Result<Vec<u8>, SignerError> {
        self.run(&Mounts::read(repo, None), Sizing::repo(repo), args)
    }

    /// `prikk init` in `repo` (which must exist and be empty of a repository). Signs nothing.
    pub(crate) fn init(&self, repo: &Path) -> Result<(), SignerError> {
        self.write_keyless(repo, &["init"]).map(|_| ())
    }

    /// `prikk trust maintainer add` for the wiki's maintainer key.
    pub(crate) fn trust_maintainer(
        &self,
        repo: &Path,
        key_id: &str,
        public_key_hex: &str,
    ) -> Result<(), SignerError> {
        self.write_keyless(
            repo,
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

    /// `prikk trust maintainer remove --key-id <id>`: the key can seal nothing more, **and** (prikk 0.54.1,
    /// ledger PK-34) everything it sealed stops verifying. A retired key is the answer to a compromise, not
    /// to a routine rotation.
    pub(crate) fn untrust_maintainer(&self, repo: &Path, key_id: &str) -> Result<(), SignerError> {
        self.write_keyless(repo, &["trust", "maintainer", "remove", "--key-id", key_id])
            .map(|_| ())
    }

    /// `prikk commit -m <message>` under the author seed, then `prikk seal --allow-no-audit` under the
    /// maintainer seed, then read the tip from `log` (read-only, no keys) and return its ids. A failure
    /// after `commit` is [`SignerError::Incomplete`]; a failure after `seal` is
    /// [`SignerError::SealedButUnread`].
    pub(crate) fn commit_and_seal(
        &self,
        repo: &Path,
        seeds: &impl SeedAccess,
        message: &str,
    ) -> Result<SealedIds, SignerError> {
        self.write_signed(repo, seeds, Role::Author, 0, &["commit", "-m", message])?;
        self.write_signed(
            repo,
            seeds,
            Role::Maintainer,
            0,
            &["seal", "--allow-no-audit"],
        )
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
            Sizing::none(),
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
            Sizing::none(),
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
        let (run, ceiling) = self.run_raw(&Mounts::read(repo, None), Sizing::repo(repo), args)?;
        report_outcome(verb, run, ceiling)
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
    pub(crate) fn switch(&self, repo: &Path, view: &str) -> Result<(), SignerError> {
        self.write_keyless(repo, &["branch", "switch", view])
            .map(|_| ())
    }

    /// `prikk mv <from> <to>`: moves the file and declares the rename for the next commit. A move back
    /// cancels the declaration (ledger PK-29).
    pub(crate) fn mv(&self, repo: &Path, from: &str, to: &str) -> Result<(), SignerError> {
        self.write_keyless(repo, &["mv", from, to]).map(|_| ())
    }

    /// `prikk commit --ref <view> -m <message>` under the author seed.
    pub(crate) fn commit(
        &self,
        repo: &Path,
        seeds: &impl SeedAccess,
        view: &str,
        message: &str,
        change_bytes: u64,
    ) -> Result<(), SignerError> {
        self.write_signed(
            repo,
            seeds,
            Role::Author,
            change_bytes,
            &["commit", "--ref", view, "-m", message],
        )
        .map(|_| ())
    }

    /// `prikk seal --allow-no-audit --ref <view>` under the maintainer seed.
    pub(crate) fn seal(
        &self,
        repo: &Path,
        seeds: &impl SeedAccess,
        view: &str,
        change_bytes: u64,
    ) -> Result<(), SignerError> {
        self.write_signed(
            repo,
            seeds,
            Role::Maintainer,
            change_bytes,
            &["seal", "--allow-no-audit", "--ref", view],
        )
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
    /// is a report, not an error, **unless the failure is a stage that ran out of memory**: under a memory
    /// ceiling `verify` can print a complete failing report whose `objects` stage failed with `i/o error: out of
    /// memory` (ledger PK-22), which is the ceiling and says nothing about the repository, so it is
    /// [`SignerError::CeilingExceeded`] and never a verdict. (The read driver's full rule, which also refuses
    /// every other partial report, is the verify-verdict follow-on's; the signer needs this part now, because
    /// a seal followed by a ceiling-failed `verify` must not read as a failed verification.)
    pub(crate) fn verify(&self, repo: &Path) -> Result<Verification, SignerError> {
        self.verify_under(&Mounts::read(repo, None), repo)
    }

    /// `prikk verify` with the repository **writable and no key in sight** (the keyless mode): the one thing
    /// that makes prikk record its checkpoint (`.prikk/cache/replay-verified-blocks.v1`) besides `seal`, which a
    /// read-only `verify` does not (ledger PK-24). The same verdict rules as [`Writes::verify`].
    pub(crate) fn verify_writable(&self, repo: &Path) -> Result<Verification, SignerError> {
        self.verify_under(&Mounts::write(repo, None, Vec::new()), repo)
    }

    fn verify_under(&self, mounts: &Mounts, repo: &Path) -> Result<Verification, SignerError> {
        let args = ["verify", "--format", "json"];
        let (run, ceiling) = self.run_raw(mounts, Sizing::repo(repo), &args)?;
        if !run.timed_out && !run.output_exceeded && resource_failure_in(&run.stdout) {
            return Err(SignerError::CeilingExceeded {
                verb: "verify".to_owned(),
                bytes: ceiling,
            });
        }
        let out = report_outcome("verify", run, ceiling)?;
        parse_report("verify", &out, schema::VERIFY_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))
    }

    /// `prikk archive export <file>` with the repository writable (prikk writes lock files and caches inside
    /// `.prikk/`, ledger PK-23), no key in sight, and `output_dir` the one other directory writable: the archive
    /// is written to `file`, which must lie directly in `output_dir`. The report is read under its pinned schema
    /// and must say `ok`.
    pub(crate) fn archive_export(
        &self,
        repo: &Path,
        output_dir: &Path,
        file: &Path,
    ) -> Result<ArchiveReport, SignerError> {
        let file_text = file
            .to_str()
            .ok_or_else(|| SignerError::Refused("the archive's path is not UTF-8".to_owned()))?;
        let out = self.run(
            &Mounts::write_keyless_with_output(repo, output_dir),
            Sizing::repo(repo),
            &["archive", "export", file_text, "--format", "json"],
        )?;
        let report: ArchiveReport = parse_report("archive export", &out, ARCHIVE_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))?;
        if !report.ok {
            return Err(SignerError::Damaged(
                "`archive export` did not report success".to_owned(),
            ));
        }
        Ok(report)
    }

    /// Every view of the repository, closed ones included (`branch list --all`): what a rotation records the
    /// tips of, and what the check of seals by superseded keys walks. Read-only, no keys.
    pub(crate) fn all_views(&self, repo: &Path) -> Result<Vec<String>, SignerError> {
        let stdout = self.read(repo, &["branch", "list", "--all", "--format", "json"])?;
        let views: inklkunn_prikk::storage::Views =
            parse_report("branch list", &stdout, schema::BRANCH_LIST)
                .map_err(|e| SignerError::Damaged(e.to_string()))?;
        Ok(views.views.into_iter().map(|v| v.name).collect())
    }

    /// The block ids of a view's history, newest first, at most `limit` of them (`log --ref <view> --limit`).
    pub(crate) fn view_blocks(
        &self,
        repo: &Path,
        view: &str,
        limit: u64,
    ) -> Result<Vec<String>, SignerError> {
        let limit = limit.to_string();
        let stdout = self.read(
            repo,
            &["log", "--ref", view, "--limit", &limit, "--format", "json"],
        )?;
        let history: History = parse_report("log", &stdout, schema::LOG_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))?;
        Ok(history.blocks.into_iter().map(|b| b.block_id).collect())
    }

    /// Who sealed which block, as `prikk verify` lists it: one `sealed-block <block id>: <maintainer key id>`
    /// line per sealed block of the repository (prikk 0.55.0; **the only place prikk names a block's signer**,
    /// ledger PK-39: no JSON report does). A repository whose verification fails (a retired key's history,
    /// ledger PK-34) still prints them.
    pub(crate) fn sealed_block_signers(
        &self,
        repo: &Path,
    ) -> Result<Vec<(String, String)>, SignerError> {
        let (run, _ceiling) =
            self.run_raw(&Mounts::read(repo, None), Sizing::repo(repo), &["verify"])?;
        if run.timed_out || run.output_exceeded {
            return Err(SignerError::Unavailable(
                "`verify` was stopped before it listed the sealed blocks".to_owned(),
            ));
        }
        let text = String::from_utf8_lossy(&run.stdout).into_owned();
        let signers = parse_sealed_blocks(&text);
        if signers.is_empty() && !run.status.success() {
            // Nothing listed and a failure: the ceiling, or a refusal, with the usual words.
            return outcome("verify", run, 0).map(|_| Vec::new());
        }
        Ok(signers)
    }

    /// `prikk branch create heads/<name> --from <ref>`, published under the maintainer seed.
    pub(crate) fn branch_create(
        &self,
        repo: &Path,
        seeds: &impl SeedAccess,
        name: &str,
        from: &str,
    ) -> Result<(), SignerError> {
        self.write_signed(
            repo,
            seeds,
            Role::Maintainer,
            0,
            &["branch", "create", name, "--from", from],
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests;
