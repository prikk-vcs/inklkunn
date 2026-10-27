//! `authorize(principal, action, resource) -> Decision` (RFC 002 D-2; internal design AZ-*): pure,
//! default-deny, consulted on every path. **In M0 there are no roles and no grants, so every call is
//! denied**; the one exception a reader would expect, public read, arrives with RFC 002's handoff together
//! with the visibility rule and the route-enumeration test. The contract is fixed now so every surface is
//! written against it from its first route.
//!
//! Two things the signature leaves to RFC 002's handoff (T2), recorded here so no caller builds on the
//! gap: a [`Principal::System`] can be constructed by any code today, and must become mintable only by
//! core's own import and recovery entry points (the route-enumeration harness flags a handler that
//! constructs one); and the visibility in [`Resource::Wiki`] is the caller's statement, so the handoff
//! puts the wiki lookup inside the authorization layer rather than trusting a handler's value.

use inklkunn_store::{Visibility, WikiId};

/// Who is asking (RFC 002 D-1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Principal {
    /// No session, no token.
    Anonymous,
    /// An account, by id.
    Account(String),
    /// The service itself (import, recovery), with the acting operator or job named. Never reached from
    /// the network (RFC 002 D-1); see the module docs for the constraint RFC 002's handoff adds.
    System(String),
}

/// What is asked (RFC 002 D-2's fixed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Read pages, history, discussions.
    Read,
    /// Comment on a proposal or page.
    Comment,
    /// Open a proposal.
    Propose,
    /// Review a proposal.
    Review,
    /// Accept a proposal (the signer then seals).
    Accept,
    /// Create a view.
    CreateView,
    /// Moderate content and discussions.
    Moderate,
    /// Administer the wiki.
    Admin,
    /// Export a bundle.
    Export,
    /// Import a bundle.
    Import,
}

impl Action {
    /// Every action.
    pub const ALL: [Action; 10] = [
        Action::Read,
        Action::Comment,
        Action::Propose,
        Action::Review,
        Action::Accept,
        Action::CreateView,
        Action::Moderate,
        Action::Admin,
        Action::Export,
        Action::Import,
    ];

    /// The audit name.
    pub const fn name(self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Comment => "comment",
            Action::Propose => "propose",
            Action::Review => "review",
            Action::Accept => "accept",
            Action::CreateView => "create-view",
            Action::Moderate => "moderate",
            Action::Admin => "admin",
            Action::Export => "export",
            Action::Import => "import",
        }
    }

    /// Consequential actions are audited (RFC 002 D-9).
    pub const fn is_consequential(self) -> bool {
        matches!(
            self,
            Action::Accept | Action::Moderate | Action::Admin | Action::Export | Action::Import
        )
    }
}

/// What is asked about. Every resource names its wiki.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resource {
    /// The wiki as a whole.
    Wiki {
        /// The wiki.
        wiki: WikiId,
        /// Its visibility, **as the caller states it** (RFC 002's handoff moves the lookup inside).
        visibility: Visibility,
    },
    /// A namespace (hierarchy prefix) in a wiki.
    Namespace {
        /// The wiki.
        wiki: WikiId,
        /// The prefix.
        prefix: String,
    },
    /// A page.
    Page {
        /// The wiki.
        wiki: WikiId,
        /// The page path.
        path: String,
    },
    /// A view (branch).
    View {
        /// The wiki.
        wiki: WikiId,
        /// The view name.
        name: String,
    },
    /// A proposal.
    Proposal {
        /// The wiki.
        wiki: WikiId,
        /// The proposal id.
        id: String,
    },
    /// A discussion.
    Discussion {
        /// The wiki.
        wiki: WikiId,
        /// The discussion id.
        id: String,
    },
}

impl Resource {
    /// The wiki the resource belongs to.
    pub fn wiki(&self) -> &WikiId {
        match self {
            Resource::Wiki { wiki, .. }
            | Resource::Namespace { wiki, .. }
            | Resource::Page { wiki, .. }
            | Resource::View { wiki, .. }
            | Resource::Proposal { wiki, .. }
            | Resource::Discussion { wiki, .. } => wiki,
        }
    }
}

/// The answer. A denial carries no reason that would reveal whether the resource exists (RFC 002 D-2: a
/// denied private resource is indistinguishable from an absent one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Allowed.
    Allow,
    /// Denied.
    Deny,
}

impl Decision {
    /// `true` for [`Decision::Allow`].
    pub const fn is_allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// The one decision function. M0: default deny, no grants exist yet.
pub fn authorize(principal: &Principal, action: Action, resource: &Resource) -> Decision {
    let _ = (principal, action, resource);
    Decision::Deny
}

#[cfg(test)]
mod tests;
