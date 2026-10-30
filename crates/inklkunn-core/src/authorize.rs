//! `authorize(principal, action, resource) -> Decision` (RFC 002 D-2; internal design AZ-1 … AZ-3; threat
//! model T-3 and INV-4): pure, default-deny, existence-neutral, consulted on every path.
//!
//! **Two layers, so that the rule is pure and the lookup cannot be skipped.**
//!
//! - [`decide`] is the rule: a function of a [`Snapshot`] (the wiki's visibility, and the role the account
//!   holds in it), the principal and the action. No I/O, no clock, no hidden state; the exhaustive tests run
//!   against it.
//! - [`Authorizer`] is what a surface calls. It **loads the snapshot itself**, from a source it was built
//!   with, so a handler cannot assert that a wiki is public: a [`Resource`] names a wiki and nothing about
//!   it. It fails closed (a source that cannot answer denies), records consequential decisions in the audit
//!   trail (RFC 002 D-9), and refuses an *allowed* consequential action whose record it could not write.
//!
//! **What each principal may do** (RFC 002 D-1, D-2; a principal with no grant is not a principal that may
//! do nothing: it may read what the wiki's visibility lets it read):
//!
//! | Principal | Reads | Everything else |
//! |---|---|---|
//! | anonymous | a `public` wiki | denied |
//! | account, no role in the wiki | a `public` or `internal` wiki | denied |
//! | account with a role | whatever its role allows ([`Action::minimum_role`]), limited by its credential's [`Scope`] | denied |
//! | the system (import, recovery) | any existing wiki; accepts and imports | denied |
//!
//! A wiki that is not there, and a `restricted` wiki the caller holds no role in, produce **the same
//! answer** (and, for a consequential action, the same audit event), so a decision cannot be used to find
//! out which wikis exist (RFC 002 D-2; planeter RFC 002).
//!
//! An account principal is built by `inklkunn-auth` from a verified credential and by nothing else, and the
//! types say so (corrections handoff C7): the only constructors, [`Principal::session`] and
//! [`Principal::token`], need an [`IdentityAuthority`], which exists **once per process**, is claimed by the
//! binary when it builds the authenticator, and is held by the authenticator alone; [`AccountPrincipal`] has
//! private fields, so nothing builds one by hand. A [`SystemPrincipal`] cannot be built outside this crate at
//! all. The boundary gate still fails if a web source names any of them (`tools/boundary-check`, rule
//! "privileged principals"): the gate catches the shortcut in review, the types refuse it in the compiler.

use std::sync::Arc;

use inklkunn_signer::message::SystemLabel;
use inklkunn_store::{
    AccountId, AuditEvent, AuditOutcome, AuditSink, Role, RoleStore, Scope, StoreError, Visibility,
    WikiId, WikiStore,
};

/// The authority to say who a request is: to turn a verified credential into a [`Principal`]. **There is one
/// per process.** The binary claims it once, when it builds the authenticator, and gives it to the
/// authenticator, which keeps it; a handler, a route or any other crate never holds it, so none can build an
/// account principal and act as someone. (`claim` answers `None` the second time, whoever asks.)
///
/// The compiler refuses the shortcuts (these are compile-fail tests of the rule, run with the documentation):
///
/// ```compile_fail
/// use inklkunn_core::Principal;
/// use inklkunn_store::AccountId;
/// // A session principal needs the authority.
/// let _ = Principal::session(AccountId::new());
/// ```
///
/// ```compile_fail
/// use inklkunn_core::{AccountPrincipal, Principal};
/// use inklkunn_store::{AccountId, Scope};
/// // The account principal has private fields: it cannot be written out by hand.
/// let _ = Principal::Account(AccountPrincipal { id: AccountId::new(), scope: Scope::Admin });
/// ```
///
/// ```compile_fail
/// use inklkunn_core::IdentityAuthority;
/// // There is no public constructor but `claim`, and the test constructor is not in a product build.
/// let _ = IdentityAuthority(());
/// ```
#[derive(Debug)]
pub struct IdentityAuthority(());

impl IdentityAuthority {
    /// Claim the process's one identity authority: `Some` the first time, `None` ever after.
    pub fn claim() -> Option<IdentityAuthority> {
        static CLAIMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        (!CLAIMED.swap(true, std::sync::atomic::Ordering::SeqCst)).then_some(IdentityAuthority(()))
    }

