//! The stand-ins of the views' tests: a signer that makes a branch in the in-memory repository only when the
//! ledger's record says to (as the real signer's guard does), and an acceptor that fails as it is told.
#![allow(clippy::expect_used, clippy::unwrap_used, dead_code)]

use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_core::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceParts, AcceptanceToken, LeaseConfig,
    PreparedChange, Principal, ProposalAcceptor, SealedChange, StoreLedger, ViewCreated,
    ViewCreationAct, ViewFailure, WikiLeases, WriteLeases,
};
use inklkunn_store::{ProposalId, RootLayout, ViewKind, WikiId, WikiRecord};
use inklkunn_web::services::Acceptor;

use super::common::{Fake, World};

/// One call to the stand-in signer's `create_view`.
#[derive(Debug, Clone)]
pub struct Call {
    pub name: String,
    pub from: String,
    /// Did the ledger say the act was open and named exactly this wiki, view and ref?
    pub guarded: bool,
}

/// A signer that creates a view in the in-memory repository **if the record the ledger holds says to**.
pub struct Brancher {
    ledger: Arc<StoreLedger>,
    fake: Arc<Fake>,
    calls: Mutex<Vec<Call>>,
}

impl Brancher {
    pub fn calls(&self) -> Vec<Call> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Accept for Brancher {
    fn accept(
        &self,
        _: &WikiId,
        _: &str,
        _: &AcceptanceToken,
        _: &PreparedChange,
    ) -> Result<SealedChange, AcceptError> {
        Err(AcceptError::NotPending)
    }

    fn create_view(
        &self,
        wiki: &WikiId,
        name: &str,
        from: &str,
        act: &ViewCreationAct,
    ) -> Result<(), AcceptError> {
        let facts = self.ledger.view_creation(act).unwrap_or(None);
        let guarded = facts
            .as_ref()
            .is_some_and(|f| f.wiki == *wiki && f.name == name && f.from == from);
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Call {
                name: name.to_owned(),
                from: from.to_owned(),
                guarded,
            });
        if !guarded {
            return Err(AcceptError::NotPending);
        }
        self.fake
            .add_view(name.strip_prefix("heads/").unwrap_or(name));
        Ok(())
    }
}

/// The world with the real acceptor over the stand-in signer.
pub fn attach(mut w: World) -> (World, Arc<Brancher>) {
    let dir = std::env::temp_dir().join(format!(
        "inklkunn-views-{}-{}",
        std::process::id(),
        WikiId::new()
    ));
    let layout = RootLayout::new(dir.join("root"));
    layout.prepare(&w.wiki.id).unwrap();
    let leases = WikiLeases::new(
        WriteLeases::new(layout.clone()),
        w.store.clone(),
        "test-process",
        LeaseConfig::default(),
    );
    let ledger = Arc::new(StoreLedger::new(w.store.clone(), w.store.clone()));
    let signer = Arc::new(Brancher {
        ledger,
        fake: w.fake.clone(),
        calls: Mutex::new(Vec::new()),
    });
    let acceptor = ProposalAcceptor::new(AcceptanceParts {
        proposals: w.services.proposals.clone(),
        acceptances: w.store.clone(),
        acts: w.store.clone(),
        roles: w.store.clone(),
        accounts: w.store.clone(),
        storage: w.fake.clone(),
        layout,
        signer: signer.clone(),
        leases,
        reads: w.services.read.clone(),
    })
    .with_reconciliation(false);
    w.services = Arc::new(inklkunn_web::Services {
        acceptor: Some(Arc::new(acceptor)),
        ..(*w.services).clone()
    });
    (w, signer)
}

/// An acceptor that creates no view: it fails as it is told, whatever it is asked.
struct Failing(ViewFailure);

impl Acceptor for Failing {
    fn accept(
        &self,
        _: &Principal,
        _: &WikiRecord,
        _: &ProposalId,
        _: u32,
    ) -> Result<inklkunn_core::Accepted, inklkunn_core::AcceptFailure> {
        Err(inklkunn_core::AcceptFailure::NotAllowed)
    }

    fn create_view(
        &self,
        _: &Principal,
        _: &WikiRecord,
        _: &str,
        _: Option<&str>,
        _: ViewKind,
    ) -> Result<ViewCreated, ViewFailure> {
        Err(self.0.clone())
    }
}

/// The world's acceptor, replaced by one that fails with `failure`.
pub fn attach_stub(w: &mut World, failure: ViewFailure) {
    w.services = Arc::new(inklkunn_web::Services {
        acceptor: Some(Arc::new(Failing(failure))),
        ..(*w.services).clone()
    });
}

/// An act store that keeps no record of a view's creation (K3: neither of the real stores is one any more): the older
/// methods go to the store it wraps, the view-act ones are the trait's defaults.
pub struct KeepsNoViewActs(pub Arc<dyn inklkunn_store::ActStore>);

type R<T> = inklkunn_store::Result<T>;

impl inklkunn_store::ActStore for KeepsNoViewActs {
    fn recovery_flag(&self, w: &WikiId) -> R<Option<inklkunn_store::RecoveryFlag>> {
        self.0.recovery_flag(w)
    }
    fn set_recovery_flag(&self, f: &inklkunn_store::RecoveryFlag) -> R<bool> {
        self.0.set_recovery_flag(f)
    }
    fn clear_recovery_flag(&self, w: &WikiId) -> R<bool> {
        self.0.clear_recovery_flag(w)
    }
    fn record_overlap(&self, o: &inklkunn_store::OverlapRecord) -> R<()> {
        self.0.record_overlap(o)
    }
    fn overlaps_of(&self, p: &ProposalId) -> R<Vec<inklkunn_store::OverlapRecord>> {
        self.0.overlaps_of(p)
    }
    fn begin_system_act(&self, a: &inklkunn_store::SystemActRecord) -> R<()> {
        self.0.begin_system_act(a)
    }
    fn finish_system_act(&self, e: &inklkunn_store::SystemActEnd) -> R<()> {
        self.0.finish_system_act(e)
    }
    fn open_system_act(
        &self,
        i: &inklkunn_store::record::SystemActId,
    ) -> R<Option<inklkunn_store::SystemActRecord>> {
        self.0.open_system_act(i)
    }
    fn system_acts(&self, w: &WikiId) -> R<Vec<inklkunn_store::SystemActEntry>> {
        self.0.system_acts(w)
    }
    fn insert_wiki_with_creation_act(
        &self,
        r: WikiRecord,
        b: &inklkunn_store::SystemActRecord,
        e: &inklkunn_store::SystemActEnd,
    ) -> R<WikiRecord> {
        self.0.insert_wiki_with_creation_act(r, b, e)
    }
    fn begin_recovery_act(&self, a: &inklkunn_store::RecoveryActRecord) -> R<()> {
        self.0.begin_recovery_act(a)
    }
    fn finish_recovery_act(&self, e: &inklkunn_store::RecoveryActEnd) -> R<()> {
        self.0.finish_recovery_act(e)
    }
    fn open_recovery_act(
        &self,
        i: &inklkunn_store::record::RecoveryActId,
    ) -> R<Option<inklkunn_store::RecoveryActRecord>> {
        self.0.open_recovery_act(i)
    }
    fn recovery_acts(&self, w: &WikiId) -> R<Vec<inklkunn_store::RecoveryActEntry>> {
        self.0.recovery_acts(w)
    }
}
