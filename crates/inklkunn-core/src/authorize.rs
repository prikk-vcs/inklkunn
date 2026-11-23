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
//! | account, no role in the wiki | a `public` or `internal` wiki | denied, unless the wiki's `entry` rung (below) gives it one |
//! | account with a role | whatever its role allows ([`Action::minimum_role`], or the wiki's policy), limited by its credential's [`Scope`] | denied |
//! | the system (import, recovery) | any existing wiki; accepts and imports | denied |
//!
//! **The rules of a wiki are data** (RFC 006 D-5; handoff G-T1 part C): the [`Rules`] in force, read from the default
//! view's tip by the [`AuthzSource`] and never passed in by a handler. Without governance files they are the defaults,
//! which are this table (the test that holds it is acceptance criterion 1). With them:
//!
//! - **The rung an account holds** is the higher of its grant and the wiki's `entry`, **then capped by a restriction**
//!   ([`Snapshot::cap`]: the loader looks the account's restriction up, at the clock's time, for every write; **an admin is
//!   never capped**, so that the wiki's admins can always change its rules, INV-14). **`entry` does not apply to a
//!   `restricted` wiki**: its members are the accounts the operator granted a rung (visibility is the operator's, RFC 006
//!   D-3). The lowest rung of each action is the policy's ([`Policy::minimum_role`](crate::governance::Policy)).
//! - **A governance file that cannot be read** ([`Rules::fault`]) makes the wiki **refuse every write** (everything but
//!   `read`) to every account, with one exception, the repair path of INV-14: an *admin* of the wiki may *propose* a change to
//!   one of the two governance files or a removal of what is below one, and may *accept*; the acceptance judges what the change touches (nothing but
//!   `governance/`, [`ProposalAcceptor`](crate::ProposalAcceptor)). The system principal's acts (import, recovery) are not
//!   writes of a person and are decided as before. Whoever may read the wiki can be told why ([`Authorizer::write_refusal`]);
//!   everyone else gets the existence-neutral answer they always got.
//!
//! - **A protection of a page is an outcome of `authorize()`** (RFC 006 D-7; threat model C-3g), standing (the policy's `protect`
//!   list) or temporary (a record of the `moderate` rung, kept by [`Authorizer::with_protections`]): `authorize(_, accept, page)`
//!   denies an account whose rung is below the one a protection asks of whoever accepts a change to the page, and **the stricter of
//!   all that apply decides**. `authorize(_, propose, page)` is decided as it is without protections (a protection asks something of
//!   the acceptance, never of who may propose), and what an acceptance lacks of the rest (the approval a review asks) is the
//!   acceptance's, from the same notices. See [`protection`](crate::protection). **No template and no handler evaluates one.**
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

mod protections;

pub use protections::{ProtectError, Setting};

use crate::governance::{GovernanceFile, ReadFault, Rules};
use inklkunn_signer::message::SystemLabel;
use inklkunn_store::{
    AccountId, AuditEvent, AuditOutcome, AuditSink, GovernanceStore, RestrictionEvent, Role,
    RoleStore, Scope, StoreError, Visibility, WikiId, WikiStore,
};

mod holding;

