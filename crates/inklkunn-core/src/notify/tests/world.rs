//! A world for the notification tests: one wiki whose visibility the test can change, accounts holding each
//! role (some with a sign-in name, for mentions), the proposal service wired to a notifier, and the inbox.

use std::cell::Cell;
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_store::{
    Account, AccountId, AccountStore, Base, DisplayName, FileChange, InboxEvent, InboxEventKind,
    InboxStore, LocalCredential, LoginName, MemoryAudit, MemoryStore, NewWiki, ProposalRecord,
    Role, RoleGrant, RoleStore, Slug, SqliteStore, Visibility, WikiId, WikiRecord, WikiStore,
};

use crate::authorize::{Authorizer, AuthzSource, IdentityAuthority, Principal, SourceError};
use crate::inbox::Inbox;
use crate::notify::{Channel, ChannelError, Notifier};
use crate::proposal::{
    BaseError, BaseSource, NewComment, ProposalEdit, ProposalService, QuotaConfig,
};

thread_local! {
    static NOW: Cell<i64> = const { Cell::new(0) };
}

fn now() -> i64 {
    NOW.with(Cell::get)
}

pub(super) const T0: i64 = 10_000_000;

pub(super) fn set_now(t: i64) {
    NOW.with(|n| n.set(t));
}

/// Every kind of store the world reads.
pub(super) trait Everything:
    inklkunn_store::ProposalStore
    + inklkunn_store::AcceptanceStore
    + InboxStore
    + WikiStore
    + RoleStore
    + AccountStore
{
}

impl<T> Everything for T where
    T: inklkunn_store::ProposalStore
        + inklkunn_store::AcceptanceStore
        + InboxStore
        + WikiStore
        + RoleStore
        + AccountStore
{
}

/// The facts for a decision, with the wiki's visibility the test can change.
pub(super) struct Facts {
    store: Arc<dyn Everything>,
    visibility: Mutex<Visibility>,
}

impl AuthzSource for Facts {
    fn visibility(&self, _: &WikiId) -> Result<Option<Visibility>, SourceError> {
        Ok(Some(
            *self
                .visibility
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        ))
    }

    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>, SourceError> {
        self.store
            .role(account, wiki)
            .map_err(|e| SourceError(e.to_string()))
    }
}

struct Tip;

impl BaseSource for Tip {
    fn base(&self, _: &WikiRecord, _: &str) -> Result<Base, BaseError> {
        Ok(Base::new(
            &format!("{:064x}", 1_u64),
            &format!("{:064x}", 0xb10c_0001_u64),
        )
        .expect("a base"))
    }
}

/// A person: the account and a session principal for it.
#[derive(Clone)]
pub(super) struct Person {
    pub(super) id: AccountId,
    pub(super) principal: Principal,
}

/// A channel that only remembers: the test's "fake" for the function the acceptance calls.
#[derive(Default)]
pub(super) struct Recorder {
    pub(super) events: Mutex<Vec<InboxEvent>>,
}

impl Recorder {
    pub(super) fn seen(&self) -> Vec<InboxEvent> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Channel for Recorder {
    fn name(&self) -> &'static str {
        "recorder"
    }

    fn deliver(&self, event: &InboxEvent) -> Result<(), ChannelError> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event.clone());
        Ok(())
    }
}

pub(super) struct World {
    pub(super) store: Arc<dyn Everything>,
    pub(super) facts: Arc<Facts>,
    pub(super) wiki: WikiRecord,
    pub(super) service: ProposalService,
    pub(super) notifier: Arc<Notifier>,
    pub(super) inbox: Inbox,
    pub(super) authority: IdentityAuthority,
    pub(super) author: Person,
    pub(super) reviewer: Person,
    pub(super) maintainer: Person,
    pub(super) observer: Person,
    pub(super) outsider: Person,
}

impl World {
    pub(super) fn memory() -> World {
        World::over(Arc::new(MemoryStore::new()), None)
    }

    pub(super) fn sqlite() -> World {
        World::over(Arc::new(SqliteStore::open_in_memory().expect("open")), None)
    }

    /// A world whose notifier's only channel is `recorder`.
    pub(super) fn recording(recorder: Arc<Recorder>) -> World {
        World::over(Arc::new(MemoryStore::new()), Some(recorder))
    }

