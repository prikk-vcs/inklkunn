//! The memory a signer call is allowed (threat model C-9a; corrections handoff C1). Every prikk call the signer
//! makes runs under `RLIMIT_DATA`, and the limit is **derived from the repository's size and the verb**, the
//! way the read driver's cost table does it: `ceiling = 1.5 × max(floor, S × multiple)`, where *S* is the
//! store size (a fresh walk of `.prikk`, the truth at the moment of the call). A fixed 1 GiB, which the first
//! version of this crate used, is both too much for a flat verb and too little for `verify` of a store holding
//! one 300 MiB attachment.
//!
//! **Provenance of the multiples** (2026-10-29, prikk 0.55.0 installed, the floor 0.54.0 re-baselined to it:
//! the read multiples did not move; the smallest working `RLIMIT_DATA` by bisection to 256 KiB over a fresh copy
//! of the repository per probe, the method of spike A, on wiki-shaped stores of 3.6, 18.6, 76 and 212 MiB (1,000
//! and 10,000 pages) and single-attachment stores of 64 and 200 MiB, plus a change that *adds* a 64 or 200 MiB
//! file to a small wiki): `init`, `key`, `trust`, `status`, `log`, `mv` and `branch create` need 4 MiB or less at
//! every size; `worktree-status` 28 MiB on 10,000 pages and 5.0 × S for one attachment; `verify` 2.0 × S, 5.0 ×
//! S for one attachment; `branch switch` 2.0 × S, 6.0 × S for one attachment; `commit` 23 MiB on 10,000 pages and
//! **7.0 × the bytes of a file it adds** (452 MiB for 64, 1,404 MiB for 200); `seal` 13 MiB on 10,000 pages, 2.0 × S
//! when a large file sits in an earlier block, and 2.0 × the bytes it seals; and a `commit` right after a large file was written reads it again (prikk cannot
//! trust a timestamp from the same second): 5.0 × S for one attachment (found by the 300 MiB acceptance test, not
//! by the first measurement). The rules round up (the commit's multiple is 8) and the ceiling is 1.5 × the cost.
//! Every rule carries the prikk version it is bound to ([`BOUND_TO`]); a test fails when
//! that is not the floor, so the table is revisited at every re-baseline (RFC 010).
//!
//! **The keyless writable mode's two verbs** (handoff 003 T7; measured 2026-10-29 on prikk 0.55.0 by the same
//! method, on a copy per probe: wiki stores of 3.0, 29.4 and 86.6 MiB (1,000 and 10,000 pages) and single-attachment
//! stores of 64 and 200 MiB): `archive export` needs 0.7, 3.5, 3.5, 0.4 and 0.4 MiB, so it is a flat 16 MiB cost
//! (ceiling 24 MiB); a `verify` with the repository writable (the checkpoint refresh) needs 2.09, 2.02 and 2.01 × S on
//! the wiki stores and 5.01 and 5.00 × S for one attachment, the same as the read-only `verify`, so it runs under the
//! existing `verify` rule.

use std::path::Path;

use inklkunn_prikk::cost::{MIB, ceiling_for, store_size};

use crate::writes::SignerError;

/// The prikk version this table is bound to: the floor.
#[cfg(test)]
pub(crate) const BOUND_TO: (u64, u64, u64) = (0, 54, 0);

/// How a verb's cost is derived: `max(floor, S × store_tenths / 10 + change × change_tenths / 10)`, where *S* is
/// the store size and *change* the bytes of the files an acceptance writes (`commit` and `seal` read them whole).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WriteRule {
    /// The multiple of the store size, in tenths.
    pub(crate) store_tenths: u64,
    /// The multiple of the change's bytes, in tenths.
    pub(crate) change_tenths: u64,
    /// The smallest cost.
    pub(crate) floor: u64,
}

/// A prikk verb the signer runs. Matched exhaustively: a new verb must be given a cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WriteVerb {
    /// `init`.
    Init,
    /// `trust maintainer add` / `remove`.
    Trust,
    /// `key generate`.
    KeyGenerate,
    /// `key public`.
    KeyPublic,
    /// `status`.
    Status,
    /// `worktree-status`.
    WorktreeStatus,
    /// `log`.
    Log,
    /// `verify`.
    Verify,
    /// `branch switch`.
    Switch,
    /// `mv`.
    Mv,
    /// `commit`.
    Commit,
    /// `seal`.
    Seal,
    /// `branch create`.
    BranchCreate,
    /// `branch list`.
    BranchList,
    /// `archive export` (keyless writable mode; handoff 003 T7, IS-8).
    ArchiveExport,
}

