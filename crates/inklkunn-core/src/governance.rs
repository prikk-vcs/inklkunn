//! The rules of a wiki as data (RFC 006 D-1 – D-4; handoff G-T1 part A): the types, the two files' schema and reader,
//! the validator, the defaults and the platform limits. **There is no surface here**: nothing registers a route, reads
//! a request or touches the store; the acceptance code and `authorize()` call these pure functions (part C).
//!
//! # The two files
//!
//! Both are optional, both are read from **the default view's tip**, and both are the plain YAML subset of the
//! frontmatter ([`inklkunn_markup::frontmatter`]: no anchors, no tags, no repeated keys), like `knowledge/`. A missing
//! file is its default, and **the defaults are 0.3.0's behaviour** ([`defaults`]). **Schema version 1 has no version
//! key**: the knowledge files have none, and the same rule is followed (a file is version 1 because it has these keys
//! and no others). A later version adds keys, and a file that carries a key this version does not know is refused as an
//! [`UnknownKey`](Refusal::UnknownKey): it is never half read. (A `version` key is itself an unknown key.)
//!
//! ## `governance/policy.yaml` ([`Policy`])
//!
//! | Key | Values | Default (0.3.0) |
//! |---|---|---|
//! | `entry` | [`Entry`]: `none`, `observer`, `questioner`, `contributor`; up to the platform's `INKLKUNN_GOVERNANCE_ENTRY_MAX` | `none` |
//! | `rungs.comment` | a rung: `observer`, `questioner`, `contributor`, `reviewer`, `maintainer`, `admin` | `questioner` |
//! | `rungs.propose` | a rung; at most `rungs.review` | `contributor` |
//! | `rungs.review` | a rung; at least `rungs.propose`, at most `rungs.accept` | `reviewer` |
//! | `rungs.create_view` | a rung | `reviewer` |
//! | `rungs.accept` | a rung; at least `rungs.review` | `maintainer` |
//! | `rungs.moderate` | a rung; at least `reviewer` | `maintainer` |
//! | `rungs.decide_appeal` | [`AppealRung`]: `maintainer`, `admin` | `admin` |
//! | `rungs.grant_up_to` | [`GrantUpTo`]: `none`, `observer`, `questioner`, `contributor`, `reviewer` | `none` |
//! | `acceptance.approvals` | 0 to [`MAX_APPROVALS`] (5), as a number | `0` |
//! | `acceptance.self` | [`SelfAcceptance`]: `never`, `small-wiki` | `small-wiki` |
//! | `knowledge.review_after` | days, 1 to [`MAX_REVIEW_AFTER_DAYS`] (36,500), as a number | the operator's |
//! | `promotion.<rung>` | a text of at most [`MAX_PROMOTION_CHARS`] characters, for any of the six rungs | none |
//! | `protect` | a list of at most [`MAX_PROTECT`] entries, see [`protect`] | none |
//!
//! ```yaml
//! entry: none
//! rungs:
//!   comment: questioner
//!   propose: contributor
//!   review: reviewer
//!   create_view: reviewer
//!   accept: maintainer
//!   moderate: maintainer
//!   decide_appeal: admin
//!   grant_up_to: none
//! acceptance:
//!   approvals: 0
//!   self: small-wiki
//! knowledge:
//!   review_after: 1096
//! promotion:
//!   contributor: "We look for a few edits that were accepted without a correction."
//! protect:
//!   - namespace: Policies
//!     require: review
//!     reason: "Rules of the community are read before they change."
//! ```
//!
//! **The ordering rule** (D-2): `propose` ≤ `review` ≤ `accept`, `moderate` ≥ `reviewer`, and `decide_appeal` ≥ `moderate`
//! (whoever decides appeals can reverse what a moderator did, so they must be able to do it); the refusal says which
//! pair ([`OrderingPair`]). **The admin actions stay `admin` and reading stays `observer`**, and no policy changes
//! them ([`Policy::minimum_role`]). **Promotion texts are guidance**: nothing promotes by them.
//!
//! ## `governance/constitution.yaml` ([`Constitution`])
//!
//! | Key | Values | Default |
//! |---|---|---|
//! | `policy_change.rung` | `maintainer`, `admin` | `maintainer` |
//! | `policy_change.approvals` | 0 to 5, as a number | `0` |
//! | `constitution_change.approvals` | 0 to 5, as a number | `0` |
//!
//! A constitution change always needs an **admin**: that rung is not a key, and writing it is an unknown key. The
//! hatch (an admin accepts a governance change with no approvals, whatever these say) is the acceptance code's.
//!
//! ## Bounds and characters
//!
//! A policy file is read to at most [`MAX_POLICY_BYTES`] and a constitution to [`MAX_CONSTITUTION_BYTES`]; `protect`
//! has at most [`MAX_PROTECT`] entries; a promotion text at most [`MAX_PROMOTION_CHARS`] characters, a protection's
//! reason [`MAX_REASON_CHARS`] and its title [`MAX_TARGET_CHARS`]. No control, bidirectional or line-separator
//! character, and no byte-order mark, may appear anywhere in a file. A text is one line.
//!
//! # What is refused, and how
//!
//! Every refusal is a [`Fault`]: the file, the schema's field (never a word of the file's), the line where it can be
//! said, and a [`Refusal`] with a stable [`name`](Refusal::name). The reading of a file already in the repository gives
//! a [`ReadFault`] (`governance/policy.yaml, line 4: <rule>`), which is what makes every write refuse while the wiki
//! still reads.
//!
//! - [`validate_policy`], [`validate_constitution`]: what acceptance asks (the schema, the ordering rule, the
//!   platform limits).
//! - [`read_policy`], [`read_constitution`]: what the service asks of the repository's file.
//! - [`refuse_outside_default_view`]: a governance file in an alternative view.
//! - [`Policy::as_read_under`]: a policy above a platform limit read as if it were at it.
//! - [`GovernanceChange::required`]: the rung and approvals a governance change needs.
//! - [`approvals_unreachable`]: the approvals warning.

