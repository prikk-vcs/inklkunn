//! What the appeals pages say with: the name of each kind of action, the sentences that say what a person may contest, what a
//! decision came to, and what each refusal says. **No name of anyone but the people the page is for, and never the words of an
//! appeal outside the two pages that are the appellant's and the deciders'.**

use inklkunn_core::appeal::{
    AppealError, ContestRef, ContestWhat, DeciderRefusal, DecisionRow, Outcome, OverturnRefusal,
    TextFault,
};
use inklkunn_store::AppealedKind;

use super::paths;
use crate::catalog::{Key, T};
use crate::people::sentences::role_label;
use crate::proposals::render::LinkView;
use crate::time::utc_minutes;
use crate::tombstone::{category_word, part_words};

/// What kind of action an appeal is against, as a phrase that follows *against*.
pub(crate) fn kind_label(t: T, kind: AppealedKind) -> String {
    t.get(match kind {
        AppealedKind::Decline => Key::AppealsKindDecline,
        AppealedKind::Tombstone => Key::AppealsKindTombstone,
        AppealedKind::Restriction => Key::AppealsKindRestriction,
        AppealedKind::Protection => Key::AppealsKindProtection,
        AppealedKind::Grant => Key::AppealsKindGrant,
    })
}

/// The link *Contest this* for an action: the form for it.
pub(crate) fn contest_link(t: T, slug: &str, what: &ContestRef) -> LinkView {
    LinkView {
        href: paths::contest(slug, what),
        label: t.get(Key::ContestLink),
    }
}

/// The way on a refusal by a restriction offers the restricted person: *contest this*, the form that appeals it. (Told to the
/// restricted person alone, as the restriction is.)
pub(crate) fn restriction_choice(
    t: T,
    slug: &str,
    notice: &inklkunn_core::RestrictionNotice,
) -> crate::wiki::Choice {
    let link = contest_link(
        t,
        slug,
        &ContestRef {
            kind: AppealedKind::Restriction,
            id: notice.id.to_string(),
        },
    );
    crate::wiki::Choice {
        label: link.label,
        href: link.href,
    }
}

/// *Contest this* for what moderation hid of something the person wrote: the link of the first of the hidings in force that the
/// person may appeal, or `None` (for a person it does not affect, and for one who has appealed it already). **Asked of the core,
/// which answers only the person the hiding affects.**
pub(crate) fn tombstone_contest(
    ctx: &crate::proposals::Ctx,
    t: T,
    hidden: Option<&inklkunn_core::tombstone::Hidden>,
) -> Option<LinkView> {
    ctx.account()?;
    hidden?.marks().iter().find_map(|mark| {
        let what = ContestRef {
            kind: AppealedKind::Tombstone,
            id: mark.event.to_string(),
        };
        matches!(
            ctx.services.appeals.offer(&ctx.principal, &ctx.wiki, &what),
            inklkunn_core::appeal::Offer::Contest
        )
        .then(|| contest_link(t, ctx.slug(), &what))
    })
}

