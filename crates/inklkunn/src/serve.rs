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

/// Bind and serve until `shutdown`. `on_listening` hears the bound address.
pub(crate) fn run(
    prepared: Prepared,
    shutdown: Shutdown,
    err: &mut dyn Write,
    on_listening: impl FnOnce(std::net::SocketAddr),
) -> Result<(), String> {
    for line in &prepared.banner {
        let _ = writeln!(err, "{line}");
    }
    run_blocking(&prepared.web, prepared.site, shutdown, on_listening).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
