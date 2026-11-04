//! The words for the knowledge layer's enumerations (RFC 005 part 2 D-1, D-2): which catalog message says each
//! value of the core's closed lists, in a form's options and in a sentence. Code, not words: every sentence is a
//! catalog message, and **no word here ranks** (the retired-words test holds the catalog to that).

use inklkunn_core::knowledge::{
    Availability, Bearing, Lifecycle, Link, ReferenceForm, RelationType, SourceKind, Standing,
    StatementKind,
};

use crate::catalog::Key;

/// A choice in a select: its value (the core's word), its label and whether it is the one chosen.
pub(crate) struct OptionView {
    pub(crate) value: &'static str,
    pub(crate) label: String,
    pub(crate) selected: bool,
}

/// Every value of a closed list as the options of a select.
pub(crate) fn options<E: Copy + PartialEq>(
    all: &[E],
    word: fn(E) -> &'static str,
    key: fn(E) -> Key,
    chosen: E,
    label: impl Fn(Key) -> String,
) -> Vec<OptionView> {
    all.iter()
        .map(|e| OptionView {
            value: word(*e),
            label: label(key(*e)),
            selected: *e == chosen,
        })
        .collect()
}

// --- in a form's options

pub(crate) const fn source_kind_option(k: SourceKind) -> Key {
    match k {
        SourceKind::Primary => Key::AttachKindPrimary,
        SourceKind::Secondary => Key::AttachKindSecondary,
        SourceKind::Observational => Key::AttachKindObservational,
        SourceKind::Computational => Key::AttachKindComputational,
        SourceKind::Anecdotal => Key::AttachKindAnecdotal,
        SourceKind::Other => Key::AttachKindOther,
    }
}

pub(crate) const fn reference_option(f: ReferenceForm) -> Key {
    match f {
        ReferenceForm::Url => Key::AttachRefUrl,
        ReferenceForm::Doi => Key::AttachRefDoi,
        ReferenceForm::Isbn => Key::AttachRefIsbn,
        ReferenceForm::Page => Key::AttachRefPage,
        ReferenceForm::Citation => Key::AttachRefCitation,
    }
}

pub(crate) const fn bearing_option(b: Bearing) -> Key {
    match b {
        Bearing::Direct => Key::AttachBearingDirect,
        Bearing::Partial => Key::AttachBearingPartial,
        Bearing::Indirect => Key::AttachBearingIndirect,
    }
}

pub(crate) const fn statement_option(k: StatementKind) -> Key {
    match k {
        StatementKind::Observation => Key::AttachClaimKindObservation,
        StatementKind::Interpretation => Key::AttachClaimKindInterpretation,
        StatementKind::Hypothesis => Key::AttachClaimKindHypothesis,
        StatementKind::Opinion => Key::AttachClaimKindOpinion,
        StatementKind::Decision => Key::AttachClaimKindDecision,
        StatementKind::Policy => Key::AttachClaimKindPolicy,
        StatementKind::Question => Key::AttachClaimKindQuestion,
        StatementKind::Unknown => Key::AttachClaimKindUnknown,
    }
}

// --- in a sentence

pub(crate) const fn statement_word(k: StatementKind) -> Key {
    match k {
        StatementKind::Observation => Key::KnowKindObservation,
        StatementKind::Interpretation => Key::KnowKindInterpretation,
        StatementKind::Hypothesis => Key::KnowKindHypothesis,
        StatementKind::Opinion => Key::KnowKindOpinion,
        StatementKind::Decision => Key::KnowKindDecision,
        StatementKind::Policy => Key::KnowKindPolicy,
        StatementKind::Question => Key::KnowKindQuestion,
        StatementKind::Unknown => Key::KnowKindUnknown,
    }
}

pub(crate) const fn source_word(k: SourceKind) -> Key {
    match k {
        SourceKind::Primary => Key::KnowSourcePrimary,
        SourceKind::Secondary => Key::KnowSourceSecondary,
        SourceKind::Observational => Key::KnowSourceObservational,
        SourceKind::Computational => Key::KnowSourceComputational,
        SourceKind::Anecdotal => Key::KnowSourceAnecdotal,
        SourceKind::Other => Key::KnowSourceOther,
    }
}

pub(crate) const fn link_word(l: Link) -> Key {
    match l {
        Link::Supports => Key::KnowSupports,
        Link::Challenges => Key::KnowChallenges,
    }
}

pub(crate) const fn bearing_word(b: Bearing) -> Key {
    match b {
        Bearing::Direct => Key::KnowDirectly,
        Bearing::Partial => Key::KnowInPart,
        Bearing::Indirect => Key::KnowByImplication,
    }
}

pub(crate) const fn lifecycle_word(l: Lifecycle) -> Key {
    match l {
        Lifecycle::Proposed => Key::KnowLifeProposed,
        Lifecycle::Adopted => Key::KnowLifeAdopted,
        Lifecycle::Retired => Key::KnowLifeRetired,
        Lifecycle::Unknown => Key::KnowLifeUnknown,
    }
}

pub(crate) const fn standing_word(s: Standing) -> Key {
    match s {
        Standing::Accepted => Key::KnowStandingAccepted,
        Standing::Contested => Key::KnowStandingContested,
        Standing::Unresolved => Key::KnowStandingUnresolved,
        Standing::Unknown => Key::KnowStandingUnknown,
    }
}

pub(crate) const fn availability_word(a: Availability) -> Key {
    match a {
        Availability::Available => Key::KnowAvailable,
        Availability::Unavailable => Key::KnowUnavailable,
        Availability::Unknown => Key::KnowAvailabilityUnknown,
    }
}

pub(crate) const fn relation_type_word(r: RelationType) -> Key {
    match r {
        RelationType::Refines => Key::KnowRelRefines,
        RelationType::Supersedes => Key::KnowRelSupersedes,
        RelationType::Contradicts => Key::KnowRelContradicts,
        RelationType::SplitFrom => Key::KnowRelSplitFrom,
        RelationType::MergedInto => Key::KnowRelMergedInto,
    }
}
