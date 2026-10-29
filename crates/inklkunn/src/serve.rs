//! `inklkunn [--dev-unconfined] serve [--config FILE]`: check everything, then serve. A server that cannot
//! do its job says so at start, naming the setting or the tool, before it opens a socket.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use inklkunn_auth::ProxyPolicy;
use inklkunn_core::read::{ReadConfig, ReadService};
use inklkunn_core::{Authorizer, StoreAuthz, Verifier};
use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::{Admission, PrikkCli, SizeHints, Storage};
use inklkunn_store::{AuditSink, RootLayout, SqliteStore};
use inklkunn_web::serve::{Shutdown, run_blocking};
use inklkunn_web::{AnonymousOnly, Services, Site, WebConfig, routes};

use crate::config::{ConfigError, Settings, WebSettings};

/// Everything `serve` checked and built, ready to bind.
pub(crate) struct Prepared {
    web: WebConfig,
    site: Site,
    /// What the start-up printed first: what is configured and found.
    pub banner: Vec<String>,
    /// Whether the operator declared a trusted proxy (`INKLKUNN_TRUSTED_PROXIES`): the warning below depends
    /// on it, and the sign-in's per-address throttle will read the policy.
    trusted_proxies_declared: bool,
    // Held so what `prepare` checked stays true while serving: the services the pages ask hold these too.
    _services: Arc<Services>,
}

fn refusal(e: ConfigError) -> String {
    e.to_string()
}

/// Validate the settings, find prikk at the floor, open the store, build the site.
pub(crate) fn prepare(settings: &Settings, web: &WebSettings) -> Result<Prepared, String> {
    let config = WebConfig::new(&web.bind, &web.app_origin, &web.content_origin)
        .map_err(|e| e.to_string())?;
    let sandbox = settings.sandbox().map_err(refusal)?;
    let confined = !matches!(sandbox, Sandbox::Unconfined);
    let admission = Admission::new(
        web.admission_budget_mib << 20,
        64 << 20,
        Duration::from_millis(web.admission_wait_ms),
    );
    // The networks of the trusted proxies are checked now, so a typo stops the start (the policy itself is
    // read by the sign-in's throttle when that is served).
    let proxy_networks: Vec<&str> = web.trusted_proxies.iter().map(String::as_str).collect();
    ProxyPolicy::trusting(&proxy_networks).map_err(|e| {
        format!("INKLKUNN_TRUSTED_PROXIES must be addresses or networks like 10.0.0.0/8 ({e})")
    })?;
    let hints = SizeHints::new();
    let storage = PrikkCli::open(&settings.prikk, sandbox, admission)
        .map_err(|e| {
            format!(
                "prikk is not usable ({e}); install prikk {} or later",
                inklkunn_prikk::MIN_PRIKK_VERSION
            )
        })?
        .with_size_hints(hints.clone());
    std::fs::create_dir_all(&settings.data_dir)
        .map_err(|e| format!("INKLKUNN_DATA_DIR could not be made ({})", e.kind()))?;
    let store = Arc::new(SqliteStore::open(&settings.store_path()).map_err(|e| {
        format!(
            "the store at {} could not be opened: {e}",
            settings.store_path().display()
        )
    })?);
    let version = storage.version().to_owned();
    let layout = RootLayout::new(settings.wikis_root());
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let read = Arc::new(ReadService::new(
        storage.clone(),
        store.clone(),
        store.clone(),
        layout.clone(),
        ReadConfig {
            tip_ttl: Duration::from_secs(web.read.tip_ttl_secs),
            page_max_bytes: web.read.page_max_kib << 10,
            index_max_bytes: web.read.index_max_mib << 20,
            index_max_files: web.read.index_max_files,
        },
        Some(hints),
    ));
    let authorizer = Arc::new(Authorizer::new(
        Arc::new(StoreAuthz::new(store.clone(), store.clone())),
        store.clone() as Arc<dyn AuditSink>,
    ));
    let services = Arc::new(Services {
        wikis: store.clone(),
        read,
        verifier: Arc::new(Verifier::new(storage, store, layout)),
        authorizer,
        // Sign-in is not served yet (RFC 002's pages are the next surface), so every request is anonymous:
        // a public wiki is readable, an internal or restricted one is not.
        identity: Arc::new(AnonymousOnly),
        log: Arc::new(|line| {
            let _ = writeln!(std::io::stderr(), "inklkunn: {line}");
        }),
        clock: inklkunn_store::record::now_secs,
    });
    let site = routes::site_with(&config, Some(services.clone()))
        .map_err(|e| format!("the routes are inconsistent: {e}"))?;
    let banner = vec![
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
    Ok(Prepared {
        web: config,
        site,
        banner,
        trusted_proxies_declared: !web.trusted_proxies.is_empty(),
        _services: services,
    })
}

/// The warning for a bind address that anybody can reach while no trusted proxy is declared (handoff 002
/// C10), or `None`. Behind a reverse proxy that is not declared trusted, every client is the proxy's address,
/// so the sign-in's per-address throttle (30 failures, `inklkunn_auth::ThrottlePolicy::PER_ADDRESS`) counts
/// every failed attempt by anyone against one key and thirty of them lock everyone out. A loopback bind is
/// not warned about: only a process on the same host reaches it.
pub(crate) fn proxy_warning(bind: std::net::SocketAddr, declared: bool) -> Option<String> {
    (!bind.ip().is_loopback() && !declared).then(|| {
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
    web: &WebConfig,
    trusted_proxies_declared: bool,
) {
    for line in banner {
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
        &prepared.web,
        prepared.trusted_proxies_declared,
    );
    run_blocking(&prepared.web, prepared.site, shutdown, on_listening).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
