#![forbid(unsafe_code)]
//! `inklkunn-web` — inklkunn's HTTP surface (RFC 003). This release holds its foundations: the validated
//! configuration with its two origins, the headers every response carries, the one registry of routes, and
//! the server. Pages, the catalog, the sanitizer, feeds and the API are added on top, task by task, and
//! inherit the tests written here.
//!
//! - [`config`]: the origins and the bind address, checked together before a socket is opened.
//! - [`headers`]: the security and cache headers, set in one place for both origins.
//! - [`routes`]: the route registry — the only place a route is registered — and the routers built from it.
//! - [`origin`]: one listener, two origins, a request goes to the one its `Host` names.
//! - [`serve`]: bind, serve, stop on a signal.

pub mod config;
pub mod headers;
pub mod origin;
pub mod routes;
pub mod serve;

/// The assembled application: both origins behind one listener (see [`routes::site`]).
pub type Site = axum::Router;

pub use config::{ConfigRefusal, Origin, OriginError, Scheme, WebConfig};
