//! A world for the proposal service tests: one wiki, accounts holding each role, the authorizer over the
//! store's role grants, a base source the test moves, and a clock the test sets. Built over either store, so
//! each test body runs on both (`on_both`).

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_store::{
    Account, AccountId, AccountStore, Base, DisplayName, FileChange, MemoryAudit, MemoryStore,
    NewWiki, ProposalRecord, ProposalStore, Role, RoleGrant, RoleStore, Scope, Slug, SqliteStore,
    WikiRecord, WikiStore,
};

use crate::authorize::{Authorizer, IdentityAuthority, Principal, StoreAuthz};
use crate::proposal::{BaseError, BaseSource, ProposalEdit, ProposalService, QuotaConfig};

thread_local! {
    static NOW: Cell<i64> = const { Cell::new(0) };
}

fn now() -> i64 {
    NOW.with(Cell::get)
}

/// The time every test starts at: far past a new account's first week.
pub(super) const T0: i64 = 10_000_000;

pub(super) fn set_now(t: i64) {
    NOW.with(|n| n.set(t));
}

/// A base source the test controls.
#[derive(Default)]
pub(super) struct Bases {
    n: AtomicU64,
    fail: Mutex<Option<BaseError>>,
}

impl Bases {
    pub(super) fn move_to(&self, n: u64) {
        self.n.store(n, Ordering::SeqCst);
    }

    pub(super) fn current(&self) -> Base {
        base(self.n.load(Ordering::SeqCst))
    }

    pub(super) fn fail_with(&self, e: Option<BaseError>) {
        *self.fail.lock().unwrap_or_else(PoisonError::into_inner) = e;
    }
}

impl BaseSource for Bases {
    fn base(&self, _: &WikiRecord, _: &str) -> Result<Base, BaseError> {
        if let Some(e) = self
            .fail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Err(e);
        }
        Ok(self.current())
    }
}

/// A base made from one number.
pub(super) fn base(n: u64) -> Base {
    Base::new(&format!("{n:064x}"), &format!("{:064x}", 0xb10c_0000 + n)).expect("a base")
}

/// Which store a world is built over.
#[derive(Debug, Clone, Copy)]
pub(super) enum Kind {
    Memory,
    Sqlite,
}

pub(super) struct Env {
    pub(super) service: ProposalService,
    pub(super) wiki: WikiRecord,
    pub(super) bases: Arc<Bases>,
    pub(super) authority: IdentityAuthority,
    pub(super) store: Arc<dyn Everything>,
    pub(super) author: Actor,
    pub(super) reviewer: Actor,
    pub(super) maintainer: Actor,
    pub(super) admin: Actor,
    pub(super) observer: Actor,
    pub(super) outsider: Actor,
}

/// A store that is every kind of store the service reads.
pub(super) trait Everything:
    ProposalStore
    + inklkunn_store::AcceptanceStore
    + inklkunn_store::InboxStore
    + WikiStore
    + RoleStore
    + AccountStore
{
}

impl<T> Everything for T where
    T: ProposalStore
        + inklkunn_store::AcceptanceStore
        + inklkunn_store::InboxStore
        + WikiStore
        + RoleStore
        + AccountStore
{
}

/// A person: the account and a session principal for it.
#[derive(Clone)]
pub(super) struct Actor {
    pub(super) id: AccountId,
    pub(super) principal: Principal,
}

impl Env {
    pub(super) fn new(kind: Kind) -> Env {
        Env::with(kind, QuotaConfig::default())
    }

    pub(super) fn with(kind: Kind, config: QuotaConfig) -> Env {
        match kind {
            Kind::Memory => Env::over(Arc::new(MemoryStore::new()), config),
            Kind::Sqlite => Env::over(
                Arc::new(SqliteStore::open_in_memory().expect("open")),
                config,
            ),
        }
    }

    pub(super) fn over<S: Everything + 'static>(store: Arc<S>, config: QuotaConfig) -> Env {
        set_now(T0);
        let wiki = store
            .create(NewWiki::new(
                Slug::new("proposals").expect("slug"),
                DisplayName::new("Proposals").expect("name"),
            ))
            .expect("a wiki");
        let authority = IdentityAuthority::for_tests();
        let bases = Arc::new(Bases::default());
        bases.move_to(1);
        let service = Env::service_over(&store, &bases, config);
        let mut env = Env {
            service,
            wiki,
            bases,
            authority,
            store: store.clone(),
            author: placeholder(),
            reviewer: placeholder(),
            maintainer: placeholder(),
            admin: placeholder(),
            observer: placeholder(),
            outsider: placeholder(),
        };
        env.author = env.person(Some(Role::Contributor));
        env.reviewer = env.person(Some(Role::Reviewer));
        env.maintainer = env.person(Some(Role::Maintainer));
        env.admin = env.person(Some(Role::Admin));
        env.observer = env.person(Some(Role::Observer));
        env.outsider = env.person(None);
        env
    }

    /// A service over `store`: the same records, a new process (a restart).
    pub(super) fn service_over<S: Everything + 'static>(
        store: &Arc<S>,
        bases: &Arc<Bases>,
        config: QuotaConfig,
    ) -> ProposalService {
        let authz = StoreAuthz::new(store.clone(), store.clone());
        let authorizer = Authorizer::new(Arc::new(authz), Arc::new(MemoryAudit::new()));
        ProposalService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            authorizer,
            bases.clone(),
            config,
        )
        .with_clock(now)
    }

    /// A new account holding `role` in the wiki, created long ago.
    pub(super) fn person(&self, role: Option<Role>) -> Actor {
        self.person_created(role, 0)
    }

    pub(super) fn person_created(&self, role: Option<Role>, created_at: i64) -> Actor {
        let id = AccountId::new();
        self.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new("Somebody").expect("name"),
                local: None,
                created_at,
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
        Actor { id, principal }
    }

    /// The same account reached with an API token of `scope`.
    pub(super) fn token(&self, who: &Actor, scope: Scope) -> Principal {
        Principal::token(&self.authority, who.id.clone(), scope)
    }

    /// A draft by `who`, one page.
    pub(super) fn draft(&self, who: &Actor, summary: &str, path: &str) -> ProposalRecord {
        self.service
            .create(
                &who.principal,
                &self.wiki,
                None,
                edit(summary, path, "text"),
            )
            .expect("a draft")
    }

    /// A proposal by `who`, submitted.
    pub(super) fn held(&self, who: &Actor, summary: &str, path: &str) -> ProposalRecord {
        let d = self.draft(who, summary, path);
        self.service
            .submit(&who.principal, &self.wiki, &d.id)
            .expect("submitted")
    }
}

fn placeholder() -> Actor {
    Actor {
        id: AccountId::new(),
        principal: Principal::Anonymous,
    }
}

/// An edit of one page.
pub(super) fn edit(summary: &str, path: &str, text: &str) -> ProposalEdit {
    ProposalEdit {
        summary: summary.to_owned(),
        files: vec![FileChange {
            path: path.to_owned(),
            text: Some(text.to_owned()),
        }],
        ..ProposalEdit::default()
    }
}

/// Run a test body over each store.
pub(super) fn on_both(body: impl Fn(&Env)) {
    for kind in [Kind::Memory, Kind::Sqlite] {
        body(&Env::new(kind));
    }
}

/// Run a test body over each store, with these limits.
pub(super) fn on_both_with(config: &QuotaConfig, body: impl Fn(&Env)) {
    for kind in [Kind::Memory, Kind::Sqlite] {
        body(&Env::with(kind, config.clone()));
    }
}
