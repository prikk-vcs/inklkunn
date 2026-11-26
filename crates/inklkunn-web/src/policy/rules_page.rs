//! The policy page of a wiki (RFC 006 D-11, GV-3, WEB-12): **where governance is visible**. The rules of this wiki in sentences,
//! never raw YAML and never the word *official*; why they are what they are (each change of the rules with the proposal that made
//! it); the limits the server puts on any wiki, and the note when one lowers a value the wiki's own rules say; the protections in
//! force, the policy's and the moderators', in the one display; the approvals warning; and the ways to the people, the
//! moderation log and the appeals.
//!
//! **Authorized as a wiki's pages are** (`read`) and existence-neutral: a wiki that is absent and a wiki that may not be read are
//! one page. A part that could not be read now says so, and the rest of the page is whole. **While a file of the rules cannot be
//! read** the page says which file, where, and what repairs it (the sentences of the unreadable-rules page), and lists only what
//! the default rules say, which is what decides until the repair.

use askama::Template;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::ProposalError;
use inklkunn_core::governance::{Lowered, Require, SelfAcceptance};
use inklkunn_core::read::{RuleChange, RuleHidden, WALK_BLOCKS};
use inklkunn_store::{ProposalId, Visibility};

use super::rules_change::{Compared, file_lines};
use super::rules_data::{RulesData, gather};
use super::rules_paths as paths;
use super::rules_phrases::{self as phrases, Fact};
use crate::catalog::{Key, Rich, T};
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::governance::{fault_text, unreadable_paragraphs};
use crate::proposals::{Ctx, serve};
use crate::wiki::{Shell, generic_not_found, services_of};

type PathName = Result<Path<String>, PathRejection>;

struct LineView {
    label: String,
    value: String,
    note: Option<String>,
}

struct SectionView {
    id: &'static str,
    heading: String,
    intro: Option<String>,
    lines: Vec<LineView>,
    notes: Vec<String>,
}

struct HistoryRow {
    head: Rich,
    lines: Vec<String>,
}

struct LinkView {
    href: String,
    label: String,
}

#[derive(Template)]
#[template(path = "policy.html")]
struct PolicyTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    unreadable: Option<(String, Vec<String>)>,
    source: Option<Rich>,
    defaults: Option<String>,
    warning: Option<Rich>,
    sections: Vec<SectionView>,
    protections_heading: String,
    protections_intro: String,
    protections: Vec<crate::protections::NoticeView>,
    protections_notes: Vec<String>,
    history_heading: String,
    history_intro: String,
    history: Vec<HistoryRow>,
    history_notes: Vec<String>,
    links: Vec<LinkView>,
}

fn line(fact: Fact) -> LineView {
    LineView {
        label: fact.label,
        value: fact.value,
        note: None,
    }
}

/// The facts of the policy that say who may do what.
fn is_who(fact: &Fact) -> bool {
    fact.paths
        .first()
        .is_some_and(|p| p.starts_with("rungs.") || *p == "entry")
}

fn is_promotion(fact: &Fact) -> bool {
    fact.paths
        .first()
        .is_some_and(|p| p.starts_with("promotion."))
}

