//! The form that proposes a change to the rules of a wiki (RFC 006 D-4; handoff G-T7; threat model C-3d, INV-14).
//!
//! **Each key is a sentence and a control, never raw YAML**: the labels are the policy page's own ([`super::rules_phrases`]), the
//! controls are selects for the rungs and the enumerations, numbers, one line of text for each promotion, a list of protections
//! with add and remove buttons, the constitution's three settings, and **the interwiki map as a list of rows, a prefix and the
//! address it leads to, with the same add and remove buttons** (RFC 007 D-8, W-6). It is filled from the rules in force.
//!
//! **What it does is only to make a proposal.** What was typed becomes a draft ([`PolicyDraft`], [`ConstitutionDraft`],
//! [`InterwikiDraft`]), which is written as the text of a governance file and **read by the very validator acceptance uses**
//! ([`validate_policy`], [`validate_constitution`], [`validate_interwiki`], with the platform's limits and the wiki's
//! namespaces); what that accepts is written in its canonical text ([`Policy::to_yaml`], [`Interwiki::to_yaml`]) and sent as an
//! ordinary proposal that touches the governance files and **nothing else**: a change to the rules is its own proposal (handoff
//! G-T1 correction C-1), and one proposal carries every file the form changed. Each refusal of the validator is said **by its
//! name, as a sentence next to the control it concerns** ([`refusal_text`], [`map_refusal_text`]: one arm for each [`Refusal`],
//! so a refusal with no sentence does not compile), and the proposal service and the acceptance judge it again: the form is no
//! authority.
//!
//! **Who may open it.** Anyone who may propose may open the form and send a change: who may *accept* it is the rules' own and is
//! said on the page. Someone who may not propose is told why, as every refusal of a proposal says it. **While the rules cannot
//! be read** only an admin of the wiki may (the repair, INV-14), and the form starts from the default rules and names the fault.
//!
//! **An unchanged form creates nothing** and says so. *Back to the default rules* proposes removing the files that are there.

use std::collections::{BTreeSet, HashMap};

use askama::Template;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Action;
use inklkunn_core::governance::draft::{
    ConstitutionDraft, InterwikiDraft, PolicyDraft, ProtectDraft, fields as schema,
};
use inklkunn_core::governance::{
    Constitution, Fault, GovernanceFile, Interwiki, Limit, MAX_INTERWIKI_PREFIXES,
    MAX_PREFIX_CHARS, MAX_TEMPLATE_CHARS, OrderingPair, PlatformLimits, Policy, ReadFault, Refusal,
    Rules, namespaces_of_titles, validate_constitution, validate_interwiki, validate_policy,
};
use inklkunn_core::proposal::{Invalid, ProposalEdit, ProposalError};
use inklkunn_store::{Base, FileChange, Role};

use super::rules_fields as f;
use super::rules_paths as paths;
use super::rules_phrases as phrases;
use crate::catalog::{Key, T};
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::people::sentences::role_label;
use crate::proposals::governance::{rule_key, unreadable_paragraphs};
use crate::proposals::{
    Ctx, from_this_site, needs_account, notice_page, refused, see, serve, words,
};
use crate::wiki::{Choice, Shell, generic_not_found, services_of};

/// How the page title's place is written in an address, said on the form (the one placeholder; it is no message's word).
const PLACEHOLDER: &str = "{title}";

type Raw = crate::proposals::urlencoded::Raw;
type PathName = Result<Path<String>, PathRejection>;

// ---------------------------------------------------------------------------------------------- what was typed

/// One protection's row, as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Row {
    kind: String,
    title: String,
    asks: String,
    reason: String,
}

impl Row {
    fn is_blank(&self) -> bool {
        self.title.trim().is_empty() && self.reason.trim().is_empty()
    }
}

/// One row of the interwiki map, as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MapRow {
    prefix: String,
    template: String,
}

impl MapRow {
    /// A row nobody filled in (an added row) says nothing and is left out.
    fn is_blank(&self) -> bool {
        self.prefix.trim().is_empty() && self.template.trim().is_empty()
    }
}

/// What the controls hold, by field name, the protections' rows and the interwiki map's rows.
#[derive(Debug, Clone, Default)]
pub(crate) struct Typed {
    values: HashMap<String, String>,
    rows: Vec<Row>,
    map: Vec<MapRow>,
}

/// The value the form sends for *a maintainer may accept their own proposal while the wiki is small*: not the file's word.
const OWN_SMALL: &str = "small";
const OWN_NEVER: &str = "never";

/// The words of a file for a value of the form.
fn own_word(code: &str) -> String {
    if code == OWN_SMALL {
        "small-wiki".to_owned()
    } else {
        code.to_owned()
    }
}

impl Typed {
    fn get(&self, name: &str) -> &str {
        self.values.get(name).map_or("", String::as_str)
    }

    fn set(&mut self, name: &str, value: impl Into<String>) {
        self.values.insert(name.to_owned(), value.into());
    }

    /// The form filled from the rules in force and the interwiki map in force.
    fn of(t: T, rules: &Rules, map: &Interwiki) -> Typed {
        let d = PolicyDraft::of(rules.policy());
        let c = ConstitutionDraft::of(rules.constitution());
        let mut typed = Typed::default();
        typed.set(f::SUMMARY, t.get(Key::PolicyEditSummaryDefault));
        typed.set(f::EVERYONE, d.entry);
        typed.set(f::WHO_COMMENT, d.comment);
        typed.set(f::WHO_PROPOSE, d.propose);
        typed.set(f::WHO_REVIEW, d.review);
        typed.set(f::WHO_VIEW, d.create_view);
        typed.set(f::WHO_ACCEPT, d.accept);
        typed.set(f::WHO_MODERATE, d.moderate);
        typed.set(f::WHO_APPEALS, d.decide_appeal);
        typed.set(f::WHO_GRANTS, d.grant_up_to);
        typed.set(f::APPROVALS, d.approvals.to_string());
        typed.set(
            f::OWN,
            if d.self_acceptance == "small-wiki" {
                OWN_SMALL.to_owned()
            } else {
                d.self_acceptance
            },
        );
        typed.set(
            f::REVIEW_DAYS,
            d.review_after.map(|n| n.to_string()).unwrap_or_default(),
        );
        for (role, text) in d.promotion {
            typed.set(&promo_field(role), text);
        }
        typed.set(f::CHANGE_RUNG, c.policy_rung);
        typed.set(f::CHANGE_APPROVALS, c.policy_approvals.to_string());
        typed.set(
            f::CHANGE_WHO_APPROVALS,
            c.constitution_approvals.to_string(),
        );
        typed.rows = d
            .protect
            .into_iter()
            .map(|p| Row {
                kind: if p.namespace {
                    f::ROW_KIND_NAMESPACE
                } else {
                    f::ROW_KIND_PAGE
                }
                .to_owned(),
                title: p.title,
                asks: p.require,
                reason: p.reason,
            })
            .collect();
        typed.map = InterwikiDraft::of(map)
            .rows
            .into_iter()
            .map(|(prefix, template)| MapRow { prefix, template })
            .collect();
        typed
    }

