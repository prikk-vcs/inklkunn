//! The governance endpoints, as the document describes them (RFC 006 D-12; handoff G-T7): the rules of a wiki as data, the
//! protections in force, the moderation log and a person's own appeals. **They only read.** Moderation, restrictions,
//! protections, grants and appeal decisions are web-only in 0.4.0: deliberate human acts, and a scripted moderator is the
//! automated sanction GV-7 rules out. A change to the rules goes through the propose API, as the governance files.
//!
//! Each is authorized as the pages are (`read`) and existence-neutral: a wiki that is absent and one the caller may not read are
//! one `not-found`, byte for byte.

use super::{Answer, CURSOR, Doc, LIMIT, Param};
use crate::api::problem::Code;

const NO_PARAMS: &[Param] = &[];

const LOG_PARAMS: &[Param] = &[LIMIT, CURSOR];

pub(crate) const POLICY: Doc = Doc {
    path: "/{slug}/-/api/v1/policy",
    operation_id: "getPolicy",
    tag: "governance",
    summary: "Get the rules of a wiki, as data",
    description: "The rules in force at the default view's tip: who may do what (`policy.rungs`, `policy.entry`), how a proposal is accepted (`policy.acceptance`), the wiki's review interval for sources (`policy.knowledge`, with the `in_force` days), the texts it gives for the roles (`policy.promotion`), its standing protections (`policy.protect`) and who may change the rules (`constitution`), each as the schema's own words. `block` is the change that set them, the one a decision names as the rules it was made under; `null` with `defaults` true for a wiki with no governance files, which follows the default rules. `lowered` lists a value the wiki's rules say that a limit of the server lowered (the value in force is the one in `policy`), `platform_limits` the server's two limits, and `approvals_warning` is set when the rules ask for more approvals than the people who could give them. **While a governance file cannot be read** `fault` names the file, the rule and the line (never what the file said), `policy` and `constitution` are the default rules, and every write of the wiki is refused until the repair; the read is not. Moderation, restrictions, protections, grants and the decisions of appeals are web-only in 0.4.0: they are deliberate human acts, and this API only reads their results. A change to the rules is proposed through the propose API, as the governance files.",
    query: NO_PARAMS,
    answer: Answer::Schema("Policy"),
    write: None,
    problems: &[],
};

pub(crate) const PROTECTIONS: Doc = Doc {
    path: "/{slug}/-/api/v1/protections",
    operation_id: "listProtections",
    tag: "governance",
    summary: "List the protections in force",
    description: "Every protection in force now, the wiki's standing ones first and then the temporary ones that have not ended, each once, as the pages show them: its `kind`, what it is `on` (a page, or every page below a title and never the page of that title), what it `requires` (a `review`, or a `rung`: the `role`), its `reason` in the words of whoever set it, `until` for a temporary one, and `set_by` for a temporary one: the role of whoever put it in place and the block of the rules they did it under (never a name). A protection asks more of whoever accepts a change; it never restricts who may propose. Moderation, restrictions, protections, grants and the decisions of appeals are web-only in 0.4.0: they are deliberate human acts, and this API only reads their results.",
    query: NO_PARAMS,
    answer: Answer::Schema("Protections"),
    write: None,
    problems: &[],
};

pub(crate) const MODERATION_LOG: Doc = Doc {
    path: "/{slug}/-/api/v1/moderation-log",
    operation_id: "listModerationLog",
    tag: "governance",
    summary: "List the moderation log",
    description: "Each hide and each restoring of the wiki, newest first, one page of the list at a time (follow `next_cursor` until it is `null`): the `item` by kind and identifier, which `parts` it hid, the reason `category`, the role that acted (`rung`) and the block of the rules it acted under (`policy_block`), when (`at`), whether the hiding is `in_force`, whether it declined a proposal in the same act, and, for a hiding, whether it was `appealed` and how that ended. **Never the hidden content, never a name and never a moderator's note**: the log says what was hidden and why in the closed set of reasons, and that an appeal was made, never what it said. Moderation, restrictions, protections, grants and the decisions of appeals are web-only in 0.4.0: they are deliberate human acts, and this API only reads their results.",
    query: LOG_PARAMS,
    answer: Answer::Schema("ModerationLog"),
    write: None,
    problems: &[Code::InvalidParameter, Code::InvalidCursor],
};

pub(crate) const MY_APPEALS: Doc = Doc {
    path: "/{slug}/-/api/v1/appeals/mine",
    operation_id: "listMyAppeals",
    tag: "governance",
    summary: "List the caller's own appeals",
    description: "The appeals the calling account made in this wiki, newest first: each with the action it is against (`kind` and `id`), the text the account wrote, its `status` (`open`, `upheld` or `overturned`) and, once decided, the decision (when, the reason given, the role the decider held as `rung` and the change that set the rules it was held under as `policy_block`: `null` for `rung` is a decision that was recorded without them, and `null` for `policy_block` beside a `rung` is the default rules). **Only the caller's own, never anyone else's**, and nobody else's decider is named, only their role. It needs an account: a caller with no credential is `authentication-required`, and a credential that is not an account's is `forbidden`. While a governance file of the wiki cannot be read the answer is `forbidden` with `reason` `rules-unreadable`, as the page's. Moderation, restrictions, protections, grants and the decisions of appeals are web-only in 0.4.0: filing an appeal and deciding one are done on the site, and this API only reads.",
    query: NO_PARAMS,
    answer: Answer::Schema("MyAppeals"),
    write: None,
    problems: &[Code::AuthenticationRequired, Code::Forbidden],
};
