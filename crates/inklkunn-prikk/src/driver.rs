//! `PrikkCli`: the `Storage` implementation over prikk's read verbs with `--format json` (RFC 001 D-3,
//! D-4). Every call: size the store, cost the verb, pass the admission gate, set the ceiling, run confined
//! under the server-kept deadline, map the exit to a typed outcome, check the report's `schema_version`
//! against the pinned name, deserialize.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::MIN_PRIKK_VERSION;
use crate::admission::{Admission, Refusal};
use crate::cost::{ReadVerb, StoreSizeError, ceiling_for, store_size};
use crate::sandbox::{CallLimits, Run, Sandbox, probe_bwrap, probe_prlimit, resolve_binary};
use crate::storage::{
    Change, Diff, ExtractBounds, FileMeta, History, Materialized, Point, Result, Storage,
    StorageError, Tree, Unavailable, Verification, Views,
};

/// The pinned report schemas (RFC 001 D-3): one name per verb. A report naming anything else is refused.
pub mod schema {
    /// `branch list --format json`.
    pub const BRANCH_LIST: &str = "branch-list-v1";
    /// `tree --format json`.
    pub const TREE_LISTING: &str = "tree-listing-v1";
    /// `cat --format json`.
    pub const PATH_CONTENT: &str = "path-content-v1";
    /// `log --format json`.
    pub const LOG_REPORT: &str = "log-report-v1";
    /// `show --format json`.
    pub const SHOW_REPORT: &str = "show-report-v1";
    /// `diff --format json`.
    pub const DIFF_REPORT: &str = "diff-report-v1";
    /// `extract --format json`.
    pub const EXTRACT_REPORT: &str = "extract-report-v1";
    /// `verify --format json`.
    pub const VERIFY_REPORT: &str = "verify-report-v1";
}

/// The driver.
#[derive(Debug, Clone)]
pub struct PrikkCli {
    binary: PathBuf,
    sandbox: Sandbox,
    admission: Admission,
    /// A fixed ceiling for every call (tests; the ceiling test needs one small enough to fire).
    fixed_ceiling: Option<u64>,
    /// `false` turns the ceiling off (never in production).
    ceilings: bool,
    version: String,
}

/// `prikk --version` → `(major, minor, patch)`.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let v = text.trim().strip_prefix("prikk")?.trim();
    let mut it = v.split(['.', '-', '+']).map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

fn floor() -> (u64, u64, u64) {
    parse_version(&format!("prikk {MIN_PRIKK_VERSION}")).unwrap_or((u64::MAX, 0, 0))
}