/// The sections that say the rules: who may do what, how a proposal is accepted, what the wiki looks for before granting a role,
/// and how the rules are changed.
fn sections(t: T, ctx: &Ctx, data: &RulesData) -> Vec<SectionView> {
    let rules = &data.rules;
    let facts = phrases::policy_facts(t, rules.policy(), data.server_days);
    let (who, rest): (Vec<Fact>, Vec<Fact>) = facts.into_iter().partition(is_who);
    let (promotion, accept): (Vec<Fact>, Vec<Fact>) = rest.into_iter().partition(is_promotion);

    let restricted = ctx.wiki.visibility == Visibility::Restricted;
    let mut who_lines: Vec<LineView> = who.into_iter().map(line).collect();
    // `entry` gives nothing in a wiki that only the people granted a role can read: said beside it.
    if let Some(entry) = who_lines
        .iter_mut()
        .find(|l| l.label == t.get(Key::PolicyLabelEntry))
        && restricted
        && rules.policy().entry().rung().is_some()
    {
        entry.note = Some(t.get(Key::PolicyEntryNoEffect));
    }
    let mut who_notes = Vec::new();
    for lowered in rules.lowered() {
        match lowered {
            Lowered::Entry { was, now } => who_notes.push(
                t.msg(Key::PolicyLoweredEntry)
                    .with("was", phrases::entry_value(t, *was))
                    .with("now", phrases::entry_value(t, *now))
                    .text(),
            ),
        }
    }

    let held: Vec<LineView> = promotion
        .into_iter()
        .filter(|f| {
            rules
                .policy()
                .promotions()
                .any(|(role, _)| phrases::promotion_paths(role) == f.paths)
        })
        .map(line)
        .collect();
    let mut promotion_notes = vec![t.get(Key::PolicyPromotionNote)];
    if held.is_empty() {
        promotion_notes.push(t.get(Key::PolicyPromotionNone));
    }

    let mut change_notes = vec![t.get(Key::PolicyHatchNote)];
    if let Some(warning) = &data.warning {
        change_notes.push(
            t.msg(Key::ProposalApprovalsUnreachable)
                .n(u64::from(warning.needed))
                .text(),
        );
    }
    vec![
        SectionView {
            id: "who",
            heading: t.get(Key::PolicyWhoHeading),
            intro: None,
            lines: who_lines,
            notes: who_notes,
        },
        SectionView {
            id: "accept",
            heading: t.get(Key::PolicyAcceptHeading),
            intro: None,
            lines: accept.into_iter().map(line).collect(),
            notes: Vec::new(),
        },
        SectionView {
            id: "promotion",
            heading: t.get(Key::PolicyPromotionHeading),
            intro: None,
            lines: held,
            notes: promotion_notes,
        },
        SectionView {
            id: "change",
            heading: t.get(Key::PolicyChangeHeading),
            intro: None,
            lines: phrases::constitution_facts(
                t,
                rules.constitution(),
                rules.policy().acceptance().approvals(),
            )
            .into_iter()
            .map(line)
            .collect(),
            notes: change_notes,
        },
        interwiki_section(t, data),
        SectionView {
            id: "limits",
            heading: t.get(Key::PolicyLimitsHeading),
            intro: Some(t.get(Key::PolicyLimitsIntro)),
            lines: Vec::new(),
            notes: vec![
                t.msg(Key::PolicyLimitEntry)
                    .with("role", phrases::entry_value(t, data.limits.entry_max))
                    .text(),
                t.msg(Key::PolicyLimitDays)
                    .with("days", data.limits.limit_days)
                    .text(),
            ],
        },
    ]
}

/// The interwiki map, read-only: each prefix and the address it leads to (RFC 007 D-8). It is changed by a proposal made with the policy form (W-6), like
/// the rest of the rules, and this page is where it is shown. **While the map is the file that cannot be read** nothing is listed
/// as if the wiki had none: the section says it could not be read.
fn interwiki_section(t: T, data: &RulesData) -> SectionView {
    let broken = data
        .rules
        .fault()
        .is_some_and(|f| f.file() == inklkunn_core::governance::GovernanceFile::Interwiki);
    let (lines, notes) = match (&data.interwiki, broken) {
        (Some(map), false) if !map.is_empty() => (
            map.iter()
                .map(|(prefix, template)| LineView {
                    label: prefix.to_owned(),
                    value: template.to_owned(),
                    note: None,
                })
                .collect(),
            Vec::new(),
        ),
        (Some(_), false) => (Vec::new(), vec![t.get(Key::PolicyInterwikiNone)]),
        _ => (Vec::new(), vec![t.get(Key::PolicyInterwikiUnavailable)]),
    };
    SectionView {
        id: "interwiki",
        heading: t.get(Key::PolicyInterwikiHeading),
        intro: Some(t.get(Key::PolicyInterwikiIntro)),
        lines,
        notes,
    }
}

