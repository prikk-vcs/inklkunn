//! Test support for the acceptance path: a wiki over the in-memory repository, a memory store, the lease and the
//! services, and a signer stand-in whose answers a test scripts. Nothing here is compiled outside tests.

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use inklkunn_prikk::{Point, Storage};
use inklkunn_signer::accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ExpectedMessage, PreparedChange,
    SealedChange, ViewCreationAct,
};
use inklkunn_store::{
    AcceptanceStore, Account, AccountId, AccountStore, Base, DisplayName, FileChange, MemoryAudit,
    MemoryStore, NewWiki, ProposalId, ProposalRecord, ProposalStore, Role, RoleGrant, RoleStore,
    RootLayout, Slug, Visibility, WikiRecord, WikiStore,
};

use super::{AcceptanceParts, ProposalAcceptor, StoreLedger, SystemActs};
use crate::authorize::{Authorizer, IdentityAuthority, Principal, StoreAuthz};
use crate::lease::{LeaseConfig, WikiLeases};
use crate::notify::Notifier;
use crate::proposal::{BaseError, BaseSource, ProposalEdit, ProposalService, QuotaConfig};
use crate::read::testing::{Fake, TempDir};
use crate::read::{ReadConfig, ReadService};

/// One call to the stand-in signer.
#[derive(Debug, Clone)]
pub(crate) struct Call {
    pub(crate) view: String,
    pub(crate) token: AcceptanceToken,
    pub(crate) change: PreparedChange,
    /// What the signer's guard (the store's ledger) said about the token **at the moment of the call**.
    pub(crate) pending: Option<ExpectedMessage>,
}

type Hook = Box<dyn Fn(&Call) + Send + Sync>;

/// One call to the stand-in signer's `create_view`.
#[derive(Debug, Clone)]
pub(crate) struct ViewCall {
    pub(crate) name: String,
    pub(crate) from: String,
    pub(crate) act: ViewCreationAct,
    /// What the signer's guard (the store's ledger) said about the act **at the moment of the call**.
    pub(crate) facts: Option<inklkunn_signer::accept::ViewCreationFacts>,
    /// What the wiki's recorded lease holder was held for at the moment of the call.
    pub(crate) lease_purpose: Option<String>,
}

/// A signer stand-in: answers from a script (a sealed change when the script is empty), keeps every call, and
/// runs a hook when called.
pub(crate) struct Double {
    ledger: Arc<StoreLedger>,
    answers: Mutex<VecDeque<Result<SealedChange, AcceptError>>>,
    calls: Mutex<Vec<Call>>,
    hook: Mutex<Option<Hook>>,
    view_answers: Mutex<VecDeque<Result<(), AcceptError>>>,
    /// The next `create_view` lists its view whatever it answers (a `branch create` that failed after it published the
    /// ref's pointer).
    view_appears_anyway: Mutex<bool>,
    /// A view this stand-in creates is closed at once (someone closed it before the creation looked).
    closes_what_it_creates: std::sync::atomic::AtomicBool,
    view_calls: Mutex<Vec<ViewCall>>,
    /// Where a view the stand-in creates appears (the in-memory repository).
    views_in: Mutex<Option<Arc<Fake>>>,
    /// Where the stand-in reads the lease holder's record (the service store).
    leases_in: Mutex<Option<Arc<MemoryStore>>>,
    /// After the next `create_view`, the repository cannot list its views.
    views_break_after: Mutex<bool>,
}

impl Double {
    pub(crate) fn new(ledger: Arc<StoreLedger>) -> Arc<Self> {
        Arc::new(Double {
            ledger,
            answers: Mutex::new(VecDeque::new()),
            calls: Mutex::new(Vec::new()),
            hook: Mutex::new(None),
            view_answers: Mutex::new(VecDeque::new()),
            view_appears_anyway: Mutex::new(false),
            closes_what_it_creates: std::sync::atomic::AtomicBool::new(false),
            view_calls: Mutex::new(Vec::new()),
            views_in: Mutex::new(None),
            leases_in: Mutex::new(None),
            views_break_after: Mutex::new(false),
        })
    }

    /// The next `create_view` answers `answer` (and, when it is `Ok`, the view appears in the repository).
    pub(crate) fn then_view(&self, answer: Result<(), AcceptError>) {
        self.view_answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(answer);
    }

    /// The next `create_view` answers `answer` and its view appears in the repository all the same: a `branch create`
    /// that failed after it published the ref's pointer lists the view and leaves the ref half published.
    pub(crate) fn then_view_listed_anyway(&self, answer: Result<(), AcceptError>) {
        self.then_view(answer);
        *self
            .view_appears_anyway
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
    }

