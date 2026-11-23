//! The governance read endpoints (RFC 006 D-12; handoff G-T7): the rules of a wiki as data, the protections in force, the
//! moderation log and a person's own appeals. **They only read, and no write route is added**: moderation, restrictions,
//! protections, grants and appeal decisions are web-only in 0.4.0.
//!
//! **Authorized as the pages are, and existence-neutral.** The start is every endpoint's ([`serve`]): a wiki that is absent and
//! one the caller may not read answer the same problem before anything else, and nothing here is asked for before that. Every
//! answer is read through what the pages read: the rules of the authorizer, the protections `authorize()` evaluates, the
//! moderation service's log (which never carries content, a name or a note to a caller who is not the `moderate` rung, and
//! carries none of them here to anyone), and the appeal service's own list of the caller's appeals.

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::Principal;
use inklkunn_core::appeal::{AppealError, AppealRow, Outcome};
use inklkunn_core::governance::{
    ApprovalsWarning, Constitution, Lowered, Policy, ReadFault, Require, Rules, Target,
};
use inklkunn_core::protection::Notice;
use inklkunn_core::tombstone::LogRow;
use inklkunn_store::{HiddenItemKind, TombstoneAct, TombstoneEventId};
use serde_json::{Value, json};

use crate::api::describe;
use crate::api::problem::{Code, Problem};
use crate::api::reply::Reply;
use crate::api::request::serve;
use crate::api::write::refuse::rules_unreadable_problem;
use crate::pages::PageState;
use crate::policy::rules_data::gather;
use crate::services::Services;

/// How many rows a page of the moderation log holds when `limit` is not given.
const LOG_DEFAULT: usize = 50;

fn problem_of(services: &Services, wiki: &inklkunn_store::WikiRecord, error: &str) -> Problem {
    (services.log)(&format!(
        "the API could not read the rules of wiki `{}` now: {error}",
        wiki.slug
    ));
    Problem::new(Code::Unavailable)
}

fn protection_json(target: &Target, require: Require, reason: &str) -> Value {
    let (on, title) = match target {
        Target::Path(title) => ("page", title),
        Target::Namespace(title) => ("namespace", title),
    };
    let (requires, role) = match require {
        Require::Review => ("review", None),
        Require::Rung(role) => ("rung", Some(role.as_str())),
    };
    json!({
        "on": on,
        "title": title,
        "requires": requires,
        "role": role,
        "reason": reason,
    })
}

fn policy_json(policy: &Policy, server_days: u32) -> Value {
    let r = policy.rungs();
    let promotion: serde_json::Map<String, Value> = policy
        .promotions()
        .map(|(role, text)| (role.as_str().to_owned(), json!(text)))
        .collect();
    json!({
        "entry": policy.entry().word(),
        "rungs": {
            "comment": r.comment().as_str(),
            "propose": r.propose().as_str(),
            "review": r.review().as_str(),
            "create_view": r.create_view().as_str(),
            "accept": r.accept().as_str(),
            "moderate": r.moderate().as_str(),
            "decide_appeal": r.decide_appeal().word(),
            "grant_up_to": r.grant_up_to().word(),
        },
        "acceptance": {
            "approvals": policy.acceptance().approvals(),
            "self": policy.acceptance().self_acceptance().word(),
        },
        "knowledge": {
            "review_after": policy.review_after(),
            "in_force": policy.review_after_days(server_days),
        },
        "promotion": promotion,
        "protect": policy
            .protections()
            .iter()
            .map(|p| protection_json(p.target(), p.require(), p.reason()))
            .collect::<Vec<_>>(),
    })
}

fn constitution_json(constitution: &Constitution) -> Value {
    let rules = constitution.policy_change();
    let who = constitution.constitution_change();
    json!({
        "policy_change": {"rung": rules.rung.as_str(), "approvals": rules.approvals},
        "constitution_change": {"rung": who.rung.as_str(), "approvals": who.approvals},
    })
}

fn fault_json(fault: &ReadFault) -> Value {
    let mut out = json!({
        "file": fault.file().path(),
        "rule": fault.refusal().name(),
        "line": fault.line(),
    });
    if !fault.found().is_empty()
        && let Some(map) = out.as_object_mut()
    {
        map.insert("found".to_owned(), json!(fault.found()));
        map.insert("found_more".to_owned(), json!(fault.more()));
    }
    out
}

fn lowered_json(rules: &Rules) -> Vec<Value> {
    rules
        .lowered()
        .iter()
        .map(|lowered| match lowered {
            Lowered::Entry { was, now } => json!({
                "field": lowered.field(),
                "was": was.word(),
                "now": now.word(),
                "limit": lowered.limit().variable(),
            }),
        })
        .collect()
}

fn warning_json(warning: Option<ApprovalsWarning>) -> Value {
    warning.map_or(
        Value::Null,
        |w| json!({"needed": w.needed, "available": w.available}),
    )
}

/// `GET /{slug}/-/api/v1/policy`.
pub(super) async fn policy(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::POLICY,
        |services, wiki, _, ctx| {
            let data = gather(services, wiki, &ctx.principal, false)
                .map_err(|e| problem_of(services, wiki, &e))?;
            let rules = &data.rules;
            Reply::json(&json!({
                "block": rules.block(),
                "defaults": rules.block().is_none() && rules.fault().is_none(),
                "fault": rules.fault().map(fault_json),
                "policy": policy_json(rules.policy(), data.server_days),
                "constitution": constitution_json(rules.constitution()),
                "lowered": lowered_json(rules),
                "platform_limits": {
                    "entry_max": data.limits.entry_max.word(),
                    "limit_days": data.limits.limit_days,
                },
                "approvals_warning": warning_json(data.warning),
            }))
        },
    )
    .await
}

