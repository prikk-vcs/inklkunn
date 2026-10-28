//! `Accept`, the one way into a wiki's history (RFC 002 D-4), as `inklkunn-core` offers it: the trait, the
//! types it speaks and the ledger it reads are **defined in `inklkunn-signer`**, the crate that implements
//! and enforces them (a lower crate may not depend on core, and the internal design calls this "the
//! signer's trait"), and are re-exported here so that callers name them through core, as the web tier does.

pub use inklkunn_signer::accept::*;
