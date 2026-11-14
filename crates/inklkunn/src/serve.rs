//! `inklkunn [--dev-unconfined] serve [--config FILE]`: check everything, then serve. A server that cannot
//! do its job says so at start, naming the setting or the tool, before it opens a socket.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use inklkunn_auth::{AuthConfig, Authenticator, ProxyPolicy};
use inklkunn_core::proposal::{ProposalService, QuotaConfig};
use inklkunn_core::read::ReadService;
use inklkunn_core::{Authorizer, IdentityAuthority, Inbox, Notifier, StoreAuthz, Verifier};
use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::{Admission, PrikkCli, SizeHints, Storage};
use inklkunn_store::{AuditSink, RootLayout, SqliteStore};
use inklkunn_web::serve::{Shutdown, run_blocking};
use inklkunn_web::{AuthIdentity, Services, Site, WebConfig, routes};

use crate::admin;
use crate::config::{ConfigError, Settings, WebSettings};
use crate::outbound;

mod acceptor;
mod follow_up;
#[cfg(test)]
mod writes_on;

/// Everything `serve` checked and built, ready to bind.
pub(crate) struct Prepared {
    web: WebConfig,
    site: Site,
    /// What the start-up printed first: what is configured and found.
    pub banner: Vec<String>,
    /// Whether the operator declared a trusted proxy (`INKLKUNN_TRUSTED_PROXIES`): the warning below depends
    /// on it, and the sign-in's per-address throttle will read the policy.
    trusted_proxies_declared: bool,
    /// What the start-up work did or could not do (unfinished creations removed, checkpoint records
    /// refreshed), one line each. None of it stops the server.
    pub notes: Vec<String>,
    // Held so what `prepare` checked stays true while serving: the services the pages ask hold these too.
    _services: Arc<Services>,
}

fn refusal(e: ConfigError) -> String {
    e.to_string()
}

/// Validate the settings, find prikk at the floor, open the store, build the site. `authority` is the process's
/// one right to say who a request is (`IdentityAuthority::claim`, made once by the caller, corrections handoff
/// C7): the authenticator takes it and keeps it, so no other code can build an account principal.
pub(crate) fn prepare(
    settings: &Settings,
    web: &WebSettings,
    authority: IdentityAuthority,
) -> Result<Prepared, String> {
    prepare_with(
        settings,
        web,
        authority,
        Arc::new(|line| {
            let _ = writeln!(std::io::stderr(), "inklkunn: {line}");
        }),
    )
}