pub use holding::{HeldBy, Holding, RestrictionNotice};

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
            // Creating a view is a reviewers' act (the owner's ruling, 2026-10-31; RFC 005 part 2 D-9): a view is a
            // lasting community object, and the people who read proposals for the wiki are the ones who start one.
            Action::Review | Action::CreateView => Role::Reviewer,
            Action::Accept | Action::Moderate => Role::Maintainer,
            Action::Admin | Action::Export | Action::Import => Role::Admin,
        }
    }

    /// The narrowest token scope that covers this (RFC 002 D-1: `read ⊂ propose ⊂ review ⊂ accept ⊂
    /// admin`). Commenting is covered by `propose`; creating a view by `review`, as the role that may do it
    /// is the reviewer's; moderating by `accept`.
    pub const fn required_scope(self) -> Scope {
        match self {
            Action::Read => Scope::Read,
            Action::Comment | Action::Propose => Scope::Propose,
            Action::Review | Action::CreateView => Scope::Review,
            Action::Accept | Action::Moderate => Scope::Accept,
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
    /// The role the account holds in the wiki **through a grant** (always `None` for a principal that is not an account).
    pub role: Option<Role>,
    /// The highest rung a restriction leaves the account (RFC 006 D-7: a restriction caps an account at observer), or
    /// `None` when nothing caps it. **Filled by the [`Authorizer`]'s own loader** from the restrictions in force at the
    /// clock's time ([`AuthzSource::restriction`]), for a write by an account (a restriction never reduces reading), so that
    /// the rung is computed in one place, in the order the RFC gives (the higher of grant and `entry`, then capped).
    pub cap: Option<Role>,
}

/// The rung an account holds in the wiki, under `rules`: **the higher of its grant and the wiki's `entry`, then capped by a
/// restriction** (RFC 006 D-5). `None` is no rung at all (the account may read what the wiki's visibility lets it read, and
/// nothing else). `entry` gives nothing in a `restricted` wiki: its members are the accounts the operator granted a rung.
/// A wiki that is not there gives no rung.
///
/// **An admin is never capped** (RFC 006 INV-14: an admin of a wiki can always change its rules; D-7 and C-14h: nobody
/// restricts an admin). A restriction is refused for an admin when it is set, and an account that was restricted and then
/// became an admin is not held down by the restriction it had.
pub fn rung_of(snapshot: Snapshot, rules: &Rules) -> Option<Role> {
    let visibility = snapshot.visibility?;
    let held = snapshot.role.max(entry_rung(visibility, rules));
    match (held, snapshot.cap) {
        (Some(Role::Admin), _) => held,
        (Some(held), Some(cap)) => Some(held.min(cap)),
        (held, _) => held,
    }
}

/// The rung `entry` gives every signed-in account in a wiki of this visibility, or `None`: nothing in a `restricted` wiki,
/// whose members are the accounts the operator granted a rung.
pub fn entry_rung(visibility: Visibility, rules: &Rules) -> Option<Role> {
    match visibility {
        Visibility::Restricted => None,
        Visibility::Public | Visibility::Internal => rules.policy().entry().rung(),
    }
}

/// The rule (see the module docs for the table), **with 0.3.0's rules**: [`decide_under`] with no governance. Pure: the same
/// inputs always give the same answer, and every path that is not explicitly allowed is denied.
pub fn decide(snapshot: Snapshot, principal: &Principal, action: Action) -> Decision {
    decide_under(snapshot, &Rules::defaults(), principal, action, false)
}

/// The rule under the wiki's [`Rules`]. Pure: the same inputs always give the same answer, and every path that is not
/// explicitly allowed is denied.
///
/// `on_governance_file` says that the resource is one of the two governance files or a path below one (a page resource whose
/// path is `governance/constitution.yaml`, `governance/policy.yaml` or lies below either): it matters only in the repair
/// state, where the wiki refuses every write ([`Rules::fault`]) and an admin may still propose a change to those paths (the
/// file written, what is below it removed).
pub fn decide_under(
    snapshot: Snapshot,
    rules: &Rules,
    principal: &Principal,
    action: Action,
    on_governance_file: bool,
) -> Decision {
    let Some(visibility) = snapshot.visibility else {
        // No such wiki: denied for everyone, in the same words as a wiki one may not see.
        return Decision::Deny;
    };
    let allowed = match principal {
        Principal::System(_) => matches!(action, Action::Read | Action::Accept | Action::Import),
        Principal::Anonymous => action == Action::Read && visibility == Visibility::Public,
        Principal::Account(a) => {
            let scope = &a.scope;
            let rung = rung_of(snapshot, rules);
            // The credential limits the account; the role limits the credential. Both must allow.
            *scope >= action.required_scope()
                && if action != Action::Read && rules.fault().is_some() {
                    // A governance file cannot be read: every write refuses, but an admin's repair of the rules.
                    rung == Some(Role::Admin)
                        && match action {
                            Action::Propose => on_governance_file,
                            Action::Accept => true,
                            _ => false,
                        }
                } else {
                    match rung {
                        Some(role) => role >= rules.policy().minimum_role(action),
                        None => action == Action::Read && visibility != Visibility::Restricted,
                    }
                }
        }
    };
    if allowed {
        Decision::Allow
    } else {
        Decision::Deny
    }
}

/// Is the resource one of the two governance files, **or a path below one** (what stands where the file belongs, and what a
/// repair removes: handoff G-T1 correction D-1)? In the repair state an admin may propose a change to exactly these.
fn is_governance_file(resource: &Resource) -> bool {
    matches!(resource, Resource::Page { path, .. } if GovernanceFile::at_or_below(path).is_some())
}

/// Where an account stands in a wiki at this moment: **the rung it holds and the block that set the rules it holds it
/// under** (RFC 006 D-5). What a decision record names (*accepted by Name (maintainer), under the rules as of
/// `<block>`*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// The rung, or `None` for an account that holds none.
    pub rung: Option<Role>,
    /// The block that last changed a governance file, or `None` for the defaults.
    pub policy_block: Option<String>,
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
    /// The role the account holds in the wiki through a grant, if any.
    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>, SourceError>;
    /// The restriction in force on the account in the wiki at `at` (RFC 006 D-7), the one that ends last, or `None`. **The
    /// default answer is none**: a source that does not keep restrictions caps no one. (The service's own source,
    /// [`StoreAuthz::with_restrictions`], reads them; a surface cannot make one.) An error is *could not be read now*, and the
    /// decision it was for is **deny**.
    fn restriction(
        &self,
        account: &AccountId,
        wiki: &WikiId,
        at: i64,
    ) -> Result<Option<RestrictionEvent>, SourceError> {
        let _ = (account, wiki, at);
        Ok(None)
    }
    /// The rules in force for the wiki: **its governance as read from the tip of its default view** (RFC 006 D-5). The
    /// default answer is the defaults, which are 0.3.0's rules: a source that does not read governance decides every wiki as
    /// a wiki with none. (The service's own source, [`StoreAuthz::with_rules`], reads it; a surface cannot make one.)
    fn rules(&self, wiki: &WikiId) -> Result<Arc<Rules>, SourceError> {
        let _ = wiki;
        Ok(Arc::new(Rules::defaults()))
    }
}

