#![forbid(unsafe_code)]
//! `inklkunn-signer` — inklkunn's signer: the per-wiki key store and the only path to prikk's write verbs (commit, seal, branch create). Nothing else reaches key material.
//!
//! RFC 001 D-1 places this crate in the workspace; the task that fills it names its decisions.