/// [`prepare`], with the operator's log going to `log` (a test hears it).
fn prepare_with(
    settings: &Settings,
    web: &WebSettings,
    authority: IdentityAuthority,
    log: Arc<dyn Fn(&str) + Send + Sync>,
) -> Result<Prepared, String> {
    let config = WebConfig::new(&web.bind, &web.app_origin, &web.content_origin)
        .map_err(|e| e.to_string())?;
    let sandbox = settings.sandbox().map_err(refusal)?;
    let confined = !matches!(sandbox, Sandbox::Unconfined);
    let admission = Admission::new(
        web.admission_budget_mib << 20,
        64 << 20,
        Duration::from_millis(web.admission_wait_ms),
    );
    // The networks of the trusted proxies are checked now, so a typo stops the start; the policy is what the
    // sign-in's per-address throttle counts addresses by.
    let proxy_networks: Vec<&str> = web.trusted_proxies.iter().map(String::as_str).collect();
    let proxies = ProxyPolicy::trusting(&proxy_networks).map_err(|e| {
        format!(
            "INKLKUNN_TRUSTED_PROXIES must be the addresses of your proxies themselves, like 127.0.0.1 ({e})"
        )
    })?;
    // The provider (its secret read, its vector probed) is checked now too, before anything is opened.
    let provider = outbound::provider_if_configured(web)?;
    let hints = SizeHints::new();
    let storage = PrikkCli::open(&settings.prikk, sandbox.clone(), admission)
        .map_err(|e| {
            format!(
                "prikk is not usable ({e}); install prikk {} or later",
                inklkunn_prikk::MIN_PRIKK_VERSION
            )
        })?
        .with_size_hints(hints.clone());
    std::fs::create_dir_all(&settings.data_dir)
        .map_err(|e| format!("INKLKUNN_DATA_DIR could not be made ({})", e.kind()))?;
    let store_is_new = !settings.store_path().is_file();
    let store = Arc::new(SqliteStore::open(&settings.store_path()).map_err(|e| {
        format!(
            "the store at {} could not be opened: {e}",
            settings.store_path().display()
        )
    })?);
    // Before anything is served: remove the creations that were killed, refresh prikk's checkpoint records
    // where the prikk version changed or a refresh was asked for (handoff 003 T7; IS-8, IS-9, IS-13).
    let notes = admin::start_up(
        &store,
        settings,
        &sandbox,
        &storage,
        store_is_new,
        inklkunn_store::record::now_secs(),
    );
    let version = storage.version().to_owned();
    let layout = RootLayout::new(settings.wikis_root());
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let verifier = Arc::new(Verifier::new(
        storage.clone(),
        store.clone(),
        layout.clone(),
    ));
    verifier.flag_format_markers_in(store.clone());
    let read = Arc::new(
        ReadService::new(
            storage.clone(),
            store.clone(),
            store.clone(),
            layout.clone(),
            web.read.read_config(),
            Some(hints),
        )
        // The flags of accepted changes (minor, machine-assisted) are the proposal records' (RFC 004 D-10).
        .with_revision_flags(store.clone())
        // `knowledge.review_after` (RFC 005 part 2 D-10): the operator's setting, in days; the config bounds it.
        .with_review_after_days(
            u32::try_from(web.read.knowledge_review_after_days).unwrap_or(u32::MAX),
        ),
    );
    let authorizer = Arc::new(Authorizer::new(
        Arc::new(StoreAuthz::new(store.clone(), store.clone())),
        store.clone() as Arc<dyn AuditSink>,
    ));
    let inbox = Arc::new(Inbox::new(
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
        (*authorizer).clone(),
    ));
    let notifier = Arc::new(Notifier::new(
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
        (*authorizer).clone(),
    ));
    let proposals = Arc::new(
        ProposalService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            (*authorizer).clone(),
            read.clone(),
            QuotaConfig::default(),
        )
        .with_notifier(notifier)
        // A claim's own discussion (RFC 005 part 2 D-11) is kept in the same store, under the same limits.
        .with_claim_threads(store.clone(), read.clone()),
    );
    let auth = Arc::new(outbound::attach(
        Authenticator::new(
            store.clone(),
            AuthConfig {
                proxies,
                join: web.join,
                ..AuthConfig::default()
            },
            authority,
        )
        .map_err(|e| format!("sign-in could not be set up ({e})"))?
        .with_audit(store.clone() as Arc<dyn AuditSink>),
        provider,
    ));
    // The acceptor comes last among the parts and only after `admin::start_up` ran above: recovery has looked at
    // every wiki that needed it before anything can be accepted (a wiki it could not recover stays flagged and
    // refuses writes). Without the secret file the server still serves reads (RR-1; handoff U5).
    let signing = acceptor::assemble(acceptor::Parts {
        settings: settings.clone(),
        sandbox: sandbox.clone(),
        layout: layout.clone(),
        store: store.clone(),
        storage: storage.clone(),
        read: read.clone(),
        proposals: proposals.clone(),
        verifier: verifier.clone(),
        lease: acceptor::lease_config(),
        log: log.clone(),
    });
    let mut notes = notes;
    notes.extend(signing.notes);
    let services = Arc::new(Services {
        wikis: store.clone(),
        read,
        verifier,
        authorizer,
        inbox,
        proposals,
        accounts: store.clone(),
        acceptances: store.clone(),
        acts: store.clone(),
        // The acceptor of RFC 004 D-4 over the same lease as the command line, or `NoSigner` when this server was
        // started without the means to sign (then every change to the history is refused with one sentence).
        acceptor: Some(signing.acceptor),
        // A request is who its credential says: a session cookie (a write only with the session's CSRF token), or
        // a bearer token judged alone (the sign-in handoff's U4).
        identity: Arc::new(AuthIdentity::new(
            auth.clone(),
            store.clone(),
            inklkunn_store::record::now_secs,
            log.clone(),
        )),
        auth: Some(auth),
        log,
        clock: inklkunn_store::record::now_secs,
    });
    let site = routes::site_with(&config, Some(services.clone()))
        .map_err(|e| format!("the routes are inconsistent: {e}"))?;
    notes.extend(join_note(web.join));
    notes.extend(outbound::start_notes(web));
    let signing_on = signing.signing;
    let mut banner = vec![
        format!(
            "inklkunn {} serving {} (content {}) on {}",
            env!("CARGO_PKG_VERSION"),
            config.app_origin,
            config.content_origin,
            config.bind
        ),
        format!(
            "{} ({}), store {}",
            version,
            if confined {
                "confined by bubblewrap"
            } else {
                "UNCONFINED"
            },
            settings.store_path().display()
        ),
    ];
    if signing_on {
        banner.push(format!(
            "accepting proposals: the signer is open over {}",
            settings.key_dir.display()
        ));
    }
    Ok(Prepared {
        web: config,
        site,
        banner,
        trusted_proxies_declared: !web.trusted_proxies.is_empty(),
        notes,
        _services: services,
    })
}

