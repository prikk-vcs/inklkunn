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
//! - `wiki recover <wiki> [--plan]`: recovery of a wiki that is flagged *needs recovery* or was left half-done
//!   (RFC 004 D-7; handoff 004 T5), under the wiki's write lease: the witnesses are read, the plan is shown and
//!   recorded, then taken step by step (a queued patch is sealed, a torn commit is discarded, stale locks are cleared,
//!   the records are ended as what happened, the repository is verified, the flag is cleared). `--plan` shows the plan
//!   and does nothing else. A state no verb can answer is a *stop*: nothing is changed and a person decides. The
//!   start of the service runs the same recovery for every wiki that needs it.
//!
//! The M0 bootstrap signer is not here and cannot be: this crate does not enable the `bootstrap` feature of the
//! signer, and a test reads the manifest to say so.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::export::is_checkpoint_not_recorded;
use inklkunn_core::export::{ExportError, export_bundle_with, knowledge_records};
use inklkunn_core::import::{ImportError, judge_knowledge, plan_folder, run_import, wiki_state};
use inklkunn_core::proposal::{Period, operator_metrics};
use inklkunn_core::read::{ReadConfig, ReadService};
use inklkunn_core::recovery::needs_recovery;
use inklkunn_core::{
    Authorizer, LeaseConfig, Leased, Maintenance, NoLedger, Recover, RecoverOptions, Recovery,
    RecoveryError, RecoveryMode, RecoveryOutcome, RecoveryParts, RecoveryReport, Signer,
    SignerConfig, StoreAuthz, StoreLedger, SweepAudit, SystemActFailure, SystemActs, WikiLeases,
    Witnesses, WriteLeases, create_wiki, sweep_unfinished_creations,
};
use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::{Admission, PrikkCli, Storage};
use inklkunn_store::{
    AuditEvent, AuditOutcome, AuditSink, DisplayName, NewWiki, PrikkVersion, RecoveryReason,
    RootLayout, Slug, SqliteStore, Visibility, WikiId, WikiRecord, WikiStore,
};

use crate::config::{Settings, UNCONFINED_BANNER};
use crate::keys::{find_wiki, key_authority, make_private_directory};

/// The operator at the host, in the audit trail.
pub(crate) const PRINCIPAL: &str = "system:operator-cli";

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
    /// The participation metrics of a wiki, as one JSON Lines record (RFC 004 D-10).
    Metrics {
        wiki: String,
        from: Option<i64>,
        to: Option<i64>,
        days: Option<u32>,
    },
    Recover {
        wiki: String,
        plan_only: bool,
        /// `--discard-queued-patch <patch id>`: the person's confirmation that this queued patch is taken back
        /// (prikk 0.57.0, ledger PK-43).
        discard_queued_patch: Option<String>,
    },
}

/// The command line was not understood (exit status 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usage(pub String);

