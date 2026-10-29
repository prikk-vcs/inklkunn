//! `inklkunn wiki create | import-folder | export` and the start-up work (handoff 003 T7; RFC 003 D-11, D-12):
//! the administrative acts that fill, copy and bring wikis into being, from the command line of the host that
//! runs the service.
//!
//! **Who may do this.** Whoever can run the command on the host and read the operator's secret file, as for
//! `wiki keys`. The web administration surface that offers the exports as downloads from the content origin
//! needs sign-in pages that do not exist yet; this release has the command line only.
//!
//! **Every act is audited**, principal `system:operator-cli`, the trail written *before* the act (an act that
//! cannot be recorded is not begun) and again when it ends. Events carry no content: an action name, the wiki's
//! id and a few fixed words.
//!
//! - `wiki create <slug> --name <name>`: *create wiki* with the real signer and key store. Kill-safe: the
//!   creation is marked, and a start-up sweep (here, and in `serve`) removes an unfinished one whole.
//! - `wiki import-folder <wiki> <dir> [--dry-run] [--replace-existing]`: the folder as one accepted change under
//!   the system principal. `--dry-run` prints the same report and writes nothing; a refusal anywhere refuses all;
//!   a folder the wiki already has, byte for byte, is *nothing changed* (exit 0, no act begun).
//! - `wiki export <wiki> [--bundle <dir>] [--archive <file>]`: a Portable Wiki Bundle at fidelity level 3, and/or
//!   prikk's own archive, written by the signer's keyless writable mode, serialized with the wiki's writes.
//!
//! The M0 bootstrap signer is not here and cannot be: this crate does not enable the `bootstrap` feature of the
//! signer, and a test reads the manifest to say so.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::export::is_checkpoint_not_recorded;
use inklkunn_core::export::{ExportError, export_bundle};
use inklkunn_core::import::{ImportError, ImportLedger, plan_folder, run_import, wiki_state};
use inklkunn_core::{
    AcceptError, Maintenance, Signer, SignerConfig, SignerError, create_wiki,
    sweep_unfinished_creations,
};
use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::{Admission, PrikkCli};
use inklkunn_store::{
    AuditEvent, AuditOutcome, AuditSink, DisplayName, NewWiki, PrikkVersion, RootLayout, Slug,
    SqliteStore, Visibility, WikiRecord, WikiStore,
};

use crate::config::{Settings, UNCONFINED_BANNER};
use crate::keys::{find_wiki, key_authority, make_private_directory};

/// The operator at the host, in the audit trail.
const PRINCIPAL: &str = "system:operator-cli";

/// What was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Create {
        slug: String,
        name: String,
        license: Option<String>,
        languages: Vec<String>,
        visibility: Option<String>,
    },
    ImportFolder {
        wiki: String,
        dir: String,
        dry_run: bool,
        replace_existing: bool,
    },
    Export {
        wiki: String,
        bundle: Option<String>,
        archive: Option<String>,
    },
}

/// The command line was not understood (exit status 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usage(pub String);

pub(crate) const USAGE: &str = "usage: inklkunn [--dev-unconfined] wiki create <slug> --name <name> [--license <spdx>] [--lang <tag>]... [--visibility public|private]\n       \
inklkunn [--dev-unconfined] wiki import-folder <wiki> <dir> [--dry-run] [--replace-existing]\n       \
inklkunn [--dev-unconfined] wiki export <wiki> [--bundle <dir>] [--archive <file>]\n\
<wiki> is the wiki's slug or id. An export's destination must not exist.";