impl PrikkCli {
    /// Open the driver: check the binary's version against the floor and the sandbox prerequisites. Nothing
    /// here touches a repository.
    pub fn open(
        binary: impl Into<OsString>,
        sandbox: Sandbox,
        admission: Admission,
    ) -> Result<Self> {
        let binary = resolve_binary(&binary.into());
        let output = std::process::Command::new(&binary)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| StorageError::Unavailable {
                verb: "--version",
                reason: Unavailable::Spawn(format!("{}: {e}", binary.display())),
            })?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let Some(found) = parse_version(&text) else {
            return Err(StorageError::UnsupportedVersion {
                found: text,
                required: MIN_PRIKK_VERSION,
            });
        };
        if found < floor() {
            return Err(StorageError::UnsupportedVersion {
                found: text,
                required: MIN_PRIKK_VERSION,
            });
        }
        if let Sandbox::Bwrap(cfg) = &sandbox {
            probe_prlimit(&cfg.prlimit).map_err(|e| StorageError::Unavailable {
                verb: "prlimit",
                reason: Unavailable::Prerequisite(e),
            })?;
            probe_bwrap(&cfg.bwrap).map_err(|e| StorageError::Unavailable {
                verb: "bwrap",
                reason: Unavailable::Prerequisite(e),
            })?;
        }
        Ok(PrikkCli {
            binary,
            sandbox,
            admission,
            fixed_ceiling: None,
            ceilings: true,
            version: text,
        })
    }

    /// The version text the binary reported.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Every call gets this ceiling instead of the costed one (for tests of the ceiling path).
    #[must_use]
    pub fn with_fixed_ceiling(mut self, bytes: u64) -> Self {
        self.fixed_ceiling = Some(bytes);
        self
    }

    /// Turn ceilings off (tests on hosts where `prlimit` misbehaves; never in production).
    #[must_use]
    pub fn without_ceilings(mut self) -> Self {
        self.ceilings = false;
        self
    }

    fn execute(
        &self,
        verb: ReadVerb,
        repo: &Path,
        scratch: Option<&Path>,
        args: &[&str],
    ) -> Result<(Run, Option<u64>)> {
        let name = verb.name();
        let store = match store_size(repo) {
            Ok(s) => s,
            Err(StoreSizeError::NoRepository) => {
                return Err(StorageError::Refused {
                    verb: name,
                    message: format!("no prikk repository at {}", repo.display()),
                    kind: Some("no-repository".to_owned()),
                });
            }
            Err(e) => {
                return Err(StorageError::Damaged {
                    verb: name,
                    detail: e.to_string(),
                });
            }
        };
        let cost = verb.cost(store);
        let permit = self.admission.admit(cost).map_err(|r| match r {
            Refusal::Busy => StorageError::Unavailable {
                verb: name,
                reason: Unavailable::Busy,
            },
            Refusal::TooLarge { pool } => StorageError::TooLarge {
                verb: name,
                cost,
                budget: pool,
            },
        })?;
        let ceiling = if !self.ceilings {
            None
        } else if let Some(fixed) = self.fixed_ceiling {
            Some(fixed)
        } else {
            Some(ceiling_for(cost, Some(permit.bytes())))
        };
        let limits = CallLimits {
            data_ceiling: ceiling,
        };
        let deadline = Duration::from_secs(verb.deadline_secs());
        let run = self
            .sandbox
            .run(
                self.binary.as_os_str(),
                repo,
                scratch,
                args,
                limits,
                Some(deadline),
            )
            .map_err(|e| StorageError::Unavailable {
                verb: name,
                reason: Unavailable::Spawn(e.to_string()),
            })?;
        drop(permit);
        Ok((run, ceiling))
    }

    /// Map a non-success run to the typed outcome.
    fn failure(verb: ReadVerb, run: &Run, ceiling: Option<u64>) -> StorageError {
        use std::os::unix::process::ExitStatusExt;
        let name = verb.name();
        if run.timed_out {
            return StorageError::Unavailable {
                verb: name,
                reason: Unavailable::TimedOut {
                    seconds: verb.deadline_secs(),
                },
            };
        }
        let stderr = String::from_utf8_lossy(&run.stderr).trim().to_owned();
        if let Some(bytes) = ceiling {
            let aborted = run.status.code() == Some(134) || run.status.signal() == Some(6);
            let alloc_failed = stderr.contains("memory allocation of") && stderr.contains("failed");
            let loader_failed =
                run.status.code() == Some(127) && stderr.contains("Cannot allocate memory");
            let partial_verify =
                run.status.code() == Some(1) && stderr.contains("did not complete every stage");
            if aborted || alloc_failed || loader_failed || partial_verify {
                return StorageError::Unavailable {
                    verb: name,
                    reason: Unavailable::CeilingExceeded { bytes },
                };
            }
        }
        // Since 0.54 a refusal under `--format json` also comes as a `refusal-v1` report on stdout.
        let refusal: Option<RefusalReport> = serde_json::from_slice(&run.stdout)
            .ok()
            .filter(|r: &RefusalReport| r.schema_version == "refusal-v1");
        let message = match &refusal {
            Some(r) if !r.message.is_empty() => r.message.clone(),
            _ => stderr
                .strip_prefix("error:")
                .map_or(stderr.as_str(), str::trim)
                .to_owned(),
        };
        match run.status.code() {
            Some(1) => StorageError::Refused {
                verb: name,
                message,
                kind: refusal.and_then(|r| r.refusal),
            },
            Some(2) => StorageError::Driver {
                verb: name,
                detail: message,
            },
            code => StorageError::Damaged {
                verb: name,
                detail: format!(
                    "exit {}: {message}",
                    code.map_or("by signal".to_owned(), |c| c.to_string())
                ),
            },
        }
    }

    fn run_json<T: DeserializeOwned>(
        &self,
        verb: ReadVerb,
        repo: &Path,
        scratch: Option<&Path>,
        args: &[&str],
        pinned: &'static str,
    ) -> Result<T> {
        let (run, ceiling) = self.execute(verb, repo, scratch, args)?;
        if !run.status.success() || run.timed_out {
            return Err(Self::failure(verb, &run, ceiling));
        }
        parse_report(verb.name(), &run.stdout, pinned)
    }

    fn run_bytes(&self, verb: ReadVerb, repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let (run, ceiling) = self.execute(verb, repo, None, args)?;
        if !run.status.success() || run.timed_out {
            return Err(Self::failure(verb, &run, ceiling));
        }
        Ok(run.stdout)
    }
}

