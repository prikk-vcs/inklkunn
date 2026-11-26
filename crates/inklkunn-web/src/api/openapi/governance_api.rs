//! The schemas of the governance endpoints (RFC 006 D-12; handoff G-T7): the rules of a wiki as data, the protections in force,
//! the moderation log and a person's own appeals. **They only read.** The enumerations are read from the files' own words
//! (`inklkunn_core::governance`) and the stores' (`inklkunn_store`), so the document cannot say a word the rules do not. A test
//! holds each endpoint's real answer to its schema's members (`tests/api/governance.rs`).

use inklkunn_core::governance::{AppealRung, Entry, GrantUpTo, Limit, SelfAcceptance};
use inklkunn_store::{HiddenItemKind, ReasonCategory, Role};
use serde_json::{Map, Value, json};

use super::shapes::{object, schema_ref};

fn words<T: Copy>(all: &[T], word: impl Fn(T) -> &'static str) -> Vec<&'static str> {
    all.iter().map(|w| word(*w)).collect()
}

fn roles() -> Vec<&'static str> {
    Role::ALL.iter().map(|r| r.as_str()).collect()
}

/// The schemas, by name.
pub(super) fn schemas() -> Map<String, Value> {
    let role = json!({"type": "string", "enum": roles(), "description": "A role of the ladder, lowest first."});
    let block = json!({"type": ["string", "null"], "description": "The change that set the rules in force when it was done: the id of a change of the default view. `null` for the default rules (a wiki with no governance files)."});
    let mut out = Map::new();
    out.insert(
        "Protection".to_owned(),
        object(
            "One protection: what it covers, what it asks and why. A standing one is the wiki's rules'; a temporary one was put in place by the role that moderates, for a time.",
            json!({
                "on": {"type": "string", "enum": ["page", "namespace"], "description": "One page, or every page below a title (never the page of that title)."},
                "title": {"type": "string", "description": "The page's title, or the namespace."},
                "requires": {"type": "string", "enum": ["review", "rung"], "description": "An approval first, or a role."},
                "role": {"type": ["string", "null"], "enum": roles().into_iter().map(Value::from).chain([Value::Null]).collect::<Vec<_>>(), "description": "The role it asks, for `rung`."},
                "reason": {"type": "string", "description": "Why, in the words of whoever set it."},
            }),
            &[],
        ),
    );
    out.insert(
        "Policy".to_owned(),
        object(
            "The rules of a wiki in force, as data (RFC 006 D-2), with the change that set them. The words are the schema's own.",
            json!({
                "block": block,
                "defaults": {"type": "boolean", "description": "True when the wiki has no governance files: it follows the default rules."},
                "fault": {
                    "oneOf": [
                        {"type": "null"},
                        object(
                            "A governance file that cannot be read: the wiki refuses every write until it is repaired, and still reads. Never what the file said.",
                            json!({
                                "file": {"type": "string", "description": "`governance/policy.yaml`, `governance/constitution.yaml` or `governance/interwiki.yaml`."},
                                "rule": {"type": "string", "description": "The rule it breaks, by its stable name (`not-a-file` where something that is not an ordinary file stands at its place)."},
                                "line": {"type": ["integer", "null"], "description": "The line, from 1, where it can be said."},
                                "found": {"type": "array", "items": {"type": "string"}, "description": "For `not-a-file`: the repository paths found at or below the file's place, at most 50."},
                                "found_more": {"type": "integer", "description": "For `not-a-file`: how many more paths than `found` lists."},
                            }),
                            &["found", "found_more"],
                        ),
                    ],
                    "description": "`null` when every governance file reads. While one does not, `policy` and `constitution` are the default rules.",
                },
                "policy": object(
                    "Who may do what, and how a proposal is accepted.",
                    json!({
                        "entry": {"type": "string", "enum": words(Entry::ALL, Entry::word), "description": "The role every signed-in account holds without a grant (`none`: none). It has no effect in a restricted wiki. As lowered by the server's limit when `lowered` says so."},
                        "rungs": object(
                            "The lowest role for each action.",
                            json!({
                                "comment": role.clone(),
                                "propose": role.clone(),
                                "review": role.clone(),
                                "create_view": role.clone(),
                                "accept": role.clone(),
                                "moderate": role.clone(),
                                "decide_appeal": {"type": "string", "enum": words(AppealRung::ALL, AppealRung::word)},
                                "grant_up_to": {"type": "string", "enum": words(GrantUpTo::ALL, GrantUpTo::word), "description": "The highest role a maintainer may grant (`none`: only admins grant)."},
                            }),
                            &[],
                        ),
                        "acceptance": object(
                            "How a proposal is accepted.",
                            json!({
                                "approvals": {"type": "integer", "minimum": 0, "maximum": 5, "description": "The approvals from people other than the proposer a proposal needs."},
                                "self": {"type": "string", "enum": words(SelfAcceptance::ALL, SelfAcceptance::word), "description": "Whether a maintainer may accept their own proposal: `small-wiki`, while the wiki has fewer than three maintainers."},
                            }),
                            &[],
                        ),
                        "knowledge": object(
                            "How long a source goes before it is due for review.",
                            json!({
                                "review_after": {"type": ["integer", "null"], "description": "The wiki's own number of days, or `null` when it sets none."},
                                "in_force": {"type": "integer", "description": "The days that apply: the wiki's own, else the server's."},
                            }),
                            &[],
                        ),
                        "promotion": {"type": "object", "description": "What the wiki looks for before granting a role: a text for each role that has one, by the role's name. Guidance for people: nothing promotes by it.", "additionalProperties": {"type": "string"}},
                        "protect": {"type": "array", "description": "The standing protections.", "items": schema_ref("Protection")},
                    }),
                    &[],
                ),
                "constitution": object(
                    "Who may change the rules.",
                    json!({
                        "policy_change": object(
                            "What a change to the policy needs.",
                            json!({"rung": role.clone(), "approvals": {"type": "integer"}}),
                            &[],
                        ),
                        "constitution_change": object(
                            "What a change to the constitution needs (always an admin).",
                            json!({"rung": role.clone(), "approvals": {"type": "integer"}}),
                            &[],
                        ),
                    }),
                    &[],
                ),
                "lowered": {
                    "type": "array",
                    "description": "A value of the wiki's rules that a limit of the server lowered: the value in `policy` is the lowered one.",
                    "items": object(
                        "One lowered value.",
                        json!({
                            "field": {"type": "string", "description": "The key of the rules (`entry`)."},
                            "was": {"type": "string", "description": "What the rules say."},
                            "now": {"type": "string", "description": "What applies."},
                            "limit": {"type": "string", "enum": [Limit::EntryMax.variable(), Limit::LimitDays.variable()], "description": "The operator's setting that lowered it."},
                        }),
                        &[],
                    ),
                },
                "platform_limits": object(
                    "What the operator allows any wiki.",
                    json!({
                        "entry_max": {"type": "string", "enum": words(Entry::ALL, Entry::word), "description": "The highest `entry` a wiki may set."},
                        "limit_days": {"type": "integer", "description": "The longest a temporary protection or restriction may last."},
                    }),
                    &[],
                ),
                "approvals_warning": {
                    "oneOf": [
                        {"type": "null"},
                        object(
                            "The rules ask for more approvals than the people who could give them: no proposal can be accepted until more people hold the role that reviews.",
                            json!({
                                "needed": {"type": "integer", "description": "The approvals the rules ask for."},
                                "available": {"type": "integer", "description": "The people who hold the review role or above."},
                            }),
                            &[],
                        ),
                    ],
                },
            }),
            &[],
        ),
    );
    out.insert(
        "ProtectionInForce".to_owned(),
        object(
            "One protection in force: what it covers, what it asks and why, and whether it is the wiki's rules' or was put in place for a time.",
            json!({
                "on": {"type": "string", "enum": ["page", "namespace"], "description": "One page, or every page below a title (never the page of that title)."},
                "title": {"type": "string", "description": "The page's title, or the namespace."},
                "requires": {"type": "string", "enum": ["review", "rung"], "description": "An approval first, or a role."},
                "role": {"type": ["string", "null"], "enum": roles().into_iter().map(Value::from).chain([Value::Null]).collect::<Vec<_>>(), "description": "The role it asks, for `rung`."},
                "reason": {"type": "string", "description": "Why, in the words of whoever set it."},
                "kind": {"type": "string", "enum": ["standing", "temporary"]},
                "until": {"type": ["string", "null"], "description": "When a temporary protection ends (RFC 3339, UTC); `null` for a standing one."},
                "set_by": {
                    "oneOf": [
                        {"type": "null"},
                        object(
                            "Who put a temporary protection in place: the role they held and the rules in force then. Never a name.",
                            json!({"role": role.clone(), "block": block.clone()}),
                            &[],
                        ),
                    ],
                },
            }),
            &[],
        ),
    );
    out.insert(
        "Protections".to_owned(),
        object(
            "The protections in force now: the wiki's rules' first, then the temporary ones, each once.",
            json!({"items": {"type": "array", "items": schema_ref("ProtectionInForce")}}),
            &[],
        ),
    );
    out.insert(
        "ModerationLogRow".to_owned(),
        object(
            "One hide or restoring. Never the hidden content, never a name and never a moderator's note.",
            json!({
                "id": {"type": "string", "description": "The record's id: also the cursor of the page after it."},
                "act": {"type": "string", "enum": ["hide", "unhide"]},
                "at": {"type": "string", "description": "When (RFC 3339, UTC)."},
                "item": object(
                    "What it is about.",
                    json!({
                        "kind": {"type": "string", "enum": HiddenItemKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>()},
                        "id": {"type": "string", "description": "The item's identifier."},
                        "reference": {"type": ["string", "null"], "description": "A proposal's revision: its number. Otherwise `null`."},
                    }),
                    &[],
                ),
                "parts": {"type": "array", "items": {"type": "string", "enum": ["content", "summary", "author"]}, "description": "Which parts of the item."},
                "category": {"type": ["string", "null"], "enum": ReasonCategory::ALL.iter().map(|c| Value::from(c.as_str())).chain([Value::Null]).collect::<Vec<_>>(), "description": "Why, in the closed set of reasons (an unhide says the one of the hide it follows)."},
                "rung": {"type": "string", "enum": roles(), "description": "The role that acted."},
                "policy_block": block,
                "in_force": {"type": "boolean", "description": "For a hide: it is in force now, and can still be undone."},
                "declined": {"type": "boolean", "description": "The act also declined an open proposal."},
                "appeal": {"type": ["string", "null"], "enum": ["appealed", "upheld", "overturned", null], "description": "For a hide: that it was appealed and how that ended, and nothing else."},
                "restored_by_appeal": {"type": "boolean", "description": "For an unhide: it was made because an appeal was decided."},
            }),
            &[],
        ),
    );
    out.insert(
        "ModerationLog".to_owned(),
        object(
            "A page of the moderation log, newest first.",
            json!({
                "items": {"type": "array", "items": schema_ref("ModerationLogRow")},
                "next_cursor": {"type": ["string", "null"], "description": "Where the next page of the list begins; `null` on the last. Opaque."},
            }),
            &[],
        ),
    );
    out.insert(
        "MyAppeal".to_owned(),
        object(
            "One of the caller's own appeals.",
            json!({
                "id": {"type": "string"},
                "at": {"type": "string", "description": "When it was filed (RFC 3339, UTC)."},
                "action": object(
                    "What it is against.",
                    json!({
                        "kind": {"type": "string", "enum": ["decline", "tombstone", "restriction", "protection", "grant"]},
                        "id": {"type": "string", "description": "The action's record id."},
                    }),
                    &[],
                ),
                "proposal": {"type": ["string", "null"], "description": "For an appeal against a decline: the proposal's id."},
                "text": {"type": "string", "description": "What the caller wrote."},
                "status": {"type": "string", "enum": ["open", "upheld", "overturned"]},
                "decision": {
                    "oneOf": [
                        {"type": "null"},
                        object(
                            "How it was decided. Who decided is not named: only the role they held and the rules in force.",
                            json!({
                                "at": {"type": "string", "description": "When (RFC 3339, UTC)."},
                                "reason": {"type": "string", "description": "The reason given."},
                                "rung": {"type": ["string", "null"], "enum": roles().into_iter().map(Value::from).chain([Value::Null]).collect::<Vec<_>>(), "description": "The role the decider held when they decided. `null`: it was not recorded (a decision made without it)."},
                                "policy_block": {"type": ["string", "null"], "description": "The change that set the rules the role was held under. `null` is the default rules (a wiki with no governance files) when `rung` is set, and not recorded when it is not."},
                            }),
                            &[],
                        ),
                    ],
                },
            }),
            &[],
        ),
    );
    out.insert(
        "MyAppeals".to_owned(),
        object(
            "The caller's own appeals, newest first.",
            json!({"items": {"type": "array", "items": schema_ref("MyAppeal")}}),
            &[],
        ),
    );
    out
}