/// Parse what follows `wiki` for `create`, `import-folder` and `export`.
pub(crate) fn parse(args: &[String]) -> Result<Command, Usage> {
    let usage = |why: &str| Usage(format!("{why}\n{USAGE}"));
    let (sub, rest) = args.split_first().ok_or_else(|| usage("which command?"))?;
    let (first, mut flags) = match rest.split_first() {
        Some((w, f)) if !w.starts_with("--") => (w.clone(), f),
        _ => return Err(usage("which wiki?")),
    };
    let value = |flag: &str, it: &mut std::slice::Iter<'_, String>| {
        it.next()
            .cloned()
            .ok_or_else(|| usage(&format!("{flag} needs a value")))
    };
    match sub.as_str() {
        "create" => {
            let (mut name, mut license, mut visibility) = (None, None, None);
            let mut languages = Vec::new();
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--name" => name = Some(value("--name", &mut it)?),
                    "--license" => license = Some(value("--license", &mut it)?),
                    "--lang" => languages.push(value("--lang", &mut it)?),
                    "--visibility" => visibility = Some(value("--visibility", &mut it)?),
                    other => return Err(usage(&format!("`{other}` is not an option of `create`"))),
                }
            }
            let name = name.ok_or_else(|| usage("a wiki needs a name: --name <name>"))?;
            Ok(Command::Create {
                slug: first,
                name,
                license,
                languages,
                visibility,
            })
        }
        "import-folder" => {
            let (dir, rest) = match flags.split_first() {
                Some((d, r)) if !d.starts_with("--") => (d.clone(), r),
                _ => return Err(usage("which folder?")),
            };
            flags = rest;
            let (mut dry_run, mut replace_existing) = (false, false);
            for flag in flags {
                match flag.as_str() {
                    "--dry-run" => dry_run = true,
                    "--replace-existing" => replace_existing = true,
                    other => {
                        return Err(usage(&format!(
                            "`{other}` is not an option of `import-folder`"
                        )));
                    }
                }
            }
            Ok(Command::ImportFolder {
                wiki: first,
                dir,
                dry_run,
                replace_existing,
            })
        }
        "export" => {
            let (mut bundle, mut archive) = (None, None);
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--bundle" => bundle = Some(value("--bundle", &mut it)?),
                    "--archive" => archive = Some(value("--archive", &mut it)?),
                    other => return Err(usage(&format!("`{other}` is not an option of `export`"))),
                }
            }
            if bundle.is_none() && archive.is_none() {
                return Err(usage(
                    "what to export? --bundle <dir> and/or --archive <file>",
                ));
            }
            Ok(Command::Export {
                wiki: first,
                bundle,
                archive,
            })
        }
        other => Err(usage(&format!("`{other}` is not a command"))),
    }
}

/// The audit trail for one act. `begin` first: an act the trail cannot take is not begun.
struct Trail<'a> {
    sink: &'a dyn AuditSink,
    now: i64,
}

impl Trail<'_> {
    fn event(
        &self,
        action: &str,
        resource: &str,
        outcome: AuditOutcome,
        detail: &str,
    ) -> AuditEvent {
        AuditEvent::new(self.now, PRINCIPAL, action, resource, outcome, detail)
    }

    fn begin(&self, action: &str, resource: &str, detail: &str) -> Result<(), String> {
        self.sink
            .record(&self.event(action, resource, AuditOutcome::Allowed, detail))
            .map_err(|e| format!("the audit trail could not be written ({e}); nothing was done"))
    }

    /// The act has happened either way, so a failure here is an error the operator sees.
    fn finish(
        &self,
        action: &str,
        resource: &str,
        outcome: AuditOutcome,
        detail: &str,
    ) -> Result<(), String> {
        self.sink
            .record(&self.event(action, resource, outcome, detail))
            .map_err(|e| {
                format!(
                    "the act was done but its completion could not be written to the audit trail ({e}); \
                     the trail shows it as begun and not finished"
                )
            })
    }
}

fn wiki_resource(record: &WikiRecord) -> String {
    format!("wiki:{}", record.id)
}

/// The same read bounds the server starts with, for the reads a command makes.
fn admission() -> Admission {
    Admission::new(1024 << 20, 64 << 20, Duration::from_millis(2000))
}

