//! The server's acceptor (handoff `serve-sign-in-and-accept` U5; decision D1 of the review of handoff 004): the
//! pieces that take a proposal into the history, assembled the way the command line assembles the same things
//! (`admin::recovery_for`, `wiki import-folder`), so that **one signer design serves both and nothing is
//! written twice**.
//!
//! **Keys in the process, seeds per call** (RFC 002 D-3, D-4; threat model RR-1). The serving process opens the
//! key store: the key directory and the operator's secret file from the settings, the seeds decrypted for one
//! signing call onto the tmpfs and removed with the call, bubblewrap required for the signer's prikk calls
//! exactly as it is for every other prikk call of the server (`Settings::sandbox`). There is no second signer
//! process (IQ-4 stays). The secret is read from its file **at start**, not once: the sweep of leftover seed
//! directories, the start-up recovery and this acceptor each open the signer and read it (up to three times), and
//! every copy is zeroized when its signer is dropped. **It is never asked for on a terminal, never put in a
//! variable, a log line or a page.**
//!
//! **One lease with the command line.** The signer is opened `with_leases` over the [`WriteLeases`] that the
//! acceptor's [`WikiLeases`] wrap, as the command line's acts are: an acceptance holds the wiki's `flock` for
//! all of its steps, so the command line beside a running server is told *busy* naming the process, and a
//! server is told the same when the command line holds the wiki.
//!
//! **A server without the means to sign still serves.** No secret file, or a key store that does not open
//! (a wrong secret, a key directory open to others, a seed directory that is not on a tmpfs, no keys made yet):
//! the server reads, **stores proposals and comments as usual** and refuses **every change to the history** (an
//! acceptance) with the one sentence of [`NoSigner`]; the operator is told why in one line at start. It does not stop, because everything it can still do is useful, and it does not retry
//! behind a visitor's back: fix the cause and restart.

use std::path::Path;
use std::sync::Arc;

use inklkunn_core::proposal::{ProposalService, QuotaConfig};
use inklkunn_core::{
    AcceptanceParts, Authorizer, LeaseConfig, Notifier, ProposalAcceptor, ReadService, Signer,
    SignerConfig, StoreAuthz, StoreLedger, Verifier, WikiLeases, WriteLeases,
};
use inklkunn_prikk::Storage;
use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_store::{AuditSink, RootLayout, SqliteStore};
use inklkunn_web::{Acceptor, NoSigner};

use super::follow_up::FollowUp;
use crate::config::Settings;
use crate::keys::make_private_directory;

/// What the server's other services are, for the acceptor to lean on.
pub(crate) struct Parts {
    /// The deployment's settings.
    pub settings: Settings,
    /// How prikk is confined: the one the server already checked.
    pub sandbox: Sandbox,
    /// The wikis' root.
    pub layout: RootLayout,
    /// The service store.
    pub store: Arc<SqliteStore>,
    /// prikk, read-only.
    pub storage: Arc<dyn Storage>,
    /// The read service the pages use: its caches are forgotten after a seal.
    pub read: Arc<ReadService>,
    /// The proposal service the pages use.
    pub proposals: Arc<ProposalService>,
    /// The verifier the pages use: the follow-up of a seal.
    pub verifier: Arc<Verifier>,
    /// How the lease queues and waits.
    pub lease: LeaseConfig,
    /// Where an operator's line goes.
    pub log: Arc<dyn Fn(&str) + Send + Sync>,
}

/// What was built.
pub(crate) struct Assembled {
    /// What `Services::acceptor` is: the acceptor, or [`NoSigner`].
    pub acceptor: Arc<dyn Acceptor>,
    /// Whether the server can sign.
    pub signing: bool,
    /// The lines the operator reads at start: why it cannot sign, or what the signer found.
    pub notes: Vec<String>,
}

/// The lease's default for the server: the queue bounded as the quotas say (RFC 004 D-11).
pub(crate) fn lease_config() -> LeaseConfig {
    LeaseConfig::bounded_by(&QuotaConfig::default())
}

fn off(why: String) -> Assembled {
    Assembled {
        acceptor: Arc::new(NoSigner),
        signing: false,
        notes: vec![format!(
            "inklkunn: {why}; this server reads pages and stores proposals and comments, and refuses every change to the history until that is fixed and the server is restarted"
        )],
    }
}

