//! The read service against an in-memory repository (so both test runs, with and without prikk, run all of
//! it): the numbers of prikk calls, the cache, the index, the sanitizer on what pages and metadata carry,
//! transclusion, the title caps, the store-size figure and the checkpoint refusal. The real prikk is in
//! `tests/read_path.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use super::testing::{Env, World};
use super::*;

fn found(outcome: Result<PageOutcome, ReadError>) -> PageView {
    match outcome.expect("a read") {
        PageOutcome::Found(view) => *view,
        other => panic!("expected a page, got {other:?}"),
    }
}

fn read_page(w: &World, title: &str) -> PageView {
    found(w.service.page(&w.wiki, None, title, &Env::EN))
}

fn ttl_zero() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::ZERO,
        ..ReadConfig::default()
    }
}

mod cache;
mod counts;
mod failures;
mod safety;
