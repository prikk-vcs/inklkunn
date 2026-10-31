//! The per-verb cost table and the repository size estimate (RFC 001 D-4; internal design STO-3, STO-4).
//!
//! A read is admitted by **estimated peak memory**, derived from the repository's store size *S* (a fresh
//! walk of `.prikk`, or a caller's recent [`SizeHints`] figure) and the verb, through the one table here.
//! Every rule carries the prikk version the table is bound to ([`BOUND_TO`]); a test fails when that is not
//! [`crate::MIN_PRIKK_VERSION`], so the table is revisited at every re-baseline (RFC 010).
//!
//! **Provenance of the multiples (spike A, 2026-10-27, prikk 0.54.1, `tools/spikes/spike-a.py` with
//! `--bisect` on wiki-shaped text repositories of 3.6 / 18.6 / 76 / 212 MiB store size, 1,000 and 10,000
//! pages, a 500-deep chain; the smallest working `RLIMIT_DATA`, which is what the ceiling counts):**
//! `branch` and `log` are flat (7 MiB); `tree` and `cat` need 1.0 × S; `show` of a small patch is flat
//! (10 MiB) and follows the patch's content otherwise; `diff` of the whole history needs **4.6 × S**;
//! `extract` 0.9 × S; `verify` 2.0 × S. `status` (handoff 004 T5, prikk 0.55.0): flat, 2 MiB at 3,000 pages. The table keeps the **larger** of inklkunn's measurement and
//! planeter's single-large-file worst case on 0.52.0 (`tree`/`cat` 2.1 ×, `show` 3.6 ×, `verify` 4.1 ×),
//! because an attachment is exactly that shape and M0 had none to measure; the single-large-attachment
//! shape joins the re-baseline corpus when attachments arrive (M1). `diff` has no planeter precedent and
//! takes 5.0 × from the measurement with margin. Every rule carries [`BOUND_TO`]; a test fails when that is
//! not [`crate::MIN_PRIKK_VERSION`], so the table is revisited at every re-baseline (RFC 010).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// One mebibyte.
pub const MIB: u64 = 1 << 20;
/// The smallest cost any read is admitted with: prikk's own working set.
pub const COST_FLOOR: u64 = 16 * MIB;
/// `show` follows the patch's content, which no store walk sees; its floor is higher.
pub const SHOW_FLOOR: u64 = 32 * MIB;
/// Reads costing at most this draw from the light pool; the rest from the heavy pool (STO-4).
pub const LIGHT_CLASS_MAX_COST: u64 = 2 * COST_FLOOR;
/// The ceiling is this many tenths of the cost (15 = 1.5 ×).
pub const CEILING_TENTHS: u64 = 15;
/// The most entries a store walk visits before refusing.
pub const MAX_STORE_ENTRIES: usize = 100_000;
/// The most bytes of a JSON report the server keeps from one call; past it the call is stopped and
/// reported as exceeding its output bound (`cat` is bounded by the caller's `max_bytes` instead).
pub const MAX_REPORT_BYTES: u64 = 64 * MIB;
/// The prikk version this table is bound to: the floor.
pub const BOUND_TO: (u64, u64, u64) = (0, 54, 0);
/// `false` since spike A measured the multiples on the floor (2026-10-27); `true` again whenever a
/// re-baseline moves the floor before the table is re-measured.
pub const PROVISIONAL: bool = false;

// A provisional table never reaches a release: this fails to compile until the floor is re-measured.
const _: () = assert!(
    !PROVISIONAL,
    "re-measure the cost table on the floor (spike A) before shipping"
);

/// A read verb of this driver, for costing. Matched exhaustively: a new verb must be given a cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadVerb {
    /// `branch list`.
    Branch,
    /// `tree`.
    Tree,
    /// `cat` (metadata or bytes).
    Cat,
    /// `log`.
    Log,
    /// `show`.
    Show,
    /// `diff`.
    Diff,
    /// `extract` (and its plan).
    Extract,
    /// `verify`.
    Verify,
    /// `status` (the unsealed queue, the interrupted states; recovery's witness).
    Status,
}

/// How a verb's cost is derived: `max(floor, S × store_tenths / 10)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostRule {
    /// The multiple of the store size, in tenths.
    pub store_tenths: u64,
    /// The smallest cost.
    pub floor: u64,
}

