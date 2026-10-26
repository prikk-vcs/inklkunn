//! The write sequence (internal design SGN-3): the only code in inklkunn that runs a prikk write verb.
//! Every call runs confined with the repository writable and the key directory read-only, under the
//! server-kept deadline; the environment carries seed-file *paths* and key ids, never a seed.

use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use inklkunn_prikk::sandbox::{CallLimits, Mounts, Run, Sandbox};
use inklkunn_prikk::storage::History;
use inklkunn_prikk::{parse_report, schema};

use crate::keys::KeyDir;

/// Why a write did not happen (or did not complete).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignerError {
    /// prikk refused and said why; nothing changed.
    Refused(String),
    /// The call could not run (spawn, deadline); nothing is known to have changed.
    Unavailable(String),
    /// prikk's output could not be read.
    Damaged(String),
    /// `commit` succeeded and a later step failed: a queued patch awaits recovery (RFC 004).
    Incomplete {
        /// Which step failed.
        step: &'static str,
        /// What prikk said.
        detail: String,
    },
}

impl std::fmt::Display for SignerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignerError::Refused(m) => write!(f, "prikk refused the write: {m}"),
            SignerError::Unavailable(m) => write!(f, "the write could not run: {m}"),
            SignerError::Damaged(m) => write!(f, "prikk's output could not be read: {m}"),
            SignerError::Incomplete { step, detail } => {
                write!(
                    f,
                    "the change is committed but `{step}` failed; recovery must complete it: {detail}"
                )
            }
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

/// The write-verb runner over a sandbox and a key directory.
#[derive(Debug, Clone)]
pub struct Writes {
    binary: std::ffi::OsString,
    sandbox: Sandbox,
}

impl Writes {
    /// A runner for `binary` under `sandbox`.
    pub fn new(binary: impl Into<std::ffi::OsString>, sandbox: Sandbox) -> Self {
        Writes {
            binary: binary.into(),
            sandbox,
        }
    }

    fn run(&self, repo: &Path, keys: &KeyDir, args: &[&str]) -> Result<Run, SignerError> {
        let mounts = Mounts::write(repo, keys.path(), keys.env());
        self.sandbox
            .run_mounts(
                self.binary.as_os_str(),
                &mounts,
                args,
                CallLimits::default(),
                Some(WRITE_DEADLINE),
            )
            .map_err(|e| SignerError::Unavailable(e.to_string()))
    }

    /// Run a write verb; a non-zero exit is a refusal with prikk's text.
    pub fn verb(&self, repo: &Path, keys: &KeyDir, args: &[&str]) -> Result<Vec<u8>, SignerError> {
        let run = self.run(repo, keys, args)?;
        if run.timed_out {
            return Err(SignerError::Unavailable(format!(
                "`{}` exceeded the {} s bound",
                args.first().copied().unwrap_or(""),
                WRITE_DEADLINE.as_secs()
            )));
        }
        if !run.status.success() {
            let stderr = String::from_utf8_lossy(&run.stderr).trim().to_owned();
            return Err(SignerError::Refused(
                stderr
                    .strip_prefix("error:")
                    .map_or(stderr.as_str(), str::trim)
                    .to_owned(),
            ));
        }
        Ok(run.stdout)
    }

    /// `prikk init` in `repo` (which must exist and be empty of a repository).
    pub fn init(&self, repo: &Path, keys: &KeyDir) -> Result<(), SignerError> {
        self.verb(repo, keys, &["init"]).map(|_| ())
    }

    /// `prikk trust maintainer add` for the wiki's maintainer key.
    pub fn trust_maintainer(
        &self,
        repo: &Path,
        keys: &KeyDir,
        key_id: &str,
        public_key_hex: &str,
    ) -> Result<(), SignerError> {
        self.verb(
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
    /// maintainer seed, then read the tip from `log` and return its ids. A failure after `commit` is
    /// [`SignerError::Incomplete`].
    pub fn commit_and_seal(
        &self,
        repo: &Path,
        keys: &KeyDir,
        message: &str,
    ) -> Result<SealedIds, SignerError> {
        self.verb(repo, keys, &["commit", "-m", message])?;
        self.verb(repo, keys, &["seal", "--allow-no-audit"])
            .map_err(|e| SignerError::Incomplete {
                step: "seal",
                detail: e.to_string(),
            })?;
        let stdout = self
            .verb(repo, keys, &["log", "--limit", "1", "--format", "json"])
            .map_err(|e| SignerError::Incomplete {
                step: "log",
                detail: e.to_string(),
            })?;
        let history: History = parse_report("log", &stdout, schema::LOG_REPORT)
            .map_err(|e| SignerError::Damaged(e.to_string()))?;
        let Some(tip) = history.blocks.first() else {
            return Err(SignerError::Damaged(
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

    /// `prikk key generate --out <path>` (writes the seed, mode 0600, and its `.key-id` file).
    pub fn generate_key(&self, keys: &KeyDir, role: &str) -> Result<(), SignerError> {
        // Key generation needs no repository: the key directory itself is the only writable mount, so a
        // tiny empty "repository" directory is not required — prikk writes only the `--out` path.
        let mounts = Mounts {
            repo: keys.path().to_path_buf(),
            repo_writable: true,
            scratch: None,
            key_dir: None,
            env: Vec::new(),
        };
        let out = keys.seed_file(role);
        let Some(out_text) = out.to_str() else {
            return Err(SignerError::Refused(
                "the key directory path is not UTF-8".to_owned(),
            ));
        };
        let run = self
            .sandbox
            .run_mounts(
                self.binary.as_os_str(),
                &mounts,
                &["key", "generate", "--out", out_text],
                CallLimits::default(),
                Some(WRITE_DEADLINE),
            )
            .map_err(|e| SignerError::Unavailable(e.to_string()))?;
        if !run.status.success() {
            return Err(SignerError::Refused(
                String::from_utf8_lossy(&run.stderr).trim().to_owned(),
            ));
        }
        Ok(())
    }

    /// `prikk key public --seed-file <path>` → the public key hex (a public value).
    pub fn public_key(&self, keys: &KeyDir, role: &str) -> Result<String, SignerError> {
        let seed = keys.seed_file(role);
        let Some(seed_text) = seed.to_str() else {
            return Err(SignerError::Refused(
                "the key directory path is not UTF-8".to_owned(),
            ));
        };
        let mounts = Mounts {
            repo: keys.path().to_path_buf(),
            repo_writable: false,
            scratch: None,
            key_dir: None,
            env: Vec::new(),
        };
        let run = self
            .sandbox
            .run_mounts(
                self.binary.as_os_str(),
                &mounts,
                &["key", "public", "--seed-file", seed_text],
                CallLimits::default(),
                Some(WRITE_DEADLINE),
            )
            .map_err(|e| SignerError::Unavailable(e.to_string()))?;
        if !run.status.success() {
            return Err(SignerError::Refused(
                String::from_utf8_lossy(&run.stderr).trim().to_owned(),
            ));
        }
        let text = String::from_utf8_lossy(&run.stdout).trim().to_owned();
        // prikk prints the hex, possibly with a label; keep the last hex token.
        let hex = text
            .split_whitespace()
            .rev()
            .find(|t| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned);
        hex.ok_or_else(|| {
            SignerError::Damaged(format!("`key public` printed no 64-hex public key: {text}"))
        })
    }
}

/// Resolve a bare `prikk` the way the driver does.
pub fn default_binary() -> std::ffi::OsString {
    inklkunn_prikk::sandbox::resolve_binary(OsStr::new("prikk")).into_os_string()
}
