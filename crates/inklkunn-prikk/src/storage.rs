//! The `Storage` trait: what inklkunn needs of a repository, in inklkunn's words (internal design STO-1), and
//! the typed values and outcomes every implementation speaks. The first implementation is
//! [`crate::PrikkCli`]; the trait is the seam for another backend or a future library (UD-2).
//!
//! Vocabulary: a **view** is what prikk calls a branch (`heads/<name>`); a **point** is a view name or a
//! block id; a **change** is a sealed block (planeter's word, kept); a **history** is the sealed blocks of a
//! view, newest first.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A place in a repository's history: a view (branch) or a block id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Point {
    /// A view by its full ref name, e.g. `heads/main`.
    View(String),
    /// A sealed block by its 64-hex id.
    Block(String),
}

impl Point {
    /// The text prikk's `--ref` takes.
    pub fn as_arg(&self) -> &str {
        match self {
            Point::View(v) | Point::Block(v) => v,
        }
    }
}

impl std::fmt::Display for Point {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_arg())
    }
}

/// One view (branch) and the id of its current state. `branch-list-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    /// Full ref name, e.g. `heads/main`.
    #[serde(rename = "ref_name")]
    pub name: String,
    /// The RefState id: changes whenever the view moves, so it keys caches.
    #[serde(rename = "ref_state_id")]
    pub state_id: String,
    /// Closed views keep their history and accept nothing.
    pub closed: bool,
    /// The repository's current branch (worktree), of no interest to a reader.
    pub current: bool,
}

/// The views of a repository. `branch-list-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Views {
    /// Pinned schema name.
    pub schema_version: String,
    /// Open views (prikk lists closed ones only with `--all`, which the driver never passes).
    #[serde(rename = "branches")]
    pub views: Vec<View>,
}

/// A file or directory in a tree. `tree-listing-v1` / `path-content-v1` metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Repository-relative path, `/`-separated.
    pub path: String,
    /// `file` or `dir`.
    pub kind: String,
    /// `text` or `binary` (prikk's classification).
    #[serde(default)]
    pub encoding: Option<String>,
    /// POSIX mode bits.
    #[serde(default)]
    pub mode: Option<u32>,
    /// Size in bytes.
    #[serde(default)]
    pub size: Option<u64>,
}

/// The files at a point. `tree-listing-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    /// Pinned schema name.
    pub schema_version: String,
    /// The point as asked.
    pub point: String,
    /// The block the point resolved to.
    pub target_block_id: String,
    /// The prefix filter, when one was given.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Entries in prikk's deterministic order.
    pub entries: Vec<Entry>,
}

/// One file's metadata at a point. `path-content-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    /// Pinned schema name.
    pub schema_version: String,
    /// The point as asked.
    pub point: String,
    /// The block the point resolved to.
    pub target_block_id: String,
    /// The file's path.
    pub path: String,
    /// `file`.
    pub kind: String,
    /// `text` or `binary`.
    #[serde(default)]
    pub encoding: Option<String>,
    /// POSIX mode bits.
    #[serde(default)]
    pub mode: Option<u32>,
    /// Size in bytes.
    pub size: u64,
}

/// One signed patch message in a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchMessage {
    /// The patch id.
    pub patch_id: String,
    /// The message stored with the patch (signed since prikk 0.32.0); inklkunn's CT-03 trailers live here.
    pub message: String,
}

/// One sealed block in a view's history. `log-report-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryBlock {
    /// The block id.
    pub block_id: String,
    /// The RefState id after this block.
    pub ref_state_id: String,
    /// Monotonic update sequence within the view.
    pub update_seq: u64,
    /// `Root`, `Normal`, …
    pub kind: String,
    /// A rollback block undoes earlier ones.
    #[serde(default)]
    pub rollback_block: bool,
    /// How many patches the block seals.
    #[serde(default)]
    pub patch_count: u64,
    /// The patches' messages.
    #[serde(default)]
    pub patch_messages: Vec<PatchMessage>,
    /// The RefState id before this block, `None` for the root.
    #[serde(default)]
    pub previous_ref_state_id: Option<String>,
}