#[derive(serde::Deserialize)]
struct SchemaOnly {
    schema_version: String,
}

/// prikk's `refusal-v1` report: what it says on stdout when `--format json` and it refuses.
#[derive(serde::Deserialize)]
struct RefusalReport {
    schema_version: String,
    #[serde(default)]
    refusal: Option<String>,
    #[serde(default)]
    message: String,
}

/// Parse a `--format json` report: the `schema_version` must be `pinned` before the shape is read.
pub fn parse_report<T: DeserializeOwned>(
    verb: &'static str,
    stdout: &[u8],
    pinned: &'static str,
) -> Result<T> {
    let head: SchemaOnly = serde_json::from_slice(stdout).map_err(|e| StorageError::Damaged {
        verb,
        detail: format!("not a JSON report with a schema_version: {e}"),
    })?;
    if head.schema_version != pinned {
        return Err(StorageError::UnsupportedSchema {
            verb,
            found: head.schema_version,
            pinned,
        });
    }
    serde_json::from_slice(stdout).map_err(|e| StorageError::Damaged {
        verb,
        detail: format!("report `{pinned}` did not have the pinned shape: {e}"),
    })
}

fn into_check(into: &Path) -> Result<&Path> {
    if into.exists() {
        return Err(StorageError::Refused {
            verb: "extract",
            message: format!(
                "{} already exists; extract writes into a new directory",
                into.display()
            ),
            kind: Some("target-exists".to_owned()),
        });
    }
    into.parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| StorageError::Refused {
            verb: "extract",
            message: format!(
                "{} has no existing parent directory to mount as scratch",
                into.display()
            ),
            kind: Some("no-scratch".to_owned()),
        })
}

impl Storage for PrikkCli {
    fn views(&self, repo: &Path) -> Result<Views> {
        self.run_json(
            ReadVerb::Branch,
            repo,
            None,
            &["branch", "list", "--format", "json"],
            schema::BRANCH_LIST,
        )
    }

    fn tree(&self, repo: &Path, point: &Point, prefix: Option<&str>) -> Result<Tree> {
        let mut args = vec!["tree", "--ref", point.as_arg()];
        if let Some(p) = prefix {
            args.extend(["--prefix", p]);
        }
        args.extend(["--format", "json"]);
        self.run_json(ReadVerb::Tree, repo, None, &args, schema::TREE_LISTING)
    }

    fn stat(&self, repo: &Path, point: &Point, path: &str) -> Result<FileMeta> {
        let args = [
            "cat",
            "--path",
            path,
            "--ref",
            point.as_arg(),
            "--format",
            "json",
        ];
        self.run_json(ReadVerb::Cat, repo, None, &args, schema::PATH_CONTENT)
    }

