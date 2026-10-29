//! `PrikkCli`: the `Storage` implementation over prikk's read verbs with `--format json` (RFC 001 D-3,
//! D-4). Every call: **check the arguments' shapes** (nothing user-shaped reaches prikk's parser), size the
//! store, cost the verb, derive the ceiling, pass the admission gate with the ceiling, run confined under
//! the server-kept deadline and output bound, map the exit to a typed outcome, check the report's
//! `schema_version` against the pinned name, deserialize.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::MIN_PRIKK_VERSION;
use crate::admission::{Admission, Refusal};
use crate::cost::{MAX_REPORT_BYTES, ReadVerb, SizeHints, StoreSizeError, ceiling_for, store_size};
use crate::sandbox::{
    CallLimits, Mounts, Run, Sandbox, probe_bwrap, probe_prlimit, resolve_binary,
};
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

/// `history` lists this many blocks when the caller gives no limit.
pub const DEFAULT_HISTORY_LIMIT: u64 = 100;
/// The wall-time bound of the version probe at open.
const PROBE_DEADLINE: Duration = Duration::from_secs(10);

/// The driver.
#[derive(Debug, Clone)]
pub struct PrikkCli {
    binary: PathBuf,
    sandbox: Sandbox,
    admission: Admission,
    version: String,
    size_hints: Option<SizeHints>,
    knobs: Knobs,
}

/// Test knobs: every field `None` in a production build (the `test-knobs` feature is never enabled there).
#[derive(Debug, Clone, Copy, Default)]
struct Knobs {
    fixed_ceiling: Option<u64>,
    fixed_deadline: Option<Duration>,
    output_cap: Option<u64>,
}

/// `prikk --version` → `(major, minor, patch)`.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let v = text.trim().strip_prefix("prikk")?.trim();
    let mut it = v.split(['.', '-', '+']).map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// The floor as a tuple; `floor-check` keeps the string in step with the workflows.
pub const FLOOR: (u64, u64, u64) = crate::cost::BOUND_TO;

const _: () = assert!(
    FLOOR.0 == 0 && FLOOR.1 == 54 && FLOOR.2 == 0,
    "the floor tuple must equal MIN_PRIKK_VERSION"
);

