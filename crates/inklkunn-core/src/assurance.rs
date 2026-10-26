//! The honesty view-model (RFC 002 D-6; requirements ID-5, ID-9; planeter's Assurance vocabulary). A
//! surface shows one of these words and never another; the catalog carries the texts.

/// What the reader may believe about a thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Assurance {
    /// `verify` ran now and every condition held (the repository; re-derived, never cached).
    Verified,
    /// `verify` ran now and a condition failed.
    VerificationFailed,
    /// `verify` could not run to completion: not a verdict (the repository).
    CouldNotBeCheckedNow,
    /// A change read from the sealed history.
    SignedIntoTheHistory,
    /// An open proposal.
    HeldForReview,
    /// A proposal that was accepted.
    Accepted,
    /// A proposal that was declined.
    Declined,
}

impl Assurance {
    /// Every value.
    pub const ALL: [Assurance; 7] = [
        Assurance::Verified,
        Assurance::VerificationFailed,
        Assurance::CouldNotBeCheckedNow,
        Assurance::SignedIntoTheHistory,
        Assurance::HeldForReview,
        Assurance::Accepted,
        Assurance::Declined,
    ];

    /// The catalog key (the English text is the catalog's, RFC 003's).
    pub const fn key(self) -> &'static str {
        match self {
            Assurance::Verified => "assurance.verified",
            Assurance::VerificationFailed => "assurance.verification-failed",
            Assurance::CouldNotBeCheckedNow => "assurance.could-not-be-checked-now",
            Assurance::SignedIntoTheHistory => "assurance.signed-into-the-history",
            Assurance::HeldForReview => "assurance.held-for-review",
            Assurance::Accepted => "assurance.accepted",
            Assurance::Declined => "assurance.declined",
        }
    }

    /// Is this a statement about the repository's integrity (as opposed to a proposal's state)?
    pub const fn is_about_the_repository(self) -> bool {
        matches!(
            self,
            Assurance::Verified | Assurance::VerificationFailed | Assurance::CouldNotBeCheckedNow
        )
    }

    /// From the store's recorded verification state.
    pub fn from_verification(state: inklkunn_store::VerificationState) -> Option<Self> {
        use inklkunn_store::VerificationState as V;
        match state {
            V::Verified => Some(Assurance::Verified),
            V::Failed => Some(Assurance::VerificationFailed),
            V::CouldNotCheck => Some(Assurance::CouldNotBeCheckedNow),
            V::Unknown => None,
        }
    }
}
