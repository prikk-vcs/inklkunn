#![forbid(unsafe_code)]
//! `inklkunn-web` — inklkunn's HTTP surface (RFC 003). This release holds its foundations: the validated
//! configuration with its two origins, the headers every response carries, the one registry of routes, the
//! server, and the words, the layout and the stylesheet every page is built from. Pages, the sanitizer, feeds
//! and the API are added on top, task by task, and inherit the tests written here.
//!
//! - [`config`]: the origins and the bind address, checked together before a socket is opened.
//! - [`headers`]: the security and cache headers, set in one place for both origins.
//! - [`routes`]: the route registry — the only place a route is registered — and the routers built from it.
//! - [`origin`]: one listener, two origins, a request goes to the one its `Host` names.
//! - [`serve`]: bind, serve, stop on a signal.
//! - [`catalog`]: every sentence a page says, by stable key, in English and (as a preview) Japanese.
//! - [`lang`]: which language a request wants, the cookie that remembers it, where a switch returns to.
//! - [`pages`]: the layout, the language page and the notices; the extractor every page handler asks for.
//! - [`style`]: the one stylesheet, served under a fingerprinted name.
//! - [`proposals`]: the proposal form, the proposal's page and its review, a page's discussion (RFC 004; handoff 004 T3).
//! - `signin` and `account`: signing in and out, joining, an invitation, and the account page (the sign-in handoff).
//! - `csrf`: the one check that every write of a signed-in person carries the session's token (handoff 004 P6).
//! - [`identity`]: the real `Identify`, over the authenticator.
//! - [`api`]: the read API under `/{slug}/-/api/v1/` and its OpenAPI description, generated from the route table.

mod account;
pub mod api;
pub mod catalog;
mod changes;
pub mod config;
mod csrf;
mod discover;
mod feed;
pub mod headers;
mod health;
mod history;
pub mod identity;
mod inbox;
pub mod lang;
pub mod origin;
pub mod pages;
mod present;
mod probe;
pub mod proposals;
mod raw;
pub mod routes;
mod search;
pub mod serve;
pub mod services;
mod signin;
pub mod style;
mod time;
mod url;
mod watch;
mod wiki;

/// The assembled application: both origins behind one listener (see [`routes::site`]).
pub type Site = axum::Router;

/// The request headers [`Identify`] reads: re-exported so that the binary, which implements it, needs no web
/// framework of its own.
pub use axum::http::HeaderMap;
pub use config::{ConfigRefusal, Origin, OriginError, Scheme, WebConfig};
pub use csrf::{FIELD as CSRF_FIELD, HEADER as CSRF_HEADER};
pub use identity::AuthIdentity;
pub use services::{
    Acceptor, AnonymousOnly, Identify, NoSigner, Services, SessionView, WriteIdentity,
};
pub use signin::names::OIDC_CALLBACK_PATH;