    /// The form as it was sent.
    fn from_fields(given: &HashMap<String, String>) -> Typed {
        let mut typed = Typed::default();
        let mut names = vec![
            f::SUMMARY,
            f::EVERYONE,
            f::WHO_COMMENT,
            f::WHO_PROPOSE,
            f::WHO_REVIEW,
            f::WHO_VIEW,
            f::WHO_ACCEPT,
            f::WHO_MODERATE,
            f::WHO_APPEALS,
            f::WHO_GRANTS,
            f::APPROVALS,
            f::OWN,
            f::REVIEW_DAYS,
            f::CHANGE_RUNG,
            f::CHANGE_APPROVALS,
            f::CHANGE_WHO_APPROVALS,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        names.extend(Role::ALL.iter().map(|r| promo_field(*r)));
        for name in names {
            typed.set(&name, given.get(&name).cloned().unwrap_or_default());
        }
        // The rows: as many as the form says it sent, at most the most a policy may hold, plus one (an added row).
        let rows = given
            .get(f::ROWS)
            .and_then(|n| n.trim().parse::<usize>().ok())
            .unwrap_or(0)
            .min(inklkunn_core::governance::MAX_PROTECT + 1);
        for n in 1..=rows {
            let at = |field: &str| {
                given
                    .get(&f::row_field(n, field))
                    .cloned()
                    .unwrap_or_default()
            };
            typed.rows.push(Row {
                kind: at(f::ROW_KIND),
                title: at(f::ROW_TITLE),
                asks: at(f::ROW_ASKS),
                reason: at(f::ROW_REASON),
            });
        }
        // The interwiki map's rows, in the same way: as many as the form says it sent, at most the most a map may hold, plus one.
        let map_rows = given
            .get(f::MAP_ROWS)
            .and_then(|n| n.trim().parse::<usize>().ok())
            .unwrap_or(0)
            .min(MAX_INTERWIKI_PREFIXES + 1);
        for n in 1..=map_rows {
            let at = |field: &str| {
                given
                    .get(&f::map_field(n, field))
                    .cloned()
                    .unwrap_or_default()
            };
            typed.map.push(MapRow {
                prefix: at(f::MAP_PREFIX),
                template: at(f::MAP_TEMPLATE),
            });
        }
        typed
    }
}

fn promo_field(role: Role) -> String {
    format!("{}{}", f::PROMO, role.as_str())
}

// ---------------------------------------------------------------------------------------------- what is refused

/// One thing wrong with what was typed: the sentence, and the control it is said beside (none: said at the top only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Problem {
    pub(crate) control: Option<String>,
    pub(crate) text: String,
}

/// **The sentence of each refusal of the rules' validator**, as the form says it beside a control. One arm for each
/// [`Refusal`]: a refusal with no sentence does not compile, and a test reads every one of [`Refusal::ALL`] in both languages.
/// The sentences of the rules themselves ([`rule_key`]) say the refusals a person cannot make through this form.
pub(crate) fn refusal_text(t: T, refusal: Refusal, limits: &PlatformLimits) -> String {
    let key = match refusal {
        Refusal::WrongShape => Key::PolicyEditRefusalWrongShape,
        Refusal::UnknownValue => Key::PolicyEditRefusalUnknownValue,
        Refusal::OutOfRange => Key::PolicyEditRefusalOutOfRange,
        Refusal::Bounds => Key::PolicyEditRefusalBounds,
        Refusal::ForbiddenCharacter => Key::PolicyEditRefusalForbidden,
        Refusal::Missing => Key::PolicyEditRefusalMissing,
        Refusal::Duplicate => Key::PolicyEditRefusalDuplicate,
        Refusal::Ordering(OrderingPair::ProposeAboveReview) => Key::PolicyEditRefusalOrderPropose,
        Refusal::Ordering(OrderingPair::ReviewAboveAccept) => Key::PolicyEditRefusalOrderReview,
        Refusal::Ordering(OrderingPair::ModerateBelowReviewer) => {
            Key::PolicyEditRefusalOrderModerate
        }
        Refusal::Ordering(OrderingPair::DecideAppealBelowModerate) => {
            Key::PolicyEditRefusalOrderDecideAppeal
        }
        Refusal::PlatformLimit(Limit::EntryMax) => {
            return t
                .msg(Key::PolicyEditRefusalLimitEntry)
                .with("role", phrases::entry_value(t, limits.entry_max))
                .text();
        }
        Refusal::PlatformLimit(Limit::LimitDays) => {
            return t
                .msg(Key::PolicyEditRefusalLimitDays)
                .with("days", limits.limit_days)
                .text();
        }
        Refusal::NotText
        | Refusal::TooLong
        | Refusal::RepeatedKey
        | Refusal::UnknownKey
        | Refusal::OutsideDefaultView
        | Refusal::UnknownFile
        | Refusal::NotAFile
        // The interwiki map's refusals are said by [`map_refusal_text`]; these are the rules' own sentences, for the match.
        | Refusal::PrefixIsNamespace
        | Refusal::TemplateScheme
        | Refusal::TemplateHost
        | Refusal::TemplateUserInfo
        | Refusal::TemplatePlaceholder
        | Refusal::TemplateCharacter => rule_key(refusal),
    };
    t.get(key)
}

/// Where in the interwiki map a refusal is said: beside a row's prefix, beside a row's address, or about the map as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapAt {
    /// The prefix of a row.
    Prefix,
    /// The address of a row.
    Template,
    /// The map as a whole (it has too many prefixes).
    Whole,
}

