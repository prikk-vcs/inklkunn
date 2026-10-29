//! Problem details (RFC 9457) with a registry of stable codes (external design API-04).
//!
//! The **code** is the identity of a problem: its `type` is `urn:inklkunn:problem:<code>` and does not change
//! with the language, the host the server is reached by, or the release (a code is never reused for another
//! meaning; a retired one is kept in the registry as retired). The **words** are the catalog's: `title`,
//! `detail` and `hint` are three messages per code (`problem.<code>.title`, `.detail`, `.hint`), in the
//! caller's language. A value the sentence names (`{parameter}`, `{view}`) is also an extension member of the
//! same name, so a program reads the value and a person reads the sentence.

use axum::http::StatusCode;
use serde_json::{Map, Value, json};

use crate::catalog::{Key, T};

/// A problem the API can answer. The registry: [`Code::ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    /// No such thing at this address, for a caller who may know nothing of it: a wiki that does not exist, a
    /// wiki the caller may not read, an address the API does not have.
    NotFound,
    /// The wiki has no page with this title.
    PageNotFound,
    /// Several pages match the title and none is nearer.
    AmbiguousTitle,
    /// The title is a redirect whose chain cannot be followed to a page.
    RedirectBroken,
    /// The page exists and its stored text cannot be read as a page.
    PageUnreadable,
    /// The title cannot name a page.
    InvalidTitle,
    /// The wiki has no open view of that name.
    NoSuchView,
    /// A parameter is unknown, repeated, missing, or has a value outside what it may be.
    InvalidParameter,
    /// A diff was asked for without the page it is for.
    DiffPathRequired,
    /// The cursor is not one this server gave out.
    InvalidCursor,
    /// This build does not offer the read yet.
    NotAvailable,
    /// The server could not read the wiki just now; asking again may work.
    Unavailable,
    /// The server failed and the request was not served.
    Failed,
    /// The address does not take that method.
    MethodNotAllowed,
}

impl Code {
    /// Every code, in documentation order. A new code is added here, in `catalog.rs`, in both language
    /// tables and in `docs/glossary.md`; the tests fail for each one that is missing.
    pub(crate) const ALL: &'static [Code] = &[
        Code::NotFound,
        Code::PageNotFound,
        Code::AmbiguousTitle,
        Code::RedirectBroken,
        Code::PageUnreadable,
        Code::InvalidTitle,
        Code::NoSuchView,
        Code::InvalidParameter,
        Code::DiffPathRequired,
        Code::InvalidCursor,
        Code::NotAvailable,
        Code::Unavailable,
        Code::Failed,
        Code::MethodNotAllowed,
    ];

    /// The stable name: lower case, hyphenated.
    pub(crate) const fn slug(self) -> &'static str {
        match self {
            Code::NotFound => "not-found",
            Code::PageNotFound => "page-not-found",
            Code::AmbiguousTitle => "ambiguous-title",
            Code::RedirectBroken => "redirect-broken",
            Code::PageUnreadable => "page-unreadable",
            Code::InvalidTitle => "invalid-title",
            Code::NoSuchView => "no-such-view",
            Code::InvalidParameter => "invalid-parameter",
            Code::DiffPathRequired => "diff-path-required",
            Code::InvalidCursor => "invalid-cursor",
            Code::NotAvailable => "not-available",
            Code::Unavailable => "unavailable",
            Code::Failed => "failed",
            Code::MethodNotAllowed => "method-not-allowed",
        }
    }

    /// The `type` member: a URN, so it names the problem without promising a page at that name.
    pub(crate) fn type_uri(self) -> String {
        format!("urn:inklkunn:problem:{}", self.slug())
    }

    /// The HTTP status.
    pub(crate) const fn status(self) -> StatusCode {
        match self {
            Code::NotFound | Code::PageNotFound | Code::RedirectBroken | Code::NoSuchView => {
                StatusCode::NOT_FOUND
            }
            Code::AmbiguousTitle => StatusCode::MULTIPLE_CHOICES,
            Code::PageUnreadable => StatusCode::UNPROCESSABLE_ENTITY,
            Code::InvalidTitle
            | Code::InvalidParameter
            | Code::DiffPathRequired
            | Code::InvalidCursor => StatusCode::BAD_REQUEST,
            Code::NotAvailable => StatusCode::NOT_IMPLEMENTED,
            Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Code::Failed => StatusCode::INTERNAL_SERVER_ERROR,
            Code::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
        }
    }

    /// The catalog messages: title, detail, hint.
    pub(crate) const fn keys(self) -> (Key, Key, Key) {
        match self {
            Code::NotFound => (
                Key::ProblemNotFoundTitle,
                Key::ProblemNotFoundDetail,
                Key::ProblemNotFoundHint,
            ),
            Code::PageNotFound => (
                Key::ProblemPageNotFoundTitle,
                Key::ProblemPageNotFoundDetail,
                Key::ProblemPageNotFoundHint,
            ),
            Code::AmbiguousTitle => (
                Key::ProblemAmbiguousTitleTitle,
                Key::ProblemAmbiguousTitleDetail,
                Key::ProblemAmbiguousTitleHint,
            ),
            Code::RedirectBroken => (
                Key::ProblemRedirectBrokenTitle,
                Key::ProblemRedirectBrokenDetail,
                Key::ProblemRedirectBrokenHint,
            ),
            Code::PageUnreadable => (
                Key::ProblemPageUnreadableTitle,
                Key::ProblemPageUnreadableDetail,
                Key::ProblemPageUnreadableHint,
            ),
            Code::InvalidTitle => (
                Key::ProblemInvalidTitleTitle,
                Key::ProblemInvalidTitleDetail,
                Key::ProblemInvalidTitleHint,
            ),
            Code::NoSuchView => (
                Key::ProblemNoSuchViewTitle,
                Key::ProblemNoSuchViewDetail,
                Key::ProblemNoSuchViewHint,
            ),
            Code::InvalidParameter => (
                Key::ProblemInvalidParameterTitle,
                Key::ProblemInvalidParameterDetail,
                Key::ProblemInvalidParameterHint,
            ),
            Code::DiffPathRequired => (
                Key::ProblemDiffPathRequiredTitle,
                Key::ProblemDiffPathRequiredDetail,
                Key::ProblemDiffPathRequiredHint,
            ),
            Code::InvalidCursor => (
                Key::ProblemInvalidCursorTitle,
                Key::ProblemInvalidCursorDetail,
                Key::ProblemInvalidCursorHint,
            ),
            Code::NotAvailable => (
                Key::ProblemNotAvailableTitle,
                Key::ProblemNotAvailableDetail,
                Key::ProblemNotAvailableHint,
            ),
            Code::Unavailable => (
                Key::ProblemUnavailableTitle,
                Key::ProblemUnavailableDetail,
                Key::ProblemUnavailableHint,
            ),
            Code::Failed => (
                Key::ProblemFailedTitle,
                Key::ProblemFailedDetail,
                Key::ProblemFailedHint,
            ),
            Code::MethodNotAllowed => (
                Key::ProblemMethodNotAllowedTitle,
                Key::ProblemMethodNotAllowedDetail,
                Key::ProblemMethodNotAllowedHint,
            ),
        }
    }
}