/// What is wrong with the secret file's mode and owner, as warnings (corrections A2): readable or writable by
/// group or others, or owned by a user who is neither the server's nor root. **A warning only**: a Kubernetes
/// secret mount is 0644 by default and an operator may have reasons, but anyone who can read the file and the key
/// records holds every wiki's keys, and the operator should hear it at every start.
pub(crate) fn secret_file_warnings(path: &Path, mode: u32, owner: u32, server: u32) -> Vec<String> {
    let mut out = Vec::new();
    if mode & 0o066 != 0 {
        out.push(format!(
            "inklkunn: warning: the operator's secret file {} (INKLKUNN_SECRET_FILE) can be read or changed by group or \
             others (mode {:04o}); anyone who can read it and the key records holds every wiki's keys, so make it \
             owned by the server's user with mode 0400 or 0600",
            path.display(),
            mode & 0o777
        ));
    }
    if owner != server && owner != 0 {
        out.push(format!(
            "inklkunn: warning: the operator's secret file {} (INKLKUNN_SECRET_FILE) is owned by user {owner}, who is \
             neither the server's user ({server}) nor root, and so can change it; make it owned by the server's user",
            path.display()
        ));
    }
    out
}

/// The warnings about the file at `path`, read through any link (secret mounts are often links); none when it
/// cannot be looked at (the signer says why it cannot be opened).
fn secret_file_check(path: &Path) -> Vec<String> {
    use std::os::unix::fs::MetadataExt;
    match std::fs::metadata(path) {
        Ok(meta) => secret_file_warnings(
            path,
            meta.mode(),
            meta.uid(),
            rustix::process::getuid().as_raw(),
        ),
        Err(_) => Vec::new(),
    }
}

/// Is there a file at the secret's path? Looking at it reads nothing of it.
fn secret_file_is_missing(path: &Path) -> bool {
    matches!(std::fs::metadata(path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

/// Open the signer and build the acceptor, or say why not. **Run after the start-up work** (the sweep of unfinished
/// creations and recovery, `admin::start_up`): an acceptor must never be built on a wiki that start-up has not
/// looked at, and a wiki that start-up flagged *needs recovery* refuses writes whatever the acceptor is.
pub(crate) fn assemble(parts: Parts) -> Assembled {
    let warnings = secret_file_check(&parts.settings.secret_file);
    let mut assembled = assemble_signer(parts);
    assembled.notes.splice(0..0, warnings);
    assembled
}

fn assemble_signer(parts: Parts) -> Assembled {
    let settings = &parts.settings;
    if secret_file_is_missing(&settings.secret_file) {
        return off(format!(
            "the operator's secret file {} (INKLKUNN_SECRET_FILE) does not exist",
            settings.secret_file.display()
        ));
    }
    if let Err(e) = make_private_directory(&settings.seed_dir) {
        return off(format!(
            "the seed directory {} (INKLKUNN_SEED_DIR) could not be made private ({})",
            settings.seed_dir.display(),
            e.kind()
        ));
    }
    let write_leases = WriteLeases::new(parts.layout.clone());
    let ledger = Arc::new(StoreLedger::new(parts.store.clone(), parts.store.clone()));
    let config = SignerConfig::new(
        settings.prikk.clone(),
        parts.sandbox.clone(),
        parts.layout.clone(),
        &settings.key_dir,
        &settings.secret_file,
        &settings.seed_dir,
    );
    let signer = match Signer::open(config, ledger) {
        Ok(signer) => signer.with_leases(&write_leases),
        Err(e) => return off(format!("the signer could not be opened ({e})")),
    };
    let signer = Arc::new(signer);
    // The verification page says when a failed check is made of what a key retirement recorded (P2′); the badge is
    // not touched.
    parts.verifier.explain_retirements_with(signer.clone());
    let mut notes = Vec::new();
    let swept = signer.leftover_call_directories_removed();
    if swept > 0 {
        notes.push(format!(
            "inklkunn: removed {swept} leftover key directories of a call that was killed; look at how the service stopped"
        ));
    }
    let leases = WikiLeases::new(
        write_leases,
        parts.store.clone(),
        format!("inklkunn-serve-{}", std::process::id()),
        parts.lease,
    );
    let store = &parts.store;
    let authorizer = Authorizer::new(
        Arc::new(StoreAuthz::new(store.clone(), store.clone())),
        store.clone() as Arc<dyn AuditSink>,
    );
    let notifier = Arc::new(Notifier::new(
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
        authorizer,
    ));
    let follow_up = Arc::new(FollowUp::new(
        parts.read.clone(),
        parts.verifier.clone(),
        parts.log.clone(),
    ));
    let acceptor = ProposalAcceptor::new(AcceptanceParts {
        proposals: parts.proposals.clone(),
        acceptances: store.clone(),
        acts: store.clone(),
        roles: store.clone(),
        accounts: store.clone(),
        storage: parts.storage.clone(),
        layout: parts.layout.clone(),
        signer,
        leases,
        reads: parts.read.clone(),
    })
    .with_notifier(notifier)
    .with_audit(store.clone() as Arc<dyn AuditSink>)
    .with_reconciliation(true)
    .with_follow_up(follow_up);
    Assembled {
        acceptor: Arc::new(acceptor),
        signing: true,
        notes,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod secret_file_tests;
