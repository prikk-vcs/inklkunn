//! Local passwords (RFC 002 D-1): **Argon2id** in the PHC string format, with a salt from the operating
//! system. Verification reads the cost from the stored string, so raising the cost later leaves every old
//! hash verifiable; it also refuses a string whose cost is above a ceiling, so a damaged or hostile row
//! cannot make a verification allocate gigabytes.
//!
//! Passwords are normalised to NFKC before they are hashed or checked (NIST SP 800-63B §5.1.1.2), so the same
//! password typed on two devices or two input methods, a full-width letter or a composed and a decomposed
//! accent, is the same password.

use argon2::password_hash::{
    PasswordHash as Phc, PasswordHasher as _, PasswordVerifier as _, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};
use unicode_normalization::UnicodeNormalization as _;

use crate::error::AuthError;
use crate::random;

/// The cost new hashes are made with: 19 MiB, two passes, one lane (the OWASP baseline for Argon2id).
pub const MEMORY_KIB: u32 = 19 * 1024;
/// See [`MEMORY_KIB`].
pub const PASSES: u32 = 2;
/// See [`MEMORY_KIB`].
pub const LANES: u32 = 1;

/// The most a stored hash may ask of one verification: 64 MiB, six passes, four lanes.
const CEILING: (u32, u32, u32) = (64 * 1024, 6, 4);

/// Hashing and checking a password. A trait so a test can count the work done; the one implementation a
/// product uses is [`Argon2idHasher`].
pub trait PasswordHasher: Send + Sync {
    /// A PHC string for `password`.
    fn hash(&self, password: &str) -> Result<String, AuthError>;
    /// Can `stored` be verified at all: a well-formed Argon2id string whose cost is within the ceiling? A
    /// caller that holds a string for which this is `false` verifies against a dummy hash instead, so that a
    /// row nobody can verify costs what any other does (constant work).
    fn is_verifiable(&self, stored: &str) -> bool;
    /// Does `password` match the stored PHC string? A string that cannot be verified is a mismatch.
    fn verify(&self, password: &str, stored: &str) -> bool;
}

/// Argon2id.
#[derive(Debug, Clone)]
pub struct Argon2idHasher {
    /// The cost new hashes are made at; `None` only if the constants were ever made invalid, in which case
    /// [`PasswordHasher::hash`] says so instead of hashing at some other cost.
    params: Option<Params>,
}

impl Argon2idHasher {
    /// The production cost ([`MEMORY_KIB`], [`PASSES`], [`LANES`]).
    pub fn new() -> Self {
        Argon2idHasher {
            params: Params::new(MEMORY_KIB, PASSES, LANES, None).ok(),
        }
    }

    /// A cheaper cost, for a test that hashes many passwords. Never available to a product build.
    #[cfg(any(test, feature = "test-knobs"))]
    pub fn with_cost(memory_kib: u32, passes: u32, lanes: u32) -> Result<Self, AuthError> {
        Params::new(memory_kib, passes, lanes, None)
            .map(|params| Argon2idHasher {
                params: Some(params),
            })
            .map_err(|_| AuthError::Misconfigured("those Argon2 costs are outside Argon2's limits"))
    }

    /// The `m=…,t=…,p=…` text of the cost new hashes are made at, for a test that compares costs.
    #[cfg(any(test, feature = "test-knobs"))]
    pub fn cost_text(&self) -> Option<String> {
        self.params
            .as_ref()
            .map(|p| format!("m={},t={},p={}", p.m_cost(), p.t_cost(), p.p_cost()))
    }
}

impl Default for Argon2idHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// The text that is hashed: the password, NFKC-normalised.
fn prepared(password: &str) -> String {
    password.nfkc().collect()
}

/// A stored string that can be verified: Argon2id, well formed, within the ceiling. Its parameters are the
/// ones it carries.
fn inspect(stored: &str) -> Option<(Phc<'_>, Params)> {
    let phc = Phc::new(stored).ok()?;
    if phc.algorithm.as_str() != "argon2id" {
        return None;
    }
    let params = Params::try_from(&phc).ok()?;
    if params.m_cost() > CEILING.0 || params.t_cost() > CEILING.1 || params.p_cost() > CEILING.2 {
        return None;
    }
    Some((phc, params))
}

impl PasswordHasher for Argon2idHasher {
    fn hash(&self, password: &str) -> Result<String, AuthError> {
        let params = self.params.clone().ok_or(AuthError::Misconfigured(
            "the password hashing costs are outside Argon2's limits",
        ))?;
        let salt = SaltString::encode_b64(&random::bytes::<16>()?)
            .map_err(|_| AuthError::Backend("a salt could not be made".to_owned()))?;
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password(prepared(password).as_bytes(), &salt)
            .map(|phc| phc.to_string())
            .map_err(|_| AuthError::Backend("a password could not be hashed".to_owned()))
    }

    fn is_verifiable(&self, stored: &str) -> bool {
        inspect(stored).is_some()
    }

    fn verify(&self, password: &str, stored: &str) -> bool {
        let Some((phc, params)) = inspect(stored) else {
            return false;
        };
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .verify_password(prepared(password).as_bytes(), &phc)
            .is_ok()
    }
}

/// Why a new password was refused. The sign-in path never says this: it says *bad credentials*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordRefusal {
    /// Fewer than [`PASSWORD_MIN_CHARS`] characters.
    TooShort,
    /// More than [`PASSWORD_MAX_BYTES`] bytes.
    TooLong,
}

impl std::fmt::Display for PasswordRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasswordRefusal::TooShort => {
                write!(f, "a password has at least {PASSWORD_MIN_CHARS} characters")
            }
            PasswordRefusal::TooLong => {
                write!(f, "a password has at most {PASSWORD_MAX_BYTES} bytes")
            }
        }
    }
}

impl std::error::Error for PasswordRefusal {}

/// The shortest new password, in characters. There is no rule about which characters (no composition rules:
/// NIST SP 800-63B), and pasting is allowed.
pub const PASSWORD_MIN_CHARS: usize = 10;
/// The longest password, in bytes: the bound on the work a request can ask for.
pub const PASSWORD_MAX_BYTES: usize = 1024;

/// Is this an acceptable new password?
pub fn check_new_password(password: &str) -> Result<(), PasswordRefusal> {
    if password.len() > PASSWORD_MAX_BYTES {
        return Err(PasswordRefusal::TooLong);
    }
    if prepared(password).chars().count() < PASSWORD_MIN_CHARS {
        return Err(PasswordRefusal::TooShort);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
