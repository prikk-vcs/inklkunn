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
//! - [`api`]: the read API under `/{slug}/-/api/v1/` and its OpenAPI description, generated from the route table.

pub mod api;
pub mod catalog;
pub mod config;
pub mod headers;
mod history;
pub mod lang;
pub mod origin;
pub mod pages;
mod present;
mod probe;
pub mod routes;
pub mod serve;
pub mod services;
pub mod style;
mod time;
mod url;
mod wiki;

/// The assembled application: both origins behind one listener (see [`routes::site`]).
pub type Site = axum::Router;

pub use config::{ConfigRefusal, Origin, OriginError, Scheme, WebConfig};
pub use services::{AnonymousOnly, Identify, Services};
