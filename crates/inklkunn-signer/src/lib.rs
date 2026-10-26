#![forbid(unsafe_code)]
//! `inklkunn-signer` — the signer: the per-wiki key store and **the only path to prikk's write verbs**
//! (`init`, `trust`, `commit`, `seal`, `branch create`). Nothing else reaches key material; the
//! key-reachability gate (`tools/boundary-check`) lets only `inklkunn-core` depend on this crate and keeps
//! `key_store` private.
//!
//! **M0 (RFC 001 D-9) holds the shape, not the real thing.** `key_store` is empty so the gate has a
//! target; [`BootstrapSigner`] is the write sequence over a **plain-text key directory** for tests and the
//! first wiki on a developer's machine. RFC 002 D-3 replaces the directory with encrypted records that are
//! decrypted into a private tmpfs for one call, and D-4 adds the acceptance guard in front of the sequence.
//! The sequence itself ([`writes`]) is what RFC 002 keeps.

mod key_store;

pub mod bootstrap;
pub mod keys;
pub mod writes;

pub use bootstrap::BootstrapSigner;
pub use keys::{KeyDir, KeyIds};
pub use writes::{SealedIds, SignerError};
