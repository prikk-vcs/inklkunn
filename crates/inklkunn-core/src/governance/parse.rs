//! Reading the two files: the text as a [`Value`] on the frontmatter's machinery
//! ([`inklkunn_markup::frontmatter`], the plain subset: no anchors, no tags, no repeated keys), then the schema.
//!
//! Every step refuses with a [`Fault`] that names the file, the schema's field and (where it can be said) the line,
//! and never what the file said. **Nothing is read from a file that is not wholly well formed**: a refusal anywhere
//! gives no policy at all, never half of one.

use inklkunn_markup::Value;
use inklkunn_markup::frontmatter::parse_yaml;
use inklkunn_store::Role;

use super::chars::is_forbidden;
use super::fault::{Fault, GovernanceFile as File, OrderingPair, Refusal};
use super::locate::{Step, line_of};
use super::protect::{Protection, Require, Target};
use super::{
    Acceptance, AppealRung, Constitution, Entry, GrantUpTo, MAX_APPROVALS, MAX_PROMOTION_CHARS,
    MAX_PROTECT, MAX_REASON_CHARS, MAX_REVIEW_AFTER_DAYS, Policy, Requirement, Rungs,
    SelfAcceptance,
};

type Pairs = [(String, Value)];

/// A refusal on its way up: the fault, and the key of the file that was unknown or repeated, kept only to find its
/// line (it is never shown).
pub(super) struct Hit {
    fault: Fault,
    sub: Option<String>,
}

impl From<Fault> for Hit {
    fn from(fault: Fault) -> Hit {
        Hit { fault, sub: None }
    }
}

type Parsed<T> = Result<T, Hit>;

fn raise(file: File, refusal: Refusal, field: &'static str) -> Hit {
    Fault::at(file, refusal, field).into()
}

/// The fault of a refusal found after the parse, with the line of the key it is about when there is none yet.
pub(super) fn locate_fault(text: &str, fault: Fault) -> Fault {
    finish(text, fault.into())
}

/// The fault of a hit, with the line of the key it is about when the parser gave none.
fn finish(text: &str, hit: Hit) -> Fault {
    let Hit { mut fault, sub } = hit;
    if fault.line.is_none() {
        let mut steps = Vec::new();
        let mut keys = fault.field.split('.').filter(|k| !k.is_empty());
        if let Some(first) = keys.next() {
            steps.push(Step::Key(first.to_owned()));
            if first == "protect"
                && let Some(n) = fault.item
            {
                steps.push(Step::Item(n));
            }
            steps.extend(keys.map(|k| Step::Key(k.to_owned())));
        }
        if let Some(sub) = sub {
            steps.push(Step::Key(sub));
        }
        fault.line = line_of(text, &steps);
    }
    fault
}

/// The text as a mapping: its size, its characters and the YAML subset judged.
fn prepare(file: File, text: &str) -> Result<Value, Fault> {
    if text.len() > file.max_bytes() {
        return Err(Fault::whole(file, Refusal::TooLong));
    }
    for (i, line) in text.lines().enumerate() {
        if line.chars().any(|c| is_forbidden(c) && c != '\t') {
            return Err(Fault {
                line: Some(i + 1),
                ..Fault::whole(file, Refusal::ForbiddenCharacter)
            });
        }
    }
    let value = parse_yaml(text).map_err(|e| Fault::from_yaml(file, &e))?;
    match value {
        Value::Map(_) => Ok(value),
        _ => Err(Fault::whole(file, Refusal::WrongShape)),
    }
}

/// A value as a mapping whose keys are all in `allowed` and none repeated.
fn mapping<'a>(
    file: File,
    value: &'a Value,
    field: &'static str,
    allowed: &[&str],
) -> Parsed<&'a Pairs> {
    let Value::Map(pairs) = value else {
        return Err(raise(file, Refusal::WrongShape, field));
    };
    for (i, (key, _)) in pairs.iter().enumerate() {
        let sub = Some(key.clone());
        if !allowed.contains(&key.as_str()) {
            return Err(Hit {
                fault: Fault::at(file, Refusal::UnknownKey, field),
                sub,
            });
        }
        if pairs.iter().skip(i + 1).any(|(other, _)| other == key) {
            return Err(Hit {
                fault: Fault::at(file, Refusal::RepeatedKey, field),
                sub,
            });
        }
    }
    Ok(pairs)
}