/// **The sentence of each refusal of the interwiki map's validator**, as the form says it beside a row (or about the map as a
/// whole). One arm for each [`Refusal`], none a wildcard: a refusal with no sentence does not compile, and a test reads every one of
/// [`Refusal::ALL`] in both languages. The rules' own sentences ([`rule_key`]) name the *interwiki map* and say what an address
/// must hold, and read as they are beside a row; the others are said here, in the words of a row (a *prefix*, an *address*).
pub(crate) fn map_refusal_text(t: T, refusal: Refusal, at: MapAt) -> String {
    let key = match refusal {
        // The file's keys are the prefixes: a key that is none is a prefix with a character no prefix has.
        Refusal::UnknownKey => Key::PolicyEditInterwikiPrefixCharacters,
        Refusal::RepeatedKey => Key::PolicyEditInterwikiRepeated,
        Refusal::Bounds => {
            return match at {
                MapAt::Prefix => t
                    .msg(Key::PolicyEditInterwikiPrefixLong)
                    .with("max", MAX_PREFIX_CHARS)
                    .text(),
                MapAt::Template => t
                    .msg(Key::PolicyEditInterwikiTemplateLong)
                    .with("max", MAX_TEMPLATE_CHARS)
                    .text(),
                MapAt::Whole => t
                    .msg(Key::PolicyEditInterwikiTooMany)
                    .with("max", MAX_INTERWIKI_PREFIXES)
                    .text(),
            };
        }
        // The form says an empty prefix or address itself, before the validator is asked.
        Refusal::Missing => Key::PolicyEditRefusalMissing,
        // The rest: the rules' own sentences. The five of the map are the ones a person can meet; the others a row cannot make.
        Refusal::PrefixIsNamespace
        | Refusal::TemplateScheme
        | Refusal::TemplateHost
        | Refusal::TemplateUserInfo
        | Refusal::TemplatePlaceholder
        | Refusal::TemplateCharacter
        | Refusal::NotText
        | Refusal::TooLong
        | Refusal::WrongShape
        | Refusal::UnknownValue
        | Refusal::OutOfRange
        | Refusal::ForbiddenCharacter
        | Refusal::Duplicate
        | Refusal::Ordering(_)
        | Refusal::PlatformLimit(_)
        | Refusal::OutsideDefaultView
        | Refusal::UnknownFile
        | Refusal::NotAFile => rule_key(refusal),
    };
    t.get(key)
}

/// The control a fault of the interwiki map is about: the prefix or the address of the row numbered `n` on the form.
fn map_control(at: MapAt, n: usize) -> Option<String> {
    match at {
        MapAt::Prefix => Some(f::map_field(n, f::MAP_PREFIX)),
        MapAt::Template => Some(f::map_field(n, f::MAP_TEMPLATE)),
        MapAt::Whole => None,
    }
}

/// The control a fault of the rules is about, by field name: the key's own control, or the row and the field of the protection
/// it names. `kept` is the row number of each protection the draft held, in order (blank rows are not sent).
pub(crate) fn control_of(fault: &Fault, kept: &[usize]) -> Option<String> {
    let row = |field: &str| {
        let n = fault.item.and_then(|item| kept.get(item.checked_sub(1)?))?;
        Some(f::row_field(*n, field))
    };
    let own = |name: &str| Some(name.to_owned());
    match fault.field {
        schema::ENTRY => own(f::EVERYONE),
        schema::COMMENT => own(f::WHO_COMMENT),
        schema::PROPOSE => own(f::WHO_PROPOSE),
        schema::REVIEW => own(f::WHO_REVIEW),
        schema::CREATE_VIEW => own(f::WHO_VIEW),
        schema::ACCEPT => own(f::WHO_ACCEPT),
        schema::MODERATE => own(f::WHO_MODERATE),
        schema::DECIDE_APPEAL => own(f::WHO_APPEALS),
        schema::GRANT_UP_TO => own(f::WHO_GRANTS),
        schema::APPROVALS => own(f::APPROVALS),
        schema::SELF_ACCEPTANCE => own(f::OWN),
        schema::REVIEW_AFTER => own(f::REVIEW_DAYS),
        schema::PROTECT_PATH | schema::PROTECT_NAMESPACE | schema::PROTECT => row(f::ROW_TITLE),
        schema::PROTECT_REQUIRE | schema::PROTECT_REQUIRE_RUNG => row(f::ROW_ASKS),
        schema::PROTECT_REASON => row(f::ROW_REASON),
        schema::POLICY_CHANGE_RUNG => own(f::CHANGE_RUNG),
        schema::POLICY_CHANGE_APPROVALS => own(f::CHANGE_APPROVALS),
        schema::CONSTITUTION_CHANGE_APPROVALS => own(f::CHANGE_WHO_APPROVALS),
        other => Role::ALL
            .iter()
            .find(|role| schema::promotion(**role) == other)
            .map(|role| promo_field(*role)),
    }
}

fn problem_of(t: T, fault: &Fault, kept: &[usize], limits: &PlatformLimits) -> Problem {
    Problem {
        control: control_of(fault, kept),
        text: match (fault.field, fault.refusal) {
            // A title that is not written as a page is named is said so, not as one of a list of options.
            (schema::PROTECT_PATH | schema::PROTECT_NAMESPACE, Refusal::UnknownValue) => {
                t.get(Key::PolicyEditRefusalTitle)
            }
            _ => refusal_text(t, fault.refusal, limits),
        },
    }
}

/// What the form's fields say, as rules: or every problem with them, each beside its control.
pub(crate) struct Built {
    pub(crate) policy: Policy,
    pub(crate) constitution: Constitution,
    pub(crate) map: Interwiki,
}

/// The rows of the interwiki map that say something, each with the number it has on the form (a blank row is an added row nobody
/// filled in) and its prefix and address trimmed.
fn map_rows(typed: &Typed) -> Vec<(usize, String, String)> {
    typed
        .map
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.is_blank())
        .map(|(i, row)| {
            (
                i + 1,
                row.prefix.trim().to_owned(),
                row.template.trim().to_owned(),
            )
        })
        .collect()
}

/// The interwiki map the rows say, **each row judged by the validator acceptance uses** ([`validate_interwiki`], with the wiki's
/// `namespaces`) and the whole after it, so that every row that breaks a rule is said at once, beside the row. `None` when any
/// row does, and the problems are pushed. A row with only one of its two filled is refused as empty, a prefix written twice is
/// refused at its second row (the validator's own *repeated key*), and nothing a person typed is written as syntax
/// ([`InterwikiDraft::to_text`]).
fn judge_map(
    t: T,
    typed: &Typed,
    namespaces: &BTreeSet<String>,
    problems: &mut Vec<Problem>,
) -> Option<Interwiki> {
    let mut kept: Vec<(String, String)> = Vec::new();
    let mut found: Vec<Problem> = Vec::new();
    let say = |found: &mut Vec<Problem>, at: MapAt, n: usize, refusal: Refusal| {
        found.push(Problem {
            control: map_control(at, n),
            text: map_refusal_text(t, refusal, at),
        });
    };
    for (n, prefix, template) in map_rows(typed) {
        if prefix.is_empty() || template.is_empty() {
            if prefix.is_empty() {
                say(&mut found, MapAt::Prefix, n, Refusal::Missing);
            }
            if template.is_empty() {
                say(&mut found, MapAt::Template, n, Refusal::Missing);
            }
            continue;
        }
        let row = InterwikiDraft {
            rows: vec![(prefix.clone(), template.clone())],
        };
        if let Err(fault) = validate_interwiki(&row.to_text(), namespaces) {
            let at = if fault.field == schema::INTERWIKI_TEMPLATE {
                MapAt::Template
            } else {
                MapAt::Prefix
            };
            say(&mut found, at, n, fault.refusal);
            continue;
        }
        if kept.iter().any(|(earlier, _)| *earlier == prefix) {
            say(&mut found, MapAt::Prefix, n, Refusal::RepeatedKey);
            continue;
        }
        kept.push((prefix, template));
    }
    if found.is_empty() {
        match validate_interwiki(&InterwikiDraft { rows: kept }.to_text(), namespaces) {
            Ok(map) => return Some(map),
            Err(fault) => say(&mut found, MapAt::Whole, 0, fault.refusal),
        }
    }
    problems.extend(found);
    None
}

