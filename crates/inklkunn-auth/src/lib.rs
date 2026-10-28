#![forbid(unsafe_code)]
//! `inklkunn-auth` — who is asking (RFC 002 D-1). Turns a presented credential into a
//! [`inklkunn_core::Principal`], which is all `authorize()` ever sees. Local passwords (Argon2id), browser
//! sessions with CSRF tokens, scoped hashed API tokens, throttles per sign-in name and per client address,
//! and OpenID Connect (authorization code + PKCE, ID tokens verified against the provider's keys).
//!
//! **Nothing here decides what a principal may do** — that is `inklkunn-core`'s one function — and nothing
//! here is asynchronous: password hashing is deliberately heavy, so a web handler calls into this crate from
//! a blocking context (`spawn_blocking`), never from an async worker. Time is passed in (`now`, Unix
//! seconds) so every rule is testable without a clock.
//!
//! - [`hashing`]: Argon2id, the cost ceiling, the new-password rule.
//! - [`gate`]: the bounded number of password verifications at once.
//! - [`throttle`], [`proxy`]: failures counted per sign-in name and per client address; the address from a
//!   declared trusted proxy only.
//! - [`session`]: opaque server-side sessions, their cookies and CSRF tokens.
//! - [`token`]: API tokens, hashed, scoped, expiring, revocable.
//! - [`oidc`]: the provider seams and the code + PKCE flow.
//! - [`authenticator`]: the pieces together.

pub mod authenticator;
pub mod error;
mod gate;
pub mod hashing;
pub mod oidc;
pub mod proxy;
mod random;
pub mod session;
mod stored;
mod throttle;
pub mod token;

pub use authenticator::{
    AccountError, AuthConfig, Authenticator, OidcSignInError, SessionCheck, SessionPrincipal,
    SignedIn,
};
pub use error::AuthError;
pub use hashing::{Argon2idHasher, PasswordHasher, PasswordRefusal};
pub use proxy::{ClientAddress, ProxyPolicy};
pub use session::{CsrfToken, NewSession};
pub use throttle::ThrottlePolicy;
