#![forbid(unsafe_code)]
//! `inklkunn-prikk` — the storage boundary: the [`Storage`] trait in inklkunn's words and its first
//! implementation, [`PrikkCli`], a sandboxed, version-pinned prikk CLI driver for the **read** verbs with
//! typed `--format json` output (RFC 001 D-3, D-4; internal design STO-1 … STO-5).
//!
//! This crate never names a write verb; the boundary gate (`tools/boundary-check`) enforces it. Writes
//! live in `inklkunn-signer`.
//!
//! - [`storage`]: the trait, the typed values, the typed outcomes.
//! - [`driver`]: `PrikkCli`, the pinned schema names, the exit-to-outcome mapping.
//! - [`sandbox`]: bubblewrap, `prlimit`, the server-kept deadline.
//! - [`cost`] and [`admission`]: the per-verb cost table and the light/heavy pools.

pub mod admission;
pub mod cost;
pub mod driver;
pub mod sandbox;
pub mod storage;

pub use admission::{Admission, Permit, Refusal};
pub use cost::{ReadVerb, SizeHints};
pub use driver::{
    DEFAULT_HISTORY_LIMIT, PrikkCli, is_repo_path, parse_report, parse_status, parse_version,
    schema,
};
pub use sandbox::{BwrapConfig, CallLimits, Mounts, Run, Sandbox};
pub use storage::{
    Change, Diff, DiffEntry, Entry, ExtractBounds, FileMeta, History, HistoryBlock, Materialized,
    Operation, Patch, PatchMessage, Point, Queue, QueuedPatch, RepoStatus, Result, Stage,
    StageOutcome, Storage, StorageError, Tree, Unavailable, Verdict, Verification, View, Views,
};

/// The prikk floor (RFC 001 D-3 / RFC 010 D-3): the oldest prikk the driver accepts. One value with the
/// CI workflow and the container recipe; `tools/floor-check` fails when they differ.
///
/// `0.54.0` carries what M1 needs: `extract` (with the claim's parent), `diff`, `tree`, `cat`, the
/// `log`/`show` JSON, and the `show <block-id>` fix.
pub const MIN_PRIKK_VERSION: &str = "0.54.0";