pub(crate) const USAGE: &str = "usage: inklkunn [--dev-unconfined] wiki create <slug> --name <name> [--license <spdx>] [--lang <tag>]... [--visibility public|private]\n       \
inklkunn [--dev-unconfined] wiki import-folder <wiki> <dir> [--dry-run] [--replace-existing]\n       \
inklkunn [--dev-unconfined] wiki export <wiki> [--bundle <dir>] [--archive <file>]\n       \
inklkunn [--dev-unconfined] wiki recover <wiki> [--plan] [--discard-queued-patch <patch id>]\n       \
inklkunn [--dev-unconfined] wiki metrics <wiki> [--days <n> | --from <unix> --to <unix>]\n\
<wiki> is the wiki's slug or id. An export's destination must not exist. `recover --plan` shows what recovery would do and changes nothing (prikk's own preview of taking a queued patch back is shown by a run, not by --plan). `--discard-queued-patch <patch id>` asks to take the one queued patch back (prikk 0.57.0 and later) when no recorded act describes it; the id is the confirmation, and the plan and the audit trail say whether it was used.";

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
        "metrics" => {
            let (mut from, mut to, mut days) = (None, None, None);
            let number = |flag: &str, it: &mut std::slice::Iter<'_, String>| {
                value(flag, it)?
                    .parse::<i64>()
                    .map_err(|_| usage(&format!("{flag} needs a whole number")))
            };
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--from" => from = Some(number("--from", &mut it)?),
                    "--to" => to = Some(number("--to", &mut it)?),
                    "--days" => {
                        days = Some(
                            u32::try_from(number("--days", &mut it)?)
                                .ok()
                                .filter(|d| (1..=3660).contains(d))
                                .ok_or_else(|| usage("--days is between 1 and 3660"))?,
                        );
                    }
                    other => {
                        return Err(usage(&format!("`{other}` is not an option of `metrics`")));
                    }
                }
            }
            if days.is_some() && (from.is_some() || to.is_some()) {
                return Err(usage("--days or --from/--to, not both"));
            }
            if from.is_some() != to.is_some() {
                return Err(usage("--from and --to go together"));
            }
            Ok(Command::Metrics {
                wiki: first,
                from,
                to,
                days,
            })
        }
        "recover" => {
            let mut plan_only = false;
            let mut discard_queued_patch = None;
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--plan" => plan_only = true,
                    "--discard-queued-patch" => {
                        let id = value("--discard-queued-patch", &mut it)?;
                        // A patch id is 64 lower-case hex characters: the confirmation is typed in full.
                        if id.len() != 64
                            || !id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                        {
                            return Err(usage(
                                "--discard-queued-patch needs the patch's id: 64 lower-case hex characters, as `prikk status --format json` or `recover --plan` prints it",
                            ));
                        }
                        discard_queued_patch = Some(id);
                    }
                    other => {
                        return Err(usage(&format!("`{other}` is not an option of `recover`")));
                    }
                }
            }
            if plan_only && discard_queued_patch.is_some() {
                return Err(usage(
                    "--plan changes nothing, so it cannot take a patch back: give --discard-queued-patch without --plan",
                ));
            }
            Ok(Command::Recover {
                wiki: first,
                plan_only,
                discard_queued_patch,
            })
        }
        other => Err(usage(&format!("`{other}` is not a command"))),
    }
}

/// The audit trail for one act. `begin` first: an act the trail cannot take is not begun.
pub(crate) struct Trail<'a> {
    sink: &'a dyn AuditSink,
    now: i64,
}

impl<'a> Trail<'a> {
    /// A trail that writes to `sink`, stamped `now`.
    pub(crate) fn new(sink: &'a dyn AuditSink, now: i64) -> Self {
        Trail { sink, now }
    }
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

    pub(crate) fn begin(&self, action: &str, resource: &str, detail: &str) -> Result<(), String> {
        self.sink
            .record(&self.event(action, resource, AuditOutcome::Allowed, detail))
            .map_err(|e| format!("the audit trail could not be written ({e}); nothing was done"))
    }

    /// The act has happened either way, so a failure here is an error the operator sees.
    pub(crate) fn finish(
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
        Command::Recover {
            wiki,
            plan_only,
            discard_queued_patch,
        } => recover(
            settings,
            now,
            wiki,
            *plan_only,
            discard_queued_patch.as_deref(),
            out,
            err,
        ),
        Command::Metrics {
            wiki,
            from,
            to,
            days,
        } => metrics(settings, now, wiki, *from, *to, *days, out),
    }
}

/// `inklkunn wiki metrics`: the participation metrics of one wiki over a period, as one JSON Lines record on
/// standard output (RFC 004 D-10; CT-08). The figures are about people, so the act is audited before it is done.
/// Without a period: the last 30 days.
fn metrics(
    settings: &Settings,
    now: i64,
    wiki: &str,
    from: Option<i64>,
    to: Option<i64>,
    days: Option<u32>,
    out: &mut dyn Write,
) -> Result<(), String> {
    let store = existing_store(settings)?;
    let record = find_wiki(&store, wiki)?;
    let period = match (from, to) {
        (Some(from), Some(to)) if from < to => Period { from, to },
        (Some(_), Some(_)) => return Err("--from must be before --to".to_owned()),
        _ => Period {
            from: now.saturating_sub(i64::from(days.unwrap_or(30)) * 86_400),
            to: now,
        },
    };
    let trail = Trail { sink: &store, now };
    let resource = wiki_resource(&record);
    trail.begin("wiki-metrics", &resource, "begun")?;
    let result = operator_metrics(&store, &record, period)
        .map_err(|e| format!("the records could not be read ({e})"));
    let outcome = if result.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Failed
    };
    trail.finish("wiki-metrics", &resource, outcome, "done")?;
    writeln!(out, "{}", result?.json_line())
        .map_err(|e| format!("the result could not be written ({})", e.kind()))
}

