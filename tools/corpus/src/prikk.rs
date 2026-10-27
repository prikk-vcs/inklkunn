//! prikk as the generator drives it: a throwaway key pair under a private `HOME`, and the handful of verbs
//! the corpora need. Every call is unconfined (this is a tool run by hand on a scratch directory).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::sh;

/// A prikk handle bound to a key directory's parent as `HOME`.
pub struct Prikk {
    home: PathBuf,
}

impl Prikk {
    /// Use (and create) `keys` as the key directory: `HOME=<keys>/..`, keys in `<HOME>/.config/prikk`.
    /// `keys` must end in `.config/prikk` or be empty; the simplest call passes a fresh directory and lets
    /// `setup` fill `<keys>/.config/prikk`.
    pub fn with_home(home: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        Ok(Prikk {
            home: home.to_path_buf(),
        })
    }

    fn env(&self) -> Vec<(&'static str, OsString)> {
        vec![("HOME", self.home.clone().into_os_string())]
    }

    /// Run a prikk verb in `repo`.
    pub fn run(&self, repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
        let env = self.env();
        let env_refs: Vec<(&str, &std::ffi::OsStr)> =
            env.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
        sh(repo, &env_refs, "prikk", args)
    }

    /// `prikk setup .`: init, both keys (created in `HOME/.config/prikk` if absent), maintainer trusted.
    pub fn setup(&self, repo: &Path) -> Result<(), String> {
        std::fs::create_dir_all(repo).map_err(|e| e.to_string())?;
        self.run(repo, &["setup", "."]).map(|_| ())
    }

    /// Commit the worktree and seal; return the new tip block id.
    pub fn commit_and_seal(&self, repo: &Path, message: &str) -> Result<String, String> {
        self.run(repo, &["commit", "-m", message])?;
        self.run(repo, &["seal", "--allow-no-audit"])?;
        self.tip(repo, None)
    }

    /// The tip block id of a ref (the current branch when `None`).
    pub fn tip(&self, repo: &Path, reference: Option<&str>) -> Result<String, String> {
        let mut args = vec!["log", "--limit", "1", "--format", "json"];
        if let Some(r) = reference {
            args.extend(["--ref", r]);
        }
        let out = self.run(repo, &args)?;
        let v: serde_json::Value = serde_json::from_slice(&out).map_err(|e| e.to_string())?;
        v.get("blocks")
            .and_then(|b| b.get(0))
            .and_then(|b| b.get("block_id"))
            .and_then(|id| id.as_str())
            .map(str::to_owned)
            .ok_or_else(|| "log reported no block".to_owned())
    }

    /// The store size in bytes (`.prikk`, apparent sizes).
    pub fn store_size(repo: &Path) -> u64 {
        let mut total = 0u64;
        let mut stack = vec![repo.join(".prikk")];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let Ok(md) = e.metadata() else {
                    continue;
                };
                if md.is_dir() {
                    stack.push(e.path());
                } else {
                    total += md.len();
                }
            }
        }
        total
    }
}