/// The namespaces of the wiki as an interwiki map's prefixes are judged against them: **the ones acceptance uses**
/// ([`namespaces_of_titles`] over the titles of the default view's tip). `Err` is *could not be asked now*, never *no namespaces*.
fn wiki_namespaces(ctx: &Ctx) -> Result<BTreeSet<String>, ()> {
    let index = ctx
        .services
        .read
        .title_index(&ctx.wiki, Some(&ctx.wiki.default_view))
        .map_err(|e| {
            ctx.log(&format!(
                "the titles of the wiki could not be read now: {e}"
            ))
        })?;
    let titles: Vec<String> = index
        .entries()
        .iter()
        .map(|e| inklkunn_markup::path::path_to_title(&e.path).unwrap_or_else(|| e.path.clone()))
        .collect();
    Ok(namespaces_of_titles(titles.iter().map(String::as_str)))
}

/// The namespaces the rows are judged against: **the wiki's when the map is changed, none when it is not**. Acceptance judges a
/// map against the namespaces only when the proposal writes the file, so a form that does not change the map must not refuse for
/// a prefix that a page made after the map was accepted has since taken as its namespace.
fn namespaces_for(
    ctx: &Ctx,
    t: T,
    typed: &Typed,
    in_force: &Interwiki,
) -> Result<BTreeSet<String>, ()> {
    let none = BTreeSet::new();
    match judge_map(t, typed, &none, &mut Vec::new()) {
        Some(map) if map == *in_force => Ok(none),
        _ => wiki_namespaces(ctx),
    }
}

fn number(
    t: T,
    typed: &Typed,
    name: &'static str,
    field: &'static str,
    file: GovernanceFile,
    problems: &mut Vec<Problem>,
) -> Option<i64> {
    match typed.get(name).trim().parse::<i64>() {
        Ok(n) => Some(n),
        Err(_) => {
            let fault = Fault::at(file, Refusal::WrongShape, field);
            problems.push(Problem {
                control: control_of(&fault, &[]),
                text: t.get(Key::PolicyEditProblemNumber),
            });
            None
        }
    }
}

/// The rules the controls say, **judged by the validator acceptance uses**.
pub(crate) fn build(
    t: T,
    typed: &Typed,
    limits: &PlatformLimits,
    namespaces: &BTreeSet<String>,
) -> Result<Built, Vec<Problem>> {
    let mut problems = Vec::new();
    let map = judge_map(t, typed, namespaces, &mut problems);
    let approvals = number(
        t,
        typed,
        f::APPROVALS,
        schema::APPROVALS,
        GovernanceFile::Policy,
        &mut problems,
    );
    let review_after = match typed.get(f::REVIEW_DAYS).trim() {
        "" => Some(None),
        _ => number(
            t,
            typed,
            f::REVIEW_DAYS,
            schema::REVIEW_AFTER,
            GovernanceFile::Policy,
            &mut problems,
        )
        .map(Some),
    };
    let change_approvals = number(
        t,
        typed,
        f::CHANGE_APPROVALS,
        schema::POLICY_CHANGE_APPROVALS,
        GovernanceFile::Constitution,
        &mut problems,
    );
    let who_approvals = number(
        t,
        typed,
        f::CHANGE_WHO_APPROVALS,
        schema::CONSTITUTION_CHANGE_APPROVALS,
        GovernanceFile::Constitution,
        &mut problems,
    );
    // The rows that say something; a blank row is an added row nobody filled in.
    let (rows, kept): (Vec<&Row>, Vec<usize>) = typed
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.is_blank())
        .map(|(i, row)| (row, i + 1))
        .unzip();
    let (Some(approvals), Some(review_after), Some(change_approvals), Some(who_approvals)) =
        (approvals, review_after, change_approvals, who_approvals)
    else {
        return Err(problems);
    };
    let draft = PolicyDraft {
        entry: typed.get(f::EVERYONE).to_owned(),
        comment: typed.get(f::WHO_COMMENT).to_owned(),
        propose: typed.get(f::WHO_PROPOSE).to_owned(),
        review: typed.get(f::WHO_REVIEW).to_owned(),
        create_view: typed.get(f::WHO_VIEW).to_owned(),
        accept: typed.get(f::WHO_ACCEPT).to_owned(),
        moderate: typed.get(f::WHO_MODERATE).to_owned(),
        decide_appeal: typed.get(f::WHO_APPEALS).to_owned(),
        grant_up_to: typed.get(f::WHO_GRANTS).to_owned(),
        approvals,
        self_acceptance: own_word(typed.get(f::OWN)),
        review_after,
        promotion: Role::ALL
            .iter()
            .map(|role| (*role, typed.get(&promo_field(*role)).trim().to_owned()))
            .collect(),
        protect: rows
            .iter()
            .map(|row| ProtectDraft {
                namespace: row.kind == f::ROW_KIND_NAMESPACE,
                title: row.title.clone(),
                require: row.asks.clone(),
                reason: row.reason.clone(),
            })
            .collect(),
    };
    let constitution = ConstitutionDraft {
        policy_rung: typed.get(f::CHANGE_RUNG).to_owned(),
        policy_approvals: change_approvals,
        constitution_approvals: who_approvals,
    };
    let policy = validate_policy(&draft.to_text(), limits);
    let constitution = validate_constitution(&constitution.to_text());
    match (policy, constitution) {
        (Ok(policy), Ok(constitution)) => match map {
            Some(map) => Ok(Built {
                policy,
                constitution,
                map,
            }),
            None => Err(problems),
        },
        (policy, constitution) => {
            for fault in [policy.err(), constitution.err()].into_iter().flatten() {
                problems.push(problem_of(t, &fault, &kept, limits));
            }
            Err(problems)
        }
    }
}

// ---------------------------------------------------------------------------------------------- the page

struct Opt {
    value: String,
    label: String,
    selected: bool,
}

