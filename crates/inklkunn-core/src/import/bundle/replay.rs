//! `import-bundle`'s replay (RFC 007 D-1 – D-3, D-6; handoff I-T3): the plan's wiki is made, **with its history**, and appears only when it
//! is complete.
//!
//! **The import is *create wiki* extended.** The creation's own steps run first and are shared with it ([`crate::wiki`]): the
//! creation mark is held from before the first directory, the repository and its keys are made, the first block is sealed. Then
//! the history is replayed, one change a block, through the service's own path ([`SystemActs`]: the system principal, the wiki's
//! write lease, the record of the act, the signer, the keys of the wiki), and the record is stored **last, in the same transaction
//! as every act** ([`inklkunn_store::ActStore::insert_wiki_with_acts`]), and only then the mark is removed. Until that transaction
//! nothing can name the wiki: no reader, no list, no link, and a request answers as it answers for a wiki that does not exist,
//! because for the store it does not.
//!
//! **What needed the record earlier, and the smallest way.** [`SystemActs`] and the signer's guard read the wiki's record, the act's
//! record and the lease from the *store*, and every one of them refuses a wiki the store does not have. The replay therefore runs them
//! over a **staging store held in this process's memory** ([`ImportStaging`]: the same code over a store that nothing else reads),
//! which holds the record, the lease and the acts while the history is made. The real store gets nothing until the end; then the
//! record and **all** the acts go in as one transaction. A kill leaves the staging store with the process, and on disk what a
//! killed creation leaves, which the sweep reports and `wiki discard-unfinished` removes. No new way into a history exists: every block is
//! written by `Accept::accept` behind the acceptance guard, bound to an act that was recorded first.
//!
//! **What is replayed**, in the plan's order ([`BundlePlan::schedule`]): first, in one change, the files with no history (the pages of
//! a level 2 bundle, the attachments with their sidecars, and the bundle's interwiki map as the plan judged it,
//! `governance/interwiki.yaml`: the one path below `governance/` an import may write, RFC 007 D-8; a bundle whose pages all have a
//! history gets a first change of the map alone); then one change a revision (a text writes the page,
//! a rename moves it, a removal removes it), each with the source's facts as typed trailers ([`SourceRevision`]) and the bundle's
//! notice ([`ImportNotice`]); last, the change that brings the pages to exactly the bundle's files, if any page differs. **A page the
//! replay did not have** (its text was hidden at the source, say) **is not moved or removed** (there is nothing to move): the report
//! counts it. **Last, the claim and source files** ([`BundlePlan::knowledge_changes`]): after the history and the change at the end, so that
//! every page a claim anchors to is there, each file after the files it names, in as many changes as the caps of one acceptance need,
//! and judged exactly as an acceptance judges a change that is not a curation proposal. **They arrive unsettled** (Z-1,
//! [`crate::knowledge::unsettle`]): what only an accepted curation proposal sets, a community status, a review day and an availability,
//! is not written (an import is the service writing, never a curation), the summary of the change and the report say how many, and a
//! file that had any of them is written as its canonical text without them; **a retired claim or source is not written at all** (Z-6:
//! *retiring is a curation here*): the plan leaves it out of the files and out of the changes, the replay reads it again, checks its
//! digest and counts it by the same step (the count must be the plan's), and the summary of the change of its family says how many were
//! left out (that of the other family's change, or the report alone, when its own family has no file to write). A file that names one is
//! refused at the plan, by name, so the replay never meets it.
//!
//! **The bundle is read again, and checked again.** The plan keeps no text. A history file is read from the directories the plan
//! walked, through the inode it recorded, and must be the file it judged (the size and the digest); a page of the bundle, an attachment, a sidecar and a
//! knowledge file are read the same way and must each be the digest the plan took (V-2). A file that changed since the plan is a failed import, and nothing of it remains.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use inklkunn_markup::path::path_to_title;
use inklkunn_prikk::Storage;
use inklkunn_signer::accept::{Accept, PreparedFile};
use inklkunn_signer::message::{
    ChecksState, ImportNotice, ImportSource, LicenseWord, SourceAuthorKind, SourceRevision,
    SystemLabel,
};
use inklkunn_signer::{KeyAuthority, MAX_CHANGE_BYTES, MAX_CHANGE_FILES, Signer};
use inklkunn_store::{
    AcceptanceStore, ActStore, AuditSink, MemoryStore, NewWiki, RootLayout, SystemActEnd,
    SystemActKind, SystemActRecord, Verification, VerificationState, WikiId, WikiRecord, WikiStore,
};
use sha2::{Digest as _, Sha256};