/// The value of `key`, if the mapping has it with something in it (`key:` and nothing is as good as absent).
fn optional<'a>(pairs: &'a Pairs, key: &str) -> Option<&'a Value> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .filter(|v| !matches!(v, Value::Null))
}

/// A word of a closed list.
fn word<E>(
    file: File,
    value: &Value,
    field: &'static str,
    from_word: fn(&str) -> Option<E>,
) -> Parsed<E> {
    match value {
        Value::Str(s) => from_word(s).ok_or_else(|| raise(file, Refusal::UnknownValue, field)),
        _ => Err(raise(file, Refusal::WrongShape, field)),
    }
}

/// A whole number from `min` to `max`.
fn number(file: File, value: &Value, field: &'static str, min: i64, max: i64) -> Parsed<i64> {
    match value {
        Value::Int(n) if (min..=max).contains(n) => Ok(*n),
        Value::Int(_) => Err(raise(file, Refusal::OutOfRange, field)),
        _ => Err(raise(file, Refusal::WrongShape, field)),
    }
}

/// A one-line text of at most `max` characters, not empty.
fn text(file: File, value: &Value, field: &'static str, max: usize) -> Parsed<String> {
    let s = match value {
        Value::Str(s) | Value::Date(s) => s,
        _ => return Err(raise(file, Refusal::WrongShape, field)),
    };
    if s.trim().is_empty() {
        return Err(raise(file, Refusal::Missing, field));
    }
    if s.chars().count() > max {
        return Err(raise(file, Refusal::Bounds, field));
    }
    if s.chars().any(is_forbidden) {
        return Err(raise(file, Refusal::ForbiddenCharacter, field));
    }
    Ok(s.clone())
}

fn rung(file: File, pairs: &Pairs, key: &str, field: &'static str, default: Role) -> Parsed<Role> {
    optional(pairs, key).map_or(Ok(default), |v| word(file, v, field, Role::parse))
}

fn approvals(file: File, pairs: &Pairs, field: &'static str, default: u8) -> Parsed<u8> {
    match optional(pairs, "approvals") {
        None => Ok(default),
        Some(v) => number(file, v, field, 0, i64::from(MAX_APPROVALS))
            .map(|n| u8::try_from(n).unwrap_or(MAX_APPROVALS)),
    }
}

// ----------------------------------------------------------------------------------------------- the policy

const POLICY_KEYS: [&str; 6] = [
    "entry",
    "rungs",
    "acceptance",
    "knowledge",
    "promotion",
    "protect",
];
const RUNG_KEYS: [&str; 8] = [
    "comment",
    "propose",
    "review",
    "create_view",
    "accept",
    "moderate",
    "decide_appeal",
    "grant_up_to",
];
const PROTECT_KEYS: [&str; 4] = ["path", "namespace", "require", "reason"];

/// The field name of a promotion text, from the closed set of the schema's own words.
const fn promotion_field(role: Role) -> &'static str {
    match role {
        Role::Observer => "promotion.observer",
        Role::Questioner => "promotion.questioner",
        Role::Contributor => "promotion.contributor",
        Role::Reviewer => "promotion.reviewer",
        Role::Maintainer => "promotion.maintainer",
        Role::Admin => "promotion.admin",
    }
}

/// Read the text of `governance/policy.yaml`. The ordering rule is judged; the platform limits are not (they are the
/// operator's and change: [`PlatformLimits::check`](super::PlatformLimits::check)).
pub(super) fn parse_policy(text: &str) -> Result<Policy, Fault> {
    let file = File::Policy;
    let value = prepare(file, text)?;
    build_policy(file, &value).map_err(|hit| finish(text, hit))
}