/// What a command needs once the settings are read: the sandbox, the layout and the driver.
struct Env {
    sandbox: Sandbox,
    layout: RootLayout,
    storage: PrikkCli,
}

fn environment(settings: &Settings, err: &mut dyn Write) -> Result<Env, String> {
    let sandbox = settings.sandbox().map_err(|e| e.to_string())?;
    if settings.dev_unconfined {
        let _ = writeln!(err, "{UNCONFINED_BANNER}");
    }
    let storage = PrikkCli::open(&settings.prikk, sandbox.clone(), admission()).map_err(|e| {
        format!(
            "prikk is not usable ({e}); install prikk {} or later",
            inklkunn_prikk::MIN_PRIKK_VERSION
        )
    })?;
    Ok(Env {
        sandbox,
        layout: RootLayout::new(settings.wikis_root()),
        storage,
    })
}

fn signer_config(settings: &Settings, env: &Env) -> SignerConfig {
    SignerConfig::new(
        settings.prikk.clone(),
        env.sandbox.clone(),
        env.layout.clone(),
        &settings.key_dir,
        &settings.secret_file,
        &settings.seed_dir,
    )
}

fn existing_store(settings: &Settings) -> Result<SqliteStore, String> {
    let path = settings.store_path();
    if !path.is_file() {
        return Err(format!(
            "there is no store at {} (INKLKUNN_DATA_DIR); this command works on a deployment that has been started or has a wiki",
            path.display()
        ));
    }
    SqliteStore::open(&path).map_err(|e| format!("the store could not be opened: {e}"))
}

/// A destination as an absolute path (a relative one is taken from the directory the command runs in).
fn absolute(p: &str) -> Result<PathBuf, String> {
    std::path::absolute(p)
        .map_err(|e| format!("`{p}` cannot be made an absolute path ({})", e.kind()))
}

/// Run a parsed command. Returns the process exit status: 0, or 1 for a refusal or a failure.
pub(crate) fn execute(
    command: &Command,
    settings: &Settings,
    now: i64,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    match run(command, settings, now, out, err) {
        Ok(()) => 0,
        Err(why) => {
            let _ = writeln!(err, "inklkunn: {why}");
            1
        }
    }
}

fn run(
    command: &Command,
    settings: &Settings,
    now: i64,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), String> {
    match command {
        Command::Create {
            slug,
            name,
            license,
            languages,
            visibility,
        } => create(
            settings, now, slug, name, license, languages, visibility, out, err,
        ),
        Command::ImportFolder {
            wiki,
            dir,
            dry_run,
            replace_existing,
        } => import_folder(
            settings,
            now,
            wiki,
            dir,
            *dry_run,
            *replace_existing,
            out,
            err,
        ),
        Command::Export {
            wiki,
            bundle,
            archive,
        } => export(
            settings,
            now,
            wiki,
            bundle.as_deref(),
            archive.as_deref(),
            out,
            err,
        ),
    }
}