impl WriteVerb {
    /// Every verb.
    #[cfg(test)]
    pub(crate) const ALL: [WriteVerb; 15] = [
        WriteVerb::Init,
        WriteVerb::Trust,
        WriteVerb::KeyGenerate,
        WriteVerb::KeyPublic,
        WriteVerb::Status,
        WriteVerb::WorktreeStatus,
        WriteVerb::Log,
        WriteVerb::Verify,
        WriteVerb::Switch,
        WriteVerb::Mv,
        WriteVerb::Commit,
        WriteVerb::Seal,
        WriteVerb::BranchCreate,
        WriteVerb::BranchList,
        WriteVerb::ArchiveExport,
    ];

    /// The verb a prikk argument vector runs, or `None` for one the signer has no cost for (and so never runs).
    pub(crate) fn of(args: &[&str]) -> Option<WriteVerb> {
        Some(match args {
            ["init", ..] => WriteVerb::Init,
            ["trust", ..] => WriteVerb::Trust,
            ["key", "generate", ..] => WriteVerb::KeyGenerate,
            ["key", "public", ..] => WriteVerb::KeyPublic,
            ["status", ..] => WriteVerb::Status,
            ["worktree-status", ..] => WriteVerb::WorktreeStatus,
            ["log", ..] => WriteVerb::Log,
            ["verify", ..] => WriteVerb::Verify,
            ["branch", "switch", ..] => WriteVerb::Switch,
            ["branch", "create", ..] => WriteVerb::BranchCreate,
            ["branch", "list", ..] => WriteVerb::BranchList,
            ["archive", "export", ..] => WriteVerb::ArchiveExport,
            ["mv", ..] => WriteVerb::Mv,
            ["commit", ..] => WriteVerb::Commit,
            ["seal", ..] => WriteVerb::Seal,
            _ => return None,
        })
    }

    /// The one table (see the module documentation for the provenance of every number).
    pub(crate) const fn rule(self) -> WriteRule {
        let (store_tenths, change_tenths, floor) = match self {
            // Measured flat, 4 MiB or less, at every size and shape.
            WriteVerb::Init
            | WriteVerb::KeyGenerate
            | WriteVerb::KeyPublic
            | WriteVerb::Trust
            | WriteVerb::Status
            | WriteVerb::Log
            | WriteVerb::Mv
            | WriteVerb::BranchCreate
            | WriteVerb::BranchList => (0, 0, 16 * MIB),
            // 28 MiB for 10,000 pages; 5.0 × S where one attachment is most of the store.
            WriteVerb::WorktreeStatus => (50, 0, 48 * MIB),
            // 2.0 × S on many pages; 5.0 × S for one attachment (IS-12's 4.99).
            WriteVerb::Verify => (50, 0, 32 * MIB),
            // 2.0 × S on many pages; 6.0 × S for one attachment; 22 MiB flat on a small store.
            WriteVerb::Switch => (60, 0, 48 * MIB),
            // 23 MiB for 10,000 pages whatever S is; 5.0 × S when a large file was written just before (prikk
            // reads a file modified in the same second again); 7.1 × the bytes of a file it adds.
            WriteVerb::Commit => (50, 80, 48 * MIB),
            // 13 MiB for 10,000 pages; 2.0 × S once a large file is in an earlier block; 2.1 × the bytes of a
            // file it seals.
            WriteVerb::Seal => (30, 30, 32 * MIB),
            // 0.4 MiB for one 64 or 200 MiB attachment, 0.7 MiB at 3 MiB, 3.5 MiB at 29 and 87 MiB (10,000
            // pages): it streams, and its need does not follow S. Flat, at four times the largest need.
            WriteVerb::ArchiveExport => (0, 0, 16 * MIB),
        };
        WriteRule {
            store_tenths,
            change_tenths,
            floor,
        }
    }

    /// The estimated peak memory of this verb against a store of `store_bytes` and a change of `change_bytes`.
    pub(crate) fn cost(self, store_bytes: u64, change_bytes: u64) -> u64 {
        let r = self.rule();
        let tenths = u128::from(store_bytes) * u128::from(r.store_tenths)
            + u128::from(change_bytes) * u128::from(r.change_tenths);
        u64::try_from(tenths / 10).unwrap_or(u64::MAX).max(r.floor)
    }

    /// The memory ceiling for this verb.
    pub(crate) fn ceiling(self, store_bytes: u64, change_bytes: u64) -> u64 {
        ceiling_for(self.cost(store_bytes, change_bytes))
    }
}

/// The ceiling for `verb` run in `repo` (a fresh walk of the store; `None` for a verb that has no repository),
/// for a change of `change_bytes` (0 for a verb that does not read the change).
pub(crate) fn ceiling_in(
    verb: WriteVerb,
    repo: Option<&Path>,
    change_bytes: u64,
) -> Result<u64, SignerError> {
    let size = match repo {
        // `init` makes the store: there is nothing to walk yet, and the rule is flat.
        Some(repo) if verb != WriteVerb::Init => {
            store_size(repo).map_err(|e| SignerError::Unavailable(e.to_string()))?
        }
        _ => 0,
    };
    Ok(verb.ceiling(size, change_bytes))
}

#[cfg(test)]
mod tests;