use super::history::{Record, Walker};
use super::report::{Checksums, LicenseFrom};
use super::{BundlePlan, HistorySource, PageHistory, Part, Revision, Step};
use crate::acceptance::{StoreLedger, SystemActFailure, SystemActs, SystemRequest};
use crate::authorize::{Authorizer, StoreAuthz};
use crate::import::{Digest, PlannedFile, open_chain, read_regular, read_some, sha256};
use crate::knowledge::Unsettled;
use crate::lease::{LeaseConfig, WikiLeases};
use crate::read::{ReadConfig, ReadService};
use crate::wiki::{
    Cleanup, CreateWikiError, CreationStep, begin_creation, creation_act, discard, settle,
    verify_repository,
};

/// The steps of an import, each named **after it completed**, for a test that stops the process there (as [`CreationStep`] does for a
/// creation: a panic runs no cleanup, as `SIGKILL` runs none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportStep {
    /// A step of the creation the import begins with.
    Creation(CreationStep),
    /// The staging store holds the record and the creation's act.
    Staged,
    /// The first change is sealed (the files with no history).
    FirstChange,
    /// This many revisions are replayed (each is sealed and recorded in the staging store).
    Revision(usize),
    /// The change that brings the pages to the bundle's files is sealed.
    FinalState,
    /// The claim and source files are sealed (one change, or as many as the caps of one acceptance need): the last change of the history.
    Knowledge,
    /// The whole repository verified.
    Verified,
    /// The report is kept beside the repository.
    ReportKept,
    /// The record and every act are stored, in one transaction. The wiki is complete from here and only the mark remains.
    Stored,
}

/// The records the replay's own acts are kept in until the wiki exists: **a store in this process's memory, which nothing else
/// reads** (see the module).
#[derive(Debug, Default)]
pub struct ImportStaging {
    store: Arc<MemoryStore>,
}

impl ImportStaging {
    /// An empty staging store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The ledger the signer that writes the history is opened over (`Signer::open_creating_store`): it reads the acts from here.
    pub fn ledger(&self) -> Arc<StoreLedger> {
        let acceptances: Arc<dyn AcceptanceStore> = self.store.clone();
        let acts: Arc<dyn ActStore> = self.store.clone();
        Arc::new(StoreLedger::new(acceptances, acts))
    }

    /// The write leases of the signer, recorded here (`signer`'s own, so that the command line and a running server take turns).
    pub fn leases(
        &self,
        write_leases: Arc<inklkunn_signer::WriteLeases>,
        holder: &str,
    ) -> Arc<WikiLeases> {
        WikiLeases::new(
            write_leases,
            self.store.clone(),
            holder.to_owned(),
            LeaseConfig::default(),
        )
    }

    /// A read service over the staging store, to judge the knowledge files of the import as an acceptance judges them.
    pub fn reads(
        &self,
        storage: Arc<dyn Storage>,
        layout: RootLayout,
        config: ReadConfig,
    ) -> Arc<ReadService> {
        Arc::new(ReadService::new(
            storage,
            self.store.clone(),
            self.store.clone(),
            layout,
            config,
            None,
        ))
    }
}

/// What an import is run with.
pub struct ImportWorld<'a> {
    /// The wiki store the wiki is put in at the end (and asked first whether the slug is taken).
    pub store: &'a dyn WikiStore,
    /// The act store of the same store: it takes the record and every act in one transaction.
    pub acts: &'a dyn ActStore,
    /// The root layout.
    pub layout: &'a RootLayout,
    /// The signer, opened over the staging store's ledger ([`ImportStaging::ledger`]) and the leases' locks.
    pub signer: &'a Arc<Signer>,
    /// The repository driver.
    pub storage: &'a Arc<dyn Storage>,
    /// The staging store.
    pub staging: &'a ImportStaging,
    /// The leases over the staging store ([`ImportStaging::leases`]).
    pub leases: Arc<WikiLeases>,
    /// Where the system principal's decisions are recorded (the real audit trail).
    pub audit: Arc<dyn AuditSink>,
    /// A read service over the staging store, for the knowledge files of the import.
    pub reads: Option<Arc<ReadService>>,
    /// The process's authority to make a wiki's keys.
    pub authority: &'a KeyAuthority,
    /// This build's version, for the wiki's manifest.
    pub version: &'a str,
    /// The time, in Unix seconds.
    pub clock: fn() -> i64,
}

