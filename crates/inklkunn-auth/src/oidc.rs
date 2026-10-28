//! OpenID Connect sign-in (RFC 002 D-1; AUTH-01): the **authorization code flow with PKCE**.
//!
//! [`OidcProvider::begin_login`] makes a `state`, a `nonce` and a PKCE verifier, keeps them on the server
//! (ten minutes, one-shot) and returns the authorization URL and the `state` to put in a cookie.
//! [`OidcProvider::finish_login`] takes the callback's `state` **and the state cookie** (login CSRF: a
//! callback the browser did not start is refused), redeems the pending login, exchanges the code with the
//! PKCE verifier, and has the [`TokenVerifier`] check the ID token against the provider's keys, issuer,
//! audience, times and nonce. The result is an [`inklkunn_store::ExternalIdentity`]; mapping it to an account is the account
//! store's link, and **an unlinked identity signs nobody in** (no auto-provisioning in this release).
//!
//! The state cookie is `SameSite=Lax`, not `Strict`: the provider's redirect back is a cross-site top-level
//! navigation, which a `Strict` cookie would not accompany. It is `__Host-`, `Secure` and `HttpOnly`, and
//! lives as long as a login may take.
//!
//! Every outbound request goes through the [`HttpsFetcher`] seam (the egress guard); the verifier is a seam
//! too, so the flow can be tested without cryptography and the cryptography without the flow.

mod fetch;
mod pending;
mod remote;
mod verify;

use std::fmt::{self, Write as _};
use std::sync::Arc;

use base64ct::{Base64, Base64UrlUnpadded, Encoding as _};
use sha2::{Digest as _, Sha256};

pub use fetch::{FetchError, FetchRequest, HttpsFetcher, MAX_RESPONSE_BYTES};
pub use pending::PENDING_TTL_SECS;
pub use verify::{TokenVerifier, VerifiedIdentity};

use crate::random;
use crate::stored::same_secret;
use pending::PendingLogins;
use remote::Remote;

/// The cookie that carries a login's `state` to the callback.
pub const STATE_COOKIE: &str = "__Host-inklkunn-oidc";

/// Why OpenID Connect sign-in did not complete. Coarse on purpose: a person sees "sign-in failed", the log
/// gets the detail.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OidcError {
    /// No provider is configured.
    NotConfigured,
    /// The ID token failed a check (signature, key, issuer, audience, times, nonce).
    Invalid,
    /// The callback's state is unknown, expired, already used, or not the one in the browser's cookie.
    State,
    /// The provider could not be reached or answered something unusable. The text is for operators.
    Provider(String),
    /// The system's random source failed.
    Random,
}

impl fmt::Display for OidcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OidcError::NotConfigured => f.write_str("no OpenID Connect provider is configured"),
            OidcError::Invalid => f.write_str("the ID token is not valid"),
            OidcError::State => {
                f.write_str("the sign-in state is unknown, expired or not this browser's")
            }
            OidcError::Provider(e) => write!(f, "OpenID Connect provider error: {e}"),
            OidcError::Random => f.write_str("the system's random source failed"),
        }
    }
}

impl std::error::Error for OidcError {}

impl From<crate::error::AuthError> for OidcError {
    fn from(_: crate::error::AuthError) -> Self {
        OidcError::Random
    }
}

/// A provider as the operator configured it.
#[derive(Clone)]
pub struct OidcConfig {
    /// The issuer, exactly as the provider publishes it; discovery is at
    /// `<issuer>/.well-known/openid-configuration`.
    pub issuer: String,
    /// The client id registered at the provider.
    pub client_id: String,
    /// The client secret; `None` for a public client (PKCE alone).
    pub client_secret: Option<String>,
    /// This service's callback URL, exactly as registered.
    pub redirect_uri: String,
}

impl fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret.as_ref().map(|_| "…"))
            .field("redirect_uri", &self.redirect_uri)
            .finish()
    }
}

/// What starting a login gives the web layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginLogin {
    /// Where to send the browser.
    pub authorization_url: String,
    /// The opaque `state`, to set as the [`STATE_COOKIE`] (see [`BeginLogin::state_cookie`]).
    pub state: String,
}

impl BeginLogin {
    /// The `Set-Cookie` value for the state cookie.
    pub fn state_cookie(&self) -> String {
        format!(
            "{STATE_COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={PENDING_TTL_SECS}",
            self.state
        )
    }
}

/// A configured OpenID Connect provider.
pub struct OidcProvider {
    cfg: OidcConfig,
    remote: Arc<Remote>,
    verifier: Arc<dyn TokenVerifier>,
    pending: PendingLogins,
}