/// One control: a select (it has options) or a line of text or a number.
struct Control {
    id: String,
    name: String,
    label: String,
    hint: Option<String>,
    error: Option<String>,
    options: Vec<Opt>,
    value: String,
    numeric: bool,
    max: Option<usize>,
}

struct Button {
    name: &'static str,
    value: String,
    label: String,
}

struct Group {
    legend: String,
    note: Option<String>,
    controls: Vec<Control>,
    button: Option<Button>,
}

#[derive(Template)]
#[template(path = "policy_form.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    problems: Vec<String>,
    notes: Vec<String>,
    action: String,
    base_state_field: &'static str,
    base_state: String,
    base_block_field: &'static str,
    base_block: String,
    rows_field: &'static str,
    rows: usize,
    map_rows_field: &'static str,
    map_rows: usize,
    summary: Control,
    groups: Vec<Group>,
    buttons: Vec<Button>,
    back: Choice,
}

/// The errors by control, from the problems.
fn errors_of(problems: &[Problem]) -> HashMap<&str, &str> {
    problems
        .iter()
        .filter_map(|p| Some((p.control.as_deref()?, p.text.as_str())))
        .collect()
}

struct Maker<'a> {
    typed: &'a Typed,
    errors: HashMap<&'a str, &'a str>,
}

impl Maker<'_> {
    fn base(&self, name: &str, label: String, hint: Option<String>) -> Control {
        Control {
            id: f::control_id(name),
            name: name.to_owned(),
            label,
            hint,
            error: self.errors.get(name).map(|e| (*e).to_owned()),
            options: Vec::new(),
            value: self.typed.get(name).to_owned(),
            numeric: false,
            max: None,
        }
    }

    fn select(
        &self,
        name: &str,
        label: String,
        hint: Option<String>,
        options: Vec<(String, String)>,
    ) -> Control {
        let mut c = self.base(name, label, hint);
        let chosen = c.value.clone();
        c.options = options
            .into_iter()
            .map(|(value, label)| Opt {
                selected: value == chosen,
                value,
                label,
            })
            .collect();
        c
    }

    fn line(&self, name: &str, label: String, hint: Option<String>, numeric: bool) -> Control {
        let mut c = self.base(name, label, hint);
        c.numeric = numeric;
        c
    }
}

/// Every role, lowest first, as the options of a select of rungs.
fn rung_options(t: T, from: Role) -> Vec<(String, String)> {
    Role::ALL
        .into_iter()
        .filter(|r| *r >= from)
        .map(|r| (r.as_str().to_owned(), role_label(t, r)))
        .collect()
}