    /// An authority for a test, which builds principals freely (a process may have any number of these; the
    /// product build has the feature off, and a gate fails if a product dependency turns it on).
    #[cfg(any(test, feature = "test-authority"))]
    pub fn for_tests() -> IdentityAuthority {
        IdentityAuthority(())
    }
}

/// An account reached through a credential: the account and how much of its authority the credential carries.
/// Built only by [`Principal::session`] and [`Principal::token`], which need an [`IdentityAuthority`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountPrincipal {
    id: AccountId,
    scope: Scope,
}

impl AccountPrincipal {
    /// The account.
    pub fn id(&self) -> &AccountId {
        &self.id
    }

    /// How much of the account's authority the credential carries. A session carries all of it.
    pub fn scope(&self) -> Scope {
        self.scope
    }
}

/// Who is asking (RFC 002 D-1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Principal {
    /// No session, no token.
    Anonymous,
    /// An account, reached through a session or a token, with the scope that credential carries.
    Account(AccountPrincipal),
    /// The service itself (import, recovery). Never reached from the network, and not constructible
    /// outside this crate.
    System(SystemPrincipal),
}

impl Principal {
    /// An account signed in with a session: the credential carries the account's whole authority. Needs the
    /// process's [`IdentityAuthority`].
    pub fn session(_authority: &IdentityAuthority, id: AccountId) -> Self {
        Principal::Account(AccountPrincipal {
            id,
            scope: Scope::Admin,
        })
    }

    /// An account reached with an API token of `scope`. Needs the process's [`IdentityAuthority`].
    pub fn token(_authority: &IdentityAuthority, id: AccountId, scope: Scope) -> Self {
        Principal::Account(AccountPrincipal { id, scope })
    }

    /// The system principal for a service-made change, named by `label`. **Crate-private on purpose**: only
    /// core's own import and recovery entry points mint one (handoff 002 T2; RFC 002 D-1), so no handler,
    /// no surface and no other crate can act as the service.
    // Nothing in this crate calls it yet: the import entry point arrives with handoff 003 T7 and recovery
    // with RFC 004. The tests mint one; the product build says nothing about it until then.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn system(label: SystemLabel) -> Self {
        Principal::System(SystemPrincipal { label })
    }

    /// The account behind the principal, if it is one: what a surface needs to say whose a draft is, without
    /// being able to name (and so to build) an account principal.
    pub fn account_id(&self) -> Option<&AccountId> {
        match self {
            Principal::Account(a) => Some(&a.id),
            Principal::Anonymous | Principal::System(_) => None,
        }
    }

    /// The audit name: `anonymous`, `account:<id>` or `system:<label>`.
    pub fn audit_name(&self) -> String {
        match self {
            Principal::Anonymous => "anonymous".to_owned(),
            Principal::Account(a) => format!("account:{}", a.id),
            Principal::System(s) => format!("system:{}", s.label.as_str()),
        }
    }
}

/// The service acting for itself. The field is private and so is the constructor: only this crate's own
/// import and recovery entry points can make one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SystemPrincipal {
    label: SystemLabel,
}

impl SystemPrincipal {
    /// Which part of the service acts (`import`, `recovery`).
    pub fn label(&self) -> &SystemLabel {
        &self.label
    }
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

    /// Consequential actions are audited (RFC 002 D-9), allowed or not.
    pub const fn is_consequential(self) -> bool {
        matches!(
            self,
            Action::Accept | Action::Moderate | Action::Admin | Action::Export | Action::Import
        )
    }

    /// The lowest role that may do this (the fixed M2 ladder; RFC 006 replaces it with policy as data
    /// without changing this contract). Roles are a ladder, so a higher role may do everything a lower one
    /// may.
    pub const fn minimum_role(self) -> Role {
        match self {
            Action::Read => Role::Observer,
            Action::Comment => Role::Questioner,
            Action::Propose => Role::Contributor,
            Action::Review => Role::Reviewer,
            Action::Accept | Action::CreateView | Action::Moderate => Role::Maintainer,
            Action::Admin | Action::Export | Action::Import => Role::Admin,
        }
    }