/// The audit trail of a start-up sweep: one `creation-sweep` event **before** each removal (*allowed*, the wiki's
/// id, never its name; the sweep removes nothing if the trail cannot take it, S4) and its outcome after (*allowed*
/// when removed, *failed* for a creation that was left behind). A failure to write the outcome comes back as a line
/// for the operator: the act has happened, and saying so is all that is left.
struct SweepTrail<'a> {
    trail: Trail<'a>,
    problems: std::cell::RefCell<Vec<String>>,
}

impl<'a> SweepTrail<'a> {
    fn new(sink: &'a dyn AuditSink, now: i64) -> Self {
        SweepTrail {
            trail: Trail { sink, now },
            problems: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// The outcome lines that could not be written.
    fn problems(self) -> Vec<String> {
        self.problems.into_inner()
    }
}

impl SweepAudit for SweepTrail<'_> {
    fn before(&self, id: &WikiId) -> Result<(), String> {
        self.trail
            .begin("creation-sweep", &format!("wiki:{id}"), "removal begun")
    }

    fn after(&self, id: &WikiId, removed: bool) {
        let (outcome, detail) = if removed {
            (AuditOutcome::Allowed, "unfinished creation removed")
        } else {
            (AuditOutcome::Failed, "unfinished creation not removed")
        };
        if let Err(line) =
            self.trail
                .finish("creation-sweep", &format!("wiki:{id}"), outcome, detail)
        {
            self.problems
                .borrow_mut()
                .push(format!("wiki {id}: {line}"));
        }
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
    let signer = Signer::open_creating_store(signer_config(settings, &env), Arc::new(NoLedger))
        .map_err(|e| format!("the signer could not be opened: {e}"))?;
    let swept = signer.leftover_call_directories_removed();
    if swept > 0 {
        let _ = writeln!(
            err,
            "inklkunn: removed {swept} leftover key directories of a call that was killed; look at how the service stopped"
        );
    }
    // What an earlier, killed creation left goes first.
    let trail = SweepTrail::new(&store, now);
    let report = sweep_unfinished_creations(
        &store,
        &store,
        &env.layout,
        &signer,
        &env.storage,
        fresh,
        &trail,
    )?;
    report_sweep(&report, err);
    for line in trail.problems() {
        let _ = writeln!(err, "{line}");
    }
    let trail = Trail { sink: &store, now };
    trail.begin("wiki-create", "wiki:new", "begun")?;
    let created = create_wiki(
        &store,
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
    let store = Arc::new(existing_store(settings)?);
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
        let maintained = Maintained::over(
            settings,
            &env.sandbox,
            &env.layout,
            &store,
            format!("inklkunn-cli-{}", std::process::id()),
        );
        refresh_checkpoint(&store, &maintained, &env.storage, &record, now)?;
        have = wiki_state(&env.storage, &env.layout, &record);
    }
    let have = have.map_err(|e| e.to_string())?;
    let mut plan = plan_folder(&folder, &have, replace_existing).map_err(|e| e.to_string())?;
    // The knowledge files of a bundle that declares the extension are judged as an acceptance judges them (RFC 005 part 2
    // D-12), so a dry run says what the run would refuse. The service's own path judges them again before the signer.
    let reads = Arc::new(ReadService::new(
        Arc::new(env.storage.clone()),
        store.clone(),
        store.clone(),
        env.layout.clone(),
        ReadConfig::default(),
        None,
    ));
    judge_knowledge(&mut plan, &reads, &env.storage, &env.layout, &record)
        .map_err(|e| e.to_string())?;
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
    // The service's own path (RFC 004 D-14): the wiki's write lease, taken on the lock a running server holds
    // too, the record of the act, the signer. The command line has no other way into a history.
    let write_leases = WriteLeases::new(env.layout.clone());
    let ledger = Arc::new(StoreLedger::new(store.clone(), store.clone()));
    let signer = Arc::new(
        Signer::open(signer_config(settings, &env), ledger)
            .map_err(|e| format!("the signer could not be opened: {e}"))?
            .with_leases(&write_leases),
    );
    let leases = WikiLeases::new(
        write_leases,
        store.clone(),
        format!("inklkunn-cli-{}", std::process::id()),
        LeaseConfig::default(),
    );
    let storage: Arc<dyn Storage> = Arc::new(
        PrikkCli::open(&settings.prikk, env.sandbox.clone(), admission())
            .map_err(|e| format!("prikk is not usable ({e})"))?,
    );
    let acts = SystemActs::new(
        Authorizer::new(
            Arc::new(StoreAuthz::new(store.clone(), store.clone())),
            store.clone(),
        ),
        store.clone(),
        store.clone(),
        storage,
        env.layout.clone(),
        signer,
        leases,
    )
    .with_reads(reads);
    let resource = wiki_resource(&record);
    let trail = Trail { sink: &*store, now };
    trail.begin("wiki-import", &resource, "begun")?;
    match run_import(&acts, &record, &plan) {
        Ok(done) => {
            let _ = writeln!(
                out,
                "imported {} file(s) as one change, block {}",
                done.files, done.sealed.block_id
            );
            trail.finish("wiki-import", &resource, AuditOutcome::Allowed, "imported")
        }
        Err(e) => {
            // Whatever state the service says the repository is in, the trail records that the act ended badly.
            let detail = match &e {
                ImportError::Act(SystemActFailure::Incomplete {
                    reason: RecoveryReason::SealedUnconfirmed,
                    ..
                }) => "sealed but not confirmed; do not repeat",
                ImportError::Act(SystemActFailure::Incomplete { .. }) => {
                    "committed or interrupted; the wiki needs recovery"
                }
                ImportError::Act(SystemActFailure::NeedsRecovery(_)) => "the wiki needs recovery",
                ImportError::Act(SystemActFailure::Busy(_)) => "the wiki is busy",
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
    let store = Arc::new(existing_store(settings)?);
    let record = find_wiki(&store, wiki)?;
    let env = environment(settings, err)?;
    let resource = wiki_resource(&record);
    let trail = Trail { sink: &*store, now };
    let maintained = Maintained::over(
        settings,
        &env.sandbox,
        &env.layout,
        &store,
        format!("inklkunn-cli-{}", std::process::id()),
    );
    let mut failure: Option<String> = None;
    // The claim threads and the views' creation records are the service's (RFC 005 part 2 D-11, D-12): the bundle's
    // knowledge extension. Read before the export is begun in the trail, so a failure to read them leaves nothing begun.
    let records = if bundle.is_some() {
        knowledge_records(&*store, &*store, &*store, &record.id).map_err(|e| e.to_string())?
    } else {
        inklkunn_core::export::KnowledgeRecords::default()
    };
    if let Some(dest) = &bundle {
        trail.begin("wiki-export-bundle", &resource, "begun")?;
        let mut result = export_bundle_with(
            &env.storage,
            &env.layout,
            &record,
            dest,
            env!("CARGO_PKG_VERSION"),
            now,
            &records,
        );
        if matches!(result, Err(ExportError::CheckpointNotRecorded)) {
            // The record `extract` anchors on is missing (a prikk upgrade, a restore): bring it back, once.
            let _ = writeln!(
                err,
                "inklkunn: prikk's checkpoint record is missing; refreshing it before the export"
            );
            match refresh_checkpoint(&store, &maintained, &env.storage, &record, now) {
                Ok(()) => {
                    result = export_bundle_with(
                        &env.storage,
                        &env.layout,
                        &record,
                        dest,
                        env!("CARGO_PKG_VERSION"),
                        now,
                        &records,
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
                if done.knowledge_files > 0 || done.threads > 0 || done.views > 0 {
                    let _ = writeln!(
                        out,
                        "  knowledge (extension inklkunn-knowledge@1): {} claim and source file(s), {} claim thread(s), {} view(s)",
                        done.knowledge_files, done.threads, done.views
                    );
                }
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
        // Under the wiki's lease, with its holder record and its place in the queue, like every act that writes in
        // the repository (corrections P9).
        match maintained
            .leases
            .export_archive(&maintained.maintenance, &record.id, dest)
        {
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

/// Recovery over the deployment's real parts: the real signer over the real key store, the wiki's write lease on the
/// lock a running server holds too, the store's records, the audit trail. `holder` names this process in the lease
/// record (two processes with one name could clear each other's record).
fn recovery_for(
    settings: &Settings,
    sandbox: &Sandbox,
    storage: &PrikkCli,
    store: &Arc<SqliteStore>,
    layout: &RootLayout,
    holder: String,
) -> Result<Recovery, String> {
    make_private_directory(&settings.seed_dir).map_err(|e| {
        format!(
            "the seed directory could not be made private ({})",
            e.kind()
        )
    })?;
    let write_leases = WriteLeases::new(layout.clone());
    let ledger = Arc::new(StoreLedger::new(store.clone(), store.clone()));
    let config = SignerConfig::new(
        settings.prikk.clone(),
        sandbox.clone(),
        layout.clone(),
        &settings.key_dir,
        &settings.secret_file,
        &settings.seed_dir,
    );
    let signer = Arc::new(
        Signer::open(config, ledger)
            .map_err(|e| format!("the signer could not be opened: {e}"))?
            .with_leases(&write_leases),
    );
    let leases = WikiLeases::new(write_leases, store.clone(), holder, LeaseConfig::default());
    Ok(Recovery::new(RecoveryParts {
        acceptances: store.clone(),
        acts: store.clone(),
        proposals: store.clone(),
        storage: Arc::new(storage.clone()),
        witnesses: Arc::new(Witnesses::new(
            settings.prikk.clone(),
            sandbox.clone(),
            layout.clone(),
        )),
        signer: signer as Arc<dyn Recover>,
        leases,
        layout: layout.clone(),
        audit: store.clone(),
    }))
}

fn recover(
    settings: &Settings,
    now: i64,
    wiki: &str,
    plan_only: bool,
    discard_queued_patch: Option<&str>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), String> {
    let store = Arc::new(existing_store(settings)?);
    let record = find_wiki(&store, wiki)?;
    let env = environment(settings, err)?;
    let recovery = recovery_for(
        settings,
        &env.sandbox,
        &env.storage,
        &store,
        &env.layout,
        format!("inklkunn-cli-{}", std::process::id()),
    )?;
    let resource = wiki_resource(&record);
    // A plan shown changes nothing, so it is not an act to audit; a recovery is, before and after.
    let trail = Trail { sink: &*store, now };
    if !plan_only {
        // The person's word is part of the act: what they asked is in the trail before anything is done. It says
        // *asked*: whether the patch is taken back is decided by what recovery then finds, and the end event says
        // what happened (M4).
        trail.begin(
            "wiki-recover",
            &resource,
            &match discard_queued_patch {
                Some(id) => format!("begun; a person asked to take queued patch {id} back"),
                None => "begun".to_owned(),
            },
        )?;
    }
    let mode = if plan_only {
        RecoveryMode::PlanOnly
    } else {
        RecoveryMode::Run
    };
    let options = RecoverOptions {
        discard_queued_patch: discard_queued_patch.map(str::to_owned),
    };
    let result = recovery.recover_with(&record, mode, &options);
    let (outcome, detail, message) = match &result {
        Ok(report) => {
            for line in report.lines() {
                let _ = writeln!(out, "{line}");
            }
            match &report.outcome {
                RecoveryOutcome::Stopped { .. } => (
                    AuditOutcome::Failed,
                    "stopped",
                    Some(
                        "recovery stopped and the wiki stays flagged: the plan above says why and what a person decides"
                            .to_owned(),
                    ),
                ),
                RecoveryOutcome::NothingToDo => (AuditOutcome::Allowed, "nothing to recover", None),
                RecoveryOutcome::Planned => (AuditOutcome::Allowed, "planned", None),
                RecoveryOutcome::Recovered => (AuditOutcome::Allowed, "recovered", None),
            }
        }
        Err(RecoveryError::Busy(_)) => (
            AuditOutcome::Failed,
            "the wiki is busy",
            result.as_ref().err().map(ToString::to_string),
        ),
        Err(e) => (AuditOutcome::Failed, "failed", Some(e.to_string())),
    };
    if !plan_only {
        // The end event says what happened to the word, too: a take-back that never happened is not left standing in
        // the trail as one (M4).
        let detail = match (discard_queued_patch, discard_word_fate(&result)) {
            (Some(id), Some(why)) => {
                format!("{detail}; --discard-queued-patch {id} was not used: {why}")
            }
            _ => detail.to_owned(),
        };
        trail.finish("wiki-recover", &resource, outcome, &detail)?;
    }
    message.map_or(Ok(()), Err)
}

/// Why a patch id typed with `--discard-queued-patch` was not used, when it was not: `None` when the patch was taken
/// back. **The sentence is derived from the steps that finished** (maintenance M18), never from what the plan meant to
/// do: a take-back that happened is not denied because a later write failed, and a lock that was not cleared is not
/// reported as cleared. The plan's own reason is used only for a run that took no step to explain it (it planned no
/// take-back at all).
fn discard_word_fate(result: &Result<RecoveryReport, RecoveryError>) -> Option<String> {
    const TAKEN_BACK: &str = "discard-queued";
    let report = match result {
        Ok(report) => report,
        Err(e) if e.steps_done().contains(&TAKEN_BACK) => return None,
        Err(_) => {
            return Some("recovery ended in an error before it took the patch back".to_owned());
        }
    };
    if report.done.iter().any(|d| d.step == TAKEN_BACK) {
        return None;
    }
    if let RecoveryOutcome::Stopped {
        at: Some(step),
        why,
    } = &report.outcome
    {
        return Some(match *step {
            // The step's reason is a whole sentence of what the clear and the check after it did (M22).
            "clear-locks" => why.clone(),
            TAKEN_BACK => format!("taking the patch back failed: {why}"),
            other => format!("recovery stopped at {other} before it took the patch back"),
        });
    }
    if report.plan.steps.iter().any(|s| s.name() == TAKEN_BACK) {
        return Some("recovery stopped before it took the patch back".to_owned());
    }
    Some(
        report
            .plan
            .discard_unused
            .clone()
            .unwrap_or_else(|| "recovery planned no take-back".to_owned()),
    )
}

/// The prikk version the driver found, as the record keeps it.
fn prikk_version(storage: &PrikkCli) -> Option<PrikkVersion> {
    PrikkVersion::parse(storage.version())
}

/// The keyless maintenance handle (the checkpoint refresh, the archive export) together with the wiki leases it
/// runs inside (corrections P9): a maintenance act writes in the repository, so it takes the wiki's lease like an
/// acceptance does, with the holder record in the store and its place in the queue, and never meets a lock that
/// another act of this deployment holds.
pub(crate) struct Maintained {
    pub(crate) leases: Arc<WikiLeases>,
    pub(crate) maintenance: Maintenance,
}

impl Maintained {
    /// Maintenance over `layout`, leased through `store`'s holder records under the name `holder` (two
    /// processes with one name could clear each other's record).
    pub(crate) fn over(
        settings: &Settings,
        sandbox: &Sandbox,
        layout: &RootLayout,
        store: &Arc<SqliteStore>,
        holder: String,
    ) -> Self {
        let write_leases = WriteLeases::new(layout.clone());
        let maintenance = Maintenance::new(settings.prikk.clone(), sandbox.clone(), layout.clone())
            .with_leases(&write_leases);
        let leases = WikiLeases::new(write_leases, store.clone(), holder, LeaseConfig::default());
        Maintained {
            leases,
            maintenance,
        }
    }
}

/// Refresh prikk's checkpoint record for one wiki (the keyless writable mode), and record it: the wiki record
/// says which prikk version did it, and the request is cleared. Audited like any act on a repository, and done
/// under the wiki's lease. `Ok` only when the refresh ran to a passing, complete verification.
fn refresh_checkpoint(
    store: &SqliteStore,
    maintained: &Maintained,
    storage: &PrikkCli,
    record: &WikiRecord,
    now: i64,
) -> Result<(), String> {
    let version = prikk_version(storage)
        .ok_or_else(|| "prikk's version text is not usable in a record".to_owned())?;
    let resource = wiki_resource(record);
    let trail = Trail { sink: store, now };
    trail.begin("checkpoint-refresh", &resource, "begun")?;
    let outcome = match maintained
        .leases
        .refresh_checkpoint(&maintained.maintenance, &record.id)
    {
        Ok(v) if v.verdict.ok => store
            .record_checkpoint_refresh(&record.id, &version)
            .map(|_| ())
            .map_err(|e| format!("the refresh ran but the wiki record could not be updated ({e})")),
        Ok(_) => Err(
            "the repository failed verification, so the checkpoint was not refreshed; run `prikk verify` in it"
                .to_owned(),
        ),
        Err(Leased::NotLeased(e)) => Err(format!(
            "the wiki's lease was not given ({e}); the refresh is tried again at the next start"
        )),
        Err(Leased::Failed(e)) => Err(e.to_string()),
    };
    let (audit, detail) = match &outcome {
        Ok(()) => (AuditOutcome::Allowed, "refreshed"),
        Err(_) => (AuditOutcome::Failed, "not refreshed"),
    };
    trail.finish("checkpoint-refresh", &resource, audit, detail)?;
    outcome
}

/// Recover every wiki that needs it, at start. One line per wiki that did; a wiki that could not be recovered says
/// why and keeps refusing writes (reads are served). Nothing here stops the server.
fn recover_at_start(
    store: &SqliteStore,
    settings: &Settings,
    sandbox: &Sandbox,
    storage: &PrikkCli,
    now: i64,
) -> Vec<String> {
    let records = match store.list() {
        Ok(r) => r,
        Err(e) => {
            return vec![format!(
                "inklkunn: the wikis could not be listed for recovery ({e})"
            )];
        }
    };
    let mut needing = Vec::new();
    for record in records {
        match needs_recovery(store, store, &record.id, None) {
            Ok(reasons) if reasons.is_empty() => {}
            Ok(reasons) => needing.push((record, reasons)),
            Err(e) => {
                return vec![format!(
                    "inklkunn: whether wiki {} needs recovery could not be read ({e})",
                    record.slug
                )];
            }
        }
    }
    if needing.is_empty() {
        return Vec::new();
    }
    let layout = RootLayout::new(settings.wikis_root());
    // A second connection to the same store, shared by the parts recovery is made of.
    let shared = match SqliteStore::open(&settings.store_path()) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            return vec![format!(
                "inklkunn: {} wiki(s) need recovery and the store could not be opened for it ({e}); they refuse writes until `inklkunn wiki recover`",
                needing.len()
            )];
        }
    };
    let recovery = match recovery_for(
        settings,
        sandbox,
        storage,
        &shared,
        &layout,
        format!("inklkunn-serve-{}", std::process::id()),
    ) {
        Ok(r) => r,
        Err(why) => {
            return vec![format!(
                "inklkunn: {} wiki(s) need recovery and it could not begin ({why}); they refuse writes until `inklkunn wiki recover`",
                needing.len()
            )];
        }
    };
    let trail = Trail {
        sink: &*shared,
        now,
    };
    let mut notes = Vec::new();
    for (record, reasons) in needing {
        let resource = wiki_resource(&record);
        let slug = &record.slug;
        if let Err(why) = trail.begin("wiki-recover", &resource, "begun at start") {
            notes.push(format!(
                "inklkunn: wiki {slug} needs recovery and was left alone: {why}"
            ));
            continue;
        }
        let reasons = reasons.join(", ");
        let (outcome, detail, note) = match recovery.recover(&record, RecoveryMode::Run) {
            Ok(report) => match &report.outcome {
                RecoveryOutcome::Recovered => (
                    AuditOutcome::Allowed,
                    "recovered",
                    format!(
                        "inklkunn: wiki {slug} needed recovery ({reasons}) and was recovered: {} step(s) taken",
                        report.done.len()
                    ),
                ),
                RecoveryOutcome::NothingToDo => (
                    AuditOutcome::Allowed,
                    "nothing to recover",
                    format!("inklkunn: wiki {slug} had nothing to recover"),
                ),
                RecoveryOutcome::Stopped { at, why } => (
                    AuditOutcome::Failed,
                    "stopped",
                    format!(
                        "inklkunn: wiki {slug} needs recovery ({reasons}) and recovery stopped{}: {why}; it refuses writes and serves reads until a person runs `inklkunn wiki recover {slug} --plan` and decides",
                        at.map(|s| format!(" at {s}")).unwrap_or_default()
                    ),
                ),
                RecoveryOutcome::Planned => (AuditOutcome::Allowed, "planned", String::new()),
            },
            Err(RecoveryError::Busy(_)) => (
                AuditOutcome::Failed,
                "the wiki is busy",
                format!(
                    "inklkunn: wiki {slug} needs recovery ({reasons}) and another process is writing it; recovery is tried again at the next start or by `inklkunn wiki recover {slug}`"
                ),
            ),
            Err(e) => (
                AuditOutcome::Failed,
                "failed",
                format!(
                    "inklkunn: wiki {slug} needs recovery ({reasons}) and recovery could not be done: {e}; it refuses writes and serves reads"
                ),
            ),
        };
        notes.push(note);
        if let Err(line) = trail.finish("wiki-recover", &resource, outcome, detail) {
            notes.push(format!("inklkunn: wiki {slug}: {line}"));
        }
    }
    notes
}

/// The work every start does before it serves (handoff 003 T7): remove the creations that were killed, and
/// refresh prikk's checkpoint record for every wiki whose last refresh ran under another prikk version or whose
/// refresh was asked for (IS-9). Failures are returned as lines for the operator; none of them stops the server,
/// because what they leave undone is housekeeping and the pages that need it say *could not be read now*.
/// `store_is_new` is true when this start made the store: the sweep then removes nothing (R9).
pub(crate) fn start_up(
    store: &SqliteStore,
    settings: &Settings,
    sandbox: &Sandbox,
    storage: &PrikkCli,
    store_is_new: bool,
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
                .and_then(|()| Signer::open(config, Arc::new(NoLedger)).map_err(|e| e.to_string()));
            match opened {
                Ok(signer) => {
                    let trail = SweepTrail::new(store, now);
                    match sweep_unfinished_creations(
                        store,
                        store,
                        &layout,
                        &signer,
                        storage,
                        store_is_new,
                        &trail,
                    ) {
                        Ok(report) => {
                            let mut text = Vec::new();
                            report_sweep(&report, &mut text);
                            notes.extend(
                                String::from_utf8_lossy(&text).lines().map(str::to_owned),
                            );
                            notes.extend(trail.problems());
                        }
                        Err(why) => notes.push(format!(
                            "inklkunn: unfinished creations could not be examined: {why}"
                        )),
                    }
                }
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
    // 1b. Recovery: a wiki whose last write did not finish (flagged, an unfinished act, a lease holder that died) is
    //     recovered before anything is served, by the same procedure as `wiki recover`. Asking is a few queries; the
    //     signer is opened only when some wiki needs it.
    notes.extend(recover_at_start(store, settings, sandbox, storage, now));
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
    // The leases need the store as a shared handle: a second connection, opened when the first wiki wants one.
    let mut maintained: Option<Maintained> = None;
    for record in records {
        let stale = record.checkpoint.refreshed_with.as_ref() != Some(&version)
            || record.checkpoint.refresh_requested_at.is_some();
        if !stale {
            continue;
        }
        // A wiki whose recovery stopped (or has not run) is left as it is: a maintenance act would write in a
        // repository that is in a state a person has yet to decide about (corrections P9).
        match needs_recovery(store, store, &record.id, None) {
            Ok(reasons) if reasons.is_empty() => {}
            Ok(reasons) => {
                notes.push(format!(
                    "inklkunn: the checkpoint record of wiki {} was not refreshed: the wiki needs recovery ({}); see `inklkunn wiki recover {} --plan`",
                    record.slug,
                    reasons.join(", "),
                    record.slug
                ));
                continue;
            }
            Err(e) => {
                notes.push(format!(
                    "inklkunn: the checkpoint record of wiki {} was not refreshed: whether it needs recovery could not be read ({e})",
                    record.slug
                ));
                continue;
            }
        }
        if maintained.is_none() {
            match SqliteStore::open(&settings.store_path()) {
                Ok(shared) => {
                    maintained = Some(Maintained::over(
                        settings,
                        sandbox,
                        &layout,
                        &Arc::new(shared),
                        format!("inklkunn-serve-{}", std::process::id()),
                    ));
                }
                Err(e) => {
                    notes.push(format!(
                        "inklkunn: the checkpoint records were not refreshed: the store could not be opened for the leases ({e})"
                    ));
                    return notes;
                }
            }
        }
        let Some(maintenance) = maintained.as_ref() else {
            return notes;
        };
        match refresh_checkpoint(store, maintenance, storage, &record, now) {
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
