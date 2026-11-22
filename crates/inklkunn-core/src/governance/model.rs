//! The closed enumerations of the policy and the pieces it is made of. Each enumeration is the schema's own list of
//! words ([`word`](Entry::word) is what the file says, and [`parse`](Entry::parse) reads exactly that word, in lower
//! case, and nothing else).

use inklkunn_store::Role;

use super::defaults;

/// Give a closed enumeration its words: `ALL`, `word` and `parse`.
macro_rules! words {
    ($ty:ident { $($variant:ident => $word:literal),+ $(,)? }) => {
        impl $ty {
            /// Every word, lowest first.
            pub const ALL: &'static [$ty] = &[$($ty::$variant),+];

            /// The word a file says.
            pub const fn word(self) -> &'static str {
                match self {
                    $($ty::$variant => $word),+
                }
            }

            /// The value a word names, if it names one: exactly the word, in lower case.
            pub fn parse(word: &str) -> Option<$ty> {
                $ty::ALL.iter().copied().find(|v| v.word() == word)
            }
        }
    };
}

/// `entry`: the rung every signed-in account holds in a wiki without a grant (RFC 006 D-2). It is **a rung or
/// nothing**: [`Entry::None`] is no rung at all, and is not the same as observer, which is a rung (an observer is an
/// account the wiki has taken in and may be restricted to).
///
/// Ordered as the ladder is, with no rung lowest; the platform's `INKLKUNN_GOVERNANCE_ENTRY_MAX` is one of these
/// ([`PlatformLimits`](super::PlatformLimits)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entry {
    /// No rung: an account holds only what it was granted (0.3.0's behaviour, and the default).
    None,
    /// Every signed-in account is an observer.
    Observer,
    /// Every signed-in account is a questioner.
    Questioner,
    /// Every signed-in account is a contributor.
    Contributor,
}

words!(Entry {
    None => "none",
    Observer => "observer",
    Questioner => "questioner",
    Contributor => "contributor",
});

impl Entry {
    /// The rung this entry gives, or `None` for [`Entry::None`].
    pub const fn rung(self) -> Option<Role> {
        match self {
            Entry::None => None,
            Entry::Observer => Some(Role::Observer),
            Entry::Questioner => Some(Role::Questioner),
            Entry::Contributor => Some(Role::Contributor),
        }
    }
}

/// `rungs.grant_up_to`: the highest rung a maintainer may grant (RFC 006 D-2). Admins grant any rung whatever this
/// says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GrantUpTo {
    /// A maintainer grants nothing: only admins grant (0.3.0's behaviour, and the default).
    None,
    /// Up to observer.
    Observer,
    /// Up to questioner.
    Questioner,
    /// Up to contributor.
    Contributor,
    /// Up to reviewer.
    Reviewer,
}

words!(GrantUpTo {
    None => "none",
    Observer => "observer",
    Questioner => "questioner",
    Contributor => "contributor",
    Reviewer => "reviewer",
});

impl GrantUpTo {
    /// The highest rung a maintainer may grant, or `None` when a maintainer may grant none.
    pub const fn highest(self) -> Option<Role> {
        match self {
            GrantUpTo::None => None,
            GrantUpTo::Observer => Some(Role::Observer),
            GrantUpTo::Questioner => Some(Role::Questioner),
            GrantUpTo::Contributor => Some(Role::Contributor),
            GrantUpTo::Reviewer => Some(Role::Reviewer),
        }
    }

    /// May a maintainer grant `rung`?
    pub fn maintainer_may_grant(self, rung: Role) -> bool {
        self.highest().is_some_and(|highest| rung <= highest)
    }
}

/// `rungs.decide_appeal`: who decides an appeal (RFC 006 D-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppealRung {
    /// A maintainer decides.
    Maintainer,
    /// An admin decides (the default).
    Admin,
}

words!(AppealRung {
    Maintainer => "maintainer",
    Admin => "admin",
});

impl AppealRung {
    /// The lowest rung that decides an appeal.
    pub const fn role(self) -> Role {
        match self {
            AppealRung::Maintainer => Role::Maintainer,
            AppealRung::Admin => Role::Admin,
        }
    }
}