    /// The narrowest token scope that covers this (RFC 002 D-1: `read ⊂ propose ⊂ review ⊂ accept ⊂
    /// admin`). Commenting is covered by `propose`; creating views and moderating by `accept`.
    pub const fn required_scope(self) -> Scope {
        match self {
            Action::Read => Scope::Read,
            Action::Comment | Action::Propose => Scope::Propose,
            Action::Review => Scope::Review,
            Action::Accept | Action::CreateView | Action::Moderate => Scope::Accept,
            Action::Admin | Action::Export | Action::Import => Scope::Admin,
        }
    }
}

/// What is asked about. Every resource names its wiki and **nothing about the wiki's visibility**: that is
/// looked up by the [`Authorizer`], never stated by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resource {
    /// The wiki as a whole.
    Wiki {
        /// The wiki.
        wiki: WikiId,
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
            Resource::Wiki { wiki }
            | Resource::Namespace { wiki, .. }
            | Resource::Page { wiki, .. }
            | Resource::View { wiki, .. }
            | Resource::Proposal { wiki, .. }
            | Resource::Discussion { wiki, .. } => wiki,
        }
    }

    /// The audit name: `<kind>:<wiki id>` and, for what has an id of its own, `:<id>`. **Never a page path
    /// or a namespace**: those are titles, and the trail holds events, not content (RFC 002 D-9).
    pub fn audit_name(&self) -> String {
        let wiki = self.wiki();
        match self {
            Resource::Wiki { .. } => format!("wiki:{wiki}"),
            Resource::Namespace { .. } => format!("namespace:{wiki}"),
            Resource::Page { .. } => format!("page:{wiki}"),
            Resource::View { name, .. } => format!("view:{wiki}:{name}"),
            Resource::Proposal { id, .. } => format!("proposal:{wiki}:{id}"),
            Resource::Discussion { id, .. } => format!("discussion:{wiki}:{id}"),
        }
    }
}

/// The answer. A denial carries no reason that would reveal whether the resource exists (RFC 002 D-2: a
/// denied private resource is indistinguishable from an absent one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "a decision that is not read is a check that was not made"]
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

/// What the rule needs to know about the wiki and the caller's place in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// The wiki's visibility, or `None` when there is no such wiki.
    pub visibility: Option<Visibility>,
    /// The role the account holds in the wiki (always `None` for a principal that is not an account).
    pub role: Option<Role>,
}

/// The rule (see the module docs for the table). Pure: the same inputs always give the same answer, and
/// every path that is not explicitly allowed is denied.
pub fn decide(snapshot: Snapshot, principal: &Principal, action: Action) -> Decision {
    let Some(visibility) = snapshot.visibility else {
        // No such wiki: denied for everyone, in the same words as a wiki one may not see.
        return Decision::Deny;
    };
    let allowed = match principal {
        Principal::System(_) => matches!(action, Action::Read | Action::Accept | Action::Import),
        Principal::Anonymous => action == Action::Read && visibility == Visibility::Public,
        Principal::Account(a) => {
            let scope = &a.scope;
            // The credential limits the account; the role limits the credential. Both must allow.
            *scope >= action.required_scope()
                && match snapshot.role {
                    Some(role) => role >= action.minimum_role(),
                    None => action == Action::Read && visibility != Visibility::Restricted,
                }
        }
    };
    if allowed {
        Decision::Allow
    } else {
        Decision::Deny
    }
}

/// A source that could not answer. The text is for operators (it never reaches a user); the decision
/// given is *deny*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError(pub String);

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the authorization source could not answer: {}", self.0)
    }
}

impl std::error::Error for SourceError {}

/// Where the [`Authorizer`] reads the facts a decision needs.
pub trait AuthzSource: Send + Sync {
    /// The wiki's visibility, or `None` when there is no such wiki.
    fn visibility(&self, wiki: &WikiId) -> Result<Option<Visibility>, SourceError>;
    /// The role the account holds in the wiki, if any.
    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>, SourceError>;
}

/// The facts from the service store: the wiki record and the role grants.
#[derive(Clone)]
pub struct StoreAuthz {
    wikis: Arc<dyn WikiStore>,
    roles: Arc<dyn RoleStore>,
}

impl std::fmt::Debug for StoreAuthz {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StoreAuthz")
    }
}