/// One problem to answer: a code, and the values its sentences name or its members carry.
#[derive(Debug, Clone)]
pub(crate) struct Problem {
    pub(crate) code: Code,
    /// A value a sentence names; also a member of the same name.
    named: Vec<(&'static str, String)>,
    /// A member that no sentence names.
    members: Vec<(&'static str, Value)>,
}

impl Problem {
    pub(crate) fn new(code: Code) -> Problem {
        Problem {
            code,
            named: Vec::new(),
            members: Vec::new(),
        }
    }

    /// A value the sentences name (`{name}`), also given as the member `name`.
    #[must_use]
    pub(crate) fn named(mut self, name: &'static str, value: impl Into<String>) -> Problem {
        self.named.push((name, value.into()));
        self
    }

    /// A member no sentence names.
    #[must_use]
    pub(crate) fn member(mut self, name: &'static str, value: Value) -> Problem {
        self.members.push((name, value));
        self
    }

    /// The body, in the language of `t`: `type`, `title`, `status`, `detail`, `hint`, then the members.
    pub(crate) fn body(&self, t: T) -> Value {
        let (title, detail, hint) = self.code.keys();
        let say = |key: Key| {
            let mut msg = t.msg(key);
            for (name, value) in &self.named {
                msg = msg.with(name, value);
            }
            msg.text()
        };
        let mut body = Map::new();
        // Members first, so that none of them can replace what the standard names.
        for (name, value) in &self.members {
            body.insert((*name).to_owned(), value.clone());
        }
        for (name, value) in &self.named {
            body.insert((*name).to_owned(), json!(value));
        }
        body.insert("type".to_owned(), json!(self.code.type_uri()));
        body.insert("title".to_owned(), json!(say(title)));
        body.insert("status".to_owned(), json!(self.code.status().as_u16()));
        body.insert("detail".to_owned(), json!(say(detail)));
        body.insert("hint".to_owned(), json!(say(hint)));
        Value::Object(body)
    }
}

#[cfg(test)]
mod tests;