/// What a person may contest, as sentences, and the link to the proposal when the action is about one.
pub(crate) fn what_lines(t: T, slug: &str, what: &ContestWhat) -> (Vec<String>, Option<LinkView>) {
    match what {
        ContestWhat::Decline { proposal } => (
            vec![t.get(Key::ContestWhatDecline)],
            Some(LinkView {
                href: crate::proposals::paths::proposal(slug, proposal.as_str()),
                label: t.get(Key::ContestProposalLink),
            }),
        ),
        ContestWhat::Tombstone {
            parts,
            category,
            at,
            ..
        } => (
            vec![
                t.msg(Key::ContestWhatTombstone)
                    .with("part", part_words(t, *parts).join(", "))
                    .with(
                        "category",
                        category.map_or_else(String::new, |c| category_word(t, c)),
                    )
                    .with("date", utc_minutes(*at))
                    .text(),
            ],
            None,
        ),
        ContestWhat::Restriction { until, reason } => (
            vec![
                t.msg(Key::ContestWhatRestriction)
                    .with("date", utc_minutes(*until))
                    .with("reason", reason.as_str())
                    .text(),
            ],
            None,
        ),
        ContestWhat::Protection {
            target,
            until,
            reason,
            ..
        } => (
            vec![
                t.msg(Key::ContestWhatProtection)
                    .with("target", target.as_str())
                    .with("date", utc_minutes(*until))
                    .with("reason", reason.as_str())
                    .text(),
            ],
            None,
        ),
        ContestWhat::Grant { before, after, at } => {
            let word = |role: &Option<inklkunn_store::Role>| match role {
                Some(role) => role_label(t, *role),
                None => t.get(Key::ContestNoRole),
            };
            (
                vec![
                    t.msg(Key::ContestWhatGrant)
                        .with("before", word(before))
                        .with("after", word(after))
                        .with("date", utc_minutes(*at))
                        .text(),
                ],
                None,
            )
        }
    }
}

/// How an appeal was decided, in one sentence with its reason.
pub(crate) fn decision_line(t: T, decision: &DecisionRow) -> String {
    t.msg(match decision.outcome {
        Outcome::Upheld => Key::AppealsDecisionUpheld,
        Outcome::Overturned => Key::AppealsDecisionOverturned,
    })
    .with("date", utc_minutes(decision.at))
    .with("person", decision.by.as_str())
    .with("reason", decision.reason.as_str())
    .text()
}

/// What an appeal said, as the lines it was written in (a blank line is no paragraph).
pub(crate) fn text_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// The sentence that says why an overturn was refused, and what is left to do.
pub(crate) fn overturn_refusal(t: T, why: &OverturnRefusal) -> String {
    t.get(match why {
        OverturnRefusal::RestrictionEnded => Key::AppealsMootRestriction,
        OverturnRefusal::ProtectionEnded => Key::AppealsMootProtection,
        OverturnRefusal::RoleChanged => Key::AppealsMootRole,
        OverturnRefusal::AboveReach { .. } => Key::AppealsMootReach,
        OverturnRefusal::ProposalMovedOn => Key::AppealsMootProposal,
        OverturnRefusal::StillHidden => Key::AppealsMootHidden,
        OverturnRefusal::HidingEnded => Key::AppealsMootHiding,
    })
}

/// The sentence that says why this person may not decide this appeal.
pub(crate) fn decider_refusal(t: T, why: DeciderRefusal) -> String {
    t.get(match why {
        DeciderRefusal::TheAppellant => Key::AppealsBlockedAppellant,
        DeciderRefusal::TheActor => Key::AppealsBlockedActor,
    })
}

/// The sentence of a refusal that is about what was typed in an appeal.
pub(crate) fn text_problem(t: T, fault: TextFault) -> String {
    match fault {
        TextFault::Empty => t.get(Key::ContestProblemEmpty),
        TextFault::TooLong => t
            .msg(Key::ContestProblemLong)
            .with("limit", inklkunn_core::appeal::MAX_TEXT_BYTES / 1024)
            .text(),
        TextFault::NotText => t.get(Key::ContestProblemChars),
    }
}

/// The sentence of a refusal that is about the reason of a decision, or `None` for every other refusal.
pub(crate) fn reason_problem(t: T, error: &AppealError) -> Option<String> {
    let AppealError::Reason(why) = error else {
        return None;
    };
    Some(match why {
        inklkunn_core::people::ReasonFault::Missing => t.get(Key::PeopleProblemReasonMissing),
        inklkunn_core::people::ReasonFault::TooLong => t
            .msg(Key::PeopleProblemReasonLong)
            .with("limit", inklkunn_core::people::MAX_REASON_CHARS)
            .text(),
        inklkunn_core::people::ReasonFault::NotOneLine => t.get(Key::PeopleProblemReasonLines),
    })
}
