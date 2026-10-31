//! `inklkunn account invite | link | disable | enable | grant | revoke-grant`: how accounts come into being and what
//! they may do, from the command line of the host that runs the service (the sign-in handoff's U2; the owner's
//! ruling DR-11).
//!
//! **Who may do this.** Whoever can run the command on the host and reach the data directory, as for
//! `audit export`. It needs no operator secret (no key is touched), no prikk and no network.
//!
//! **Every act is audited, principal `system:operator-cli`, the trail written *before* the act** (an act that
//! cannot be recorded is not begun) and again when it ends. Events carry no secret and no content: an action
//! name, an id and a few fixed words. The link of an invitation is never in the trail.
//!
//! - `account invite [--role <role> --wiki <wiki>] [--days <n>]`: a one-time link, seven days by default, shown
//!   **once**, here. The person who opens it chooses a password (or, when OpenID Connect is on, a provider) and
//!   the account is made in that step with the role, if one was named; the first administrator of a wiki is made
//!   this way. No password is ever sent or printed.
//! - `account link <login | id> <issuer> <subject>`: bind an OpenID Connect identity (the provider's `iss` and
//!   `sub`, nothing else: never an e-mail address) to an account, so that person can sign in with the provider.
//!   This is the **administrative** way to link; the person's own is an invitation redeemed with the provider
//!   (or, where `INKLKUNN_JOIN` lets a first provider sign-in make an account, that first sign-in). An identity
//!   that another account holds is refused. Audited before it is done.
//! - `account disable <login | id>` / `account enable <login | id>`: a disabled account cannot sign in, its
//!   sessions end and its API tokens are revoked; its history stays attributed to it.
//! - `account grant <login | id> <role> <wiki>` / `account revoke-grant <login | id> <wiki>`: the role the account
//!   holds in a wiki.

use std::ffi::OsString;
use std::io::Write;

use inklkunn_auth::invitation::{DEFAULT_LIFETIME_SECS, MAX_LIFETIME_SECS};
use inklkunn_auth::{AccountError, AuthConfig, Authenticator, InviteError};
use inklkunn_core::IdentityAuthority;
use inklkunn_store::{
    Account, AccountId, AccountStore, AuditOutcome, AuditSink, ExternalIdentity, LoginName, Role,
    RoleGrant, RoleStore, SqliteStore, WikiRecord,
};

use crate::admin::Trail;
use crate::config::{ConfigError, absolute_setting};
use crate::keys::find_wiki;

/// The usage text.
pub(crate) const USAGE: &str = "usage: inklkunn account invite [--role <role> --wiki <wiki>] [--days <n>]\n       \
inklkunn account link <login | account id> <issuer> <subject>\n       \
inklkunn account disable <login | account id>\n       \
inklkunn account enable <login | account id>\n       \
inklkunn account grant <login | account id> <role> <wiki>\n       \
inklkunn account revoke-grant <login | account id> <wiki>\n\
<role> is one of observer, questioner, contributor, reviewer, maintainer, admin. <wiki> is the wiki's slug or id. \
An invitation lasts 7 days unless --days says otherwise (1 to 30); its link is shown once. <issuer> and <subject> \
are the provider's own (its `iss` and the person's `sub`), exactly as it reports them; an e-mail address is neither.";

/// What was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Invite {
        grant: Option<(Role, String)>,
        days: i64,
    },
    Link {
        who: String,
        issuer: String,
        subject: String,
    },
    Disable {
        who: String,
    },
    Enable {
        who: String,
    },
    Grant {
        who: String,
        role: Role,
        wiki: String,
    },
    RevokeGrant {
        who: String,
        wiki: String,
    },
}

/// The command line was not understood (exit status 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usage(pub String);

fn parse_role(text: &str) -> Result<Role, Usage> {
    Role::parse(text).ok_or_else(|| {
        Usage(format!(
            "`{text}` is not a role; there are observer, questioner, contributor, reviewer, maintainer and admin\n{USAGE}"
        ))
    })
}

