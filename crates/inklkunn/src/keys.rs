//! `inklkunn wiki keys <status|create|rotate|retire|finish-retirement|abandon-retirement> <wiki>` (RFC 002 D-8): a wiki's
//! keys over time, from the command line of the host that runs the service.
//!
//! **Who may do this.** Whoever can run the command on the host and read the operator's secret file: the key
//! store does not open without it, so opening it *is* the re-authentication D-8 asks for. The web
//! administration surface (RFC 003, RFC 006) will ask for the password again instead. Rotation also needs the
//! wiki's slug typed (`--confirm`), so the wrong wiki is not rotated by a slip, and retiring the old key needs
//! a second flag that says what it costs.
//!
//! **A retirement that ends without its record** (`rotate --retire-old` exits non-zero and says so) pauses every new
//! seal of the wiki (handoff 004, W1) and has two ways out. `finish-retirement <wiki>` does whatever step remains
//! (W2): with the old key still trusted it takes a fresh `verify`, removes the key itself and records what the
//! removal made fail; with the key already removed it records from the state the retirement kept, while nothing has
//! been written and no other key was added or removed since. `abandon-retirement <wiki> --confirm <slug>` (W3) gives
//! the retirement up: the old key is trusted again (adopted again if it had been removed) and **nothing is retired**;
//! `retire <wiki> --key <id> --confirm <slug>` (X1) retires it later, and any other superseded key that is still
//! trusted, by its id. Abandon never adopts again a key whose retirement the audit trail records as finished (X2).
//! Both need the same operator rights as the other key commands and are audited the same way. **Whichever of
//! `rotate --retire-old`, `finish-retirement` and `abandon-retirement` ends, it ends by recording a fresh verdict**
//! through the check the server uses (W6), or, when that cannot run, by clearing the stored explanation of the last
//! verdict.
//!
//! **Every `create`, `rotate`, `retire`, `finish-retirement` and `abandon-retirement` is audited** (principal `system:operator-cli`, resource `wiki:<id>`, allowed
//! or failed, with the generation and what was done to the old key). `status` prints ids and dates and never
//! a secret; nothing here can print one.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::wiki::repo_of;
use inklkunn_core::{
    ACTION_FINISH_RETIREMENT, ACTION_RETIRE, ACTION_ROTATE, AcceptanceLedger, AcceptanceToken,
    ExpectedMessage, KeyAuthority, KeyStatus, LATE_SEALS_SENTENCE, LedgerError, OldKey,
    OldKeyOutcome, Signer, SignerConfig, Verifier, ViewCreationAct, ViewCreationFacts,
    recorded_detail, retire_command,
};
use inklkunn_prikk::{Admission, PrikkCli};
use inklkunn_store::record::rfc3339;
use inklkunn_store::{
    AuditEvent, AuditOutcome, AuditSink, RootLayout, Slug, SqliteStore, WikiId, WikiRecord,
    WikiStore,
};

use crate::config::{Settings, UNCONFINED_BANNER};

/// The process's one [`KeyAuthority`], claimed the first time a key command runs and kept for the life of the
/// process (a command-line run makes one command; the tests of this module make many in one process, and reuse
/// it). Nothing outside this module can reach it: the binary's surfaces never hold the right to make or change
/// keys (corrections handoff C7).
pub(crate) fn key_authority() -> &'static KeyAuthority {
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
    Retire {
        wiki: String,
        key: String,
        confirm: String,
    },
    FinishRetirement {
        wiki: String,
    },
    AbandonRetirement {
        wiki: String,
        confirm: String,
    },
}

/// The command line was not understood (exit status 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usage(pub String);