impl ReadVerb {
    /// Every verb.
    pub const ALL: [ReadVerb; 9] = [
        ReadVerb::Branch,
        ReadVerb::Tree,
        ReadVerb::Cat,
        ReadVerb::Log,
        ReadVerb::Show,
        ReadVerb::Diff,
        ReadVerb::Extract,
        ReadVerb::Verify,
        ReadVerb::Status,
    ];

    /// The prikk subcommand.
    pub const fn name(self) -> &'static str {
        match self {
            ReadVerb::Branch => "branch",
            ReadVerb::Tree => "tree",
            ReadVerb::Cat => "cat",
            ReadVerb::Log => "log",
            ReadVerb::Show => "show",
            ReadVerb::Diff => "diff",
            ReadVerb::Extract => "extract",
            ReadVerb::Verify => "verify",
            ReadVerb::Status => "status",
        }
    }

    /// The one table.
    pub const fn rule(self) -> CostRule {
        let (store_tenths, floor) = match self {
            // Measured flat: 7 MiB of data at every size. `status` reads the active WAL's records and nothing of the
            // store: 2 MiB on a 3,000-page repository and on an empty one (prikk 0.55.0, the smallest working
            // `RLIMIT_DATA`), so it is flat at the floor.
            ReadVerb::Branch | ReadVerb::Log | ReadVerb::Status => (0, COST_FLOOR),
            // Measured 1.0 × S on many small pages; 2.1 × S is the single-large-file worst case (planeter).
            ReadVerb::Tree | ReadVerb::Cat => (21, COST_FLOOR),
            // Measured flat for a small patch; follows the patch's content, up to 3.6 × S for large text.
            ReadVerb::Show => (36, SHOW_FLOOR),
            // Measured 4.6 × S for a whole-history diff at 212 MiB (two replays and the hunks); 5.0 with margin.
            ReadVerb::Diff => (50, COST_FLOOR),
            // Measured 0.9 × S; kept at the tree worst case plus the files written.
            ReadVerb::Extract => (31, COST_FLOOR),
            // Measured 2.0 × S on many small pages; 4.1 × S is the single-large-file worst case (planeter).
            ReadVerb::Verify => (41, COST_FLOOR),
        };
        CostRule {
            store_tenths,
            floor,
        }
    }

    /// The estimated peak memory of this read against a store of `store_bytes`.
    pub fn cost(self, store_bytes: u64) -> u64 {
        let r = self.rule();
        let tenths = u128::from(store_bytes) * u128::from(r.store_tenths);
        u64::try_from(tenths / 10).unwrap_or(u64::MAX).max(r.floor)
    }

    /// Does a read of this verb at this cost belong to the light class?
    pub fn is_light(cost: u64) -> bool {
        cost <= LIGHT_CLASS_MAX_COST
    }

    /// The server's wall-time bound for this verb, in seconds.
    pub const fn deadline_secs(self) -> u64 {
        match self {
            ReadVerb::Branch
            | ReadVerb::Log
            | ReadVerb::Cat
            | ReadVerb::Tree
            | ReadVerb::Show
            | ReadVerb::Status => 30,
            ReadVerb::Diff | ReadVerb::Extract => 120,
            ReadVerb::Verify => 300,
        }
    }
}

/// The memory ceiling for a call of `cost`: `cost × 1.5` (saturating). The ceiling, not the cost, is what
/// the admission gate is charged, so the headroom is real and the pool's arithmetic matches `RLIMIT_DATA`.
pub fn ceiling_for(cost: u64) -> u64 {
    u64::try_from(u128::from(cost) * u128::from(CEILING_TENTHS) / 10).unwrap_or(u64::MAX)
}

/// The inverse of [`ceiling_for`]: the cost a ceiling was derived from (for classing a call).
pub fn cost_of_ceiling(ceiling: u64) -> u64 {
    u64::try_from(u128::from(ceiling) * 10 / u128::from(CEILING_TENTHS)).unwrap_or(u64::MAX)
}

/// Why the store could not be sized.
#[derive(Debug)]
pub enum StoreSizeError {
    /// `.prikk` is missing: not a repository.
    NoRepository,
    /// A directory or file could not be read.
    Io(std::io::Error),
    /// More than [`MAX_STORE_ENTRIES`] entries.
    TooManyEntries,
}

