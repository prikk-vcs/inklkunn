//! Signing in, signing out, the account page, invitations and joining through the real router and the real
//! authenticator (the sign-in handoff's U1 and U2; RFC 002 D-1). The identity is `AuthIdentity` over an
//! `Authenticator` on the same store as the wiki, with a cheap real Argon2id; time stands at `common::NOW`, and
//! an expired credential is one made in the past.
//!
//! * the sign-in page and its cookie: `__Host-`, `Secure`, `HttpOnly`, `SameSite=Strict`, no `Domain`;
//! * the refusal never says whether the account exists (the same page for a wrong password, an unknown name and
//!   a disabled account), is throttled per name and per address, and the address is the peer's unless a trusted
//!   proxy says otherwise;
//! * the header names who is signed in and offers the way out; a signed-out visitor is offered *Sign in* only
//!   where sign-in is served;
//! * the account page: the name, the sessions with *End this session*, tokens made with a scope and an expiry
//!   and shown once, revoked; none of it reachable with a token alone;
//! * an invitation makes an account once and says the same about a link that was used, one that ended and one
//!   that never was; joining is off unless the operator opened it.
#![allow(clippy::expect_used, clippy::unwrap_used, dead_code)]

#[path = "../common/mod.rs"]
mod common;

mod account;
mod harness;
mod identify;
mod invitations;
mod sign_in;
mod states;