    /// The views this stand-in creates are closed at once, as if somebody closed them before the creation looked.
    pub(crate) fn then_views_it_creates_are_closed(&self) {
        self.closes_what_it_creates
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// A view this stand-in creates appears in `fake`.
    pub(crate) fn creates_views_in(&self, fake: Arc<Fake>) {
        *self.views_in.lock().unwrap_or_else(PoisonError::into_inner) = Some(fake);
    }

    /// A `create_view` of this stand-in notes the purpose of the lease record it finds in `store`.
    pub(crate) fn sees_leases_in(&self, store: Arc<MemoryStore>) {
        *self
            .leases_in
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(store);
    }

    /// After the next `create_view`, the in-memory repository cannot list its views any more.
    pub(crate) fn then_views_cannot_be_listed(&self) {
        *self
            .views_break_after
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
    }

    pub(crate) fn view_calls(&self) -> Vec<ViewCall> {
        self.view_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The next call answers `answer`.
    pub(crate) fn then(&self, answer: Result<SealedChange, AcceptError>) {
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(answer);
    }

    /// Run `hook` at every call, before the answer.
    pub(crate) fn on_call(&self, hook: impl Fn(&Call) + Send + Sync + 'static) {
        *self.hook.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(hook));
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// A sealed change with these ids.
pub(crate) fn sealed(block: char) -> SealedChange {
    SealedChange {
        block_id: block.to_string().repeat(64),
        patch_ids: vec!["9".repeat(64)],
    }
}

impl Accept for Double {
    fn accept(
        &self,
        _wiki: &inklkunn_store::WikiId,
        view: &str,
        token: &AcceptanceToken,
        change: &PreparedChange,
    ) -> Result<SealedChange, AcceptError> {
        let call = Call {
            view: view.to_owned(),
            token: token.clone(),
            change: change.clone(),
            pending: self.ledger.pending(token).unwrap_or(None),
        };
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call.clone());
        if let Some(hook) = self
            .hook
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            hook(&call);
        }
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| Ok(sealed('f')))
    }

    fn create_view(
        &self,
        wiki: &inklkunn_store::WikiId,
        name: &str,
        from: &str,
        act: &ViewCreationAct,
    ) -> Result<(), AcceptError> {
        // The guard of the real signer: the act is open in the ledger and names this wiki, view and ref.
        let facts = self.ledger.view_creation(act).unwrap_or(None);
        self.view_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(ViewCall {
                name: name.to_owned(),
                from: from.to_owned(),
                act: act.clone(),
                facts: facts.clone(),
                lease_purpose: self
                    .leases_in
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .as_ref()
                    .and_then(|store| store.lease_holder(wiki).ok().flatten())
                    .map(|h| h.purpose),
            });
        match facts {
            Some(f) if f.wiki == *wiki && f.name == name && f.from == from => {}
            _ => return Err(AcceptError::NotPending),
        }
        let answer = self
            .view_answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or(Ok(()));
        let appears_anyway = std::mem::take(
            &mut *self
                .view_appears_anyway
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        if (appears_anyway || matches!(answer, Ok(()) | Err(AcceptError::SeedsNotRemoved { .. })))
            && let Some(fake) = self
                .views_in
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
        {
            if self
                .closes_what_it_creates
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                fake.add_closed_view(name);
            } else {
                fake.add_view(name);
            }
        }
        if *self
            .views_break_after
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            && let Some(fake) = self
                .views_in
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
        {
            fake.fail_views(Some(inklkunn_prikk::StorageError::Driver {
                verb: "branch",
                detail: "the repository cannot list its views".to_owned(),
            }));
        }
        answer
    }
}

/// Bases from the in-memory repository's tip.
struct FakeBase(Arc<Fake>);

impl BaseSource for FakeBase {
    fn base(&self, _: &WikiRecord, _: &str) -> Result<Base, BaseError> {
        Base::new(&self.0.state_id(), &self.0.block_id())
            .ok_or_else(|| BaseError::Unavailable("not ids".to_owned()))
    }

    fn page_paths(&self, _: &WikiRecord, base: &Base) -> Result<BTreeSet<String>, BaseError> {
        let tree = self
            .0
            .tree(
                std::path::Path::new(""),
                &Point::Block(base.block_id.clone()),
                Some("pages"),
            )
            .map_err(|e| BaseError::Unavailable(e.to_string()))?;
        Ok(tree.entries.into_iter().map(|e| e.path).collect())
    }

    fn page_text(
        &self,
        _: &WikiRecord,
        base: &Base,
        path: &str,
    ) -> Result<Option<String>, BaseError> {
        match self.0.read(
            std::path::Path::new(""),
            &Point::Block(base.block_id.clone()),
            path,
            u64::MAX,
        ) {
            Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
            Err(_) => Ok(None),
        }
    }
}

