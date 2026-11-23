//! Who holds which rung and who is restricted (RFC 006 D-6, D-7; handoff G-T2): the rules as pure tables, the acts through
//! the real service over both stores, the restriction's cap and its end by the clock, and what each refusal tells the
//! person. Each test names the rule it holds, and fails without it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod entry;
mod grants;
mod page;
mod restrictions;
mod rules;
mod support;
