//! What the people pages say with: the name of each role, the sentences that tell a person their restriction (their
//! reason and its end), and the sentence for each way an act of the people forms can be refused.

use inklkunn_core::people::{GrantRefusal, PeopleError, ReasonFault, RestrictRefusal};
use inklkunn_core::{HeldBy, Holding, RestrictionNotice};
use inklkunn_store::Role;

use crate::catalog::{Key, T};
use crate::time::utc_minutes;

/// The word for a role, in the reader's language.
pub(crate) fn role_label(t: T, role: Role) -> String {
    t.get(match role {
        Role::Observer => Key::RoleObserver,
        Role::Questioner => Key::RoleQuestioner,
        Role::Contributor => Key::RoleContributor,
        Role::Reviewer => Key::RoleReviewer,
        Role::Maintainer => Key::RoleMaintainer,
        Role::Admin => Key::RoleAdmin,
    })
}

/// **What a restriction tells the restricted person**: that they are restricted and what that means, the reason they were
/// given and the moment it ends. Three sentences, never more than the person's own restriction: this is said to them and to
/// no one else.
pub(crate) fn restriction_paragraphs(t: T, notice: &RestrictionNotice) -> Vec<String> {
    vec![
        t.get(Key::RestrictionNotice),
        t.msg(Key::RestrictionReason)
            .with("reason", notice.reason.as_str())
            .text(),
        t.msg(Key::RestrictionUntil)
            .with("date", utc_minutes(notice.until))
            .text(),
    ]
}

/// [`restriction_paragraphs`] and, after them, that nothing was changed: the page of an act that a restriction refused.
pub(crate) fn restriction_refusal(t: T, notice: &RestrictionNotice) -> Vec<String> {
    let mut out = restriction_paragraphs(t, notice);
    out.push(t.get(Key::RestrictionNothingChanged));
    out
}

/// What a person holds in a wiki and what gives it to them, as sentences: the role and its cause (a grant, or the wiki's
/// `entry`), or that they hold none, and the restriction when one holds them down. For the people page and the account
/// page: **never raw rules**.
pub(crate) fn holding_paragraphs(t: T, holding: &Holding) -> Vec<String> {
    let mut out = Vec::new();
    out.push(match (holding.held(), holding.held_by()) {
        (Some(role), HeldBy::Grant) => t
            .msg(Key::PeopleYouGrant)
            .with("role", role_label(t, role))
            .text(),
        (Some(role), HeldBy::Entry) => t
            .msg(Key::PeopleYouEntry)
            .with("role", role_label(t, role))
            .text(),
        (None, _) | (Some(_), HeldBy::Nothing) => t.get(Key::PeopleYouNone),
    });
    if let Some(notice) = &holding.restriction {
        out.extend(restriction_paragraphs(t, notice));
    }
    out
}

/// The sentence of a refusal that is about what was typed (shown beside the form), or `None` for a refusal that is a page of
/// its own (not allowed, restricted, the rules cannot be read, changed meanwhile, could not be done now).
pub(crate) fn problem(t: T, error: &PeopleError, up_to: Option<Role>) -> Option<String> {
    let key = match error {
        PeopleError::NoSuchAccount => Key::PeopleProblemNoAccount,
        PeopleError::Grant(why) => match why {
            GrantRefusal::OwnRung => Key::PeopleProblemOwnRole,
            GrantRefusal::NothingChanges => Key::PeopleProblemNothingChanges,
            GrantRefusal::LastAdmin => Key::PeopleProblemLastAdmin,
            GrantRefusal::AboveReach { .. } => {
                return Some(
                    t.msg(Key::PeopleProblemAboveReach)
                        .with("role", up_to.map_or_else(String::new, |r| role_label(t, r)))
                        .text(),
                );
            }
            GrantRefusal::NotAllowed => return None,
        },
        PeopleError::Restrict(why) => match why {
            RestrictRefusal::OwnAccount => Key::PeopleProblemOwnRestriction,
            RestrictRefusal::AdminIsNotRestricted => Key::PeopleProblemAdmin,
            RestrictRefusal::NotBelowYou => Key::PeopleProblemNotBelow,
            RestrictRefusal::NotRestricted => Key::PeopleProblemNotRestricted,
            RestrictRefusal::AlreadyRestricted { until } => {
                return Some(
                    t.msg(Key::PeopleProblemAlreadyRestricted)
                        .with("date", utc_minutes(*until))
                        .text(),
                );
            }
            RestrictRefusal::DaysOutOfRange { limit_days } => {
                return Some(
                    t.msg(Key::PeopleProblemDays)
                        .with("limit", *limit_days)
                        .text(),
                );
            }
            RestrictRefusal::NotAllowed => return None,
        },
        PeopleError::Reason(why) => match why {
            ReasonFault::Missing => Key::PeopleProblemReasonMissing,
            ReasonFault::TooLong => {
                return Some(
                    t.msg(Key::PeopleProblemReasonLong)
                        .with("limit", inklkunn_core::people::MAX_REASON_CHARS)
                        .text(),
                );
            }
            ReasonFault::NotOneLine => Key::PeopleProblemReasonLines,
        },
        PeopleError::NotFound
        | PeopleError::NotAllowed
        | PeopleError::Restricted(_)
        | PeopleError::RulesUnreadable(_)
        | PeopleError::Changed
        | PeopleError::Unavailable(_) => return None,
    };
    Some(t.get(key))
}