/// What the start says about who may join, or `None` when joining is off (the default, which needs no word). An
/// open door is the operator's choice and is said at every start, so it is never forgotten.
pub(crate) fn join_note(policy: inklkunn_auth::JoinPolicy) -> Option<String> {
    use inklkunn_auth::JoinPolicy;
    match policy {
        JoinPolicy::Off => None,
        JoinPolicy::Provider => Some(
            "INKLKUNN_JOIN=provider: a first sign-in through the configured provider makes an account; a public \
             provider makes this open registration"
                .to_owned(),
        ),
        JoinPolicy::Anyone => Some(
            "INKLKUNN_JOIN=anyone: anyone who can reach this server can make an account with a password \
             (no role beyond what each wiki gives everyone; the new-account quotas apply)"
                .to_owned(),
        ),
    }
}

/// The warning for a server that has no trusted proxy declared (handoff 002 C10; corrections G1), or `None`
/// once one is. Behind a reverse proxy that is not declared trusted, every client is the proxy's address, so
/// the sign-in's per-address throttle (30 failures, `inklkunn_auth::ThrottlePolicy::PER_ADDRESS`) counts every
/// failed attempt by anyone against one key and thirty of them lock everyone out. **A loopback bind is warned
/// about too**: the default bind is `127.0.0.1:8080` and this server has no TLS of its own, so the likeliest
/// deployment is a TLS proxy on the same host, which reaches it as a loopback peer.
pub(crate) fn proxy_warning(bind: std::net::SocketAddr, declared: bool) -> Option<String> {
    if declared {
        return None;
    }
    Some(if bind.ip().is_loopback() {
        format!(
            "warning: INKLKUNN_BIND {bind} is a loopback address and no trusted proxy is declared \
             (INKLKUNN_TRUSTED_PROXIES): if a reverse proxy on this host passes requests on, every client \
             counts as the same address and thirty failed sign-ins by anyone would lock everyone out"
        )
    } else {
        format!(
            "warning: INKLKUNN_BIND {bind} is not a loopback address and no trusted proxy is declared \
             (INKLKUNN_TRUSTED_PROXIES), so behind a reverse proxy every client counts as the same address \
             and thirty failed sign-ins by anyone would lock everyone out"
        )
    })
}

/// Print the banner and the warnings to `err`: the first thing `run` does.
fn write_start_notes(
    err: &mut dyn Write,
    banner: &[String],
    notes: &[String],
    web: &WebConfig,
    trusted_proxies_declared: bool,
) {
    for line in banner {
        let _ = writeln!(err, "{line}");
    }
    for line in notes {
        let _ = writeln!(err, "{line}");
    }
    if let Some(w) = proxy_warning(web.bind, trusted_proxies_declared) {
        let _ = writeln!(err, "{w}");
    }
}

/// Bind and serve until `shutdown`. `on_listening` hears the bound address.
pub(crate) fn run(
    prepared: Prepared,
    shutdown: Shutdown,
    err: &mut dyn Write,
    on_listening: impl FnOnce(std::net::SocketAddr),
) -> Result<(), String> {
    write_start_notes(
        err,
        &prepared.banner,
        &prepared.notes,
        &prepared.web,
        prepared.trusted_proxies_declared,
    );
    run_blocking(&prepared.web, prepared.site, shutdown, on_listening).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
