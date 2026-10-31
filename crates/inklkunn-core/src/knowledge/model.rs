//! The claim and the source, as values (RFC 005 part 2, D-1 and D-2).
//!
//! **Nothing here is numeric and nothing ranks.** The enumerations are closed (a word the format does not know is
//! refused, never carried); `bearing` says how directly a source speaks to a claim and nothing about how true
//! either is; the community status is a word and the proposal that decided it, never a count.

use std::fmt;

use inklkunn_markup::Block;
use sha2::{Digest as _, Sha256};

use super::{Fault, Rule};

// ---------------------------------------------------------------------------------------------- identifiers

/// Is `s` an identifier in the layout of a version-7 UUID, lower-case with hyphens?
fn is_uuid_v7(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            14 => *c == b'7',
            19 => matches!(*c, b'8' | b'9' | b'a' | b'b'),
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}

/// Is `s` 36 lower-case hexadecimal characters and hyphens in UUID layout (any version)? The ids of proposals.
pub(super) fn is_uuid_shape(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}

macro_rules! uuid_v7_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// The id a text spells, if it is a version-7 UUID in lower case with hyphens.
            pub fn parse(text: &str) -> Option<Self> {
                is_uuid_v7(text).then(|| $name(text.to_owned()))
            }

            /// A fresh id (version 7: time-ordered).
            pub fn new() -> Self {
                $name(inklkunn_store::record::uuid_v7())
            }

            /// The id as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

uuid_v7_id! {
    /// A claim's id (the name of its file, `knowledge/claims/<id>.yaml`).
    ClaimId
}

uuid_v7_id! {
    /// A source's id (the name of its file, `knowledge/sources/<id>.yaml`).
    SourceId
}

/// The digest of the words a relation or a status was judged on: SHA-256 of an expression's block text, 64
/// lower-case hexadecimal characters. See [`block_digest`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(String);

impl Digest {
    /// The digest a text spells, if it is 64 lower-case hexadecimal characters.
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 64
            && text
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
        .then(|| Digest(text.to_owned()))
    }

    /// The digest as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The digest of a block's text **as [`inklkunn_markup::blocks`] gives it** (surrounding whitespace trimmed,
/// nothing else changed: not normalized, not folded): the one definition of what an expression's wording is, used
/// when a relation is judged and every time the judgement is compared with the page. The block's own `^id`
/// marker is part of its text, as it is everywhere the merge compares blocks.
pub fn block_digest(block: &Block<'_>) -> Digest {
    let sum = Sha256::digest(block.text.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in sum {
        for nibble in [byte >> 4, byte & 0x0f] {
            out.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
        }
    }
    Digest(out)
}

/// A page's id as an expression names it: the page's frontmatter `id`, any text of a sensible length.
pub(super) fn page_id_ok(text: &str) -> bool {
    !text.is_empty() && text.chars().count() <= MAX_ID_CHARS && !text.chars().any(char::is_control)
}

/// A block's `^id` as an expression names it (the markup profile's own rule: ASCII letters and digits, `_` and
/// `-`, beginning with a letter or digit).
pub(super) fn block_id_ok(text: &str) -> bool {
    let mut chars = text.chars();
    text.len() <= MAX_ID_CHARS
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The most characters of a page id or a block id.
pub const MAX_ID_CHARS: usize = 128;

// ---------------------------------------------------------------------------------------------- enumerations

macro_rules! words {
    ($(#[$meta:meta])* $name:ident, $field:literal { $($(#[$vmeta:meta])* $variant:ident = $word:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every word, in the order the format lists them.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The word as the file spells it.
            pub const fn word(self) -> &'static str {
                match self { $($name::$variant => $word),+ }
            }

            /// The value a word spells.
            pub fn from_word(word: &str) -> Option<Self> {
                match word { $($word => Some($name::$variant),)+ _ => None }
            }

            /// The key of the format this enumeration is the value of.
            pub(super) const FIELD: &'static str = $field;
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.word())
            }
        }
    };
}