/// Parse what follows `account`.
pub(crate) fn parse(args: &[String]) -> Result<Command, Usage> {
    let usage = |why: &str| Usage(format!("{why}\n{USAGE}"));
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["invite", flags @ ..] => {
            let (mut role, mut wiki) = (None, None);
            let mut days = DEFAULT_LIFETIME_SECS / 86_400;
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                let mut value = |name: &str| {
                    it.next()
                        .map(|v| (*v).to_owned())
                        .ok_or_else(|| usage(&format!("{name} needs a value")))
                };
                match *flag {
                    "--role" => role = Some(parse_role(&value("--role")?)?),
                    "--wiki" => wiki = Some(value("--wiki")?),
                    "--days" => {
                        let text = value("--days")?;
                        days = text
                            .parse::<i64>()
                            .ok()
                            .filter(|d| (1..=MAX_LIFETIME_SECS / 86_400).contains(d))
                            .ok_or_else(|| {
                                usage(&format!(
                                    "--days is a whole number from 1 to {} (it is `{text}`)",
                                    MAX_LIFETIME_SECS / 86_400
                                ))
                            })?;
                    }
                    other => {
                        return Err(usage(&format!("`{other}` is not an option of `invite`")));
                    }
                }
            }
            let grant = match (role, wiki) {
                (Some(r), Some(w)) => Some((r, w)),
                (None, None) => None,
                _ => {
                    return Err(usage(
                        "--role and --wiki go together: a role is a role in a wiki",
                    ));
                }
            };
            Ok(Command::Invite { grant, days })
        }
        ["link", who, issuer, subject] => Ok(Command::Link {
            who: (*who).to_owned(),
            issuer: (*issuer).to_owned(),
            subject: (*subject).to_owned(),
        }),
        ["disable", who] => Ok(Command::Disable {
            who: (*who).to_owned(),
        }),
        ["enable", who] => Ok(Command::Enable {
            who: (*who).to_owned(),
        }),
        ["grant", who, role, wiki] => Ok(Command::Grant {
            who: (*who).to_owned(),
            role: parse_role(role)?,
            wiki: (*wiki).to_owned(),
        }),
        ["revoke-grant", who, wiki] => Ok(Command::RevokeGrant {
            who: (*who).to_owned(),
            wiki: (*wiki).to_owned(),
        }),
        [] => Err(usage("which command?")),
        [other, ..] => Err(usage(&format!(
            "`{other}` is not a command of `account`, or it has the wrong arguments"
        ))),
    }
}

/// `inklkunn account …`: parse, open the store, act. The exit status: 0, 1 for a refusal or a failure, 2 for a
/// command line that was not understood.
pub(crate) fn run(
    args: &[&str],
    get_env: &dyn Fn(&str) -> Option<OsString>,
    now: i64,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    // The process's one right to say who a request is, claimed here and nowhere else (corrections handoff C7).
    run_with(args, get_env, now, out, err, IdentityAuthority::claim)
}

/// As [`run`], with the way the authority is claimed given (a test of the commands runs many in one process,
/// and the real claim answers once).
pub(crate) fn run_with(
    args: &[&str],
    get_env: &dyn Fn(&str) -> Option<OsString>,
    now: i64,
    out: &mut dyn Write,
    err: &mut dyn Write,
    claim: fn() -> Option<IdentityAuthority>,
) -> i32 {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let command = match parse(&args) {
        Ok(c) => c,
        Err(Usage(why)) => {
            let _ = writeln!(err, "inklkunn: {why}");
            return 2;
        }
    };
    match execute(&command, get_env, now, out, claim) {
        Ok(()) => 0,
        Err(why) => {
            let _ = writeln!(err, "inklkunn: {why}");
            1
        }
    }
}

/// The store, which must exist (a typo in the data directory must not make a new empty one).
fn open_store(get_env: &dyn Fn(&str) -> Option<OsString>) -> Result<SqliteStore, String> {
    let name = "INKLKUNN_DATA_DIR";
    let data_dir = get_env(name)
        .ok_or_else(|| ConfigError(format!("{name} is not set; it is required")).to_string())
        .and_then(|v| absolute_setting(name, &v).map_err(|e| e.to_string()))?;
    let path = data_dir.join("store.sqlite");
    if !path.is_file() {
        return Err(format!(
            "there is no store at {} ({name}); make one first with `wiki create`, or check the setting",
            path.display()
        ));
    }
    SqliteStore::open(&path).map_err(|e| format!("the store could not be opened: {e}"))
}

