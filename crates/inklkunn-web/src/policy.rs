//! The rules of a wiki as people read and change them (RFC 006 D-4, D-11, D-12; handoff G-T7; threat model C-3d and INV-14):
//! the **policy page** that says them in sentences, the **policy form** that proposes a change to them, and the data the
//! read API gives of them.
//!
//! **Never raw YAML, and never the word *official*.** A rule is said as a label and a value, in the catalog's sentences
//! ([`rules_phrases`]); the form shows each key as such a sentence and a control, and writes the file only by the core's own
//! drafts ([`inklkunn_core::governance::draft`]), read by the one validator acceptance uses. The proposal page says a change
//! to the rules as *what it said, what it says* ([`rules_change`]).
//!
//! **Authorized as a wiki's pages are** (`read`) and existence-neutral: a wiki that is absent and a wiki that may not be read
//! are one page. The page, the form and the API read the rules through the core's reader (never a file): the rules in force
//! ([`ReadService::rules_of`](inklkunn_core::ReadService::rules_of)), the platform limits, the protections in force
//! ([`Authorizer::protections_in_force`](inklkunn_core::Authorizer::protections_in_force)) and the changes of the rules
//! ([`ReadService::rule_history`](inklkunn_core::ReadService::rule_history)). This crate decides no rule and writes no file:
//! the form makes a proposal, which the proposal service judges and the acceptance validates again.
//!
//! The pieces: `rules_data` (what the page and the API read, once), `rules_phrases` (the sentences), `rules_change` (a change
//! as before and after sentences), `rules_page` (the page), `rules_form` (the form), `rules_paths` and `rules_fields`.

pub(crate) mod rules_change;
pub(crate) mod rules_data;
pub(crate) mod rules_fields;
pub(crate) mod rules_form;
pub(crate) mod rules_page;
pub(crate) mod rules_paths;
pub(crate) mod rules_phrases;

/// The body bound of the form: a whole policy's controls (promotion texts, protections and their reasons).
pub(crate) const FORM_LIMIT: usize = crate::proposals::SMALL_FORM_LIMIT;

#[cfg(test)]
mod tests;
