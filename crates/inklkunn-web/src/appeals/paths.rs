//! The addresses of the appeals pages, under the reserved prefix `-/` so that no page title can shadow them, and the names of the
//! fields a form and a handler agree on. Code, not words.

use inklkunn_core::appeal::ContestRef;
use inklkunn_store::AppealedKind;

/// `/<slug>/-/appeals`: the appeals page of the people who decide them.
pub(crate) fn page(slug: &str) -> String {
    format!("/{slug}/-/appeals")
}

/// `/<slug>/-/appeals?done=<what>`: the same, with a note of what was just done (one of a fixed set of words).
pub(crate) fn page_done(slug: &str, what: &str) -> String {
    format!("/{slug}/-/appeals?{}={what}", fields::DONE)
}

/// `/<slug>/-/appeals/mine`: a person's own appeals and what they can contest.
pub(crate) fn mine(slug: &str) -> String {
    format!("/{slug}/-/appeals/mine")
}

/// `/<slug>/-/appeals/mine?done=filed`: the same, after an appeal was filed.
pub(crate) fn mine_done(slug: &str) -> String {
    format!(
        "/{slug}/-/appeals/mine?{}={}",
        fields::DONE,
        fields::DONE_FILED
    )
}

/// `/<slug>/-/appeals/new?kind=<kind>&id=<id>`: **the form of *contest this*** for one action.
pub(crate) fn contest(slug: &str, what: &ContestRef) -> String {
    format!(
        "/{slug}/-/appeals/new?{}={}&{}={}",
        fields::KIND,
        what.kind.as_str(),
        fields::ID,
        what.id
    )
}

/// `/<slug>/-/appeals/new`: where the form posts.
pub(crate) fn contest_action(slug: &str) -> String {
    format!("/{slug}/-/appeals/new")
}

/// `/<slug>/-/appeals/decide`: where a decision posts.
pub(crate) fn decide(slug: &str) -> String {
    format!("/{slug}/-/appeals/decide")
}

/// The action a pair of fields names, if they name one: a kind's word and an id of the characters a record's id has.
pub(crate) fn contest_of(kind: &str, id: &str) -> Option<ContestRef> {
    let kind = AppealedKind::parse(kind)?;
    let fits = (1..=36).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-');
    fits.then(|| ContestRef {
        kind,
        id: id.to_owned(),
    })
}

/// The names of the fields and of the query.
pub(crate) mod fields {
    /// The kind of action, in a form and in an address.
    pub(crate) const KIND: &str = "kind";
    /// The action's id.
    pub(crate) const ID: &str = "id";
    /// What the person says.
    pub(crate) const TEXT: &str = "text";
    /// The appeal a decision is about.
    pub(crate) const APPEAL: &str = "appeal";
    /// The decision.
    pub(crate) const DECISION: &str = "decision";
    /// The decision: the action stands.
    pub(crate) const UPHELD: &str = "upheld";
    /// The decision: the action is reversed.
    pub(crate) const OVERTURNED: &str = "overturned";
    /// Why.
    pub(crate) const REASON: &str = "reason";
    /// The note of what was just done, in the query.
    pub(crate) const DONE: &str = "done";
    /// An appeal was filed.
    pub(crate) const DONE_FILED: &str = "filed";
    /// A decision was recorded: the action stands.
    pub(crate) const DONE_UPHELD: &str = "upheld";
    /// A decision was recorded: the action was reversed.
    pub(crate) const DONE_OVERTURNED: &str = "overturned";
    /// A decision was recorded: the action was reversed, and a proposal is held for review again.
    pub(crate) const DONE_REOPENED: &str = "reopened";
}