/// What an import made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedWiki {
    /// The wiki's record, stored.
    pub record: WikiRecord,
    /// The repository directory.
    pub repo: PathBuf,
    /// The wiki's first block (its creation).
    pub creation_block: String,
    /// The tip: the last block of the import.
    pub tip: String,
    /// Whether there was a first change.
    pub first_change: bool,
    /// How many revisions were replayed, one block each.
    pub revisions: usize,
    /// How many renames and removals named a page the replay did not have (its text was hidden at the source, or the source never gave
    /// it), and so moved and removed nothing.
    pub not_applied: usize,
    /// How many changes at the end brought the pages to the bundle's files.
    pub final_changes: usize,
    /// How many pages the change at the end wrote, and how many it removed.
    pub final_written: usize,
    /// See [`ImportedWiki::final_written`].
    pub final_removed: usize,
    /// How many claim and source files were written (after the history and the change at the end).
    pub knowledge_files: usize,
    /// In how many changes the claim and source files were written.
    pub knowledge_changes: usize,
    /// What those files had that only a curation sets, and was not written (Z-1).
    pub knowledge_not_imported: Unsettled,
}

/// Why an import did not make a wiki. **No variant leaves a record behind**; what remains on disk is said.
#[derive(Debug)]
pub enum ImportBundleError {
    /// The plan has refusals: nothing was made.
    Refused(usize),
    /// The creation the import begins with failed (it cleaned up after itself, as a creation does).
    Creation(CreateWikiError),
    /// A later step failed, and what became of what the attempt made.
    Failed {
        /// Where: a short statement of the step.
        at: String,
        /// Why.
        why: String,
        /// What became of the directories and the keys.
        cleanup: Cleanup,
    },
}

impl std::fmt::Display for ImportBundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportBundleError::Refused(n) => write!(
                f,
                "{n} entr{} refused, so none of the bundle was imported; nothing was made",
                if *n == 1 { "y is" } else { "ies are" }
            ),
            ImportBundleError::Creation(e) => write!(f, "{e}"),
            ImportBundleError::Failed { at, why, cleanup } => write!(
                f,
                "the import failed at {at}: {why}; the wiki was not made, and {cleanup}"
            ),
        }
    }
}

impl std::error::Error for ImportBundleError {}

/// What a path of a page is called in a summary: its title, bounded by the summary itself.
fn title_of(path: &str) -> String {
    path_to_title(path).unwrap_or_else(|| path.to_owned())
}

/// The kind of author the source says, and the name to show for them (the record's, else the `users.yaml` entry's, else the id).
fn author_of(
    revision: &Revision,
    users: &BTreeMap<String, (String, String)>,
) -> Option<(SourceAuthorKind, String)> {
    let a = revision.author.as_ref()?;
    let listed = a.id.as_ref().and_then(|id| users.get(id));
    let name = a
        .name
        .clone()
        .or_else(|| a.label.clone())
        .or_else(|| listed.map(|(n, _)| n.clone()))
        .or_else(|| a.id.clone())?;
    let kind = match a.kind.as_deref().or(listed.map(|(_, k)| k.as_str())) {
        Some("person") => SourceAuthorKind::Person,
        Some("bot") => SourceAuthorKind::Bot,
        Some("group") => SourceAuthorKind::Group,
        Some("anonymous") => SourceAuthorKind::Anonymous,
        _ if a.anonymous => SourceAuthorKind::Anonymous,
        _ => SourceAuthorKind::Unspecified,
    };
    Some((kind, name))
}

/// What the source said of a revision, as the typed facts its change's message carries. **A name is text**: nothing here is an
/// account.
fn source_of(
    revision: &Revision,
    users: &BTreeMap<String, (String, String)>,
) -> Result<SourceRevision, String> {
    let mut out = SourceRevision::new(&revision.rev, revision.at.secs)
        .map_err(|e| format!("a revision's facts could not be described: {e}"))?;
    if revision.hidden.contains(&Part::Author) {
        out = out.with_author_hidden();
    } else if let Some((kind, name)) = author_of(revision, users) {
        out = out.with_author(kind, &name);
    }
    if revision.hidden.contains(&Part::Summary) {
        out = out.with_summary_hidden();
    } else if let Some(summary) = &revision.summary {
        out = out.with_summary(summary);
    }
    Ok(out.minor(revision.minor).automated(revision.automated))
}

/// One history file read again, with the walk of its text.
struct Cursor {
    records: Vec<Record>,
    walker: Walker,
    next: usize,
}

/// Read `history`'s file again from the directories the plan walked, and take it only if it is the file the plan judged.
fn load(plan: &BundlePlan, history: &PageHistory) -> Result<Cursor, String> {
    let HistorySource {
        path,
        identity,
        size,
        digest,
    } = &history.source;
    let (dir, name) = open_chain(&plan.plan.root, &plan.plan.dirs, path)
        .map_err(|why| format!("`{}` {why}", super::shown(&history.file)))?;
    let bytes = read_regular(&dir, &name, *identity, *size).map_err(|why| {
        format!(
            "`{}` is not the file that was planned (it changed since the plan: it {why})",
            super::shown(&history.file)
        )
    })?;
    if bytes.len() as u64 != *size || sha256(&bytes) != *digest {
        return Err(format!(
            "`{}` is not the file that was planned (it changed since the plan)",
            super::shown(&history.file)
        ));
    }
    let records = super::history::read(&bytes)
        .map_err(|fault| format!("`{}` {}", super::shown(&history.file), fault.rule))?;
    Ok(Cursor {
        records,
        walker: Walker::new(),
        next: 0,
    })
}