/// The label of a proposal as a link names it: its summary, or the words that stand in for one that moderation hides, that has
/// none, or that **cannot be known not to be hidden** (the store that says what is hidden failed): then the tombstone's place is
/// shown, never the text.
fn proposal_label(ctx: &Ctx, t: T, id: &ProposalId) -> String {
    let service = &ctx.services.proposals;
    // What moderation hides is asked first and on its own, so that no failure after it can put the text back.
    let hidden = match service.hidden_of(&ctx.principal, &ctx.wiki, id) {
        Ok(hidden) => hidden.summary.is_some(),
        Err(ProposalError::Store(_)) => true,
        Err(_) => return t.get(Key::ProposalAnother),
    };
    if hidden {
        return t.get(Key::TombstoneSummaryHidden);
    }
    match service.get(&ctx.principal, &ctx.wiki, id) {
        Ok(record) if record.summary.is_empty() => t.get(Key::ProposalNoSummary),
        Ok(record) => record.summary,
        Err(ProposalError::Store(_)) => t.get(Key::TombstoneSummaryHidden),
        Err(_) => t.get(Key::ProposalAnother),
    }
}

/// One change of the rules: where it is, the proposal that made it, and what it did, in sentences.
fn history_row(ctx: &Ctx, t: T, data: &RulesData, change: &RuleChange) -> HistoryRow {
    let slug = ctx.slug();
    let change_link = |label: &str| {
        t.msg(Key::PolicyHistoryRowAlone)
            .link(
                "change",
                label,
                crate::url::change_path(slug, None, &change.block_id),
            )
            .html()
    };
    let short = inklkunn_core::governance::short_block(&change.block_id);
    let head = match change.proposal.as_deref().and_then(ProposalId::parse) {
        Some(id) => t
            .msg(Key::PolicyHistoryRow)
            .link(
                "change",
                short,
                crate::url::change_path(slug, None, &change.block_id),
            )
            .link(
                "proposal",
                proposal_label(ctx, t, &id),
                crate::proposals::paths::proposal(slug, id.as_str()),
            )
            .html(),
        None => change_link(short),
    };
    // A change whose text moderation hides says that, and nothing of what it did.
    if let Some(hidden) = change.hidden {
        let line = match hidden {
            RuleHidden::ThisChange => t.get(Key::PolicyHistoryHidden),
            RuleHidden::EarlierChange => t.get(Key::TombstoneDiffFrom),
        };
        return HistoryRow {
            head,
            lines: vec![line],
        };
    }
    let mut lines = Vec::new();
    for file in &change.files {
        match file_lines(
            t,
            file.file,
            file.before.as_deref(),
            file.after.as_deref(),
            data.server_days,
        ) {
            Compared::Lines(found) => lines.extend(found),
            Compared::Unreadable(fault) => lines.push(fault_text(t, &fault)),
        }
        if file.after.is_none() {
            lines.push(t.get(Key::GovernanceChangeDefaults));
        }
    }
    if change.files.is_empty() {
        lines.push(t.get(Key::PolicyHistoryRepair));
    } else if lines.is_empty() {
        lines.push(t.get(Key::GovernanceChangeNone));
    }
    HistoryRow { head, lines }
}