/// The account a login or an id names.
fn find_account(store: &SqliteStore, who: &str) -> Result<Account, String> {
    let found = match AccountId::parse(who) {
        Some(id) => store.account(&id),
        None => match LoginName::parse(who) {
            Ok(login) => store.account_by_login(&login),
            Err(_) => {
                return Err(format!(
                    "`{who}` is neither an account id nor a sign-in name"
                ));
            }
        },
    };
    found
        .map_err(|e| format!("the account could not be read: {e}"))?
        .ok_or_else(|| format!("no account `{who}`"))
}

/// The address of an invitation: under the application origin when the operator's environment names it,
/// otherwise the path alone.
fn link(get_env: &dyn Fn(&str) -> Option<OsString>, secret: &str) -> String {
    let path = format!("/-/invite/{secret}");
    match get_env("INKLKUNN_APP_ORIGIN").and_then(|v| v.into_string().ok()) {
        Some(origin) if !origin.trim().is_empty() => {
            format!("{}{path}", origin.trim().trim_end_matches('/'))
        }
        _ => path,
    }
}

fn authenticator(
    store: &std::sync::Arc<SqliteStore>,
    claim: fn() -> Option<IdentityAuthority>,
) -> Result<Authenticator, String> {
    // The command line serves no request, so the authenticator here only changes accounts (a disabled account's
    // sessions end and its tokens are revoked), but it is the one place those rules live.
    let authority = claim().ok_or("the identity authority was claimed already")?;
    Authenticator::new(store.clone(), AuthConfig::default(), authority)
        .map_err(|e| format!("accounts could not be set up ({e})"))
}

fn wiki_resource(record: &WikiRecord) -> String {
    format!("wiki:{}", record.id)
}

fn account_resource(account: &Account) -> String {
    format!("account:{}", account.id)
}