impl std::fmt::Display for StoreSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreSizeError::NoRepository => {
                write!(f, "no prikk repository (no `.prikk` directory)")
            }
            StoreSizeError::Io(e) => write!(f, "the repository store could not be measured: {e}"),
            StoreSizeError::TooManyEntries => write!(
                f,
                "the repository store has more than {MAX_STORE_ENTRIES} entries"
            ),
        }
    }
}

impl std::error::Error for StoreSizeError {}

/// The store size *S*: the apparent bytes of every regular file under `<repo>/.prikk`, by a fresh walk;
/// `lstat` only, symlinks never followed, no cache. This is the truth; [`SizeHints`] is what a caller who
/// already walked may hand the driver so that it does not walk again.
pub fn store_size(repo: &Path) -> Result<u64, StoreSizeError> {
    store_size_limited(repo, MAX_STORE_ENTRIES)
}

/// [`store_size`] with an explicit entry bound.
pub fn store_size_limited(repo: &Path, max_entries: usize) -> Result<u64, StoreSizeError> {
    let store = repo.join(".prikk");
    if !store.is_dir() {
        return Err(StoreSizeError::NoRepository);
    }
    let mut total: u64 = 0;
    let mut seen: usize = 0;
    let mut stack = vec![store];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(StoreSizeError::Io)? {
            let entry = entry.map_err(StoreSizeError::Io)?;
            seen += 1;
            if seen > max_entries {
                return Err(StoreSizeError::TooManyEntries);
            }
            let md = std::fs::symlink_metadata(entry.path()).map_err(StoreSizeError::Io)?;
            if md.file_type().is_dir() {
                stack.push(entry.path());
            } else if md.file_type().is_file() {
                total = total.saturating_add(md.len());
            }
        }
    }
    Ok(total)
}

/// Recent figures for *S*, shared between the driver and whoever knows when a repository changed (the read
/// service, which keys its figure by the view tips; RFC 003 D-2, handoff 003 T3).
///
/// The driver walks `.prikk` on every call whose cost follows *S* (`tree`, `cat`, `show`, `diff`, `extract`,
/// `verify`): up to [`MAX_STORE_ENTRIES`] `lstat`s each, which on a large wiki costs more than a cheap read.
/// With a hint for the repository the walk is skipped; **without one, or once the hint is older than its
/// maximum age, the walk is the fallback and the truth**. A hint is a figure for the cost estimate and never
/// a statement that the repository exists: the driver still checks that `.prikk` is a directory. The handle
/// is cheap to clone and every clone sees the same figures.
///
/// A holder that learns the repository changed (a write, a moved view) calls [`SizeHints::forget`] or sets a
/// fresh figure; the maximum age bounds the damage of a change nobody announced.
#[derive(Debug, Clone)]
pub struct SizeHints {
    figures: Arc<Mutex<HashMap<PathBuf, (Instant, u64)>>>,
    max_age: Duration,
}

impl SizeHints {
    /// How long a figure is trusted when the holder does not say otherwise.
    pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(15 * 60);

    /// An empty set of hints with [`SizeHints::DEFAULT_MAX_AGE`].
    pub fn new() -> Self {
        Self::with_max_age(Self::DEFAULT_MAX_AGE)
    }

    /// An empty set of hints whose figures are trusted for `max_age`.
    pub fn with_max_age(max_age: Duration) -> Self {
        SizeHints {
            figures: Arc::new(Mutex::new(HashMap::new())),
            max_age,
        }
    }

    /// Record that the store of `repo` measured `bytes` just now.
    pub fn set(&self, repo: &Path, bytes: u64) {
        self.figures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(repo.to_path_buf(), (Instant::now(), bytes));
    }

    /// The figure for `repo`, unless there is none or it is older than the maximum age.
    pub fn get(&self, repo: &Path) -> Option<u64> {
        let figures = self.figures.lock().unwrap_or_else(PoisonError::into_inner);
        figures
            .get(repo)
            .filter(|(at, _)| at.elapsed() <= self.max_age)
            .map(|(_, bytes)| *bytes)
    }

    /// Drop the figure for `repo`: the next call walks (the truth after a write).
    pub fn forget(&self, repo: &Path) {
        self.figures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(repo);
    }
}

impl Default for SizeHints {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
