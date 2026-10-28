//! Randomness for secrets: the operating system's, read from `/dev/urandom`, **with no fallback**. A weak
//! source would silently weaken every session id, token, salt and nonce, so a failure to read is a failure
//! to proceed. (This is the same contract as the signer's; the two are separate because the signer's module
//! is private by design.)

use std::io::Read;

use crate::error::AuthError;

/// Fill `buf` from the operating system.
pub(crate) fn fill(buf: &mut [u8]) -> Result<(), AuthError> {
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .map_err(|_| AuthError::Backend("the system's random source could not be read".to_owned()))
}

/// `N` random bytes.
pub(crate) fn bytes<const N: usize>() -> Result<[u8; N], AuthError> {
    let mut b = [0u8; N];
    fill(&mut b)?;
    Ok(b)
}

/// 32 random bytes as unpadded base64url: 256 bits, 43 characters. The form of a session id, a CSRF token,
/// an OpenID Connect state, nonce and PKCE verifier.
pub(crate) fn token() -> Result<String, AuthError> {
    use base64ct::{Base64UrlUnpadded, Encoding as _};
    Ok(Base64UrlUnpadded::encode_string(&bytes::<32>()?))
}