fn form_page(
    page: &Page,
    ctx: &Ctx,
    typed: &Typed,
    base: &Base,
    problems: &[Problem],
    notes: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let limits = *ctx.services.read.platform_limits();
    let server_days = ctx.services.read.review_after_days();
    let maker = Maker {
        typed,
        errors: errors_of(problems),
    };
    let label = |key| t.get(key);
    let mut groups = Vec::new();

    // Who may do what.
    let entry_options: Vec<(String, String)> = inklkunn_core::governance::Entry::ALL
        .iter()
        .map(|e| (e.word().to_owned(), phrases::entry_value(t, *e)))
        .collect();
    let grant_options: Vec<(String, String)> = inklkunn_core::governance::GrantUpTo::ALL
        .iter()
        .map(|g| (g.word().to_owned(), phrases::grant_value(t, *g)))
        .collect();
    let rung_select = |name: &str, key: Key, from: Role| {
        maker.select(name, label(key), None, rung_options(t, from))
    };
    groups.push(Group {
        legend: t.get(Key::PolicyWhoHeading),
        note: None,
        controls: vec![
            rung_select(f::WHO_COMMENT, Key::PolicyLabelComment, Role::Observer),
            rung_select(f::WHO_PROPOSE, Key::PolicyLabelPropose, Role::Observer),
            rung_select(f::WHO_REVIEW, Key::PolicyLabelReview, Role::Observer),
            rung_select(f::WHO_VIEW, Key::PolicyLabelCreateView, Role::Observer),
            rung_select(f::WHO_ACCEPT, Key::PolicyLabelAccept, Role::Observer),
            rung_select(f::WHO_MODERATE, Key::PolicyLabelModerate, Role::Observer),
            maker.select(
                f::WHO_APPEALS,
                label(Key::PolicyLabelDecideAppeal),
                None,
                rung_options(t, Role::Maintainer),
            ),
            maker.select(
                f::WHO_GRANTS,
                label(Key::PolicyLabelGrant),
                None,
                grant_options,
            ),
            maker.select(
                f::EVERYONE,
                label(Key::PolicyLabelEntry),
                Some(
                    t.msg(Key::PolicyEditEntryHint)
                        .with("role", phrases::entry_value(t, limits.entry_max))
                        .text(),
                ),
                entry_options,
            ),
        ],
        button: None,
    });

    // How a proposal is accepted.
    groups.push(Group {
        legend: t.get(Key::PolicyAcceptHeading),
        note: None,
        controls: vec![
            maker.line(
                f::APPROVALS,
                label(Key::PolicyLabelApprovals),
                Some(
                    t.msg(Key::PolicyEditApprovalsHint)
                        .with("max", inklkunn_core::governance::MAX_APPROVALS)
                        .text(),
                ),
                true,
            ),
            maker.select(
                f::OWN,
                label(Key::PolicyLabelSelf),
                None,
                vec![
                    (
                        OWN_NEVER.to_owned(),
                        phrases::self_value(t, inklkunn_core::governance::SelfAcceptance::Never),
                    ),
                    (
                        OWN_SMALL.to_owned(),
                        phrases::self_value(
                            t,
                            inklkunn_core::governance::SelfAcceptance::SmallWiki,
                        ),
                    ),
                ],
            ),
            maker.line(
                f::REVIEW_DAYS,
                label(Key::PolicyLabelReviewAfter),
                Some(
                    t.msg(Key::PolicyEditReviewHint)
                        .with("max", inklkunn_core::governance::MAX_REVIEW_AFTER_DAYS)
                        .with("days", server_days)
                        .text(),
                ),
                true,
            ),
        ],
        button: None,
    });

    // What the wiki looks for before it grants a role.
    groups.push(Group {
        legend: t.get(Key::PolicyPromotionHeading),
        note: Some(t.get(Key::PolicyEditPromotionHint)),
        controls: Role::ALL
            .into_iter()
            .map(|role| {
                let mut c = maker.line(
                    &promo_field(role),
                    phrases::promotion_label(t, role),
                    None,
                    false,
                );
                c.max = Some(inklkunn_core::governance::MAX_PROMOTION_CHARS);
                c
            })
            .collect(),
        button: None,
    });

    // How the rules are changed.
    groups.push(Group {
        legend: t.get(Key::PolicyChangeHeading),
        note: None,
        controls: vec![
            maker.select(
                f::CHANGE_RUNG,
                label(Key::PolicyLabelChangeRules),
                None,
                rung_options(t, Role::Maintainer)
                    .into_iter()
                    .filter(|(word, _)| word == "maintainer" || word == "admin")
                    .collect(),
            ),
            maker.line(
                f::CHANGE_APPROVALS,
                label(Key::PolicyEditLabelChangeApprovals),
                None,
                true,
            ),
            maker.line(
                f::CHANGE_WHO_APPROVALS,
                label(Key::PolicyEditLabelChangeWhoApprovals),
                None,
                true,
            ),
        ],
        button: None,
    });

    // The protections: a list with a row each.
    groups.push(Group {
        legend: t.get(Key::PolicyProtectionsHeading),
        note: Some(t.get(Key::PolicyEditProtectIntro)),
        controls: Vec::new(),
        button: Some(Button {
            name: f::ACT,
            value: f::ACT_ADD.to_owned(),
            label: t.get(Key::PolicyEditProtectAdd),
        }),
    });
    for (i, row) in typed.rows.iter().enumerate() {
        let n = i + 1;
        let named = |field: &str| f::row_field(n, field);
        let mut kind = maker.base(
            &named(f::ROW_KIND),
            t.get(Key::PolicyEditProtectKindLabel),
            None,
        );
        kind.value = row.kind.clone();
        let kind = {
            let chosen = row.kind.clone();
            let page_chosen = chosen != f::ROW_KIND_NAMESPACE;
            kind.options = vec![
                Opt {
                    value: f::ROW_KIND_PAGE.to_owned(),
                    label: t.get(Key::PolicyEditProtectKindPage),
                    selected: page_chosen,
                },
                Opt {
                    value: f::ROW_KIND_NAMESPACE.to_owned(),
                    label: t.get(Key::PolicyEditProtectKindNamespace),
                    selected: !page_chosen,
                },
            ];
            kind
        };
        let mut title = maker.line(
            &named(f::ROW_TITLE),
            t.get(Key::PolicyEditProtectTitleLabel),
            None,
            false,
        );
        title.value = row.title.clone();
        title.max = Some(inklkunn_core::governance::MAX_TARGET_CHARS);
        let mut asks = maker.base(
            &named(f::ROW_ASKS),
            t.get(Key::PolicyEditProtectAsksLabel),
            None,
        );
        asks.value = row.asks.clone();
        let mut options = vec![Opt {
            value: f::ASKS_REVIEW.to_owned(),
            label: t.get(Key::ProtectionRequiresReview),
            selected: row.asks == f::ASKS_REVIEW,
        }];
        options.extend(Role::ALL.into_iter().map(|role| {
            Opt {
                value: role.as_str().to_owned(),
                label: t
                    .msg(Key::ProtectionRequiresRung)
                    .with("role", role_label(t, role))
                    .text(),
                selected: row.asks == role.as_str(),
            }
        }));
        asks.options = options;
        let mut reason = maker.line(
            &named(f::ROW_REASON),
            t.get(Key::PolicyEditProtectReasonLabel),
            None,
            false,
        );
        reason.value = row.reason.clone();
        reason.max = Some(inklkunn_core::governance::MAX_REASON_CHARS);
        groups.push(Group {
            legend: t.msg(Key::PolicyEditProtectRow).with("number", n).text(),
            note: None,
            controls: vec![kind, title, asks, reason],
            button: Some(Button {
                name: f::ACT,
                value: format!("{}{n}", f::ACT_REMOVE),
                label: t.get(Key::PolicyEditProtectRemove),
            }),
        });
    }

    // The interwiki map: a list with a row each, a prefix and the address it leads to.
    groups.push(Group {
        legend: t.get(Key::PolicyInterwikiHeading),
        note: Some(
            t.msg(Key::PolicyEditInterwikiIntro)
                .with("placeholder", PLACEHOLDER)
                .text(),
        ),
        controls: Vec::new(),
        button: Some(Button {
            name: f::ACT,
            value: f::ACT_MAP_ADD.to_owned(),
            label: t.get(Key::PolicyEditInterwikiAdd),
        }),
    });
    for (i, row) in typed.map.iter().enumerate() {
        let n = i + 1;
        let mut prefix = maker.line(
            &f::map_field(n, f::MAP_PREFIX),
            t.get(Key::PolicyEditInterwikiPrefixLabel),
            None,
            false,
        );
        prefix.value = row.prefix.clone();
        prefix.max = Some(MAX_PREFIX_CHARS);
        let mut template = maker.line(
            &f::map_field(n, f::MAP_TEMPLATE),
            t.msg(Key::PolicyEditInterwikiTemplateLabel)
                .with("placeholder", PLACEHOLDER)
                .text(),
            None,
            false,
        );
        template.value = row.template.clone();
        template.max = Some(MAX_TEMPLATE_CHARS);
        groups.push(Group {
            legend: t.msg(Key::PolicyEditInterwikiRow).with("number", n).text(),
            note: None,
            controls: vec![prefix, template],
            button: Some(Button {
                name: f::ACT,
                value: format!("{}{n}", f::ACT_MAP_REMOVE),
                label: t.get(Key::PolicyEditInterwikiRemove),
            }),
        });
    }

    let mut summary = maker.line(
        f::SUMMARY,
        t.msg(Key::ProposeLabelSummary)
            .with("limit", inklkunn_core::proposal::MAX_SUMMARY_CHARS)
            .text(),
        Some(t.get(Key::ProposeSummaryHint)),
        false,
    );
    summary.max = Some(inklkunn_core::proposal::MAX_SUMMARY_CHARS);

    let general: Vec<String> = problems
        .iter()
        .filter(|p| p.control.is_none())
        .map(|p| p.text.clone())
        .collect();
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitlePolicyEdit)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::PolicyEditHeading),
        problems: general,
        notes,
        action: paths::edit(ctx.slug()),
        base_state_field: f::BASE_STATE,
        base_state: base.state_id.clone(),
        base_block_field: f::BASE_BLOCK,
        base_block: base.block_id.clone(),
        rows_field: f::ROWS,
        rows: typed.rows.len(),
        map_rows_field: f::MAP_ROWS,
        map_rows: typed.map.len(),
        summary,
        groups,
        buttons: vec![
            Button {
                name: f::ACT,
                value: f::ACT_SEND.to_owned(),
                label: t.get(Key::PolicyEditSend),
            },
            Button {
                name: f::ACT,
                value: f::ACT_DEFAULTS.to_owned(),
                label: t.get(Key::PolicyEditDefaultsButton),
            },
        ],
        back: Choice {
            label: t.get(Key::PolicyHeading),
            href: paths::page(ctx.slug()),
        },
    }
    .render();
    answer(status, &head, rendered)
}