fn notice_json(notice: &Notice) -> Value {
    let mut out = protection_json(notice.target(), notice.require(), notice.reason());
    if let Some(map) = out.as_object_mut() {
        map.insert(
            "kind".to_owned(),
            json!(if notice.until().is_some() {
                "temporary"
            } else {
                "standing"
            }),
        );
        map.insert(
            "until".to_owned(),
            json!(notice.until().map(crate::time::rfc3339)),
        );
        map.insert(
            "set_by".to_owned(),
            notice.set_by().map_or(
                Value::Null,
                |by| json!({"role": by.rung.as_str(), "block": by.block}),
            ),
        );
    }
    out
}

/// `GET /{slug}/-/api/v1/protections`.
pub(super) async fn protections(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::PROTECTIONS,
        |services, wiki, _, ctx| {
            let notices = services
                .authorizer
                .protections_in_force(&ctx.principal, &wiki.id)
                .ok_or_else(|| problem_of(services, wiki, "the protections could not be read"))?;
            Reply::json(&json!({
                "items": notices.iter().map(notice_json).collect::<Vec<_>>(),
            }))
        },
    )
    .await
}

fn parts_json(row: &LogRow) -> Vec<&'static str> {
    let mut parts = Vec::new();
    if row.parts.content {
        parts.push("content");
    }
    if row.parts.summary {
        parts.push("summary");
    }
    if row.parts.author {
        parts.push("author");
    }
    parts
}

/// A row of the log: **no content, no name and no note**.
fn log_row_json(row: &LogRow) -> Value {
    use inklkunn_core::appeal::AppealMark;
    json!({
        "id": row.id.as_str(),
        "act": match row.act {
            TombstoneAct::Hide => "hide",
            TombstoneAct::Unhide => "unhide",
        },
        "at": crate::time::rfc3339(row.at),
        "item": {
            "kind": row.item.kind.as_str(),
            "id": row.item.id,
            // The revision's number is part of what a row names; a page revision's reference is a page's path, which this does not say.
            "reference": (row.item.kind == HiddenItemKind::ProposalRevision)
                .then_some(row.item.reference.as_deref())
                .flatten(),
        },
        "parts": parts_json(row),
        "category": row.category.map(|c| c.as_str()),
        "rung": row.rung.as_str(),
        "policy_block": row.policy_block,
        "in_force": row.in_force,
        "declined": row.declined,
        "appeal": row.appeal.map(|mark| match mark {
            AppealMark::Appealed => "appealed",
            AppealMark::Upheld => "upheld",
            AppealMark::Overturned => "overturned",
        }),
        "restored_by_appeal": row.by_appeal,
    })
}

/// `GET /{slug}/-/api/v1/moderation-log`.
pub(super) async fn moderation_log(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::MODERATION_LOG,
        |services, wiki, params, ctx| {
            let limit = params
                .int("limit")
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(LOG_DEFAULT);
            // The cursor is the id of the last row of the page before; nothing else is one.
            let before = match params.text("cursor") {
                None => None,
                Some(text) => Some(
                    TombstoneEventId::parse(text)
                        .ok_or_else(|| Problem::new(Code::InvalidCursor))?,
                ),
            };
            let rows = services
                .moderation
                .log(&ctx.principal, wiki, before.as_ref(), limit + 1)
                .map_err(|error| match error {
                    inklkunn_core::tombstone::ModerationError::NotFound => {
                        Problem::new(Code::NotFound)
                    }
                    inklkunn_core::tombstone::ModerationError::NoSuchHiding => {
                        Problem::new(Code::InvalidCursor)
                    }
                    other => problem_of(services, wiki, &other.to_string()),
                })?;
            let more = rows.len() > limit;
            let shown: Vec<&LogRow> = rows.iter().take(limit).collect();
            Reply::json(&json!({
                "items": shown.iter().map(|row| log_row_json(row)).collect::<Vec<_>>(),
                "next_cursor": more
                    .then(|| shown.last().map(|row| row.id.as_str().to_owned()))
                    .flatten(),
            }))
        },
    )
    .await
}

fn appeal_json(row: &AppealRow) -> Value {
    let (status, decision) = match &row.decision {
        None => ("open", Value::Null),
        Some(d) => (
            match d.outcome {
                Outcome::Upheld => "upheld",
                Outcome::Overturned => "overturned",
            },
            // Who decided is never said here: the caller was told the outcome and the reason.
            json!({"at": crate::time::rfc3339(d.at), "reason": d.reason}),
        ),
    };
    json!({
        "id": row.id.as_str(),
        "at": crate::time::rfc3339(row.at),
        "action": {"kind": row.action.kind.as_str(), "id": row.action.id},
        "proposal": row.proposal.as_ref().map(|p| p.as_str()),
        "text": row.text,
        "status": status,
        "decision": decision,
    })
}

/// `GET /{slug}/-/api/v1/appeals/mine`.
pub(super) async fn my_appeals(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::MY_APPEALS,
        |services, wiki, _, ctx| {
            let view =
                services
                    .appeals
                    .mine(&ctx.principal, wiki)
                    .map_err(|error| match error {
                        AppealError::NotFound => Problem::new(Code::NotFound),
                        AppealError::NotAllowed
                            if matches!(ctx.principal, Principal::Anonymous) =>
                        {
                            Problem::new(Code::AuthenticationRequired)
                        }
                        AppealError::NotAllowed => Problem::new(Code::Forbidden),
                        AppealError::RulesUnreadable(fault) => rules_unreadable_problem(&fault),
                        other => problem_of(services, wiki, &format!("{other:?}")),
                    })?;
            Reply::json(&json!({
                "items": view.appeals.iter().map(appeal_json).collect::<Vec<_>>(),
            }))
        },
    )
    .await
}
