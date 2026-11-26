//! The governance module's tests: the table of 0.3.0 first, then the schema, the refusals, the matching and the
//! hostile inputs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use inklkunn_store::Role;

/// A policy from a text that must be valid under the default limits.
pub(crate) fn policy(text: &str) -> Policy {
    validate_policy(text, &PlatformLimits::default()).expect("a valid policy")
}

/// The refusal a policy text gets under the default limits.
pub(crate) fn policy_fault(text: &str) -> Fault {
    validate_policy(text, &PlatformLimits::default()).expect_err("a refused policy")
}

/// The refusal a constitution text gets.
pub(crate) fn constitution_fault(text: &str) -> Fault {
    validate_constitution(text).expect_err("a refused constitution")
}

mod defaults;
mod draft;
mod hostile;
mod interwiki;
mod keys;
mod limits;
mod lines;
mod protect;
mod refusals;