// ---------------------------------------------------------------------------------------------- the handlers

fn unavailable(page: &Page, ctx: &Ctx) -> Response {
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

/// Who may open the form, said in words for who may not: an account, and the rung to propose; while the rules cannot be read,
/// an admin of the wiki (the repair).
fn gate(page: &Page, ctx: &Ctx, rules: &Rules) -> Option<Response> {
    if let Some(response) = needs_account(page, ctx) {
        return Some(response);
    }
    let t = page.t();
    match rules.fault() {
        None => {
            if ctx.may_wiki(Action::Propose) {
                None
            } else {
                Some(refused(
                    page,
                    ctx,
                    &ctx.refusal_wiki(Action::Propose),
                    Vec::new(),
                ))
            }
        }
        // In the repair state the admin of the wiki holds the right to accept, and nobody else holds any.
        Some(fault) => {
            if ctx.may_wiki(Action::Accept) {
                None
            } else {
                let mut paragraphs = vec![t.get(Key::PolicyEditRepairNotAdmin)];
                paragraphs.extend(unreadable_paragraphs(t, fault));
                Some(notice_page(
                    page,
                    ctx,
                    StatusCode::FORBIDDEN,
                    Key::PolicyEditHeading,
                    paragraphs,
                    vec![Choice {
                        label: t.get(Key::PolicyHeading),
                        href: paths::page(ctx.slug()),
                    }],
                ))
            }
        }
    }
}

/// What the page says above the form: who may accept what is sent, and the fault when the rules cannot be read.
fn notes_of(t: T, rules: &Rules) -> Vec<String> {
    let mut notes = Vec::new();
    match rules.fault() {
        Some(fault) => {
            notes.push(t.get(Key::PolicyEditRepairNote));
            notes.extend(unreadable_paragraphs(t, fault));
        }
        None => {
            notes.push(t.get(Key::PolicyEditWhoNote));
            notes.push(
                t.msg(Key::PolicyEditAcceptNote)
                    .with(
                        "rule",
                        phrases::change_rules_value(
                            t,
                            rules.constitution(),
                            rules.policy().acceptance().approvals(),
                        ),
                    )
                    .text(),
            );
        }
    }
    notes
}

/// The interwiki map in force, **as the policy page reads it** (`interwiki_of` of the default view's tip): empty while the map is
/// the file that cannot be read (the form then starts from none, and the repair writes what is typed or removes the file).
/// `Err` is *could not be read now*, never *none*.
fn map_in_force(ctx: &Ctx, rules: &Rules) -> Result<Interwiki, ()> {
    if rules.fault().map(ReadFault::file) == Some(GovernanceFile::Interwiki) {
        return Ok(Interwiki::empty());
    }
    ctx.services
        .read
        .interwiki_of(&ctx.wiki)
        .map_err(|e| ctx.log(&format!("the interwiki map could not be read now: {e}")))
}

/// The base a form is opened on: the state of the default view now.
fn capture(ctx: &Ctx) -> Result<Base, ()> {
    ctx.services
        .proposals
        .capture_base(&ctx.wiki, None)
        .map_err(|e| ctx.log(&format!("a base could not be read now: {e}")))
}

/// `GET /{slug}/-/policy/edit`.
pub(crate) async fn edit_form(
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
        let Ok(rules) = ctx.services.authorizer.rules(&ctx.wiki.id) else {
            return unavailable(page, &ctx);
        };
        if let Some(response) = gate(page, &ctx, &rules) {
            return response;
        }
        let Ok(base) = capture(&ctx) else {
            return unavailable(page, &ctx);
        };
        // The map in force is the one the policy page shows. A map that cannot be read now is not an empty one: the form is not
        // offered, as it would propose to remove what it does not know.
        let Ok(map) = map_in_force(&ctx, &rules) else {
            return unavailable(page, &ctx);
        };
        form_page(
            page,
            &ctx,
            &Typed::of(page.t(), &rules, &map),
            &base,
            &[],
            notes_of(page.t(), &rules),
            StatusCode::OK,
        )
    })
    .await
}

/// `POST /{slug}/-/policy/edit`.
pub(crate) async fn edit_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: PathName,
    form: Raw,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        let Ok(rules) = ctx.services.authorizer.rules(&ctx.wiki.id) else {
            return unavailable(page, &ctx);
        };
        if let Some(response) = gate(page, &ctx, &rules) {
            return response;
        }
        let given = match crate::proposals::fields_of(page, &ctx, &headers, form) {
            Ok(given) => given,
            Err(response) => return *response,
        };
        let base = match Base::new(
            given.get(f::BASE_STATE).map_or("", String::as_str),
            given.get(f::BASE_BLOCK).map_or("", String::as_str),
        ) {
            Some(base) => base,
            None => match capture(&ctx) {
                Ok(base) => base,
                Err(()) => return unavailable(page, &ctx),
            },
        };
        let typed = Typed::from_fields(&given);
        let act = given.get(f::ACT).map_or("", String::as_str);
        post_act(page, &ctx, &rules, &base, typed, act)
    })
    .await
}