    fn over<S: Everything + 'static>(store: Arc<S>, recorder: Option<Arc<Recorder>>) -> World {
        set_now(T0);
        let wiki = store
            .create(NewWiki::new(
                Slug::new("notify").expect("slug"),
                DisplayName::new("Notify").expect("name"),
            ))
            .expect("a wiki");
        let store: Arc<dyn Everything> = store;
        let facts = Arc::new(Facts {
            store: store.clone(),
            visibility: Mutex::new(Visibility::Internal),
        });
        let authorizer = Authorizer::new(facts.clone(), Arc::new(MemoryAudit::new()));
        let mut notifier = Notifier::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            authorizer.clone(),
        )
        .with_clock(now);
        if let Some(recorder) = recorder {
            notifier = notifier.with_channels(vec![recorder]);
        }
        let notifier = Arc::new(notifier);
        let service = ProposalService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            authorizer.clone(),
            Arc::new(Tip),
            QuotaConfig::default(),
        )
        .with_clock(now)
        .with_notifier(notifier.clone());
        let inbox = Inbox::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            authorizer,
        )
        .with_clock(now);
        let placeholder = || Person {
            id: AccountId::new(),
            principal: Principal::Anonymous,
        };
        let mut world = World {
            store,
            facts,
            wiki,
            service,
            notifier,
            inbox,
            authority: IdentityAuthority::for_tests(),
            author: placeholder(),
            reviewer: placeholder(),
            maintainer: placeholder(),
            observer: placeholder(),
            outsider: placeholder(),
        };
        world.author = world.person(Some(Role::Contributor), Some("author"));
        world.reviewer = world.person(Some(Role::Reviewer), Some("reviewer"));
        world.maintainer = world.person(Some(Role::Maintainer), Some("maintainer"));
        world.observer = world.person(Some(Role::Observer), Some("observer"));
        world.outsider = world.person(None, Some("outsider"));
        world
    }

    /// A new account holding `role` in the wiki, signing in as `login`.
    pub(super) fn person(&self, role: Option<Role>, login: Option<&str>) -> Person {
        let id = AccountId::new();
        self.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new(login.unwrap_or("Somebody")).expect("name"),
                local: login.map(|l| LocalCredential {
                    login: LoginName::parse(l).expect("a login"),
                    password_hash:
                        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaC1mb3ItdGVzdHM1"
                            .to_owned(),
                }),
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
        Person {
            principal: Principal::session(&self.authority, id.clone()),
            id,
        }
    }

    pub(super) fn set_visibility(&self, visibility: Visibility) {
        *self
            .facts
            .visibility
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = visibility;
    }

    /// A proposal by `who` of one page, sent for review.
    pub(super) fn held(&self, who: &Person, summary: &str) -> ProposalRecord {
        let draft = self
            .service
            .create(
                &who.principal,
                &self.wiki,
                None,
                ProposalEdit {
                    summary: summary.to_owned(),
                    files: vec![FileChange {
                        path: "pages/Home.md".to_owned(),
                        text: Some("text".to_owned()),
                    }],
                    ..ProposalEdit::default()
                },
            )
            .expect("a draft");
        self.service
            .submit(&who.principal, &self.wiki, &draft.id)
            .expect("submitted")
    }

    pub(super) fn comment(
        &self,
        who: &Person,
        p: &ProposalRecord,
        body: &str,
    ) -> inklkunn_store::CommentRecord {
        self.service
            .comment(
                &who.principal,
                &self.wiki,
                &p.id,
                NewComment {
                    body: body.to_owned(),
                    ..NewComment::default()
                },
            )
            .expect("a comment")
    }

    /// What is in this person's inbox, newest first, kinds only.
    pub(super) fn kinds(&self, who: &Person) -> Vec<InboxEventKind> {
        self.store
            .inbox(&who.id, false, 100)
            .expect("an inbox")
            .into_iter()
            .map(|e| e.kind)
            .collect()
    }
}

/// Run a test body over each store.
pub(super) fn on_both(body: impl Fn(&World)) {
    body(&World::memory());
    body(&World::sqlite());
}