impl OidcProvider {
    /// A provider that verifies ID tokens against its published keys.
    pub fn new(cfg: OidcConfig, fetcher: Arc<dyn HttpsFetcher>) -> Self {
        let remote = Arc::new(Remote::new(cfg.issuer.clone(), fetcher));
        let verifier = Arc::new(verify::JwtVerifier::new(
            Arc::clone(&remote),
            cfg.client_id.clone(),
        ));
        Self::with_verifier_and_remote(cfg, remote, verifier)
    }

    /// A provider whose tokens a different verifier checks (a test's double). The discovery and the token
    /// exchange still go through `fetcher`. Never available to a product build: a verifier that checks
    /// nothing would sign anyone in.
    #[cfg(any(test, feature = "test-knobs"))]
    pub fn with_verifier(
        cfg: OidcConfig,
        fetcher: Arc<dyn HttpsFetcher>,
        verifier: Arc<dyn TokenVerifier>,
    ) -> Self {
        let remote = Arc::new(Remote::new(cfg.issuer.clone(), fetcher));
        Self::with_verifier_and_remote(cfg, remote, verifier)
    }

    fn with_verifier_and_remote(
        cfg: OidcConfig,
        remote: Arc<Remote>,
        verifier: Arc<dyn TokenVerifier>,
    ) -> Self {
        OidcProvider {
            cfg,
            remote,
            verifier,
            pending: PendingLogins::default(),
        }
    }

    /// The issuer.
    pub fn issuer(&self) -> &str {
        &self.cfg.issuer
    }

    /// Start a login: make the state, nonce and PKCE verifier, keep them, and build the URL.
    pub fn begin_login(&self, now: i64) -> Result<BeginLogin, OidcError> {
        let d = self.remote.discovery(now)?;
        let state = random::token()?;
        let nonce = random::token()?;
        let verifier = random::token()?;
        let challenge = Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()));
        self.pending
            .insert(state.clone(), nonce.clone(), verifier, now);
        let sep = if d.authorization_endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        let authorization_url = format!(
            "{}{sep}response_type=code&client_id={}&redirect_uri={}&scope=openid&state={}&nonce={}\
             &code_challenge={}&code_challenge_method=S256",
            d.authorization_endpoint,
            pct(&self.cfg.client_id),
            pct(&self.cfg.redirect_uri),
            pct(&state),
            pct(&nonce),
            pct(&challenge),
        );
        Ok(BeginLogin {
            authorization_url,
            state,
        })
    }

    /// Finish a login from the callback. `state` is the callback's query parameter and `state_cookie` the
    /// value of the browser's [`STATE_COOKIE`]; they must be the same, and the state must be a login this
    /// server started a moment ago and has not finished.
    pub fn finish_login(
        &self,
        state: &str,
        state_cookie: &str,
        code: &str,
        now: i64,
    ) -> Result<VerifiedIdentity, OidcError> {
        if state.is_empty() || !same_secret(state, state_cookie) {
            return Err(OidcError::State);
        }
        let pending = self.pending.take(state, now).ok_or(OidcError::State)?;
        let d = self.remote.discovery(now)?;
        let form = format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
            pct(code),
            pct(&self.cfg.redirect_uri),
            pct(&self.cfg.client_id),
            pct(&pending.verifier),
        );
        let authorization = self.cfg.client_secret.as_ref().map(|secret| {
            format!(
                "Basic {}",
                Base64::encode_string(
                    format!("{}:{}", pct(&self.cfg.client_id), pct(secret)).as_bytes()
                )
            )
        });
        #[derive(serde::Deserialize)]
        struct TokenResponse {
            id_token: String,
        }
        let body = self.remote_fetch(&FetchRequest {
            url: d.token_endpoint,
            form: Some(form),
            authorization,
        })?;
        let tokens: TokenResponse = serde_json::from_slice(&body).map_err(|_| {
            OidcError::Provider("the token response was not the expected JSON".to_owned())
        })?;
        self.verifier.verify(&tokens.id_token, &pending.nonce, now)
    }

    fn remote_fetch(&self, request: &FetchRequest) -> Result<Vec<u8>, OidcError> {
        self.remote.fetch_raw(request)
    }

    #[cfg(test)]
    fn peek_pending(&self, state: &str) -> Option<(String, String)> {
        self.pending.peek(state).map(|p| (p.nonce, p.verifier))
    }
}

/// Percent-encode a URL query or form value: unreserved characters pass, everything else is `%XX`.
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b))
            }
            _ => {
                // Writing to a String cannot fail.
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
