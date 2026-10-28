//! ID-token verification: the [`TokenVerifier`] seam and the one real implementation, over `jsonwebtoken`'s
//! `aws_lc_rs` backend (the owner's ruling DR-9). Nothing here is home-grown cryptography.
//!
//! What is checked, in this order; each failure is [`OidcError::Invalid`] with the one [`InvalidReason`] that
//! says which (the person sees "sign-in failed"; the log gets the reason):
//! the algorithm is asymmetric and on the list (never `none`, never `HS*`, which would let a public key be
//! used as an HMAC secret); the token names a `kid` and the provider's key set has it; the key's declared
//! algorithm (if it declares one) is the header's; the signature verifies; `iss` is exactly the configured
//! issuer; `aud` contains the client id (and `azp` is the client id when there is more than one audience, or
//! when it is present); `exp` is in the future, `nbf` and `iat` are not, and `iat` is present and no more than
//! fifteen minutes old, within 60 seconds of clock skew, **by the `now` the caller passes** (so a test needs
//! no clock); the `nonce` is the one this login made. The key must be one the provider published for
//! signatures. The algorithms are the six that cannot be confused with a shared secret: RS256, RS384, RS512,
//! ES256, ES384 and EdDSA.

use std::sync::Arc;

use inklkunn_store::ExternalIdentity;
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::jwk::PublicKeyUse;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;

use super::remote::Remote;
use super::{InvalidReason, OidcError};
use crate::stored::same_secret;

/// What a verified ID token asserts: who, and nothing else that anything relies on. **It cannot be built
/// outside this crate** (only a verifier that checked a token makes one), so no code can sign in as an
/// identity it did not verify; a test builds one with `VerifiedIdentity::for_test`, which only the `test-knobs` feature has.
#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    identity: ExternalIdentity,
    email_hint: Option<String>,
}

impl std::fmt::Debug for VerifiedIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The e-mail address is personal data and only a hint: it is not printed.
        f.debug_struct("VerifiedIdentity")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl VerifiedIdentity {
    pub(crate) fn new(identity: ExternalIdentity, email_hint: Option<String>) -> Self {
        VerifiedIdentity {
            identity,
            email_hint,
        }
    }

    /// For a test double of [`TokenVerifier`] and for tests that sign in without a provider.
    #[cfg(any(test, feature = "test-knobs"))]
    pub fn for_test(identity: ExternalIdentity, email_hint: Option<String>) -> Self {
        Self::new(identity, email_hint)
    }

    /// The issuer and the provider's subject. The only thing an account is linked by.
    pub fn identity(&self) -> &ExternalIdentity {
        &self.identity
    }

    /// The e-mail address the provider reported, if any, cleaned. **A hint for an administrator choosing
    /// which account to link; never an identity, never a reason to link or sign in.**
    pub fn email_hint(&self) -> Option<&str> {
        self.email_hint.as_deref()
    }
}

/// Verify an ID token for one login.
pub trait TokenVerifier: Send + Sync {
    /// Verify `id_token` at `now`; its nonce must be `expected_nonce`.
    fn verify(
        &self,
        id_token: &str,
        expected_nonce: &str,
        now: i64,
    ) -> Result<VerifiedIdentity, OidcError>;
}

/// Allowed clock skew, in seconds.
const LEEWAY_SECS: i64 = 60;
/// The oldest an ID token may be when it is presented, in seconds: a sign-in is minutes long.
const MAX_AGE_SECS: i64 = 15 * 60;

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    /// How many audiences the token names (the library has already checked that ours is among them).
    fn count(&self) -> usize {
        match self {
            Audience::One(a) => usize::from(!a.is_empty()),
            Audience::Many(all) => all.len(),
        }
    }
}

