//! A world for the people tests: one wiki of a chosen visibility under chosen rules, the real authorizer over the store's
//! grants and restrictions with a clock the test sets, the people service and the proposal service over it, and an audit
//! trail the test reads. Built over either store, so each test body runs on both.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::sync::Arc;

use inklkunn_store::{
    AcceptanceStore, Account, AccountId, AccountStore, Base, ClaimThreadStore, DisplayName,
    GovernanceStore, InboxStore, MemoryAudit, MemoryStore, NewWiki, ProposalStore, Role, RoleGrant,
    RoleStore, Slug, SqliteStore, Visibility, WikiRecord, WikiStore,
};

use crate::authorize::{
    Authorizer, IdentityAuthority, Principal, RulesSource, SourceError, StoreAuthz,
};
use crate::governance::{Constitution, PlatformLimits, Rules, read_policy};
use crate::people::PeopleService;
use crate::proposal::{BaseError, BaseSource, ProposalService, QuotaConfig};

thread_local! {
    static NOW: Cell<i64> = const { Cell::new(0) };
}

/// The time every test starts at.
pub(super) const T0: i64 = 10_000_000;

/// One day, in seconds.
pub(super) const DAY: i64 = 86_400;

pub(super) fn now() -> i64 {
    NOW.with(Cell::get)
}

pub(super) fn set_now(t: i64) {
    NOW.with(|n| n.set(t));
}

/// The block the rules of a world with a policy file were set by.
pub(super) const BLOCK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A store that is every kind of store the services read.
pub(super) trait Everything:
    ProposalStore
    + AcceptanceStore
    + InboxStore
    + ClaimThreadStore
    + WikiStore
    + RoleStore
    + AccountStore
    + GovernanceStore
{
}

impl<T> Everything for T where
    T: ProposalStore
        + AcceptanceStore
        + InboxStore
        + ClaimThreadStore
        + WikiStore
        + RoleStore
        + AccountStore
        + GovernanceStore
{
}

/// The rules a test fixes.
struct Fixed(Arc<Rules>);

impl RulesSource for Fixed {
    fn rules(&self, _: &inklkunn_store::WikiId) -> Result<Arc<Rules>, SourceError> {
        Ok(self.0.clone())
    }
}

/// A repository that has no pages: the proposal service needs a base, and nothing here reads one.
struct NoPages;

impl BaseSource for NoPages {
    fn base(&self, _: &WikiRecord, _: &str) -> Result<Base, BaseError> {
        Ok(Base::new(
            &format!("{:064x}", 1_u64),
            &format!("{:064x}", 0xb10c_0001_u64),
        )
        .expect("a base"))
    }

    fn page_paths(&self, _: &WikiRecord, _: &Base) -> Result<BTreeSet<String>, BaseError> {
        Ok(BTreeSet::new())
    }

    fn page_text(&self, _: &WikiRecord, _: &Base, _: &str) -> Result<Option<String>, BaseError> {
        Ok(None)
    }
}

/// A person: the account, and a session principal for it.
#[derive(Clone)]
pub(super) struct Person {
    pub(super) id: AccountId,
    pub(super) principal: Principal,
}

pub(super) struct World {
    pub(super) people: PeopleService,
    pub(super) authorizer: Authorizer,
    pub(super) proposals: ProposalService,
    pub(super) audit: Arc<MemoryAudit>,
    pub(super) store: Arc<dyn Everything>,
    pub(super) wiki: WikiRecord,
    pub(super) authority: IdentityAuthority,
}

/// Which store a world is built over.
#[derive(Debug, Clone, Copy)]
pub(super) enum Kind {
    Memory,
    Sqlite,
}

pub(super) const KINDS: [Kind; 2] = [Kind::Memory, Kind::Sqlite];

impl World {
    /// A public wiki under the default rules.
    pub(super) fn new(kind: Kind) -> World {
        World::with(kind, Visibility::Public, None, PlatformLimits::default())
    }

    /// A public wiki under the policy `policy` (a policy file's text).
    pub(super) fn under(kind: Kind, policy: &str) -> World {
        World::with(
            kind,
            Visibility::Public,
            Some(policy),
            PlatformLimits::default(),
        )
    }

    pub(super) fn with(
        kind: Kind,
        visibility: Visibility,
        policy: Option<&str>,
        limits: PlatformLimits,
    ) -> World {
        match kind {
            Kind::Memory => World::over(Arc::new(MemoryStore::new()), visibility, policy, limits),
            Kind::Sqlite => World::over(
                Arc::new(SqliteStore::open_in_memory().expect("a store")),
                visibility,
                policy,
                limits,
            ),
        }
    }

    fn over<S: Everything + 'static>(
        store: Arc<S>,
        visibility: Visibility,
        policy: Option<&str>,
        limits: PlatformLimits,
    ) -> World {
        set_now(T0);
        let mut new = NewWiki::new(
            Slug::new("people").expect("slug"),
            DisplayName::new("People").expect("name"),
        );
        new.visibility = visibility;
        let wiki = store.create(new).expect("a wiki");
        let rules = Arc::new(match policy {
            Some(text) => Rules::read(
                Some(BLOCK.to_owned()),
                read_policy(text.as_bytes()).expect("a policy"),
                Constitution::default(),
                &limits,
            ),
            None => Rules::defaults(),
        });
        let audit = Arc::new(MemoryAudit::new());
        let authz = StoreAuthz::new(store.clone(), store.clone())
            .with_rules(Arc::new(Fixed(rules)))
            .with_restrictions(store.clone());
        let authorizer = Authorizer::new(Arc::new(authz), audit.clone()).with_clock(now);
        let people = PeopleService::new(
            authorizer.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            audit.clone(),
            limits,
        );
        let proposals = ProposalService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            authorizer.clone(),
            Arc::new(NoPages),
            QuotaConfig::default(),
        )
        .with_clock(now);
        World {
            people,
            authorizer,
            proposals,
            audit,
            store,
            wiki,
            authority: IdentityAuthority::for_tests(),
        }
    }

    /// A new account named `name` holding `role` in the wiki through a grant (none: no grant).
    pub(super) fn person(&self, name: &str, role: Option<Role>) -> Person {
        let id = AccountId::new();
        self.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new(name).expect("a name"),
                local: None,
                created_at: 0,
                disabled: false,
            })
            .expect("an account");
        if let Some(role) = role {
            self.store
                .grant(RoleGrant {
                    account: id.clone(),
                    wiki: self.wiki.id.clone(),
                    role,
                    granted_by: None,
                    granted_at: 0,
                })
                .expect("a grant");
        }
        let principal = Principal::session(&self.authority, id.clone());
        Person { id, principal }
    }

    /// The account of `who`, disabled.
    pub(super) fn disable(&self, who: &Person) {
        self.store.set_disabled(&who.id, true).expect("disabled");
    }

    /// The rung `who` holds in the wiki through a grant now.
    pub(super) fn grant_of(&self, who: &Person) -> Option<Role> {
        self.store.role(&who.id, &self.wiki.id).expect("a role")
    }

    /// May `who` (a session) do `action` to the wiki?
    pub(super) fn may(&self, who: &Person, action: crate::Action) -> bool {
        self.authorizer
            .authorize(
                &who.principal,
                action,
                &crate::Resource::Wiki {
                    wiki: self.wiki.id.clone(),
                },
            )
            .is_allowed()
    }
}