/// The text of record `at`, going through the records before it.
fn text_at(cursor: &mut Cursor, at: usize, page_cap: u64) -> Result<Option<String>, String> {
    let mut text = None;
    while cursor.next <= at {
        let record = cursor
            .records
            .get(cursor.next)
            .ok_or_else(|| "the file has fewer records than the plan judged".to_owned())?;
        let stepped = cursor
            .walker
            .step(record, page_cap)
            .map_err(|rule| format!("a record {rule}"))?;
        text = stepped.text;
        cursor.next += 1;
    }
    Ok(text)
}

/// The digest the plan took of the file at `path` (a page's, or any other file the import reads): what the replay's read of it is
/// checked against.
fn plan_digest<'a>(plan: &'a BundlePlan, path: &str) -> Option<&'a Digest> {
    plan.page_digests
        .get(path)
        .or_else(|| plan.file_digests.get(path))
}

fn hex(digest: &Digest) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The pages the wiki holds as the replay made them: by path, the digest of the text there.
type Live = BTreeMap<String, Digest>;

/// The run: the plan, the world, the observer and the state that goes from one change to the next.
struct Run {
    acts: SystemActs,
    wiki: WikiRecord,
    source: ImportSource,
    notice: ImportNotice,
    label: SystemLabel,
    live: Live,
    blocks: usize,
}

type Done = Result<String, String>;

impl Run {
    /// One change, through the service's own path. The block it made.
    fn change(
        &mut self,
        summary: &str,
        files: Vec<PreparedFile>,
        renames: Vec<(PathBuf, PathBuf)>,
        revision: Option<SourceRevision>,
    ) -> Done {
        let request = SystemRequest {
            kind: SystemActKind::Import,
            summary: summary.to_owned(),
            files,
            renames,
            imported_from: Some(self.source.clone()),
            source: revision,
            notice: Some(self.notice.clone()),
            planned_against: None,
        };
        let signed = self
            .acts
            .run(&self.wiki, &self.label, request)
            .map_err(|e: SystemActFailure| e.to_string())?;
        self.blocks += 1;
        Ok(signed.block_id)
    }
}

/// The path a page text is written at, with the text.
fn write_of(path: &str, text: String) -> PreparedFile {
    PreparedFile {
        path: PathBuf::from(path),
        content: Some(text.into_bytes()),
    }
}