fn build_policy(file: File, value: &Value) -> Parsed<Policy> {
    let top = mapping(file, value, "", &POLICY_KEYS)?;
    let mut policy = Policy::default();
    if let Some(v) = optional(top, "entry") {
        policy.entry = word(file, v, "entry", Entry::parse)?;
    }
    if let Some(v) = optional(top, "rungs") {
        let pairs = mapping(file, v, "rungs", &RUNG_KEYS)?;
        let d = Rungs::default();
        policy.rungs = Rungs {
            comment: rung(file, pairs, "comment", "rungs.comment", d.comment)?,
            propose: rung(file, pairs, "propose", "rungs.propose", d.propose)?,
            review: rung(file, pairs, "review", "rungs.review", d.review)?,
            create_view: rung(
                file,
                pairs,
                "create_view",
                "rungs.create_view",
                d.create_view,
            )?,
            accept: rung(file, pairs, "accept", "rungs.accept", d.accept)?,
            moderate: rung(file, pairs, "moderate", "rungs.moderate", d.moderate)?,
            decide_appeal: match optional(pairs, "decide_appeal") {
                None => d.decide_appeal,
                Some(v) => word(file, v, "rungs.decide_appeal", AppealRung::parse)?,
            },
            grant_up_to: match optional(pairs, "grant_up_to") {
                None => d.grant_up_to,
                Some(v) => word(file, v, "rungs.grant_up_to", GrantUpTo::parse)?,
            },
        };
    }
    if let Some(v) = optional(top, "acceptance") {
        let pairs = mapping(file, v, "acceptance", &["approvals", "self"])?;
        let d = Acceptance::default();
        policy.acceptance = Acceptance {
            approvals: approvals(file, pairs, "acceptance.approvals", d.approvals)?,
            self_acceptance: match optional(pairs, "self") {
                None => d.self_acceptance,
                Some(v) => word(file, v, "acceptance.self", SelfAcceptance::parse)?,
            },
        };
    }
    if let Some(v) = optional(top, "knowledge") {
        let pairs = mapping(file, v, "knowledge", &["review_after"])?;
        if let Some(v) = optional(pairs, "review_after") {
            let days = number(
                file,
                v,
                "knowledge.review_after",
                1,
                i64::from(MAX_REVIEW_AFTER_DAYS),
            )?;
            policy.review_after = Some(u32::try_from(days).unwrap_or(MAX_REVIEW_AFTER_DAYS));
        }
    }
    if let Some(v) = optional(top, "promotion") {
        let allowed: Vec<&str> = Role::ALL.iter().map(|r| r.as_str()).collect();
        let pairs = mapping(file, v, "promotion", &allowed)?;
        for (key, v) in pairs {
            let Some(role) = Role::parse(key) else {
                continue;
            };
            if matches!(v, Value::Null) {
                continue;
            }
            let text = text(file, v, promotion_field(role), MAX_PROMOTION_CHARS)?;
            policy.promotion.insert(role, text);
        }
    }
    if let Some(v) = optional(top, "protect") {
        policy.protect = protections(file, v)?;
    }
    ordering(file, &policy.rungs)?;
    Ok(policy)
}

fn protections(file: File, value: &Value) -> Parsed<Vec<Protection>> {
    let Value::Seq(items) = value else {
        return Err(raise(file, Refusal::WrongShape, "protect"));
    };
    if items.len() > MAX_PROTECT {
        return Err(raise(file, Refusal::Bounds, "protect"));
    }
    let mut out: Vec<Protection> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let n = i + 1;
        let protection = protection(file, item).map_err(|mut hit| {
            hit.fault.item = Some(n);
            hit
        })?;
        if out.iter().any(|p| p.target == protection.target) {
            let mut fault = Fault::at(file, Refusal::Duplicate, "protect");
            fault.item = Some(n);
            return Err(fault.into());
        }
        out.push(protection);
    }
    Ok(out)
}

