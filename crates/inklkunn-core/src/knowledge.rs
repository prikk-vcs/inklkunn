//! The knowledge layer's files and what is derived from them (RFC 005 parts 1 and 2; handoff K-T1 … K-T4).
//!
//! Claims and sources are **small files beside the pages**: `knowledge/claims/<id>.yaml` and
//! `knowledge/sources/<id>.yaml` (part 2 D-1, D-2), changed only by proposals, read here in the plain YAML subset of
//! the frontmatter (no anchors, no tags, no repeated keys; the machinery is `inklkunn_markup::frontmatter`, not a
//! second parser). Everything else is derived from them and rebuilt from them, never a second source of truth.
//!
//! - [`Claim`] and [`Source`]: the files as values, read with [`Claim::parse`] and [`Source::parse`] (or
//!   [`read_file`], which also judges the path and the size) and written canonically with `to_yaml`. The
//!   enumerations are closed, each word the format's own; **nothing is numeric and nothing ranks**: `bearing` says
//!   how directly a source speaks to a claim, never how true either is.
//! - [`block_digest`]: the one definition of *the wording a relation or a status was judged on*.
//! - [`check`]: validation at acceptance (part 2 D-5): the schema, the ids, every source and claim a file names
//!   present in the same view, every anchor naming a block that exists in the named page at the proposal's result,
//!   no owned page key affected. A refusal names the file and the rule and never repeats what a file said.
//! - [`merge`]: the knowledge files in RFC 004's merge (part 2 D-4): YAML key by key, lists by stable identity,
//!   a value changed on both sides an overlap with both texts, nothing lost.
//! - [`compare`]: two views' claims side by side, claim by claim, what differs and nothing about which is right
//!   (part 2 D-9; handoff K-T8).
//! - [`index`]: the derived knowledge index of a tip (part 2 D-6), built from the same `extract` of the tip as the
//!   title index and the link graph.
//! - [`compose`], [`marker`], [`onpage`] and [`summary`]: making a claim and attaching a source in a proposal
//!   (part 2 D-3, handoff K-T5): the files a paragraph and a source come to, the paragraph's `^id` held by a form
//!   and put back (part 1 D-1 b), what a page holds, and what a revision's knowledge files do, in facts.
//! - [`touched`]: the claims a change touches and what it does to what they were judged on (part 1 D-1 c).
//! - [`curation`]: the curation proposal, the only way a community status, a review date or an availability is
//!   set, and the stamp that names the proposal that decided.

mod anchor;
pub mod check;
mod claim;
pub mod compare;
pub mod compose;
pub mod curation;
mod fault;
pub mod index;
pub mod marker;
pub mod merge;
mod model;
pub mod onpage;
mod path;
mod source;
pub mod summary;
pub mod touched;
mod tree;
mod yaml;

pub use anchor::{AnchorState, anchor_state};
pub use fault::{Fault, Rule};
pub use model::{
    Availability, Bearing, Claim, ClaimId, CommunityStatus, Digest, Expression, Identity,
    Lifecycle, Link, MAX_ID_CHARS, Reference, ReferenceForm, Relation, RelationType, Reviewed,
    Snapshot, Source, SourceId, SourceKind, SourceLink, Standing, StatementKind, block_digest,
};
pub use path::{
    CLAIMS_DIR, Family, File, KNOWLEDGE_DIR, MAX_FILE_BYTES, SOURCES_DIR, claim_path, family_of,
    is_keep, is_knowledge_path, read_file, source_path,
};
pub use tree::{MAX_TREE_BYTES, TreeError, TreeFile, list_tree, read_head, read_tree};
pub use yaml::{MAX_EXPRESSIONS, MAX_LINKS, MAX_TEXT_CHARS};

#[cfg(test)]
mod tests;