/// `acceptance.self`: whether a maintainer may accept their own proposal (RFC 004's bootstrap rule, RFC 006 D-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SelfAcceptance {
    /// Never: a proposal is accepted by someone other than its proposer.
    Never,
    /// While the wiki has fewer than [`defaults::SELF_ACCEPTANCE_BELOW`] maintainers (the default, 0.3.0's rule).
    SmallWiki,
}

words!(SelfAcceptance {
    Never => "never",
    SmallWiki => "small-wiki",
});

impl SelfAcceptance {
    /// May a maintainer accept their own proposal in a wiki with `maintainers` maintainers? (The maintainer count
    /// is the caller's: this is the rule, not the count.)
    pub fn allows(self, maintainers: usize) -> bool {
        match self {
            SelfAcceptance::Never => false,
            SelfAcceptance::SmallWiki => maintainers < defaults::SELF_ACCEPTANCE_BELOW,
        }
    }
}

/// The lowest rung for each action the policy sets (`rungs.*`, RFC 006 D-2). The ordering rule holds for every
/// `Rungs` a [`Policy`](super::Policy) carries: a policy that breaks it is never made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rungs {
    pub(super) comment: Role,
    pub(super) propose: Role,
    pub(super) review: Role,
    pub(super) create_view: Role,
    pub(super) accept: Role,
    pub(super) moderate: Role,
    pub(super) decide_appeal: AppealRung,
    pub(super) grant_up_to: GrantUpTo,
}

impl Default for Rungs {
    fn default() -> Self {
        Rungs {
            comment: defaults::COMMENT,
            propose: defaults::PROPOSE,
            review: defaults::REVIEW,
            create_view: defaults::CREATE_VIEW,
            accept: defaults::ACCEPT,
            moderate: defaults::MODERATE,
            decide_appeal: defaults::DECIDE_APPEAL,
            grant_up_to: defaults::GRANT_UP_TO,
        }
    }
}

impl Rungs {
    /// `rungs.comment`.
    pub const fn comment(&self) -> Role {
        self.comment
    }

    /// `rungs.propose`.
    pub const fn propose(&self) -> Role {
        self.propose
    }

    /// `rungs.review`.
    pub const fn review(&self) -> Role {
        self.review
    }

    /// `rungs.create_view`.
    pub const fn create_view(&self) -> Role {
        self.create_view
    }

    /// `rungs.accept`.
    pub const fn accept(&self) -> Role {
        self.accept
    }

    /// `rungs.moderate`.
    pub const fn moderate(&self) -> Role {
        self.moderate
    }

    /// `rungs.decide_appeal`.
    pub const fn decide_appeal(&self) -> AppealRung {
        self.decide_appeal
    }

    /// `rungs.grant_up_to`.
    pub const fn grant_up_to(&self) -> GrantUpTo {
        self.grant_up_to
    }
}

/// `acceptance.*`: how a proposal is accepted (RFC 006 D-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Acceptance {
    pub(super) approvals: u8,
    pub(super) self_acceptance: SelfAcceptance,
}

impl Default for Acceptance {
    fn default() -> Self {
        Acceptance {
            approvals: defaults::APPROVALS,
            self_acceptance: defaults::SELF_ACCEPTANCE,
        }
    }
}

impl Acceptance {
    /// `acceptance.approvals`: how many approvals, from people other than the proposer, a proposal needs before it
    /// is accepted (0 to [`MAX_APPROVALS`](super::MAX_APPROVALS)). How they are counted is RFC 004's.
    pub const fn approvals(&self) -> u8 {
        self.approvals
    }

    /// `acceptance.self`.
    pub const fn self_acceptance(&self) -> SelfAcceptance {
        self.self_acceptance
    }

    /// May a maintainer accept their own proposal in a wiki with `maintainers` maintainers?
    pub fn may_self_accept(&self, maintainers: usize) -> bool {
        self.self_acceptance.allows(maintainers)
    }
}
