//! The route table (RFC 002 D-2; internal design LAY-2, ENF-3; threat model INV-4): **every entry point a
//! surface serves is declared here, with the check that guards it**, and a surface builds its router from
//! this table and from nothing else.
//!
//! An entry is either guarded by an [`Action`] (its handler consults [`crate::Authorizer::authorize`] for that
//! action before it touches wiki data) or **public with a reason** in words (it serves no wiki data: a
//! stylesheet, the language switcher, a health check). There is no third kind and no empty reason. The table
//! itself is plain data and framework-neutral; the web crate builds its routers from it. Its tests hold every
//! entry to its shape (a guard or a real reason) and to the URL scheme (no fixed path can shadow a page
//! title). **What is not yet tested** is that a guarded entry's *handler* really calls
//! [`crate::Authorizer::authorize`] for the action it declares, and answers an anonymous caller asking for a
//! restricted wiki exactly as for one that does not exist: that needs a request-level harness, which arrives
//! with the first guarded route (handoff 003 T4). Until then the only entry is a public health check.
//!
//! The table is only complete if nothing else registers a route. That half is a workspace gate
//! (`tools/boundary-check`, rule "route registration"): axum's own registration calls may appear in the web
//! crate's `routes.rs` and nowhere else.

use crate::authorize::Action;

/// The shortest reason a public entry may give, in characters: "a stylesheet" is not a reason, "the page
/// stylesheet: static, the same for everyone, no wiki data" is.
pub const MIN_REASON_CHARS: usize = 16;

/// What guards an entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// The handler consults `authorize(_, action, _)` before it touches wiki data.
    Action(Action),
    /// No wiki data is served; the reason says why that is safe.
    Public {
        /// The reason, in words ([`MIN_REASON_CHARS`] or more).
        reason: &'static str,
    },
}

/// One entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteEntry {
    /// The path pattern as the router writes it (`/{slug}/-/history/{*title}`).
    pub path: &'static str,
    /// The HTTP methods it answers, upper case.
    pub methods: &'static [&'static str],
    /// What guards it.
    pub guard: Guard,
}

/// Why an entry was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    /// The path is empty, does not start with `/`, or carries a character a route pattern must not.
    BadPath,
    /// No method is named.
    NoMethods,
    /// A method is not an upper-case HTTP method token, or is named twice.
    BadMethod,
    /// A public entry gave no reason, or a reason too thin to be one.
    ThinReason,
    /// The path is already in the table.
    Duplicate,
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::BadPath => write!(
                f,
                "a route path starts with `/` and has no spaces or control characters"
            ),
            RouteError::NoMethods => write!(f, "a route answers at least one method"),
            RouteError::BadMethod => {
                write!(f, "a route's methods are distinct upper-case HTTP methods")
            }
            RouteError::ThinReason => write!(
                f,
                "a public route gives a reason of at least {MIN_REASON_CHARS} characters for serving no wiki data"
            ),
            RouteError::Duplicate => write!(f, "the path is already registered"),
        }
    }
}

impl std::error::Error for RouteError {}

/// The entry points of a surface, in the order they were declared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteTable {
    entries: Vec<RouteEntry>,
}

fn is_method(m: &str) -> bool {
    !m.is_empty() && m.bytes().all(|b| b.is_ascii_uppercase())
}

fn is_path(p: &str) -> bool {
    p.starts_with('/')
        && p.len() <= 256
        && !p.contains("//")
        && !p.chars().any(|c| c.is_control() || c.is_whitespace())
}

impl RouteTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare one entry point. Nothing is added unless the whole entry is valid.
    pub fn register(
        &mut self,
        path: &'static str,
        methods: &'static [&'static str],
        guard: Guard,
    ) -> Result<(), RouteError> {
        if !is_path(path) {
            return Err(RouteError::BadPath);
        }
        if methods.is_empty() {
            return Err(RouteError::NoMethods);
        }
        let mut seen: Vec<&str> = Vec::with_capacity(methods.len());
        for m in methods {
            if !is_method(m) || seen.contains(m) {
                return Err(RouteError::BadMethod);
            }
            seen.push(m);
        }
        if let Guard::Public { reason } = guard
            && reason.trim().chars().count() < MIN_REASON_CHARS
        {
            return Err(RouteError::ThinReason);
        }
        if self.entries.iter().any(|e| e.path == path) {
            return Err(RouteError::Duplicate);
        }
        self.entries.push(RouteEntry {
            path,
            methods,
            guard,
        });
        Ok(())
    }

    /// The entries, in declaration order.
    pub fn entries(&self) -> &[RouteEntry] {
        &self.entries
    }

    /// The entry for a path pattern.
    pub fn find(&self, path: &str) -> Option<&RouteEntry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// The entries guarded by an action (every one that can show or change wiki data).
    pub fn guarded(&self) -> impl Iterator<Item = (&RouteEntry, Action)> {
        self.entries.iter().filter_map(|e| match e.guard {
            Guard::Action(a) => Some((e, a)),
            Guard::Public { .. } => None,
        })
    }
}

#[cfg(test)]
mod tests;