    fn read(&self, repo: &Path, point: &Point, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let max = max_bytes.to_string();
        let args = [
            "cat",
            "--path",
            path,
            "--ref",
            point.as_arg(),
            "--max-bytes",
            &max,
        ];
        self.run_bytes(ReadVerb::Cat, repo, &args)
    }

    fn history(&self, repo: &Path, view: &str, limit: Option<u64>) -> Result<History> {
        let limit_text = limit.map(|l| l.to_string());
        let mut args = vec!["log", "--ref", view];
        if let Some(l) = &limit_text {
            args.extend(["--limit", l.as_str()]);
        }
        args.extend(["--format", "json"]);
        self.run_json(ReadVerb::Log, repo, None, &args, schema::LOG_REPORT)
    }

    fn change(&self, repo: &Path, id: &str) -> Result<Change> {
        self.run_json(
            ReadVerb::Show,
            repo,
            None,
            &["show", id, "--format", "json"],
            schema::SHOW_REPORT,
        )
    }

    fn diff(&self, repo: &Path, from: &Point, to: &Point, paths: &[&str]) -> Result<Diff> {
        let mut args = vec!["diff", "--from", from.as_arg(), "--to", to.as_arg()];
        for p in paths {
            args.extend(["--path", p]);
        }
        args.extend(["--format", "json"]);
        self.run_json(ReadVerb::Diff, repo, None, &args, schema::DIFF_REPORT)
    }

    fn plan(&self, repo: &Path, point: &Point) -> Result<Materialized> {
        // `extract` wants a directory name even for a plan; nothing is written, so no scratch is mounted
        // and the name is one that cannot exist inside the read-only view.
        let args = [
            "extract",
            "--ref",
            point.as_arg(),
            "/nonexistent-plan-target",
            "--plan-only",
            "--format",
            "json",
        ];
        self.run_json(ReadVerb::Extract, repo, None, &args, schema::EXTRACT_REPORT)
    }

    fn materialize(
        &self,
        repo: &Path,
        point: &Point,
        into: &Path,
        bounds: ExtractBounds,
    ) -> Result<Materialized> {
        let scratch = into_check(into)?;
        let Some(into_text) = into.to_str() else {
            return Err(StorageError::Driver {
                verb: "extract",
                detail: "the target directory is not UTF-8".to_owned(),
            });
        };
        let max_bytes = bounds.max_bytes.map(|b| b.to_string());
        let max_files = bounds.max_files.map(|f| f.to_string());
        let mut args = vec!["extract", "--ref", point.as_arg(), into_text];
        if let Some(b) = &max_bytes {
            args.extend(["--max-bytes", b.as_str()]);
        }
        if let Some(f) = &max_files {
            args.extend(["--max-files", f.as_str()]);
        }
        args.extend(["--format", "json"]);
        self.run_json(
            ReadVerb::Extract,
            repo,
            Some(scratch),
            &args,
            schema::EXTRACT_REPORT,
        )
    }

    fn verify(&self, repo: &Path) -> Result<Verification> {
        let (run, ceiling) = self.execute(
            ReadVerb::Verify,
            repo,
            None,
            &["verify", "--format", "json"],
        )?;
        // The verdict is read from the report on stdout, never from the exit code or stderr text: a failed
        // verdict exits non-zero but is a fact about the repository, while a ceiling hit is not a verdict.
        if !run.timed_out
            && let Ok(report) =
                parse_report::<Verification>("verify", &run.stdout, schema::VERIFY_REPORT)
            && (run.status.success() || !report.verdict.ok)
        {
            return Ok(report);
        }
        if run.status.success() && !run.timed_out {
            return parse_report("verify", &run.stdout, schema::VERIFY_REPORT);
        }
        Err(Self::failure(ReadVerb::Verify, &run, ceiling))
    }
}

#[cfg(test)]
mod tests;
