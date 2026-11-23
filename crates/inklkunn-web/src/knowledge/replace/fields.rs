//! The names the form that replaces a file the wiki cannot read and its handler agree on. Code, not words; scoped in
//! `tests/literals.allow`.

/// The file's place in the repository (the query parameter and the hidden field).
pub(super) const FILE: &str = "file";
/// The view the file is in, when it is not the default view.
pub(super) const VIEW: &str = "view";
/// The state and the block the form was opened on.
pub(super) const BASE_STATE: &str = "base_state";
pub(super) const BASE_BLOCK: &str = "base_block";
/// What to do with the file, and the two answers.
pub(super) const CHOICE: &str = "choice";
pub(super) const CHOICE_WRITE: &str = "write";
pub(super) const CHOICE_RETIRE: &str = "retire";
/// The text of the file written again, the reason for a retirement and the proposal's summary.
pub(super) const TEXT: &str = "text";
pub(super) const REASON: &str = "reason";
pub(super) const SUMMARY: &str = "summary";
