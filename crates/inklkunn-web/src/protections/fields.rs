//! The names of the protection form's fields and of its query: code a form and a handler agree on, no sentence a reader sees.

/// The query parameter that names one page to protect.
pub(super) const Q_PAGE: &str = "page";
/// The query parameter that names a title whose pages below it are to be protected.
pub(super) const Q_NAMESPACE: &str = "namespace";

/// What is protected: `page` or `namespace`.
pub(super) const TARGET: &str = "target";
/// The value of [`TARGET`] for one page.
pub(super) const TARGET_PAGE: &str = "page";
/// The value of [`TARGET`] for the pages below a title.
pub(super) const TARGET_NAMESPACE: &str = "namespace";
/// The page's or namespace's title.
pub(super) const TITLE: &str = "title";
/// What the protection asks: `review` or `rung`.
pub(super) const REQUIRE: &str = "require";
/// The value of [`REQUIRE`] for an approval first.
pub(super) const REQUIRE_REVIEW: &str = "review";
/// The value of [`REQUIRE`] for a role.
pub(super) const REQUIRE_RUNG: &str = "rung";
/// The role, when the protection asks one.
pub(super) const ROLE: &str = "role";
/// The reason.
pub(super) const REASON: &str = "reason";
/// For how many days.
pub(super) const DAYS: &str = "days";
/// What the button does: `set`, `renew` or `lift`.
pub(super) const ACT: &str = "act";
/// The value of [`ACT`] that puts a protection in place.
pub(super) const ACT_SET: &str = "set";
/// The value of [`ACT`] that renews it.
pub(super) const ACT_RENEW: &str = "renew";
/// The value of [`ACT`] that lifts it.
pub(super) const ACT_LIFT: &str = "lift";
