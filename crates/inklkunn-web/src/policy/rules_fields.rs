//! The names of the policy form's fields and of its buttons: code a form and a handler agree on, no sentence a reader sees.
//!
//! **None of them is a key of the rules' files**: a field is named for the control it is (`who_accept`), never for the key it
//! sets, so that no page prints a key as it is written in a file.

/// The summary of the proposal.
pub(super) const SUMMARY: &str = "summary";
/// What the button does: [`ACT_SEND`], [`ACT_DEFAULTS`], [`ACT_ADD`] or `remove-<n>`.
pub(super) const ACT: &str = "act";
/// The value of [`ACT`] that sends the change as a proposal.
pub(super) const ACT_SEND: &str = "send";
/// The value of [`ACT`] that proposes going back to the default rules.
pub(super) const ACT_DEFAULTS: &str = "defaults";
/// The value of [`ACT`] that adds an empty protection's row.
pub(super) const ACT_ADD: &str = "add";
/// The prefix of the value of [`ACT`] that removes a protection's row (`remove-2`).
pub(super) const ACT_REMOVE: &str = "remove-";
/// How many protection rows the form sent.
pub(super) const ROWS: &str = "rows";
/// The value of [`ACT`] that adds an empty row to the interwiki map.
pub(super) const ACT_MAP_ADD: &str = "map-add";
/// The prefix of the value of [`ACT`] that removes a row of the interwiki map (`map-remove-2`).
pub(super) const ACT_MAP_REMOVE: &str = "map-remove-";
/// How many interwiki map rows the form sent.
pub(super) const MAP_ROWS: &str = "map_rows";
/// The state of the view the form was opened on: the base of the proposal it makes.
pub(super) const BASE_STATE: &str = "base_state";
/// The change that state belongs to.
pub(super) const BASE_BLOCK: &str = "base_block";

/// Who may comment.
pub(super) const WHO_COMMENT: &str = "who_comment";
/// Who may propose.
pub(super) const WHO_PROPOSE: &str = "who_propose";
/// Who may review.
pub(super) const WHO_REVIEW: &str = "who_review";
/// Who may create a view.
pub(super) const WHO_VIEW: &str = "who_view";
/// Who may accept.
pub(super) const WHO_ACCEPT: &str = "who_accept";
/// Who may moderate.
pub(super) const WHO_MODERATE: &str = "who_moderate";
/// Who decides an appeal.
pub(super) const WHO_APPEALS: &str = "who_appeals";
/// The highest role a maintainer may grant.
pub(super) const WHO_GRANTS: &str = "who_grants";
/// The role everyone who signs in holds.
pub(super) const EVERYONE: &str = "everyone";
/// The approvals a proposal needs.
pub(super) const APPROVALS: &str = "approvals";
/// Whether a person may accept their own proposal.
pub(super) const OWN: &str = "own_proposal";
/// The review interval, in days.
pub(super) const REVIEW_DAYS: &str = "review_days";
/// The prefix of a promotion text's field (`promo-contributor`).
pub(super) const PROMO: &str = "promo-";
/// Who may accept a change to the rules.
pub(super) const CHANGE_RUNG: &str = "change_rung";
/// The approvals a change to the rules needs.
pub(super) const CHANGE_APPROVALS: &str = "change_approvals";
/// The approvals a change to who may change the rules needs.
pub(super) const CHANGE_WHO_APPROVALS: &str = "change_who_approvals";

/// The prefix of the fields of one protection's row, with the row's number after it (`row-2-title`).
pub(super) const ROW: &str = "row-";
/// What the row covers.
pub(super) const ROW_KIND: &str = "kind";
/// The value of [`ROW_KIND`] for one page.
pub(super) const ROW_KIND_PAGE: &str = "page";
/// The value of [`ROW_KIND`] for the pages under a title.
pub(super) const ROW_KIND_NAMESPACE: &str = "namespace";
/// The row's title.
pub(super) const ROW_TITLE: &str = "title";
/// What the row asks.
pub(super) const ROW_ASKS: &str = "asks";
/// The value of a row's [`ROW_ASKS`] that asks for a review; any other is a role's word.
pub(super) const ASKS_REVIEW: &str = "review";
/// The row's reason.
pub(super) const ROW_REASON: &str = "reason";

/// The prefix of the fields of one row of the interwiki map, with the row's number after it (`map-2-prefix`).
pub(super) const MAP_ROW: &str = "map-";
/// The row's prefix.
pub(super) const MAP_PREFIX: &str = "prefix";
/// The row's address template.
pub(super) const MAP_TEMPLATE: &str = "template";

/// The name of one of a protection's row's fields.
pub(super) fn row_field(n: usize, field: &str) -> String {
    format!("{ROW}{n}-{field}")
}

/// The name of one of an interwiki map row's fields.
pub(super) fn map_field(n: usize, field: &str) -> String {
    format!("{MAP_ROW}{n}-{field}")
}

/// The id of the control a refusal of the rules is shown beside.
pub(super) fn control_id(name: &str) -> String {
    format!("c-{name}")
}
