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
//! - [`proxy`]: the client address a throttle counts, from a declared trusted proxy only.
//! - [`session`]: opaque server-side sessions (kept private to the crate), their cookies and CSRF tokens.
//! - [`token`]: API tokens, hashed, scoped, expiring, revocable (the store-facing type is private).
//! - [`oidc`]: the provider seams and the code + PKCE flow.
//! - [`authenticator`]: the pieces together, and the only way into all of the above. The bounded gate in
//!   front of password verification and the failure throttles (counted before they are judged) are private
//!   to the crate; their policy is [`ThrottlePolicy`].

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
