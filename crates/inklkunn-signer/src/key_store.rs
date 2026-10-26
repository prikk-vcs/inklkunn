//! The per-wiki key store (RFC 002 D-3): one record per wiki, the author and maintainer seeds encrypted
//! under a key derived from the operator's secret, decrypted into a private tmpfs for one call.
//!
//! Empty in M0 (RFC 001 D-9): the key-reachability gate guards this module's name, so the module must
//! exist before the gate means anything. RFC 002's handoff fills it.