/// Run a parsed command.
pub(crate) fn execute(
    command: &Command,
    get_env: &dyn Fn(&str) -> Option<OsString>,
    now: i64,
    out: &mut dyn Write,
    claim: fn() -> Option<IdentityAuthority>,
) -> Result<(), String> {
    let store = std::sync::Arc::new(open_store(get_env)?);
    let trail = Trail::new(store.as_ref() as &dyn AuditSink, now);
    match command {
        Command::Invite { grant, days } => {
            let named = grant
                .as_ref()
                .map(|(role, wiki)| find_wiki(&store, wiki).map(|record| (record, *role)))
                .transpose()?;
            let detail = match &named {
                Some((record, role)) => {
                    format!("role {} in {} for {days} days", role.as_str(), record.id)
                }
                None => format!("no role for {days} days"),
            };
            trail.begin("account-invite", "invitation", &detail)?;
            let auth = authenticator(&store, claim)?;
            let issued = auth.invite(
                named.map(|(record, role)| (record.id, role)),
                days * 86_400,
                now,
            );
            let issued = match issued {
                Ok(i) => i,
                Err(e) => {
                    let _ = trail.finish(
                        "account-invite",
                        "invitation",
                        AuditOutcome::Failed,
                        "no invitation was made",
                    );
                    return Err(match e {
                        InviteError::NoSuchWiki => "that wiki is not there".to_owned(),
                        other => format!("the invitation could not be made ({other})"),
                    });
                }
            };
            trail.finish(
                "account-invite",
                &format!("invitation:{}", issued.id),
                AuditOutcome::Allowed,
                "made; its link was shown once",
            )?;
            let _ = writeln!(
                out,
                "An invitation was made. It works once and ends at {} (UTC).",
                inklkunn_store::record::rfc3339(issued.expires_at)
            );
            let _ = writeln!(out, "{}", link(get_env, issued.secret.as_str()));
            let _ = writeln!(
                out,
                "This link is shown only now and cannot be shown again. Pass it on to the person it is for."
            );
            Ok(())
        }
        Command::Link {
            who,
            issuer,
            subject,
        } => {
            let identity = ExternalIdentity::new(issuer, subject).map_err(|e| {
                format!("that is not an identity a provider could report ({e}); give the issuer and the subject exactly as the provider does")
            })?;
            let account = find_account(&store, who)?;
            let resource = account_resource(&account);
            let detail = format!("issuer {issuer} subject {subject}");
            trail.begin("account-link", &resource, &detail)?;
            let auth = authenticator(&store, claim)?;
            if let Err(e) = auth.link_external(&account.id, &identity, now) {
                let _ = trail.finish("account-link", &resource, AuditOutcome::Failed, "not done");
                return Err(match e {
                    AccountError::IdentityTaken => {
                        "another account is linked to that identity already; one person has one account"
                            .to_owned()
                    }
                    other => format!("the identity could not be linked ({other})"),
                });
            }
            trail.finish("account-link", &resource, AuditOutcome::Allowed, &detail)?;
            let _ = writeln!(
                out,
                "The account {} can now sign in with {issuer} as {subject}.",
                account.id
            );
            Ok(())
        }
        Command::Disable { who } | Command::Enable { who } => {
            let disabling = matches!(command, Command::Disable { .. });
            let account = find_account(&store, who)?;
            let (action, word) = if disabling {
                ("account-disable", "disabled")
            } else {
                ("account-enable", "enabled")
            };
            let resource = account_resource(&account);
            trail.begin(action, &resource, "")?;
            let auth = authenticator(&store, claim)?;
            let done = if disabling {
                auth.disable(&account.id, now)
            } else {
                auth.enable(&account.id)
            };
            if let Err(e) = done {
                let _ = trail.finish(action, &resource, AuditOutcome::Failed, "not done");
                return Err(format!("the account could not be {word} ({e})"));
            }
            trail.finish(action, &resource, AuditOutcome::Allowed, word)?;
            let _ = writeln!(out, "The account {} is {word}.", account.id);
            Ok(())
        }
        Command::Grant { who, role, wiki } => {
            let account = find_account(&store, who)?;
            let record = find_wiki(&store, wiki)?;
            let resource = format!("{} {}", account_resource(&account), wiki_resource(&record));
            let detail = format!("role {}", role.as_str());
            trail.begin("account-grant", &resource, &detail)?;
            if let Err(e) = store.grant(RoleGrant {
                account: account.id.clone(),
                wiki: record.id.clone(),
                role: *role,
                granted_by: None,
                granted_at: now,
            }) {
                let _ = trail.finish("account-grant", &resource, AuditOutcome::Failed, "not done");
                return Err(format!("the role could not be granted ({e})"));
            }
            trail.finish("account-grant", &resource, AuditOutcome::Allowed, &detail)?;
            let _ = writeln!(
                out,
                "The account {} is now {} in {}.",
                account.id,
                role.as_str(),
                record.slug
            );
            Ok(())
        }
        Command::RevokeGrant { who, wiki } => {
            let account = find_account(&store, who)?;
            let record = find_wiki(&store, wiki)?;
            let resource = format!("{} {}", account_resource(&account), wiki_resource(&record));
            trail.begin("account-revoke-grant", &resource, "")?;
            let had = match store.revoke(&account.id, &record.id) {
                Ok(had) => had,
                Err(e) => {
                    let _ = trail.finish(
                        "account-revoke-grant",
                        &resource,
                        AuditOutcome::Failed,
                        "not done",
                    );
                    return Err(format!("the role could not be taken away ({e})"));
                }
            };
            trail.finish(
                "account-revoke-grant",
                &resource,
                AuditOutcome::Allowed,
                if had { "revoked" } else { "held none" },
            )?;
            let _ = writeln!(
                out,
                "{}",
                if had {
                    format!(
                        "The account {} no longer has a role in {}.",
                        account.id, record.slug
                    )
                } else {
                    format!(
                        "The account {} held no role in {}; nothing changed.",
                        account.id, record.slug
                    )
                }
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
