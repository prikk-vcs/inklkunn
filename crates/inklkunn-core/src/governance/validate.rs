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

/// Is `path` **below** `governance/`, spelled as the repository spells it (`governance/` and then at least one more
/// character; no case folding, no `./`)? What a **removal** is allowed to name (handoff G-T1 correction D-1): a change may
/// remove any path below `governance/`, and write only the two files. Narrower than [`is_governance_path`], which is a
/// safety net that answers *yes* when in doubt: a path this answers *yes* to is one the signer can remove.
pub fn is_below_governance(path: &str) -> bool {
    path.strip_prefix(GOVERNANCE_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|rest| !rest.is_empty())
}

/// **The set of paths under `governance/` that a change may WRITE is closed**: exactly the files
/// ([`GovernanceFile::ALL`], three). Any other path under it that is written, or renamed to or from, is refused as
/// [`Refusal::UnknownFile`]: no other file there is part of the rules of a wiki, and none is ever read as if it were.
/// `written` is what the change writes and both ends of each rename; **a removal is not asked here**, because a change may
/// remove any path below `governance/` (D-1: it is how something standing where a governance file belongs is cleared, and in a
/// sound wiki there is nothing else there to remove).
pub fn refuse_unknown_files<'a>(written: impl IntoIterator<Item = &'a str>) -> Result<(), Fault> {
    match written
        .into_iter()
        .find(|p| is_governance_path(p) && GovernanceFile::of_path(p).is_none())
    {
        None => Ok(()),
        Some(_) => Err(Fault::unknown_file()),
    }
}

/// Does the change touch nothing but the rules, and at least one path? What the repair of a wiki whose governance cannot be
/// read is made of (RFC 006 D-1; threat model C-3d): a change whose every path is one of the two governance files **or lies
/// below one** ([`GovernanceFile::at_or_below`]: what stands where the file belongs, which a repair removes, handoff G-T1
/// correction D-1). The one definition, for the acceptance's judgement and for the transitions that end a proposal (correction
/// C-3). That a path below a file is only ever *removed* is the writes' rule ([`refuse_unknown_files`], asked when the change is
/// proposed and again before the signer); this answers *what is it about*, not *what is it allowed to do*.
pub fn only_governance_files<'a>(touched: impl IntoIterator<Item = &'a str>) -> bool {
    let (mut any, mut all) = (false, true);
    for path in touched {
        any = true;
        all &= GovernanceFile::at_or_below(path).is_some();
    }
    any && all
}

/// **A change to the rules is its own proposal** (RFC 006 D-4; handoff G-T1 correction C-1): a change that touches a path
/// under `governance/` touches nothing else. `touched` is every path the proposal writes, removes or renames to or from;
/// the answer is *yes* when at least one is under `governance/` and at least one is not. Touching both governance files
/// together is not mixing. What accepts a change to the rules (the hatch, and the approvals it lifts) is then never what
/// accepts a page.
pub fn mixes_rules_with_other_paths<'a>(touched: impl IntoIterator<Item = &'a str>) -> bool {
    let (mut rules, mut others) = (false, false);
    for path in touched {
        if is_governance_path(path) {
            rules = true;
        } else {
            others = true;
        }
    }
    rules && others
}