fn protection(file: File, value: &Value) -> Parsed<Protection> {
    let pairs = mapping(file, value, "protect", &PROTECT_KEYS)?;
    let target = match (optional(pairs, "path"), optional(pairs, "namespace")) {
        (Some(_), Some(_)) => return Err(raise(file, Refusal::WrongShape, "protect")),
        (None, None) => return Err(raise(file, Refusal::Missing, "protect.path")),
        (Some(v), None) => {
            let title = text(file, v, "protect.path", super::MAX_TARGET_CHARS)?;
            Target::path(&title)
                .ok_or_else(|| raise(file, Refusal::UnknownValue, "protect.path"))?
        }
        (None, Some(v)) => {
            let title = text(file, v, "protect.namespace", super::MAX_TARGET_CHARS)?;
            Target::namespace(&title)
                .ok_or_else(|| raise(file, Refusal::UnknownValue, "protect.namespace"))?
        }
    };
    let require = match optional(pairs, "require") {
        None => return Err(raise(file, Refusal::Missing, "protect.require")),
        Some(Value::Str(s)) if s == "review" => Require::Review,
        // Any other word, a rung's among them (`reviewer` is not `review`), is refused, not read as either.
        Some(Value::Str(_)) => return Err(raise(file, Refusal::UnknownValue, "protect.require")),
        Some(v @ Value::Map(_)) => {
            let inner = mapping(file, v, "protect.require", &["rung"])?;
            let rung = optional(inner, "rung")
                .ok_or_else(|| raise(file, Refusal::Missing, "protect.require.rung"))?;
            Require::Rung(word(file, rung, "protect.require.rung", Role::parse)?)
        }
        Some(_) => return Err(raise(file, Refusal::WrongShape, "protect.require")),
    };
    let reason = match optional(pairs, "reason") {
        None => return Err(raise(file, Refusal::Missing, "protect.reason")),
        Some(v) => text(file, v, "protect.reason", MAX_REASON_CHARS)?,
    };
    Ok(Protection {
        target,
        require,
        reason,
    })
}

/// The ordering rule (RFC 006 D-2): `propose` ≤ `review` ≤ `accept`, `moderate` ≥ reviewer, and `decide_appeal` ≥ `moderate`
/// (A-4: whoever decides appeals can reverse what a moderator did, so they must be able to do it: `decide_appeal: maintainer`
/// beside `moderate: admin` would let a maintainer undo an admin's hiding or restriction). The first pair broken, in that
/// order, is the one named.
fn ordering(file: File, r: &Rungs) -> Parsed<()> {
    let broken = if r.propose > r.review {
        Some((OrderingPair::ProposeAboveReview, "rungs.propose"))
    } else if r.review > r.accept {
        Some((OrderingPair::ReviewAboveAccept, "rungs.review"))
    } else if r.moderate < Role::Reviewer {
        Some((OrderingPair::ModerateBelowReviewer, "rungs.moderate"))
    } else if r.decide_appeal.role() < r.moderate {
        Some((
            OrderingPair::DecideAppealBelowModerate,
            "rungs.decide_appeal",
        ))
    } else {
        None
    };
    match broken {
        Some((pair, field)) => Err(raise(file, Refusal::Ordering(pair), field)),
        None => Ok(()),
    }
}

// ------------------------------------------------------------------------------------------ the constitution

/// Read the text of `governance/constitution.yaml`.
pub(super) fn parse_constitution(text: &str) -> Result<Constitution, Fault> {
    let file = File::Constitution;
    let value = prepare(file, text)?;
    build_constitution(file, &value).map_err(|hit| finish(text, hit))
}

/// A policy change's rung: a maintainer or an admin.
fn change_rung(word: &str) -> Option<Role> {
    Role::parse(word).filter(|r| matches!(r, Role::Maintainer | Role::Admin))
}

fn build_constitution(file: File, value: &Value) -> Parsed<Constitution> {
    let top = mapping(file, value, "", &["policy_change", "constitution_change"])?;
    let mut out = Constitution::default();
    if let Some(v) = optional(top, "policy_change") {
        let pairs = mapping(file, v, "policy_change", &["rung", "approvals"])?;
        let d = out.policy_change;
        out.policy_change = Requirement {
            rung: match optional(pairs, "rung") {
                None => d.rung,
                Some(v) => word(file, v, "policy_change.rung", change_rung)?,
            },
            approvals: approvals(file, pairs, "policy_change.approvals", d.approvals)?,
        };
    }
    if let Some(v) = optional(top, "constitution_change") {
        // The rung is fixed at admin and is no key: `rung` here is an unknown key.
        let pairs = mapping(file, v, "constitution_change", &["approvals"])?;
        out.constitution_change.approvals = approvals(
            file,
            pairs,
            "constitution_change.approvals",
            out.constitution_change.approvals,
        )?;
    }
    Ok(out)
}