/// The sealed history of a view, newest first. `log-report-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    /// Pinned schema name.
    pub schema_version: String,
    /// The view the history is of.
    #[serde(rename = "ref")]
    pub view: String,
    /// Blocks, newest first.
    pub blocks: Vec<HistoryBlock>,
}

/// A path reference in a patch operation: by path, or by node id when the path is not resolvable at show
/// time (a patch shown on its own).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRef {
    /// The path, when resolved.
    #[serde(default)]
    pub path: Option<String>,
    /// The node id, when not.
    #[serde(default)]
    pub unresolved_node_id: Option<String>,
}

/// One operation of a patch. `show-report-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    /// `edit-text`, `add-file`, `remove`, `rename`, …
    pub kind: String,
    /// The paths the operation touches.
    #[serde(default)]
    pub paths: Vec<PathRef>,
    /// The operation's content, whose shape depends on `kind`; kept as JSON for the presentation layer.
    #[serde(default)]
    pub content: serde_json::Value,
}

/// One patch of a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    /// The patch id.
    pub patch_id: String,
    /// Still queued (unsealed)? Never for a block's patches.
    #[serde(default)]
    pub queued: bool,
    /// Operations in order.
    #[serde(default)]
    pub operations: Vec<Operation>,
}

/// What one change (block) or one patch did. `show-report-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    /// Pinned schema name.
    pub schema_version: String,
    /// The patches.
    pub patches: Vec<Patch>,
}

/// One side of a diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSide {
    /// The point as asked.
    pub point: String,
    /// The block it resolved to.
    pub target_block_id: String,
}

/// One changed path. `diff-report-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffEntry {
    /// The path.
    pub path: String,
    /// `added`, `removed`, `modified`, …
    pub status: String,
    /// Metadata on the from side.
    #[serde(default)]
    pub from: Option<serde_json::Value>,
    /// Metadata on the to side.
    #[serde(default)]
    pub to: Option<serde_json::Value>,
    /// Unified hunks for text; empty for binary.
    #[serde(default)]
    pub hunks: Vec<String>,
}

/// What changed between two points. `diff-report-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    /// Pinned schema name.
    pub schema_version: String,
    /// The from side.
    pub from: DiffSide,
    /// The to side.
    pub to: DiffSide,
    /// Changed paths.
    pub entries: Vec<DiffEntry>,
    /// Paths prikk could not diff.
    #[serde(default)]
    pub unsupported_paths: Vec<String>,
}

/// The outcome of materializing a point. `extract-report-v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Materialized {
    /// Pinned schema name.
    pub schema_version: String,
    /// The point as asked.
    pub point: String,
    /// `ref`, `block`, `claim`.
    pub point_kind: String,
    /// The block the point resolved to.
    pub target_block_id: String,
    /// For a claim preview, its parent block.
    #[serde(default)]
    pub claim_parent_block_id: Option<String>,
    /// A provisional (claim) materialization.
    #[serde(default)]
    pub provisional: bool,
    /// `false` for a plan.
    pub written: bool,
    /// Files at the point.
    pub file_count: u64,
    /// Bytes at the point.
    pub total_bytes: u64,
    /// The largest single file.
    pub largest_file_bytes: u64,
}

/// One verification stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage {
    /// Stage name.
    pub stage: String,
    /// `evaluated`, `skipped`, …
    pub status: String,
}

/// A verification verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    /// Everything prikk checks holds.
    pub ok: bool,
    /// What failed, in prikk's terms.
    #[serde(default)]
    pub failed_conditions: Vec<serde_json::Value>,
}

/// The result of `verify`. `verify-report-v1`. A verdict is a fact about the repository; a call that could
/// not run to completion is a [`StorageError`], never a verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// Pinned schema name.
    pub schema_version: String,
    /// The verdict.
    pub verdict: Verdict,
    /// The stages run.
    #[serde(default)]
    pub stages: Vec<Stage>,
}

