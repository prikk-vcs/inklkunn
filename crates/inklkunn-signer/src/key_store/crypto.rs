//! The two primitives the key store stands on (RFC 002 D-3), and nothing else:
//!
//! - **Argon2id** (RFC 9106) turns the operator's secret into a 256-bit master key once, at start. It is
//!   memory-hard on purpose and costs a fraction of a second and tens of megabytes, which is why it is paid
//!   once per process and not once per signature.
//! - **XChaCha20-Poly1305** seals each record. A 192-bit random nonce per record makes a repeat
//!   impossible in practice under one key, and the associated data binds the record's plain fields (wiki,
//!   key ids, generation, times) to the ciphertext so that none can be changed or moved to another wiki.
//!
//! The master key lives in a [`Zeroizing`] buffer and is never printed, cloned or exposed.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use super::random;

/// The master and record key length, in bytes.
pub(super) const KEY_LEN: usize = 32;
/// The XChaCha20 nonce length, in bytes.
pub(super) const NONCE_LEN: usize = 24;
/// The Argon2 salt length, in bytes.
pub(super) const SALT_LEN: usize = 16;

/// The cost parameters of the key derivation, recorded in the store header so that they can be raised later
/// without making an old store unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KdfParams {
    /// Memory, in KiB.
    pub(super) m_cost_kib: u32,
    /// Passes over that memory.
    pub(super) t_cost: u32,
    /// Lanes.
    pub(super) p_cost: u32,
}

impl KdfParams {
    /// 64 MiB, three passes, one lane: RFC 9106's second recommended option at a single lane. Measured on
    /// the development host: about 56 ms in a release build (1 s unoptimized), paid once when the store is
    /// opened. The costs are recorded in the store's header, so a store keeps the ones it was made with.
    pub(super) const DEFAULT: KdfParams = KdfParams {
        m_cost_kib: 64 * 1024,
        t_cost: 3,
        p_cost: 1,
    };

    /// Bounds a header may carry. A store's header is the operator's, but a derivation whose cost a file can
    /// set must not be able to ask for gigabytes or minutes.
    pub(super) fn check(self) -> Result<Self, CryptoError> {
        let ok = (8..=1 << 20).contains(&self.m_cost_kib)
            && (1..=10).contains(&self.t_cost)
            && (1..=8).contains(&self.p_cost);
        if ok {
            Ok(self)
        } else {
            Err(CryptoError::Params)
        }
    }
}

/// Why a primitive could not do its work. No variant carries key material, plaintext or ciphertext.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CryptoError {
    /// The cost parameters are outside the bounds.
    Params,
    /// The derivation failed.
    Derivation,
    /// The system's randomness could not be read.
    Random,
    /// Sealing failed.
    Seal,
    /// A sealed record did not authenticate: the wrong key, or changed bytes.
    Authentication,
}

/// The key that seals and opens records. Derived from the operator's secret; never leaves this module.
pub(super) struct MasterKey(Zeroizing<[u8; KEY_LEN]>);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(..)")
    }
}

impl MasterKey {
    /// Derive the master key from `secret` and the store's `salt`.
    pub(super) fn derive(
        secret: &[u8],
        salt: &[u8; SALT_LEN],
        params: KdfParams,
    ) -> Result<Self, CryptoError> {
        let p = params.check()?;
        let argon_params = Params::new(p.m_cost_kib, p.t_cost, p.p_cost, Some(KEY_LEN))
            .map_err(|_| CryptoError::Params)?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
        let mut out = Zeroizing::new([0u8; KEY_LEN]);
        argon
            .hash_password_into(secret, salt, out.as_mut())
            .map_err(|_| CryptoError::Derivation)?;
        Ok(MasterKey(out))
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305, CryptoError> {
        XChaCha20Poly1305::new_from_slice(self.0.as_ref()).map_err(|_| CryptoError::Seal)
    }

    /// Seal `plaintext` under a fresh random nonce, binding `aad`. Returns the nonce and the ciphertext
    /// (which carries the authentication tag).
    pub(super) fn seal(
        &self,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<([u8; NONCE_LEN], Vec<u8>), CryptoError> {
        let nonce = random::bytes::<NONCE_LEN>().map_err(|_| CryptoError::Random)?;
        let sealed = self
            .cipher()?
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| CryptoError::Seal)?;
        Ok((nonce, sealed))
    }

    /// Open a sealed record, checking `aad`. Anything wrong, whether the key, the nonce, the associated data
    /// or a single changed byte, is the same [`CryptoError::Authentication`]: the answer never says which.
    pub(super) fn open(
        &self,
        nonce: &[u8; NONCE_LEN],
        sealed: &[u8],
        aad: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        self.cipher()?
            .decrypt(&XNonce::from(*nonce), Payload { msg: sealed, aad })
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::Authentication)
    }
}