pub(crate) const USAGE: &str = "usage: inklkunn [--dev-unconfined] wiki keys status <wiki>\n       \
inklkunn [--dev-unconfined] wiki keys create <wiki>\n       \
inklkunn [--dev-unconfined] wiki keys rotate <wiki> --confirm <slug> [--retire-old --accept-unverifiable-history]\n       \
inklkunn [--dev-unconfined] wiki keys retire <wiki> --key <maintainer key id> --confirm <slug>\n       \
inklkunn [--dev-unconfined] wiki keys finish-retirement <wiki>\n       \
inklkunn [--dev-unconfined] wiki keys abandon-retirement <wiki> --confirm <slug>\n\
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
        "status" | "create" | "finish-retirement" => {
            if let Some(extra) = flags.first() {
                return Err(usage(&format!("`{extra}` is not an option of `{sub}`")));
            }
            Ok(match sub.as_str() {
                "status" => Command::Status { wiki },
                "create" => Command::Create { wiki },
                _ => Command::FinishRetirement { wiki },
            })
        }
        "retire" => {
            let (mut key, mut confirm) = (None, None);
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--key" => {
                        key = Some(
                            it.next()
                                .ok_or_else(|| usage("--key needs a maintainer key id"))?
                                .clone(),
                        );
                    }
                    "--confirm" => {
                        confirm = Some(
                            it.next()
                                .ok_or_else(|| usage("--confirm needs the wiki's slug"))?
                                .clone(),
                        );
                    }
                    other => {
                        return Err(usage(&format!("`{other}` is not an option of `retire`")));
                    }
                }
            }
            let key = key.ok_or_else(|| {
                usage("retiring needs --key <maintainer key id>: the superseded key to take out of the repository's trust (`status` lists them)")
            })?;
            let confirm = confirm.ok_or_else(|| {
                usage("retiring needs --confirm <slug>: type the wiki's slug to say it is this one")
            })?;
            Ok(Command::Retire { wiki, key, confirm })
        }
        "abandon-retirement" => {
            let mut confirm = None;
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
                    other => {
                        return Err(usage(&format!(
                            "`{other}` is not an option of `abandon-retirement`"
                        )));
                    }
                }
            }
            let confirm = confirm.ok_or_else(|| {
                usage(
                    "abandoning needs --confirm <slug>: type the wiki's slug to say it is this one",
                )
            })?;
            Ok(Command::AbandonRetirement { wiki, confirm })
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

