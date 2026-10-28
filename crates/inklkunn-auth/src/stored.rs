//! The stored form of a secret that is already random (a session id, an API token): its SHA-256, so a copy
//! of the database opens no session and reveals no token. A password is different (a person chose it, so it
//! gets Argon2id: `hashing`); these are 256 random bits, and a fast hash costs an attacker nothing they did
//! not already have to guess.

use std::fmt::Write as _;

use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

/// `sha256$<64 lower-case hex digits>` of `secret`.
pub(crate) fn stored_form(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256$");
    for b in digest {
        // Writing to a String cannot fail.
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Equality that does not stop at the first differing byte, for CSRF tokens and OpenID Connect nonces. Texts
/// of different lengths are different (the length of a random token is not a secret).
pub(crate) fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}