/// What the button that was pressed does.
fn post_act(
    page: &Page,
    ctx: &Ctx,
    rules: &Rules,
    base: &Base,
    mut typed: Typed,
    act: &str,
) -> Response {
    let t = page.t();
    let again = |typed: &Typed, problems: &[Problem], notes: Vec<String>, status| {
        let mut shown = notes_of(t, rules);
        shown.extend(notes);
        form_page(page, ctx, typed, base, problems, shown, status)
    };
    if act == f::ACT_ADD {
        typed.rows.push(Row {
            kind: f::ROW_KIND_PAGE.to_owned(),
            asks: f::ASKS_REVIEW.to_owned(),
            ..Row::default()
        });
        return again(&typed, &[], Vec::new(), StatusCode::OK);
    }
    if let Some(n) = act
        .strip_prefix(f::ACT_REMOVE)
        .and_then(|n| n.parse::<usize>().ok())
    {
        if (1..=typed.rows.len()).contains(&n) {
            typed.rows.remove(n - 1);
        }
        return again(&typed, &[], Vec::new(), StatusCode::OK);
    }
    if act == f::ACT_MAP_ADD {
        typed.map.push(MapRow::default());
        return again(&typed, &[], Vec::new(), StatusCode::OK);
    }
    if let Some(n) = act
        .strip_prefix(f::ACT_MAP_REMOVE)
        .and_then(|n| n.parse::<usize>().ok())
    {
        if (1..=typed.map.len()).contains(&n) {
            typed.map.remove(n - 1);
        }
        return again(&typed, &[], Vec::new(), StatusCode::OK);
    }
    let limits = *ctx.services.read.platform_limits();
    let summary = typed.get(f::SUMMARY).trim().to_owned();
    let (files, summary) = match act {
        f::ACT_DEFAULTS => match defaults_files(ctx, rules, base) {
            Ok(files) if files.is_empty() => {
                return again(
                    &typed,
                    &[],
                    vec![t.get(Key::PolicyEditDefaultsUnchanged)],
                    StatusCode::OK,
                );
            }
            Ok(files) => (files, t.get(Key::PolicyEditSummaryDefaults)),
            Err(()) => return unavailable(page, ctx),
        },
        f::ACT_SEND => {
            let Ok(in_force) = map_in_force(ctx, rules) else {
                return unavailable(page, ctx);
            };
            let Ok(namespaces) = namespaces_for(ctx, t, &typed, &in_force) else {
                return unavailable(page, ctx);
            };
            let built = match build(t, &typed, &limits, &namespaces) {
                Ok(built) => built,
                Err(problems) => {
                    return again(
                        &typed,
                        &problems,
                        Vec::new(),
                        StatusCode::UNPROCESSABLE_ENTITY,
                    );
                }
            };
            let files = changed_files(rules, &built, &in_force);
            if files.is_empty() {
                return again(
                    &typed,
                    &[],
                    vec![t.get(Key::PolicyEditUnchanged)],
                    StatusCode::OK,
                );
            }
            (files, summary)
        }
        _ => return again(&typed, &[], Vec::new(), StatusCode::OK),
    };
    let edit = ProposalEdit {
        summary,
        files,
        base: Some(base.clone()),
        ..ProposalEdit::default()
    };
    // Made and sent in one step, as the other forms that send at once: a refusal leaves nothing behind. **The repair** (the rules
    // cannot be read, and an admin proposes a change to the files of the rules alone) is a draft and its submission: the one step
    // judges the proposal as a whole, which the wiki that refuses every write cannot allow, and the submission asks the paths.
    let service = &ctx.services.proposals;
    let made = if rules.fault().is_some() {
        service
            .create(&ctx.principal, &ctx.wiki, None, edit)
            .and_then(|draft| service.submit(&ctx.principal, &ctx.wiki, &draft.id))
    } else {
        service.create_and_submit(&ctx.principal, &ctx.wiki, None, edit)
    };
    match made {
        Ok(made) => see(&crate::proposals::paths::proposal(
            ctx.slug(),
            made.id.as_str(),
        )),
        Err(ProposalError::Invalid(why)) => match why {
            Invalid::SummaryMissing => again(
                &typed,
                &[Problem {
                    control: Some(f::SUMMARY.to_owned()),
                    text: words::invalid_text(t, why),
                }],
                Vec::new(),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            other => again(
                &typed,
                &[Problem {
                    control: None,
                    text: words::invalid_text(t, other),
                }],
                Vec::new(),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        },
        Err(error) => refused(page, ctx, &error, Vec::new()),
    }
}

/// The files a change writes: **the governance files that differ from the rules in force, and nothing else**. While the rules
/// cannot be read (the repair), the file that cannot be read is written too, in its canonical text, and so are the paths found
/// standing in its place removed. **The interwiki map is the file that differs from `in_force`**: written in its canonical text
/// ([`Interwiki::to_yaml`]), or removed when no row is left (a map with no prefix is no file). One proposal carries every file.
fn changed_files(rules: &Rules, built: &Built, in_force: &Interwiki) -> Vec<FileChange> {
    let broken = rules.fault().map(ReadFault::file);
    let mut files = Vec::new();
    if let Some(fault) = rules.fault() {
        // What stands where the file belongs, when it is no file: the repair removes it (handoff G-T1 correction D-1).
        files.extend(fault.found().iter().map(|path| FileChange {
            path: path.clone(),
            text: None,
        }));
    }
    if built.constitution != *rules.constitution() || broken == Some(GovernanceFile::Constitution) {
        files.push(FileChange {
            path: GovernanceFile::Constitution.path().to_owned(),
            text: Some(built.constitution.to_yaml()),
        });
    }
    if built.policy != *rules.policy() || broken == Some(GovernanceFile::Policy) {
        files.push(FileChange {
            path: GovernanceFile::Policy.path().to_owned(),
            text: Some(built.policy.to_yaml()),
        });
    }
    // The interwiki map (RFC 007 D-8, W-6). When it is the file that cannot be read, the repair writes what the rows say, or takes
    // the file away when there is none: the wiki is writable again, with that map, or with none (every prefix is then dangling as
    // before). A path the fault lists is already removed above, **and that is all the removal there is to ask**: where the map's
    // place holds a directory, the file at the map's own path is not there to be removed (the acceptance would find nothing to
    // read), and what is below it, once removed, leaves no directory (Y-1).
    let map_path = GovernanceFile::Interwiki.path();
    let text = (!built.map.is_empty()).then(|| built.map.to_yaml());
    if text.is_some() && (built.map != *in_force || broken == Some(GovernanceFile::Interwiki)) {
        files.push(FileChange {
            path: map_path.to_owned(),
            text,
        });
    } else if text.is_none()
        && (!in_force.is_empty() || broken == Some(GovernanceFile::Interwiki))
        && !files
            .iter()
            .any(|c| GovernanceFile::at_or_below(&c.path) == Some(GovernanceFile::Interwiki))
    {
        files.push(FileChange {
            path: map_path.to_owned(),
            text: None,
        });
    }
    files
}

/// The files *back to the default rules* removes: each of the two files of the rules that is there (and what stands at its place).
/// **The interwiki map is not a rule of who may do what, and stays**, unless it is the file that cannot be read: then it is
/// removed with the rest, as a repair (RFC 007 D-8).
fn defaults_files(ctx: &Ctx, rules: &Rules, base: &Base) -> Result<Vec<FileChange>, ()> {
    let mut files = Vec::new();
    if let Some(fault) = rules.fault() {
        files.extend(fault.found().iter().map(|path| FileChange {
            path: path.clone(),
            text: None,
        }));
    }
    let broken = rules.fault().map(ReadFault::file);
    for file in GovernanceFile::ALL {
        if file == GovernanceFile::Interwiki && broken != Some(GovernanceFile::Interwiki) {
            continue;
        }
        let there = ctx
            .services
            .read
            .file_exists_at(&ctx.wiki, &base.block_id, file.path())
            .map_err(|e| ctx.log(&format!("a governance file could not be asked for: {e}")))?;
        if there && !files.iter().any(|c| c.path == file.path()) {
            files.push(FileChange {
                path: file.path().to_owned(),
                text: None,
            });
        }
    }
    Ok(files)
}