/// The page every rig starts with, and a few more.
pub(crate) const HOME: &str = "---\ntitle: Home\nid: p-home\n---\n\nOne.\n\nTwo.\n\nThree.\n";

/// A wiki and everything an acceptance needs of it.
pub(crate) struct Rig {
    pub(crate) _dir: TempDir,
    pub(crate) store: Arc<MemoryStore>,
    pub(crate) fake: Arc<Fake>,
    pub(crate) wiki: WikiRecord,
    pub(crate) layout: RootLayout,
    pub(crate) leases: Arc<WikiLeases>,
    pub(crate) authority: IdentityAuthority,
    pub(crate) proposals: Arc<ProposalService>,
    pub(crate) reads: Arc<ReadService>,
    pub(crate) ledger: Arc<StoreLedger>,
    pub(crate) double: Arc<Double>,
    pub(crate) audit: Arc<MemoryAudit>,
    pub(crate) notifier: Arc<Notifier>,
}

impl Rig {
    pub(crate) fn new(tag: &str) -> Rig {
        Rig::with(tag, LeaseConfig::default(), QuotaConfig::default())
    }

    pub(crate) fn with(tag: &str, lease: LeaseConfig, quotas: QuotaConfig) -> Rig {
        Rig::build(tag, lease, quotas, Visibility::Public)
    }

    /// A wiki of this visibility.
    pub(crate) fn of_visibility(tag: &str, visibility: Visibility) -> Rig {
        Rig::build(
            tag,
            LeaseConfig::default(),
            QuotaConfig::default(),
            visibility,
        )
    }