words! {
    /// What kind of statement a claim is (KN-3).
    StatementKind, "kind" {
        /// Something observed.
        Observation = "observation",
        /// A reading of observations.
        Interpretation = "interpretation",
        /// A proposed explanation not yet tested.
        Hypothesis = "hypothesis",
        /// A view someone holds.
        Opinion = "opinion",
        /// A decision a community took.
        Decision = "decision",
        /// A rule a community keeps.
        Policy = "policy",
        /// An open question.
        Question = "question",
        /// Not said.
        Unknown = "unknown",
    }
}

words! {
    /// Where a claim stands in its life (H-4).
    Lifecycle, "lifecycle" {
        /// Put forward.
        Proposed = "proposed",
        /// Taken up.
        Adopted = "adopted",
        /// No longer maintained (with or without a successor).
        Retired = "retired",
        /// Not said.
        Unknown = "unknown",
    }
}

words! {
    /// What the community has decided about a claim, by an accepted curation proposal and never by a count (KN-2).
    Standing, "status" {
        /// Accepted.
        Accepted = "accepted",
        /// Disputed.
        Contested = "contested",
        /// Not settled.
        Unresolved = "unresolved",
        /// Not decided.
        Unknown = "unknown",
    }
}

words! {
    /// How one claim relates to another (part 2 D-1).
    RelationType, "type" {
        /// This claim narrows the other.
        Refines = "refines",
        /// This claim replaces the other.
        Supersedes = "supersedes",
        /// This claim is at odds with the other.
        Contradicts = "contradicts",
        /// This claim came out of the other when it was split.
        SplitFrom = "split-from",
        /// This claim took the other in when they were joined.
        MergedInto = "merged-into",
    }
}

words! {
    /// What a source does for a claim.
    Link, "relation" {
        /// The source supports the claim.
        Supports = "supports",
        /// The source challenges the claim.
        Challenges = "challenges",
    }
}

words! {
    /// How directly a source speaks to a claim. **Never how true either is** (KN-5).
    Bearing, "bearing" {
        /// The source speaks to the claim itself.
        Direct = "direct",
        /// The source speaks to part of the claim.
        Partial = "partial",
        /// The source speaks to the claim only by implication.
        Indirect = "indirect",
    }
}

words! {
    /// What kind of source it is.
    SourceKind, "kind" {
        /// First-hand.
        Primary = "primary",
        /// About first-hand sources.
        Secondary = "secondary",
        /// A record of an observation.
        Observational = "observational",
        /// The result of a computation.
        Computational = "computational",
        /// A report by a person of what they saw or heard.
        Anecdotal = "anecdotal",
        /// None of these.
        Other = "other",
    }
}

words! {
    /// Whether the source can be had, as a person who checked says (never a crawler).
    Availability, "availability" {
        /// It can.
        Available = "available",
        /// It cannot.
        Unavailable = "unavailable",
        /// Nobody has checked.
        Unknown = "unknown",
    }
}

words! {
    /// How a source's reference is written.
    ReferenceForm, "type" {
        /// A web address.
        Url = "url",
        /// A DOI.
        Doi = "doi",
        /// An ISBN.
        Isbn = "isbn",
        /// A page in this wiki.
        Page = "page",
        /// A citation in words.
        Citation = "citation",
    }
}

// ---------------------------------------------------------------------------------------------- the claim

/// Where a claim is said: a block of a page, named by the page's id and the block's `^id` (part 1 D-1 a). A claim
/// may have expressions in several pages and languages.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Expression {
    /// The page's frontmatter `id`.
    pub page_id: String,
    /// The block's `^id`, without the caret.
    pub block: String,
}

impl Expression {
    /// An expression, if both parts are well formed.
    pub fn new(page_id: &str, block: &str) -> Result<Expression, Fault> {
        if !page_id_ok(page_id) || !block_id_ok(block) {
            return Err(Fault::at(Rule::Id, "expressions"));
        }
        Ok(Expression {
            page_id: page_id.to_owned(),
            block: block.to_owned(),
        })
    }
}