impl StoreAuthz {
    /// Over a wiki store and a role store (the same SQLite store serves both in service).
    pub fn new(wikis: Arc<dyn WikiStore>, roles: Arc<dyn RoleStore>) -> Self {
        StoreAuthz { wikis, roles }
    }
}

fn source_error(e: StoreError) -> SourceError {
    SourceError(e.to_string())
}

impl AuthzSource for StoreAuthz {
    fn visibility(&self, wiki: &WikiId) -> Result<Option<Visibility>, SourceError> {
        match self.wikis.get(wiki) {
            Ok(record) => Ok(Some(record.visibility)),
            Err(StoreError::NotFound) => Ok(None),
            Err(e) => Err(source_error(e)),
        }
    }

    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>, SourceError> {
        self.roles.role(account, wiki).map_err(source_error)
    }
}

/// What a surface calls: [`decide`] over a snapshot it loads itself, with the audit trail.
#[derive(Clone)]
pub struct Authorizer {
    source: Arc<dyn AuthzSource>,
    audit: Arc<dyn AuditSink>,
    clock: fn() -> i64,
}

impl std::fmt::Debug for Authorizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Authorizer")
    }
}

impl Authorizer {
    /// An authorizer reading from `source` and recording consequential decisions in `audit`. There is no
    /// authorizer without an audit sink: a service that cannot record its consequential decisions does not
    /// run (a test double is [`inklkunn_store::MemoryAudit`]).
    pub fn new(source: Arc<dyn AuthzSource>, audit: Arc<dyn AuditSink>) -> Self {
        Authorizer {
            source,
            audit,
            clock: inklkunn_store::record::now_secs,
        }
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    fn snapshot(&self, principal: &Principal, wiki: &WikiId) -> Result<Snapshot, SourceError> {
        let visibility = self.source.visibility(wiki)?;
        // The role is looked up for every account whether or not the wiki exists, so that an absent wiki and
        // a private one cost the same to ask about.
        let role = match principal {
            Principal::Account(a) => self.source.role(&a.id, wiki)?,
            Principal::Anonymous | Principal::System(_) => None,
        };
        Ok(Snapshot { visibility, role })
    }

    /// May this account, **carrying the whole authority of its role** as a browser session does, do the
    /// non-consequential `action` to `resource`? For the service's own bookkeeping about *other* people (whom
    /// to tell something, a watch to keep): it builds no principal that leaves this crate and records
    /// nothing. It is never the answer to a request: a request is authorized as the principal it came from,
    /// whose credential may carry less. Fails closed, and refuses a consequential action outright.
    pub(crate) fn account_may(
        &self,
        account: &AccountId,
        action: Action,
        resource: &Resource,
    ) -> bool {
        if action.is_consequential() {
            return false;
        }
        let principal = Principal::Account(AccountPrincipal {
            id: account.clone(),
            scope: Scope::Admin,
        });
        self.snapshot(&principal, resource.wiki())
            .is_ok_and(|snapshot| decide(snapshot, &principal, action).is_allowed())
    }

    /// May `principal` do `action` to `resource`? **Fails closed**: if the facts cannot be read the answer is
    /// deny. A consequential action is recorded whatever the answer; an *allowed* one whose record cannot be
    /// written is denied, because nothing consequential happens unrecorded.
    pub fn authorize(
        &self,
        principal: &Principal,
        action: Action,
        resource: &Resource,
    ) -> Decision {
        let (decision, outcome, detail) = match self.snapshot(principal, resource.wiki()) {
            Ok(snapshot) => {
                let decision = decide(snapshot, principal, action);
                let outcome = if decision.is_allowed() {
                    AuditOutcome::Allowed
                } else {
                    AuditOutcome::Denied
                };
                (decision, outcome, "")
            }
            Err(_) => (
                Decision::Deny,
                AuditOutcome::Failed,
                "the facts for the decision could not be read",
            ),
        };
        if action.is_consequential() {
            let event = AuditEvent::new(
                (self.clock)(),
                &principal.audit_name(),
                action.name(),
                &resource.audit_name(),
                outcome,
                detail,
            );
            if self.audit.record(&event).is_err() && decision.is_allowed() {
                return Decision::Deny;
            }
        }
        decision
    }
}

#[cfg(test)]
mod tests;
