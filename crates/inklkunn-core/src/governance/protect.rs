//! Standing protections: the policy's `protect` list (RFC 006 D-2, D-7).
//!
//! ```yaml
//! protect:
//!   - namespace: Policies          # a whole namespace: every page below it
//!     require: review              # every proposal there needs a review before it is accepted
//!     reason: "Rules of the community are read before they change."
//!   - path: Home                   # or one page
//!     require:
//!       rung: maintainer           # acceptance there is restricted to this rung and above
//!     reason: "The front page is edited by those who answer for it."
//! ```
//!
//! - **Exactly one of `path` and `namespace`**, written as a page **title** (`Help/Start`), the way people name a
//!   page and the way the title index does, never as a repository path. A title is written once, in one form: NFC,
//!   no slash at the start or the end, no empty or padded component, no `.` or `..` component. `Help/` and ` Help`
//!   are refused, not read as `Help`, so that a name means one thing.
//! - **`require` is a word or a mapping, and the two cannot be mistaken for each other.** The one word is `review`.
//!   The mapping has one key, `rung`, and a rung of the ladder (`reviewer` and `maintainer` are rungs, and `review`
//!   is not): `require: reviewer` is refused instead of being read as either. What `review` and `rung` mean for an
//!   acceptance is G-T3's; this module only reads them.
//! - **`reason` is required**: a protection with no reason cannot be shown to the people it stops.
//!
//! **The matching rule (whole path segments).** A [`Target::Path`] covers the one page with exactly that title. A
//! [`Target::Namespace`] covers the pages **below** it: those whose title starts with the namespace followed by `/`.
//! So `Help` covers `Help/x` and `Help/x/y`, never `Helpful` or `Hel/p`, and `Hel` covers neither `Help/x` nor
//! `Help`. **A namespace does not cover the page of the same name**: `Help` the page is a page (protect it with
//! `path: Help`), and this is the rule the read API's namespace filter and a watched namespace already have (a title
//! is in a namespace when the namespace is a proper prefix of it, whole segments). Matching is case-sensitive: `help`
//! and `Help` are two titles.

use inklkunn_markup::{path_to_title, title_to_path};
use inklkunn_store::Role;
use unicode_normalization::UnicodeNormalization;

use super::MAX_REASON_CHARS;
use super::MAX_TARGET_CHARS;
use super::chars::is_forbidden;

/// What a protection covers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// One page, by its title.
    Path(String),
    /// Every page below a namespace, by its title.
    Namespace(String),
}

/// What a protection requires of a proposal there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Require {
    /// A review before the proposal is accepted (`require: review`), whatever `acceptance.approvals` says.
    Review,
    /// Acceptance restricted to this rung and above (`require: {rung: <rung>}`). The stricter of this and
    /// `rungs.accept` decides.
    Rung(Role),
}

/// One standing protection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Protection {
    pub(super) target: Target,
    pub(super) require: Require,
    pub(super) reason: String,
}

impl Protection {
    /// What it covers.
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// What it requires.
    pub fn require(&self) -> Require {
        self.require
    }

    /// Why: shown on the page, in the proposal form and on every refusal it causes.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Does it cover the page with this `title`?
    pub fn covers_title(&self, title: &str) -> bool {
        self.target.covers_title(title)
    }

    /// Does it cover the page at this repository path (`pages/Help/x.md`)?
    pub fn covers_path(&self, path: &str) -> bool {
        self.target.covers_path(path)
    }
}

/// Is `title` written in the one form a protection's title takes: NFC, no leading or trailing `/`, every component
/// non-empty, unpadded and neither `.` nor `..`, at most [`MAX_TARGET_CHARS`] characters, and a title that has a
/// page path?
pub fn is_canonical_title(title: &str) -> bool {
    if title.is_empty() || title.chars().count() > MAX_TARGET_CHARS {
        return false;
    }
    if title.nfc().collect::<String>() != title {
        return false;
    }
    if title
        .split('/')
        .any(|component| component.is_empty() || component.trim() != component)
    {
        return false;
    }
    title_to_path(title).is_ok()
}

/// Does `namespace` cover the page `title`: is the namespace a **proper prefix of whole segments** of the title?
///
/// Both are titles in the canonical form ([`is_canonical_title`]); a namespace written any other way (`Help/`) covers
/// nothing, and no protection holds one. `Help` covers `Help/x` and `Help/x/y`; it does not cover `Help` itself, nor
/// `Helpful/x`.
pub fn namespace_covers(namespace: &str, title: &str) -> bool {
    !namespace.is_empty()
        && !namespace.ends_with('/')
        && title
            .strip_prefix(namespace)
            .and_then(|rest| rest.strip_prefix('/'))
            .is_some_and(|below| !below.is_empty() && !below.starts_with('/'))
}

impl Target {
    /// A page, if `title` is a canonical title ([`is_canonical_title`]).
    pub fn path(title: &str) -> Option<Target> {
        is_canonical_title(title).then(|| Target::Path(title.to_owned()))
    }

    /// A namespace, if `title` is a canonical title ([`is_canonical_title`]).
    pub fn namespace(title: &str) -> Option<Target> {
        is_canonical_title(title).then(|| Target::Namespace(title.to_owned()))
    }

    /// The title it names.
    pub fn title(&self) -> &str {
        match self {
            Target::Path(t) | Target::Namespace(t) => t,
        }
    }

    /// Does it cover the page with this `title`?
    pub fn covers_title(&self, title: &str) -> bool {
        match self {
            Target::Path(t) => t == title,
            Target::Namespace(ns) => namespace_covers(ns, title),
        }
    }

    /// Does it cover the page at this repository path (`pages/Help/x.md`)? A path that is no page's covers nothing.
    pub fn covers_path(&self, path: &str) -> bool {
        path_to_title(path).is_some_and(|title| self.covers_title(&title))
    }
}

/// Is `reason` a reason a protection may be shown with: not empty once trimmed, at most [`MAX_REASON_CHARS`]
/// characters, one line, and no control, bidirectional or line-separator character? The policy's `reason` is read by the
/// same rule; a temporary protection's is checked by this function, so that one text means one thing in both.
pub fn is_valid_reason(reason: &str) -> bool {
    !reason.trim().is_empty()
        && reason.chars().count() <= MAX_REASON_CHARS
        && !reason.chars().any(is_forbidden)
}
