//! The names of the fields of the people forms and of the page's query: code a form and a handler agree on, no sentence a
//! reader sees.

/// The person the act is about: an account id (from a link on the people page) or a sign-in name.
pub(super) const WHO: &str = "who";
/// The role to give: a role's word, or [`NO_ROLE`].
pub(super) const ROLE: &str = "role";
/// How many days a restriction lasts.
pub(super) const DAYS: &str = "days";
/// Why: one line.
pub(super) const REASON: &str = "reason";
/// The person a link on the people page is about (an account id), for a form to open on.
pub(super) const ACCOUNT: &str = "account";
/// The people page's note of what was just done.
pub(super) const DONE: &str = "done";

/// The value of the role field that takes the person's role away.
pub(super) const NO_ROLE: &str = "none";

/// What `done` says was done, for each act: the words the people page's query carries (a fixed set).
pub(super) const DONE_GRANTED: &str = "granted";
/// A role was taken away.
pub(super) const DONE_REVOKED: &str = "revoked";
/// A person was restricted.
pub(super) const DONE_RESTRICTED: &str = "restricted";
/// A restriction was renewed.
pub(super) const DONE_RENEWED: &str = "renewed";
/// A restriction was lifted.
pub(super) const DONE_LIFTED: &str = "lifted";
