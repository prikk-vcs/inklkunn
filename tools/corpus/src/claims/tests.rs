#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The `claims` tests. Every text in them is invented; nothing touches the network (a scripted fetcher stands in
//! for `curl`) and nothing reads a real wiki.

mod convert;
mod get;
mod measure;
mod plan;
mod pool;
mod replay;
mod stats;
mod support;
mod wire;