/// Make the plan's wiki, with its history, and store it **last**. See the module.
///
/// `new` is the request as `wiki create` takes it (its `default_license` is the plan's licence). `observe` is called after each step
/// (a test stops the process there). A failure the process survives removes everything the attempt made.
pub fn import_bundle(
    world: &ImportWorld<'_>,
    new: NewWiki,
    plan: &BundlePlan,
    report_text: &str,
    observe: &dyn Fn(ImportStep),
) -> Result<ImportedWiki, ImportBundleError> {
    if !plan.is_importable() {
        return Err(ImportBundleError::Refused(plan.plan.refused.len()));
    }
    let started = begin_creation(
        world.store,
        world.layout,
        &**world.signer,
        new,
        world.version,
        world.authority,
        &|step| observe(ImportStep::Creation(step)),
    )
    .map_err(ImportBundleError::Creation)?;
    let crate::wiki::Started {
        record,
        id,
        mark,
        repo,
        sealed,
    } = started;
    let signer_bootstrap: &dyn inklkunn_signer::RepositoryBootstrap = &**world.signer;
    // From here, every failure takes the attempt away whole, and says why.
    macro_rules! abandon {
        ($at:expr, $why:expr) => {
            return Err(ImportBundleError::Failed {
                at: $at.to_string(),
                why: $why.to_string(),
                cleanup: settle(mark, discard(world.layout, signer_bootstrap, &id)),
            })
        };
    }

    // The staging store holds the record, and the creation as the first act.
    let creation_block = sealed.block_id.clone();
    let (creation_begin, creation_end) = creation_act(&record, &sealed, (world.clock)());
    let staging: &MemoryStore = &world.staging.store;
    if let Err(e) = WikiStore::insert(staging, record.clone()) {
        abandon!("staging the record", e);
    }
    if let Err(e) = ActStore::begin_system_act(staging, &creation_begin) {
        abandon!("staging the creation", e);
    }
    if let Err(e) = ActStore::finish_system_act(staging, &creation_end) {
        abandon!("staging the creation", e);
    }
    observe(ImportStep::Staged);

    // The service's own path, over the staging store.
    let authorizer = Authorizer::new(
        Arc::new(StoreAuthz::new(
            world.staging.store.clone(),
            world.staging.store.clone(),
        )),
        world.audit.clone(),
    );
    let accept: Arc<dyn Accept> = world.signer.clone();
    let mut acts = SystemActs::new(
        authorizer,
        world.staging.store.clone(),
        world.staging.store.clone(),
        world.storage.clone(),
        world.layout.clone(),
        accept,
        world.leases.clone(),
    );
    if let Some(reads) = &world.reads {
        acts = acts.with_reads(reads.clone());
    }
    let source = match ImportSource::new(&format!("bundle sha256:{}", hex(&plan.bundle_digest))) {
        Ok(s) => s,
        Err(e) => abandon!("describing the import", e),
    };
    let import_day = (world.clock)();
    let notice = {
        let checks = match plan.report.checksums {
            Checksums::Verified { .. } => ChecksState::Verified,
            _ => ChecksState::None,
        };
        let license = match plan.report.license_from {
            Some(LicenseFrom::DeclaredByAdmin) => LicenseWord::DeclaredByAdmin,
            _ => LicenseWord::Manifest,
        };
        let mut n = match ImportNotice::new(import_day, checks, license) {
            Ok(n) => n,
            Err(e) => abandon!("describing the import", e),
        };
        if let Some(name) = &plan.source_name {
            n = n.with_source_name(name);
        }
        if let Some(address) = &plan.source_address {
            n = n.with_source_address(address);
        }
        n
    };
    let mut run = Run {
        acts,
        wiki: record.clone(),
        source,
        notice,
        label: SystemLabel::inklkunn(),
        live: Live::new(),
        blocks: 0,
    };

    // The first change: what has no history (the knowledge files wait for the pages they anchor to).
    let first: Vec<&PlannedFile> = plan
        .plan
        .files
        .iter()
        .filter(|f| !f.is_knowledge())
        .filter(|f| !f.path.starts_with("pages/") || plan.first_pages.contains(&f.path))
        .collect();
    let mut first_change = false;
    // **The interwiki map, in the first change** (RFC 007 D-8): the plan's judged map in its canonical text, and nothing else of
    // `governance/` (the system act's guard allows an import this one file, and judges its bytes: [`SystemActs::run`]).
    let map_file = (!plan.interwiki.is_empty()).then(|| PreparedFile {
        path: PathBuf::from(crate::governance::INTERWIKI_PATH),
        content: Some(plan.interwiki.to_yaml().into_bytes()),
    });
    if !first.is_empty() || map_file.is_some() {
        let mut files = match read_some(&plan.plan, first.iter().copied()) {
            Ok(f) => f,
            Err(e) => abandon!("reading the files of the first change", e),
        };
        for file in &files {
            let path = file.path.to_string_lossy().into_owned();
            if let Some(bytes) = &file.content {
                // **Every file read is the file the plan read** (V-2): a page, an attachment, a sidecar, a knowledge file.
                let digest = sha256(bytes);
                if plan_digest(plan, &path) != Some(&digest) {
                    abandon!(
                        "reading the files of the first change",
                        format!(
                            "`{}` is not the file that was planned (it changed since the plan)",
                            super::shown(&path)
                        )
                    );
                }
                if path.starts_with("pages/") {
                    run.live.insert(path, digest);
                }
            }
        }
        let (pages, others) = first.iter().fold((0_usize, 0_usize), |(p, o), f| {
            if f.path.starts_with("pages/") {
                (p + 1, o)
            } else {
                (p, o + 1)
            }
        });
        let summary = match (&map_file, first.is_empty()) {
            (None, _) => {
                format!("Import {pages} page(s) and {others} other file(s) with no history")
            }
            (Some(_), true) => "Import the interwiki map".to_owned(),
            (Some(_), false) => format!(
                "Import {pages} page(s) and {others} other file(s) with no history, and the interwiki map"
            ),
        };
        files.extend(map_file);
        if let Err(why) = run.change(&summary, files, Vec::new(), None) {
            abandon!("the first change", why);
        }
        first_change = true;
        observe(ImportStep::FirstChange);
    }

    // The history, one revision a block.
    let last_of: BTreeMap<usize, usize> = {
        let mut last = BTreeMap::new();
        for (h, r) in &plan.schedule {
            last.insert(*h, *r);
        }
        last
    };
    let mut cursors: BTreeMap<usize, Cursor> = BTreeMap::new();
    let (mut revisions, mut not_applied) = (0_usize, 0_usize);
    let page_cap = plan.plan.page_cap;
    for &(h, r) in &plan.schedule {
        let Some(history) = plan.histories.get(h) else {
            abandon!(
                "the history",
                "the plan's order names a history it does not have"
            );
        };
        let Some(revision) = history.revisions.get(r) else {
            abandon!(
                "the history",
                "the plan's order names a revision it does not have"
            );
        };
        let at = format!(
            "revision `{}` of `{}`",
            super::shown(&revision.rev),
            super::shown(history.page.as_deref().unwrap_or(&history.file))
        );
        let cursor = match cursors.entry(h) {
            std::collections::btree_map::Entry::Occupied(held) => held.into_mut(),
            std::collections::btree_map::Entry::Vacant(free) => match load(plan, history) {
                Ok(c) => free.insert(c),
                Err(why) => abandon!(at, why),
            },
        };
        let text = match text_at(cursor, r, page_cap) {
            Ok(t) => t,
            Err(why) => abandon!(at, why),
        };
        let facts = match source_of(revision, &plan.users) {
            Ok(f) => f,
            Err(why) => abandon!(at, why),
        };
        let Some(place) = revision.path.clone() else {
            abandon!(at, "the plan gave the revision no place");
        };
        let change = match &revision.step {
            Step::Text { sha256: digest, .. } => {
                let Some(text) = text else {
                    abandon!(at, "the text the plan judged is not there");
                };
                let got: Digest = Sha256::digest(text.as_bytes()).into();
                if got != *digest {
                    abandon!(at, "the text is not the one the plan judged");
                }
                run.live.insert(place.clone(), got);
                Some((
                    format!("Import a revision of {}", title_of(&place)),
                    vec![write_of(&place, text)],
                    Vec::new(),
                ))
            }
            Step::Rename { from, .. } => {
                let from_path = inklkunn_markup::path::title_to_path(from).unwrap_or_default();
                if run.live.contains_key(&from_path) && from_path != place {
                    let digest = run.live.remove(&from_path);
                    if let Some(digest) = digest {
                        run.live.insert(place.clone(), digest);
                    }
                    Some((
                        format!(
                            "Import the move of {} to {}",
                            title_of(&from_path),
                            title_of(&place)
                        ),
                        Vec::new(),
                        vec![(PathBuf::from(from_path), PathBuf::from(&place))],
                    ))
                } else {
                    None
                }
            }
            Step::Delete => {
                if run.live.remove(&place).is_some() {
                    Some((
                        format!("Import the removal of {}", title_of(&place)),
                        vec![PreparedFile {
                            path: PathBuf::from(&place),
                            content: None,
                        }],
                        Vec::new(),
                    ))
                } else {
                    None
                }
            }
            Step::NotReplayed(_) => None,
        };
        match change {
            Some((summary, files, renames)) => {
                if let Err(why) = run.change(&summary, files, renames, Some(facts)) {
                    abandon!(at, why);
                }
                revisions += 1;
                observe(ImportStep::Revision(revisions));
            }
            None => not_applied += 1,
        }
        if last_of.get(&h) == Some(&r) {
            cursors.remove(&h);
        }
    }
    drop(cursors);

    // The change at the end: the pages as the bundle's files have them.
    let kept: BTreeSet<&String> = plan.page_digests.keys().collect();
    let to_write: Vec<&PlannedFile> = plan
        .plan
        .files
        .iter()
        .filter(|f| f.path.starts_with("pages/"))
        .filter(|f| run.live.get(&f.path) != plan.page_digests.get(&f.path))
        .collect();
    let to_remove: Vec<String> = run
        .live
        .keys()
        .filter(|p| !kept.contains(p))
        .cloned()
        .collect();
    let (final_written, final_removed) = (to_write.len(), to_remove.len());
    let mut final_changes = 0_usize;
    if final_written + final_removed > 0 {
        // As many changes as the caps of one acceptance need: files and bytes.
        let mut groups: Vec<(Vec<&PlannedFile>, Vec<String>)> = Vec::new();
        let (mut files, mut bytes) = (0_usize, 0_u64);
        let mut current: (Vec<&PlannedFile>, Vec<String>) = (Vec::new(), Vec::new());
        for f in to_write {
            if files + 1 > MAX_CHANGE_FILES || bytes.saturating_add(f.size) > MAX_CHANGE_BYTES {
                groups.push(std::mem::take(&mut current));
                (files, bytes) = (0, 0);
            }
            files += 1;
            bytes = bytes.saturating_add(f.size);
            current.0.push(f);
        }
        for path in to_remove {
            if files + 1 > MAX_CHANGE_FILES {
                groups.push(std::mem::take(&mut current));
                files = 0;
            }
            files += 1;
            current.1.push(path);
        }
        groups.push(current);
        for (writes, removes) in groups {
            if writes.is_empty() && removes.is_empty() {
                continue;
            }
            let mut files = match read_some(&plan.plan, writes.iter().copied()) {
                Ok(f) => f,
                Err(e) => abandon!("reading the pages of the change at the end", e),
            };
            for file in &files {
                let path = file.path.to_string_lossy().into_owned();
                if let Some(bytes) = &file.content {
                    let digest = sha256(bytes);
                    if plan_digest(plan, &path) != Some(&digest) {
                        abandon!(
                            "reading the pages of the change at the end",
                            format!(
                                "`{}` is not the file that was planned (it changed since the plan)",
                                super::shown(&path)
                            )
                        );
                    }
                    run.live.insert(path, digest);
                }
            }
            for path in &removes {
                run.live.remove(path);
                files.push(PreparedFile {
                    path: PathBuf::from(path),
                    content: None,
                });
            }
            let summary = format!(
                "Import: the final state of the bundle ({} page(s) written, {} removed)",
                writes.len(),
                removes.len()
            );
            if let Err(why) = run.change(&summary, files, Vec::new(), None) {
                abandon!("the change at the end", why);
            }
            final_changes += 1;
        }
        observe(ImportStep::FinalState);
    }

    // The claim and source files, last (Z-1): the pages they anchor to are there now, and each file comes after the files it names. They
    // are judged as an acceptance judges a change that is not a curation, and written without what only a curation sets.
    let mut knowledge_files = 0_usize;
    // The retired files are read again and counted by the step that counted them at the plan (Z-6): each must be the file the plan read,
    // and must still be retired.
    let mut retired = Unsettled::default();
    if !plan.retired_knowledge.is_empty() {
        let files = match read_some(&plan.plan, plan.retired_knowledge.iter()) {
            Ok(f) => f,
            Err(e) => abandon!("reading the retired knowledge files", e),
        };
        for file in &files {
            let path = file.path.to_string_lossy().into_owned();
            let Some(bytes) = &file.content else {
                continue;
            };
            if plan_digest(plan, &path) != Some(&sha256(bytes)) {
                abandon!(
                    "reading the retired knowledge files",
                    format!(
                        "`{}` is not the file that was planned (it changed since the plan)",
                        super::shown(&path)
                    )
                );
            }
            match crate::knowledge::unsettle(&path, bytes) {
                Ok(done) if done.retired => retired = retired.plus(done.left_out),
                Ok(_) => abandon!(
                    "reading the retired knowledge files",
                    format!("`{}` is no longer retired", super::shown(&path))
                ),
                Err(fault) => abandon!(
                    "reading the retired knowledge files",
                    format!("`{}` {fault}", super::shown(&path))
                ),
            }
        }
    }
    // The count of the retired claims is said by the first change of the claims, and that of the retired sources by the first change of
    // the sources; a family with no file to write has no change, and then its count is said by the first change there is. With no change
    // at all, only the report says it.
    let first_change_of = |directory: &str| {
        plan.knowledge_changes
            .iter()
            .position(|group| group.first().is_some_and(|p| p.starts_with(directory)))
            .or_else(|| (!plan.knowledge_changes.is_empty()).then_some(0))
    };
    let claims_say_retired = first_change_of("knowledge/claims/");
    let sources_say_retired = first_change_of("knowledge/sources/");
    let mut knowledge_not_imported = if plan.knowledge_changes.is_empty() {
        retired
    } else {
        Unsettled::default()
    };
    let planned_files: BTreeMap<&str, &PlannedFile> = plan
        .plan
        .files
        .iter()
        .filter(|f| f.is_knowledge())
        .map(|f| (f.path.as_str(), f))
        .collect();
    for (at, group) in plan.knowledge_changes.iter().enumerate() {
        let planned: Vec<&PlannedFile> = group
            .iter()
            .filter_map(|path| planned_files.get(path.as_str()).copied())
            .collect();
        if planned.len() != group.len() {
            abandon!(
                "the knowledge files",
                "the plan's order names a file it does not have"
            );
        }
        let mut files = match read_some(&plan.plan, planned.iter().copied()) {
            Ok(f) => f,
            Err(e) => abandon!("reading the knowledge files", e),
        };
        let mut left_out = Unsettled::default();
        for file in &mut files {
            let path = file.path.to_string_lossy().into_owned();
            let Some(bytes) = &file.content else {
                continue;
            };
            // **Every file read is the file the plan read** (V-2): it is the bundle's bytes that are compared, before anything is left out.
            if plan_digest(plan, &path) != Some(&sha256(bytes)) {
                abandon!(
                    "reading the knowledge files",
                    format!(
                        "`{}` is not the file that was planned (it changed since the plan)",
                        super::shown(&path)
                    )
                );
            }
            match crate::knowledge::unsettle(&path, bytes) {
                Ok(done) if done.retired => abandon!(
                    "reading the knowledge files",
                    format!(
                        "`{}` is retired, and the plan did not leave it out",
                        super::shown(&path)
                    )
                ),
                Ok(done) => {
                    left_out = left_out.plus(done.left_out);
                    if let Some(rewritten) = done.rewritten {
                        file.content = Some(rewritten);
                    }
                }
                Err(fault) => abandon!(
                    "reading the knowledge files",
                    format!("`{}` {fault}", super::shown(&path))
                ),
            }
        }
        if claims_say_retired == Some(at) {
            left_out.retired_claims = retired.retired_claims;
        }
        if sources_say_retired == Some(at) {
            left_out.retired_sources = retired.retired_sources;
        }
        let summary = left_out.summary(files.len());
        knowledge_files += files.len();
        knowledge_not_imported = knowledge_not_imported.plus(left_out);
        if let Err(why) = run.change(&summary, files, Vec::new(), None) {
            abandon!("the knowledge change", why);
        }
    }
    // **The count is the plan's** (Z-6): the step that counted at the plan and the one that counted here are the same, over the same
    // files, so a difference is a bundle that changed under the import.
    if knowledge_not_imported != plan.report.knowledge_not_imported {
        abandon!(
            "the knowledge files",
            "what was left out is not what the plan counted"
        );
    }
    let knowledge_changes = plan.knowledge_changes.len();
    if knowledge_changes > 0 {
        observe(ImportStep::Knowledge);
    }

    // The whole repository verifies, as a creation's does, and the report is kept beside it; then the record, last.
    if let Err(fault) = verify_repository(&**world.storage, &repo) {
        let e = fault.with_cleanup(settle(mark, discard(world.layout, signer_bootstrap, &id)));
        return Err(ImportBundleError::Creation(e));
    }
    observe(ImportStep::Verified);
    if let Err(e) = keep_report(world.layout, &id, report_text) {
        abandon!("keeping the report", e);
    }
    observe(ImportStep::ReportKept);
    let mut record = record;
    record.verification = Verification {
        state: VerificationState::Verified,
        at: Some((world.clock)()),
    };
    let entries = match staged_acts(staging, &id) {
        Ok(e) => e,
        Err(why) => abandon!("collecting the acts of the history", why),
    };
    let stored = match world.acts.insert_wiki_with_acts(record, &entries) {
        Ok(r) => r,
        Err(e) => abandon!("storing the record", e),
    };
    observe(ImportStep::Stored);
    let _ = mark.finish();
    let tip = run_tip(&entries, &creation_block);
    Ok(ImportedWiki {
        record: stored,
        repo,
        creation_block,
        tip,
        first_change,
        revisions,
        not_applied,
        final_changes,
        final_written,
        final_removed,
        knowledge_files,
        knowledge_changes,
        knowledge_not_imported,
    })
}

