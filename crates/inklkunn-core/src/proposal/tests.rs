//! The proposal service (handoff 004 T1): the records and the life of a proposal on both stores, the state
//! machine and a walk of every sequence of acts to a depth, the quotas at their boundaries, drafts that only
//! their author can see, and the metrics against a hand count. No repository is touched here (no merge, no
//! acceptance: handoff 004 T2 and T4); the base source is a double, except in `bases`, which asks the real
//! read service over its in-memory repository.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod authorization;
mod bases;
mod drafts;
mod lifecycle;
mod machine;
mod metrics;
mod quotas;
mod rollback;
mod support;
mod validation;
mod walk;