/// Where the rules of a wiki are read from: the tip of its default view. Implemented by the read service
/// ([`ReadService`](crate::ReadService)), which holds the derived snapshot and rebuilds it from the repository.
pub trait RulesSource: Send + Sync {
    /// The rules in force for the wiki. An error is *could not be read now*, and the decision it was for is **deny**.
    fn rules(&self, wiki: &WikiId) -> Result<Arc<Rules>, SourceError>;
}

/// The facts from the service store: the wiki record and the role grants.
#[derive(Clone)]
pub struct StoreAuthz {
    wikis: Arc<dyn WikiStore>,
    roles: Arc<dyn RoleStore>,
    rules: Option<Arc<dyn RulesSource>>,
    restrictions: Option<Arc<dyn GovernanceStore>>,
}

impl std::fmt::Debug for StoreAuthz {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StoreAuthz")
    }
}

impl StoreAuthz {
    /// Over a wiki store and a role store (the same SQLite store serves both in service), **with no governance**: every wiki
    /// is decided by the defaults, 0.3.0's rules. The service reads governance through [`StoreAuthz::with_rules`].
    pub fn new(wikis: Arc<dyn WikiStore>, roles: Arc<dyn RoleStore>) -> Self {
        StoreAuthz {
            wikis,
            roles,
            rules: None,
            restrictions: None,
        }
    }

    /// The same, reading each wiki's rules from `rules`: the tip of its default view.
    #[must_use]
    pub fn with_rules(mut self, rules: Arc<dyn RulesSource>) -> Self {
        self.rules = Some(rules);
        self
    }

    /// The same, reading each account's restriction from the records of governance (RFC 006 D-7). **Without it no one is
    /// restricted**, which is every authorizer of a test that is not about restrictions.
    #[must_use]
    pub fn with_restrictions(mut self, restrictions: Arc<dyn GovernanceStore>) -> Self {
        self.restrictions = Some(restrictions);
        self
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

    fn restriction(
        &self,
        account: &AccountId,
        wiki: &WikiId,
        at: i64,
    ) -> Result<Option<RestrictionEvent>, SourceError> {
        match &self.restrictions {
            Some(records) => Ok(records
                .active_restrictions(wiki, account, at)
                .map_err(source_error)?
                .into_iter()
                .next()),
            None => Ok(None),
        }
    }

    fn rules(&self, wiki: &WikiId) -> Result<Arc<Rules>, SourceError> {
        match &self.rules {
            Some(source) => source.rules(wiki),
            None => Ok(Arc::new(Rules::defaults())),
        }
    }
}

/// What a surface calls: [`decide_under`] over a snapshot and the wiki's rules, which it loads itself, with the audit trail.
#[derive(Clone)]
pub struct Authorizer {
    source: Arc<dyn AuthzSource>,
    audit: Arc<dyn AuditSink>,
    clock: fn() -> i64,
    /// The rules of a wiki that has none (and what a decision uses when the rules cannot matter to it).
    defaults: Arc<Rules>,
    /// Where temporary protections are kept (RFC 006 D-7); none until [`Authorizer::with_protections`].
    protections: Option<protections::Book>,
}

impl std::fmt::Debug for Authorizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Authorizer")
    }
}

