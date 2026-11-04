//! What each word of a claim's file is called on its page (RFC 005 part 2 D-7): the closed enumerations of the
//! format, each to its catalog message. A `match` with no wildcard on the core's own enums, so a word the format
//! gains cannot reach a reader unnamed. A word the index holds that the format does not know (a stored row from
//! another build) is said as *not said*, never made up.

use inklkunn_core::knowledge::{Bearing, Link, SourceKind, Standing, StatementKind};

use crate::catalog::Key;

/// The kind of statement a claim is.
pub(crate) fn kind(word: &str) -> Key {
    match StatementKind::from_word(word) {
        Some(StatementKind::Observation) => Key::ClaimKindObservation,
        Some(StatementKind::Interpretation) => Key::ClaimKindInterpretation,
        Some(StatementKind::Hypothesis) => Key::ClaimKindHypothesis,
        Some(StatementKind::Opinion) => Key::ClaimKindOpinion,
        Some(StatementKind::Decision) => Key::ClaimKindDecision,
        Some(StatementKind::Policy) => Key::ClaimKindPolicy,
        Some(StatementKind::Question) => Key::ClaimKindQuestion,
        Some(StatementKind::Unknown) | None => Key::ClaimKindUnknown,
    }
}

/// The kind of a source.
pub(crate) fn source_kind(word: &str) -> Key {
    match SourceKind::from_word(word) {
        Some(SourceKind::Primary) => Key::ClaimSourcePrimary,
        Some(SourceKind::Secondary) => Key::ClaimSourceSecondary,
        Some(SourceKind::Observational) => Key::ClaimSourceObservational,
        Some(SourceKind::Computational) => Key::ClaimSourceComputational,
        Some(SourceKind::Anecdotal) => Key::ClaimSourceAnecdotal,
        Some(SourceKind::Other) | None => Key::ClaimSourceOther,
    }
}

/// How a source bears on a claim, in one phrase (*supports, directly*; *challenges, in part*): the relation and
/// how directly the source speaks to the claim, and nothing about how good either is. `None` for a relation or a
/// bearing this build does not know, so that the page leaves the phrase out rather than guess one.
pub(crate) fn how(relation: &str, bearing: &str) -> Option<Key> {
    Some(
        match (Link::from_word(relation)?, Bearing::from_word(bearing)?) {
            (Link::Supports, Bearing::Direct) => Key::ClaimHowSupportsDirect,
            (Link::Supports, Bearing::Partial) => Key::ClaimHowSupportsPartial,
            (Link::Supports, Bearing::Indirect) => Key::ClaimHowSupportsIndirect,
            (Link::Challenges, Bearing::Direct) => Key::ClaimHowChallengesDirect,
            (Link::Challenges, Bearing::Partial) => Key::ClaimHowChallengesPartial,
            (Link::Challenges, Bearing::Indirect) => Key::ClaimHowChallengesIndirect,
        },
    )
}

/// What the community has decided, as the sentence the status section opens with.
pub(crate) fn status(word: &str) -> (Standing, Key) {
    match Standing::from_word(word) {
        Some(Standing::Accepted) => (Standing::Accepted, Key::ClaimStatusAccepted),
        Some(Standing::Contested) => (Standing::Contested, Key::ClaimStatusContested),
        Some(Standing::Unresolved) => (Standing::Unresolved, Key::ClaimStatusUnresolved),
        Some(Standing::Unknown) | None => (Standing::Unknown, Key::ClaimStatusUnknown),
    }
}