/// Print what a start-up sweep found.
fn report_sweep(report: &inklkunn_core::SweepReport, err: &mut dyn Write) {
    for id in &report.removed {
        let _ = writeln!(
            err,
            "inklkunn: removed the unfinished creation of wiki {id} (the process that was creating it is gone); nothing of it is left"
        );
    }
    for id in &report.completed {
        let _ = writeln!(
            err,
            "inklkunn: wiki {id} had finished being created; its creation mark was removed"
        );
    }
    for id in &report.live {
        let _ = writeln!(
            err,
            "inklkunn: wiki {id} is being created by another process; left alone"
        );
    }
    for (id, why) in &report.left_behind {
        let _ = writeln!(
            err,
            "inklkunn: the unfinished creation of wiki {id} could not be removed whole: {why}; it is tried again at the next start"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn create(
    settings: &Settings,
    now: i64,
    slug: &str,
    name: &str,
    license: &Option<String>,
    languages: &[String],
    visibility: &Option<String>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), String> {
    // The request first: nothing is made for one that cannot be a wiki.
    let slug = Slug::new(slug).map_err(|e| format!("`{slug}` is not a usable slug ({e})"))?;
    let display_name =
        DisplayName::new(name).map_err(|e| format!("`{name}` is not a usable name ({e})"))?;
    let mut new = NewWiki::new(slug.clone(), display_name);
    new.default_license = license.clone();
    new.languages = languages.to_vec();
    if let Some(v) = visibility {
        new.visibility = Visibility::parse(v)
            .ok_or_else(|| format!("`{v}` is not a visibility (public or private)"))?;
    }
    let env = environment(settings, err)?;
    make_private_directory(&settings.seed_dir).map_err(|e| {
        format!(
            "the seed directory could not be made private ({})",
            e.kind()
        )
    })?;
    // *Create* is the act that is asked to bring a deployment, and its keys, into being.
    let fresh = !settings.store_path().is_file();
    std::fs::create_dir_all(&settings.data_dir)
        .map_err(|e| format!("INKLKUNN_DATA_DIR could not be made ({})", e.kind()))?;
    let store = SqliteStore::open(&settings.store_path())
        .map_err(|e| format!("the store could not be opened: {e}"))?;
    if fresh {
        let _ = writeln!(
            err,
            "inklkunn: made a new store at {}",
            settings.store_path().display()
        );
    }
    let signer =
        Signer::open_creating_store(signer_config(settings, &env), Arc::new(ImportLedger::new()))
            .map_err(|e| format!("the signer could not be opened: {e}"))?;
    let swept = signer.leftover_call_directories_removed();
    if swept > 0 {
        let _ = writeln!(
            err,
            "inklkunn: removed {swept} leftover key directories of a call that was killed; look at how the service stopped"
        );
    }
    // What an earlier, killed creation left goes first.
    let report = sweep_unfinished_creations(&store, &env.layout, &signer)?;
    report_sweep(&report, err);
    let trail = Trail { sink: &store, now };
    trail.begin("wiki-create", "wiki:new", "begun")?;
    let created = create_wiki(
        &store,
        &env.layout,
        &signer,
        &env.storage,
        new,
        env!("CARGO_PKG_VERSION"),
        key_authority(),
    );
    match created {
        Ok(c) => {
            let done = trail.finish(
                "wiki-create",
                &wiki_resource(&c.record),
                AuditOutcome::Allowed,
                "created",
            );
            let _ = writeln!(out, "created wiki {} ({})", c.record.slug, c.record.id);
            let _ = writeln!(out, "  name:            {}", c.record.display_name);
            let _ = writeln!(out, "  repository:      {}", c.repo.display());
            let _ = writeln!(out, "  first block:     {}", c.first_block_id);
            let _ = writeln!(
                out,
                "  author key:      {}",
                c.record.author_key_id.as_deref().unwrap_or("(none)")
            );
            let _ = writeln!(
                out,
                "  maintainer key:  {}",
                c.record.maintainer_key_id.as_deref().unwrap_or("(none)")
            );
            done
        }
        Err(e) => {
            let done = trail.finish(
                "wiki-create",
                "wiki:new",
                AuditOutcome::Failed,
                "refused or failed",
            );
            done.and(Err(e.to_string()))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn import_folder(
    settings: &Settings,
    now: i64,
    wiki: &str,
    dir: &str,
    dry_run: bool,
    replace_existing: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), String> {
    let folder = absolute(dir)?;
    let store = existing_store(settings)?;
    let record = find_wiki(&store, wiki)?;
    let env = environment(settings, err)?;
    // What the wiki has now, at one tip: to tell a new path from an existing one, a file that would not change
    // from one that would, and the page ids and names that are taken. The plan is pinned to that tip.
    let mut have = wiki_state(&env.storage, &env.layout, &record);
    if let Err(ImportError::Storage(e)) = &have
        && is_checkpoint_not_recorded(e)
    {
        // The record `extract` anchors on is missing (a prikk upgrade, a restore): bring it back, once.
        let _ = writeln!(
            err,
            "inklkunn: prikk's checkpoint record is missing; refreshing it before the import"
        );
        let maintenance = Maintenance::new(
            settings.prikk.clone(),
            env.sandbox.clone(),
            env.layout.clone(),
        );
        refresh_checkpoint(&store, &maintenance, &env.storage, &record, now)?;
        have = wiki_state(&env.storage, &env.layout, &record);
    }
    let have = have.map_err(|e| e.to_string())?;
    let plan = plan_folder(&folder, &have, replace_existing).map_err(|e| e.to_string())?;
    let _ = write!(out, "{}", plan.report());
    if plan.is_unchanged() {
        // Nothing to write and nothing wrong: said by name, in a dry run and in the run, and no act is begun.
        let _ = writeln!(out, "{}", ImportError::NothingChanged(plan.unchanged.len()));
        return Ok(());
    }
    if dry_run {
        let _ = writeln!(
            out,
            "dry run: nothing was written{}",
            if plan.is_importable() {
                ""
            } else {
                "; the import would be refused"
            }
        );
        return if plan.is_importable() {
            Ok(())
        } else {
            Err(ImportError::Refused(plan.refused.len()).to_string())
        };
    }
    if !plan.is_importable() {
        return Err(ImportError::Refused(plan.refused.len()).to_string());
    }
    make_private_directory(&settings.seed_dir).map_err(|e| {
        format!(
            "the seed directory could not be made private ({})",
            e.kind()
        )
    })?;
    let ledger = Arc::new(ImportLedger::new());
    let signer = Signer::open(signer_config(settings, &env), ledger.clone())
        .map_err(|e| format!("the signer could not be opened: {e}"))?;
    let resource = wiki_resource(&record);
    let trail = Trail { sink: &store, now };
    trail.begin("wiki-import", &resource, "begun")?;
    match run_import(&signer, &ledger, &record, &plan) {
        Ok(done) => {
            let _ = writeln!(
                out,
                "imported {} file(s) as one change, block {}",
                done.files, done.sealed.block_id
            );
            trail.finish("wiki-import", &resource, AuditOutcome::Allowed, "imported")
        }
        Err(e) => {
            // Whatever state the signer says the repository is in, the trail records that the act ended badly.
            let detail = match &e {
                ImportError::Accept(AcceptError::SealedUnconfirmed { .. }) => {
                    "sealed but not confirmed; do not repeat"
                }
                ImportError::Accept(AcceptError::Queued { .. }) => "committed, not sealed",
                _ => "refused or failed",
            };
            let done = trail.finish("wiki-import", &resource, AuditOutcome::Failed, detail);
            done.and(Err(e.to_string()))
        }
    }
}

fn export(
    settings: &Settings,
    now: i64,
    wiki: &str,
    bundle: Option<&str>,
    archive: Option<&str>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), String> {
    // Destinations are resolved before anything is read or written.
    let bundle = bundle.map(absolute).transpose()?;
    let archive = archive.map(absolute).transpose()?;
    let store = existing_store(settings)?;
    let record = find_wiki(&store, wiki)?;
    let env = environment(settings, err)?;
    let resource = wiki_resource(&record);
    let trail = Trail { sink: &store, now };
    let maintenance = Maintenance::new(
        settings.prikk.clone(),
        env.sandbox.clone(),
        env.layout.clone(),
    );
    let mut failure: Option<String> = None;
    if let Some(dest) = &bundle {
        trail.begin("wiki-export-bundle", &resource, "begun")?;
        let mut result = export_bundle(
            &env.storage,
            &env.layout,
            &record,
            dest,
            env!("CARGO_PKG_VERSION"),
            now,
        );
        if matches!(result, Err(ExportError::CheckpointNotRecorded)) {
            // The record `extract` anchors on is missing (a prikk upgrade, a restore): bring it back, once.
            let _ = writeln!(
                err,
                "inklkunn: prikk's checkpoint record is missing; refreshing it before the export"
            );
            match refresh_checkpoint(&store, &maintenance, &env.storage, &record, now) {
                Ok(()) => {
                    result = export_bundle(
                        &env.storage,
                        &env.layout,
                        &record,
                        dest,
                        env!("CARGO_PKG_VERSION"),
                        now,
                    );
                }
                Err(why) => failure = Some(why),
            }
        }
        match result {
            Ok(done) => {
                let _ = writeln!(
                    out,
                    "exported a bundle of {} page(s) and {} attachment(s) as of block {} to {}",
                    done.pages,
                    done.attachments,
                    done.tip,
                    done.dir.display()
                );
                let _ = writeln!(
                    out,
                    "  history: {} page(s), {} line(s){}{}; users: {}",
                    done.histories,
                    done.history_entries,
                    if done.histories_not_written > 0 {
                        format!(
                            "; {} page(s) have no history file (the page id cannot name a file or is shared)",
                            done.histories_not_written
                        )
                    } else {
                        String::new()
                    },
                    if done.unattributed_changes > 0 {
                        format!(
                            "; {} change(s) to paths that are no longer pages are in no history file",
                            done.unattributed_changes
                        )
                    } else {
                        String::new()
                    },
                    done.users
                );
                trail.finish(
                    "wiki-export-bundle",
                    &resource,
                    AuditOutcome::Allowed,
                    "exported",
                )?;
            }
            Err(e) => {
                trail.finish(
                    "wiki-export-bundle",
                    &resource,
                    AuditOutcome::Failed,
                    "refused or failed",
                )?;
                failure.get_or_insert(e.to_string());
            }
        }
    }
    if let (Some(dest), None) = (&archive, &failure) {
        trail.begin("wiki-export-archive", &resource, "begun")?;
        match maintenance.archive_export(&record.id, dest) {
            Ok(done) => {
                let _ = writeln!(
                    out,
                    "exported prikk's archive of the repository ({} bytes, repository format {}, prikk {}) to {}",
                    done.bytes,
                    done.repository_format,
                    done.prikk_version,
                    dest.display()
                );
                let _ = writeln!(
                    out,
                    "  it holds every page, private ones included, and is readable by you alone"
                );
                trail.finish(
                    "wiki-export-archive",
                    &resource,
                    AuditOutcome::Allowed,
                    "exported",
                )?;
            }
            Err(e) => {
                trail.finish(
                    "wiki-export-archive",
                    &resource,
                    AuditOutcome::Failed,
                    "refused or failed",
                )?;
                failure.get_or_insert(e.to_string());
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

/// The prikk version the driver found, as the record keeps it.
fn prikk_version(storage: &PrikkCli) -> Option<PrikkVersion> {
    PrikkVersion::parse(storage.version())
}

/// Refresh prikk's checkpoint record for one wiki (the keyless writable mode), and record it: the wiki record
/// says which prikk version did it, and the request is cleared. Audited like any act on a repository. `Ok` only
/// when the refresh ran to a passing, complete verification.
fn refresh_checkpoint(
    store: &SqliteStore,
    maintenance: &Maintenance,
    storage: &PrikkCli,
    record: &WikiRecord,
    now: i64,
) -> Result<(), String> {
    let version = prikk_version(storage)
        .ok_or_else(|| "prikk's version text is not usable in a record".to_owned())?;
    let resource = wiki_resource(record);
    let trail = Trail { sink: store, now };
    trail.begin("checkpoint-refresh", &resource, "begun")?;
    let outcome = match maintenance.refresh_checkpoint(&record.id) {
        Ok(v) if v.verdict.ok => store
            .record_checkpoint_refresh(&record.id, &version)
            .map(|_| ())
            .map_err(|e| format!("the refresh ran but the wiki record could not be updated ({e})")),
        Ok(_) => Err(
            "the repository failed verification, so the checkpoint was not refreshed; run `prikk verify` in it"
                .to_owned(),
        ),
        Err(SignerError::Busy { .. }) => Err(
            "another act is writing the wiki; the refresh is tried again at the next start".to_owned(),
        ),
        Err(e) => Err(e.to_string()),
    };
    let (audit, detail) = match &outcome {
        Ok(()) => (AuditOutcome::Allowed, "refreshed"),
        Err(_) => (AuditOutcome::Failed, "not refreshed"),
    };
    trail.finish("checkpoint-refresh", &resource, audit, detail)?;
    outcome
}

/// The work every start does before it serves (handoff 003 T7): remove the creations that were killed, and
/// refresh prikk's checkpoint record for every wiki whose last refresh ran under another prikk version or whose
/// refresh was asked for (IS-9). Failures are returned as lines for the operator; none of them stops the server,
/// because what they leave undone is housekeeping and the pages that need it say *could not be read now*.
pub(crate) fn start_up(
    store: &SqliteStore,
    settings: &Settings,
    sandbox: &Sandbox,
    storage: &PrikkCli,
    now: i64,
) -> Vec<String> {
    let mut notes = Vec::new();
    let layout = RootLayout::new(settings.wikis_root());
    // 1. Creations that were killed. Looking is free; the key store is opened only when there is something.
    match layout.creation_marks() {
        Ok(marks) if !marks.is_empty() => {
            let config = SignerConfig::new(
                settings.prikk.clone(),
                sandbox.clone(),
                layout.clone(),
                &settings.key_dir,
                &settings.secret_file,
                &settings.seed_dir,
            );
            let opened = make_private_directory(&settings.seed_dir)
                .map_err(|e| {
                    format!(
                        "the seed directory could not be made private ({})",
                        e.kind()
                    )
                })
                .and_then(|()| {
                    Signer::open(config, Arc::new(ImportLedger::new())).map_err(|e| e.to_string())
                });
            match opened {
                Ok(signer) => match sweep_unfinished_creations(store, &layout, &signer) {
                    Ok(report) => {
                        let mut text = Vec::new();
                        report_sweep(&report, &mut text);
                        notes.extend(String::from_utf8_lossy(&text).lines().map(str::to_owned));
                    }
                    Err(why) => notes.push(format!(
                        "inklkunn: unfinished creations could not be examined: {why}"
                    )),
                },
                Err(why) => notes.push(format!(
                    "inklkunn: {} wiki creation(s) did not finish and were not removed, because the signer could not be opened ({why}); the next start tries again",
                    marks.len()
                )),
            }
        }
        Ok(_) => {}
        Err(e) => notes.push(format!(
            "inklkunn: the wikis' root could not be examined for unfinished creations ({})",
            e.kind()
        )),
    }
    // 2. The checkpoint refresh.
    let Some(version) = prikk_version(storage) else {
        notes.push(
            "inklkunn: prikk's version text is not usable in a record; no checkpoint was refreshed"
                .to_owned(),
        );
        return notes;
    };
    let records = match store.list() {
        Ok(r) => r,
        Err(e) => {
            notes.push(format!("inklkunn: the wikis could not be listed ({e})"));
            return notes;
        }
    };
    let maintenance = Maintenance::new(settings.prikk.clone(), sandbox.clone(), layout);
    for record in records {
        let stale = record.checkpoint.refreshed_with.as_ref() != Some(&version)
            || record.checkpoint.refresh_requested_at.is_some();
        if !stale {
            continue;
        }
        match refresh_checkpoint(store, &maintenance, storage, &record, now) {
            Ok(()) => notes.push(format!(
                "inklkunn: refreshed prikk's checkpoint record for wiki {} ({})",
                record.slug, version
            )),
            Err(why) => notes.push(format!(
                "inklkunn: the checkpoint record of wiki {} was not refreshed: {why}",
                record.slug
            )),
        }
    }
    notes
}

#[cfg(test)]
mod tests;
