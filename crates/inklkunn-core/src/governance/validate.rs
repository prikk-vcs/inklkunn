//! Validation (RFC 006 D-4) and reading (D-1): pure functions from a file's text to a value or to the rule it breaks.
//!
//! - [`validate_policy`] and [`validate_constitution`] are **what acceptance asks before the signer is** (part C wires
//!   them in): the schema, the ordering rule and the platform limits. A refusal is a [`Fault`] with a stable
//!   [`name`](Fault::name).
//! - [`read_policy`] and [`read_constitution`] are **what the service asks of a file already in the repository**: the
//!   same schema and ordering rule, but not the platform limits (a policy above a limit is read as if it were at the
//!   limit, [`Policy::as_read_under`]), and the fault is a [`ReadFault`], the one that makes every write refuse and
//!   names the file and the line (threat model C-3d).
//! - [`refuse_outside_default_view`] is D-1's rule that `governance/` is changed in the default view only.

use super::fault::{Fault, GovernanceFile, ReadFault, Refusal};
use super::parse::{locate_fault, parse_constitution, parse_policy};
use super::{Constitution, GOVERNANCE_DIR, PlatformLimits, Policy};

/// Judge the text of a proposed `governance/policy.yaml` for acceptance: it must be wholly well formed, obey the
/// ordering rule and stay within `limits`. The error names the file, the rule, the field and (where it can be said)
/// the line, and never repeats what the file said.
pub fn validate_policy(text: &str, limits: &PlatformLimits) -> Result<Policy, Fault> {
    let policy = parse_policy(text)?;
    limits
        .check(&policy)
        .map_err(|fault| locate_fault(text, fault))?;
    Ok(policy)
}

/// [`validate_policy`] for the bytes of a file: refused as [`Refusal::NotText`] when they are not UTF-8.
pub fn validate_policy_bytes(bytes: &[u8], limits: &PlatformLimits) -> Result<Policy, Fault> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Fault::whole(GovernanceFile::Policy, Refusal::NotText))?;
    validate_policy(text, limits)
}

/// Judge the text of a proposed `governance/constitution.yaml` for acceptance.
pub fn validate_constitution(text: &str) -> Result<Constitution, Fault> {
    parse_constitution(text)
}

/// [`validate_constitution`] for the bytes of a file.
pub fn validate_constitution_bytes(bytes: &[u8]) -> Result<Constitution, Fault> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Fault::whole(GovernanceFile::Constitution, Refusal::NotText))?;
    validate_constitution(text)
}

/// Read the bytes of `governance/policy.yaml` as they are in the repository. The platform limits are not judged here:
/// apply them with [`Policy::as_read_under`].
pub fn read_policy(bytes: &[u8]) -> Result<Policy, ReadFault> {
    let file = GovernanceFile::Policy;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ReadFault::new(file, Fault::whole(file, Refusal::NotText)))?;
    parse_policy(text).map_err(|fault| ReadFault::new(file, fault))
}

/// Read the bytes of `governance/constitution.yaml` as they are in the repository.
pub fn read_constitution(bytes: &[u8]) -> Result<Constitution, ReadFault> {
    let file = GovernanceFile::Constitution;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ReadFault::new(file, Fault::whole(file, Refusal::NotText)))?;
    parse_constitution(text).map_err(|fault| ReadFault::new(file, fault))
}

/// Where a proposal is: in the wiki's default view, or in another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewPlace {
    /// The default view: the only place `governance/` is read from and changed.
    Default,
    /// Any other view (an alternative view).
    Alternative,
}

/// Is `path` under `governance/` (or is it `governance`)? A leading `./`, an empty component and the case of the
/// directory's name do not hide it: this is a safety net for an accident, and it answers *yes* when in doubt.
pub fn is_governance_path(path: &str) -> bool {
    path.split('/')
        .find(|c| !c.is_empty() && *c != ".")
        .is_some_and(|first| first.eq_ignore_ascii_case(GOVERNANCE_DIR))
}

/// A governance file in any view but the default one is refused (RFC 006 D-1: a governance fork is not in scope;
/// threat model C-3e). `touched` is every path the proposal writes, removes or renames **to or from**; one under
/// `governance/` in an alternative view is the refusal [`Refusal::OutsideDefaultView`], naming the file when it is one of
/// the two. In the default view nothing is refused here (the files' own validation is [`validate_policy`] and
/// [`validate_constitution`]).
pub fn refuse_outside_default_view<'a>(
    touched: impl IntoIterator<Item = &'a str>,
    place: ViewPlace,
) -> Result<(), Fault> {
    if place == ViewPlace::Default {
        return Ok(());
    }
    match touched.into_iter().find(|p| is_governance_path(p)) {
        None => Ok(()),
        Some(path) => Err(Fault::outside_default_view(GovernanceFile::of_path(path))),
    }
}