fn render(page: &Page, ctx: &Ctx, data: &RulesData) -> Response {
    let t = page.t();
    let slug = ctx.slug();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let rules = &data.rules;

    let unreadable = rules.fault().map(|fault| {
        (t.get(Key::PolicyUnreadableHeading), {
            let mut paragraphs = unreadable_paragraphs(t, fault);
            paragraphs.push(t.get(Key::PolicyUnreadableDefaults));
            paragraphs
        })
    });
    let source = match (rules.fault(), rules.block()) {
        (None, Some(block)) => Some(
            t.msg(Key::PolicyAsOf)
                .link(
                    "change",
                    inklkunn_core::governance::short_block(block),
                    crate::url::change_path(slug, None, block),
                )
                .html(),
        ),
        _ => None,
    };
    let defaults = (rules.fault().is_none() && rules.block().is_none())
        .then(|| t.get(Key::PolicyDefaultsNote));
    let warning = data.warning.map(|w| {
        t.msg(Key::ProposalApprovalsUnreachable)
            .n(u64::from(w.needed))
            .html()
    });

    // The protections, in the one display, and what a review asks of a proposer who accepts their own proposal.
    let mut protections_notes = Vec::new();
    let protections = match &data.notices {
        None => {
            protections_notes.push(t.get(Key::PolicyProtectionsUnavailable));
            Vec::new()
        }
        Some(notices) if notices.is_empty() => {
            protections_notes.push(t.get(Key::PolicyProtectionsNone));
            Vec::new()
        }
        Some(notices) => {
            if notices.iter().any(|n| n.require() == Require::Review)
                && rules.policy().acceptance().self_acceptance() == SelfAcceptance::SmallWiki
            {
                protections_notes.push(t.get(Key::PolicyProtectionsSelfNote));
            }
            crate::protections::views(t, slug, notices)
        }
    };

    // The history of the rules.
    let mut history_notes = Vec::new();
    let history = match &data.history {
        None => {
            history_notes.push(t.get(Key::PolicyHistoryUnavailable));
            Vec::new()
        }
        Some(found) => {
            if found.changes.is_empty() {
                history_notes.push(t.get(Key::PolicyHistoryNone));
            }
            if found.more > 0 {
                history_notes.push(t.msg(Key::PolicyHistoryMore).n(found.more as u64).text());
            }
            if found.window_ended {
                history_notes.push(
                    t.msg(Key::PolicyHistoryWindow)
                        .with("blocks", WALK_BLOCKS)
                        .text(),
                );
            }
            found
                .changes
                .iter()
                .map(|change| history_row(ctx, t, data, change))
                .collect()
        }
    };

    let mut links = vec![
        LinkView {
            href: paths::edit(slug),
            label: t.get(Key::PolicyLinkChange),
        },
        LinkView {
            href: crate::people::paths::page(slug),
            label: t.get(Key::AboutPeopleLink),
        },
        LinkView {
            href: crate::tombstone::paths::log(slug),
            label: t.get(Key::PolicyLinkLog),
        },
    ];
    if ctx.account().is_some() {
        links.push(LinkView {
            href: crate::appeals::paths::mine(slug),
            label: t.get(Key::AppealsMineLink),
        });
    }
    if ctx.services.appeals.may_decide(&ctx.principal, &ctx.wiki) {
        links.push(LinkView {
            href: crate::appeals::paths::page(slug),
            label: t.get(Key::AppealsHeading),
        });
    }

    let rendered = PolicyTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitlePolicy)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::PolicyHeading),
        intro: t.get(Key::PolicyIntro),
        unreadable,
        source,
        defaults,
        warning,
        sections: sections(t, ctx, data),
        protections_heading: t.get(Key::PolicyProtectionsHeading),
        protections_intro: t.get(Key::PolicyProtectionsIntro),
        protections,
        protections_notes,
        history_heading: t.get(Key::PolicyHistoryHeading),
        history_intro: t.get(Key::PolicyHistoryIntro),
        history,
        history_notes,
        links,
    }
    .render();
    answer(StatusCode::OK, &head, rendered)
}

/// `GET /{slug}/-/policy`.
pub(crate) async fn policy_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: PathName,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        match gather(&services, &ctx.wiki, &ctx.principal, true) {
            Ok(data) => render(page, &ctx, &data),
            Err(error) => {
                ctx.log(&format!("the rules could not be read now: {error}"));
                crate::wiki::with_retry_after(crate::wiki::notice(
                    page,
                    &ctx.wiki,
                    StatusCode::SERVICE_UNAVAILABLE,
                    Key::UnavailableHeading,
                    vec![page.get(Key::UnavailableBody)],
                    Vec::new(),
                    None,
                ))
            }
        }
    })
    .await
}