/// Bounds for materializing a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractBounds {
    /// Refuse the whole point before the first byte when it exceeds this many bytes.
    pub max_bytes: Option<u64>,
    /// Refuse when it has more files than this.
    pub max_files: Option<u64>,
}

/// Why a call did not run to completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// The process could not be started (binary or sandbox tool missing, permission).
    Spawn(String),
    /// The server's wall-time bound fired and the call was killed.
    TimedOut {
        /// The bound, in seconds.
        seconds: u64,
    },
    /// The memory ceiling stopped the call. This says nothing about the repository.
    CeilingExceeded {
        /// The ceiling, in bytes.
        bytes: u64,
    },
    /// The admission gate's pool was full for longer than the bounded wait.
    Busy,
    /// A runtime prerequisite is missing or failed (`bwrap`, `prlimit`): the tool's own first line.
    Prerequisite(String),
    /// The call wrote more than the server's output bound and was stopped; nothing of it is kept.
    OutputExceeded,
}

/// Every way a storage call ends other than with its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// prikk refused, and said why (its exit 1, "precondition not met"): a missing path or view, a bound
    /// exceeded. **`message` is operator text** (prikk's own words, which may name paths); a surface shows
    /// the user its own words chosen by `kind`, never `message`.
    Refused {
        /// The prikk verb.
        verb: &'static str,
        /// prikk's message, `error: ` prefix removed; for operators and logs.
        message: String,
        /// prikk's refusal code when it reported one on stdout (`refusal-v1`: `destination-exists`,
        /// `precondition-not-met`, …), or one of the driver's own, prefixed `driver:` (`driver:no-repository`),
        /// for callers that branch on it.
        kind: Option<String>,
    },
    /// An argument did not have the shape the trait requires (a view name, a block id, a repository path,
    /// a target name); refused before anything ran. `what` states the rule in one sentence and never
    /// echoes the value.
    InvalidArgument {
        /// The prikk verb.
        verb: &'static str,
        /// The rule the argument broke.
        what: &'static str,
    },
    /// The call could not run to completion; try again later or fix the deployment. Never a verdict.
    Unavailable {
        /// The prikk verb.
        verb: &'static str,
        /// Why.
        reason: Unavailable,
    },
    /// The read's ceiling is larger than the pool it would have to fit in; it can never be admitted here.
    TooLarge {
        /// The prikk verb.
        verb: &'static str,
        /// The ceiling the read would need, in bytes.
        needed: u64,
        /// The pool it would have to fit in, in bytes.
        pool: u64,
    },
    /// prikk's output could not be read: not UTF-8, not JSON, not the pinned shape, or an unexpected exit.
    Damaged {
        /// The prikk verb.
        verb: &'static str,
        /// What was wrong.
        detail: String,
    },
    /// The report's `schema_version` is not the pinned one (RFC 001 D-3): a newer prikk than the driver knows.
    UnsupportedSchema {
        /// The prikk verb.
        verb: &'static str,
        /// What prikk sent.
        found: String,
        /// What the driver pins.
        pinned: &'static str,
    },
    /// The prikk binary is older than the floor.
    UnsupportedVersion {
        /// `prikk --version`'s text.
        found: String,
        /// The floor.
        required: &'static str,
    },
    /// The driver itself misused prikk (its exit 2, usage): a bug here, not the repository's fault.
    Driver {
        /// The prikk verb.
        verb: &'static str,
        /// prikk's usage text.
        detail: String,
    },
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Refused { verb, message, .. } => {
                write!(f, "prikk {verb} refused: {message}")
            }
            StorageError::InvalidArgument { verb, what } => {
                write!(f, "prikk {verb} was not called: {what}")
            }
            StorageError::Unavailable { verb, reason } => match reason {
                Unavailable::Spawn(e) => write!(f, "prikk {verb} could not start: {e}"),
                Unavailable::TimedOut { seconds } => {
                    write!(
                        f,
                        "prikk {verb} exceeded its {seconds} s wall-time bound and was stopped"
                    )
                }
                Unavailable::CeilingExceeded { bytes } => {
                    write!(
                        f,
                        "prikk {verb} exceeded its {bytes}-byte memory ceiling and was stopped"
                    )
                }
                Unavailable::Busy => write!(f, "prikk {verb} was not admitted: the server is busy"),
                Unavailable::Prerequisite(p) => write!(f, "prikk {verb} cannot run: {p}"),
                Unavailable::OutputExceeded => {
                    write!(
                        f,
                        "prikk {verb} produced more output than the server keeps and was stopped"
                    )
                }
            },
            StorageError::TooLarge { verb, needed, pool } => {
                write!(
                    f,
                    "prikk {verb} would need a {needed}-byte ceiling, more than the {pool}-byte pool"
                )
            }
            StorageError::Damaged { verb, detail } => {
                write!(f, "prikk {verb} output could not be read: {detail}")
            }
            StorageError::UnsupportedSchema {
                verb,
                found,
                pinned,
            } => {
                write!(
                    f,
                    "prikk {verb} reported schema `{found}`; this driver pins `{pinned}`"
                )
            }
            StorageError::UnsupportedVersion { found, required } => {
                write!(f, "prikk `{found}` is older than the floor {required}")
            }
            StorageError::Driver { verb, detail } => {
                write!(f, "the driver misused prikk {verb}: {detail}")
            }
        }
    }
}

