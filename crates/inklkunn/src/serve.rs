//! `inklkunn [--dev-unconfined] serve [--config FILE]`: check everything, then serve. A server that cannot
//! do its job says so at start, naming the setting or the tool, before it opens a socket.

use std::io::Write;
use std::time::Duration;

use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::{Admission, PrikkCli};
use inklkunn_store::SqliteStore;
use inklkunn_web::serve::{Shutdown, run_blocking};
use inklkunn_web::{Site, WebConfig, routes};

use crate::config::{ConfigError, Settings, WebSettings};

/// Everything `serve` checked and built, ready to bind.
pub(crate) struct Prepared {
    web: WebConfig,
    site: Site,
    /// What the start-up printed first: what is configured and found.
    pub banner: Vec<String>,
    // Held so the checks above stay true while serving; the read service takes them over (RFC 003 T3).
    _storage: PrikkCli,
    _store: SqliteStore,
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
    let storage = PrikkCli::open(&settings.prikk, sandbox, admission).map_err(|e| {
        format!(
            "prikk is not usable ({e}); install prikk {} or later",
            inklkunn_prikk::MIN_PRIKK_VERSION
        )
    })?;
    std::fs::create_dir_all(&settings.data_dir)
        .map_err(|e| format!("INKLKUNN_DATA_DIR could not be made ({})", e.kind()))?;
    let store = SqliteStore::open(&settings.store_path()).map_err(|e| {
        format!(
            "the store at {} could not be opened: {e}",
            settings.store_path().display()
        )
    })?;
    let site = routes::site(&config).map_err(|e| format!("the routes are inconsistent: {e}"))?;
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
            storage.version(),
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
        _storage: storage,
        _store: store,
    })
}

/// Trusted proxies the operator declared. **This build has no setting for them** (the sign-in routes do not
/// exist yet, and `inklkunn_auth::ProxyPolicy` is not read from the environment), so none is ever declared
/// and the warning below is shown for any address that is reachable from elsewhere.
const TRUSTED_PROXIES_DECLARED: bool = false;

/// The warning for a bind address that anybody can reach while no trusted proxy is declared (handoff 002
/// C10), or `None`. Behind a reverse proxy that is not declared trusted, every client is the proxy's address,
/// so the sign-in's per-address throttle (30 failures, `inklkunn_auth::ThrottlePolicy::PER_ADDRESS`) counts
/// every failed attempt by anyone against one key and thirty of them lock everyone out. A loopback bind is
/// not warned about: only a process on the same host reaches it.
pub(crate) fn proxy_warning(bind: std::net::SocketAddr, declared: bool) -> Option<String> {
    (!bind.ip().is_loopback() && !declared).then(|| {
        format!(
            "warning: INKLKUNN_BIND {bind} is not a loopback address and no trusted proxy is declared \
             (INKLKUNN_TRUSTED_PROXIES, planned and not read by this build), so every client counts as \
             the same address and thirty failed sign-ins by anyone would lock everyone out"
        )
    })
}

/// Print the banner and the warnings to `err`: the first thing `run` does.
fn write_start_notes(err: &mut dyn Write, banner: &[String], web: &WebConfig) {
    for line in banner {
        let _ = writeln!(err, "{line}");
    }
    if let Some(w) = proxy_warning(web.bind, TRUSTED_PROXIES_DECLARED) {
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
    write_start_notes(err, &prepared.banner, &prepared.web);
    run_blocking(&prepared.web, prepared.site, shutdown, on_listening).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