mod chars;
mod constitution;
pub mod defaults;
pub mod draft;
mod fault;
mod limits;
mod locate;
mod model;
mod parse;
mod policy;
pub mod protect;
mod rules;
mod validate;
mod warning;

pub use constitution::{Constitution, GovernanceChange, Requirement};
pub use fault::{Fault, GovernanceFile, Limit, MAX_FOUND, OrderingPair, ReadFault, Refusal};
pub use limits::{LIMIT_DAYS_MAX, LIMIT_DAYS_MIN, Lowered, PlatformLimits};
pub use model::{Acceptance, AppealRung, Entry, GrantUpTo, Rungs, SelfAcceptance};
pub use policy::Policy;
pub use protect::{
    Protection, Require, Target, holds_only_allowed_characters, is_canonical_title,
    is_valid_reason, namespace_covers,
};
pub use rules::{DEFAULTS_WORD, Rules, SHORT_BLOCK_CHARS, short_block};
pub use validate::{
    ViewPlace, is_below_governance, is_governance_path, mixes_rules_with_other_paths,
    only_governance_files, read_constitution, read_policy, refuse_outside_default_view,
    refuse_unknown_files, validate_constitution, validate_constitution_bytes, validate_policy,
    validate_policy_bytes,
};
pub use warning::{ApprovalsWarning, approvals_unreachable, approvals_warning};

/// The directory the rules are in: reserved by RFC 001, given its meaning by RFC 006.
pub const GOVERNANCE_DIR: &str = "governance";

/// [`GOVERNANCE_DIR`] with its slash, as a refusal names it.
pub(crate) const GOVERNANCE_DIR_SLASH: &str = "governance/";

/// The constitution's path.
pub const CONSTITUTION_PATH: &str = "governance/constitution.yaml";

/// The policy's path.
pub const POLICY_PATH: &str = "governance/policy.yaml";

/// The most bytes `governance/policy.yaml` may have. A file with [`MAX_PROTECT`] protections at the length limits and
/// every promotion text at its limit fits even in four-byte characters.
pub const MAX_POLICY_BYTES: usize = 128 * 1024;

/// The most bytes `governance/constitution.yaml` may have.
pub const MAX_CONSTITUTION_BYTES: usize = 16 * 1024;

/// The most standing protections a policy lists. A namespace covers a whole subtree, so a community needs few.
pub const MAX_PROTECT: usize = 32;

/// The most characters of a promotion text.
pub const MAX_PROMOTION_CHARS: usize = 600;

/// The most characters of a protection's reason.
pub const MAX_REASON_CHARS: usize = 300;

/// The most characters of a protection's page or namespace title.
pub const MAX_TARGET_CHARS: usize = 255;

/// The most approvals a policy or a constitution may ask for.
pub const MAX_APPROVALS: u8 = 5;

/// The most days `knowledge.review_after` may be (the operator's own bound, `INKLKUNN_KNOWLEDGE_REVIEW_AFTER_DAYS`).
pub const MAX_REVIEW_AFTER_DAYS: u32 = 36_500;

#[cfg(test)]
mod tests;