/// A claim's relation to another claim.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Relation {
    /// How.
    pub kind: RelationType,
    /// To which claim.
    pub claim: ClaimId,
}

/// What the community has decided, with the proposal that decided it and the words it was decided on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityStatus {
    /// The word.
    pub status: Standing,
    /// The accepted curation proposal that decided it (a UUID), when one did.
    pub proposal: Option<String>,
    /// The digest of the expression's text when it was decided; the status is shown as *judged on an earlier
    /// wording* once the text no longer matches.
    pub judged_on: Option<Digest>,
}

impl CommunityStatus {
    /// Not decided.
    pub fn undecided() -> CommunityStatus {
        CommunityStatus {
            status: Standing::Unknown,
            proposal: None,
            judged_on: None,
        }
    }
}

/// A claim's relation to a source file, and the wording it was judged on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLink {
    /// The source.
    pub source: SourceId,
    /// Supports or challenges.
    pub relation: Link,
    /// How directly the source speaks to the claim.
    pub bearing: Bearing,
    /// A note, in words.
    pub note: Option<String>,
    /// The digest of the expression's text when the relation was judged.
    pub judged_on: Digest,
}

/// A claim file (`knowledge/claims/<id>.yaml`; part 2 D-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The claim's id: the file's name, not a key in it.
    pub id: ClaimId,
    /// What kind of statement it is.
    pub kind: StatementKind,
    /// Where it is said.
    pub expressions: Vec<Expression>,
    /// How it relates to other claims.
    pub relations: Vec<Relation>,
    /// Where it stands in its life.
    pub lifecycle: Lifecycle,
    /// What the community has decided.
    pub community_status: CommunityStatus,
    /// Its sources.
    pub sources: Vec<SourceLink>,
}

impl Claim {
    /// A claim with nothing said about it yet.
    pub fn new(id: ClaimId) -> Claim {
        Claim {
            id,
            kind: StatementKind::Unknown,
            expressions: Vec::new(),
            relations: Vec::new(),
            lifecycle: Lifecycle::Unknown,
            community_status: CommunityStatus::undecided(),
            sources: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------- the source

/// Who and what a source is, as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Identity {
    /// A title.
    pub title: Option<String>,
    /// An author or a publisher.
    pub author: Option<String>,
    /// A date, as written (a year, a season, a day).
    pub date: Option<String>,
}

/// Where a source is found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// How the reference is written.
    pub form: ReferenceForm,
    /// The reference.
    pub value: String,
}

/// A copy of a source kept for the day it cannot be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Snapshot {
    /// An archived copy kept in the wiki as an attachment, **only where its licence allows**.
    Attachment {
        /// The attachment's path in the repository.
        path: String,
    },
    /// An archive address, with the digest of what was archived.
    Archive {
        /// Where.
        address: String,
        /// SHA-256 of the archived bytes.
        digest: Digest,
    },
}

/// When a person last checked a source, and the proposal that said so (KN-8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reviewed {
    /// The day, `YYYY-MM-DD`, written as text.
    pub date: String,
    /// The accepted curation proposal (a UUID).
    pub proposal: Option<String>,
}

/// A source file (`knowledge/sources/<id>.yaml`; part 2 D-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The source's id: the file's name.
    pub id: SourceId,
    /// What kind of source it is.
    pub kind: SourceKind,
    /// Who and what it is.
    pub identity: Identity,
    /// Where it is found.
    pub reference: Option<Reference>,
    /// A copy kept.
    pub snapshot: Option<Snapshot>,
    /// When a person last checked it.
    pub last_reviewed: Option<Reviewed>,
    /// The source that replaces it (a newer edition, a correction).
    pub superseded_by: Option<SourceId>,
    /// Whether it can be had.
    pub availability: Availability,
}

impl Source {
    /// A source with nothing said about it yet.
    pub fn new(id: SourceId) -> Source {
        Source {
            id,
            kind: SourceKind::Other,
            identity: Identity::default(),
            reference: None,
            snapshot: None,
            last_reviewed: None,
            superseded_by: None,
            availability: Availability::Unknown,
        }
    }
}
