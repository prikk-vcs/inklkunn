#![forbid(unsafe_code)]
//! `inklkunn-prikk` — the storage boundary: the `Storage` trait in inklkunn's words and its first
//! implementation, a sandboxed, version-pinned prikk CLI driver for the **read** verbs with typed
//! `--format json` output (RFC 001 D-3, D-4).
//!
//! This crate never names a write verb; the boundary gate (`tools/boundary-check`) enforces it. Writes
//! live in `inklkunn-signer`.

/// The prikk floor (RFC 001 D-3 / RFC 010 D-3): the oldest prikk the driver accepts. One value with the
/// CI workflow and the container recipe; `tools/floor-check` fails when they differ.
///
/// `0.54.0` carries what M1 needs: `extract` (with the claim's parent), `diff`, `tree`, `cat`, the
/// `log`/`show` JSON, and the `show <block-id>` fix.
pub const MIN_PRIKK_VERSION: &str = "0.54.0";