impl std::error::Error for StorageError {}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, StorageError>;

/// What inklkunn needs of a repository. Every method is a read; nothing here takes a write lock (STO-5).
///
/// **Every method blocks the calling thread** for up to the admission wait plus the verb's deadline (five
/// minutes for `verify`); an async caller runs it on a blocking thread. Arguments are checked for shape
/// before anything runs ([`StorageError::InvalidArgument`]): a view is `heads/<name>` (or `tags/`,
/// `remotes/`), a block id is 64 lower-case hex characters, a path is repository-relative without `.` or
/// `..` components.
pub trait Storage: Send + Sync {
    /// The open views and their state ids.
    fn views(&self, repo: &Path) -> Result<Views>;
    /// The files at a point, optionally under a path prefix (whole components).
    fn tree(&self, repo: &Path, point: &Point, prefix: Option<&str>) -> Result<Tree>;
    /// One file's metadata at a point.
    fn stat(&self, repo: &Path, point: &Point, path: &str) -> Result<FileMeta>;
    /// One file's bytes at a point, refused when larger than `max_bytes`.
    fn read(&self, repo: &Path, point: &Point, path: &str, max_bytes: u64) -> Result<Vec<u8>>;
    /// The sealed history at a point (a view, or a block to read the history that led to it), newest first,
    /// at most `limit` blocks (the implementation's default when `None`).
    fn history(&self, repo: &Path, view: &Point, limit: Option<u64>) -> Result<History>;
    /// What one block or patch changed.
    fn change(&self, repo: &Path, id: &str) -> Result<Change>;
    /// What changed between two points, restricted to `paths` when given.
    fn diff(&self, repo: &Path, from: &Point, to: &Point, paths: &[&str]) -> Result<Diff>;
    /// The size of a point without writing it.
    fn plan(&self, repo: &Path, point: &Point) -> Result<Materialized>;
    /// Write the files at a point into `<scratch>/<name>`. **`scratch` is the one directory the call may
    /// write** (it is what the sandbox binds read-write; it must exist and must not overlap the
    /// repository); `name` is one plain path component that does not exist there yet.
    fn materialize(
        &self,
        repo: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        bounds: ExtractBounds,
    ) -> Result<Materialized>;
    /// Verify the repository. A failed verdict is `Ok(Verification { verdict.ok: false, .. })`.
    fn verify(&self, repo: &Path) -> Result<Verification>;
}
