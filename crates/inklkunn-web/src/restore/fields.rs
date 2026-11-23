//! The names a restore form and its handler agree on: the field that says which way back the form was shown, the view's name,
//! and the two ways. Code, not words; scoped in `tests/literals.allow`.

/// The field that says which way back the form was shown (`rollback` or `last-text`).
pub(super) const MODE: &str = "mode";
/// The field and the query key that name the view.
pub(super) const VIEW: &str = "view";
/// The way back that is a rollback of the change that removed the page.
pub(super) const MODE_ROLLBACK: &str = "rollback";
/// The way back that writes the page again with its last text.
pub(super) const MODE_LAST_TEXT: &str = "last-text";