/// Every act of the wiki the staging store holds, each with its end: an act that has none is a history that is not whole.
fn staged_acts(
    staging: &MemoryStore,
    id: &WikiId,
) -> Result<Vec<(SystemActRecord, SystemActEnd)>, String> {
    let entries = ActStore::system_acts(staging, id).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry.end {
            Some(end) => out.push((entry.act, end)),
            None => return Err("an act of the history has no end".to_owned()),
        }
    }
    Ok(out)
}

/// The block the last act sealed.
fn run_tip(entries: &[(SystemActRecord, SystemActEnd)], creation_block: &str) -> String {
    entries
        .iter()
        .rev()
        .find_map(|(_, end)| match &end.outcome {
            inklkunn_store::AcceptanceOutcome::Sealed { block_id, .. } => Some(block_id.clone()),
            inklkunn_store::AcceptanceOutcome::Failed { .. } => None,
        })
        .unwrap_or_else(|| creation_block.to_owned())
}

/// The report of the plan, kept beside the repository (`<id>.import-report.txt`): **kept with the act**, as D-6 says. The store's
/// schema is frozen and the audit trail carries no content, so the wiki's own directory is where it lives and dies with the wiki.
fn keep_report(layout: &RootLayout, id: &WikiId, text: &str) -> Result<(), String> {
    use std::io::Write as _;
    let path = layout.import_report(id);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("the report could not be kept ({})", e.kind()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("the report could not be kept ({})", e.kind()))
}