impl PrikkCli {
    /// Open the driver: check the binary's version against the floor and the sandbox prerequisites. Nothing
    /// here touches a repository. The version probe runs bounded and with a cleared environment.
    pub fn open(
        binary: impl Into<OsString>,
        sandbox: Sandbox,
        admission: Admission,
    ) -> Result<Self> {
        let binary = resolve_binary(&binary.into());
        let mut cmd = std::process::Command::new(&binary);
        cmd.arg("--version")
            .env_clear()
            .env("PATH", "/usr/bin:/bin");
        let run = crate::sandbox::run_probe(cmd, PROBE_DEADLINE).map_err(|e| {
            StorageError::Unavailable {
                verb: "--version",
                reason: Unavailable::Spawn(format!("{}: {e}", binary.display())),
            }
        })?;
        if run.timed_out || !run.status.success() {
            return Err(StorageError::Unavailable {
                verb: "--version",
                reason: Unavailable::Spawn(format!(
                    "`{} --version` did not succeed ({})",
                    binary.display(),
                    if run.timed_out {
                        "no answer within the probe's bound".to_owned()
                    } else {
                        run.status.to_string()
                    }
                )),
            });
        }
        let text = String::from_utf8_lossy(&run.stdout).trim().to_owned();
        let Some(found) = parse_version(&text) else {
            return Err(StorageError::UnsupportedVersion {
                found: text,
                required: MIN_PRIKK_VERSION,
            });
        };
        if found < FLOOR {
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
            version: text,
            size_hints: None,
            knobs: Knobs::default(),
        })
    }

    /// Let a caller who knows when a repository changed hand the driver its store size, so that the cost
    /// estimate does not walk `.prikk` on every call. The walk stays the fallback whenever there is no
    /// fresh figure for the repository (see [`SizeHints`]).
    #[must_use]
    pub fn with_size_hints(mut self, hints: SizeHints) -> Self {
        self.size_hints = Some(hints);
        self
    }

    /// The version text the binary reported.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Every call gets this ceiling instead of the costed one (tests of the ceiling path).
    #[cfg(feature = "test-knobs")]
    #[must_use]
    pub fn with_fixed_ceiling(mut self, bytes: u64) -> Self {
        self.knobs.fixed_ceiling = Some(bytes);
        self
    }

    /// Every call gets this deadline instead of the verb's (tests of the deadline path).
    #[cfg(feature = "test-knobs")]
    #[must_use]
    pub fn with_fixed_deadline(mut self, deadline: Duration) -> Self {
        self.knobs.fixed_deadline = Some(deadline);
        self
    }

    /// Every call gets this output cap instead of the verb's (tests of the output bound).
    #[cfg(feature = "test-knobs")]
    #[must_use]
    pub fn with_output_cap(mut self, bytes: u64) -> Self {
        self.knobs.output_cap = Some(bytes);
        self
    }

    fn execute(
        &self,
        verb: ReadVerb,
        repo: &Path,
        scratch: Option<&Path>,
        args: &[&str],
        max_output: u64,
    ) -> Result<(Run, Option<u64>)> {
        let name = verb.name();
        let store = match self.store_bytes(verb, repo) {
            Ok(s) => s,
            Err(StoreSizeError::NoRepository) => {
                return Err(StorageError::Refused {
                    verb: name,
                    message: "no prikk repository at the wiki's directory".to_owned(),
                    kind: Some("driver:no-repository".to_owned()),
                    reason: None,
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
        // The ceiling is what the process may really take, so the ceiling is what the pool is charged.
        let ceiling = self
            .knobs
            .fixed_ceiling
            .unwrap_or_else(|| ceiling_for(cost));
        let permit = self.admission.admit(ceiling).map_err(|r| match r {
            Refusal::Busy => StorageError::Unavailable {
                verb: name,
                reason: Unavailable::Busy,
            },
            Refusal::TooLarge { pool } => StorageError::TooLarge {
                verb: name,
                needed: ceiling,
                pool,
            },
        })?;
        let limits = CallLimits {
            data_ceiling: Some(ceiling),
            max_output: self.knobs.output_cap.unwrap_or(max_output),
        };
        let deadline = self
            .knobs
            .fixed_deadline
            .unwrap_or_else(|| Duration::from_secs(verb.deadline_secs()));
        let run = self
            .sandbox
            .run_mounts(
                self.binary.as_os_str(),
                &Mounts::read(repo, scratch),
                args,
                limits,
                Some(deadline),
            )
            .map_err(|e| StorageError::Unavailable {
                verb: name,
                reason: Unavailable::Spawn(e.to_string()),
            })?;
        drop(permit);
        Ok((run, Some(ceiling)))
    }

    /// The figure the cost estimate is derived from. A verb whose cost does not follow *S* (`branch`, `log`)
    /// needs only to know that the repository is there; a hint stands in for the walk when the caller gave a
    /// fresh one; otherwise the walk is the truth. A hint never vouches for the repository: `.prikk` is
    /// still checked.
    fn store_bytes(&self, verb: ReadVerb, repo: &Path) -> std::result::Result<u64, StoreSizeError> {
        let hint = if verb.rule().store_tenths == 0 {
            Some(0)
        } else {
            self.size_hints.as_ref().and_then(|h| h.get(repo))
        };
        match hint {
            Some(bytes) if repo.join(".prikk").is_dir() => Ok(bytes),
            Some(_) => Err(StoreSizeError::NoRepository),
            None => store_size(repo),
        }
    }

    /// Map a non-success run to the typed outcome.
    fn failure(
        verb: ReadVerb,
        run: &Run,
        ceiling: Option<u64>,
        deadline: Duration,
    ) -> StorageError {
        use std::os::unix::process::ExitStatusExt;
        let name = verb.name();
        if run.output_exceeded {
            return StorageError::Unavailable {
                verb: name,
                reason: Unavailable::OutputExceeded,
            };
        }
        if run.timed_out {
            return StorageError::Unavailable {
                verb: name,
                reason: Unavailable::TimedOut {
                    seconds: deadline.as_secs(),
                },
            };
        }
        let stderr = String::from_utf8_lossy(&run.stderr).trim().to_owned();
        // A death by signal: on the direct child as a signal, through bwrap (pid 1 of its namespace) as exit
        // 128 + n. Rust's allocator abort is SIGABRT (134).
        let signal = run
            .status
            .signal()
            .map(|s| u8::try_from(s).unwrap_or(0))
            .or_else(|| match run.status.code() {
                Some(c @ 129..=192) => u8::try_from(c - 128).ok(),
                _ => None,
            });
        if let Some(bytes) = ceiling
            && run.died_of_ceiling()
        {
            return StorageError::Unavailable {
                verb: name,
                reason: Unavailable::CeilingExceeded { bytes },
            };
        }
        // The sandbox tools' own failures are not prikk's refusals: bwrap and prlimit name themselves.
        if let Some(first) = stderr.lines().next()
            && (first.starts_with("bwrap:") || first.starts_with("prlimit:"))
        {
            return StorageError::Unavailable {
                verb: name,
                reason: Unavailable::Prerequisite(first.to_owned()),
            };
        }
        // Since 0.54 a refusal under `--format json` may also come as a `refusal-v1` report on stdout.
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
        let message = if message.is_empty() {
            "prikk gave no reason".to_owned()
        } else {
            message
        };
        match run.status.code() {
            Some(1) => {
                let (kind, reason) = match refusal {
                    Some(r) => (r.refusal, r.reason),
                    None => (None, None),
                };
                StorageError::Refused {
                    verb: name,
                    message,
                    kind,
                    reason,
                }
            }
            Some(2) => StorageError::Driver {
                verb: name,
                detail: message,
            },
            code => StorageError::Damaged {
                verb: name,
                detail: match (code, signal) {
                    (Some(c), Some(s)) => {
                        format!("exit {c} (signal {s} inside the sandbox): {message}")
                    }
                    (Some(c), None) => format!("exit {c}: {message}"),
                    (None, Some(s)) => format!("killed by signal {s}: {message}"),
                    (None, None) => format!("no exit status: {message}"),
                },
            },
        }
    }

    fn deadline_for(&self, verb: ReadVerb) -> Duration {
        self.knobs
            .fixed_deadline
            .unwrap_or_else(|| Duration::from_secs(verb.deadline_secs()))
    }

    fn run_json<T: DeserializeOwned>(
        &self,
        verb: ReadVerb,
        repo: &Path,
        scratch: Option<&Path>,
        args: &[&str],
        pinned: &'static str,
    ) -> Result<T> {
        let (run, ceiling) = self.execute(verb, repo, scratch, args, MAX_REPORT_BYTES)?;
        if !run.status.success() || run.timed_out || run.output_exceeded {
            return Err(Self::failure(verb, &run, ceiling, self.deadline_for(verb)));
        }
        parse_report(verb.name(), &run.stdout, pinned)
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
    reason: Option<String>,
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

// ---- argument shapes: checked here so that nothing user-shaped reaches prikk's parser ----

fn is_ref_name(s: &str) -> bool {
    let Some((kind, name)) = s.split_once('/') else {
        return false;
    };
    matches!(kind, "heads" | "tags" | "remotes")
        && !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['.', '-'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
        && !name.contains("//")
        && !name.contains("/.")
}

fn is_hex_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A repository-relative path as prikk accepts it (its `path-safety` rules, checked here first): no leading
/// `/` or `-`, no empty, `.` or `..` component, no control bytes, at most 1024 bytes.
pub fn is_repo_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 1024
        && !s.starts_with(['/', '-'])
        && !s.bytes().any(|b| b < 0x20 || b == 0x7f)
        && s.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

fn check_point(verb: &'static str, p: &Point) -> Result<()> {
    let ok = match p {
        Point::View(v) => is_ref_name(v),
        Point::Block(b) => is_hex_id(b),
    };
    if ok {
        Ok(())
    } else {
        Err(StorageError::InvalidArgument {
            verb,
            what: match p {
                Point::View(_) => "a view is `heads/<name>`, `tags/<name>` or `remotes/<name>`",
                Point::Block(_) => "a block id is 64 lower-case hex characters",
            },
        })
    }
}

fn check_path(verb: &'static str, s: &str) -> Result<()> {
    if is_repo_path(s) {
        Ok(())
    } else {
        Err(StorageError::InvalidArgument {
            verb,
            what: "a repository path is relative, has no `.` or `..` components, no control characters and at most 1024 bytes",
        })
    }
}

fn check_id(verb: &'static str, s: &str) -> Result<()> {
    if is_hex_id(s) {
        Ok(())
    } else {
        Err(StorageError::InvalidArgument {
            verb,
            what: "a block or patch id is 64 lower-case hex characters",
        })
    }
}

fn check_name(verb: &'static str, s: &str) -> Result<()> {
    if !s.is_empty()
        && s.len() <= 255
        && !s.starts_with(['.', '-'])
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(StorageError::InvalidArgument {
            verb,
            what: "a target name is one plain path component (letters, digits, `.`, `_`, `-`)",
        })
    }
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
        check_point("tree", point)?;
        if let Some(p) = prefix {
            check_path("tree", p)?;
        }
        let mut args = vec!["tree", "--ref", point.as_arg()];
        if let Some(p) = prefix {
            args.extend(["--prefix", p]);
        }
        args.extend(["--format", "json"]);
        self.run_json(ReadVerb::Tree, repo, None, &args, schema::TREE_LISTING)
    }

    fn stat(&self, repo: &Path, point: &Point, path: &str) -> Result<FileMeta> {
        check_point("cat", point)?;
        check_path("cat", path)?;
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
        check_point("cat", point)?;
        check_path("cat", path)?;
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
        let (run, ceiling) = self.execute(
            ReadVerb::Cat,
            repo,
            None,
            &args,
            max_bytes.saturating_add(64 << 10),
        )?;
        if !run.status.success() || run.timed_out || run.output_exceeded {
            return Err(Self::failure(
                ReadVerb::Cat,
                &run,
                ceiling,
                self.deadline_for(ReadVerb::Cat),
            ));
        }
        Ok(run.stdout)
    }

    fn history(&self, repo: &Path, view: &Point, limit: Option<u64>) -> Result<History> {
        check_point("log", view)?;
        if matches!(view, Point::Block(_)) {
            // prikk's `log --ref` takes a ref; a block id there is an empty list with exit 0 (ledger PK-36).
            // An answer that looks like "no history" must never come from a call that cannot give one.
            return Err(StorageError::InvalidArgument {
                verb: "log",
                what: "a history is read from a view (`heads/<name>`); prikk's `log` does not take a block id",
            });
        }
        let limit_text = limit.unwrap_or(DEFAULT_HISTORY_LIMIT).to_string();
        let args = [
            "log",
            "--ref",
            view.as_arg(),
            "--limit",
            limit_text.as_str(),
            "--format",
            "json",
        ];
        self.run_json(ReadVerb::Log, repo, None, &args, schema::LOG_REPORT)
    }

    fn change(&self, repo: &Path, id: &str) -> Result<Change> {
        check_id("show", id)?;
        self.run_json(
            ReadVerb::Show,
            repo,
            None,
            &["show", id, "--format", "json"],
            schema::SHOW_REPORT,
        )
    }

    fn diff(&self, repo: &Path, from: &Point, to: &Point, paths: &[&str]) -> Result<Diff> {
        check_point("diff", from)?;
        check_point("diff", to)?;
        for p in paths {
            check_path("diff", p)?;
        }
        let mut args = vec!["diff", "--from", from.as_arg(), "--to", to.as_arg()];
        for p in paths {
            args.extend(["--path", p]);
        }
        args.extend(["--format", "json"]);
        self.run_json(ReadVerb::Diff, repo, None, &args, schema::DIFF_REPORT)
    }

    fn plan(&self, repo: &Path, point: &Point) -> Result<Materialized> {
        check_point("extract", point)?;
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
        scratch: &Path,
        name: &str,
        bounds: ExtractBounds,
    ) -> Result<Materialized> {
        check_point("extract", point)?;
        check_name("extract", name)?;
        if !scratch.is_dir() {
            return Err(StorageError::InvalidArgument {
                verb: "extract",
                what: "the scratch directory must exist",
            });
        }
        let into = scratch.join(name);
        if into.symlink_metadata().is_ok() {
            return Err(StorageError::InvalidArgument {
                verb: "extract",
                what: "the target name already exists in the scratch directory",
            });
        }
        let Some(into_text) = into.to_str() else {
            return Err(StorageError::Driver {
                verb: "extract",
                detail: "the scratch path is not UTF-8".to_owned(),
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
            MAX_REPORT_BYTES,
        )?;
        // The verdict is read from the report on stdout, never from the exit code or stderr text (prikk's
        // rule: graded results are in the JSON report; exit 1 is any operational failure). A failed verdict
        // is a fact about the repository, while a ceiling hit or a kill is not a verdict.
        if !run.timed_out
            && !run.output_exceeded
            && let Ok(report) =
                parse_report::<Verification>("verify", &run.stdout, schema::VERIFY_REPORT)
            && (run.status.success() || !report.verdict.ok)
        {
            return Ok(report);
        }
        if run.status.success() && !run.timed_out && !run.output_exceeded {
            return parse_report("verify", &run.stdout, schema::VERIFY_REPORT);
        }
        Err(Self::failure(
            ReadVerb::Verify,
            &run,
            ceiling,
            self.deadline_for(ReadVerb::Verify),
        ))
    }
}

#[cfg(test)]
mod tests;
