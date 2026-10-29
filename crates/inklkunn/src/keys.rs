//! `inklkunn wiki keys <status|create|rotate> <wiki>` (RFC 002 D-8): a wiki's keys over time, from the
//! command line of the host that runs the service.
//!
//! **Who may do this.** Whoever can run the command on the host and read the operator's secret file: the key
//! store does not open without it, so opening it *is* the re-authentication D-8 asks for. The web
//! administration surface (RFC 003, RFC 006) will ask for the password again instead. Rotation also needs the
//! wiki's slug typed (`--confirm`), so the wrong wiki is not rotated by a slip, and retiring the old key needs
//! a second flag that says what it costs.
//!
//! **Every `create` and `rotate` is audited** (principal `system:operator-cli`, resource `wiki:<id>`, allowed
//! or failed, with the generation and what was done to the old key). `status` prints ids and dates and never
//! a secret; nothing here can print one.

use std::io::Write;
use std::sync::Arc;

use inklkunn_core::wiki::repo_of;
use inklkunn_core::{
    AcceptanceLedger, AcceptanceToken, ExpectedMessage, KeyAuthority, KeyStatus, LedgerError,
    OldKey, OldKeyOutcome, Signer, SignerConfig, ViewCreationAct, ViewCreationFacts,
};
use inklkunn_store::{
    AuditEvent, AuditOutcome, AuditSink, RootLayout, Slug, SqliteStore, WikiId, WikiRecord,
    WikiStore,
};

use crate::config::{Settings, UNCONFINED_BANNER};

/// The process's one [`KeyAuthority`], claimed the first time a key command runs and kept for the life of the
/// process (a command-line run makes one command; the tests of this module make many in one process, and reuse
/// it). Nothing outside this module can reach it: the binary's surfaces never hold the right to make or change
/// keys (corrections handoff C7).
fn key_authority() -> &'static KeyAuthority {
    static AUTHORITY: std::sync::OnceLock<KeyAuthority> = std::sync::OnceLock::new();
    AUTHORITY.get_or_init(|| {
        KeyAuthority::claim().unwrap_or_else(|| {
            // Only this module claims it. If something else did, that is a bug to see, not to paper over.
            #[allow(clippy::panic)]
            {
                panic!("the key authority was claimed elsewhere in this process")
            }
        })
    })
}

/// The operator at the host, in the audit trail.
const PRINCIPAL: &str = "system:operator-cli";

/// What was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Status {
        wiki: String,
    },
    Create {
        wiki: String,
    },
    Rotate {
        wiki: String,
        confirm: String,
        retire_old: bool,
        accept_unverifiable_history: bool,
    },
}

/// The command line was not understood (exit status 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usage(pub String);

pub(crate) const USAGE: &str = "usage: inklkunn [--dev-unconfined] wiki keys status <wiki>\n       \
inklkunn [--dev-unconfined] wiki keys create <wiki>\n       \
inklkunn [--dev-unconfined] wiki keys rotate <wiki> --confirm <slug> [--retire-old --accept-unverifiable-history]\n\
<wiki> is the wiki's slug or id.";

/// Parse what follows `wiki keys`.
pub(crate) fn parse(args: &[String]) -> Result<Command, Usage> {
    let usage = |why: &str| Usage(format!("{why}\n{USAGE}"));
    let (sub, rest) = args.split_first().ok_or_else(|| usage("which command?"))?;
    let (wiki, flags) = match rest.split_first() {
        Some((w, f)) if !w.starts_with("--") => (w.clone(), f),
        _ => return Err(usage("which wiki?")),
    };
    match sub.as_str() {
        "status" | "create" => {
            if let Some(extra) = flags.first() {
                return Err(usage(&format!("`{extra}` is not an option of `{sub}`")));
            }
            Ok(if sub == "status" {
                Command::Status { wiki }
            } else {
                Command::Create { wiki }
            })
        }
        "rotate" => {
            let mut confirm = None;
            let (mut retire_old, mut accept) = (false, false);
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--confirm" => {
                        confirm = Some(
                            it.next()
                                .ok_or_else(|| usage("--confirm needs the wiki's slug"))?
                                .clone(),
                        );
                    }
                    "--retire-old" => retire_old = true,
                    "--accept-unverifiable-history" => accept = true,
                    other => return Err(usage(&format!("`{other}` is not an option of `rotate`"))),
                }
            }
            let confirm = confirm.ok_or_else(|| {
                usage("rotating needs --confirm <slug>: type the wiki's slug to say it is this one")
            })?;
            Ok(Command::Rotate {
                wiki,
                confirm,
                retire_old,
                accept_unverifiable_history: accept,
            })
        }
        other => Err(usage(&format!("`{other}` is not a command"))),
    }
}