    fn build(tag: &str, lease: LeaseConfig, quotas: QuotaConfig, visibility: Visibility) -> Rig {
        let dir = TempDir::new(tag);
        let store = Arc::new(MemoryStore::new());
        let fake = Arc::new(Fake::new(&[("pages/Home.md", HOME)]));
        let mut new = NewWiki::new(
            Slug::new("accept-test").unwrap_or_else(|e| panic!("{e}")),
            DisplayName::new("Accept test").unwrap_or_else(|e| panic!("{e}")),
        );
        new.visibility = visibility;
        let wiki = store.create(new).unwrap_or_else(|e| panic!("{e}"));
        let layout = RootLayout::new(dir.0.join("root"));
        layout
            .prepare(&wiki.id)
            .unwrap_or_else(|e| panic!("the wiki's directories: {e}"));
        let signer_leases = inklkunn_signer::WriteLeases::new(layout.clone());
        let leases = WikiLeases::new(signer_leases, store.clone(), "test-process", lease);
        let audit = Arc::new(MemoryAudit::new());
        let authorizer = Authorizer::new(
            Arc::new(StoreAuthz::new(store.clone(), store.clone())),
            audit.clone(),
        );
        let proposals = Arc::new(ProposalService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            authorizer,
            Arc::new(FakeBase(fake.clone())),
            quotas,
        ));
        let notifier = Arc::new(Notifier::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            Authorizer::new(
                Arc::new(StoreAuthz::new(store.clone(), store.clone())),
                audit.clone(),
            ),
        ));
        let reads = Arc::new(ReadService::new(
            fake.clone(),
            store.clone(),
            store.clone(),
            layout.clone(),
            ReadConfig::default(),
            None,
        ));
        let ledger = Arc::new(StoreLedger::new(store.clone(), store.clone()));
        let double = Double::new(ledger.clone());
        double.creates_views_in(fake.clone());
        double.sees_leases_in(store.clone());
        Rig {
            _dir: dir,
            store,
            fake,
            wiki,
            layout,
            leases,
            authority: IdentityAuthority::for_tests(),
            proposals,
            reads,
            ledger,
            double,
            audit,
            notifier,
        }
    }

    /// An account with a role in the wiki (`None`: no role at all).
    pub(crate) fn person(&self, name: &str, role: Option<Role>) -> (AccountId, Principal) {
        let id = AccountId::new();
        self.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new(name).unwrap_or_else(|e| panic!("{e}")),
                local: None,
                created_at: 1,
                disabled: false,
            })
            .unwrap_or_else(|e| panic!("{e}"));
        if let Some(role) = role {
            self.store
                .grant(RoleGrant {
                    account: id.clone(),
                    wiki: self.wiki.id.clone(),
                    role,
                    granted_by: None,
                    granted_at: 1,
                })
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let principal = Principal::session(&self.authority, id.clone());
        (id, principal)
    }

    /// The acceptor over the stand-in signer, with the reconciliation off (the in-memory repository has no
    /// patch messages for it to read).
    pub(crate) fn acceptor(&self) -> ProposalAcceptor {
        ProposalAcceptor::new(self.parts(self.double.clone()))
            .with_reconciliation(false)
            .with_notifier(self.notifier.clone())
    }

    pub(crate) fn parts(&self, signer: Arc<dyn Accept>) -> AcceptanceParts {
        AcceptanceParts {
            proposals: self.proposals.clone(),
            acceptances: self.store.clone(),
            acts: self.store.clone(),
            roles: self.store.clone(),
            accounts: self.store.clone(),
            storage: self.fake.clone(),
            layout: self.layout.clone(),
            signer,
            leases: self.leases.clone(),
            reads: self.reads.clone(),
        }
    }

    /// The service's own path over a stand-in signer.
    pub(crate) fn system_acts(&self, signer: Arc<dyn Accept>) -> SystemActs {
        let authorizer = Authorizer::new(
            Arc::new(StoreAuthz::new(self.store.clone(), self.store.clone())),
            self.audit.clone(),
        );
        SystemActs::new(
            authorizer,
            self.store.clone(),
            self.store.clone(),
            self.fake.clone(),
            self.layout.clone(),
            signer,
            self.leases.clone(),
        )
        .with_reconciliation(false)
    }

    /// A submitted proposal by `author` that changes these pages (`None`: removes it), against the tip now.
    pub(crate) fn propose(
        &self,
        author: &Principal,
        summary: &str,
        files: &[(&str, Option<&str>)],
    ) -> ProposalRecord {
        let edit = ProposalEdit {
            summary: summary.to_owned(),
            files: files
                .iter()
                .map(|(path, text)| FileChange {
                    path: (*path).to_owned(),
                    text: text.map(str::to_owned),
                })
                .collect(),
            ..ProposalEdit::default()
        };
        let draft = self
            .proposals
            .create(author, &self.wiki, None, edit)
            .unwrap_or_else(|e| panic!("create: {e}"));
        self.proposals
            .submit(author, &self.wiki, &draft.id)
            .unwrap_or_else(|e| panic!("submit: {e}"))
    }

    /// A held proposal whose records are written **without the service's judgement of its knowledge files**
    /// (K7 refuses at submit what the acceptance would always refuse): what the acceptance's own gate holds
    /// against when a record reaches it some other way (an older proposal, a store written by hand).
    pub(crate) fn propose_unjudged(
        &self,
        author: &Principal,
        summary: &str,
        files: &[(&str, Option<&str>)],
    ) -> ProposalRecord {
        use inklkunn_store::{
            ProposalChange, ProposalOp, ProposalState, ProposalTransition, RevisionContent,
            RevisionRecord,
        };
        let Principal::Account(person) = author else {
            panic!("a person is needed to propose");
        };
        let account = person.id().clone();
        let base = Base::new(&self.fake.state_id(), &self.fake.block_id())
            .unwrap_or_else(|| panic!("a base"));
        let content = RevisionContent {
            base,
            files: files
                .iter()
                .map(|(path, text)| FileChange {
                    path: (*path).to_owned(),
                    text: text.map(str::to_owned),
                })
                .collect(),
            renames: Vec::new(),
            minor: false,
            automated: false,
        };
        let now = 1_000;
        let record = ProposalRecord {
            id: ProposalId::new(),
            wiki: self.wiki.id.clone(),
            view: self.wiki.default_view.clone(),
            state: ProposalState::Draft,
            author: account.clone(),
            summary: summary.to_owned(),
            sources: Vec::new(),
            superseded_by: None,
            created_at: now,
            updated_at: now,
            submitted_at: None,
            closed_at: None,
        };
        let step = |op, from, to| ProposalTransition {
            proposal: record.id.clone(),
            op,
            from,
            to,
            actor: account.clone(),
            reason: None,
            at: now,
        };
        self.store
            .create_proposal(
                &record,
                &content,
                &step(ProposalOp::Create, None, ProposalState::Draft),
            )
            .unwrap_or_else(|e| panic!("create: {e}"));
        self.store
            .change_proposal(&ProposalChange {
                transition: step(
                    ProposalOp::Submit,
                    Some(ProposalState::Draft),
                    ProposalState::HeldForReview,
                ),
                text: None,
                revision: Some(RevisionRecord {
                    proposal: record.id.clone(),
                    seq: 1,
                    content,
                    created_at: now,
                }),
                superseded_by: None,
            })
            .unwrap_or_else(|e| panic!("submit: {e}"))
    }

    pub(crate) fn state(&self, id: &ProposalId) -> inklkunn_store::ProposalState {
        self.store
            .proposal(id, None)
            .ok()
            .flatten()
            .map(|p| p.state)
            .unwrap_or_else(|| panic!("no such proposal"))
    }
}

/// A lease config that gives up quickly.
pub(crate) fn quick(wait_ms: u64) -> LeaseConfig {
    LeaseConfig {
        max_wait: Duration::from_millis(wait_ms),
        poll: Duration::from_millis(5),
        ..LeaseConfig::default()
    }
}