pub(crate) fn find_wiki(store: &SqliteStore, wiki: &str) -> Result<WikiRecord, String> {
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
        Command::Status { wiki }
        | Command::Create { wiki }
        | Command::FinishRetirement { wiki }
        | Command::Retire { wiki, .. }
        | Command::AbandonRetirement { wiki, .. } => (wiki, None),
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
    if let Command::AbandonRetirement { confirm, .. } | Command::Retire { confirm, .. } = command
        && confirm != record.slug.as_str()
    {
        return Err(format!(
            "--confirm `{confirm}` is not the slug of that wiki (`{}`); nothing was changed",
            record.slug
        ));
    }
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
    let verdict_sandbox = sandbox.clone();
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
    let signer = Arc::new(opened.map_err(|e| format!("the signer could not be opened: {e}"))?);
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
            if let Some(key) = signer
                .unfinished_retirement(&record.id)
                .map_err(|e| e.to_string())?
            {
                let _ = writeln!(
                    out,
                    "UNFINISHED: the retirement of maintainer key {key} was begun and is not finished, so nothing new is sealed in this wiki, nothing is excused \
                     and the wiki's verification may fail for what that key sealed; finish it with `{}`, or give it up with `{}` \
                     (the key stays trusted and nothing is retired; `{}` retires it later)",
                    inklkunn_core::finish_retirement_command(&record.id),
                    inklkunn_core::abandon_retirement_command(&record.id),
                    retire_command(&record.id, &key)
                );
            }
            let late = report_superseded(&signer, &record, &repo, out, err)?;
            if late.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "{} block(s) were sealed by a superseded maintainer key after the rotation that replaced it; see above",
                    late.len()
                ))
            }
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
        Command::FinishRetirement { .. } => {
            // The trail first, as for the other key commands: an act that cannot be recorded is not begun.
            begin(
                trail,
                &record,
                now,
                ACTION_FINISH_RETIREMENT,
                "begun; an unfinished retirement is to be finished from whatever step remains",
            )?;
            match signer.finish_retirement(key_authority(), &repo) {
                Ok(done) => {
                    // The key's id comes first, in the fixed form abandon reads back from the trail (X2).
                    let detail = recorded_detail(
                        &done.maintainer_key_id,
                        &format!(
                            "finished; {} finding(s) recorded{}{}",
                            done.findings_recorded,
                            if done.removed_now {
                                "; the key was removed from the repository's trust by this command"
                            } else {
                                ""
                            },
                            late_note(&done.late_seals)
                        ),
                    );
                    let finished = finish(
                        trail,
                        &record,
                        now,
                        ACTION_FINISH_RETIREMENT,
                        AuditOutcome::Allowed,
                        &detail,
                    );
                    let _ = writeln!(
                        out,
                        "the retirement of maintainer key {} is finished{}: {} finding(s) of publication trust are recorded, \
                         and a later failure made of exactly those is excused (the badge still says Verification failed)",
                        done.maintainer_key_id,
                        if done.removed_now {
                            " (this command removed the key from the repository's trust)"
                        } else {
                            ""
                        },
                        done.findings_recorded
                    );
                    let late = say_late_seals(out, &done.maintainer_key_id, &done.late_seals);
                    refresh_verdict(
                        settings,
                        verdict_sandbox,
                        &signer,
                        &record,
                        &store,
                        out,
                        err,
                    );
                    late?;
                    finished
                }
                Err(e) => {
                    let done = finish(
                        trail,
                        &record,
                        now,
                        ACTION_FINISH_RETIREMENT,
                        AuditOutcome::Failed,
                        "refused or failed",
                    );
                    refresh_verdict(
                        settings,
                        verdict_sandbox,
                        &signer,
                        &record,
                        &store,
                        out,
                        err,
                    );
                    done.and(Err(e.to_string()))
                }
            }
        }
        Command::Retire { key, .. } => {
            // The trail first, as for the other key commands: an act that cannot be recorded is not begun.
            begin(
                trail,
                &record,
                now,
                ACTION_RETIRE,
                &format!("begun; superseded maintainer key {key} is to be retired"),
            )?;
            match signer.retire_key(key_authority(), &repo, key) {
                Ok(outcome) => {
                    let detail = match &outcome {
                        // The key's id comes first, in the fixed form abandon reads back from the trail (X2).
                        OldKeyOutcome::Retired { late_seals } => recorded_detail(
                            key,
                            &format!("retired by `retire`{}", late_note(late_seals)),
                        ),
                        OldKeyOutcome::RetiredUnrecorded(_) => format!(
                            "key {key} removed from the repository's trust; its findings are NOT recorded (finish-retirement or abandon-retirement)"
                        ),
                        OldKeyOutcome::RetireFailed(_) => {
                            format!("retirement of key {key} FAILED, the key is still trusted")
                        }
                        OldKeyOutcome::Kept | OldKeyOutcome::RetireNotBegun(_) => {
                            format!("retirement of key {key} was not begun")
                        }
                    };
                    let done = finish(
                        trail,
                        &record,
                        now,
                        ACTION_RETIRE,
                        AuditOutcome::Allowed,
                        &detail,
                    );
                    let ended = report_old_key(out, &outcome, key, None, &record);
                    refresh_verdict(
                        settings,
                        verdict_sandbox,
                        &signer,
                        &record,
                        &store,
                        out,
                        err,
                    );
                    ended?;
                    done
                }
                Err(e) => {
                    let done = finish(
                        trail,
                        &record,
                        now,
                        ACTION_RETIRE,
                        AuditOutcome::Failed,
                        "refused or failed",
                    );
                    done.and(Err(e.to_string()))
                }
            }
        }
        Command::AbandonRetirement { .. } => {
            begin(
                trail,
                &record,
                now,
                "keys-abandon-retirement",
                "begun; an unfinished retirement is to be given up and its old key trusted again",
            )?;
            // The real trail is read for the answer to *was this key's retirement finished* (X2), whatever sink the
            // events are written to.
            match signer.abandon_retirement(key_authority(), &repo, &store) {
                Ok(done) => {
                    let finished = finish(
                        trail,
                        &record,
                        now,
                        "keys-abandon-retirement",
                        AuditOutcome::Allowed,
                        &format!(
                            "retirement of key {} given up; nothing is retired{}",
                            done.maintainer_key_id,
                            if done.restored {
                                " (this command adopted the key in the repository's trust again)"
                            } else {
                                " (the key was trusted in the repository when this ran)"
                            }
                        ),
                    );
                    // What abandoning costs, said where the operator reads it (X1).
                    let _ = writeln!(
                        out,
                        "the old maintainer key {} is trusted{}; nothing is retired; to retire it later run `{}`",
                        done.maintainer_key_id,
                        if done.restored { " again" } else { "" },
                        retire_command(&record.id, &done.maintainer_key_id)
                    );
                    refresh_verdict(
                        settings,
                        verdict_sandbox,
                        &signer,
                        &record,
                        &store,
                        out,
                        err,
                    );
                    finished
                }
                Err(e) => {
                    let done = finish(
                        trail,
                        &record,
                        now,
                        "keys-abandon-retirement",
                        AuditOutcome::Failed,
                        "refused or failed",
                    );
                    refresh_verdict(
                        settings,
                        verdict_sandbox,
                        &signer,
                        &record,
                        &store,
                        out,
                        err,
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
                        OldKeyOutcome::Retired { late_seals } => {
                            format!("old key retired{}", late_note(late_seals))
                        }
                        OldKeyOutcome::RetiredUnrecorded(_) => {
                            "old key retired; its findings are NOT recorded (finish-retirement or abandon-retirement)".to_owned()
                        }
                        OldKeyOutcome::RetireFailed(_) => {
                            "old key retirement FAILED, the old key is still trusted".to_owned()
                        }
                        OldKeyOutcome::RetireNotBegun(_) => {
                            "old key retirement NOT BEGUN, the old key is still trusted (retire)".to_owned()
                        }
                    };
                    let detail = format!(
                        "generation {}; {what}{}",
                        r.status.generation,
                        if recorded.is_err() {
                            "; the wiki record was not updated"
                        } else {
                            ""
                        }
                    );
                    // A rotation whose retirement ended with its record made says so first, in the fixed form abandon
                    // reads back from the trail (X2).
                    let detail = if matches!(r.old_key, OldKeyOutcome::Retired { .. }) {
                        recorded_detail(&r.previous_maintainer_key_id, &detail)
                    } else {
                        detail
                    };
                    let done = finish(
                        trail,
                        &record,
                        now,
                        ACTION_ROTATE,
                        AuditOutcome::Allowed,
                        &detail,
                    );
                    print_status(out, &record, &r.status);
                    let previous = &r.previous_maintainer_key_id;
                    let outcome = report_old_key(
                        out,
                        &r.old_key,
                        previous,
                        Some(r.status.generation),
                        &record,
                    );
                    // The check runs after every rotation: it prints what it finds, and the trail keeps it.
                    let late = report_superseded(&signer, &record, &repo, out, err);
                    if let Ok(found) = &late {
                        for seal in found {
                            let _ = finish(
                                trail,
                                &record,
                                now,
                                "keys-late-seal",
                                AuditOutcome::Failed,
                                &seal.to_string(),
                            );
                        }
                    }
                    if *retire_old {
                        refresh_verdict(
                            settings,
                            verdict_sandbox,
                            &signer,
                            &record,
                            &store,
                            out,
                            err,
                        );
                    }
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

/// The audit detail's note on the late seals of a retirement (Y1): nothing when there are none.
fn late_note(late_seals: &[String]) -> String {
    if late_seals.is_empty() {
        String::new()
    } else {
        format!(
            "; {} late seal(s) of the key were NOT recorded and nothing excuses them",
            late_seals.len()
        )
    }
}

/// Say what a retirement left out on purpose (Y1): each block the key sealed after the rotation that replaced it, by
/// its id, and why. Nothing is said when there are none. The retirement itself is done and recorded; the error makes
/// the command exit 1, as `status` does for the same blocks, so that no script passes over a block this service did
/// not make.
fn say_late_seals(out: &mut dyn Write, key: &str, late_seals: &[String]) -> Result<(), String> {
    if late_seals.is_empty() {
        return Ok(());
    }
    for block in late_seals {
        let _ = writeln!(
            out,
            "WARNING: block {block} was sealed with maintainer key {key}"
        );
    }
    let _ = writeln!(out, "{LATE_SEALS_SENTENCE}");
    Err(format!(
        "the key {key} is retired and its record is made, but {} block(s) were sealed with it after it was replaced and are not recorded; see above",
        late_seals.len()
    ))
}

/// Say what became of the old maintainer key `previous` on `out`, and give back the error that makes the command
/// exit 1 when its retirement did not end with the record made. `rotated` is the generation when a rotation made the
/// key old; `None` when `retire` asked for it.
fn report_old_key(
    out: &mut dyn Write,
    outcome: &OldKeyOutcome,
    previous: &str,
    rotated: Option<u32>,
    record: &WikiRecord,
) -> Result<(), String> {
    let finish_command = inklkunn_core::finish_retirement_command(&record.id);
    let abandon_command = inklkunn_core::abandon_retirement_command(&record.id);
    let retire_later = retire_command(&record.id, previous);
    match outcome {
        OldKeyOutcome::Kept => {
            let _ = writeln!(
                out,
                "previous maintainer key {previous} is still trusted: history it sealed keeps verifying, \
                 the service no longer holds its seed, and a copy of that seed somewhere else could still \
                 seal changes that verify; to retire it later run `{retire_later}`"
            );
            Ok(())
        }
        OldKeyOutcome::Retired { late_seals } => {
            let _ = writeln!(
                out,
                "{} {previous} was removed from the repository's trust; what it sealed no longer verifies",
                if rotated.is_some() {
                    "previous maintainer key"
                } else {
                    "maintainer key"
                }
            );
            say_late_seals(out, previous, late_seals)
        }
        OldKeyOutcome::RetiredUnrecorded(why) => {
            let _ = writeln!(
                out,
                "{} {previous} was removed from the repository's trust; what it sealed no longer verifies.",
                if rotated.is_some() {
                    "previous maintainer key"
                } else {
                    "maintainer key"
                }
            );
            // Not a success: the wiki shows `Verification failed` for what the key sealed and nothing new
            // is sealed until the retirement is finished or given up (W1).
            Err(format!(
                "the retirement was NOT recorded completely ({why}); nothing is trusted that should not be, but nothing is excused either, \
                 and nothing new is sealed in this wiki until it is finished or given up"
            ))
        }
        OldKeyOutcome::RetireFailed(why) => Err(format!(
            "{} `{previous}` is STILL TRUSTED: \
             removing it failed ({why}); the retirement stays begun and nothing new is sealed in this wiki until it is ended: \
             `{finish_command}` removes the key itself and records what the removal made fail, `{abandon_command}` leaves the key trusted \
             and retires nothing (`{retire_later}` retires it later)",
            match rotated {
                Some(generation) => format!(
                    "the keys were rotated (generation {generation}) but the old maintainer key"
                ),
                None => "the maintainer key".to_owned(),
            }
        )),
        OldKeyOutcome::RetireNotBegun(why) => Err(format!(
            "{} `{previous}` is STILL TRUSTED: {why}; nothing is paused",
            match rotated {
                Some(generation) => format!(
                    "the keys were rotated (generation {generation}) but the old maintainer key"
                ),
                None => "the maintainer key".to_owned(),
            }
        )),
    }
}

/// The check the server uses (`Verifier::check`, over the same store and the same prikk), run once after a key
/// retirement ended or was given up, so that the verdict the pages show was taken after the keys changed and never
/// keeps an explanation that was true of an earlier one (handoff 004, W6). The check is not throttled here: the
/// operator at the host is not a visitor. When it cannot run, or its verdict cannot be recorded, **the stored
/// explanation is cleared** instead (the verdict itself stays what it was), and that is said.
fn refresh_verdict(
    settings: &Settings,
    sandbox: inklkunn_prikk::sandbox::Sandbox,
    signer: &Arc<Signer>,
    record: &WikiRecord,
    store: &SqliteStore,
    out: &mut dyn Write,
    err: &mut dyn Write,
) {
    let fresh = (|| -> Result<inklkunn_core::Checked, String> {
        let storage = PrikkCli::open(
            &settings.prikk,
            sandbox,
            Admission::new(1024 << 20, 64 << 20, Duration::from_secs(5)),
        )
        .map_err(|e| e.to_string())?;
        let wikis = Arc::new(SqliteStore::open(&settings.store_path()).map_err(|e| e.to_string())?);
        let verifier = Verifier::new(
            Arc::new(storage),
            wikis,
            RootLayout::new(settings.wikis_root()),
        );
        verifier.explain_retirements_with(signer.clone());
        Ok(verifier.check_unthrottled(record))
    })();
    match fresh {
        Ok(checked)
            if checked.ran()
                && checked.recorded
                && matches!(
                    checked.outcome,
                    inklkunn_core::CheckOutcome::Verified
                        | inklkunn_core::CheckOutcome::Failed { .. }
                ) =>
        {
            let _ = writeln!(
                out,
                "a fresh verification was recorded: {}",
                match checked.outcome {
                    inklkunn_core::CheckOutcome::Verified => "the repository verifies",
                    _ => "the repository does not verify (the badge says Verification failed)",
                }
            );
        }
        other => {
            let why = match other {
                Ok(checked) => format!("{:?}", checked.outcome),
                Err(why) => why,
            };
            let _ = writeln!(
                err,
                "inklkunn: a fresh verification could not be recorded now ({why}); the stored explanation of the last verdict is cleared, and the next check on the wiki's page records one"
            );
            // The verdict stays what it was; what is said about it does not (the store's own method for this).
            if let Ok(current) = store.get(&record.id)
                && !current.verification_explained_by.is_empty()
                && let Err(e) = store.set_verification(&record.id, current.verification)
            {
                let _ = writeln!(
                    err,
                    "inklkunn: the stored explanation could not be cleared ({e}); run a check on the wiki's page"
                );
            }
        }
    }
}

/// Print the superseded keys and the seals any of them made after the rotation that replaced it (corrections
/// handoff C11). The check itself changes nothing. A check that cannot run is said on `err` and is not a
/// finding; `Ok` carries the findings.
fn report_superseded(
    signer: &Signer,
    record: &WikiRecord,
    repo: &std::path::Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Vec<inklkunn_core::LateSeal>, String> {
    let keys = signer
        .superseded_keys(&record.id)
        .map_err(|e| e.to_string())?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    for k in &keys {
        let _ = writeln!(
            out,
            "superseded maintainer key {} (generation {}, replaced {}{}); the newest block of {} view(s) was recorded then",
            k.maintainer_key_id,
            k.generation,
            rfc3339(k.superseded_at),
            if k.retired {
                format!(
                    ", retired, {} finding(s) of publication trust recorded",
                    k.findings_recorded
                )
            } else {
                ", still trusted".to_owned()
            },
            k.views_recorded
        );
    }
    match signer.seals_by_superseded_keys(repo) {
        Ok(late) if late.is_empty() => {
            let _ = writeln!(
                out,
                "no block was sealed by a superseded key after its rotation (this covers a leaked seed used against \
                 the repository while these records are intact; it does not help if the host itself was taken over)"
            );
            Ok(late)
        }
        Ok(late) => {
            for seal in &late {
                let _ = writeln!(out, "WARNING: {seal}");
            }
            Ok(late)
        }
        Err(e) => {
            let _ = writeln!(
                err,
                "inklkunn: the check for seals by superseded keys could not run: {e}"
            );
            Ok(Vec::new())
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