#[derive(Deserialize)]
struct IdClaims {
    iss: String,
    sub: String,
    aud: Audience,
    exp: i64,
    #[serde(default)]
    nbf: Option<i64>,
    #[serde(default)]
    iat: Option<i64>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

/// The real verifier: signatures against the provider's published keys.
pub(super) struct JwtVerifier {
    remote: Arc<Remote>,
    client_id: String,
}

impl JwtVerifier {
    pub(super) fn new(remote: Arc<Remote>, client_id: String) -> Self {
        JwtVerifier { remote, client_id }
    }
}

fn allowed(alg: Algorithm) -> bool {
    matches!(
        alg,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::ES256
            | Algorithm::ES384
            | Algorithm::EdDSA
    )
}

/// The e-mail hint, if it looks like one line of ordinary text.
fn clean_email(e: Option<String>) -> Option<String> {
    e.filter(|e| {
        !e.is_empty() && e.len() <= 254 && !e.chars().any(|c| c.is_control() || c.is_whitespace())
    })
}

impl TokenVerifier for JwtVerifier {
    fn verify(
        &self,
        id_token: &str,
        expected_nonce: &str,
        now: i64,
    ) -> Result<VerifiedIdentity, OidcError> {
        use InvalidReason as R;
        let header = decode_header(id_token).map_err(|_| OidcError::Invalid(R::Malformed))?;
        if !allowed(header.alg) {
            return Err(OidcError::Invalid(R::Algorithm));
        }
        let kid = header.kid.ok_or(OidcError::Invalid(R::MissingKeyId))?;
        let keys = self.remote.keys(&kid, now)?;
        let jwk = keys.find(&kid).ok_or(OidcError::Invalid(R::UnknownKey))?;
        // A key published for encryption (or anything but signatures) is not used to verify one.
        if let Some(usage) = &jwk.common.public_key_use
            && !matches!(usage, PublicKeyUse::Signature)
        {
            return Err(OidcError::Invalid(R::KeyNotForSignatures));
        }
        // A key that says which algorithm it is for is not used for another.
        if let Some(declared) = jwk.common.key_algorithm
            && format!("{declared:?}") != format!("{:?}", header.alg)
        {
            return Err(OidcError::Invalid(R::KeyAlgorithmMismatch));
        }
        let key = DecodingKey::from_jwk(jwk).map_err(|_| OidcError::Invalid(R::UnknownKey))?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[self.remote.issuer()]);
        validation.set_audience(&[self.client_id.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        // Time is checked below against the caller's `now`, not the system clock.
        validation.validate_exp = false;
        validation.validate_nbf = false;
        let claims = decode::<IdClaims>(id_token, &key, &validation)
            .map_err(|e| {
                OidcError::Invalid(match e.kind() {
                    ErrorKind::InvalidIssuer => R::Issuer,
                    ErrorKind::InvalidAudience => R::Audience,
                    ErrorKind::MissingRequiredClaim(_) | ErrorKind::InvalidClaimFormat(_) => {
                        R::MissingClaim
                    }
                    ErrorKind::InvalidAlgorithm | ErrorKind::InvalidKeyFormat => {
                        R::KeyAlgorithmMismatch
                    }
                    ErrorKind::Json(_) => R::MissingClaim,
                    _ => R::Signature,
                })
            })?
            .claims;

        if claims.exp.saturating_add(LEEWAY_SECS) <= now {
            return Err(OidcError::Invalid(R::Expired));
        }
        if claims
            .nbf
            .is_some_and(|n| n.saturating_sub(LEEWAY_SECS) > now)
        {
            return Err(OidcError::Invalid(R::NotYetValid));
        }
        // `iat` is required (OpenID Connect Core 2), is not in the future, and is recent: a sign-in finishes
        // within minutes of the token being made, so an old token is not this login's.
        let issued = claims.iat.ok_or(OidcError::Invalid(R::MissingClaim))?;
        if issued.saturating_sub(LEEWAY_SECS) > now {
            return Err(OidcError::Invalid(R::IssuedInFuture));
        }
        if now.saturating_sub(issued) > MAX_AGE_SECS {
            return Err(OidcError::Invalid(R::IssuedTooLongAgo));
        }
        let several = claims.aud.count() > 1;
        if (several || claims.azp.is_some())
            && claims.azp.as_deref() != Some(self.client_id.as_str())
        {
            return Err(OidcError::Invalid(R::AuthorizedParty));
        }
        match claims.nonce.as_deref() {
            Some(n) if same_secret(n, expected_nonce) => {}
            _ => return Err(OidcError::Invalid(R::Nonce)),
        }
        let identity = ExternalIdentity::new(&claims.iss, &claims.sub)
            .map_err(|_| OidcError::Invalid(R::Subject))?;
        Ok(VerifiedIdentity::new(identity, clean_email(claims.email)))
    }
}