/// Nothing is ever pending: the key commands accept no change.
struct NoAcceptances;

impl AcceptanceLedger for NoAcceptances {
    fn pending(&self, _: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(None)
    }
    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` for Unix seconds (proleptic Gregorian, UTC).
pub(crate) fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

fn find_wiki(store: &SqliteStore, wiki: &str) -> Result<WikiRecord, String> {
    let found = match WikiId::parse(wiki) {
        Some(id) => store.get(&id),
        None => match Slug::new(wiki) {
            Ok(slug) => store.by_slug(&slug),
            Err(e) => return Err(format!("`{wiki}` is neither a wiki id nor a slug ({e})")),
        },
    };
    found.map_err(|e| format!("no wiki `{wiki}` ({e})"))
}

fn print_status(out: &mut dyn Write, record: &WikiRecord, s: &KeyStatus) {
    let _ = writeln!(out, "wiki:        {} ({})", record.slug, record.id);
    let _ = writeln!(out, "author key:      {}", s.author_key_id);
    let _ = writeln!(out, "maintainer key:  {}", s.maintainer_key_id);
    let _ = writeln!(out, "generation:      {}", s.generation);
    let _ = writeln!(out, "created:         {}", rfc3339(s.created_at));
    let _ = writeln!(
        out,
        "rotated:         {}",
        s.rotated_at.map_or_else(|| "never".to_owned(), rfc3339)
    );
}

/// Run a parsed command. Returns the process exit status: 0, or 1 for a refusal or a failure.
pub(crate) fn execute(
    command: &Command,
    settings: &Settings,
    now: i64,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    execute_with(command, settings, now, out, err, None)
}

/// As [`execute`], with the audit trail named: the store's own, or (a test) another sink.
pub(crate) fn execute_with(
    command: &Command,
    settings: &Settings,
    now: i64,
    out: &mut dyn Write,
    err: &mut dyn Write,
    trail: Option<&dyn AuditSink>,
) -> i32 {
    match run(command, settings, now, out, err, trail) {
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
    trail: Option<&dyn AuditSink>,
) -> Result<(), String> {
    let (wiki_arg, rotating) = match command {
        Command::Status { wiki } | Command::Create { wiki } => (wiki, None),
        Command::Rotate { wiki, .. } => (wiki, Some(command)),
    };

    // The deployment must exist: a typo in a directory name must not make a fresh, empty one.
    let store_path = settings.store_path();
    if !store_path.is_file() {
        return Err(format!(
            "there is no store at {} (INKLKUNN_DATA_DIR); this command works on a deployment that has been started",
            store_path.display()
        ));
    }
    if !settings.key_dir.is_dir() {
        return Err(format!(
            "there is no key directory at {} (INKLKUNN_KEY_DIR)",
            settings.key_dir.display()
        ));
    }
    let store = SqliteStore::open(&store_path)
        .map_err(|e| format!("the store could not be opened: {e}"))?;
    let record = find_wiki(&store, wiki_arg)?;
    let trail: &dyn AuditSink = trail.unwrap_or(&store);

    // Refusals that need nothing but the command line, before any key is touched.
    if let Some(Command::Rotate {
        confirm,
        retire_old,
        accept_unverifiable_history,
        ..
    }) = rotating
    {
        if confirm != record.slug.as_str() {
            return Err(format!(
                "--confirm `{confirm}` is not the slug of that wiki (`{}`); nothing was changed",
                record.slug
            ));
        }
        if *retire_old && !accept_unverifiable_history {
            return Err(
                "--retire-old removes the old maintainer key from the repository's trust: everything it sealed \
                 then fails prikk's verification (the wiki shows `Verification failed`) until prikk can keep a \
                 retired key's history verifiable. If that is what you want — a compromise — add \
                 --accept-unverifiable-history. Nothing was changed."
                    .to_owned(),
            );
        }
    }

    let sandbox = settings.sandbox().map_err(|e| e.to_string())?;
    if settings.dev_unconfined {
        let _ = writeln!(err, "{UNCONFINED_BANNER}");
    }
    make_private_directory(&settings.seed_dir).map_err(|e| {
        format!(
            "the seed directory could not be made private ({})",
            e.kind()
        )
    })?;
    let layout = RootLayout::new(settings.wikis_root());
    let repo = repo_of(&layout, &record);
    let config = SignerConfig::new(
        settings.prikk.clone(),
        sandbox,
        layout,
        &settings.key_dir,
        &settings.secret_file,
        &settings.seed_dir,
    );
    // Only `create` is asked to bring a key store into being; every other command refuses a key directory with
    // no store in it (a mistyped path must not become a new, empty store).
    let opened = if matches!(command, Command::Create { .. }) {
        Signer::open_creating_store(config, Arc::new(NoAcceptances))
    } else {
        Signer::open(config, Arc::new(NoAcceptances))
    };
    let signer = opened.map_err(|e| format!("the signer could not be opened: {e}"))?;
    let swept = signer.leftover_call_directories_removed();
    if swept > 0 {
        let _ = writeln!(
            err,
            "inklkunn: removed {swept} leftover key directories of a call that was killed; look at how the service stopped"
        );
    }

    match command {
        Command::Status { .. } => {
            let status = signer.key_status(&record.id).map_err(|e| e.to_string())?;
            print_status(out, &record, &status);
            Ok(())
        }
        Command::Create { .. } => {
            // The trail first: an act that cannot be recorded is not begun (the Authorizer's rule too).
            begin(trail, &record, now, "keys-create", "begun")?;
            match signer.create_keys(key_authority(), &repo) {
                Ok(ids) => {
                    let recorded = record_key_ids(&store, &record, &ids.author, &ids.maintainer);
                    let status = signer.key_status(&record.id).map_err(|e| e.to_string());
                    let done = finish(
                        trail,
                        &record,
                        now,
                        "keys-create",
                        AuditOutcome::Allowed,
                        &format!(
                            "generation 1{}",
                            if recorded.is_err() {
                                "; the wiki record was not updated"
                            } else {
                                ""
                            }
                        ),
                    );
                    print_status(out, &record, &status?);
                    recorded?;
                    done
                }
                Err(e) => {
                    let done = finish(
                        trail,
                        &record,
                        now,
                        "keys-create",
                        AuditOutcome::Failed,
                        "refused or failed",
                    );
                    done.and(Err(e.to_string()))
                }
            }
        }
        Command::Rotate { retire_old, .. } => {
            let old = if *retire_old {
                OldKey::Retire
            } else {
                OldKey::Keep
            };
            begin(
                trail,
                &record,
                now,
                "keys-rotate",
                if *retire_old {
                    "begun; old key to be retired"
                } else {
                    "begun; old key to be kept"
                },
            )?;
            match signer.rotate_keys(key_authority(), &repo, old) {
                Ok(r) => {
                    // The key store changed: that is what the wiki record, the trail and the output must
                    // follow, whatever became of the old key.
                    let recorded = record_key_ids(
                        &store,
                        &record,
                        &r.status.author_key_id,
                        &r.status.maintainer_key_id,
                    );
                    let what = match &r.old_key {
                        OldKeyOutcome::Kept => "old key kept".to_owned(),
                        OldKeyOutcome::Retired => "old key retired".to_owned(),
                        OldKeyOutcome::RetireFailed(_) => {
                            "old key retirement FAILED, the old key is still trusted".to_owned()
                        }
                    };
                    let done = finish(
                        trail,
                        &record,
                        now,
                        "keys-rotate",
                        AuditOutcome::Allowed,
                        &format!(
                            "generation {}; {what}{}",
                            r.status.generation,
                            if recorded.is_err() {
                                "; the wiki record was not updated"
                            } else {
                                ""
                            }
                        ),
                    );
                    print_status(out, &record, &r.status);
                    let previous = &r.previous_maintainer_key_id;
                    let outcome = match &r.old_key {
                        OldKeyOutcome::Kept => {
                            let _ = writeln!(
                                out,
                                "previous maintainer key {previous} is still trusted: history it sealed keeps verifying, \
                                 the service no longer holds its seed, and a copy of that seed somewhere else could still \
                                 seal changes that verify"
                            );
                            Ok(())
                        }
                        OldKeyOutcome::Retired => {
                            let _ = writeln!(
                                out,
                                "previous maintainer key {previous} was removed from the repository's trust; what it sealed no longer verifies"
                            );
                            Ok(())
                        }
                        OldKeyOutcome::RetireFailed(why) => Err(format!(
                            "the keys were rotated (generation {}) but the old maintainer key `{previous}` is STILL TRUSTED: \
                             removing it failed ({why}); remove it with `prikk trust maintainer remove --key-id {previous}` \
                             in the wiki's repository",
                            r.status.generation
                        )),
                    };
                    recorded?;
                    outcome?;
                    done
                }
                Err(e) => {
                    let done = finish(
                        trail,
                        &record,
                        now,
                        "keys-rotate",
                        AuditOutcome::Failed,
                        "refused or failed",
                    );
                    done.and(Err(e.to_string()))
                }
            }
        }
    }
}

fn event(
    record: &WikiRecord,
    now: i64,
    action: &str,
    outcome: AuditOutcome,
    detail: &str,
) -> AuditEvent {
    AuditEvent::new(
        now,
        PRINCIPAL,
        action,
        &format!("wiki:{}", record.id),
        outcome,
        detail,
    )
}

/// Write the event that says an act is **about to** be done. If the trail cannot take it the act is not
/// begun: nothing is changed that cannot be accounted for.
fn begin(
    store: &dyn AuditSink,
    record: &WikiRecord,
    now: i64,
    action: &str,
    detail: &str,
) -> Result<(), String> {
    store
        .record(&event(record, now, action, AuditOutcome::Allowed, detail))
        .map_err(|e| format!("the audit trail could not be written ({e}); nothing was done"))
}

/// Write the event that says how an act ended. The act has happened either way, so a failure here is an
/// error the operator sees (the exit status is not 0), never a quiet warning.
fn finish(
    store: &dyn AuditSink,
    record: &WikiRecord,
    now: i64,
    action: &str,
    outcome: AuditOutcome,
    detail: &str,
) -> Result<(), String> {
    store
        .record(&event(record, now, action, outcome, detail))
        .map_err(|e| {
            format!(
                "the act was done but its completion could not be written to the audit trail ({e}); \
                 the trail shows it as begun and not finished"
            )
        })
}

/// The seed directory, made if it is not there and set to mode 0700: the signer refuses one that others can
/// write to.
pub(crate) fn make_private_directory(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Keep the wiki record's key ids (which the administration surface shows) equal to the key store's.
fn record_key_ids(
    store: &SqliteStore,
    record: &WikiRecord,
    author: &str,
    maintainer: &str,
) -> Result<(), String> {
    store
        .set_keys(
            &record.id,
            Some(author.to_owned()),
            Some(maintainer.to_owned()),
            record.prikk_format_version.clone(),
        )
        .map(|_| ())
        .map_err(|e| {
            format!(
                "the keys changed but the wiki record could not be updated ({e}); the key store is the truth, \
                 run `inklkunn wiki keys status` to read it"
            )
        })
}

#[cfg(test)]
mod tests;