/// What a decision was made from: the wiki's facts and its rules.
struct Facts {
    snapshot: Snapshot,
    rules: Arc<Rules>,
    /// The restriction that caps the account (the one that ends last), when one does.
    restriction: Option<RestrictionEvent>,
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
            defaults: Arc::new(Rules::defaults()),
            protections: None,
        }
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    /// The wiki's visibility and the account's grant, and, **when they can matter to the decision**, the wiki's rules.
    ///
    /// The rules decide a *write by an account* in a wiki the account may be a member of. A read is never decided by them
    /// (reading is always `observer`, no `entry` gives less, and a restriction caps at `observer`), the system's acts and an
    /// anonymous caller are not decided by them, a wiki that is not there has none, and `entry` gives nothing in a
    /// `restricted` wiki to an account that holds no grant there: for each of these the defaults stand in, and the source
    /// is not asked (a read costs no walk of the repository; an absent wiki and a restricted one the caller is no member of
    /// still cost the same).
    fn facts(
        &self,
        principal: &Principal,
        action: Action,
        wiki: &WikiId,
    ) -> Result<Facts, SourceError> {
        self.facts_under(principal, action, wiki, None)
    }

    /// [`Authorizer::facts`] with the rules **given** (`Some`), where the source would be asked for them: the acceptance
    /// judges by the rules of the state it has just read (C-2), whatever the source holds.
    fn facts_under(
        &self,
        principal: &Principal,
        action: Action,
        wiki: &WikiId,
        given: Option<&Arc<Rules>>,
    ) -> Result<Facts, SourceError> {
        let visibility = self.source.visibility(wiki)?;
        // The role is looked up for every account whether or not the wiki exists, so that an absent wiki and
        // a private one cost the same to ask about.
        let role = match principal {
            Principal::Account(a) => self.source.role(&a.id, wiki)?,
            Principal::Anonymous | Principal::System(_) => None,
        };
        // A restriction caps the account's writes and never its reading, so a read is not asked about one. It is asked for
        // every write by an account, whether or not the wiki exists, so that an absent wiki and a private one cost the same.
        let restriction = match principal {
            Principal::Account(a) if action != Action::Read => {
                self.source.restriction(&a.id, wiki, (self.clock)())?
            }
            Principal::Account(_) | Principal::Anonymous | Principal::System(_) => None,
        };
        let snapshot = Snapshot {
            visibility,
            role,
            cap: restriction.as_ref().map(|_| Role::Observer),
        };
        let rules_matter = action != Action::Read
            && matches!(principal, Principal::Account(_))
            && visibility.is_some_and(|v| role.is_some() || v != Visibility::Restricted);
        let rules = if rules_matter {
            match given {
                Some(rules) => Arc::clone(rules),
                None => self.source.rules(wiki)?,
            }
        } else {
            Arc::clone(&self.defaults)
        };
        Ok(Facts {
            snapshot,
            rules,
            restriction,
        })
    }

    /// The role `account` holds in `wiki`, for the service's own bookkeeping (which limit binds whom): nothing is
    /// recorded and nothing leaves this crate. `None` when it holds none, or when the facts cannot be read (the
    /// answer that binds the account to every limit). **A grant only**: the rung `entry` gives is not counted, which binds
    /// the account to every limit, the stricter side.
    pub(crate) fn role_in(&self, account: &AccountId, wiki: &WikiId) -> Option<Role> {
        self.source.role(account, wiki).ok().flatten()
    }

    /// The rung `principal` holds in `wiki` now and the block that set the rules it holds it under: what a decision record
    /// names (RFC 006 D-5; criterion 12). Nothing is recorded. An error is *could not be read now*.
    pub fn standing(&self, principal: &Principal, wiki: &WikiId) -> Result<Standing, SourceError> {
        self.standing_under(principal, wiki, None)
    }

    /// [`Authorizer::standing`] under the rules **given**, which the caller has just read for the state it acts on (C-2).
    pub fn standing_under(
        &self,
        principal: &Principal,
        wiki: &WikiId,
        rules: Option<&Arc<Rules>>,
    ) -> Result<Standing, SourceError> {
        // A write is asked, so that the rules are read: the rung is theirs.
        let facts = self.facts_under(principal, Action::Propose, wiki, rules)?;
        let rung = match principal {
            Principal::Account(_) => rung_of(facts.snapshot, &facts.rules),
            Principal::Anonymous | Principal::System(_) => None,
        };
        Ok(Standing {
            rung,
            policy_block: facts.rules.block().map(str::to_owned),
        })
    }

    /// The rules in force for `wiki`, as the source reads them from the default view's tip. For a service that has to say
    /// what they are (the acceptance, the approvals warning, a page): never a way to assert anything about a wiki.
    pub fn rules(&self, wiki: &WikiId) -> Result<Arc<Rules>, SourceError> {
        self.source.rules(wiki)
    }

    /// **Why every write of this wiki refuses**, for a principal who may read the wiki: the governance file that cannot be
    /// read, with its line and the rule it breaks ([`ReadFault`]'s `Display`). `None` when nothing is wrong, when the
    /// principal may not read the wiki (it is told what anyone is told about a wiki it may not see: nothing), or when the
    /// facts cannot be read. Nothing is recorded.
    pub fn write_refusal(&self, principal: &Principal, wiki: &WikiId) -> Option<ReadFault> {
        let facts = self.facts(principal, Action::Propose, wiki).ok()?;
        let reads = decide_under(facts.snapshot, &facts.rules, principal, Action::Read, false);
        if !reads.is_allowed() {
            return None;
        }
        facts.rules.fault().cloned()
    }

    /// May this account, **carrying the whole authority of its role** as a browser session does, do the
    /// non-consequential `action` to `resource`? For the service's own bookkeeping about *other* people (whom
    /// to tell something, a watch to keep, whether a reviewer still holds the rung): it builds no principal that leaves this
    /// crate and records nothing. It is never the answer to a request: a request is authorized as the principal it came
    /// from, whose credential may carry less. Fails closed, and refuses a consequential action outright.
    pub(crate) fn account_may(
        &self,
        account: &AccountId,
        action: Action,
        resource: &Resource,
    ) -> bool {
        self.account_may_under(account, action, resource, None)
    }

    /// [`Authorizer::account_may`] under the rules **given**, where the source would be asked for them (C-2).
    pub(crate) fn account_may_under(
        &self,
        account: &AccountId,
        action: Action,
        resource: &Resource,
        rules: Option<&Arc<Rules>>,
    ) -> bool {
        if action.is_consequential() {
            return false;
        }
        let principal = Principal::Account(AccountPrincipal {
            id: account.clone(),
            scope: Scope::Admin,
        });
        self.facts_under(&principal, action, resource.wiki(), rules)
            .is_ok_and(|facts| {
                decide_under(
                    facts.snapshot,
                    &facts.rules,
                    &principal,
                    action,
                    is_governance_file(resource),
                )
                .is_allowed()
            })
    }

    /// May `principal` do `action` to `resource`? **Fails closed**: if the facts cannot be read the answer is
    /// deny. A consequential action is recorded whatever the answer, **with the rung the principal held and the block of
    /// the rules it was decided under** (`rung=<rung or none>; policy=<the block's first 12 characters, or defaults>`); an
    /// *allowed* one whose record cannot be written is denied, because nothing consequential happens unrecorded.
    pub fn authorize(
        &self,
        principal: &Principal,
        action: Action,
        resource: &Resource,
    ) -> Decision {
        let (decision, outcome, detail) = match self.facts(principal, action, resource.wiki()) {
            Ok(facts) => {
                let decision = decide_under(
                    facts.snapshot,
                    &facts.rules,
                    principal,
                    action,
                    is_governance_file(resource),
                );
                // A protection of the page asks a rung of whoever accepts a change to it (RFC 006 D-7; C-3g).
                let decision = self.protected(decision, &facts, principal, action, resource);
                let outcome = if decision.is_allowed() {
                    AuditOutcome::Allowed
                } else {
                    AuditOutcome::Denied
                };
                (decision, outcome, authority_detail(principal, &facts))
            }
            Err(_) => (
                Decision::Deny,
                AuditOutcome::Failed,
                "the facts for the decision could not be read".to_owned(),
            ),
        };
        if action.is_consequential() {
            let event = AuditEvent::new(
                (self.clock)(),
                &principal.audit_name(),
                action.name(),
                &resource.audit_name(),
                outcome,
                &detail,
            );
            if self.audit.record(&event).is_err() && decision.is_allowed() {
                return Decision::Deny;
            }
        }
        decision
    }
}

/// The detail of a consequential decision's audit event: **the rung the principal held and the block of the rules the
/// decision was made under**, in one stable form (`rung=maintainer; policy=0123456789ab`, or `rung=none; policy=defaults`).
/// Never a path or a content (RFC 002 D-9).
fn authority_detail(principal: &Principal, facts: &Facts) -> String {
    let rung = match principal {
        Principal::Account(_) => rung_of(facts.snapshot, &facts.rules),
        Principal::Anonymous | Principal::System(_) => None,
    };
    format!(
        "rung={}; policy={}",
        rung.map_or("none", Role::as_str),
        facts.rules.short_block()
    )
}

#[cfg(test)]
mod tests;
